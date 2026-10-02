//! antares-net — Batch 14, mục **network** (§118 Network Stack 2.0, §119 Status Probe,
//! §121 Packet Inspector, §122 Packet Ring Buffer).
//!
//! Skeleton phase 1 — thuần logic, test được không I/O (TCP/ping thật nối ở phase sau):
//! - `rtt_stats` — parity 1:1 với `_probe_stats()` trong `legacy/python/sidecar.py`
//!   (min/avg/max/jitter/loss; mẫu `None` = fail, không bị drop trước khi tính loss)
//! - `PacketMode` — 3 mode §121: OFF / METADATA / DEBUG PAYLOAD
//! - `PacketRing` — ring buffer §122: cap 10k mặc định / 100k developer, oldest-first,
//!   memory-only, export chỉ khi user yêu cầu
//!
//! Phase 2 — ping.rs: MC Server List Ping §119. Phase 3 — probe.rs: tcp/probe/dns.
//! Phase 4 — manifest.rs: Mojang manifest + DiskCache TTL parity (mục 10.2/60).

use std::collections::VecDeque;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

pub mod manifest;
pub mod ping;
pub mod probe;

pub use ping::{
    frame, handshake_body, pack_str, pack_varint, parse_status, ping, read_varint, PingError,
    PingPlayers, PingResult, PingVersion,
};
pub use manifest::{
    download_size, find, get_manifest, ManifestCache, ManifestError, VersionEntry,
    MOJANG_MANIFEST_URL, TTL_MANIFEST_SECS,
};
pub use probe::{
    check_endpoints, dns_lookup, probe, tcp_check, DnsResult, ProbeResult, ProbeSample,
    TcpCheck, ENDPOINTS,
};

// ---------------------------------------------------------------------------
// RTT stats — parity legacy sidecar `_probe_stats`
// ---------------------------------------------------------------------------

/// RTT stats từ samples (ms) — min/avg/max/jitter/loss.
///
/// Parity `sidecar.py::_probe_stats`:
/// - samples giữ `None` cho mẫu fail → tính vào loss%
/// - jitter = trung bình delta tuyệt đối giữa các mẫu liên tiếp (chỉ tính mẫu ok)
/// - round 1 chữ số thập phân
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RttStats {
    pub min: Option<f64>,
    pub avg: Option<f64>,
    pub max: Option<f64>,
    pub jitter: Option<f64>,
    pub loss: f64,
}

pub fn rtt_stats(samples: &[Option<f64>]) -> RttStats {
    let ok: Vec<f64> = samples.iter().filter_map(|s| *s).collect();
    if ok.is_empty() {
        return RttStats {
            min: None,
            avg: None,
            max: None,
            jitter: None,
            loss: 100.0,
        };
    }
    let diffs: Vec<f64> = ok
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .collect();
    let round1 = |v: f64| (v * 10.0).round() / 10.0;
    RttStats {
        min: Some(round1(ok.iter().cloned().fold(f64::INFINITY, f64::min))),
        avg: Some(round1(ok.iter().sum::<f64>() / ok.len() as f64)),
        max: Some(round1(ok.iter().cloned().fold(f64::NEG_INFINITY, f64::max))),
        jitter: Some(round1(if diffs.is_empty() {
            0.0
        } else {
            diffs.iter().sum::<f64>() / diffs.len() as f64
        })),
        loss: round1(100.0 * (samples.len() - ok.len()) as f64 / samples.len() as f64),
    }
}

// ---------------------------------------------------------------------------
// §121 — Packet Inspector modes
// ---------------------------------------------------------------------------

/// §121 — Ba mode của packet inspector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PacketMode {
    Off,
    /// timestamp/direction/packetId/name/size — không payload.
    Metadata,
    /// Manual start/stop, memory bounded, redaction (§121).
    DebugPayload,
}

impl Default for PacketMode {
    fn default() -> Self {
        PacketMode::Off
    }
}

/// §121 — Metadata một packet (không payload).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PacketMetadata {
    pub timestamp_ms: u64,
    pub direction: PacketDirection,
    pub packet_id: u32,
    pub name: String,
    pub size: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PacketDirection {
    Clientbound,
    Serverbound,
}

// ---------------------------------------------------------------------------
// §122 — Packet Ring Buffer
// ---------------------------------------------------------------------------

pub const PACKET_RING_DEFAULT_CAP: usize = 10_000;
pub const PACKET_RING_DEV_CAP: usize = 100_000;

/// §122 — Ring buffer: cap 10k default / 100k dev, eviction oldest-first,
/// không ghi disk liên tục, export chỉ khi user yêu cầu (`drain`).
pub struct PacketRing {
    buffer: Mutex<VecDeque<PacketMetadata>>,
    cap: usize,
    /// Đếm tổng packet đã thấy (kể cả bị evict) — để UI hiển thị "x/y captured".
    total_seen: Mutex<u64>,
    dropped: Mutex<u64>,
}

impl PacketRing {
    pub fn new(cap: usize) -> Self {
        Self {
            buffer: Mutex::new(VecDeque::with_capacity(cap.min(PACKET_RING_DEV_CAP))),
            cap: cap.clamp(1, PACKET_RING_DEV_CAP),
            total_seen: Mutex::new(0),
            dropped: Mutex::new(0),
        }
    }

    pub fn default_ring() -> Self {
        Self::new(PACKET_RING_DEFAULT_CAP)
    }

    pub fn dev_ring() -> Self {
        Self::new(PACKET_RING_DEV_CAP)
    }

    pub fn cap(&self) -> usize {
        self.cap
    }

    /// Push một packet; đầy thì evict oldest-first (§122).
    pub fn push(&self, packet: PacketMetadata) {
        let mut buffer = self.buffer.lock();
        if buffer.len() >= self.cap {
            buffer.pop_front();
            *self.dropped.lock() += 1;
        }
        buffer.push_back(packet);
        *self.total_seen.lock() += 1;
    }

    pub fn len(&self) -> usize {
        self.buffer.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn total_seen(&self) -> u64 {
        *self.total_seen.lock()
    }

    pub fn dropped(&self) -> u64 {
        *self.dropped.lock()
    }

    /// §122 — Export chỉ khi user yêu cầu: trả snapshot và (tuỳ chọn) xoá buffer.
    pub fn export(&self, clear: bool) -> Vec<PacketMetadata> {
        let mut buffer = self.buffer.lock();
        let snapshot: Vec<PacketMetadata> = buffer.iter().cloned().collect();
        if clear {
            buffer.clear();
        }
        snapshot
    }

    pub fn clear(&self) {
        self.buffer.lock().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Parity tests với sidecar.py::_probe_stats — cùng input, cùng output.
    #[test]
    fn rtt_stats_parity_all_ok() {
        let stats = rtt_stats(&[Some(10.0), Some(12.0), Some(14.0)]);
        // diffs = [2, 2] → jitter 2.0; avg 12.0
        assert_eq!(stats.min, Some(10.0));
        assert_eq!(stats.avg, Some(12.0));
        assert_eq!(stats.max, Some(14.0));
        assert_eq!(stats.jitter, Some(2.0));
        assert_eq!(stats.loss, 0.0);
    }

    #[test]
    fn rtt_stats_parity_with_failures() {
        // Mẫu None = fail → tính loss, không tham gia min/avg/jitter.
        let stats = rtt_stats(&[Some(10.0), None, Some(20.0), Some(30.0)]);
        assert_eq!(stats.min, Some(10.0));
        assert_eq!(stats.avg, Some(20.0));
        assert_eq!(stats.max, Some(30.0));
        // ok = [10,20,30] → diffs [10,10] → jitter 10
        assert_eq!(stats.jitter, Some(10.0));
        assert_eq!(stats.loss, 25.0);
    }

    #[test]
    fn rtt_stats_parity_all_failed() {
        let stats = rtt_stats(&[None, None]);
        assert_eq!(stats.min, None);
        assert_eq!(stats.avg, None);
        assert_eq!(stats.jitter, None);
        assert_eq!(stats.loss, 100.0);
    }

    #[test]
    fn rtt_stats_single_sample_zero_jitter() {
        let stats = rtt_stats(&[Some(42.0)]);
        assert_eq!(stats.jitter, Some(0.0)); // không có diff → 0.0 như legacy
        assert_eq!(stats.loss, 0.0);
    }

    #[test]
    fn rtt_stats_rounding_one_decimal() {
        let stats = rtt_stats(&[Some(10.0), Some(10.5), Some(11.1)]);
        assert_eq!(stats.avg, Some(10.5));
        // Parity float path: 11.1 - 10.5 = 0.5999… (IEEE754) → diffs avg 0.5499… → 0.5
        // (đã verify với Python `round(0.5499999999999998, 1) == 0.5`)
        assert_eq!(stats.jitter, Some(0.5));
    }

    fn packet(seq: u32) -> PacketMetadata {
        PacketMetadata {
            timestamp_ms: seq as u64,
            direction: PacketDirection::Clientbound,
            packet_id: seq,
            name: format!("packet_{seq}"),
            size: 64,
        }
    }

    #[test]
    fn packet_ring_evicts_oldest_first() {
        let ring = PacketRing::new(3);
        for i in 0..5 {
            ring.push(packet(i));
        }
        assert_eq!(ring.len(), 3);
        assert_eq!(ring.total_seen(), 5);
        assert_eq!(ring.dropped(), 2);

        let snapshot = ring.export(false);
        // oldest-first: packet_2, packet_3, packet_4
        assert_eq!(snapshot[0].name, "packet_2");
        assert_eq!(snapshot[2].name, "packet_4");
        // export không clear → vẫn còn
        assert_eq!(ring.len(), 3);
    }

    #[test]
    fn packet_ring_export_clear_and_caps() {
        let ring = PacketRing::default_ring();
        assert_eq!(ring.cap(), 10_000);
        let dev = PacketRing::dev_ring();
        assert_eq!(dev.cap(), 100_000);
        // cap quá lớn bị clamp về 100k (§122 developer cap)
        assert_eq!(PacketRing::new(1_000_000).cap(), 100_000);

        for i in 0..4 {
            ring.push(packet(i));
        }
        let snapshot = ring.export(true);
        assert_eq!(snapshot.len(), 4);
        assert!(ring.is_empty());
    }

    #[test]
    fn packet_mode_default_is_off() {
        assert_eq!(PacketMode::default(), PacketMode::Off);
    }
}
