//! ZIP reader — đọc entry từ jar thuần Rust (zero-dep): parse End of Central
//! Directory + Central Directory entries, đọc data theo method:
//! - 0 (stored): copy nguyên
//! - 8 (deflate): giải nén qua `deflate::inflate` (cap max_entry — zip-bomb)
//!
//! Đủ cho mod metadata (`fabric.mod.json`, `mods.toml`, `mcmod.info`) — KHÔNG
//! thay thế zipfile đầy đủ (không zip64, không mã hoá — không cần cho metadata).

use crate::deflate::inflate;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ZipError {
    #[error("not a zip archive")]
    NotZip,
    #[error("end of central directory not found")]
    NoEocd,
    #[error("entry not found: {0}")]
    EntryNotFound(String),
    #[error("unsupported compression method: {0}")]
    UnsupportedMethod(u16),
    #[error("inflate: {0}")]
    Inflate(String),
    #[error("entry too large (max {max}): {name}")]
    EntryTooLarge { max: u64, name: String },
    #[error("io: {0}")]
    Io(String),
}

impl ZipError {
    /// Code taxonomy §117 — parity `zipfile.BadZipFile` → FILE_NOT_FOUND/_INVALID.
    pub fn code(&self) -> &'static str {
        match self {
            ZipError::NotZip | ZipError::NoEocd | ZipError::EntryNotFound(_) => "FILE_NOT_FOUND",
            ZipError::UnsupportedMethod(_) | ZipError::EntryTooLarge { .. } => "CONFIG_INVALID",
            ZipError::Inflate(_) | ZipError::Io(_) => "APP_INTERNAL",
        }
    }
}

/// Một entry trong central directory.
#[derive(Debug, Clone)]
pub struct ZipEntry {
    pub name: String,
    pub method: u16,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub local_header_offset: u64,
}

pub const DEFAULT_MAX_ENTRY: u64 = 64 * 1024 * 1024; // 64MB — metadata không lớn hơn

/// Parse central directory từ toàn bộ byte jar (mảnh EOCD quét từ cuối).
pub fn zip_entries(data: &[u8]) -> Result<Vec<ZipEntry>, ZipError> {
    if data.len() < 4 || &data[..4] != b"PK\x03\x04" {
        return Err(ZipError::NotZip);
    }
    let eocd = find_eocd(data).ok_or(ZipError::NoEocd)?;
    let count = u16::from_le_bytes([data[eocd + 10], data[eocd + 11]]) as usize;
    let cd_offset = u32::from_le_bytes([
        data[eocd + 16],
        data[eocd + 17],
        data[eocd + 18],
        data[eocd + 19],
    ]) as usize;

    let mut entries = Vec::with_capacity(count);
    let mut pos = cd_offset;
    for _ in 0..count {
        if pos + 46 > data.len() || &data[pos..pos + 4] != b"PK\x01\x02" {
            break;
        }
        let method = u16::from_le_bytes([data[pos + 10], data[pos + 11]]);
        let compressed_size = u32::from_le_bytes([
            data[pos + 20],
            data[pos + 21],
            data[pos + 22],
            data[pos + 23],
        ]) as u64;
        let uncompressed_size = u32::from_le_bytes([
            data[pos + 24],
            data[pos + 25],
            data[pos + 26],
            data[pos + 27],
        ]) as u64;
        let name_len = u16::from_le_bytes([data[pos + 28], data[pos + 29]]) as usize;
        let extra_len = u16::from_le_bytes([data[pos + 30], data[pos + 31]]) as usize;
        let comment_len = u16::from_le_bytes([data[pos + 32], data[pos + 33]]) as usize;
        let local_offset = u32::from_le_bytes([
            data[pos + 42],
            data[pos + 43],
            data[pos + 44],
            data[pos + 45],
        ]) as u64;
        let name_start = pos + 46;
        let name_end = name_start + name_len;
        if name_end > data.len() {
            return Err(ZipError::Io("central directory truncated".into()));
        }
        let name = String::from_utf8_lossy(&data[name_start..name_end]).into_owned();
        entries.push(ZipEntry {
            name,
            method,
            compressed_size,
            uncompressed_size,
            local_header_offset: local_offset,
        });
        pos = name_end + extra_len + comment_len;
    }
    Ok(entries)
}

/// Tìm EOCD signature `PK\x05\x06` — quét từ cuối về (comment ≤ 64KB).
fn find_eocd(data: &[u8]) -> Option<usize> {
    if data.len() < 22 {
        return None;
    }
    let start = data.len().saturating_sub(22 + 65_536);
    let mut i = data.len() - 22;
    while i >= start {
        if &data[i..i + 4] == b"PK\x05\x06" {
            return Some(i);
        }
        i -= 1;
        if i == 0 {
            break;
        }
    }
    None
}

/// Đọc 1 entry theo tên — method 0/8; cap `max_entry` (zip-bomb, parity MAX_BODY).
pub fn zip_read_entry(data: &[u8], name: &str) -> Result<Vec<u8>, ZipError> {
    zip_read_entry_with_limit(data, name, DEFAULT_MAX_ENTRY)
}

pub fn zip_read_entry_with_limit(
    data: &[u8],
    name: &str,
    max_entry: u64,
) -> Result<Vec<u8>, ZipError> {
    let entries = zip_entries(data)?;
    let entry = entries
        .iter()
        .find(|e| e.name == name)
        .ok_or_else(|| ZipError::EntryNotFound(name.to_string()))?;
    if entry.uncompressed_size > max_entry {
        return Err(ZipError::EntryTooLarge {
            max: max_entry,
            name: name.to_string(),
        });
    }
    let local = entry.local_header_offset as usize;
    if local + 30 > data.len() || &data[local..local + 4] != b"PK\x03\x04" {
        return Err(ZipError::Io("local header missing".into()));
    }
    let name_len = u16::from_le_bytes([data[local + 26], data[local + 27]]) as usize;
    let extra_len = u16::from_le_bytes([data[local + 28], data[local + 29]]) as usize;
    let data_start = local + 30 + name_len + extra_len;
    let data_end = data_start + entry.compressed_size as usize;
    if data_end > data.len() {
        return Err(ZipError::Io("entry data truncated".into()));
    }
    let raw = &data[data_start..data_end];
    match entry.method {
        0 => Ok(raw.to_vec()),
        8 => inflate(raw, entry.uncompressed_size.max(1) as usize + 1)
            .map_err(|err| ZipError::Inflate(err.to_string())),
        other => Err(ZipError::UnsupportedMethod(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// Golden: ZIP stored sinh bằng Python zipfile (2 entry: fabric.mod.json +
    /// assets/x.png) — full bytes hex.
    const STORED_ZIP: &str = "504b030414000000000054793d5d4f25b2af47000000470000000f0000006661627269632e6d6f642e6a736f6e7b22736368656d6156657273696f6e223a312c226964223a22746573742d6d6f64222c2276657273696f6e223a22312e302e30222c226e616d65223a2254657374204d6f64227d504b030414000000000054793d5d60080beb06000000060000000c0000006173736574732f782e706e6789504e470d0a504b0102140314000000000054793d5d4f25b2af47000000470000000f00000000000000000000008001000000006661627269632e6d6f642e6a736f6e504b0102140314000000000054793d5d60080beb06000000060000000c00000000000000000000008001740000006173736574732f782e706e67504b0506000000000200020077000000a40000000000";

    #[test]
    fn parse_entries_golden_stored_zip() {
        let data = hex(STORED_ZIP);
        let entries = zip_entries(&data).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "fabric.mod.json");
        assert_eq!(entries[0].method, 0);
        assert_eq!(entries[0].uncompressed_size, 71);
        assert_eq!(entries[1].name, "assets/x.png");
        assert_eq!(entries[1].uncompressed_size, 6);
    }

    #[test]
    fn read_stored_entry_golden() {
        let data = hex(STORED_ZIP);
        let content = zip_read_entry(&data, "fabric.mod.json").unwrap();
        assert_eq!(
            String::from_utf8(content).unwrap(),
            r#"{"schemaVersion":1,"id":"test-mod","version":"1.0.0","name":"Test Mod"}"#
        );
        // entry không tồn tại
        let err = zip_read_entry(&data, "nope.json").unwrap_err();
        assert_eq!(err.code(), "FILE_NOT_FOUND");
        // file không phải zip
        assert_eq!(zip_read_entry(b"not a zip", "x").unwrap_err().code(), "FILE_NOT_FOUND");
    }

    #[test]
    fn read_deflated_entry_roundtrip() {
        // Tự dựng jar deflated: entry nén bằng deflate.rs golden vector (short).
        // Dựng thủ công: local header (method 8) + data + central dir + EOCD.
        let name = "fabric.mod.json";
        let payload = r#"{"schemaVersion":1,"id":"test-mod","version":"1.0.0","name":"Test Mod"}"#;
        let compressed = hex("ab562a4ece48cd4d0c4b2d2acecccf53b232d451ca4c51b2522a492d2ed1cdcd4f51d2512a83c92919ea19e8190045f212735381dc10a01a055fa09a5a00");

        let mut jar = Vec::new();
        let local_offset = 0usize;
        jar.extend_from_slice(b"PK\x03\x04");
        jar.extend_from_slice(&14u16.to_le_bytes()); // version
        jar.extend_from_slice(&0u16.to_le_bytes()); // flags
        jar.extend_from_slice(&8u16.to_le_bytes()); // method = deflate
        jar.extend_from_slice(&0u16.to_le_bytes()); // time
        jar.extend_from_slice(&0u16.to_le_bytes()); // date
        jar.extend_from_slice(&0u32.to_le_bytes()); // crc
        jar.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        jar.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        jar.extend_from_slice(&(name.len() as u16).to_le_bytes());
        jar.extend_from_slice(&0u16.to_le_bytes()); // extra len
        jar.extend_from_slice(name.as_bytes());
        jar.extend_from_slice(&compressed);

        let cd_offset = jar.len() as u32;
        jar.extend_from_slice(b"PK\x01\x02");
        jar.extend_from_slice(&14u16.to_le_bytes());
        jar.extend_from_slice(&14u16.to_le_bytes());
        jar.extend_from_slice(&0u16.to_le_bytes());
        jar.extend_from_slice(&8u16.to_le_bytes());
        jar.extend_from_slice(&0u16.to_le_bytes());
        jar.extend_from_slice(&0u16.to_le_bytes());
        jar.extend_from_slice(&0u16.to_le_bytes());
        jar.extend_from_slice(&0u32.to_le_bytes()); // crc
        jar.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        jar.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        jar.extend_from_slice(&(name.len() as u16).to_le_bytes());
        jar.extend_from_slice(&[0u8; 12]); // extra/comment/eattr len+iattr+offset... đơn giản
        jar.extend_from_slice(&0u32.to_le_bytes()); // local offset — đặt trong 12 byte trên? cần đúng offset 42
        // (test này parsecentral dir tại offset 42: ghi lại đúng layout)
        // Rebuild đúng: [0..4] sig, 4..6 ver made, 6..8 ver need, 8..10 flags,
        // 10..12 method, 12..14 time, 14..16 date, 16..20 crc, 20..24 csize,
        // 24..28 usize, 28..30 nlen, 30..32 elen, 32..34 clen, 34..36 disk,
        // 36..38 iattr, 38..42 eattr, 42..46 local offset, 46.. nlen
        jar.truncate(cd_offset as usize); // bỏ phần CD thử sai phía trên
        jar.extend_from_slice(b"PK\x01\x02");
        jar.extend_from_slice(&14u16.to_le_bytes()); // 4..6
        jar.extend_from_slice(&14u16.to_le_bytes()); // 6..8
        jar.extend_from_slice(&0u16.to_le_bytes()); // 8..10 flags
        jar.extend_from_slice(&8u16.to_le_bytes()); // 10..12 method
        jar.extend_from_slice(&0u16.to_le_bytes()); // 12..14 time
        jar.extend_from_slice(&0u16.to_le_bytes()); // 14..16 date
        jar.extend_from_slice(&0u32.to_le_bytes()); // 16..20 crc
        jar.extend_from_slice(&(compressed.len() as u32).to_le_bytes()); // 20..24
        jar.extend_from_slice(&(payload.len() as u32).to_le_bytes()); // 24..28
        jar.extend_from_slice(&(name.len() as u16).to_le_bytes()); // 28..30
        jar.extend_from_slice(&0u16.to_le_bytes()); // 30..32 extra
        jar.extend_from_slice(&0u16.to_le_bytes()); // 32..34 comment
        jar.extend_from_slice(&0u16.to_le_bytes()); // 34..36 disk
        jar.extend_from_slice(&0u16.to_le_bytes()); // 36..38 iattr
        jar.extend_from_slice(&0u32.to_le_bytes()); // 38..42 eattr
        jar.extend_from_slice(&(local_offset as u32).to_le_bytes()); // 42..46
        jar.extend_from_slice(name.as_bytes());

        jar.extend_from_slice(b"PK\x05\x06");
        jar.extend_from_slice(&[0u8; 4]); // disk numbers
        jar.extend_from_slice(&1u16.to_le_bytes()); // entries
        jar.extend_from_slice(&1u16.to_le_bytes());
        jar.extend_from_slice(&(jar.len() as u32 - cd_offset).to_le_bytes());
        jar.extend_from_slice(&cd_offset.to_le_bytes());
        jar.extend_from_slice(&0u16.to_le_bytes()); // comment len

        let content = zip_read_entry(&jar, name).unwrap();
        assert_eq!(String::from_utf8(content).unwrap(), payload);
    }

    #[test]
    fn entry_too_large_is_zip_bomb_guard() {
        let data = hex(STORED_ZIP);
        let err = zip_read_entry_with_limit(&data, "fabric.mod.json", 10).unwrap_err();
        assert!(matches!(err, ZipError::EntryTooLarge { max: 10, .. }));
        assert_eq!(err.code(), "CONFIG_INVALID");
    }
}
