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

    /// Typed service result (§117 `AppError`) → envelope §96 — command map
    /// request → service → response trong 1 dòng, không hardcode code ở handler.
    pub fn from_result(result: Result<T, antares_app::AppError>) -> Self {
        match result {
            Ok(data) => Self::ok(data),
            Err(err) => Self::err(err.into()),
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

    #[test]
    fn from_result_maps_typed_service_result() {
        use antares_app::{codes, AppError};

        let ok: AntaresResponse<u32> = AntaresResponse::from_result(Ok(7));
        assert!(ok.ok);
        assert_eq!(ok.data, Some(7));

        let err: AntaresResponse<u32> = AntaresResponse::from_result(Err(AppError::new(
            codes::STORAGE_WRITE_FAILED,
            "disk full",
        )));
        assert!(!err.ok);
        let error = err.error.expect("error");
        assert_eq!(error.code, "STORAGE_WRITE_FAILED");
        assert!(error.retryable);
    }
}
