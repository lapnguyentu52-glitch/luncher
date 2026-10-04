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

pub mod accounts;
pub mod disk;
pub mod error;
pub mod install;
pub mod instances;
pub mod launch;
pub mod lock;
pub mod mojang_runtime;
pub mod play;
pub mod runtime;
pub mod services;
pub mod system;

#[cfg(test)]
pub(crate) mod test_http;

pub use error::{codes, AppError, AppResult};
pub use install::{play_install, InstallOutcome, Installer};
pub use instances::{Instance, InstanceMemory, InstanceStore};
pub use launch::{launch, LaunchOutcome};
pub use lock::ResourceLock;
pub use play::{PreflightCheck, PreflightReport, PreflightStatus};
pub use runtime::RuntimeFlags;
pub use services::{AppServices, ScopeInfo, StorageReport};
pub use accounts::AccountStore;
