//! Phase 2 — `ArtifactResolver` + `JavaResolver` (mảnh §111 Orchestrator).
//!
//! - `JavaResolver` — bọc `antares-java::resolve_java` theo resolution order §110,
//!   trả model sẵn sàng cho preflight (java_path/java_major).
//! - `ArtifactResolver` — phiên bản cục bộ: check artifact tồn tại trong
//!   `game_dir/` hoặc shared store (§104) bằng filesystem sync; phần tải-missing
//!   nhờ `antares-downloads` (nối ở phase 3 khi có HTTP engine).
//!
//! Cả hai đều sync, không thread, dễ test.

use std::path::{Path, PathBuf};

use antares_java::{resolve_java, JavaError, JavaRuntime};

/// Kết quả resolve Java cho một lần launch.
#[derive(Debug, Clone)]
pub struct ResolvedJava {
    pub path: String,
    pub major: u16,
    pub source: antares_java::JavaSource,
}

impl From<JavaRuntime> for ResolvedJava {
    fn from(rt: JavaRuntime) -> Self {
        Self {
            path: rt.path,
            major: rt.major,
            source: rt.source,
        }
    }
}

/// §110/§111 — JavaResolver: chain resolve + kiểm executable tồn tại.
pub struct JavaResolver;

impl JavaResolver {
    pub fn resolve(
        required_major: u16,
        instance_explicit: Option<JavaRuntime>,
        profile_explicit: Option<JavaRuntime>,
        managed: Option<JavaRuntime>,
        mojang: Option<JavaRuntime>,
        system: Option<JavaRuntime>,
    ) -> Result<ResolvedJava, JavaError> {
        let runtime = resolve_java(
            required_major,
            instance_explicit,
            profile_explicit,
            managed,
            mojang,
            system,
        )?;
        Ok(runtime.into())
    }
}

/// Trạng thái một artifact cần cho launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactStatus {
    /// Đã có trong local store / game dir — không cần tải.
    Present,
    /// Thiếu — cần enqueue qua antares-downloads.
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedArtifact {
    pub artifact_path: String,
    pub status: ArtifactStatus,
    pub local_path: PathBuf,
}

/// §104 — ArtifactResolver cục bộ: shared store trước (immutable, dùng chung giữa các
/// instance), rồi game_dir fallback. Không tải — chỉ trả status để caller plan download.
pub struct ArtifactResolver {
    /// Thư mục artifacts dùng chung (vd `<root>/shared/artifacts`).
    store_root: PathBuf,
}

impl ArtifactResolver {
    pub fn new(store_root: impl Into<PathBuf>) -> Self {
        Self {
            store_root: store_root.into(),
        }
    }

    pub fn store_root(&self) -> &Path {
        &self.store_root
    }

    /// Resolve 1 artifact: store → game_dir → Missing (ưu tiên store để không
    /// duplicate hàng trăm MB mỗi instance — §104).
    pub fn resolve(&self, artifact_path: &str, game_dir: &Path) -> ResolvedArtifact {
        let store_candidate = self.store_root.join(artifact_path);
        if store_candidate.is_file() {
            return ResolvedArtifact {
                artifact_path: artifact_path.to_string(),
                status: ArtifactStatus::Present,
                local_path: store_candidate,
            };
        }
        let game_candidate = game_dir.join(artifact_path);
        if game_candidate.is_file() {
            return ResolvedArtifact {
                artifact_path: artifact_path.to_string(),
                status: ArtifactStatus::Present,
                local_path: game_candidate,
            };
        }
        ResolvedArtifact {
            artifact_path: artifact_path.to_string(),
            status: ArtifactStatus::Missing,
            // Khi tải xong sẽ commit vào store (immutable) — path đích.
            local_path: store_candidate,
        }
    }

    /// Resolve danh sách artifacts — trả (present, missing) để plan download batch.
    pub fn resolve_many(
        &self,
        artifact_paths: &[String],
        game_dir: &Path,
    ) -> (Vec<ResolvedArtifact>, Vec<ResolvedArtifact>) {
        let mut present = Vec::new();
        let mut missing = Vec::new();
        for path in artifact_paths {
            match self.resolve(path, game_dir) {
                art if art.status == ArtifactStatus::Present => present.push(art),
                art => missing.push(art),
            }
        }
        (present, missing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use antares_java::{JavaSource, JavaRuntime};

    fn runtime(source: JavaSource, major: u16, path: &str) -> JavaRuntime {
        JavaRuntime {
            source,
            path: path.into(),
            major,
            minor: 0,
            architecture: "x86_64".into(),
            vendor: "vendor".into(),
            verified: true,
            capabilities: vec![],
        }
    }

    #[test]
    fn java_resolver_prefers_explicit_and_returns_source() {
        let resolved = JavaResolver::resolve(
            17,
            Some(runtime(JavaSource::InstanceExplicit, 21, "/jvm/21")),
            None,
            Some(runtime(JavaSource::Managed, 17, "/jvm/17")),
            None,
            None,
        )
        .unwrap();
        assert_eq!(resolved.source, JavaSource::InstanceExplicit);
        assert_eq!(resolved.path, "/jvm/21");
        assert_eq!(resolved.major, 21);
    }

    #[test]
    fn java_resolver_not_found_bubbles_code() {
        let err = JavaResolver::resolve(21, None, None, None, None, None).unwrap_err();
        assert_eq!(err.code(), "JAVA_NOT_FOUND");
    }

    #[test]
    fn artifact_resolver_store_priority() {
        let root = std::env::temp_dir().join(format!(
            "antares-launch-art-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("libraries")).unwrap();
        std::fs::write(root.join("libraries/shared.jar"), b"store").unwrap();

        let game_dir = root.join("game");
        std::fs::create_dir_all(game_dir.join("libraries")).unwrap();
        std::fs::write(game_dir.join("libraries/local.jar"), b"game").unwrap();

        let resolver = ArtifactResolver::new(&root);

        // 1. Store thắng trước game dir (§104 — shared immutable).
        let shared = resolver.resolve("libraries/shared.jar", &game_dir);
        assert_eq!(shared.status, ArtifactStatus::Present);
        assert!(shared.local_path.starts_with(&root));
        assert!(!shared.local_path.starts_with(&game_dir));

        // 2. Chỉ có trong game dir → Present với path game.
        let local = resolver.resolve("libraries/local.jar", &game_dir);
        assert_eq!(local.status, ArtifactStatus::Present);
        assert!(local.local_path.starts_with(&game_dir));

        // 3. Thiếu → Missing + path đích là store (commit target).
        let none = resolver.resolve("libraries/absent.jar", &game_dir);
        assert_eq!(none.status, ArtifactStatus::Missing);
        assert!(none.local_path.starts_with(&root));

        // 4. resolve_many chia đúng 2 nhóm.
        let paths = vec![
            "libraries/shared.jar".to_string(),
            "libraries/local.jar".to_string(),
            "libraries/absent.jar".to_string(),
        ];
        let (present, missing) = resolver.resolve_many(&paths, &game_dir);
        assert_eq!(present.len(), 2);
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].artifact_path, "libraries/absent.jar");

        let _ = std::fs::remove_dir_all(&root);
    }
}
