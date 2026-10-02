//! Phase 2 — Minecraft Server List Ping (§119), parity `services/diagnostics/net.py::mc_ping`.
//!
//! Tách 2 lớp:
//! - protocol thuần: VarInt pack/read trên byte buffer + build handshake/status packet
//!   (test được không socket)
//! - I/O: `ping()` — TCP connect → handshake → status request → parse response JSON
//!   (blocking sync như legacy; chạy trong thread caller)

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;

pub const DEFAULT_TIMEOUT_SECS: f64 = 4.0;

// ---------------------------------------------------------------------------
// VarInt / packet primitives — chuẩn Server List Ping (parity net.py)
// ---------------------------------------------------------------------------

/// Pack VarInt (32-bit, parity `_pack_varint`: value & 0xFFFFFFFF).
pub fn pack_varint(value: i32) -> Vec<u8> {
    let mut v = (value as u32) & 0xFFFF_FFFF;
    let mut out = Vec::with_capacity(5);
    loop {
        let byte = (v & 0x7F) as u8;
        v >>= 7;
        if v != 0 {
            out.push(byte | 0x80);
        } else {
            out.push(byte);
            return out;
        }
    }
}

/// Đọc VarInt từ byte slice — trả (value, bytes consumed). Lỗi khi quá 5 byte
/// hoặc thiếu byte (parity `_read_varint_stream`: "VarInt too long"/EOF).
pub fn read_varint(buf: &[u8]) -> Result<(i32, usize), PingError> {
    let mut num: i32 = 0;
    for i in 0..5 {
        let byte = *buf
            .get(i)
            .ok_or(PingError::Protocol("EOF while reading VarInt".into()))?;
        num |= ((byte & 0x7F) as i32) << (7 * i);
        if byte & 0x80 == 0 {
            return Ok((num, i + 1));
        }
    }
    Err(PingError::Protocol("VarInt too long".into()))
}

/// `_pack_str`: length-prefixed UTF-8.
pub fn pack_str(s: &str) -> Vec<u8> {
    let raw = s.as_bytes();
    let mut out = pack_varint(raw.len() as i32);
    out.extend_from_slice(raw);
    out
}

/// Build handshake packet body: id=0x00, protocol=-1 (status), host, port, next=1.
pub fn handshake_body(host: &str, port: u16) -> Vec<u8> {
    let mut out = pack_varint(0);
    out.extend_from_slice(&pack_varint(-1));
    out.extend_from_slice(&pack_str(host));
    out.extend_from_slice(&port.to_be_bytes());
    out.extend_from_slice(&pack_varint(1));
    out
}

/// Wrap payload thành 1 frame: length varint + payload (parity `_send_packet`).
pub fn frame(payload: &[u8]) -> Vec<u8> {
    let mut out = pack_varint(payload.len() as i32);
    out.extend_from_slice(payload);
    out
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PingError {
    #[error("invalid host/port")]
    InvalidTarget,
    #[error("io: {0}")]
    Io(String),
    #[error("protocol: {0}")]
    Protocol(String),
    #[error("unexpected packet id {0}")]
    UnexpectedPacket(i32),
    #[error("json: {0}")]
    Json(String),
}

impl PingError {
    /// Code taxonomy §117 — parity `net.ping` trả error string, map sang code.
    pub fn code(&self) -> &'static str {
        match self {
            PingError::InvalidTarget => "CONFIG_INVALID",
            PingError::Io(_) => "NET_UNREACHABLE",
            PingError::Protocol(_) => "NET_PROTOCOL",
            PingError::UnexpectedPacket(_) => "NET_PROTOCOL",
            PingError::Json(_) => "NET_PROTOCOL",
        }
    }
}

// ---------------------------------------------------------------------------
// Ping result — mirror shape legacy mc_ping (camelCase cho UI)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PingPlayers {
    pub online: i64,
    pub max: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PingVersion {
    pub name: String,
    pub protocol: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PingResult {
    pub host: String,
    pub port: u16,
    pub online: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub players: Option<PingPlayers>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<PingVersion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connect_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub favicon: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modinfo: Option<Value>,
}

/// Kết quả offline chuẩn — dùng khi probe không kết nối được.
#[allow(dead_code)]
fn offline(host: &str, port: u16, error: String) -> PingResult {
    PingResult {
        host: host.to_string(),
        port,
        online: false,
        error: Some(error),
        motd: None,
        players: None,
        version: None,
        latency_ms: None,
        connect_ms: None,
        favicon: None,
        modinfo: None,
    }
}

/// Parity legacy: MOTD từ `description` — text trực tiếp hoặc nối `extra[]`.
fn extract_motd(description: &Value) -> String {
    match description {
        Value::String(s) => s.clone(),
        Value::Object(obj) => {
            if let Some(Value::String(text)) = obj.get("text") {
                if !text.is_empty() {
                    return text.clone();
                }
            }
            match obj.get("extra") {
                Some(Value::Array(items)) => items
                    .iter()
                    .map(|item| match item {
                        Value::String(s) => s.clone(),
                        Value::Object(inner) => inner
                            .get("text")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        _ => String::new(),
                    })
                    .collect(),
                _ => String::new(),
            }
        }
        other => other.to_string(),
    }
}

/// Parse status JSON thành PingResult (không I/O — test độc lập, parity phần
/// parse của `mc_ping`).
pub fn parse_status(host: &str, port: u16, raw: &[u8], total_ms: f64, connect_ms: f64) -> Result<PingResult, PingError> {
    let text = String::from_utf8_lossy(raw);
    let data: Value = serde_json::from_str(text.trim()).map_err(|err| PingError::Json(err.to_string()))?;

    let motd_raw = extract_motd(data.get("description").unwrap_or(&Value::Null));
    let players = data.get("players").cloned().unwrap_or(Value::Null);
    let version = data.get("version").cloned().unwrap_or(Value::Null);
    let (online, max) = match &players {
        Value::Object(obj) => (
            obj.get("online").and_then(Value::as_i64).unwrap_or(0),
            obj.get("max").and_then(Value::as_i64).unwrap_or(0),
        ),
        _ => (0, 0),
    };
    let (vname, vproto) = match &version {
        Value::Object(obj) => (
            obj.get("name")
                .map(|v| match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .unwrap_or_else(|| "?".into()),
            obj.get("protocol").and_then(Value::as_i64).unwrap_or(-1),
        ),
        _ => ("?".into(), -1),
    };
    let round1 = |v: f64| (v * 10.0).round() / 10.0;
    Ok(PingResult {
        host: host.to_string(),
        port,
        online: true,
        error: None,
        motd: Some(if motd_raw.is_empty() { String::new() } else { motd_raw }),
        players: Some(PingPlayers { online, max }),
        version: Some(PingVersion { name: vname, protocol: vproto }),
        latency_ms: Some(round1(total_ms)),
        connect_ms: Some(round1(connect_ms)),
        // Parity legacy bool(favicon): chuỗi rỗng → false, có data URI → true.
        favicon: Some(
            data.get("favicon")
                .and_then(Value::as_str)
                .map_or(false, |s| !s.is_empty()),
        ),
        modinfo: data.get("modinfo").cloned(),
    })
}

/// Đọc VarInt trực tiếp từ stream (parity `_read_varint_stream` trên socket).
fn read_varint_stream(stream: &mut TcpStream) -> Result<i32, PingError> {
    let mut num: i32 = 0;
    for i in 0..5 {
        let mut byte = [0u8; 1];
        stream
            .read_exact(&mut byte)
            .map_err(|_| PingError::Protocol("EOF while reading VarInt".into()))?;
        num |= ((byte[0] & 0x7F) as i32) << (7 * i);
        if byte[0] & 0x80 == 0 {
            return Ok(num);
        }
    }
    Err(PingError::Protocol("VarInt too long".into()))
}

/// Đọc đúng n byte (parity `_recv_exact`) — KHÔNG dùng read_to_end vì server
/// không đóng connection ngay sau status (sẽ treo tới timeout).
fn read_exact_n(stream: &mut TcpStream, n: usize) -> Result<Vec<u8>, PingError> {
    let mut out = vec![0u8; n];
    stream
        .read_exact(&mut out)
        .map_err(|err| PingError::Io(err.to_string()))?;
    Ok(out)
}

/// §119 — Server List Ping: DNS+TCP → handshake → status request → response.
pub fn ping(host: &str, port: u16, timeout: Duration) -> Result<PingResult, PingError> {
    let host = host.trim();
    if host.is_empty() || port == 0 {
        return Err(PingError::InvalidTarget);
    }
    let started = Instant::now();
    let addr = format!("{host}:{port}");
    let mut stream = TcpStream::connect(&addr).map_err(|err| PingError::Io(err.to_string()))?;
    stream.set_read_timeout(Some(timeout)).map_err(|err| PingError::Io(err.to_string()))?;
    stream.set_write_timeout(Some(timeout)).map_err(|err| PingError::Io(err.to_string()))?;
    let connect_ms = started.elapsed().as_secs_f64() * 1000.0;

    // handshake + status request (2 frame liên tiếp như legacy)
    let mut request = frame(&handshake_body(host, port));
    request.extend_from_slice(&frame(&pack_varint(0)));
    stream
        .write_all(&request)
        .map_err(|err| PingError::Io(err.to_string()))?;
    stream.flush().map_err(|err| PingError::Io(err.to_string()))?;

    // Response: length varint + [packet id varint + json-length varint + json]
    let packet_len = read_varint_stream(&mut stream)?;
    if packet_len <= 0 || packet_len > 2_097_151 {
        return Err(PingError::Protocol(format!("bad packet length {packet_len}")));
    }
    let data = read_exact_n(&mut stream, packet_len as usize)?;
    let total_ms = started.elapsed().as_secs_f64() * 1000.0;

    let (packet_id, mut offset) = read_varint(&data)?;
    if packet_id != 0 {
        return Err(PingError::UnexpectedPacket(packet_id));
    }
    let (json_len, used) = read_varint(&data[offset..])?;
    offset += used;
    let json_len = json_len as usize;
    if data.len() - offset < json_len {
        return Err(PingError::Protocol("truncated status json".into()));
    }
    parse_status(host, port, &data[offset..offset + json_len], total_ms, connect_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_roundtrip_parity() {
        // Parity _pack_varint: giá trị âm bọc & 0xFFFFFFFF (5 byte chuẩn).
        assert_eq!(pack_varint(0), vec![0x00]);
        assert_eq!(pack_varint(1), vec![0x01]);
        assert_eq!(pack_varint(127), vec![0x7F]);
        assert_eq!(pack_varint(128), vec![0x80, 0x01]);
        assert_eq!(pack_varint(2097151), vec![0xFF, 0xFF, 0x7F]);
        // -1 → 0xFFFFFFFF → 5 byte
        let minus1 = pack_varint(-1);
        assert_eq!(minus1, vec![0xFF, 0xFF, 0xFF, 0xFF, 0x0F]);
        assert_eq!(read_varint(&minus1).unwrap(), (-1, 5));
        assert_eq!(read_varint(&[0x80, 0x01]).unwrap(), (128, 2));
        // Lỗi: quá 5 byte / thiếu byte
        assert!(read_varint(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x01]).is_err());
        assert!(read_varint(&[0x80]).is_err());
    }

    #[test]
    fn handshake_and_frame_shape() {
        let body = handshake_body("localhost", 25565);
        // id 0x00 + protocol -1 + "localhost" + port BE + next 1
        assert_eq!(body[0], 0x00);
        let framed = frame(&body);
        assert_eq!(framed[0] as usize, body.len());
        // pack_str: len prefix + utf8
        let s = pack_str("hi");
        assert_eq!(s, vec![0x02, b'h', b'i']);
    }

    #[test]
    fn parse_status_full_shape() {
        let json = serde_json::json!({
            "description": {"text": "Antares Server"},
            "players": {"online": 3, "max": 20},
            "version": {"name": "1.21.4", "protocol": 769},
            "favicon": "data:image/png;base64,AAAA",
            "modinfo": {"type": "neoforge", "modList": []}
        });
        let raw = serde_json::to_vec(&json).unwrap();
        let result =
            parse_status("mc.example.com", 25565, &raw, 42.34, 12.26).expect("parse ok");
        assert!(result.online);
        assert_eq!(result.motd.as_deref(), Some("Antares Server"));
        let players = result.players.clone().unwrap();
        assert_eq!((players.online, players.max), (3, 20));
        let version = result.version.clone().unwrap();
        assert_eq!(version.name, "1.21.4");
        assert_eq!(version.protocol, 769);
        assert_eq!(result.latency_ms, Some(42.3));
        assert_eq!(result.connect_ms, Some(12.3));
        assert_eq!(result.favicon, Some(true));
        assert!(result.modinfo.is_some());
    }

    #[test]
    fn parse_status_motd_extra_concat_parity() {
        // Parity: description {"extra":[...]} — nối text các phần.
        let json = serde_json::json!({
            "description": {"extra": [
                {"text": "Hello "},
                {"text": "World", "color": "red"},
                "plain"
            ]}
        });
        let raw = serde_json::to_vec(&json).unwrap();
        let result = parse_status("h", 25565, &raw, 1.0, 1.0).unwrap();
        assert_eq!(result.motd.as_deref(), Some("Hello Worldplain"));
    }

    #[test]
    fn parse_status_garbage_is_protocol_error() {
        let err = parse_status("h", 25565, b"not json", 1.0, 1.0).unwrap_err();
        assert_eq!(err.code(), "NET_PROTOCOL");
    }

    #[test]
    fn ping_invalid_target_code() {
        assert_eq!(ping("", 25565, Duration::from_secs(1)).unwrap_err().code(), "CONFIG_INVALID");
        assert_eq!(ping("localhost", 0, Duration::from_secs(1)).unwrap_err().code(), "CONFIG_INVALID");
    }
}
