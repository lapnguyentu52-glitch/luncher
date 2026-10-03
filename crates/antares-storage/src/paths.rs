/// §98.2 — Scoped roots. Service chỉ được cấp root mà nó cần.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopedRoot {
    AppData,
    Cache,
    Logs,
    Profiles,
    Instances,
    Backups,
}

impl ScopedRoot {
    /// Mọi scope — dùng cho init/report (composition root không phải liệt kê tay).
    pub const ALL: &'static [ScopedRoot] = &[
        ScopedRoot::AppData,
        ScopedRoot::Cache,
        ScopedRoot::Logs,
        ScopedRoot::Profiles,
        ScopedRoot::Instances,
        ScopedRoot::Backups,
    ];

    pub fn dir_name(self) -> &'static str {
        match self {
            ScopedRoot::AppData => "app-data",
            ScopedRoot::Cache => "cache",
            ScopedRoot::Logs => "logs",
            ScopedRoot::Profiles => "profiles",
            ScopedRoot::Instances => "instances",
            ScopedRoot::Backups => "backups",
        }
    }
}
