use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

/// §202 — Event topics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventTopic {
    App,
    Runtime,
    Download,
    Diagnostics,
    Notification,
    Telemetry,
}

/// §32 — Event QoS policies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventQos {
    Latest,
    Coalesce,
    Batched,
    Lossless,
}

/// §88.3 — Typed envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventEnvelope {
    pub id: String,
    pub schema: u32,
    pub topic: EventTopic,
    pub qos: EventQos,
    pub name: String,
    pub timestamp_ms: u64,
    pub correlation_id: Option<String>,
    pub payload: serde_json::Value,
}

static EVENT_SEQ: AtomicU64 = AtomicU64::new(0);
pub const CURRENT_SCHEMA: u32 = 1;

impl EventEnvelope {
    pub fn new(
        topic: EventTopic,
        qos: EventQos,
        name: impl Into<String>,
        payload: serde_json::Value,
    ) -> Self {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Self {
            id: format!("evt_{}", EVENT_SEQ.fetch_add(1, Ordering::Relaxed)),
            schema: CURRENT_SCHEMA,
            topic,
            qos,
            name: name.into(),
            timestamp_ms: ts,
            correlation_id: None,
            payload,
        }
    }

    pub fn with_correlation(mut self, correlation_id: impl Into<String>) -> Self {
        self.correlation_id = Some(correlation_id.into());
        self
    }
}

/// §200 — IPC Backpressure: bounded queue, producer không block vô hạn.
const LOSSLESS_CAPACITY: usize = 1024;
const DEFAULT_CAPACITY: usize = 256;

// Không derive Default: Ring cần capacity tường minh (DEFAULT_CAPACITY), khởi tạo trong EventHub::new().
struct Buffers {
    latest: RwLock<tokio_ring::Ring>,
    coalesce: RwLock<std::collections::HashMap<String, EventEnvelope>>,
    batched: RwLock<VecDeque<EventEnvelope>>,
    lossless: RwLock<VecDeque<EventEnvelope>>,
}

/// Ring buffer drop-oldest cho Latest.
mod tokio_ring {
    use super::EventEnvelope;
    use std::collections::VecDeque;

    pub struct Ring {
        pub slots: VecDeque<EventEnvelope>,
        pub capacity: usize,
    }

    impl Ring {
        pub fn push(&mut self, envelope: EventEnvelope) {
            // Latest per-name: giữ envelope mới nhất cho cùng name
            if let Some(existing) = self.slots.iter_mut().find(|e| e.name == envelope.name) {
                *existing = envelope;
                return;
            }
            if self.slots.len() >= self.capacity {
                self.slots.pop_front();
            }
            self.slots.push_back(envelope);
        }
    }
}

/// §32 — Core EventBus: phân stream theo QoS, bounded, không unbounded queue (§61).
pub struct EventHub {
    buffers: Buffers,
    published: AtomicU64,
    dropped: AtomicU64,
}

impl Default for EventHub {
    fn default() -> Self {
        Self::new()
    }
}

impl EventHub {
    pub fn new() -> Self {
        Self {
            buffers: Buffers {
                latest: RwLock::new(tokio_ring::Ring {
                    slots: VecDeque::new(),
                    capacity: DEFAULT_CAPACITY,
                }),
                coalesce: RwLock::new(std::collections::HashMap::new()),
                batched: RwLock::new(VecDeque::new()),
                lossless: RwLock::new(VecDeque::new()),
            },
            published: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
        }
    }

    /// Publish một envelope. Không bao giờ block; QoS quyết định drop/gộp.
    pub fn publish(&self, envelope: EventEnvelope) {
        self.published.fetch_add(1, Ordering::Relaxed);
        match envelope.qos {
            EventQos::Latest => {
                let mut ring = self.buffers.latest.write();
                ring.push(envelope);
            }
            EventQos::Coalesce => {
                let key = coalesce_key(&envelope);
                let mut map = self.buffers.coalesce.write();
                // bounded: cap 4096 keys — quá nhiều key thì drop (diagnostic event)
                if map.len() >= 4096 && !map.contains_key(&key) {
                    self.dropped.fetch_add(1, Ordering::Relaxed);
                    return;
                }
                map.insert(key, envelope);
            }
            EventQos::Batched => {
                let mut queue = self.buffers.batched.write();
                if queue.len() >= DEFAULT_CAPACITY {
                    queue.pop_front();
                    self.dropped.fetch_add(1, Ordering::Relaxed);
                }
                queue.push_back(envelope);
            }
            EventQos::Lossless => {
                let mut queue = self.buffers.lossless.write();
                if queue.len() >= LOSSLESS_CAPACITY {
                    // ngay cả lossless cũng phải bounded (§61) — drop oldest, log warn
                    queue.pop_front();
                    self.dropped.fetch_add(1, Ordering::Relaxed);
                    log::warn!("lossless buffer full, dropping oldest envelope");
                }
                queue.push_back(envelope);
            }
        }
    }

    /// Drain toàn bộ buffer hiện có (UI poll theo frame hoặc push qua bridge).
    pub fn drain(&self) -> Vec<EventEnvelope> {
        let mut out = Vec::new();
        out.extend(self.buffers.latest.write().slots.drain(..));
        let mut coalesce = self.buffers.coalesce.write();
        out.extend(coalesce.drain().map(|(_, v)| v));
        let mut batched = self.buffers.batched.write();
        out.extend(batched.drain(..));
        let mut lossless = self.buffers.lossless.write();
        out.extend(lossless.drain(..));
        out
    }

    pub fn stats(&self) -> HubStats {
        HubStats {
            published: self.published.load(Ordering::Relaxed),
            dropped: self.dropped.load(Ordering::Relaxed),
            pending_latest: self.buffers.latest.read().slots.len(),
            pending_coalesce: self.buffers.coalesce.read().len(),
            pending_batched: self.buffers.batched.read().len(),
            pending_lossless: self.buffers.lossless.read().len(),
        }
    }
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubStats {
    pub published: u64,
    pub dropped: u64,
    pub pending_latest: usize,
    pub pending_coalesce: usize,
    pub pending_batched: usize,
    pub pending_lossless: usize,
}

fn coalesce_key(envelope: &EventEnvelope) -> String {
    let payload = envelope.payload.as_object();
    match payload.and_then(|obj| obj.get("key")).and_then(|k| k.as_str()) {
        Some(key) => format!("{}::{key}", envelope.name),
        None => envelope.name.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_keeps_newest_per_name() {
        let hub = EventHub::new();
        for fps in [60, 90, 144] {
            hub.publish(EventEnvelope::new(
                EventTopic::Telemetry,
                EventQos::Latest,
                "runtime.fps",
                serde_json::json!({ "fps": fps }),
            ));
        }
        let drained = hub.drain();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].payload["fps"], 144);
    }

    #[test]
    fn coalesce_groups_by_key() {
        let hub = EventHub::new();
        for key in ["a", "a", "b"] {
            hub.publish(EventEnvelope::new(
                EventTopic::Download,
                EventQos::Coalesce,
                "download.progress",
                serde_json::json!({ "key": key }),
            ));
        }
        let drained = hub.drain();
        assert_eq!(drained.len(), 2);
    }

    #[test]
    fn batched_drops_oldest_when_full() {
        let hub = EventHub::new();
        for i in 0..(DEFAULT_CAPACITY + 10) {
            hub.publish(EventEnvelope::new(
                EventTopic::App,
                EventQos::Batched,
                format!("console.line.{i}"),
                serde_json::json!({}),
            ));
        }
        let drained = hub.drain();
        assert_eq!(drained.len(), DEFAULT_CAPACITY);
        assert_eq!(drained[0].name, "console.line.10");
        assert!(hub.stats().dropped > 0);
    }

    #[test]
    fn lossless_preserves_all_until_capacity() {
        let hub = EventHub::new();
        for i in 0..100 {
            hub.publish(EventEnvelope::new(
                EventTopic::Diagnostics,
                EventQos::Lossless,
                format!("critical.{i}"),
                serde_json::json!({}),
            ));
        }
        assert_eq!(hub.drain().len(), 100);
    }

    #[test]
    fn stats_track_publish_and_drop() {
        let hub = EventHub::new();
        hub.publish(EventEnvelope::new(EventTopic::App, EventQos::Batched, "a", serde_json::json!({})));
        let stats = hub.stats();
        assert_eq!(stats.published, 1);
        assert_eq!(stats.pending_batched, 1);
    }
}
