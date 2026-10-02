//! Phase 3 — TCP check / probe timeline / DNS, parity `services/diagnostics/net.py`
//! và handlers `net.probe`/`net.dns` trong sidecar (§118/§119).
//!
//! - `tcp_check` — 1 connect + RTT ms, không gửi dữ liệu (parity shape `{ok, ms, error}`)
//! - `probe` — N lần TCP (cap 30) → stats (`rtt_stats`) + timeline per-sample
//! - `dns_lookup` — resolve host → dedupe addresses + ms (parity `getaddrinfo` loop)
//! - `ENDPOINTS` + `check_endpoints` — endpoint launcher cần (mojang/modrinth/…)

use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::rtt_stats;

pub const DEFAULT_TIMEOUT_SECS: f64 = 4.0;

/// §42 — endpoint launcher cần (parity `ENDPOINTS` net.py).
pub const ENDPOINTS: &[(&str, &str, u16)] = &[
    ("mojang", "launchermeta.mojang.com", 443),
    ("mojang_session", "sessionserver.mojang.com", 443),
    ("ely_auth", "authserver.ely.by", 443),
    ("modrinth", "api.modrinth.com", 443),
    ("curseforge", "api.curseforge.com", 443),
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TcpCheck {
    pub host: String,
    pub port: u16,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn short_err(err: &std::io::Error) -> String {
    let msg = err.to_string();
    // Parity _short_err: cắt 160 ký tự.
    if msg.len() > 160 { msg[..160].to_string() } else { msg }
}

/// Parity `tcp_check` — nhưng invalid host/port là error type (Rust style),
/// caller map sang shape riêng nếu cần.
pub fn tcp_check(host: &str, port: u16, timeout: Duration) -> TcpCheck {
    let host = host.trim();
    if host.is_empty() || port == 0 {
        return TcpCheck {
            host: host.to_string(),
            port,
            ok: false,
            ms: None,
            error: Some("invalid host/port".into()),
        };
    }
    let started = Instant::now();
    let addr = format!("{host}:{port}");
    let result = addr
        .to_socket_addrs()
        .and_then(|addrs| {
            let mut last_err = None;
            for addr in addrs {
                match TcpStream::connect_timeout(&addr, timeout) {
                    Ok(_) => return Ok(()),
                    Err(err) => last_err = Some(err),
                }
            }
            Err(last_err.unwrap_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::Other, "no addresses")
            }))
        });
    let ms = round1(started.elapsed().as_secs_f64() * 1000.0);
    match result {
        Ok(()) => TcpCheck { host: host.to_string(), port, ok: true, ms: Some(ms), error: None },
        Err(err) => TcpCheck {
            host: host.to_string(),
            port,
            ok: false,
            ms: Some(ms),
            error: Some(short_err(&err)),
        },
    }
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

// ---------------------------------------------------------------------------
// Probe — network timeline (mục 42), parity handle_net_probe
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeSample {
    pub seq: usize,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Thời điểm sample (giây, round 4 — parity `at`).
    pub at: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    pub host: String,
    pub port: u16,
    pub count: usize,
    pub ok: usize,
    pub stats: crate::RttStats,
    pub timeline: Vec<ProbeSample>,
}

/// §42 — N lần TCP connect (cap 30) → RTT/jitter/loss + timeline. Sync — N nhỏ,
/// UI gọi khi bấm nút, không poll liên tục (§171).
pub fn probe(host: &str, port: u16, count: usize, timeout: Duration) -> ProbeResult {
    let count = count.clamp(1, 30);
    let mut timeline = Vec::with_capacity(count);
    let mut samples: Vec<Option<f64>> = Vec::with_capacity(count);
    for seq in 0..count {
        let started = Instant::now();
        let check = tcp_check(host, port, timeout);
        samples.push(if check.ok { check.ms } else { None });
        timeline.push(ProbeSample {
            seq,
            ok: check.ok,
            ms: check.ms,
            error: check.error,
            at: round4(started.elapsed().as_secs_f64()),
        });
    }
    let ok_count = timeline.iter().filter(|t| t.ok).count();
    ProbeResult {
        host: host.trim().to_string(),
        port,
        count,
        ok: ok_count,
        stats: rtt_stats(&samples),
        timeline,
    }
}

fn round4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

// ---------------------------------------------------------------------------
// DNS — parity handle_net_dns
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DnsResult {
    pub host: String,
    pub ok: bool,
    pub addresses: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// DNS resolve + dedupe giữ thứ tự (parity `getaddrinfo` loop).
pub fn dns_lookup(host: &str) -> DnsResult {
    let host = host.trim();
    if host.is_empty() {
        return DnsResult {
            host: String::new(),
            ok: false,
            addresses: vec![],
            ms: None,
            error: Some("host is required".into()),
        };
    }
    let started = Instant::now();
    // getaddrinfo parity: port 0 (không connect), mọi family (IPv4 + IPv6).
    match (host, 0u16).to_socket_addrs() {
        Ok(addrs) => {
            let mut addresses: Vec<String> = Vec::new();
            for addr in addrs {
                let ip = addr.ip().to_string();
                if !addresses.contains(&ip) {
                    addresses.push(ip);
                }
            }
            let ms = round1(started.elapsed().as_secs_f64() * 1000.0);
            DnsResult { host: host.to_string(), ok: true, addresses, ms: Some(ms), error: None }
        }
        Err(err) => DnsResult {
            host: host.to_string(),
            ok: false,
            addresses: vec![],
            ms: None,
            error: Some(short_err(&err)),
        },
    }
}

/// Parity `check_endpoints` — tuần tự (legacy song song qua thread pool; Rust
/// phase này giữ sync đơn giản, song song hoá khi nối tokio ở Tauri shell).
pub fn check_endpoints(timeout: Duration) -> Vec<(String, TcpCheck)> {
    let mut results: Vec<(String, TcpCheck)> = ENDPOINTS
        .iter()
        .map(|(id, host, port)| ((*id).to_string(), tcp_check(host, *port, timeout)))
        .collect();
    // Thứ tự ổn định theo tên endpoint (parity sort by id).
    results.sort_by(|a, b| a.0.cmp(&b.0));
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tcp_check_invalid_target() {
        let result = tcp_check("", 25565, Duration::from_secs(1));
        assert!(!result.ok);
        assert_eq!(result.error.as_deref(), Some("invalid host/port"));
        assert!(result.ms.is_none());
    }

    #[test]
    fn tcp_check_unreachable_host_reports_error_with_ms() {
        // Port 1 trên loopback gần như chắc chắn không mở → error path chạy thật.
        let result = tcp_check("127.0.0.1", 1, Duration::from_millis(300));
        assert!(!result.ok);
        assert!(result.error.is_some());
        assert!(result.ms.is_some());
    }

    #[test]
    fn probe_count_clamped_to_30() {
        let result = probe("127.0.0.1", 1, 100, Duration::from_millis(50));
        assert_eq!(result.count, 30); // cap 30 (parity handle_net_probe)
        assert_eq!(result.timeline.len(), 30);
        assert!(result.timeline.iter().all(|t| !t.ok));
        // stats loss 100% khi tất cả fail
        assert_eq!(result.stats.loss, 100.0);
        assert_eq!(result.stats.min, None);
    }

    #[test]
    fn probe_minimal_count_ok_shape() {
        let result = probe("127.0.0.1", 1, 1, Duration::from_millis(50));
        assert_eq!(result.count, 1);
        assert_eq!(result.timeline[0].seq, 0);
        assert!(result.timeline[0].at < 1.0);
    }

    #[test]
    fn dns_lookup_empty_host() {
        let result = dns_lookup("  ");
        assert!(!result.ok);
        assert!(result.addresses.is_empty());
    }

    #[test]
    fn dns_lookup_localhost_dedupe() {
        // "localhost" luôn resolve được trên CI; ít nhất 1 addr, dedupe không trùng.
        let result = dns_lookup("localhost");
        assert!(result.ok, "localhost phải resolve được");
        assert!(!result.addresses.is_empty());
        let unique: std::collections::HashSet<&String> = result.addresses.iter().collect();
        assert_eq!(unique.len(), result.addresses.len());
        assert!(result.ms.unwrap_or(0.0) >= 0.0);
    }

    #[test]
    fn dns_lookup_garbage_host_is_error_not_panic() {
        let result = dns_lookup("this-host-does-not-exist-antares.invalid");
        assert!(!result.ok);
        assert!(result.error.is_some());
        assert!(result.addresses.is_empty());
    }

    #[test]
    fn endpoints_list_shape() {
        assert_eq!(ENDPOINTS.len(), 5);
        assert_eq!(ENDPOINTS[0].0, "mojang");
        let results = check_endpoints(Duration::from_millis(50));
        assert_eq!(results.len(), 5);
        // sort by id ổn định
        let ids: Vec<&str> = results.iter().map(|(id, _)| id.as_str()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
    }
}
