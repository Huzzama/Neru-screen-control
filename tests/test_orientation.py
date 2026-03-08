"""
Orientation tester — tries all 8 flip/rotate combos.
Run: python3 test_orientation.py
Watch screen, type the number that shows RED top-left, GREEN top-right,
BLUE bottom-left, YELLOW bottom-right — all labels readable upright.
"""
import sys, os, time, struct
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "src"))

import numpy as np
from PIL import Image, ImageDraw, ImageFont
import usb.core, usb.util

VENDOR_ID = 0x87AD; PRODUCT_ID = 0x70DB; EP_OUT = 0x01
W = H = 320

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

def encode_rgb565be(img):
    arr = np.array(img.convert('RGB'), dtype=np.uint16)
    px = ((arr[:,:,0]>>3)<<11) | ((arr[:,:,1]>>2)<<5) | (arr[:,:,2]>>3)
    return px.flatten().astype('>u2').tobytes()

def ctrl(dev, data):
    for bmt, req in [(0x40,0x01),(0x40,0x00),(0x40,0x02)]:
        try: dev.ctrl_transfer(bmt, req, 0, 0, data, timeout=2000); return True
        except: continue
    return False

def send(dev, pixels):
    ctrl(dev, bytes([0x01,0x00]))
    for i in range(0, 4, 512): dev.write(EP_OUT, bytes(4), timeout=10000)
    data = HEADER + pixels
    for i in range(0, len(data), 512): dev.write(EP_OUT, data[i:i+512], timeout=10000)
    ctrl(dev, bytes([0x09,0x00]))

def make_test(label):
    img = Image.new('RGB', (W,H), (0,0,0))
    d = ImageDraw.Draw(img)
    try: f = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf", 28)
    except: f = ImageFont.load_default()
    d.rectangle([0,0,159,159],   fill=(255,0,0));   d.text((20,20),  "RED",    font=f, fill=(255,255,255))
    d.rectangle([160,0,319,159], fill=(0,200,0));   d.text((180,20), "GREEN",  font=f, fill=(255,255,255))
    d.rectangle([0,160,159,319], fill=(0,0,255));   d.text((20,180), "BLUE",   font=f, fill=(255,255,255))
    d.rectangle([160,160,319,319],fill=(255,255,0)); d.text((180,180),"YELLOW",font=f, fill=(0,0,0))
    d.rectangle([80,130,239,165], fill=(0,0,0))
    try: sf=ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",20)
    except: sf=f
    d.text((85,133), label, font=sf, fill=(255,255,255))
    return img

# All 8 transform combos
TRANSFORMS = [
    ("1  no transform",          lambda i: i),
    ("2  flip H",                lambda i: i.transpose(Image.FLIP_LEFT_RIGHT)),
    ("3  flip V",                lambda i: i.transpose(Image.FLIP_TOP_BOTTOM)),
    ("4  rotate 90",             lambda i: i.rotate(90,  expand=False)),
    ("5  rotate 180",            lambda i: i.rotate(180, expand=False)),
    ("6  rotate 270",            lambda i: i.rotate(270, expand=False)),
    ("7  rotate 90  + flip V",   lambda i: i.rotate(90,  expand=False).transpose(Image.FLIP_TOP_BOTTOM)),
    ("8  rotate 270 + flip V",   lambda i: i.rotate(270, expand=False).transpose(Image.FLIP_TOP_BOTTOM)),
]

dev = usb.core.find(idVendor=VENDOR_ID, idProduct=PRODUCT_ID)
if not dev: print("Device not found!"); sys.exit(1)
try:
    if dev.is_kernel_driver_active(0): dev.detach_kernel_driver(0)
except: pass
dev.set_configuration()
for _ in range(3): ctrl(dev, bytes([0x33,0xFF])+bytes(62)); time.sleep(0.05)
print("Connected.\n")
print("Look for: RED=top-left  GREEN=top-right  BLUE=bottom-left  YELLOW=bottom-right")
print("All labels should be readable (not rotated/mirrored)\n")

for name, tfm in TRANSFORMS:
    img = make_test(name)
    img = tfm(img)
    pixels = encode_rgb565be(img)
    for _ in range(3): send(dev, pixels); time.sleep(0.03)
    print(f"Showing: {name}")
    ans = input("  Correct layout? (y=yes / enter=next): ").strip().lower()
    if ans == 'y':
        print(f"\n✓ CORRECT TRANSFORM: {name}")
        break
    print()

usb.util.dispose_resources(dev)