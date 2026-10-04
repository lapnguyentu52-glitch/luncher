pub mod accounts;
pub mod app;
pub mod core_state;
pub mod dashboard;
pub mod instances;
pub mod java;
pub mod legacy;
pub mod play;

// Re-export commands để lib.rs đăng ký `commands::app_ping` (generate_handler!).
pub use accounts::*;
pub use app::*;
pub use core_state::*;
pub use dashboard::*;
pub use instances::*;
pub use java::*;
pub use legacy::*;
pub use play::*;
