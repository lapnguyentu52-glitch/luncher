pub mod app;
pub mod core_state;
pub mod legacy;

// Re-export commands để lib.rs đăng ký `commands::app_ping` (generate_handler!).
pub use app::*;
pub use core_state::*;
pub use legacy::*;
