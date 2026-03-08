#!/usr/bin/env python3
# tests/test_display.py
# Quick end-to-end test: shows metrics layout with fake data, then real data

import sys, os, time
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "src"))

import usb.core, usb.util, struct, numpy as np
from PIL import Image, ImageDraw

VENDOR_ID=0x87AD; PRODUCT_ID=0x70DB; EP=0x01; CHUNK=512
CMD_SYNC   = bytes([0x33,0xFF])+bytes(62)
CMD_START  = bytes([0x01,0x00])
CMD_COMMIT = bytes([0x09,0x00])
CMD_TRIG   = bytes(4)
W=H=320

def connect():
    dev = usb.core.find(idVendor=VENDOR_ID, idProduct=PRODUCT_ID)
    if dev is None: raise RuntimeError("Screen not found")
    try:
        if dev.is_kernel_driver_active(0): dev.detach_kernel_driver(0)
    except: pass
    dev.set_configuration()
    return dev

def ctrl(dev, data):
    for bmt,req in [(0x40,0x01),(0x40,0x00),(0x40,0x02)]:
        try: dev.ctrl_transfer(bmt,req,0,0,data,timeout=2000); return
        except: continue

def bulk(dev, data):
    for i in range(0,len(data),CHUNK): dev.write(EP,data[i:i+CHUNK],timeout=10000)

def encode(img):
    img = img.resize((W,H), Image.LANCZOS).convert('RGB')
    img = img.rotate(270, expand=False)  # compensate physical 90° CW mount
    arr = np.array(img, dtype=np.uint16)
    # GRB565 — CONFIRMED channel order
    px = ((arr[:,:,1]>>3)<<11) | ((arr[:,:,0]>>2)<<5) | (arr[:,:,2]>>3)
    raw = px.flatten().astype('<u2').tobytes()
    target = 0x32000-0xD0
    return (raw+bytes(target))[:target]

def make_header():
    hdr=bytearray(0xD0)
    struct.pack_into('<I',hdr,0x00,0x78563412)
    struct.pack_into('<I',hdr,0x04,0x00000003)
    struct.pack_into('<I',hdr,0x08,W)
    struct.pack_into('<I',hdr,0x0C,W)
    struct.pack_into('<I',hdr,0x10,H)
    struct.pack_into('<I',hdr,0x38,0x00000002)
    struct.pack_into('<I',hdr,0x3C,0x00032000)
    struct.pack_into('<I',hdr,0xC4,0x00002000)
    return bytes(hdr)

HDR = make_header()

def send(dev, img):
    raw = encode(img)
    ctrl(dev, CMD_START)
    bulk(dev, CMD_TRIG)
    bulk(dev, HDR+raw)
    ctrl(dev, CMD_COMMIT)

def build_metrics(cpu_temp, gpu_temp, cpu_usage, gpu_usage,
                  cpu_freq=0, gpu_freq=0, cpu_pwr=0, gpu_pwr=0):
    img = Image.new('RGB',(W,H),(10,10,25))
    d = ImageDraw.Draw(img)

    # Header
    d.rectangle([0,0,W,36], fill=(20,20,45))
    d.text((10,8),"CPU", fill=(0,180,255))
    d.text((W//2+10,8),"GPU", fill=(255,100,0))
    d.line([(0,36),(W,36)], fill=(50,50,80))
    d.line([(W//2,0),(W//2,H)], fill=(50,50,80))

    def temp_col(t, mx=90):
        f = min(1.0, max(0.0,(t-30)/(mx-30)))
        if f<0.5: return (int(510*f), int(255-75*f*2), int(255*(1-f*2)))
        f2=(f-0.5)*2
        return (255, int(200*(1-f2)), 0)

    def bar(x,y,w,h,val,col):
        d.rectangle([x,y,x+w,y+h], fill=(40,40,60))
        fw=int(w*min(1.0,val/100))
        if fw>0: d.rectangle([x,y,x+fw,y+h], fill=col)

    # CPU
    d.text((10,45), f"{cpu_temp}°", fill=temp_col(cpu_temp))
    d.text((10,118),"LOAD", fill=(120,120,120))
    bar(10,134,140,12,cpu_usage,(0,180,255))
    d.text((10,148), f"{cpu_usage}%", fill=(220,220,220))
    d.text((10,182),"FREQ", fill=(120,120,120))
    fstr=f"{cpu_freq/1000:.1f}G" if cpu_freq>=1000 else f"{cpu_freq}M"
    d.text((10,198), fstr, fill=(0,180,255))
    d.text((10,232),"PWR", fill=(120,120,120))
    d.text((10,248), f"{cpu_pwr}W", fill=(220,220,220))

    # GPU
    ox=W//2+10
    d.text((ox,45), f"{gpu_temp}°", fill=temp_col(gpu_temp))
    d.text((ox,118),"LOAD", fill=(120,120,120))
    bar(ox,134,140,12,gpu_usage,(255,100,0))
    d.text((ox,148), f"{gpu_usage}%", fill=(220,220,220))
    d.text((ox,182),"FREQ", fill=(120,120,120))
    fstr=f"{gpu_freq/1000:.1f}G" if gpu_freq>=1000 else f"{gpu_freq}M"
    d.text((ox,198), fstr, fill=(255,100,0))
    d.text((ox,232),"PWR", fill=(120,120,120))
    d.text((ox,248), f"{gpu_pwr}W", fill=(220,220,220))

    return img

print("Connecting...")
dev = connect()

print("Syncing...")
for _ in range(3):
    ctrl(dev, CMD_SYNC)
    time.sleep(0.1)

print("Sending test metrics (fake data)...")
import math
t0 = time.time()
frame=0
try:
    while True:
        t = time.time()-t0
        # Animate values so we can see it's live
        cpu_temp = int(45 + 30*abs(math.sin(t*0.3)))
        gpu_temp = int(55 + 25*abs(math.sin(t*0.2+1)))
        cpu_usage= int(20 + 60*abs(math.sin(t*0.5)))
        gpu_usage= int(30 + 50*abs(math.sin(t*0.4+0.5)))
        cpu_freq = 3600
        gpu_freq = 2400
        cpu_pwr  = int(65 + 40*abs(math.sin(t*0.3)))
        gpu_pwr  = int(120+ 80*abs(math.sin(t*0.25)))

        img = build_metrics(cpu_temp,gpu_temp,cpu_usage,gpu_usage,
                           cpu_freq,gpu_freq,cpu_pwr,gpu_pwr)
        send(dev, img)
        frame+=1
        if frame%30==0:
            fps = frame/(time.time()-t0)
            print(f"  frame {frame}, {fps:.1f} fps, CPU={cpu_temp}° {cpu_usage}%  GPU={gpu_temp}° {gpu_usage}%")
except KeyboardInterrupt:
    print(f"\nStopped after {frame} frames. FPS={frame/(time.time()-t0):.1f}")