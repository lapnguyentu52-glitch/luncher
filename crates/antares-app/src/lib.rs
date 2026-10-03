//! F-14 / M3 (Batch 04) — native service composition root.
//!
//! Mục tiêu kiến trúc (audit M3): Tauri command gọi application services qua
//! crate này — không gọi crate thấp trực tiếp, không qua Python sidecar:
//!
//! ```text
//! src-tauri command → antares-app::AppServices → service crate → filesystem/net
//! ```
//!
//! Command chỉ map request → service → response (§88.2); business logic nằm
//! trong service crate. Lỗi đi theo typed `AppError` (§117) → envelope §96.

pub mod error;
pub mod runtime;
pub mod services;

pub use error::{codes, AppError, AppResult};
pub use runtime::RuntimeFlags;
pub use services::{AppServices, ScopeInfo, StorageReport};
