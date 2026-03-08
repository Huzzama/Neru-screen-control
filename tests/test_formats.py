"""
Color format tester — cycles through all pixel format combinations.
Run from the TRCC repo root:
    python3 test_formats.py

It sends a test image with clearly distinct colored blocks and labels.
Watch the screen and note which number looks correct, then tell Claude.
"""

import sys, os, time, struct
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "src"))

import numpy as np
from PIL import Image, ImageDraw, ImageFont
import usb.core, usb.util

VENDOR_ID  = 0x87AD
PRODUCT_ID = 0x70DB
EP_OUT     = 0x01
CHUNK      = 512
TIMEOUT    = 10000

W = H = 320

# ── 208-byte confirmed header ─────────────────────────────────────────────────
def make_header():
    hdr = bytearray(0xD0)
    struct.pack_into('<I', hdr, 0x00, 0x78563412)
    struct.pack_into('<I', hdr, 0x04, 0x00000003)
    struct.pack_into('<I', hdr, 0x08, W)
    struct.pack_into('<I', hdr, 0x0C, W)
    struct.pack_into('<I', hdr, 0x10, H)
    struct.pack_into('<I', hdr, 0x38, 0x00000002)
    struct.pack_into('<I', hdr, 0x3C, 0x00032000)
    struct.pack_into('<I', hdr, 0xC4, 0x00002000)
    return bytes(hdr)

HEADER = make_header()

# ── All 6 pixel format encoders ───────────────────────────────────────────────
def encode(img, ch_order, endian):
    """ch_order: e.g. (0,1,2)=RGB, (1,0,2)=GRB, (2,1,0)=BGR etc."""
    arr = np.array(img.convert('RGB'), dtype=np.uint16)
    r = arr[:,:,ch_order[0]]
    g = arr[:,:,ch_order[1]]
    b = arr[:,:,ch_order[2]]
    px = ((r>>3)<<11) | ((g>>2)<<5) | (b>>3)
    return px.flatten().astype(endian + 'u2').tobytes()

FORMATS = [
    ("1  RGB565-LE", (0,1,2), '<'),
    ("2  RGB565-BE", (0,1,2), '>'),
    ("3  GRB565-LE", (1,0,2), '<'),
    ("4  GRB565-BE", (1,0,2), '>'),
    ("5  BGR565-LE", (2,1,0), '<'),
    ("6  BGR565-BE", (2,1,0), '>'),
]

# ── Test image: colored blocks with label ─────────────────────────────────────
def make_test_image(label):
    img = Image.new('RGB', (W, H), (0, 0, 0))
    d   = ImageDraw.Draw(img)
    try:
        font = ImageFont.truetype(
            "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf", 32)
        small = ImageFont.truetype(
            "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf", 20)
    except Exception:
        font = small = ImageFont.load_default()

    # Big colored blocks — easy to identify
    d.rectangle([0,   0,   159, 159], fill=(255, 0,   0  ))  # should be RED    TL
    d.rectangle([160, 0,   319, 159], fill=(0,   255, 0  ))  # should be GREEN  TR
    d.rectangle([0,   160, 159, 319], fill=(0,   0,   255))  # should be BLUE   BL
    d.rectangle([160, 160, 319, 319], fill=(255, 255, 0  ))  # should be YELLOW BR

    # Labels on each block
    d.text((10,  10),  "RED",    font=small, fill=(255,255,255))
    d.text((170, 10),  "GREEN",  font=small, fill=(0,  0,  0  ))
    d.text((10,  170), "BLUE",   font=small, fill=(255,255,255))
    d.text((170, 170), "YELLOW", font=small, fill=(0,  0,  0  ))

    # Format number/name in center
    d.rectangle([60, 130, 259, 175], fill=(0,0,0))
    d.text((70, 135), label, font=small, fill=(255,255,255))

    return img

# ── USB helpers ───────────────────────────────────────────────────────────────
def ctrl(dev, data):
    for bmt, req in [(0x40,0x01),(0x40,0x00),(0x40,0x02)]:
        try:
            dev.ctrl_transfer(bmt, req, 0, 0, data, timeout=2000)
            return True
        except Exception:
            continue
    return False

def bulk(dev, data):
    for i in range(0, len(data), CHUNK):
        dev.write(EP_OUT, data[i:i+CHUNK], timeout=TIMEOUT)

def send_frame(dev, pixel_bytes):
    ctrl(dev, bytes([0x01, 0x00]))           # CMD_START
    bulk(dev, bytes(4))                       # CMD_TRIG
    bulk(dev, HEADER + pixel_bytes)           # header + pixels
    ctrl(dev, bytes([0x09, 0x00]))            # CMD_COMMIT

# ── Main ──────────────────────────────────────────────────────────────────────
def main():
    dev = usb.core.find(idVendor=VENDOR_ID, idProduct=PRODUCT_ID)
    if dev is None:
        print("ERROR: Display not found. Check USB connection.")
        sys.exit(1)

    try:
        if dev.is_kernel_driver_active(0):
            dev.detach_kernel_driver(0)
    except Exception:
        pass
    dev.set_configuration()

    # Sync x3
    for _ in range(3):
        ctrl(dev, bytes([0x33, 0xFF]) + bytes(62))
        time.sleep(0.05)
    print("Connected. Starting format test...\n")

    rotate = 270  # physical screen rotation

    for name, ch_order, endian in FORMATS:
        img    = make_test_image(name)
        img_r  = img.rotate(rotate, expand=False)
        pixels = encode(img_r, ch_order, endian)

        # Send 3 times so it stabilises
        for _ in range(3):
            send_frame(dev, pixels)
            time.sleep(0.05)

        print(f"Showing format {name}")
        print(f"  TL=RED  TR=GREEN  BL=BLUE  BR=YELLOW ?")
        ans = input("  Correct? (y/n/skip) [wait 3s or press enter]: ").strip().lower()
        if ans == 'y':
            print(f"\n✓ CORRECT FORMAT: {name}")
            print(f"  ch_order={ch_order}  endian={'little' if endian=='<' else 'big'}")
            break
        print()

    print("\nDone.")
    usb.util.dispose_resources(dev)

if __name__ == '__main__':
    main()