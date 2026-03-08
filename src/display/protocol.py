"""
Single source of truth for the UI display encoder.

Confirmed hardware: ChiZhu Tech USBDISPLAY (87ad:70db), 320x320.

Confirmed working pipeline (matches standalone ruler test script):
  - RGB565 big-endian
  - row-major px.flatten()
  - no np.roll
  - no .T transpose
  - orientation via PIL rotate/flip at image level only

Frame sequence:
  1. ctrl_transfer  CMD_START  (0x01 0x00)
  2. bulk write     CMD_TRIG   (4 zero bytes)
  3. bulk write     FRAME_HEADER (208 bytes) + pixels
  4. ctrl_transfer  CMD_COMMIT (0x09 0x00)
"""

import struct
import numpy as np
from PIL import Image, ImageOps

# ── Screen geometry ───────────────────────────────────────────────────────────
SCREEN_WIDTH  = 320
SCREEN_HEIGHT = 320
W = H = 320
PIXEL_COUNT   = W * H
PIXEL_BYTES   = PIXEL_COUNT * 2   # 204800

# ── Control transfer commands ─────────────────────────────────────────────────
CMD_SYNC   = bytes([0x33, 0xFF]) + bytes(62)
CMD_START  = bytes([0x01, 0x00])
CMD_COMMIT = bytes([0x09, 0x00])
CMD_TRIG   = bytes(4)

# ── 208-byte frame header (confirmed from Windows USB capture) ────────────────
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

FRAME_HEADER = _make_frame_header()

# ── Active encoder settings ───────────────────────────────────────────────────
# Phase 1: match standalone ruler exactly — no rotation, no flip.
# Once image is confirmed correct, tune ACTIVE_ROTATION / ACTIVE_FLIP_Y.
ACTIVE_FORMAT             = "rgb565_be"  # confirmed by visual test
ACTIVE_HEADER             = FRAME_HEADER
ACTIVE_ROTATION           = 0      # tune via CalibrationTab — 0 / 90 / 180 / 270
ACTIVE_FLIP_Y             = False   # tune via CalibrationTab
ACTIVE_FRAMEBUFFER_OFFSET  = 0     # row offset  — np.roll(arr, n, axis=0)
ACTIVE_FRAMEBUFFER_OFFSET_X = 0    # column offset — np.roll(arr, n, axis=1)
                                    # Both calibrated via CalibrationTab
                                    # Saved to config.json as "framebuffer_offset" / "framebuffer_offset_x"

# ── Per-format pixel encoders ─────────────────────────────────────────────────
# Rules:
#   - Always use dtype=np.uint8 for arr, then .astype(np.uint16) per channel
#   - Always use px.flatten() — row-major, never px.T.flatten()
#   - Never use np.roll — no framebuffer shifting

def _rgb565_be(arr):
    px = ((arr[:, :, 0].astype(np.uint16) >> 3) << 11) | \
         ((arr[:, :, 1].astype(np.uint16) >> 2) << 5)  | \
          (arr[:, :, 2].astype(np.uint16) >> 3)
    return px.flatten().astype(">u2").tobytes()

def _rgb565_le(arr):
    px = ((arr[:, :, 0].astype(np.uint16) >> 3) << 11) | \
         ((arr[:, :, 1].astype(np.uint16) >> 2) << 5)  | \
          (arr[:, :, 2].astype(np.uint16) >> 3)
    return px.flatten().astype("<u2").tobytes()

def _bgr565_be(arr):
    px = ((arr[:, :, 2].astype(np.uint16) >> 3) << 11) | \
         ((arr[:, :, 1].astype(np.uint16) >> 2) << 5)  | \
          (arr[:, :, 0].astype(np.uint16) >> 3)
    return px.flatten().astype(">u2").tobytes()

def _bgr565_le(arr):
    px = ((arr[:, :, 2].astype(np.uint16) >> 3) << 11) | \
         ((arr[:, :, 1].astype(np.uint16) >> 2) << 5)  | \
          (arr[:, :, 0].astype(np.uint16) >> 3)
    return px.flatten().astype("<u2").tobytes()

def _grb565_le(arr):
    px = ((arr[:, :, 1].astype(np.uint16) >> 3) << 11) | \
         ((arr[:, :, 0].astype(np.uint16) >> 2) << 5)  | \
          (arr[:, :, 2].astype(np.uint16) >> 3)
    return px.flatten().astype("<u2").tobytes()

def _rgb888(arr):
    return arr.tobytes()

def _bgr888(arr):
    return arr[:, :, ::-1].tobytes()

_ENCODERS = {
    "rgb565_be": _rgb565_be,
    "rgb565_le": _rgb565_le,
    "bgr565_be": _bgr565_be,
    "bgr565_le": _bgr565_le,
    "grb565_le": _grb565_le,
    "rgb888":    _rgb888,
    "bgr888":    _bgr888,
}

# ── Core encoder ──────────────────────────────────────────────────────────────

def encode_pixels(img: Image.Image,
                  rotation:            int  = None,
                  fmt:                 str  = None,
                  flip_y:              bool = None,
                  framebuffer_offset:  int  = None,
                  framebuffer_offset_x: int = None) -> bytes:
    """
    Convert PIL image to pixel bytes.
    Falls back to ACTIVE_* globals when arguments are None.

    Pipeline (in order):
        1. resize to WxH
        2. rotate (degrees CCW)
        3. flip_y
        4. np.roll(arr, framebuffer_offset,   axis=0)  — vertical row-start fix
        5. np.roll(arr, framebuffer_offset_x, axis=1)  — horizontal col-start fix
        6. RGB565 encode + flatten
    """
    rotation             = ACTIVE_ROTATION            if rotation             is None else rotation
    fmt                  = ACTIVE_FORMAT               if fmt                  is None else fmt
    flip_y               = ACTIVE_FLIP_Y               if flip_y               is None else flip_y
    framebuffer_offset   = ACTIVE_FRAMEBUFFER_OFFSET   if framebuffer_offset   is None else framebuffer_offset
    framebuffer_offset_x = ACTIVE_FRAMEBUFFER_OFFSET_X if framebuffer_offset_x is None else framebuffer_offset_x

    print(f"[display.protocol] encode_pixels fmt={fmt} rotation={rotation} "
          f"flip_y={flip_y} fb_offset_y={framebuffer_offset} fb_offset_x={framebuffer_offset_x}")

    img = img.resize((W, H), Image.LANCZOS).convert("RGB")

    if rotation:
        img = img.rotate(rotation, expand=False)

    if flip_y:
        img = ImageOps.flip(img)

    arr = np.array(img, dtype=np.uint8)

    # Vertical framebuffer offset — display starts reading from row N instead of 0
    if framebuffer_offset:
        arr = np.roll(arr, framebuffer_offset, axis=0)

    # Horizontal framebuffer offset — display starts reading from column N instead of 0
    if framebuffer_offset_x:
        arr = np.roll(arr, framebuffer_offset_x, axis=1)

    if fmt not in _ENCODERS:
        raise ValueError(f"Unknown format: {fmt!r}. Options: {list(_ENCODERS)}")

    return _ENCODERS[fmt](arr)


def encode_frame(img:                 Image.Image,
                 rotation:            int   = None,
                 fmt:                 str   = None,
                 header:              bytes = None,
                 flip_y:              bool  = None,
                 framebuffer_offset:  int   = None,
                 framebuffer_offset_x: int  = None) -> bytes:
    """Full frame packet: FRAME_HEADER + encoded pixels."""
    hdr    = ACTIVE_HEADER if header is None else header
    pixels = encode_pixels(img, rotation=rotation, fmt=fmt,
                           flip_y=flip_y,
                           framebuffer_offset=framebuffer_offset,
                           framebuffer_offset_x=framebuffer_offset_x)
    return (hdr or b"") + pixels


# ── Runtime configuration (called once at app startup) ────────────────────────

def configure_from_config(config) -> None:
    """
    Load calibration values from a Config object into the ACTIVE_* globals.
    Call this once at startup after Config is loaded, before any frame is sent.

    Example (in controller.py or main_window.py):
        from display.protocol import configure_from_config
        configure_from_config(cfg)
    """
    global ACTIVE_ROTATION, ACTIVE_FLIP_Y, ACTIVE_FRAMEBUFFER_OFFSET, ACTIVE_FRAMEBUFFER_OFFSET_X
    ACTIVE_ROTATION             = config.get("rotation",              ACTIVE_ROTATION)
    ACTIVE_FLIP_Y               = bool(config.get("flip_y",           ACTIVE_FLIP_Y))
    ACTIVE_FRAMEBUFFER_OFFSET   = config.get("framebuffer_offset",    ACTIVE_FRAMEBUFFER_OFFSET)
    ACTIVE_FRAMEBUFFER_OFFSET_X = config.get("framebuffer_offset_x",  ACTIVE_FRAMEBUFFER_OFFSET_X)
    print(f"[display.protocol] configured: rotation={ACTIVE_ROTATION} "
          f"flip_y={ACTIVE_FLIP_Y} fb_y={ACTIVE_FRAMEBUFFER_OFFSET} fb_x={ACTIVE_FRAMEBUFFER_OFFSET_X}")


# ── Legacy aliases (keep existing callers working) ────────────────────────────
CONFIRMED_HEADER = FRAME_HEADER
CONFIRMED_FORMAT = "rgb565_be"

def image_to_rgb565(img: Image.Image) -> bytes:
    return encode_pixels(img, rotation=0, flip_y=False)

def image_to_bytes(img: Image.Image, fmt: str = None) -> bytes:
    return encode_pixels(img, rotation=0, flip_y=False, fmt=fmt)

def build_frame(pixel_bytes: bytes, header=None) -> bytes:
    return (header or FRAME_HEADER) + pixel_bytes

def rotate_image(img: Image.Image, degrees: int) -> Image.Image:
    return img if degrees == 0 else img.rotate(degrees, expand=False)