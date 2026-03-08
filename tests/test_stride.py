"""
Scanline offset finder.
Sends an image with numbered horizontal stripes every 20px.
We can read exactly which stripe number appears at the top of the screen,
which tells us the exact byte offset the display starts reading from.
"""
import sys, os, time, struct
import numpy as np
from PIL import Image, ImageDraw, ImageFont
import usb.core, usb.util

VID=0x87AD; PID=0x70DB; EP=0x01; CHUNK=512
W = H = 320
PIXEL_BYTES = W * H * 2

def make_header():
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

HEADER = make_header()

STRIPE_COLORS = [
    (220,0,0),(0,200,0),(0,0,220),(220,220,0),
    (220,0,220),(0,220,220),(220,120,0),(120,220,0),
    (0,120,220),(220,0,120),(180,180,180),(100,100,255),
    (255,100,100),(100,255,100),(200,150,50),(50,150,200),
]

def make_ruler():
    img = Image.new('RGB', (W, H), (0,0,0))
    d = ImageDraw.Draw(img)
    try: f=ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf", 14)
    except: f=ImageFont.load_default()
    
    stripe_h = 20
    for row in range(H // stripe_h):
        y0 = row * stripe_h
        y1 = y0 + stripe_h - 1
        color = STRIPE_COLORS[row % len(STRIPE_COLORS)]
        d.rectangle([0, y0, W-1, y1], fill=color)
        # Label every stripe with its row number and pixel offset
        offset = row * stripe_h * W * 2
        d.text((4, y0+2), f"row{row*20:03d} off={offset}", font=f, fill=(0,0,0))
        d.text((3, y0+1), f"row{row*20:03d} off={offset}", font=f, fill=(255,255,255))
    return img

def encode(img):
    arr = np.array(img.convert('RGB'), dtype=np.uint8)
    px = ((arr[:,:,0].astype(np.uint16)>>3)<<11) | \
         ((arr[:,:,1].astype(np.uint16)>>2)<<5)  | \
          (arr[:,:,2].astype(np.uint16)>>3)
    return px.flatten().astype('>u2').tobytes()

def ctrl(dev, data):
    for bmt,req in [(0x40,0x01),(0x40,0x00),(0x40,0x02)]:
        try: dev.ctrl_transfer(bmt,req,0,0,data,timeout=3000); return
        except: continue

def send(dev, pixels):
    ctrl(dev, bytes([0x01,0x00]))
    dev.write(EP, bytes(4), timeout=10000)
    data = HEADER + pixels
    for i in range(0, len(data), CHUNK):
        dev.write(EP, data[i:i+CHUNK], timeout=10000)
    ctrl(dev, bytes([0x09,0x00]))

dev = usb.core.find(idVendor=VID, idProduct=PID)
if not dev: print("Not found"); sys.exit(1)
try:
    if dev.is_kernel_driver_active(0): dev.detach_kernel_driver(0)
except: pass
dev.set_configuration()
ctrl(dev, bytes([0x33,0xFF])+bytes(62)); time.sleep(0.1)

img = make_ruler()
px  = encode(img)
for _ in range(5): send(dev, px); time.sleep(0.05)

print("Ruler sent. Look at the PHYSICAL SCREEN.")
print()
print("Each stripe is 20px tall, labeled with its row number and byte offset.")
print("The stripes cycle through 16 colors.")
print()
print("Question: What label/number do you see at the VERY TOP of the screen?")
top = input("Top stripe label (e.g. 'row200 off=...'): ").strip()
print()
print("Question: What label/number do you see at the VERY BOTTOM of the screen?")
bot = input("Bottom stripe label: ").strip()
print()
print(f"Top='{top}'  Bottom='{bot}'")
print("Tell Claude these values — this gives the exact scanline offset.")

usb.util.dispose_resources(dev)