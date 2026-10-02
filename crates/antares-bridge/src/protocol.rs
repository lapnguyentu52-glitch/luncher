use serde::{Deserialize, Serialize};

/// §234 — Version handshake: sidecar phải trả cùng PROTOCOL_VERSION.
pub const PROTOCOL_VERSION: u32 = 1;

/// §43 — Request: `{"id":"req-123","method":"resource.build","params":{}}`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeRequest {
    pub id: String,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// §43 — Response: `{"id":"req-123","ok":true,"data":{}}`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeResponse {
    pub id: String,
    pub ok: bool,
    #[serde(default)]
    pub data: serde_json::Value,
    #[serde(default)]
    pub error: Option<BridgeErrorPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeErrorPayload {
    pub code: String,
    pub message: String,
}

/// Notification: sidecar → launcher, không cần response (events/progress/logs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeNotification {
    pub event: String,
    #[serde(default)]
    pub payload: serde_json::Value,
}

/// Methods chuẩn của sidecar health/handshake (§234).
pub mod methods {
    pub const PING: &str = "health.ping";
    pub const VERSION: &str = "health.version";
    pub const SHUTDOWN: &str = "health.shutdown";
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    pub protocol: u32,
    pub service: String,
    pub service_version: String,
    pub python: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_roundtrip() {
        let req = BridgeRequest {
            id: "req-1".into(),
            method: "resource.build".into(),
            params: serde_json::json!({"project": "demo"}),
        };
        let line = serde_json::to_string(&req).expect("serialize");
        let parsed: BridgeRequest = serde_json::from_str(&line).expect("parse");
        assert_eq!(parsed.id, "req-1");
        assert_eq!(parsed.method, "resource.build");
    }

    #[test]
    fn response_error_optional() {
        let json = r#"{"id":"req-2","ok":false,"error":{"code":"X","message":"boom"}}"#;
        let parsed: BridgeResponse = serde_json::from_str(json).expect("parse");
        assert!(!parsed.ok);
        assert_eq!(parsed.error.expect("error").code, "X");
    }

    #[test]
    fn notification_defaults() {
        let parsed: BridgeNotification =
            serde_json::from_str(r#"{"event":"download.progress"}"#).expect("parse");
        assert_eq!(parsed.event, "download.progress");
        assert!(parsed.payload.is_null());
    }
}
