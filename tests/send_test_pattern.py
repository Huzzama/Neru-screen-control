"""
Interactive command-line tool to send specific test patterns to the display.

Use this AFTER protocol_probe.py to fine-tune the confirmed protocol variant.
Set WORKING_HEADER and WORKING_FORMAT below once you know what works.

USAGE:
    python tests/send_test_pattern.py [pattern] [format] [header]

    python tests/send_test_pattern.py red
    python tests/send_test_pattern.py green rgb565_le
    python tests/send_test_pattern.py checker rgb565_be capture_header
    python tests/send_test_pattern.py gradient
    python tests/send_test_pattern.py loop_colors   (cycles colors every 2s)

Patterns: red, green, blue, white, black, checker, gradient, loop_colors
Formats:  rgb565_be, rgb565_le, bgr565_be, bgr565_le, rgb888, bgr888
Headers:  none, capture_header, single_0x2C, single_0x55
"""

import sys
import time
import struct
import usb.core
import usb.util

# ── adjust these after confirming with protocol_probe.py ─────────────────────
WORKING_HEADER: bytes | None = None          # e.g. bytes([0x1b,0x00,...])
WORKING_FORMAT: str          = "rgb565_be"   # see pixel_formats.py

# ── device ───────────────────────────────────────────────────────────────────
VENDOR_ID  = 0x87AD
PRODUCT_ID = 0x70DB
EP_OUT     = 0x01
EP_IN      = 0x81
TIMEOUT    = 5000
CHUNK      = 512
W, H       = 320, 240

import os, sys
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "src"))

from driver.pixel_formats import (
    to_rgb565_be, to_rgb565_le, to_bgr565_be, to_bgr565_le,
    to_rgb888, to_bgr888, checkerboard_frame, solid_frame,
)
from PIL import Image
import numpy as np


def open_device():
    dev = usb.core.find(idVendor=VENDOR_ID, idProduct=PRODUCT_ID)
    if dev is None:
        print("ERROR: Device not found.")
        sys.exit(1)
    try:
        if dev.is_kernel_driver_active(0):
            dev.detach_kernel_driver(0)
    except Exception:
        pass
    dev.set_configuration()
    return dev


def send_frame(dev, pixel_bytes: bytes, header: bytes | None = WORKING_HEADER):
    payload = (header or b"") + pixel_bytes
    sent = 0
    for i in range(0, len(payload), CHUNK):
        dev.write(EP_OUT, payload[i:i+CHUNK], timeout=TIMEOUT)
        sent += min(CHUNK, len(payload) - i)
    print(f"Sent {sent} bytes  (header={len(header) if header else 0}  "
          f"pixels={len(pixel_bytes)})")
    # Check for response
    try:
        resp = bytes(dev.read(EP_IN, 64, timeout=500))
        if resp:
            print(f"Response: {resp.hex(' ')}")
    except Exception:
        pass


def make_gradient():
    """Horizontal gradient: black → red left-to-right."""
    img = Image.new("RGB", (W, H), (0, 0, 0))
    px = img.load()
    for x in range(W):
        v = int(255 * x / (W - 1))
        for y in range(H):
            px[x, y] = (v, 0, 0)
    return img


PATTERNS = {
    "red":    lambda fmt: solid_frame(255,   0,   0, fmt),
    "green":  lambda fmt: solid_frame(  0, 255,   0, fmt),
    "blue":   lambda fmt: solid_frame(  0,   0, 255, fmt),
    "white":  lambda fmt: solid_frame(255, 255, 255, fmt),
    "black":  lambda fmt: solid_frame(  0,   0,   0, fmt),
    "checker": lambda fmt: checkerboard_frame(fmt),
    "gradient": lambda fmt: {
        "rgb565_be": to_rgb565_be,
        "rgb565_le": to_rgb565_le,
        "bgr565_be": to_bgr565_be,
        "bgr565_le": to_bgr565_le,
        "rgb888":    to_rgb888,
        "bgr888":    to_bgr888,
    }[fmt](make_gradient()),
}

HEADERS = {
    "none":          None,
    "capture_header": bytes([0x1b,0x00,0x10,0x90,0xd4,0x43,0x0e,0xb2,0xff,0xff,0x00,0x00]),
    "single_0x2C":   bytes([0x2C]),
    "single_0x55":   bytes([0x55]),
}


def main():
    args = sys.argv[1:]
    pattern = args[0] if len(args) > 0 else "red"
    fmt     = args[1] if len(args) > 1 else WORKING_FORMAT
    hdr_key = args[2] if len(args) > 2 else "none"

    header = HEADERS.get(hdr_key, WORKING_HEADER)

    dev = open_device()

    if pattern == "loop_colors":
        colors = ["red", "green", "blue", "white", "black", "checker"]
        print("Looping colors — Ctrl-C to stop")
        i = 0
        while True:
            c = colors[i % len(colors)]
            pixels = PATTERNS[c](fmt)
            print(f"\n→ {c.upper()}")
            send_frame(dev, pixels, header)
            time.sleep(2)
            i += 1
    else:
        if pattern not in PATTERNS:
            print(f"Unknown pattern: {pattern}")
            print(f"Available: {', '.join(PATTERNS)}")
            sys.exit(1)
        pixels = PATTERNS[pattern](fmt)
        print(f"Pattern: {pattern}  Format: {fmt}  Header: {hdr_key}")
        send_frame(dev, pixels, header)


if __name__ == "__main__":
    main()
