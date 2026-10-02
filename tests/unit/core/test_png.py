"""PNG codec (templates.encode_png) — header, dims, decode ngược bằng zlib."""
from __future__ import annotations

import struct
import zlib

from services.resources.templates import encode_png


def _decode(png: bytes) -> list[list[bytes]]:
    idx, idat = 8, b""
    while idx < len(png):
        ln = struct.unpack(">I", png[idx:idx + 4])[0]
        tag = png[idx + 4:idx + 8]
        if tag == b"IDAT":
            idat += png[idx + 8:idx + 8 + ln]
        idx += 12 + ln
    raw = zlib.decompress(idat)
    w, h = struct.unpack(">II", png[16:24])
    stride = w * 4 + 1
    return [[raw[y * stride + 1 + x * 4: y * stride + 1 + x * 4 + 4]
             for x in range(w)] for y in range(h)]


def test_encode_png_valid_header_and_dims():
    png = encode_png(4, 3, b"\x00" * (4 * 3 * 4))
    assert png.startswith(b"\x89PNG\r\n\x1a\n")
    assert struct.unpack(">II", png[16:24]) == (4, 3)
    # CRC mỗi chunk hợp lệ
    idx = 8
    while idx < len(png):
        ln = struct.unpack(">I", png[idx:idx + 4])[0]
        tag = png[idx + 4:idx + 8]
        body = png[idx + 8:idx + 8 + ln]
        crc = struct.unpack(">I", png[idx + 8 + ln:idx + 12 + ln])[0]
        assert crc == zlib.crc32(tag + body) & 0xFFFFFFFF
        idx += 12 + ln


def test_roundtrip_pixel_values():
    px = bytearray(2 * 2 * 4)
    px[0:4] = b"\xff\x00\x00\xff"   # (0,0) đỏ
    px[4 * 3:4 * 3 + 4] = b"\x00\xff\x00\x80"  # (1,1) xanh lá alpha 128
    img = _decode(encode_png(2, 2, bytes(px)))
    assert img[0][0][:3] == b"\xff\x00\x00"
    assert img[1][1][3] == 0x80
