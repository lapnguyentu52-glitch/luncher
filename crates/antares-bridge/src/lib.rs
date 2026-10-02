//! antares-bridge — §43 Python compatibility bridge
//!
//! Quản lý legacy sidecar process: spawn, JSON Lines stdio protocol,
//! version handshake, request timeout, restart, graceful shutdown, crash detection.

pub mod bridge;
pub mod error;
pub mod protocol;

pub use bridge::LegacyBridge;
pub use error::{BridgeError, BridgeResult};
pub use protocol::{
    BridgeNotification, BridgeRequest, BridgeResponse, PROTOCOL_VERSION,
};
