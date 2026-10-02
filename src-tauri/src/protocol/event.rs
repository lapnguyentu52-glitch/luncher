//! Contract event dùng chung giữa src-tauri và TS — single source of truth
//! nằm ở antares-core (src-tauri import trực tiếp từ đó, không copy schema thứ hai).

/// Channel Tauri mà event bridge emit envelope lên. Đồng bộ với
/// apps/desktop/src/types/protocol.ts EVENT_CHANNEL.
pub const EVENT_CHANNEL: &str = "antares://event";
