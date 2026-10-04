//! B07b — version JSON (client.json) của Minecraft + rules parity
//! `minecraft-launcher-lib 8.0` (`_helper.parse_rule_list` / `command.get_libraries`
//! / `natives.get_natives`). Launcher cũ dùng MLL — đây là port để build lệnh
//! launch native không cần Python.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Parity `ClientJson` — mô hình đúng những field mà build lệnh cần.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    pub id: String,
    #[serde(rename = "type", default)]
    pub type_name: String,
    /// Version kế thừa (Fabric/Forge) — launcher vanilla 07b từ chối.
    #[serde(default)]
    pub inherits_from: Option<String>,
    /// Base jar khi `jar` khác `id` (kế thừa).
    #[serde(default)]
    pub jar: Option<String>,
    pub main_class: String,
    /// Asset index id (`assets`) — placeholder `${assets_index_name}`.
    #[serde(default)]
    pub assets: Option<String>,
    #[serde(default)]
    pub arguments: Option<Arguments>,
    /// Kiểu cũ (<1.13) thay cho `arguments.game`.
    #[serde(default)]
    pub minecraft_arguments: Option<String>,
    pub libraries: Vec<Library>,
    // ---- Batch 07c — install pipeline (parity MLL install.do_version_install) ----
    /// `downloads.client` — tải client jar về `versions/<id>/<id>.jar`.
    #[serde(default)]
    pub downloads: Option<VersionDownloads>,
    /// `assetIndex` — tải index + objects (thiếu key = version cũ, bỏ qua).
    #[serde(default)]
    pub asset_index: Option<AssetIndex>,
    /// `logging.client.file` — tải logging config (MLL luôn tải, launch chỉ
    /// dùng khi enableLoggingConfig — legacy không bật).
    #[serde(default)]
    pub logging: Option<LoggingConfig>,
    /// `javaVersion` — component Mojang JRE runtime cho version này
    /// (Batch 07d — parity MLL `runtime.get_client_json()["javaVersion"]`).
    #[serde(default)]
    pub java_version: Option<JavaVersion>,
}

/// `javaVersion` của version JSON — `{component, majorVersion}`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    /// vd `java-runtime-delta` — tên component trong manifest runtime.
    /// Option: parity legacy `jv.get("component")` falsy → coi như không có.
    #[serde(default)]
    pub component: Option<String>,
    /// Major (vd 21) — legacy chỉ dùng cho status message; fallback detect
    /// `java -showversion` khi detect fail.
    #[serde(default)]
    pub major_version: Option<u16>,
}

/// `downloads` của version JSON — chỉ `client` cần cho install.
#[derive(Debug, Clone, Deserialize)]
pub struct VersionDownloads {
    #[serde(default)]
    pub client: Option<Artifact>,
}

/// Artifact Maven/CDN — `downloads.artifact` / classifier / `assetIndex` /
/// logging file đều có shape `{url, path?, sha1?, size?}` (thiếu field = null).
#[derive(Debug, Clone, Deserialize)]
pub struct Artifact {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
}

/// `assetIndex` — URL tải index + sha1 verify (parity MLL install_assets).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndex {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub total_size: Option<u64>,
}

/// `logging` — MLL tải `assets/log_configs/<file.id>`.
#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    #[serde(default)]
    pub client: Option<LoggingClient>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingClient {
    #[serde(default)]
    pub file: Option<LoggingFile>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingFile {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
}

/// Lỗi parse/validate version JSON — caller map sang catalog §117.
#[derive(Debug, thiserror::Error)]
pub enum VersionJsonError {
    #[error("invalid version json: {0}")]
    Invalid(#[from] serde_json::Error),
    #[error("version json thiếu arguments/minecraftArguments")]
    MissingArguments,
    #[error("parse trực tiếp version json có inheritsFrom (dùng resolve_version_json): {0}")]
    InheritsFrom(String),
    #[error("version json chưa được cài/không đọc được ({path}): {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("inheritsFrom quá sâu/lặp: {0}")]
    InheritDepth(String),
}

impl VersionJsonError {
    pub fn code(&self) -> &'static str {
        match self {
            VersionJsonError::InheritsFrom(_) | VersionJsonError::InheritDepth(_) => {
                "VALIDATION_FAILED"
            }
            _ => "MC_VERSION_UNKNOWN",
        }
    }
}

/// Parse + validate (`inheritsFrom` → từ chối; thiếu game-args → hỏng).
/// Dùng cho JSON **đã merge** (launch/repair); JSON thô trên đĩa có
/// `inheritsFrom` phải đi qua [`resolve_version_json`] — Batch 07c.
pub fn parse_version_json(bytes: &[u8]) -> Result<VersionJson, VersionJsonError> {
    let data: VersionJson = serde_json::from_slice(bytes)?;
    if let Some(parent) = &data.inherits_from {
        return Err(VersionJsonError::InheritsFrom(parent.clone()));
    }
    if data.arguments.is_none() && data.minecraft_arguments.is_none() {
        return Err(VersionJsonError::MissingArguments);
    }
    Ok(data)
}

// ---------------------------------------------------------------------------
// inheritsFrom — Batch 07c (parity MLL _helper.inherit_json + command.py)
// ---------------------------------------------------------------------------

/// Parity `_get_lib_name_without_version(name)`: `":".join(name.split(":")[:-1])` —
/// bỏ component cuối (version/classifier) để so key lib kế thừa.
fn lib_name_without_version(name: &str) -> String {
    let mut parts: Vec<&str> = name.split(':').collect();
    if parts.len() > 1 {
        parts.pop();
    }
    parts.join(":")
}

/// Parity `inherit_json(original_data=child, path)` — merge version JSON con
/// (child, có `inheritsFrom`) lên bản JSON cha đã đọc:
///
/// - **libraries**: lib con giữ nguyên, lib cha thêm vào sau nếu key
///   (name bỏ version) chưa có (child-first).
/// - Mảng khác: `child + parent`; dict: giữ dict cha, chỉ concat list con
///   vào list cùng key (parent-first — parity thứ tự `new_data[key][a] + b`);
///   scalar: con thắng.
///
/// Khác MLL có chủ đích (an toàn hơn): sub-key dict con không phải list bị
/// MLL bỏ im lặng — vẫn bỏ (parity), nhưng sub-key cha thiếu thì ghi đè trực
/// tiếp thay vì KeyError.
pub fn inherit_json(child: &serde_json::Value, parent: &serde_json::Value) -> serde_json::Value {
    let mut result = parent.clone();
    let (Some(child_obj), Some(result_obj)) = (child.as_object(), result.as_object_mut()) else {
        return child.clone();
    };

    // 1. libraries — parity: child libs + parent libs chưa có key.
    let mut seen: HashSet<String> = child_obj
        .get("libraries")
        .and_then(|v| v.as_array())
        .map(|libs| {
            libs.iter()
                .filter_map(|lib| lib.get("name").and_then(|n| n.as_str()))
                .map(lib_name_without_version)
                .collect()
        })
        .unwrap_or_default();
    let mut lib_list: Vec<serde_json::Value> = child_obj
        .get("libraries")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if let Some(parent_libs) = parent.get("libraries").and_then(|v| v.as_array()) {
        for lib in parent_libs {
            let key = lib
                .get("name")
                .and_then(|n| n.as_str())
                .map(lib_name_without_version)
                .unwrap_or_default();
            if !key.is_empty() && !seen.insert(key) {
                continue; // đã có bản con → không kế thừa (parity)
            }
            lib_list.push(lib.clone());
        }
    }
    result_obj.insert("libraries".into(), serde_json::Value::Array(lib_list));

    // 2. các key còn lại — parity đúng nhánh if/elif/else của MLL.
    for (key, value) in child_obj {
        if key == "libraries" {
            continue; // đã merge ở trên
        }
        match result_obj.get(key) {
            Some(parent_value) if value.is_array() && parent_value.is_array() => {
                let mut merged = value.as_array().cloned().unwrap_or_default();
                merged.extend(parent_value.as_array().cloned().unwrap_or_default());
                result_obj.insert(key.clone(), serde_json::Value::Array(merged));
            }
            Some(parent_value) if value.is_object() && parent_value.is_object() => {                let mut sub = parent_value.clone();
                if let (Some(sub_obj), Some(child_sub)) = (sub.as_object_mut(), value.as_object()) {
                    for (sub_key, sub_value) in child_sub {
                        if !sub_value.is_array() {
                            continue; // parity MLL: chỉ list được merge, scalar con bị bỏ
                        }
                        match sub_obj.get(sub_key).cloned() {
                            Some(existing) if existing.is_array() => {
                                let mut merged = existing.as_array().cloned().unwrap_or_default();
                                merged.extend(sub_value.as_array().cloned().unwrap_or_default());
                                sub_obj.insert(sub_key.clone(), serde_json::Value::Array(merged));
                            }
                            // MLL KeyError nếu cha thiếu key — ta ghi thẳng (an toàn hơn).
                            _ => {
                                sub_obj.insert(sub_key.clone(), sub_value.clone());
                            }
                        }
                    }
                }
                result_obj.insert(key.clone(), sub);
            }
            _ => {
                result_obj.insert(key.clone(), value.clone()); // scalar / loại lệch → con thắng
            }
        }
    }
    result
}

/// Depth guard cho chuỗi `inheritsFrom` vòng lặp / quá dài (MLL không guard).
const MAX_INHERIT_DEPTH: usize = 8;

/// Đọc + merge `versions/<id>/<id>.json` với toàn bộ cha của nó (đệ quy),
/// rồi parse thành [`VersionJson`] đã hợp nhất — parity
/// `command.get_minecraft_command`: `if "inheritsFrom" in data: inherit_json`.
///
/// Lỗi I/O (thiếu JSON) → [`VersionJsonError::Io`] — caller map
/// `MC_VERSION_UNKNOWN` (legacy `VersionNotFound`).
pub fn resolve_version_json(game_dir: &Path, id: &str) -> Result<VersionJson, VersionJsonError> {
    let merged = load_merged_json(game_dir, id, 0)?;
    let data: VersionJson = serde_json::from_value(merged)?;
    if data.arguments.is_none() && data.minecraft_arguments.is_none() {
        return Err(VersionJsonError::MissingArguments);
    }
    Ok(data)
}

/// Đọc JSON thô của `id`; có `inheritsFrom` → resolve cha trước rồi merge.
fn load_merged_json(game_dir: &Path, id: &str, depth: usize) -> Result<serde_json::Value, VersionJsonError> {
    if depth > MAX_INHERIT_DEPTH {
        return Err(VersionJsonError::InheritDepth(id.to_string()));
    }
    let path = game_dir.join("versions").join(id).join(format!("{id}.json"));
    let bytes = std::fs::read(&path).map_err(|err| VersionJsonError::Io {
        path: path.display().to_string(),
        source: err,
    })?;
    let child: serde_json::Value = serde_json::from_slice(&bytes)?;
    let Some(parent_id) = child.get("inheritsFrom").and_then(|v| v.as_str()) else {
        return Ok(child);
    };
    let parent = load_merged_json(game_dir, parent_id, depth + 1)?;
    Ok(inherit_json(&child, &parent))
}

// ---------------------------------------------------------------------------
// arguments (game/jvm) — string hoặc có rules
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<ArgEntry>,
    /// Key vắng mặt ≠ rỗng: MLL `if "jvm" in arguments` — None = dùng default
    /// `-Djava.library.path=… -cp …`.
    #[serde(default)]
    pub jvm: Option<Vec<ArgEntry>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ArgEntry {
    Plain(String),
    Ruled(Box<RuledArg>),
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuledArg {
    /// Một số version JSON cũ dùng `compatibilityRules` (MLL check cả 2 —
    /// mỗi list là một nhóm rule độc lập, cả 2 phải pass).
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default, rename = "compatibilityRules")]
    pub compatibility_rules: Vec<Rule>,
    pub value: ArgValue,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ArgValue {
    One(String),
    Many(Vec<String>),
}

// ---------------------------------------------------------------------------
// libraries
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct Library {
    pub name: String,
    /// Maven **base** (vd `https://maven.fabricmc.net/`) — parity MLL
    /// `install_libraries`: vắng → `https://libraries.minecraft.net`.
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub rules: Option<Vec<Rule>>,
    /// `{"linux": "natives-linux", "windows": "natives-windows-${arch}", …}`
    #[serde(default)]
    pub natives: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(default)]
    pub extract: Option<Extract>,
    #[serde(default)]
    pub downloads: Option<LibraryDownloads>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Extract {
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LibraryDownloads {
    /// `downloads.artifact` — url khác rỗng + có path → tải đúng file đó
    /// (parity nhánh artifact của MLL `install_libraries`).
    #[serde(default)]
    pub artifact: Option<Artifact>,
    #[serde(default)]
    pub classifiers: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    /// "allow" | "disallow".
    pub action: String,
    #[serde(default)]
    pub os: Option<OsRule>,
    #[serde(default)]
    pub features: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OsRule {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
}

// ---------------------------------------------------------------------------
// rules — parity parse_rule_list(rules, options)
// ---------------------------------------------------------------------------

/// Snapshot hệ thống cho rules (parity `platform.system()/architecture()/get_os_version`).
#[derive(Debug, Clone)]
pub struct OsInfo {
    /// "windows" | "osx" | "linux" (MLL: không phải 2 đầu → rơi nhánh linux).
    pub name: &'static str,
    pub bits32: bool,
    /// Parity `get_os_version`: Windows = "major.minor", còn lại = uname release.
    pub version: String,
}

impl OsInfo {
    /// Hệ thống hiện tại.
    pub fn current() -> Self {
        Self {
            name: if cfg!(windows) {
                "windows"
            } else if cfg!(target_os = "macos") {
                "osx"
            } else {
                "linux"
            },
            bits32: cfg!(target_pointer_width = "32"),
            version: os_version(),
        }
    }
}

/// Launch options ảnh hưởng rules (features) — parity MLL `options` dict
/// trong `parse_single_rule`. Mặc định = mọi feature tắt (parity MLL
/// `options={}` — library rules của MLL cũng luôn truyền `{}`).
///
/// Batch 07d: profile launch hint (mục 40) bật customResolution/quickPlay →
/// rules features tương ứng được bật. `is_demo_user` giữ false — legacy không
/// có đường set `demo` (không key trong `_LAUNCH_KEYS`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuleOptions {
    pub custom_resolution: bool,
    /// MLL check `is None` (không phải truthiness) → Some = bật.
    pub quick_play_path: Option<String>,
    pub quick_play_singleplayer: Option<String>,
    pub quick_play_multiplayer: Option<String>,
    pub quick_play_realms: Option<String>,
}

/// Parity `parse_single_rule` + `parse_rule_list` (AND mọi rule; rỗng = true).
///
/// Quirk MLL được giữ nguyên: **giá trị** trong `features` bị bỏ qua — chỉ
/// biết key có mặt, key ánh xạ sang option tương ứng (vắng = tắt); key lạ →
/// xem như satisfied (MLL không có nhánh → rơi xuống).
pub fn rules_pass(rules: &[Rule], os: &OsInfo, opts: &RuleOptions) -> bool {
    rules.iter().all(|rule| rule_applies(rule, os, opts))
}

/// Một rule có áp dụng cho hệ thống hiện tại không (parity `parse_single_rule`).
pub fn rule_applies(rule: &Rule, os: &OsInfo, opts: &RuleOptions) -> bool {
    // action: allow → returnvalue=false (mismatch = không áp), disallow → true.
    let returnvalue = rule.action != "allow";

    if let Some(os_rule) = &rule.os {
        if let Some(name) = &os_rule.name {
            // Chỉ 3 key MLL biết; key lạ bị bỏ qua (không mismatch).
            let matches = match name.as_str() {
                "windows" => os.name == "windows",
                "osx" => os.name == "osx",
                "linux" => os.name == "linux",
                _ => true,
            };
            if !matches {
                return returnvalue;
            }
        }
        if let Some(arch) = &os_rule.arch {
            if arch == "x86" && !os.bits32 {
                return returnvalue;
            }
        }
        if let Some(pattern) = &os_rule.version {
            // parity `re.match` (anchor đầu chuỗi). Regex không compile được
            // (lookahead…) → coi như constraint không khớp nhánh đó (bỏ qua).
            match regex::Regex::new(&format!("^(?:{pattern})")) {
                Ok(re) => {
                    if !re.is_match(&os.version) {
                        return returnvalue;
                    }
                }
                Err(_) => log::warn!("os.version rule không compile được: {pattern}"),
            }
        }
    }

    if let Some(features) = &rule.features {
        for key in features.keys() {
            let live = match key.as_str() {
                "has_custom_resolution" => opts.custom_resolution,
                // legacy không set demo (ngoài `_LAUNCH_KEYS`) → luôn false.
                "is_demo_user" => false,
                // parity: `options.get(key) is None` — Some là đủ, bỏ qua
                // truthiness của giá trị.
                "has_quick_plays_support" => opts.quick_play_path.is_some(),
                "is_quick_play_singleplayer" => opts.quick_play_singleplayer.is_some(),
                "is_quick_play_multiplayer" => opts.quick_play_multiplayer.is_some(),
                "is_quick_play_realms" => opts.quick_play_realms.is_some(),
                _ => continue, // key lạ: MLL không nhánh → satisfied
            };
            if !live {
                return returnvalue;
            }
        }
    }

    !returnvalue
}

// ---------------------------------------------------------------------------
// natives + thư mục (parity natives.get_natives)
// ---------------------------------------------------------------------------

/// Parity `get_natives(lib)` — classifier natives cho OS hiện tại
/// (`${arch}` → "32"/"64"); lib không có natives → None.
pub fn natives_classifier(lib: &Library, os: &OsInfo) -> Option<String> {
    let natives = lib.natives.as_ref()?;
    let arch = if os.bits32 { "32" } else { "64" };
    let key = match os.name {
        "windows" => "windows",
        "osx" => "osx",
        _ => "linux",
    };
    natives
        .get(key)?
        .as_str()
        .map(|s| s.replace("${arch}", arch))
        .filter(|s| !s.is_empty())
}

// ---------------------------------------------------------------------------
// đường dẫn thư mục (parity get_library_path)
// ---------------------------------------------------------------------------

/// Parity `get_library_path(name, path)`:
/// `group:artifact:version[:classifier][@suffix]` →
/// `<path>/libraries/group/dots/artifact/version/artifact-version[-classifier].<suffix>`.
pub fn library_path(root: &Path, name: &str) -> PathBuf {
    let (name, suffix) = match name.split_once('@') {
        Some((n, s)) => (n, s),
        None => (name, "jar"),
    };
    let parts: Vec<&str> = name.split(':').collect();
    // JSON Mojang luôn đủ 3 phần; thiếu → defensive (MLL sẽ IndexError).
    let (group, artifact, version) = if parts.len() >= 3 {
        (parts[0], parts[1], parts[2])
    } else {
        return root.join("libraries").join(name.replace(':', "_"));
    };

    let mut path = root.join("libraries");
    for segment in group.split('.') {
        path = path.join(segment);
    }
    let mut filename = format!("{artifact}-{version}");
    for extra in &parts[3..] {
        filename.push('-');
        filename.push_str(extra);
    }
    filename.push('.');
    filename.push_str(suffix);
    path.join(artifact).join(version).join(filename)
}

/// Parity `get_natives`-extract đích: jar natives của lib =
/// `get_library_path(name)` với `-{native}` chèn trước đuôi file
/// (`os.path.splitext` + `lib_path-native+ext` — MLL KHÔNG dùng classifier path).
pub fn natives_jar_path(root: &Path, name: &str, native: &str) -> PathBuf {
    let base = library_path(root, name);
    let stem = base
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = base
        .extension()
        .map(|s| format!(".{}", s.to_string_lossy()))
        .unwrap_or_default();
    base.with_file_name(format!("{stem}-{native}{ext}"))
}

/// Parity `command.get_libraries` — classpath đầy đủ (natives jar KÈM rules
/// filter), kết thúc bằng client jar (`jar` kế thừa hoặc `id`).
/// Nối bằng separator OS (`:` / `;` trên Windows) — parity `get_classpath_separator`.
pub fn classpath(vj: &VersionJson, root: &Path, os: &OsInfo) -> String {
    let sep = if cfg!(windows) { ';' } else { ':' };
    let mut entries: Vec<String> = Vec::new();

    for lib in &vj.libraries {
        if let Some(rules) = &lib.rules {
            // parity MLL `get_libraries`: library rules luôn parse với
            // `options={}` (không phải options launch).
            if !rules_pass(rules, os, &RuleOptions::default()) {
                continue;
            }
        }
        entries.push(library_path(root, &lib.name).to_string_lossy().into_owned());

        if let Some(native) = natives_classifier(lib, os) {
            let entry = match native_classifier_path(root, lib, &native) {
                Some(path) => path,
                None => library_path(root, &format!("{}-{native}", lib.name)),
            };
            entries.push(entry.to_string_lossy().into_owned());
        }
    }

    // Client jar — parity `if "jar" in data`.
    let jar = vj.jar.as_deref().unwrap_or(&vj.id);
    entries.push(
        root.join("versions")
            .join(jar)
            .join(format!("{jar}.jar"))
            .to_string_lossy()
            .into_owned(),
    );

    entries.join(&sep.to_string())
}

/// Parity: nếu lib có `downloads.classifiers[native].path` → dùng path đó
/// (dưới `libraries/`); không → None (caller fallback maven `name-native`).
fn native_classifier_path(root: &Path, lib: &Library, native: &str) -> Option<PathBuf> {
    let classifiers = lib.downloads.as_ref()?.classifiers.as_ref()?;
    let rel = classifiers.get(native)?.get("path")?.as_str()?;
    Some(root.join("libraries").join(rel))
}

/// Danh sách jar natives cần extract cho OS hiện tại (parity
/// `natives.extract_natives` — luôn maven-style `name-version-native.jar`,
/// exclude list lấy từ `lib.extract`). Dùng khi `versions/<id>/natives` thiếu.
pub fn natives_to_extract(vj: &VersionJson, root: &Path, os: &OsInfo) -> Vec<(PathBuf, Vec<String>)> {
    let mut jobs = Vec::new();
    for lib in &vj.libraries {
        if let Some(rules) = &lib.rules {
            // parity MLL: library rules luôn với `options={}`.
            if !rules_pass(rules, os, &RuleOptions::default()) {
                continue;
            }
        }
        let Some(native) = natives_classifier(lib, os) else {
            continue;
        };
        let excludes = lib
            .extract
            .as_ref()
            .map(|e| e.exclude.clone())
            .unwrap_or_default();
        jobs.push((natives_jar_path(root, &lib.name, &native), excludes));
    }
    jobs
}

/// `game_dir/versions/<id>/natives` — parity default `nativesDirectory` của MLL.
pub fn natives_dir(game_dir: &Path, version_id: &str) -> PathBuf {
    game_dir.join("versions").join(version_id).join("natives")
}

// ---------------------------------------------------------------------------
// os.version parity
// ---------------------------------------------------------------------------

#[cfg(unix)]
fn os_version() -> String {
    // parity `platform.uname().release` (Linux/macOS — MLL nhánh else).
    let mut info: libc::utsname = unsafe { std::mem::zeroed() };
    if unsafe { libc::uname(&mut info) } != 0 {
        return String::new();
    }
    // release: [c_char; 65] — kết thúc NUL.
    let raw: &[libc::c_char] = unsafe { std::slice::from_raw_parts(info.release.as_ptr(), info.release.len()) };
    let bytes: Vec<u8> = raw
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| b as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Parity `inherit_json` — đúng thứ tự merge child/parent của MLL.
    #[test]
    fn inherit_json_merges_libraries_arguments_and_scalars() {
        let child = json!({
            "id": "fabric-loader-0.16.14-1.21.11",
            "inheritsFrom": "1.21.11",
            "jar": null,
            "type": "release",
            "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
            "arguments": { "game": ["--fabric"], "jvm": [] },
            "libraries": [
                { "name": "net.fabricmc:fabric-loader:0.16.14" },
                { "name": "org.ow2.asm:asm:9.7" }
            ]
        });
        let parent = json!({
            "id": "1.21.11",
            "type": "release",
            "mainClass": "net.minecraft.client.main.Main",
            "assets": "19",
            "arguments": { "game": ["--username"], "jvm": ["-cp"] },
            "libraries": [
                { "name": "org.ow2.asm:asm:9.6" },
                { "name": "com.mojang:brigadier:1.3.13" }
            ]
        });

        let merged = inherit_json(&child, &parent);

        // scalar: con thắng (id/mainClass); jar: null giữ (Option → None → fallback id).
        assert_eq!(merged["id"], "fabric-loader-0.16.14-1.21.11");
        assert_eq!(merged["mainClass"], "net.fabricmc.loader.impl.launch.knot.KnotClient");
        assert!(merged["jar"].is_null());
        // key chỉ có cha giữ nguyên (assets).
        assert_eq!(merged["assets"], "19");

        // libraries: 2 lib con + brigadier (asm đã có key con → không kế thừa).
        let libs = merged["libraries"].as_array().unwrap();
        assert_eq!(libs.len(), 3);
        assert_eq!(libs[0]["name"], "net.fabricmc:fabric-loader:0.16.14");
        assert_eq!(libs[2]["name"], "com.mojang:brigadier:1.3.13");

        // dict-nested list: parity MLL `new_data[key][a] + b` = CHA trước + con sau.
        let game = merged["arguments"]["game"].as_array().unwrap();
        assert_eq!(game, &["--username", "--fabric"]);
        let jvm = merged["arguments"]["jvm"].as_array().unwrap();
        assert_eq!(jvm, &["-cp"]);
    }

    #[test]
    fn inherit_json_subkey_scalar_child_dropped_by_parity() {
        let child = json!({ "arguments": { "game": ["--fabric"], "extra": "x" } });
        let parent = json!({ "arguments": { "game": ["--username"], "extra": "p" } });
        let merged = inherit_json(&child, &parent);
        // parity MLL: dict merge CHỈ concat list — scalar con bị bỏ, cha giữ.
        assert_eq!(merged["arguments"]["extra"], "p");
    }

    /// Fixture parent (client.json shape thật) copy vào temp game_dir.
    fn write_parent(game_dir: &Path) {
        let dir = game_dir.join("versions/1.21.11");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("1.21.11.json"),
            include_str!("../fixtures/game/versions/1.21.11/1.21.11.json"),
        )
        .unwrap();
    }

    #[test]
    fn resolve_version_json_merges_chain_from_disk() {
        let game_dir = std::env::temp_dir().join(format!(
            "antares-mcjson-resolve-{}-{:x}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        let _ = std::fs::remove_dir_all(&game_dir);
        write_parent(&game_dir);

        let child_dir = game_dir.join("versions/fabric-loader-0.16.14-1.21.11");
        std::fs::create_dir_all(&child_dir).unwrap();
        std::fs::write(
            child_dir.join("fabric-loader-0.16.14-1.21.11.json"),
            r#"{"id":"fabric-loader-0.16.14-1.21.11","inheritsFrom":"1.21.11","type":"release",
                "mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient",
                "arguments":{"game":[],"jvm":[]},
                "libraries":[{"name":"net.fabricmc:fabric-loader:0.16.14"}]}"#,
        )
        .unwrap();

        let vj = resolve_version_json(&game_dir, "fabric-loader-0.16.14-1.21.11")
            .expect("resolve");
        assert_eq!(vj.id, "fabric-loader-0.16.14-1.21.11");
        assert_eq!(vj.main_class, "net.fabricmc.loader.impl.launch.knot.KnotClient");
        // game args = con (rỗng) + cha → vanilla đủ cờ.
        assert!(vj.arguments.is_some());
        let game = vj.arguments.as_ref().unwrap().game.as_slice();
        assert!(!game.is_empty(), "kế thừa arguments.game của parent");
        // classpath: lib con trước, parent còn lại phía sau (key không trùng).
        assert_eq!(vj.libraries[0].name, "net.fabricmc:fabric-loader:0.16.14");
        assert!(vj.libraries.iter().any(|l| l.name.starts_with("com.mojang:brigadier")));
        // jar = None (child null) → classpath fallback về id.
        assert!(vj.jar.is_none());

        let _ = std::fs::remove_dir_all(&game_dir);
    }

    #[test]
    fn resolve_version_json_missing_is_io_mc_version_unknown() {
        let game_dir = std::env::temp_dir().join("antares-mcjson-missing-0");
        let err = resolve_version_json(&game_dir, "9.9.9").expect_err("thiếu json");
        assert_eq!(err.code(), "MC_VERSION_UNKNOWN");
        assert!(matches!(err, VersionJsonError::Io { .. }));
    }

    #[test]
    fn resolve_version_json_detects_inherit_loop() {
        let game_dir = std::env::temp_dir().join(format!(
            "antares-mcjson-loop-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&game_dir);
        let dir = game_dir.join("versions/a");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.json"), r#"{"id":"a","inheritsFrom":"b","mainClass":"M","minecraftArguments":"x","libraries":[]}"#).unwrap();
        let dir = game_dir.join("versions/b");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("b.json"), r#"{"id":"b","inheritsFrom":"a","mainClass":"M","minecraftArguments":"x","libraries":[]}"#).unwrap();

        let err = resolve_version_json(&game_dir, "a").expect_err("vòng lặp");
        assert_eq!(err.code(), "VALIDATION_FAILED");
        assert!(matches!(err, VersionJsonError::InheritDepth(_)));
        let _ = std::fs::remove_dir_all(&game_dir);
    }

    // ---- Batch 07d — features rules theo launch options (parity parse_single_rule) ----

    fn feature_rule(key: &str) -> Rule {
        Rule {
            action: "allow".into(),
            os: None,
            features: Some(
                serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&format!(
                    "{{\"{key}\": true}}"
                ))
                .unwrap(),
            ),
        }
    }

    #[test]
    fn feature_rules_default_off_match_mll_options_empty() {
        let os = OsInfo { name: "linux", bits32: false, version: "6.8.0".into() };
        let opts = RuleOptions::default();
        // parity MLL options={} → mọi feature rule allow bị loại
        // (rule_applies = returnvalue = false cho allow).
        for key in [
            "has_custom_resolution",
            "is_demo_user",
            "has_quick_plays_support",
            "is_quick_play_singleplayer",
            "is_quick_play_multiplayer",
            "is_quick_play_realms",
        ] {
            assert!(
                !rule_applies(&feature_rule(key), &os, &opts),
                "{key} phải tắt khi options mặc định"
            );
        }
        // key lạ vẫn satisfied (quirk MLL — không có nhánh → rơi xuống).
        assert!(rule_applies(&feature_rule("no_such_feature"), &os, &opts));
    }

    #[test]
    fn feature_rules_follow_launch_options() {
        let os = OsInfo { name: "linux", bits32: false, version: "6.8.0".into() };
        let opts = RuleOptions {
            custom_resolution: true,
            quick_play_singleplayer: Some("world1".into()),
            ..RuleOptions::default()
        };
        assert!(rule_applies(&feature_rule("has_custom_resolution"), &os, &opts));
        assert!(rule_applies(&feature_rule("is_quick_play_singleplayer"), &os, &opts));
        // MLL check `is None` — multi/realms/path chưa set → vẫn tắt.
        assert!(!rule_applies(&feature_rule("is_quick_play_multiplayer"), &os, &opts));
        assert!(!rule_applies(&feature_rule("is_quick_play_realms"), &os, &opts));
        assert!(!rule_applies(&feature_rule("has_quick_plays_support"), &os, &opts));
        // demo không có đường set qua profile (ngoài _LAUNCH_KEYS) → luôn tắt.
        assert!(!rule_applies(&feature_rule("is_demo_user"), &os, &opts));

        // disallow + feature đang bật → rule không áp (mismatch trả returnvalue).
        let disallow = Rule {
            action: "disallow".into(),
            features: Some(
                serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(
                    "{\"has_custom_resolution\": true}",
                )
                .unwrap(),
            ),
            os: None,
        };
        assert!(!rule_applies(&disallow, &os, &opts));
    }

    #[test]
    fn version_json_parses_java_version() {
        let vj = parse_version_json(
            br#"{"id":"1.21.11","type":"release","mainClass":"M",
                "javaVersion":{"component":"java-runtime-delta","majorVersion":21},
                "minecraftArguments":"x","libraries":[]}"#,
        )
        .expect("parse");
        let jv = vj.java_version.expect("javaVersion");
        assert_eq!(jv.component.as_deref(), Some("java-runtime-delta"));
        assert_eq!(jv.major_version, Some(21));
        // version không có javaVersion (fixture cũ) → None.
        let plain = parse_version_json(include_bytes!(
            "../fixtures/game/versions/1.8.9/1.8.9.json"
        ))
        .expect("parse legacy");
        assert!(plain.java_version.is_none());
    }
}

#[cfg(windows)]
fn os_version() -> String {
    // parity `sys.getwindowsversion()` → "major.minor" (RtlGetVersion — ntdll
    // luôn có, không cần windows-sys chỉ cho 1 struct).
    #[repr(C)]
    struct OsVersionInfoW {
        size: u32,
        major: u32,
        minor: u32,
        build: u32,
        platform_id: u32,
        csd_version: [u16; 128],
    }
    #[link(name = "ntdll")]
    extern "system" {
        fn RtlGetVersion(info: *mut OsVersionInfoW) -> i32;
    }
    let mut info = OsVersionInfoW {
        size: std::mem::size_of::<OsVersionInfoW>() as u32,
        major: 0,
        minor: 0,
        build: 0,
        platform_id: 0,
        csd_version: [0; 128],
    };
    let rc = unsafe { RtlGetVersion(&mut info) };
    if rc != 0 {
        return String::new();
    }
    format!("{}.{}", info.major, info.minor)
}
