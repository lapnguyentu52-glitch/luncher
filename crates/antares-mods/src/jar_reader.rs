//! JAR reader & Java bytecode decoder — parity `services/mods/scanner/jar_reader.py`.
//!
//! Decode Java class file format (JVMS spec) thuần Rust, không cần JVM:
//! - magic `0xCAFEBABE`, version, constant pool đầy đủ (Utf8/Class/Methodref/
//!   MethodHandle/InvokeDynamic…), interfaces, fields & methods
//! - PARSE Code attribute: walk bytecode thu per-method invokes (callee classes)
//! - BootstrapMethods attribute — phục vụ InvokeDynamic analysis
//! - `scan_jar_classes` — decode tối đa 500 class (tránh DoS); `jar_structure` —
//!   file khả nghi (exe/dll/bat), nested jar, config json/toml

use std::collections::BTreeSet;

use crate::zipread::{zip_entries, zip_read_entry, ZipError};

pub const MAGIC: u32 = 0xCAFEBABE;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClassFileError {
    #[error("not a java class file (bad magic)")]
    BadMagic,
    #[error("unknown constant tag {0}")]
    UnknownTag(u8),
    #[error("truncated class file")]
    Truncated,
    #[error("corrupt jar: {0}")]
    CorruptJar(String),
}

impl ClassFileError {
    pub fn code(&self) -> &'static str {
        "APP_INTERNAL"
    }
}

#[derive(Debug, Clone, Default)]
pub struct MethodHandleInfo {
    pub kind: u8,
    pub ref_class: String,
    pub ref_name: String,
}

#[derive(Debug, Clone, Default)]
pub struct BootstrapMethod {
    pub handle: Option<MethodHandleInfo>,
    pub args_classes: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ClassInfo {
    pub name: String,
    pub super_name: String,
    pub interfaces: Vec<String>,
    pub version_major: u16,
    pub version_minor: u16,
    pub strings: Vec<String>,
    pub class_refs: Vec<String>,
    /// "class.method"
    pub method_refs: Vec<String>,
    pub field_refs: Vec<String>,
    /// Call graph: callee class (từ Code attribute invokes).
    pub callees: BTreeSet<String>,
    pub invokedynamic_count: usize,
    pub bootstrap_methods: Vec<BootstrapMethod>,
    pub bootstrap_classes: BTreeSet<String>,
    pub uses_reflection: bool,
    pub uses_method_handles: bool,
}

impl ClassInfo {
    /// Major 52 = Java 8, 61 = Java 17, 65 = Java 21.
    pub fn java_version(&self) -> i32 {
        if self.version_major >= 45 {
            self.version_major as i32 - 44
        } else {
            self.version_major as i32
        }
    }
}

/// Đọc big-endian u16/u32.
fn be_u16(data: &[u8], pos: usize) -> Result<u16, ClassFileError> {
    data.get(pos..pos + 2)
        .map(|b| u16::from_be_bytes([b[0], b[1]]))
        .ok_or(ClassFileError::Truncated)
}

fn be_u32(data: &[u8], pos: usize) -> Result<u32, ClassFileError> {
    data.get(pos..pos + 4)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or(ClassFileError::Truncated)
}

/// Constant pool entry (subset field legacy dùng — giữ đủ shape JVM spec,
/// một số field chỉ dùng khi parse nâng cao nên chưa được đọc).
#[allow(dead_code)]
enum PoolEntry {
    Utf8(String),
    Class(u16),
    String(u16),
    Ref { class_index: u16, nat_index: u16 },
    NameAndType { name_idx: u16, desc_idx: u16 },
    MethodHandle { kind: u8, ref_index: u16 },
    InvokeDynamic { bootstrap_index: u16, nat_index: u16 },
    Numeric,
}

/// Decode 1 file .class từ bytes. Raise ClassFileError nếu không phải class.
pub fn decode_class(data: &[u8]) -> Result<ClassInfo, ClassFileError> {
    if data.len() < 10 || be_u32(data, 0)? != MAGIC {
        return Err(ClassFileError::BadMagic);
    }
    let mut info = ClassInfo::default();
    info.version_minor = be_u16(data, 4)?;
    info.version_major = be_u16(data, 6)?;

    let mut pos = 8usize;
    let cp_count = be_u16(data, pos)? as usize;
    pos += 2;

    // ---- Parse constant pool (1-indexed; long/double chiếm 2 slot) ----
    let mut pool: std::collections::BTreeMap<usize, PoolEntry> = Default::default();
    let mut i = 1usize;
    while i < cp_count {
        let tag = *data.get(pos).ok_or(ClassFileError::Truncated)?;
        pos += 1;
        match tag {
            1 => {
                // Utf8
                let length = be_u16(data, pos)? as usize;
                pos += 2;
                let raw = data
                    .get(pos..pos + length)
                    .ok_or(ClassFileError::Truncated)?;
                pos += length;
                let value = String::from_utf8_lossy(raw).into_owned();
                pool.insert(i, PoolEntry::Utf8(value));
            }
            3 | 4 => {
                // Integer / Float
                pos += 4;
                pool.insert(i, PoolEntry::Numeric);
            }
            5 | 6 => {
                // Long / Double — chiếm 2 slot
                pos += 8;
                pool.insert(i, PoolEntry::Numeric);
                i += 1;
            }
            7 | 8 | 16 | 19 | 20 => {
                // Class / String / MethodType / Module / Package — 1 ref
                let index = be_u16(data, pos)?;
                pos += 2;
                pool.insert(
                    i,
                    if tag == 7 {
                        PoolEntry::Class(index)
                    } else if tag == 8 {
                        PoolEntry::String(index)
                    } else {
                        PoolEntry::Numeric
                    },
                );
            }
            9 | 10 | 11 | 17 | 18 => {
                // Fieldref/Methodref/InterfaceMethodref/Dynamic/InvokeDynamic
                let a = be_u16(data, pos)?;
                let b = be_u16(data, pos + 2)?;
                pos += 4;
                pool.insert(
                    i,
                    match tag {
                        18 => PoolEntry::InvokeDynamic {
                            bootstrap_index: a,
                            nat_index: b,
                        },
                        17 => PoolEntry::Numeric,
                        _ => PoolEntry::Ref {
                            class_index: a,
                            nat_index: b,
                        },
                    },
                );
            }
            12 => {
                let name_idx = be_u16(data, pos)?;
                let desc_idx = be_u16(data, pos + 2)?;
                pos += 4;
                pool.insert(i, PoolEntry::NameAndType { name_idx, desc_idx });
            }
            15 => {
                let kind = *data.get(pos).ok_or(ClassFileError::Truncated)?;
                let ref_index = be_u16(data, pos + 1)?;
                pos += 3;
                pool.insert(i, PoolEntry::MethodHandle { kind, ref_index });
            }
            other => return Err(ClassFileError::UnknownTag(other)),
        }
        i += 1;
    }

    let utf8 = |idx: u16| -> String {
        match pool.get(&(idx as usize)) {
            Some(PoolEntry::Utf8(s)) => s.clone(),
            _ => String::new(),
        }
    };
    let class_name = |idx: u16| -> String {
        match pool.get(&(idx as usize)) {
            Some(PoolEntry::Class(index)) => utf8(*index).replace('/', "."),
            _ => String::new(),
        }
    };
    let nat_name = |nat_idx: u16| -> String {
        match pool.get(&(nat_idx as usize)) {
            Some(PoolEntry::NameAndType { name_idx, .. }) => utf8(*name_idx),
            _ => String::new(),
        }
    };
    let method_handle = |idx: u16| -> Option<MethodHandleInfo> {
        match pool.get(&(idx as usize)) {
            Some(PoolEntry::MethodHandle { kind, ref_index }) => {
                let mut mh = MethodHandleInfo {
                    kind: *kind,
                    ..Default::default()
                };
                if let Some(PoolEntry::Ref { class_index, nat_index }) =
                    pool.get(&(*ref_index as usize))
                {
                    mh.ref_class = class_name(*class_index);
                    mh.ref_name = nat_name(*nat_index);
                }
                Some(mh)
            }
            _ => None,
        }
    };

    // ---- Access flags + this/super/interfaces ----
    pos += 2; // access flags
    let this_idx = be_u16(data, pos)?;
    let super_idx = be_u16(data, pos + 2)?;
    pos += 4; // this(2) + super(2)
    info.name = class_name(this_idx);
    info.super_name = class_name(super_idx);
    let if_count = be_u16(data, pos)? as usize;
    pos += 2;
    for _ in 0..if_count {
        info.interfaces.push(class_name(be_u16(data, pos)?));
        pos += 2;
    }

    // ---- Collect strings & refs từ pool ----
    let mut invoke_refs_by_index: std::collections::BTreeMap<usize, String> = Default::default();
    for (idx, entry) in &pool {
        match entry {
            PoolEntry::Utf8(s) => {
                if s.len() >= 4 {
                    info.strings.push(s.clone());
                }
            }
            PoolEntry::String(index) => {
                let s = utf8(*index);
                if !s.is_empty() {
                    info.strings.push(s);
                }
            }
            PoolEntry::Ref { class_index, nat_index } => {
                let cls = class_name(*class_index);
                let name = nat_name(*nat_index);
                let r#ref = format!("{cls}.{name}");
                invoke_refs_by_index.insert(*idx, r#ref.clone());
                info.method_refs.push(r#ref);
            }
            _ => {}
        }
    }
    // Class refs (phải duyệt riêng — trong vòng trên mượn &pool 2 lần sẽ clash)
    for entry in pool.values() {
        if let PoolEntry::Class(index) = entry {
            let name = utf8(*index);
            if !name.is_empty() {
                info.class_refs.push(name.replace('/', "."));
            }
        }
    }

    // ---- Fields & methods (parse Code attribute) ----
    pos = skip_members(data, pos, false, &invoke_refs_by_index, &utf8, &mut info)?;
    pos = skip_members(data, pos, true, &invoke_refs_by_index, &utf8, &mut info)?;

    // ---- Class attributes (BootstrapMethods) ----
    let attrs = be_u16(data, pos)? as usize;
    pos += 2;
    for _ in 0..attrs {
        let a_name_idx = be_u16(data, pos)?;
        let a_name = utf8(a_name_idx);
        pos += 2;
        let alen = be_u32(data, pos)? as usize;
        pos += 4;
        if a_name == "BootstrapMethods" {
            let body = data.get(pos..pos + alen).ok_or(ClassFileError::Truncated)?;
            let mut bpos = 0usize;
            let num = be_u16(body, 0)? as usize;
            bpos += 2;
            for _ in 0..num {
                let mh_idx = be_u16(body, bpos)?;
                let arg_count = be_u16(body, bpos + 2)? as usize;
                bpos += 4;
                let handle = method_handle(mh_idx);
                if let Some(h) = &handle {
                    if !h.ref_class.is_empty() {
                        info.bootstrap_classes.insert(h.ref_class.clone());
                    }
                }
                let mut args_classes = Vec::new();
                for _a in 0..arg_count {
                    let arg_idx = be_u16(body, bpos)?;
                    bpos += 2;
                    match pool.get(&(arg_idx as usize)) {
                        Some(PoolEntry::MethodHandle { .. }) => {
                            if let Some(mh) = method_handle(arg_idx) {
                                if !mh.ref_class.is_empty() {
                                    args_classes.push(mh.ref_class);
                                }
                            }
                        }
                        Some(PoolEntry::Class(index)) => {
                            args_classes.push(class_name(*index));
                        }
                        _ => {}
                    }
                }
                info.bootstrap_methods.push(BootstrapMethod {
                    handle,
                    args_classes,
                });
            }
        }
        pos += alen;
    }

    // ---- Reflection flags ----
    let ref_set: BTreeSet<&String> = info.method_refs.iter().collect();
    info.uses_reflection = ref_set.contains(&"java.lang.Class.forName".to_string())
        && ref_set.contains(&"java.lang.reflect.Method.invoke".to_string());
    info.uses_method_handles = info
        .class_refs
        .iter()
        .any(|c| c.starts_with("java.lang.invoke.MethodHandles"));
    Ok(info)
}

/// Duyệt fields (parse_code=false) / methods (parse_code=true) — parity
/// `skip_or_parse_members`. `lookup_utf8` là closure truy cập constant pool của
/// decode_class (name_idx → string).
fn skip_members(
    data: &[u8],
    mut pos: usize,
    parse_code: bool,
    invoke_refs: &std::collections::BTreeMap<usize, String>,
    lookup_utf8: &impl Fn(u16) -> String,
    info: &mut ClassInfo,
) -> Result<usize, ClassFileError> {
    let count = be_u16(data, pos)? as usize;
    pos += 2;
    for _ in 0..count {
        pos += 6; // access, name_idx, desc_idx
        let attrs = be_u16(data, pos)? as usize;
        pos += 2;
        for _ in 0..attrs {
            let a_name_idx = be_u16(data, pos)?;
            let a_name = lookup_utf8(a_name_idx);
            pos += 2;
            let alen = be_u32(data, pos)? as usize;
            pos += 4;
            if parse_code && a_name == "Code" && alen >= 8 {
                let body = data.get(pos..pos + alen).ok_or(ClassFileError::Truncated)?;
                parse_code_attribute(info, body, invoke_refs);
            }
            pos += alen;
        }
    }
    Ok(pos)
}

/// Opcode → số byte operand (parity `_FIXED_LENGTHS`).
fn fixed_length(op: u8) -> usize {
    match op {
        0x10 => 1,                       // bipush
        0x11 => 2,                       // sipush
        0x12 => 1,                       // ldc
        0x13 | 0x14 => 2,                // ldc_w, ldc2_w
        0x15..=0x19 => 1,                // loads
        0x36..=0x3A => 1,                // stores
        0x84 => 1,                       // iinc
        0x99..=0xA8 => 2,                // if*, goto, jsr
        0xB2..=0xB8 => 2,                // get/putstatic/field, invoke*
        0xB9 | 0xBA => 4,                // invokeinterface/invokedynamic
        0xBB => 2,                       // new
        0xBC => 1,                       // newarray
        0xBD => 2,                       // anewarray
        0xC0 | 0xC1 => 2,                // checkcast, instanceof
        0xC5 => 3,                       // multianewarray
        0xC6 | 0xC7 => 2,                // ifnull, ifnonnull
        0xC8 | 0xC9 => 4,                // goto_w, jsr_w
        // Reserved/quick/impdep + mọi opcode còn lại → 0 (parity setdefault 0)
        _ => 0,
    }
}

/// Parse Code attribute: walk bytecode thu callee classes + invokedynamic.
fn parse_code_attribute(
    info: &mut ClassInfo,
    body: &[u8],
    invoke_refs: &std::collections::BTreeMap<usize, String>,
) {
    if body.len() < 8 {
        return;
    }
    let code_len = match be_u32(body, 4) {
        Ok(v) => v as usize,
        Err(_) => return,
    };
    if body.len() < 8 + code_len {
        return;
    }
    let code = &body[8..8 + code_len];
    let mut pos = 0usize;
    let n = code.len();
    while pos < n {
        let op = code[pos];
        pos += 1;
        match op {
            0xBA => {
                // invokedynamic: chỉ đếm (skip 4 bytes: idx + zero attr)
                if pos + 4 <= n {
                    info.invokedynamic_count += 1;
                }
                pos += 4;
            }
            0xB6 | 0xB7 | 0xB8 | 0x77 => {
                // invokevirtual/special/static + old invokespecial
                if pos + 2 <= n {
                    let cp_idx = u16::from_be_bytes([code[pos], code[pos + 1]]) as usize;
                    if let Some(r#ref) = invoke_refs.get(&cp_idx) {
                        if let Some(callee) = r#ref.rsplit_once('.') {
                            info.callees.insert(callee.0.to_string());
                        }
                    }
                }
                pos += 2;
            }
            0xB9 => {
                // invokeinterface
                if pos + 4 <= n {
                    let cp_idx = u16::from_be_bytes([code[pos], code[pos + 1]]) as usize;
                    if let Some(r#ref) = invoke_refs.get(&cp_idx) {
                        if let Some(callee) = r#ref.rsplit_once('.') {
                            info.callees.insert(callee.0.to_string());
                        }
                    }
                }
                pos += 4;
            }
            0xAA => {
                // tableswitch: pad + default + low + high + offsets
                let pad = (4 - (pos % 4)) % 4;
                pos += pad;
                if pos + 12 <= n {
                    let low = i32::from_be_bytes([
                        code[pos + 4],
                        code[pos + 5],
                        code[pos + 6],
                        code[pos + 7],
                    ]);
                    let high = i32::from_be_bytes([
                        code[pos + 8],
                        code[pos + 9],
                        code[pos + 10],
                        code[pos + 11],
                    ]);
                    let count = (high - low + 1).max(0) as usize;
                    pos += 12 + count * 4;
                }
            }
            0xAB => {
                // lookupswitch: pad + default + npairs + pairs
                let pad = (4 - (pos % 4)) % 4;
                pos += pad;
                if pos + 8 <= n {
                    let npairs = i32::from_be_bytes([
                        code[pos + 4],
                        code[pos + 5],
                        code[pos + 6],
                        code[pos + 7],
                    ])
                    .max(0) as usize;
                    pos += 8 + npairs * 8;
                }
            }
            0xC4 => {
                // wide: next opcode + 2 bytes; iinc: +4
                if pos < n {
                    let next_op = code[pos];
                    pos += 2 + if next_op == 0x84 { 2 } else { 0 };
                }
            }
            other => {
                pos += fixed_length(other);
            }
        }
    }
}

/// Decode tất cả .class trong jar (max 500, skip module-info.class) — parity
/// `scan_jar_classes`. Class lỗi bị bỏ qua (continue).
pub fn scan_jar_classes(jar_bytes: &[u8], max_classes: usize) -> Vec<ClassInfo> {
    let mut infos = Vec::new();
    let Ok(entries) = zip_entries(jar_bytes) else {
        return infos;
    };
    for entry in entries
        .iter()
        .filter(|e| e.name.ends_with(".class") && !e.name.ends_with("module-info.class"))
        .take(max_classes)
    {
        if let Ok(bytes) = zip_read_entry(jar_bytes, &entry.name) {
            if let Ok(info) = decode_class(&bytes) {
                infos.push(info);
            }
        }
    }
    infos
}

#[derive(Debug, Clone, Default)]
pub struct JarStructure {
    pub entries: usize,
    pub classes: usize,
    pub configs: Vec<String>,
    pub suspicious_files: Vec<String>,
    pub nested_jars: Vec<String>,
    pub fabric_mod_json: Option<String>,
    pub forge_toml: Option<String>,
}

/// Tổng quan jar — parity `jar_structure` (file trông khả nghi, nested jar,
/// config json/toml root-level, fabric/mods.toml content).
pub fn jar_structure(jar_bytes: &[u8]) -> Result<JarStructure, ClassFileError> {
    let entries = zip_entries(jar_bytes).map_err(|err| match err {
        ZipError::NotZip | ZipError::NoEocd => ClassFileError::CorruptJar(err.to_string()),
        other => ClassFileError::CorruptJar(other.to_string()),
    })?;
    let mut result = JarStructure {
        entries: entries.len(),
        ..Default::default()
    };
    const SUSPICIOUS_EXT: [&str; 8] = [
        ".exe", ".dll", ".so", ".bat", ".cmd", ".sh", ".ps1", ".vbs",
    ];
    for entry in &entries {
        let name = &entry.name;
        let low = name.to_lowercase();
        if low.ends_with(".class") {
            result.classes += 1;
        } else if (low.ends_with(".json") || low.ends_with(".toml"))
            && !low.trim_end_matches('/').contains('/')
        {
            // root-level config — đọc tối đa 20000 ký tự (parity [:20000])
            if let Ok(content) = zip_read_entry(jar_bytes, name) {
                let text = String::from_utf8_lossy(&content);
                let text = &text[..text.len().min(20_000)];
                if low.contains("fabric") {
                    result.fabric_mod_json = Some(text.to_string());
                } else if low.contains("mods") && low.ends_with(".toml") {
                    result.forge_toml = Some(text.to_string());
                } else {
                    result.configs.push(name.clone());
                }
            }
        } else if low.ends_with(".jar") {
            result.nested_jars.push(name.clone());
        } else if SUSPICIOUS_EXT.iter().any(|ext| low.ends_with(ext)) {
            result.suspicious_files.push(name.clone());
        }
    }
    Ok(result)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::zipread::zip_read_entry;

    /// Builder jar stored nhiều entry (dùng chung kiểu health.rs — tái tạo local).
    pub fn build_stored_zip(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut offsets: Vec<(String, u32, u32, u32, u32)> = Vec::new();
        for (name, content) in entries {
            let offset = out.len() as u32;
            let crc = crc32(content);
            out.extend_from_slice(b"PK\x03\x04");
            out.extend_from_slice(&14u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // method 0
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&(content.len() as u32).to_le_bytes());
            out.extend_from_slice(&(content.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(content);
            offsets.push((
                name.to_string(),
                crc,
                content.len() as u32,
                content.len() as u32,
                offset,
            ));
        }
        let cd_offset = out.len() as u32;
        for (name, crc, csize, usize_, offset) in &offsets {
            out.extend_from_slice(b"PK\x01\x02");
            out.extend_from_slice(&14u16.to_le_bytes());
            out.extend_from_slice(&14u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&csize.to_le_bytes());
            out.extend_from_slice(&usize_.to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&[0u8; 12]); // extra/comment/disk/iattr/eattr
            out.extend_from_slice(&offset.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
        }
        let cd_size = out.len() as u32 - cd_offset;
        out.extend_from_slice(b"PK\x05\x06");
        out.extend_from_slice(&[0u8; 4]);
        out.extend_from_slice(&(offsets.len() as u16).to_le_bytes());
        out.extend_from_slice(&(offsets.len() as u16).to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    pub fn crc32(data: &[u8]) -> u32 {
        let mut table = [0u32; 256];
        for (i, item) in table.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB88320 ^ (c >> 1) } else { c >> 1 };
            }
            *item = c;
        }
        let mut crc = 0xFFFFFFFFu32;
        for &b in data {
            crc = table[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
        }
        crc ^ 0xFFFFFFFF
    }

    /// Sinh 1 class file thật bằng Python `javac` không có trong CI — thay bằng
    /// golden class bytes dựng thủ công: class Java 8 tối giản có method gọi
    /// Runtime.exec qua constant pool. Dùng vector hex sinh từ Python script
    /// (tests/helpers/make_class.py) — ở đây dựng thủ công một class tối giản
    /// đủ tag để decode không lỗi và thấy method_refs.
    fn minimal_class_bytes() -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&MAGIC.to_be_bytes());
        b.extend_from_slice(&0u16.to_be_bytes()); // minor
        b.extend_from_slice(&52u16.to_be_bytes()); // major 52 = Java 8
        // constant pool count = 8 (7 entries thực: 1..6 + slot 7 rỗng cuối)
        b.extend_from_slice(&8u16.to_be_bytes());
        // #1 Utf8 "evil/Class"
        b.push(1);
        b.extend_from_slice(&10u16.to_be_bytes());
        b.extend_from_slice(b"evil/Class");
        // #2 Class -> #1
        b.push(7);
        b.extend_from_slice(&1u16.to_be_bytes());
        // #3 Utf8 "java/lang/Object"
        b.push(1);
        b.extend_from_slice(&16u16.to_be_bytes());
        b.extend_from_slice(b"java/lang/Object");
        // #4 Class -> #3
        b.push(7);
        b.extend_from_slice(&3u16.to_be_bytes());
        // #5 Utf8 "exec" (dùng cho NameAndType)
        b.push(1);
        b.extend_from_slice(&4u16.to_be_bytes());
        b.extend_from_slice(b"exec");
        // #6 Utf8 "()V"
        b.push(1);
        b.extend_from_slice(&3u16.to_be_bytes());
        b.extend_from_slice(b"()V");
        // pool kết thúc ở #6 (count 8 → i chạy 1..7 → 6 entry + long? không)
        // Sửa: count 7 cho 6 entry thực
        let cp_end = b.len();
        // Đặt lại count = 7
        b[8..10].copy_from_slice(&7u16.to_be_bytes());
        let _ = cp_end;

        // access flags, this=#2, super=#4
        b.extend_from_slice(&0x0021u16.to_be_bytes());
        b.extend_from_slice(&2u16.to_be_bytes());
        b.extend_from_slice(&4u16.to_be_bytes());
        // interfaces = 0
        b.extend_from_slice(&0u16.to_be_bytes());
        // fields = 0
        b.extend_from_slice(&0u16.to_be_bytes());
        // methods = 0
        b.extend_from_slice(&0u16.to_be_bytes());
        // class attributes = 0
        b.extend_from_slice(&0u16.to_be_bytes());
        b
    }

    #[test]
    fn decode_minimal_class() {
        let info = decode_class(&minimal_class_bytes()).unwrap();
        assert_eq!(info.name, "evil.Class");
        assert_eq!(info.super_name, "java.lang.Object");
        assert_eq!(info.version_major, 52);
        assert_eq!(info.java_version(), 8);
        assert!(info.method_refs.is_empty()); // class tối giản không method
    }

    #[test]
    fn decode_rejects_bad_magic() {
        assert_eq!(decode_class(b"not a class").unwrap_err(), ClassFileError::BadMagic);
        assert_eq!(decode_class(&[]).unwrap_err(), ClassFileError::BadMagic);
    }

    #[test]
    fn jar_structure_detects_suspicious_and_nested() {
        let jar = build_stored_zip(&[
            ("com/x/Main.class", vec![0xCA, 0xFE, 0xBA, 0xBE]),
            ("payload.exe", b"MZ binary".to_vec()),
            ("libs/inner.jar", vec![0x50, 0x4B, 0x03, 0x04]),
            ("assets/data.bin", vec![0, 1, 2]),
        ]);
        let s = jar_structure(&jar).unwrap();
        assert_eq!(s.entries, 4);
        assert_eq!(s.classes, 1);
        assert_eq!(s.suspicious_files, vec!["payload.exe"]);
        assert_eq!(s.nested_jars, vec!["libs/inner.jar"]);
        assert!(s.fabric_mod_json.is_none());
        // jar không phải zip → corrupt
        assert!(jar_structure(b"nope").is_err());
    }

    #[test]
    fn scan_jar_classes_skips_broken_and_module_info() {
        let good = minimal_class_bytes();
        let jar = build_stored_zip(&[
            ("module-info.class", vec![0xCA, 0xFE, 0xBA, 0xBE]),
            ("com/x/Main.class", good.clone()),
            ("com/x/Bad.class", b"junk".to_vec()),
        ]);
        let infos = scan_jar_classes(&jar, 500);
        assert_eq!(infos.len(), 1, "module-info skip + broken skip");
        assert_eq!(infos[0].name, "evil.Class");
    }

    #[test]
    fn zipread_entry_helper_integration() {
        let jar = build_stored_zip(&[("fabric.mod.json", br#"{"id":"x"}"#.to_vec())]);
        let content = zip_read_entry(&jar, "fabric.mod.json").unwrap();
        assert_eq!(content, br#"{"id":"x"}"#);
    }
}


