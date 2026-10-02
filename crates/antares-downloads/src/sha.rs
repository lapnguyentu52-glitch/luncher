use std::path::Path;

use crate::{verify_checksum, ChecksumAlgorithm, ChecksumError};

// ---------------------------------------------------------------------------
// SHA-256 (FIPS 180-4) — incremental
// ---------------------------------------------------------------------------

pub struct Sha256Hasher {
    h: [u32; 8],
    buffer: [u8; 64],
    buffered: usize,
    len: u64,
}

impl Default for Sha256Hasher {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha256Hasher {
    pub fn new() -> Self {
        Self {
            h: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buffer: [0u8; 64],
            buffered: 0,
            len: 0,
        }
    }

    pub fn update(&mut self, mut data: &[u8]) {
        self.len = self.len.wrapping_add(data.len() as u64);
        if self.buffered > 0 {
            let take = (64 - self.buffered).min(data.len());
            self.buffer[self.buffered..self.buffered + take].copy_from_slice(&data[..take]);
            self.buffered += take;
            data = &data[take..];
            if self.buffered == 64 {
                let block = self.buffer;
                self.compress(&block);
                self.buffered = 0;
                self.buffer = [0u8; 64];
            }
        }
        while data.len() >= 64 {
            let (block, rest) = data.split_at(64);
            let mut block_arr = [0u8; 64];
            block_arr.copy_from_slice(block);
            self.compress(&block_arr);
            data = rest;
        }
        if !data.is_empty() {
            self.buffer[..data.len()].copy_from_slice(data);
            self.buffered = data.len();
        }
    }

    pub fn finish(mut self) -> [u8; 32] {
        let bit_len = self.len.wrapping_mul(8);
        self.update_raw(&[0x80]);
        while self.buffered != 56 {
            self.update_raw(&[0]);
        }
        self.update_raw(&bit_len.to_be_bytes());
        let mut out = [0u8; 32];
        for (i, word) in self.h.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        out
    }

    /// update không đếm len (padding nội bộ).
    fn update_raw(&mut self, data: &[u8]) {
        for &byte in data {
            self.buffer[self.buffered] = byte;
            self.buffered += 1;
            if self.buffered == 64 {
                let block = self.buffer;
                self.compress(&block);
                self.buffered = 0;
            }
        }
    }

    fn compress(&mut self, block: &[u8; 64]) {
        const SHA256_K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
            0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
            0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
            0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
            0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
            0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
            0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
            0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
            0xc67178f2,
        ];
        let mut w = [0u32; 64];
        for (i, word) in w.iter_mut().enumerate().take(16) {
            *word = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (self.h[0], self.h[1], self.h[2], self.h[3], self.h[4], self.h[5], self.h[6], self.h[7]);

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(SHA256_K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        self.h[0] = self.h[0].wrapping_add(a);
        self.h[1] = self.h[1].wrapping_add(b);
        self.h[2] = self.h[2].wrapping_add(c);
        self.h[3] = self.h[3].wrapping_add(d);
        self.h[4] = self.h[4].wrapping_add(e);
        self.h[5] = self.h[5].wrapping_add(f);
        self.h[6] = self.h[6].wrapping_add(g);
        self.h[7] = self.h[7].wrapping_add(hh);
    }
}

// ---------------------------------------------------------------------------
// SHA-1 (FIPS 180-4) — incremental
// ---------------------------------------------------------------------------

pub struct Sha1Hasher {
    h: [u32; 5],
    buffer: [u8; 64],
    buffered: usize,
    len: u64,
}

impl Default for Sha1Hasher {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha1Hasher {
    pub fn new() -> Self {
        Self {
            h: [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476, 0xc3d2e1f0],
            buffer: [0u8; 64],
            buffered: 0,
            len: 0,
        }
    }

    pub fn update(&mut self, mut data: &[u8]) {
        self.len = self.len.wrapping_add(data.len() as u64);
        if self.buffered > 0 {
            let take = (64 - self.buffered).min(data.len());
            self.buffer[self.buffered..self.buffered + take].copy_from_slice(&data[..take]);
            self.buffered += take;
            data = &data[take..];
            if self.buffered == 64 {
                let block = self.buffer;
                self.compress(&block);
                self.buffered = 0;
                self.buffer = [0u8; 64];
            }
        }
        while data.len() >= 64 {
            let (block, rest) = data.split_at(64);
            let mut block_arr = [0u8; 64];
            block_arr.copy_from_slice(block);
            self.compress(&block_arr);
            data = rest;
        }
        if !data.is_empty() {
            self.buffer[..data.len()].copy_from_slice(data);
            self.buffered = data.len();
        }
    }

    pub fn finish(mut self) -> [u8; 20] {
        let bit_len = self.len.wrapping_mul(8);
        self.update_raw(&[0x80]);
        while self.buffered != 56 {
            self.update_raw(&[0]);
        }
        self.update_raw(&bit_len.to_be_bytes());
        let mut out = [0u8; 20];
        for (i, word) in self.h.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        out
    }

    fn update_raw(&mut self, data: &[u8]) {
        for &byte in data {
            self.buffer[self.buffered] = byte;
            self.buffered += 1;
            if self.buffered == 64 {
                let block = self.buffer;
                self.compress(&block);
                self.buffered = 0;
            }
        }
    }

    fn compress(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 80];
        for (i, word) in w.iter_mut().enumerate().take(16) {
            *word = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let (mut a, mut b, mut c, mut d, mut e) = (self.h[0], self.h[1], self.h[2], self.h[3], self.h[4]);

        for (i, &word) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5a82_7999u32),
                20..=39 => (b ^ c ^ d, 0x6ed9_eba1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1b_bcdc),
                _ => (b ^ c ^ d, 0xca62_c1d6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        self.h[0] = self.h[0].wrapping_add(a);
        self.h[1] = self.h[1].wrapping_add(b);
        self.h[2] = self.h[2].wrapping_add(c);
        self.h[3] = self.h[3].wrapping_add(d);
        self.h[4] = self.h[4].wrapping_add(e);
    }
}

// ---------------------------------------------------------------------------
// Hex helpers + buffer API (giữ tương thích phase 2/3)
// ---------------------------------------------------------------------------

pub fn sha256_digest(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256Hasher::new();
    hasher.update(data);
    hasher.finish()
}

pub fn sha256_hex(data: &[u8]) -> String {
    to_hex(&sha256_digest(data))
}

pub fn sha1_digest(data: &[u8]) -> [u8; 20] {
    let mut hasher = Sha1Hasher::new();
    hasher.update(data);
    hasher.finish()
}

pub fn sha1_hex(data: &[u8]) -> String {
    to_hex(&sha1_digest(data))
}

fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

// ---------------------------------------------------------------------------
// Streaming verify — verify file lớn từng khối, không load toàn bộ vào RAM
// ---------------------------------------------------------------------------

const VERIFY_BUF: usize = 256 * 1024;

/// Verify file streaming: đọc 256KB/lần, hash incremental (verify_file buffer-mode
/// giờ bọc hàm này — artifact lớn không còn load cả file vào RAM).
pub fn verify_file_streaming(
    algorithm: ChecksumAlgorithm,
    path: &Path,
    expected: &str,
) -> Result<(), ChecksumError> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|_| ChecksumError::FileUnreadable {
        path: path.display().to_string(),
    })?;
    let actual_hex = match algorithm {
        ChecksumAlgorithm::Sha1 => {
            let mut hasher = Sha1Hasher::new();
            let mut buf = vec![0u8; VERIFY_BUF];
            loop {
                let n = file.read(&mut buf).map_err(|_| ChecksumError::FileUnreadable {
                    path: path.display().to_string(),
                })?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
            }
            to_hex(&hasher.finish())
        }
        ChecksumAlgorithm::Sha256 => {
            let mut hasher = Sha256Hasher::new();
            let mut buf = vec![0u8; VERIFY_BUF];
            loop {
                let n = file.read(&mut buf).map_err(|_| ChecksumError::FileUnreadable {
                    path: path.display().to_string(),
                })?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
            }
            to_hex(&hasher.finish())
        }
    };
    verify_checksum(algorithm, expected, &actual_hex)
}

/// Verify file (buffer API giữ cho caller cũ — bọc streaming bên dưới).
pub fn verify_file(
    algorithm: ChecksumAlgorithm,
    path: &Path,
    expected: &str,
) -> Result<(), ChecksumError> {
    verify_file_streaming(algorithm, path, expected)
}

/// §103 COMMIT — atomic rename tmp → target (cùng filesystem), tạo parent trước.
/// Trả `Ok(false)` nếu target đã tồn tại (idempotent commit — shared artifact).
pub fn commit_artifact(tmp: &Path, target: &Path) -> std::io::Result<bool> {
    if target.exists() {
        let _ = std::fs::remove_file(tmp);
        return Ok(false);
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(tmp, target)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_known_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        assert_eq!(
            sha256_hex(b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu"),
            "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1"
        );
    }

    #[test]
    fn sha1_known_vectors() {
        assert_eq!(sha1_hex(b""), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(
            sha1_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
        );
    }

    #[test]
    fn incremental_matches_buffer_api() {
        // Feed nhiều size khác nhau (span block boundary) — kết quả phải bằng buffer API.
        let data: Vec<u8> = (0u8..=255).cycle().take(1000).collect();
        let expected_256 = sha256_hex(&data);
        let expected_1 = sha1_hex(&data);
        for chunk in [1usize, 7, 63, 64, 65, 128, 333, 1000] {
            let mut h256 = Sha256Hasher::new();
            let mut h1 = Sha1Hasher::new();
            for piece in data.chunks(chunk) {
                h256.update(piece);
                h1.update(piece);
            }
            assert_eq!(to_hex(&h256.finish()), expected_256, "sha256 chunk {chunk}");
            assert_eq!(to_hex(&h1.finish()), expected_1, "sha1 chunk {chunk}");
        }
    }

    #[test]
    fn verify_file_roundtrip_and_mismatch() {
        let root = std::env::temp_dir().join(format!("antares-dl-sha-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("artifact.jar");
        std::fs::write(&file, b"abc").unwrap();

        let digest = sha256_hex(b"abc");
        verify_file(ChecksumAlgorithm::Sha256, &file, &digest).expect("digest khớp");
        let err = verify_file(ChecksumAlgorithm::Sha256, &file, &"a".repeat(64)).unwrap_err();
        assert_eq!(err.code(), "CHECKSUM_MISMATCH");
        let missing = verify_file(ChecksumAlgorithm::Sha256, &root.join("nope"), &digest)
            .unwrap_err();
        assert_eq!(missing.code(), "FILE_UNREADABLE");

        // streaming API trả cùng kết quả
        verify_file_streaming(ChecksumAlgorithm::Sha256, &file, &digest).expect("streaming ok");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn commit_artifact_idempotent() {
        let root = std::env::temp_dir().join(format!("antares-dl-commit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let tmp = root.join("artifact.jar.tmp");
        let target = root.join("store/artifact.jar");
        std::fs::write(&tmp, b"data").unwrap();
        assert!(commit_artifact(&tmp, &target).unwrap());
        assert!(target.is_file());
        assert!(!tmp.exists());

        // Commit lần 2: target đã có → false + tmp bị dọn (không ghi đè immutable).
        std::fs::write(&tmp, b"other").unwrap();
        assert!(!commit_artifact(&tmp, &target).unwrap());
        assert!(!tmp.exists());
        assert_eq!(std::fs::read(&target).unwrap(), b"data");

        let _ = std::fs::remove_dir_all(&root);
    }
}
