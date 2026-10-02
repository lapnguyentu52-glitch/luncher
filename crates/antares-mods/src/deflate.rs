//! DEFLATE decompressor — RFC 1951 (thuần, zero-dep) cho việc đọc entry nén
//! trong jar (mod metadata: `fabric.mod.json`, `mods.toml`, `mcmod.info`).
//!
//! Hỗ trợ đủ 3 block types: stored (BTYPE=00), fixed Huffman (01), dynamic
//! Huffman (10) — LZ77 window qua sliding buffer 32KB. Đây là phần decode-only
//! của zlib (không compress) — đủ cho đọc jar, không phụ thuộc crate ngoài.

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InflateError {
    #[error("unexpected end of input")]
    Eof,
    #[error("invalid block type 3")]
    BadBlockType,
    #[error("invalid huffman code")]
    BadCode,
    #[error("invalid distance (before start)")]
    BadDistance,
    #[error("invalid stored block length (nlen mismatch)")]
    BadStoredLength,
    #[error("output limit exceeded")]
    OutputOverflow,
}

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bit: u32,
    acc: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0, bit: 0, acc: 0 }
    }

    /// LSB-first bit (DEFLATE dùng bit little-endian trong byte).
    fn read_bit(&mut self) -> Result<u32, InflateError> {
        if self.bit == 0 {
            if self.pos >= self.data.len() {
                return Err(InflateError::Eof);
            }
            self.acc = self.data[self.pos] as u32;
            self.pos += 1;
            self.bit = 8;
        }
        let value = self.acc & 1;
        self.acc >>= 1;
        self.bit -= 1;
        Ok(value)
    }

    fn read_bits(&mut self, count: u32) -> Result<u32, InflateError> {
        let mut value = 0u32;
        for i in 0..count {
            value |= self.read_bit()? << i;
        }
        Ok(value)
    }

    fn align_byte(&mut self) {
        self.bit = 0;
    }
}

/// Canonical Huffman decoder (đủ pattern của zlib — decode từng bit).
struct Huffman {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl Huffman {
    fn new(lengths: &[u8]) -> Self {
        let mut counts = [0u16; 16];
        for &len in lengths {
            counts[len as usize] += 1;
        }
        counts[0] = 0;
        // offsets per length
        let mut offs = [0u16; 16];
        for len in 1..16 {
            offs[len] = offs[len - 1] + counts[len - 1];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (symbol, &len) in lengths.iter().enumerate() {
            if len != 0 {
                symbols[offs[len as usize] as usize] = symbol as u16;
                offs[len as usize] += 1;
            }
        }
        Self { counts, symbols }
    }

    fn decode(&self, reader: &mut BitReader) -> Result<u16, InflateError> {
        let mut code: i32 = 0;
        let mut first: i32 = 0;
        let mut index: i32 = 0;
        for len in 1..16 {
            code |= reader.read_bit()? as i32;
            let count = self.counts[len] as i32;
            if code - first < count {
                return Ok(self.symbols[(index + (code - first)) as usize]);
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err(InflateError::BadCode)
    }
}

const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115,
    131, 163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12,
    13, 13,
];

/// Giải nén raw DEFLATE stream — `max_output` chặn zip-bomb.
pub fn inflate(data: &[u8], max_output: usize) -> Result<Vec<u8>, InflateError> {
    let mut reader = BitReader::new(data);
    let mut out: Vec<u8> = Vec::with_capacity(1024);
    loop {
        let bfinal = reader.read_bit()?;
        let btype = reader.read_bits(2)?;
        match btype {
            0 => {
                // Stored: align to byte, read LEN/NLEN
                reader.align_byte();
                let len = reader.read_bits(16)? as usize;
                let nlen = reader.read_bits(16)? as usize;
                if len ^ 0xFFFF != nlen {
                    return Err(InflateError::BadStoredLength);
                }
                if out.len() + len > max_output {
                    return Err(InflateError::OutputOverflow);
                }
                for _ in 0..len {
                    out.push(reader.read_bits(8)? as u8);
                }
            }
            1 | 2 => {
                let (lit, dist) = if btype == 1 {
                    // Fixed Huffman — lengths theo RFC 1951 §3.2.6
                    let mut lengths = [0u8; 288];
                    for (i, len) in lengths.iter_mut().enumerate() {
                        *len = match i {
                            0..=143 => 8,
                            144..=255 => 9,
                            256..=279 => 7,
                            _ => 8,
                        };
                    }
                    (Huffman::new(&lengths), Huffman::new(&[5u8; 30]))
                } else {
                    read_dynamic_tables(&mut reader)?
                };
                loop {
                    let symbol = lit.decode(&mut reader)?;
                    match symbol {
                        0..=255 => {
                            if out.len() >= max_output {
                                return Err(InflateError::OutputOverflow);
                            }
                            out.push(symbol as u8);
                        }
                        256 => break,
                        257..=285 => {
                            let idx = (symbol - 257) as usize;
                            let length = LENGTH_BASE[idx] as usize
                                + reader.read_bits(LENGTH_EXTRA[idx] as u32)? as usize;
                            let dsym = dist.decode(&mut reader)? as usize;
                            let distance = DIST_BASE[dsym] as usize
                                + reader.read_bits(DIST_EXTRA[dsym] as u32)? as usize;
                            if distance > out.len() {
                                return Err(InflateError::BadDistance);
                            }
                            if out.len() + length > max_output {
                                return Err(InflateError::OutputOverflow);
                            }
                            // LZ77 copy — byte-by-byte vì distance có thể < length
                            let start = out.len() - distance;
                            for i in 0..length {
                                let b = out[start + i];
                                out.push(b);
                            }
                        }
                        _ => return Err(InflateError::BadCode),
                    }
                }
            }
            _ => return Err(InflateError::BadBlockType),
        }
        if bfinal == 1 {
            break;
        }
    }
    Ok(out)
}

fn read_dynamic_tables(reader: &mut BitReader) -> Result<(Huffman, Huffman), InflateError> {
    const ORDER: [usize; 19] = [
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ];
    let hlit = reader.read_bits(5)? as usize + 257;
    let hdist = reader.read_bits(5)? as usize + 1;
    let hclen = reader.read_bits(4)? as usize + 4;
    let mut code_lengths = [0u8; 19];
    for i in 0..hclen {
        code_lengths[ORDER[i]] = reader.read_bits(3)? as u8;
    }
    let code_huff = Huffman::new(&code_lengths);
    let mut lengths = vec![0u8; hlit + hdist];
    let mut i = 0usize;
    while i < lengths.len() {
        let symbol = code_huff.decode(reader)?;
        match symbol {
            0..=15 => {
                lengths[i] = symbol as u8;
                i += 1;
            }
            16 => {
                // copy previous 3..6 times
                if i == 0 {
                    return Err(InflateError::BadCode);
                }
                let prev = lengths[i - 1];
                let repeat = 3 + reader.read_bits(2)? as usize;
                for _ in 0..repeat {
                    if i >= lengths.len() {
                        return Err(InflateError::BadCode);
                    }
                    lengths[i] = prev;
                    i += 1;
                }
            }
            17 => {
                // zeros 3..10
                let repeat = 3 + reader.read_bits(3)? as usize;
                i += repeat;
                if i > lengths.len() {
                    return Err(InflateError::BadCode);
                }
            }
            18 => {
                // zeros 11..138
                let repeat = 11 + reader.read_bits(7)? as usize;
                i += repeat;
                if i > lengths.len() {
                    return Err(InflateError::BadCode);
                }
            }
            _ => return Err(InflateError::BadCode),
        }
    }
    let lit = Huffman::new(&lengths[..hlit]);
    let dist = Huffman::new(&lengths[hlit..]);
    Ok((lit, dist))
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

    /// Golden vectors sinh bằng Python `zlib.compress(payload, 6)[2:-4]`
    /// (raw deflate = zlib stream bỏ 2 byte header + 4 byte adler32).
    #[test]
    fn golden_vectors_from_python_zlib() {
        // === short: fabric.mod.json mẫu (71 bytes payload) ===
        let out = inflate(&hex("ab562a4ece48cd4d0c4b2d2acecccf53b232d451ca4c51b2522a492d2ed1cdcd4f51d2512a83c92919ea19e8190045f212735381dc10a01a055fa09a5a00"), 1024).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            r#"{"schemaVersion":1,"id":"test-mod","version":"1.0.0","name":"Test Mod"}"#
        );

        // === repeat: chuỗi lặp (2440 bytes payload — LZ77 matches nhiều) ===
        let out = inflate(&hex("edcbbb09c0300c06e155b49a6cfd8690f881bc3f0452bbc80057dd355ff39257b576ca333d94165a1ab12d5567efdf9694dffbacc06030180c0683c17ff00b"), 65536).unwrap();
        let expected = "fabric fabric fabric fabric loader depends recommends breaks ".repeat(40);
        assert_eq!(String::from_utf8(out).unwrap(), expected);

        // === empty stream ===
        let out = inflate(&hex("0300"), 16).unwrap();
        assert!(out.is_empty());

        // === binary: toàn bộ 0..255 × 3 (768 bytes) ===
        let out = inflate(&hex("6360646266616563e7e0e4e2e6e1e5e3171014121611151397909492969195935750545256515553d7d0d4d2d6d1d5d33730343236313533b7b0b4b2b6b1b5b37770747276717573f7f0f4f2f6f1f5f30f080c0a0e090d0b8f888c8a8e898d8b4f484c4a4e494d4bcfc8cccacec9cdcb2f282c2a2e292d2bafa8acaaaea9adab6f686c6a6e696d6befe8eceaeee9edeb9f3071d2e42953a74d9f3173d6ec3973e7cd5fb070d1e2254b972d5fb172d5ea356bd7addfb071d3e62d5bb76ddfb173d7ee3d7bf7ed3f70f0d0e123478f1d3f71f2d4e93367cf9dbf70f1d2e52b57af5dbf71f3d6ed3b77efdd7ff0f0d1e3274f9f3d7ff1f2d5eb376fdfbdfff0f1d3e72f5fbf7dfff1f3d7ef3f7ffffd6718f5ff88f63f00"), 4096).unwrap();
        let expected: Vec<u8> = (0..=255u8).cycle().take(768).collect();
        assert_eq!(out, expected);
    }

    /// Stored blocks (BTYPE=00) — tự dựng: header 3 bit + align + LEN/NLEN + data.
    #[test]
    fn stored_block_roundtrip() {
        let payload = b"hello stored block";
        let mut stream = vec![0x01u8]; // BFINAL=1, BTYPE=00
        stream.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        stream.extend_from_slice(&(!(payload.len() as u16)).to_le_bytes());
        stream.extend_from_slice(payload);
        let out = inflate(&stream, 1024).unwrap();
        assert_eq!(out, payload);

        // NLEN sai → lỗi
        let mut bad = vec![0x01u8];
        bad.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        bad.extend_from_slice(&0x0000u16.to_le_bytes());
        assert!(matches!(inflate(&bad, 1024), Err(InflateError::BadStoredLength)));
    }

    /// Output limit chặn zip-bomb.
    #[test]
    fn output_limit_enforced() {
        // repeat stream giải nén ra 2440 bytes — limit 100 → lỗi
        let data = hex("edcbbb09c0300c06e155b49a6cfd8690f881bc3f0452bbc80057dd355ff39257b576ca333d94165a1ab12d5567efdf9694dffbacc06030180c0683c17ff00b");
        assert!(matches!(inflate(&data, 100), Err(InflateError::OutputOverflow)));
        // truncated input → EOF
        assert!(matches!(inflate(&data[..5], 65536), Err(InflateError::Eof)));
    }
}
