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
