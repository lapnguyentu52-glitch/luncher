//! PNG encode/parse tối giản — parity `services/resources/templates.py::encode_png`
//! + `validator._check_png` (mục 77 header parse không decode full).
//!
//! `encode_png`: RGBA8 → PNG (IHDR + IDAT + IEND). IDAT dùng zlib wrapper với
//! **stored deflate blocks** (không nén — byte-đúng, đơn giản, chuẩn PNG mọi
//! reader đọc được — dùng cho template generation/solid textures).
//! `decode_dims`: parse IHDR nhanh — chặn dims trước khi decode (mục 77).

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PngError {
    #[error("not a valid PNG")]
    BadSignature,
    #[error("PNG header không hợp lệ (IHDR thiếu)")]
    BadHeader,
    #[error("dimensions quá lớn: {0}x{1}")]
    TooLarge(u32, u32),
    #[error("zlib stream không hợp lệ")]
    BadZlib,
}

/// PNG signature.
pub const PNG_SIG: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];

/// Parity MAX_DIM / MAX_PNG_DIM (4096 — decision Batch 0, mục 34/77).
pub const MAX_DIM: u32 = 4096;

fn crc32(data: &[u8]) -> u32 {
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

fn chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(tag);
    out.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(tag);
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

/// Encode RGBA8 → PNG bytes (parity `encode_png`): IHDR (width/height,
/// bit depth 8, color type 6 RGBA) + IDAT + IEND. Dữ liệu quét mỗi dòng có
/// filter byte 0 (None) trước — chuẩn PNG.
pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, PngError> {
    if width == 0 || height == 0 {
        return Err(PngError::BadHeader);
    }
    if width > MAX_DIM || height > MAX_DIM {
        return Err(PngError::TooLarge(width, height));
    }
    let expected = width as usize * height as usize * 4;
    if rgba.len() != expected {
        return Err(PngError::BadHeader);
    }

    // raw scanlines: filter byte 0 + RGBA per pixel
    let stride = width as usize * 4;
    let mut raw = Vec::with_capacity((stride + 1) * height as usize);
    for y in 0..height as usize {
        raw.push(0u8); // filter None
        raw.extend_from_slice(&rgba[y * stride..(y + 1) * stride]);
    }

    // zlib stream với stored deflate blocks (BTYPE 00, chia 65535)
    let zlib = zlib_stored(&raw);

    let mut out = Vec::with_capacity(zlib.len() + 64);
    out.extend_from_slice(&PNG_SIG);
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8); // bit depth
    ihdr.push(6); // color type RGBA
    ihdr.push(0); // compression
    ihdr.push(0); // filter
    ihdr.push(0); // interlace
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &zlib);
    chunk(&mut out, b"IEND", &[]);
    Ok(out)
}

/// zlib stream (header 0x78 0x01 + stored deflate + adler32).
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78u8, 0x01]; // CM=8, CINFO=7, FCHECK hợp lệ
    let mut chunks = data.chunks(65_535).peekable();
    if data.is_empty() {
        // 1 block rỗng final
        out.extend_from_slice(&[0x01, 0x00, 0x00, 0xFF, 0xFF]);
    }
    while let Some(chunk_bytes) = chunks.next() {
        let last = chunks.peek().is_none();
        out.push(if last { 1 } else { 0 });
        let len = chunk_bytes.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(chunk_bytes);
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

/// Parse PNG header — trả `(width, height)`; lỗi theo parity `_check_png`
/// ("not a valid PNG" / dims). Không decode IDAT (mục 77).
pub fn decode_png_dims(data: &[u8]) -> Result<(u32, u32), PngError> {
    if data.len() < 33 || data[..8] != PNG_SIG {
        return Err(PngError::BadSignature);
    }
    // chunk 1 phải là IHDR (len 4..8, type 12..16)
    if &data[12..16] != b"IHDR" {
        return Err(PngError::BadHeader);
    }
    let w = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
    let h = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
    if w == 0 || h == 0 || w > MAX_DIM || h > MAX_DIM {
        return Err(PngError::TooLarge(w, h));
    }
    Ok((w, h))
}

/// Parity `_check_png` — Ok(()) hoặc message lỗi ("not a valid PNG"/"too large").
pub fn check_png_message(data: &[u8]) -> Result<(), String> {
    match decode_png_dims(data) {
        Ok(_) => Ok(()),
        Err(PngError::BadSignature) | Err(PngError::BadHeader) => {
            Err("not a valid PNG".into())
        }
        Err(PngError::TooLarge(w, h)) => Err(format!("too large: {w}x{h}")),
        Err(PngError::BadZlib) => Err("not a valid PNG".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Golden: PNG encode sinh bằng Python (struct + zlib manual build) —
    /// solid 1x1 đỏ RGBA.
    #[test]
    fn encode_png_1x1_matches_layout() {
        let png = encode_png(1, 1, &[255, 0, 0, 255]).unwrap();
        assert_eq!(&png[..8], &PNG_SIG);
        // IHDR: len 13, type IHDR
        assert_eq!(&png[8..12], &13u32.to_be_bytes());
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(&png[16..20], &1u32.to_be_bytes()); // width
        assert_eq!(&png[20..24], &1u32.to_be_bytes()); // height
        assert_eq!(png[24], 8); // bit depth
        assert_eq!(png[25], 6); // RGBA
        // IEND chunk cuối: len(4) + "IEND"(4) + crc(4) → 4 byte type ở [-8..-4]
        assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");
    }

    #[test]
    fn encode_png_roundtrip_dims() {
        // encode rồi parse lại dims qua decode_png_dims — self-consistency
        let rgba = vec![100u8; 4 * 3 * 2]; // 3x2
        let png = encode_png(3, 2, &rgba).unwrap();
        assert_eq!(decode_png_dims(&png).unwrap(), (3, 2));
        check_png_message(&png).unwrap();
    }

    #[test]
    fn decode_dims_rejects_bad_input() {
        // không signature
        assert_eq!(decode_png_dims(b"nope").unwrap_err(), PngError::BadSignature);
        // signature + thiếu IHDR
        let mut data = PNG_SIG.to_vec();
        data.extend_from_slice(&[0u8; 40]);
        assert_eq!(decode_png_dims(&data).unwrap_err(), PngError::BadHeader);
        // dims 0
        let mut data = PNG_SIG.to_vec();
        data.extend_from_slice(&13u32.to_be_bytes());
        data.extend_from_slice(b"IHDR");
        data.extend_from_slice(&0u32.to_be_bytes());
        data.extend_from_slice(&10u32.to_be_bytes());
        data.extend_from_slice(&[8, 6, 0, 0, 0]);
        data.extend_from_slice(&[0u8; 4]); // crc — chạm tối thiểu 33 bytes (parity _check_png)
        assert!(matches!(decode_png_dims(&data), Err(PngError::TooLarge(0, 10))));
    }

    #[test]
    fn check_png_message_parity() {
        assert_eq!(check_png_message(b"junk").unwrap_err(), "not a valid PNG");
        let mut data = PNG_SIG.to_vec();
        data.extend_from_slice(&13u32.to_be_bytes());
        data.extend_from_slice(b"IHDR");
        data.extend_from_slice(&5000u32.to_be_bytes()); // > 4096
        data.extend_from_slice(&5000u32.to_be_bytes());
        data.extend_from_slice(&[8, 6, 0, 0, 0]);
        data.extend_from_slice(&[0u8; 4]); // crc — chạm tối thiểu 33 bytes (parity _check_png)
        assert_eq!(check_png_message(&data).unwrap_err(), "too large: 5000x5000");
    }

    #[test]
    fn encode_dims_out_of_range() {
        assert!(encode_png(0, 1, &[0; 4]).is_err());
        assert!(encode_png(5000, 1, &[0; 4]).is_err());
        // rgba len sai
        assert!(encode_png(2, 2, &[0; 4]).is_err());
    }
}
