//! B07b — JvmConfig → argv parity `services/minecraft/launch/jvm.py`.
//!
//! Legacy orchestrator dựng `JvmConfig(max/min/gc_mode/custom_args)` từ
//! instance.json rồi `build_args()` thành `options["jvmArguments"]` — truyền
//! thẳng vào đầu lệnh launch (trước version jvm args).

/// Parity `JvmConfig` dataclass — `gc_mode`: `auto | g1 | balanced`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JvmConfig {
    pub min_memory_mb: u32,
    pub max_memory_mb: u32,
    pub gc_mode: String,
    pub custom_args: Vec<String>,
    pub system_properties: Vec<(String, String)>,
}

impl JvmConfig {
    /// Parity `__post_init__`: max < min → min = max (không sửa silent max).
    pub fn new(
        min_memory_mb: u32,
        max_memory_mb: u32,
        gc_mode: impl Into<String>,
        custom_args: Vec<String>,
    ) -> Self {
        let max = max_memory_mb;
        let min = if max < min_memory_mb { max } else { min_memory_mb };
        Self {
            min_memory_mb: min,
            max_memory_mb: max,
            gc_mode: gc_mode.into(),
            custom_args,
            system_properties: Vec::new(),
        }
    }

    /// Parity `validate()` — danh sách warning (không tự sửa silent).
    pub fn validate(&self) -> Vec<String> {
        let mut warnings = Vec::new();
        if self.max_memory_mb < 512 {
            warnings.push("Max memory < 512MB — game có thể crash.".into());
        }
        if self.max_memory_mb < self.min_memory_mb {
            warnings.push("Max memory < min memory.".into());
        }
        for arg in &self.custom_args {
            if arg.starts_with("-Xmx") || arg.starts_with("-Xms") {
                warnings.push(format!(
                    "Custom arg '{arg}' conflicts with memory settings."
                ));
            }
        }
        warnings
    }

    /// Parity `build_args(config)` — `low_end` = parity `_low_end()` (psutil
    /// total ≤ 4GiB hoặc cpu ≤ 2) — caller (app layer) tự tính vì đây là
    /// system probe §14 (không depend psutil ở crate thuần logic).
    pub fn build_args(&self, cpu_count: usize, low_end: bool) -> Vec<String> {
        let mut args = vec![
            format!("-Xms{}M", self.min_memory_mb),
            format!("-Xmx{}M", self.max_memory_mb),
        ];
        if self.gc_mode == "g1" || (self.gc_mode == "auto" && low_end) {
            args.push("-XX:+UseG1GC".into());
            args.push("-XX:MaxGCPauseMillis=50".into());
        } else if self.gc_mode == "balanced" {
            args.push("-XX:+UnlockExperimentalVMOptions".into());
            args.push(format!(
                "-XX:ParallelGCThreads={}",
                (cpu_count / 2).max(1)
            ));
        }
        for (key, value) in &self.system_properties {
            args.push(format!("-D{key}={value}"));
        }
        args.extend(
            self.custom_args
                .iter()
                .filter(|a| !a.is_empty() && !a.starts_with("-Xmx") && !a.starts_with("-Xms"))
                .cloned(),
        );
        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parity_build_args_auto_low_end_uses_g1() {
        let cfg = JvmConfig::new(512, 2048, "auto", vec![]);
        assert_eq!(
            cfg.build_args(8, true),
            vec![
                "-Xms512M".to_string(),
                "-Xmx2048M".to_string(),
                "-XX:+UseG1GC".to_string(),
                "-XX:MaxGCPauseMillis=50".to_string()
            ]
        );
        // auto + đủ mạnh → không thêm GC flag (parity: chỉ -Xms/-Xmx)
        assert_eq!(
            cfg.build_args(8, false),
            vec!["-Xms512M".to_string(), "-Xmx2048M".to_string()]
        );
    }

    #[test]
    fn parity_build_args_balanced_uses_half_cpus() {
        let cfg = JvmConfig::new(1024, 4096, "balanced", vec![]);
        assert_eq!(
            cfg.build_args(4, false),
            vec![
                "-Xms1024M".to_string(),
                "-Xmx4096M".to_string(),
                "-XX:+UnlockExperimentalVMOptions".to_string(),
                "-XX:ParallelGCThreads=2".to_string()
            ]
        );
        // cpu=1 → max(1, 0) = 1
        assert!(cfg.build_args(1, false).contains(&"-XX:ParallelGCThreads=1".to_string()));
    }

    #[test]
    fn custom_args_filtered_and_properties_prefixed() {
        let mut cfg = JvmConfig::new(512, 1024, "auto", vec!["".into(), "-Xmx999M".into(), "-Dfoo=1".into()]);
        cfg.system_properties.push(("file.encoding".into(), "UTF-8".into()));
        let args = cfg.build_args(8, false);
        assert!(!args.contains(&"-Xmx999M".to_string()), "custom -Xmx bị lọc");
        assert!(args.contains(&"-Dfile.encoding=UTF-8".to_string()));
        assert!(args.contains(&"-Dfoo=1".to_string()));
    }

    #[test]
    fn post_init_clamps_min_to_max() {
        let cfg = JvmConfig::new(4096, 1024, "auto", vec![]);
        assert_eq!(cfg.min_memory_mb, 1024);
    }

    #[test]
    fn parity_validate_warnings() {
        let cfg = JvmConfig::new(512, 256, "auto", vec!["-Xmx999M".into()]);
        // max<min → min clamp theo __post_init__ TRƯỚC khi validate (legacy cùng thứ tự)
        let warnings = cfg.validate();
        assert!(warnings.iter().any(|w| w.contains("conflicts with memory settings")));
        let low = JvmConfig::new(512, 256, "auto", vec![]);
        assert!(low.validate().iter().any(|w| w.contains("512MB")), "max<512 warning");
    }
}
