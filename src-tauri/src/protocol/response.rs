use serde::Serialize;

use crate::protocol::error::AntaresError;

/// §96 — Response envelope chuẩn hoá cho mọi command.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AntaresResponse<T> {
    pub ok: bool,
    pub data: Option<T>,
    pub error: Option<AntaresError>,
    pub warnings: Vec<String>,
}

impl<T> AntaresResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            ok: true,
            data: Some(data),
            error: None,
            warnings: Vec::new(),
        }
    }

    pub fn err(error: AntaresError) -> Self {
        Self {
            ok: false,
            data: None,
            error: Some(error),
            warnings: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_response_serializes() {
        let res: AntaresResponse<u32> = AntaresResponse::ok(42);
        let json = serde_json::to_string(&res).expect("serialize");
        assert!(json.contains("\"ok\":true"));
        assert!(json.contains("\"data\":42"));
    }

    #[test]
    fn error_response_serializes() {
        let res: AntaresResponse<u32> =
            AntaresResponse::err(crate::protocol::error::AntaresError::new("INSTANCE_LOCKED", "busy", true));
        let json = serde_json::to_string(&res).expect("serialize");
        assert!(json.contains("\"ok\":false"));
        assert!(json.contains("INSTANCE_LOCKED"));
    }
}
