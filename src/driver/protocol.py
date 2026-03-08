"""
===================
LEGACY FILE — kept for backward compatibility only.

The active encoder is in display/protocol.py.
All new code should import from display.protocol, not here.

This file only contains the USB-level send commands and the confirmed
208-byte frame header. It does NOT do pixel encoding.
"""

import struct

# ── Screen geometry ───────────────────────────────────────────────────────────
SCREEN_WIDTH  = 320
SCREEN_HEIGHT = 320
W = H = 320
PIXEL_BYTES   = W * H * 2   # 204800

# ── USB control transfer commands ─────────────────────────────────────────────
CMD_SYNC   = bytes([0x33, 0xFF]) + bytes(62)
CMD_START  = bytes([0x01, 0x00])
CMD_COMMIT = bytes([0x09, 0x00])
CMD_TRIG   = bytes(4)

# ── Confirmed 208-byte frame header ──────────────────────────────────────────
def _make_frame_header() -> bytes:
    hdr = bytearray(0xD0)
    struct.pack_into('<I', hdr, 0x00, 0x78563412)
    struct.pack_into('<I', hdr, 0x04, 0x00000003)
    struct.pack_into('<I', hdr, 0x08, W)
    struct.pack_into('<I', hdr, 0x0C, W)
    struct.pack_into('<I', hdr, 0x10, H)
    struct.pack_into('<I', hdr, 0x38, 0x00000002)
    struct.pack_into('<I', hdr, 0x3C, PIXEL_BYTES)
    struct.pack_into('<I', hdr, 0xC4, 0x00002000)
    return bytes(hdr)

FRAME_HEADER     = _make_frame_header()
CONFIRMED_HEADER = FRAME_HEADER   # legacy alias

# ── Legacy encode_frame — delegates to display.protocol ──────────────────────
def encode_frame(img, rotation=None, fmt=None, header=None, flip_y=None):
    """Delegates to display.protocol.encode_frame (the single source of truth)."""
    from display.protocol import encode_frame as _encode
    return _encode(img, rotation=rotation, fmt=fmt, header=header, flip_y=flip_y)