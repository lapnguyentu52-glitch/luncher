//! AssetStore — parity 1:1 `services/resources/assets.py` (mục 12, 12.1–12.4).
//!
//! Security (mục 12.2 + 23):
//! - Không trust extension — PNG signature + IHDR + dims (mục 77 header parse)
//! - Giới hạn: 8 MB/file, 4096px (decision Batch 0 — mục 34)
//! - Content-addressed: `data/assets/<sha256>.png` — traversal không thể vì
//!   filename không bao giờ chạm filesystem raw
//! - Duplicate import cùng content → reuse file (hash trùng), catalog vẫn thêm
//!   entry mới (nhiều entry trỏ cùng file — parity comment legacy)
//! - Catalog `data/asset-catalog.json` atomic; corrupt → backup `.corrupt` + reset
//!
//! PNG decode full (IDAT inflate) không cần cho import — dims + signature đủ
//! (mục 77); decode thật chỉ khi preview (caller đọc file rồi render).

use std::path::{Path, PathBuf};

use serde_json::json;

use crate::png::{check_png_message, decode_png_dims, PNG_SIG};
use crate::{safe_category, safe_name, safe_tag, AssetEntry, ResourceError, ASSET_CATEGORIES};

pub const MAX_FILE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_DIM: u32 = 4096;

/// Asset store — `<data>/assets/` + `<data>/asset-catalog.json`.
pub struct AssetStore {
    data_dir: PathBuf,
    now: fn() -> f64,
    id_gen: fn() -> String,
}

impl AssetStore {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            now: || {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs_f64())
                    .unwrap_or(0.0)
            },
            id_gen: || {
                let nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.subsec_nanos())
                    .unwrap_or(0);
                format!("{nanos:012x}")
            },
        }
    }

    /// Inject clock/id cho test.
    pub fn with_clock(data_dir: impl Into<PathBuf>, now: fn() -> f64, id_gen: fn() -> String) -> Self {
        Self {
            data_dir: data_dir.into(),
            now,
            id_gen,
        }
    }

    fn catalog_path(&self) -> PathBuf {
        self.data_dir.join("asset-catalog.json")
    }

    fn files_dir(&self) -> PathBuf {
        let d = self.data_dir.join("assets");
        let _ = std::fs::create_dir_all(&d);
        d
    }

    fn load_catalog(&self) -> serde_json::Value {
        let path = self.catalog_path();
        if !path.is_file() {
            return json!({"schemaVersion": 1, "assets": []});
        }
        match std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        {
            Some(data) if data.get("assets").and_then(|v| v.as_array()).is_some() => data,
            // Parity corrupt: backup `.json.corrupt` + reset
            _ => {
                let backup = path.with_extension("json.corrupt");
                if let Ok(bytes) = std::fs::read(&path) {
                    let _ = std::fs::write(backup, bytes);
                }
                json!({"schemaVersion": 1, "assets": []})
            }
        }
    }

    fn save_catalog(&self, catalog: &serde_json::Value) -> Result<(), ResourceError> {
        let path = self.catalog_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| ResourceError::Io(err.to_string()))?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, catalog.to_string())
            .map_err(|err| ResourceError::Io(err.to_string()))?;
        std::fs::rename(&tmp, &path).map_err(|err| ResourceError::Io(err.to_string()))?;
        Ok(())
    }

    /// Import 1 PNG → validate → hash → lưu file + metadata (mục 12.1 flow).
    /// Không decode IDAT — dims + signature đủ cho chặn (mục 77); decode fail-safe
    /// thật (malformed IDAT) xảy ra khi preview — caller render.
    pub fn import_png(
        &self,
        data: &[u8],
        name: Option<&str>,
        category: &str,
        tags: &[String],
    ) -> Result<AssetEntry, ResourceError> {
        // 1. Size check (trước cả decode — mục 12.2 huge file)
        if data.is_empty() {
            return Err(ResourceError::Empty);
        }
        if data.len() > MAX_FILE_BYTES {
            return Err(ResourceError::TooLarge {
                size: data.len(),
                limit: MAX_FILE_BYTES,
            });
        }
        // 2. Signature check (mục 12.2: không trust extension)
        if data.len() < 8 || data[..8] != PNG_SIG {
            return Err(ResourceError::Validation(
                "Không phải PNG hợp lệ (signature sai)".into(),
            ));
        }
        // 3. Header check + dims (mục 77 — chặn trước decode)
        if data.len() < 33 || &data[12..16] != b"IHDR" {
            return Err(ResourceError::Invalid("PNG header không hợp lệ".into()));
        }
        let (w, h) = decode_png_dims(data).map_err(|err| match err {
            crate::png::PngError::BadSignature | crate::png::PngError::BadHeader => {
                ResourceError::Invalid("PNG header không hợp lệ".into())
            }
            crate::png::PngError::TooLarge(w, h) => ResourceError::Validation(format!(
                "Kích thước {w}x{h} ngoài giới hạn (max {MAX_DIM}px)"
            )),
            crate::png::PngError::BadZlib => {
                ResourceError::Validation("PNG decode thất bại".into())
            }
        })?;
        // 4. Full header check (parity check_png_message — dims + sig)
        check_png_message(data).map_err(ResourceError::Validation)?;

        // 5. SHA256 + content-addressed file
        let sha = antares_downloads::sha256_hex(data);
        let files = self.files_dir();
        let asset_path = files.join(format!("{sha}.png"));
        if !asset_path.exists() {
            let tmp = files.join(format!(".tmp-{}", (self.id_gen)()));
            std::fs::write(&tmp, data).map_err(|err| ResourceError::Io(err.to_string()))?;
            std::fs::rename(&tmp, &asset_path)
                .map_err(|err| ResourceError::Io(err.to_string()))?;
        }

        // 6. Metadata record
        let name_clean = {
            let n = name.map(safe_name).unwrap_or_default();
            if n.is_empty() {
                format!("asset-{}", &sha[..8])
            } else {
                n
            }
        };
        let mut catalog = self.load_catalog();
        let now = (self.now)();
        let entry = AssetEntry {
            id: (self.id_gen)(),
            name: name_clean.clone(),
            sha256: sha.clone(),
            mime: "image/png".into(),
            width: w,
            height: h,
            bytes: data.len(),
            category: safe_category(category),
            tags: tags
                .iter()
                .map(|t| safe_tag(t))
                .filter(|t| !t.is_empty())
                .take(16)
                .collect(),
            source: "imported".into(),
            created_at: now,
            updated_at: now,
        };
        if let Some(assets) = catalog.get_mut("assets").and_then(|v| v.as_array_mut()) {
            assets.push(serde_json::to_value(&entry).unwrap_or(json!(null)));
        }
        self.save_catalog(&catalog)?;
        Ok(entry)
    }

    /// Query (mục 12.4): query name/sha[:12]/tags + category + tag filter, sort
    /// newest/oldest/name/size.
    pub fn list(
        &self,
        query: &str,
        category: &str,
        tag: &str,
        sort: &str,
    ) -> Result<Vec<AssetEntry>, ResourceError> {
        let catalog = self.load_catalog();
        let q = query.trim().to_lowercase();
        let mut out: Vec<AssetEntry> = catalog["assets"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| serde_json::from_value(v.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();
        out.retain(|a| {
            // get(..12): catalog tay cũ có thể chứa sha ngắn — không panic (mục 76)
            let sha12 = a.sha256.get(..12).unwrap_or(&a.sha256);
            if !q.is_empty()
                && !a.name.to_lowercase().contains(&q)
                && !sha12.contains(&q)
                && !a.tags.iter().any(|t| t.to_lowercase().contains(&q))
            {
                return false;
            }
            if !category.is_empty() && a.category != category {
                return false;
            }
            if !tag.is_empty() && !a.tags.iter().any(|t| t == tag) {
                return false;
            }
            true
        });
        match sort {
            "oldest" => out.sort_by(|a, b| a.created_at.partial_cmp(&b.created_at).unwrap_or(std::cmp::Ordering::Equal)),
            "name" => out.sort_by(|a, b| a.name.cmp(&b.name)),
            "size" => out.sort_by(|a, b| b.bytes.cmp(&a.bytes)),
            // mặc định newest
            _ => out.sort_by(|a, b| b.created_at.partial_cmp(&a.created_at).unwrap_or(std::cmp::Ordering::Equal)),
        }
        Ok(out)
    }

    pub fn get(&self, asset_id: &str) -> Result<AssetEntry, ResourceError> {
        let catalog = self.load_catalog();
        for value in catalog["assets"].as_array().unwrap_or(&vec![]) {
            if value.get("id").and_then(|v| v.as_str()) == Some(asset_id) {
                return serde_json::from_value(value.clone())
                    .map_err(|err| ResourceError::Io(err.to_string()));
            }
        }
        Err(ResourceError::NotFound(format!("Asset not found: {asset_id}")))
    }

    /// PNG bytes — path content-addressed (traversal-safe, hash regex chặn).
    pub fn read_png(&self, asset_id: &str) -> Result<Vec<u8>, ResourceError> {
        let asset = self.get(asset_id)?;
        let sha = &asset.sha256;
        // parity regex ^[0-9a-f]{64}$ — lowercase hex only (chặn filename lạ).
        if sha.len() != 64 || !sha.chars().all(|c| c.is_ascii_digit() || matches!(c, 'a'..='f')) {
            return Err(ResourceError::Validation("Invalid asset hash".into()));
        }
        let path = self.files_dir().join(format!("{sha}.png"));
        std::fs::read(&path).map_err(|_| ResourceError::NotFound("Asset file missing".into()))
    }

    /// Xoá metadata record; file theo hash giữ lại (GC sau — parity).
    pub fn delete(&self, asset_id: &str) -> Result<bool, ResourceError> {
        let mut catalog = self.load_catalog();
        let before = catalog["assets"].as_array().map(|a| a.len()).unwrap_or(0);
        if let Some(assets) = catalog.get_mut("assets").and_then(|v| v.as_array_mut()) {
            assets.retain(|a| a.get("id").and_then(|v| v.as_str()) != Some(asset_id));
        }
        let after = catalog["assets"].as_array().map(|a| a.len()).unwrap_or(0);
        if after == before {
            return Err(ResourceError::NotFound(format!("Asset not found: {asset_id}")));
        }
        self.save_catalog(&catalog)?;
        Ok(true)
    }

    /// Gán asset vào project — copy PNG vào `<project>/generated/` tại target.
    /// Target phải `assets/<ns>/textures/*.png` (mục 12.4 assignment).
    pub fn assign_to_project(
        &self,
        project_dir: &Path,
        asset_id: &str,
        target_rel: &str,
    ) -> Result<serde_json::Value, ResourceError> {
        let asset = self.get(asset_id)?;
        let rel = target_rel.trim().replace('\\', "/");
        if !rel.starts_with("assets/") || !rel.ends_with(".png") {
            return Err(ResourceError::Validation(format!(
                "Target path phải trong assets/: {rel:?}"
            )));
        }
        if rel.contains("..") || rel.contains("//") {
            return Err(ResourceError::Validation("Invalid target path".into()));
        }
        // phải nằm dưới textures/<ns> (parity regex ^assets/[a-z0-9_-]+/textures/.+\.png$)
        let valid = rel
            .strip_prefix("assets/")
            .and_then(|rest| rest.split_once('/'))
            .map(|(ns, rest)| {
                is_ns(ns) && rest.starts_with("textures/") && rest.ends_with(".png")
                    && rest.len() > "textures/.png".len()
            })
            .unwrap_or(false);
        if !valid {
            return Err(ResourceError::Validation(format!(
                "Target phải dưới textures/: {rel:?}"
            )));
        }

        let dest = project_dir.join("generated").join(&rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|err| ResourceError::Io(err.to_string()))?;
        }
        std::fs::write(&dest, self.read_png(asset_id)?)
            .map_err(|err| ResourceError::Io(err.to_string()))?;

        // Ghi assignment vào project.json (mục 5.1 assets array — replace cùng path)
        let pj_path = project_dir.join("project.json");
        let mut project: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&pj_path).map_err(|err| ResourceError::Io(err.to_string()))?,
        )
        .map_err(|err| ResourceError::Io(err.to_string()))?;
        let now = (self.now)();
        let entry = json!({
            "assetId": asset.id,
            "sha256": asset.sha256,
            "path": rel,
            "assignedAt": now,
        });
        let mut assets = project
            .get("assets")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        assets.retain(|a| a.get("path").and_then(|v| v.as_str()) != Some(rel.as_str()));
        assets.push(entry);
        project["assets"] = json!(assets);
        project["updatedAt"] = json!(now);
        let tmp = pj_path.with_extension("json.tmp");
        std::fs::write(&tmp, project.to_string())
            .map_err(|err| ResourceError::Io(err.to_string()))?;
        std::fs::rename(&tmp, &pj_path).map_err(|err| ResourceError::Io(err.to_string()))?;
        Ok(json!({
            "path": rel,
            "asset": asset.name,
            "sha256": asset.sha256,
        }))
    }
}

/// Namespace check parity regex `[a-z0-9_-]+`.
fn is_ns(ns: &str) -> bool {
    !ns.is_empty()
        && ns
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Free-function wrapper (parity API surface).
pub fn import_png(
    store: &AssetStore,
    data: &[u8],
    name: Option<&str>,
    category: &str,
    tags: &[String],
) -> Result<AssetEntry, ResourceError> {
    store.import_png(data, name, category, tags)
}

pub fn list_assets(
    store: &AssetStore,
    query: &str,
    category: &str,
    tag: &str,
    sort: &str,
) -> Result<Vec<AssetEntry>, ResourceError> {
    store.list(query, category, tag, sort)
}

pub fn read_png(store: &AssetStore, asset_id: &str) -> Result<Vec<u8>, ResourceError> {
    store.read_png(asset_id)
}

pub fn assign_to_project(
    store: &AssetStore,
    project_dir: &Path,
    asset_id: &str,
    target_rel: &str,
) -> Result<serde_json::Value, ResourceError> {
    store.assign_to_project(project_dir, asset_id, target_rel)
}

#[allow(unused_imports)]
use ASSET_CATEGORIES as _CATEGORIES_KEEP;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::png::encode_png;

    fn temp_root(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("antares-assets-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fixed_clock() -> (fn() -> f64, fn() -> String) {
        (|| 1_700_000_000.0, || "abc123def456".to_string())
    }

    fn store(root: &Path) -> AssetStore {
        let (now, id) = fixed_clock();
        AssetStore::with_clock(root, now, id)
    }

    fn sample_png() -> Vec<u8> {
        encode_png(2, 2, &[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255])
            .unwrap()
    }

    #[test]
    fn import_validates_and_stores_content_addressed() {
        let root = temp_root("import");
        let s = store(&root);
        let entry = s.import_png(&sample_png(), Some("My Icon 01!"), "block", &["Red".into(), "!!".into()]).unwrap();
        assert_eq!(entry.name, "my-icon-01");
        assert_eq!(entry.category, "block");
        assert_eq!(entry.tags, vec!["red"]);
        assert_eq!(entry.width, 2);
        assert_eq!(entry.height, 2);
        assert_eq!(entry.mime, "image/png");
        // file theo sha tồn tại
        assert!(root.join("assets").join(format!("{}.png", entry.sha256)).is_file());
        // import lần 2 cùng content — reuse file (không ghi đè), entry mới
        let entry2 = s.import_png(&sample_png(), None, "item", &[]).unwrap();
        assert_eq!(entry2.sha256, entry.sha256);
        assert_eq!(entry2.name, format!("asset-{}", &entry.sha256[..8]), "tên rỗng → asset-<sha8>");
        assert_eq!(s.list("", "", "", "newest").unwrap().len(), 2);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn import_rejects_bad_input() {
        let root = temp_root("reject");
        let s = store(&root);
        // trống
        assert_eq!(s.import_png(b"", None, "item", &[]).unwrap_err().code(), "VALIDATION_FAILED");
        // quá lớn (fake data lớn hơn 8MB)
        let big = vec![0u8; MAX_FILE_BYTES + 1];
        assert_eq!(
            s.import_png(&big, None, "item", &[]).unwrap_err().code(),
            "ASSET_TOO_LARGE"
        );
        // signature sai
        assert_eq!(
            s.import_png(b"not a png file at all", None, "item", &[]).unwrap_err().code(),
            "VALIDATION_FAILED"
        );
        // sig ok nhưng dims quá lớn — dựng header 5000x5000
        let mut data = PNG_SIG.to_vec();
        data.extend_from_slice(&13u32.to_be_bytes());
        data.extend_from_slice(b"IHDR");
        data.extend_from_slice(&5000u32.to_be_bytes());
        data.extend_from_slice(&5000u32.to_be_bytes());
        data.extend_from_slice(&[8, 6, 0, 0, 0]);
        data.extend_from_slice(&[0u8; 20]);
        let err = s.import_png(&data, None, "item", &[]).unwrap_err();
        assert_eq!(err.code(), "VALIDATION_FAILED");
        assert!(err.to_string().contains("5000x5000"));
        // IHDR thiếu
        let mut data = PNG_SIG.to_vec();
        data.extend_from_slice(&[0u8; 30]);
        assert_eq!(s.import_png(&data, None, "item", &[]).unwrap_err().code(), "ASSET_INVALID");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn list_filters_and_sorts() {
        let root = temp_root("list");
        let s = store(&root);
        let png = sample_png();
        s.import_png(&png, Some("alpha"), "block", &["red".into()]).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        // content KHÁC alpha — sha filter mới tách được 2 entry (cùng content → 2 kết quả)
        let png2 = encode_png(2, 2, &[9, 9, 9, 255, 9, 9, 9, 255, 9, 9, 9, 255, 9, 9, 9, 255])
            .unwrap();
        s.import_png(&png2, Some("beta"), "item", &[]).unwrap();
        // query theo tên
        assert_eq!(s.list("alpha", "", "", "").unwrap().len(), 1);
        // query theo tag
        assert_eq!(s.list("red", "", "", "").unwrap().len(), 1);
        // query theo sha prefix
        let all = s.list("", "", "", "").unwrap();
        assert_eq!(s.list(&all[0].sha256[..8], "", "", "").unwrap().len(), 1);
        // category filter
        assert_eq!(s.list("", "block", "", "").unwrap().len(), 1);
        assert_eq!(s.list("", "sound", "", "").unwrap().len(), 0);
        // tag filter
        assert_eq!(s.list("", "", "red", "").unwrap().len(), 1);
        // sort name
        let sorted = s.list("", "", "", "name").unwrap();
        assert_eq!(sorted[0].name, "alpha");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn get_read_delete_roundtrip() {
        let root = temp_root("get");
        let s = store(&root);
        let entry = s.import_png(&sample_png(), Some("x"), "item", &[]).unwrap();
        // get
        assert_eq!(s.get(&entry.id).unwrap().sha256, entry.sha256);
        assert_eq!(
            s.get("nope").unwrap_err().code(),
            "FILE_NOT_FOUND"
        );
        // read png → bytes đúng
        assert_eq!(s.read_png(&entry.id).unwrap(), sample_png());
        // delete → entry mất, file còn
        assert!(s.delete(&entry.id).unwrap());
        assert!(s.get(&entry.id).is_err());
        assert!(root.join("assets").join(format!("{}.png", entry.sha256)).is_file());
        // delete lần 2 → FILE_NOT_FOUND (parity assets.py delete raise)
        assert_eq!(
            s.delete(&entry.id).unwrap_err().code(),
            "FILE_NOT_FOUND",
            "delete lần 2 → not found"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn catalog_corrupt_backs_up_and_resets() {
        let root = temp_root("corrupt");
        let s = store(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("asset-catalog.json"), "{corrupt").unwrap();
        // load corrupt → reset + backup
        let entries = s.list("", "", "", "").unwrap();
        assert!(entries.is_empty());
        assert!(root.join("asset-catalog.json.corrupt").is_file());
        // import tiếp vẫn hoạt động
        let entry = s.import_png(&sample_png(), Some("x"), "item", &[]).unwrap();
        assert_eq!(s.list("", "", "", "").unwrap().len(), 1);
        let _ = entry;
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn assign_to_project_validates_target() {
        let root = temp_root("assign");
        let s = store(&root);
        let entry = s.import_png(&sample_png(), Some("x"), "item", &[]).unwrap();
        // project dir + project.json tối giản
        let pdir = root.join("projects/p1");
        std::fs::create_dir_all(&pdir).unwrap();
        std::fs::write(
            pdir.join("project.json"),
            r#"{"id": "p1", "name": "P1"}"#,
        )
        .unwrap();

        // hợp lệ: assets/<ns>/textures/*.png
        let result = s
            .assign_to_project(&pdir, &entry.id, "assets/minecraft/textures/item/x.png")
            .unwrap();
        assert_eq!(result["path"], "assets/minecraft/textures/item/x.png");
        assert!(pdir.join("generated/assets/minecraft/textures/item/x.png").is_file());
        // project.json có assets array
        let pj: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(pdir.join("project.json")).unwrap()).unwrap();
        assert_eq!(pj["assets"][0]["assetId"], entry.id);

        // sai prefix
        assert!(s.assign_to_project(&pdir, &entry.id, "textures/item/x.png").is_err());
        // sai ext
        assert!(s.assign_to_project(&pdir, &entry.id, "assets/minecraft/textures/item/x.txt").is_err());
        // traversal
        assert!(s.assign_to_project(&pdir, &entry.id, "assets/../evil.png").is_err());
        // double slash
        assert!(s.assign_to_project(&pdir, &entry.id, "assets//minecraft/textures/x.png").is_err());
        // không textures/
        assert!(s.assign_to_project(&pdir, &entry.id, "assets/minecraft/item/x.png").is_err());
        // assign lại cùng path → replace entry (không duplicate)
        s.assign_to_project(&pdir, &entry.id, "assets/minecraft/textures/item/x.png")
            .unwrap();
        let pj: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(pdir.join("project.json")).unwrap()).unwrap();
        assert_eq!(pj["assets"].as_array().unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }
}
