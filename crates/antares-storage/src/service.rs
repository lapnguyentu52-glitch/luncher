use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use crate::error::{StorageResult, StorageError};
use crate::paths::ScopedRoot;

/// §98 — StorageService: mọi service phải đi qua đây để đụng filesystem.
#[derive(Clone)]
pub struct StorageService {
    root: Arc<PathBuf>,
}

impl StorageService {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: Arc::new(root.into()),
        }
    }

    /// Cấp handle scoped cho một service — handle chỉ thấy trong root của nó.
    pub fn scoped(&self, root: ScopedRoot) -> StorageHandle {
        StorageHandle {
            root: self.root.join(root.dir_name()),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[derive(Clone)]
pub struct StorageHandle {
    root: PathBuf,
}

impl StorageHandle {
    /// Root tuyệt đối của scope — metadata cần path thật
    /// (vd `instance.json` field `directory`).
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn resolve(&self, relative: &str) -> StorageResult<PathBuf> {
        sanitize_relative(relative).map(|rel| self.root.join(rel))
    }

    /// §98.1 — tạo thư mục (kèm cha) trong scope — sanitize như read/write.
    pub fn ensure_dir(&self, relative: &str) -> StorageResult<PathBuf> {
        let path = self.resolve(relative)?;
        std::fs::create_dir_all(&path).map_err(|source| StorageError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Ok(path)
    }

    /// §98.1 — tên thư mục con TRỰC TIẾP dưới scope root (bỏ file) — parity
    /// instance scan (`InstanceService.list`). Không nhận relative → không có
    /// cửa traversal. Scope chưa tồn tại → rỗng (parity `exists() else ()`).
    pub fn list_dirs(&self) -> StorageResult<Vec<String>> {
        let read = match std::fs::read_dir(&self.root) {
            Ok(read) => read,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Vec::new());
            }
            Err(source) => {
                return Err(StorageError::Io {
                    path: self.root.display().to_string(),
                    source,
                })
            }
        };
        let mut names = Vec::new();
        for entry in read {
            let entry = entry.map_err(|source| StorageError::Io {
                path: self.root.display().to_string(),
                source,
            })?;
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    names.push(name.to_string());
                }
            }
        }
        names.sort();
        Ok(names)
    }

    /// §98.1 — read_json
    pub fn read_json<T: serde::de::DeserializeOwned>(&self, relative: &str) -> StorageResult<T> {
        let path = self.resolve(relative)?;
        let bytes = std::fs::read(&path).map_err(|source| StorageError::Io {
            path: path.display().to_string(),
            source,
        })?;
        serde_json::from_slice(&bytes).map_err(|source| StorageError::Json {
            path: path.display().to_string(),
            source,
        })
    }

    /// §98.1 — write_json_atomic: tmp + rename cùng filesystem.
    pub fn write_json_atomic<T: serde::Serialize>(&self, relative: &str, value: &T) -> StorageResult<()> {
        let path = self.resolve(relative)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| StorageError::Io {
                path: parent.display().to_string(),
                source,
            })?;
        }
        let tmp = path.with_extension("tmp");
        {
            let file = std::fs::File::create(&tmp).map_err(|source| StorageError::Io {
                path: tmp.display().to_string(),
                source,
            })?;
            let mut writer = std::io::BufWriter::new(file);
            serde_json::to_writer(&mut writer, value)
                .map_err(|source| StorageError::Json {
                    path: path.display().to_string(),
                    source,
                })?;
            writer.flush().map_err(|source| StorageError::Io {
                path: tmp.display().to_string(),
                source,
            })?;
        }
        std::fs::rename(&tmp, &path).map_err(|source| StorageError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Ok(())
    }

    /// §98.1 — remove_safe: không follow symlink ngoài root (đơn giản: chỉ remove file trong root).
    pub fn remove_safe(&self, relative: &str) -> StorageResult<()> {
        let path = self.resolve(relative)?;
        std::fs::remove_file(&path).map_err(|source| StorageError::Io {
            path: path.display().to_string(),
            source,
        })
    }

    /// §98.1 — list_scoped: tên entry (file) trong một thư mục scoped.
    pub fn list_scoped(&self, relative: &str) -> StorageResult<Vec<String>> {
        let path = self.resolve(relative)?;
        let read = std::fs::read_dir(&path).map_err(|source| StorageError::Io {
            path: path.display().to_string(),
            source,
        })?;
        let mut names = Vec::new();
        for entry in read {
            let entry = entry.map_err(|source| StorageError::Io {
                path: path.display().to_string(),
                source,
            })?;
            if entry.path().is_file() {
                if let Some(name) = entry.file_name().to_str() {
                    names.push(name.to_string());
                }
            }
        }
        names.sort();
        Ok(names)
    }
}

/// §50 Path sandbox: normalize + reject traversal/absolute trước khi join root.
fn sanitize_relative(relative: &str) -> StorageResult<PathBuf> {
    let candidate = Path::new(relative);
    if candidate.is_absolute() {
        return Err(StorageError::PathEscape(relative.to_string()));
    }
    let mut safe = PathBuf::new();
    for component in candidate.components() {
        match component {
            Component::Normal(part) => safe.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(StorageError::PathEscape(relative.to_string()))
            }
        }
    }
    if safe.as_os_str().is_empty() {
        return Err(StorageError::PathEscape(relative.to_string()));
    }
    Ok(safe)
}
