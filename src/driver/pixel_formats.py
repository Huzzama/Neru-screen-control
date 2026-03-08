"""
========================
Pixel format encoders for ChiZhu USB display (87ad:70db).

Confirmed working format: RGB565 big-endian, row-major, 320x320.
No framebuffer shifting. No np.roll. No .T transpose.
Orientation is handled at the image level (rotation/flip) before encoding.
"""

import numpy as np
from PIL import Image

W, H = 320, 320


def _prepare(img: Image.Image) -> np.ndarray:
    """Resize to 320x320 and return uint8 RGB numpy array."""
    if img.size != (W, H):
        img = img.resize((W, H), Image.LANCZOS)
    return np.array(img.convert("RGB"), dtype=np.uint8)


# ── RGB565 ────────────────────────────────────────────────────────────────────

def to_rgb565_be(img: Image.Image) -> bytes:
    """RGB565 big-endian — confirmed working format."""
    arr = _prepare(img)
    px = ((arr[:, :, 0].astype(np.uint16) >> 3) << 11) | \
         ((arr[:, :, 1].astype(np.uint16) >> 2) << 5)  | \
          (arr[:, :, 2].astype(np.uint16) >> 3)
    return px.flatten().astype(">u2").tobytes()


def to_rgb565_le(img: Image.Image) -> bytes:
    """RGB565 little-endian."""
    arr = _prepare(img)
    px = ((arr[:, :, 0].astype(np.uint16) >> 3) << 11) | \
         ((arr[:, :, 1].astype(np.uint16) >> 2) << 5)  | \
          (arr[:, :, 2].astype(np.uint16) >> 3)
    return px.flatten().astype("<u2").tobytes()


# ── BGR565 ────────────────────────────────────────────────────────────────────

def to_bgr565_be(img: Image.Image) -> bytes:
    """BGR565 big-endian."""
    arr = _prepare(img)
    px = ((arr[:, :, 2].astype(np.uint16) >> 3) << 11) | \
         ((arr[:, :, 1].astype(np.uint16) >> 2) << 5)  | \
          (arr[:, :, 0].astype(np.uint16) >> 3)
    return px.flatten().astype(">u2").tobytes()


def to_bgr565_le(img: Image.Image) -> bytes:
    """BGR565 little-endian."""
    arr = _prepare(img)
    px = ((arr[:, :, 2].astype(np.uint16) >> 3) << 11) | \
         ((arr[:, :, 1].astype(np.uint16) >> 2) << 5)  | \
          (arr[:, :, 0].astype(np.uint16) >> 3)
    return px.flatten().astype("<u2").tobytes()


# ── RGB888 / BGR888 ───────────────────────────────────────────────────────────

def to_rgb888(img: Image.Image) -> bytes:
    return _prepare(img).tobytes()


def to_bgr888(img: Image.Image) -> bytes:
    arr = _prepare(img)
    return arr[:, :, ::-1].tobytes()


# ── Test frame helpers ────────────────────────────────────────────────────────

_ENCODERS = {
    "rgb565_be": to_rgb565_be,
    "rgb565_le": to_rgb565_le,
    "bgr565_be": to_bgr565_be,
    "bgr565_le": to_bgr565_le,
    "rgb888":    to_rgb888,
    "bgr888":    to_bgr888,
}


def solid_frame(r: int, g: int, b: int, fmt: str = "rgb565_be") -> bytes:
    img = Image.new("RGB", (W, H), (r, g, b))
    return _ENCODERS[fmt](img)


def checkerboard_frame(fmt: str = "rgb565_be") -> bytes:
    img = Image.new("RGB", (W, H), (0, 0, 0))
    px  = img.load()
    for y in range(H):
        for x in range(W):
            if (x // 20 + y // 20) % 2 == 0:
                px[x, y] = (255, 255, 255)
    return _ENCODERS[fmt](img)


FORMAT_FRAME_SIZE = {k: W * H * (3 if "888" in k else 2) for k in _ENCODERS}