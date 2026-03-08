"""
PHASE 2 prober — runs after protocol_probe.py found no visual reaction.

When USB bulk writes are accepted but the screen stays blank, the cause is
almost always one of:

  A) A control transfer "wake-up" or "mode switch" is required FIRST.
  B) The display expects a specific multi-packet sequence (init + frame).
  C) The display uses a different USB interface or alternate setting.
  D) The frame data is correct but needs a "display on" command after it.
  E) The device silently ignores all data until it receives its own
     proprietary init blob (common in ChiZhu / Hua Jie display chips).

This script probes ALL of the above systematically.

USAGE (from project root, venv active):
    python tests/deep_probe.py

Watch the physical screen after EACH phase. Note the phase + step number
if any reaction occurs (even a flash, flicker, or brief image).
"""

import sys
import os
import time
import struct
import usb.core
import usb.util

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "src"))

VENDOR_ID  = 0x87AD
PRODUCT_ID = 0x70DB
EP_OUT     = 0x01
EP_IN      = 0x81
TIMEOUT    = 5000
CHUNK      = 512
W, H       = 320, 240


# ── device ────────────────────────────────────────────────────────────────────

def open_device():
    dev = usb.core.find(idVendor=VENDOR_ID, idProduct=PRODUCT_ID)
    if dev is None:
        print("ERROR: Device not found. Check USB and udev rule.")
        sys.exit(1)
    try:
        if dev.is_kernel_driver_active(0):
            dev.detach_kernel_driver(0)
    except Exception:
        pass
    dev.set_configuration()

    # Print full device descriptor for analysis
    print(f"\nDevice: {VENDOR_ID:#06x}:{PRODUCT_ID:#06x}")
    try:
        print(f"  Manufacturer : {dev.manufacturer}")
        print(f"  Product      : {dev.product}")
        print(f"  Serial       : {dev.serial_number}")
    except Exception:
        pass

    cfg = dev.get_active_configuration()
    print(f"  Num interfaces: {cfg.bNumInterfaces}")
    for intf in cfg:
        print(f"  Interface {intf.bInterfaceNumber} alt={intf.bAlternateSetting} "
              f"class={intf.bInterfaceClass:#04x} sub={intf.bInterfaceSubClass:#04x} "
              f"proto={intf.bInterfaceProtocol:#04x}")
        for ep in intf:
            direction = "IN " if (ep.bEndpointAddress & 0x80) else "OUT"
            xfer = {0: "CTRL", 1: "ISO", 2: "BULK", 3: "INT"}.get(
                ep.bmAttributes & 0x03, "?")
            print(f"    EP {ep.bEndpointAddress:#04x} {direction} {xfer} "
                  f"maxPacket={ep.wMaxPacketSize}")
    return dev


def send_bulk(dev, data: bytes, label: str = "") -> bool:
    try:
        for i in range(0, len(data), CHUNK):
            dev.write(EP_OUT, data[i:i+CHUNK], timeout=TIMEOUT)
        if label:
            print(f"  ✓ BULK {len(data)}B — {label}")
        return True
    except Exception as e:
        print(f"  ✗ BULK failed ({label}): {e}")
        return False


def ctrl(dev, bmReqType: int, bRequest: int, wValue: int = 0,
         wIndex: int = 0, data_or_len=None, label: str = "") -> bytes:
    """Send a control transfer and return any response bytes."""
    try:
        if isinstance(data_or_len, int):
            # IN transfer — read N bytes from device
            result = dev.ctrl_transfer(bmReqType, bRequest, wValue, wIndex,
                                       data_or_len, timeout=TIMEOUT)
            resp = bytes(result)
        else:
            # OUT transfer
            result = dev.ctrl_transfer(bmReqType, bRequest, wValue, wIndex,
                                       data_or_len or b"", timeout=TIMEOUT)
            resp = b""
        if label:
            tag = f"→ {resp.hex(' ')}" if resp else ""
            print(f"  ✓ CTRL {bmReqType:#04x} req={bRequest:#04x} "
                  f"val={wValue:#06x} idx={wIndex} {label} {tag}")
        return resp
    except Exception as e:
        print(f"  ✗ CTRL req={bRequest:#04x} val={wValue:#06x} ({label}): {e}")
        return b""


def read_in(dev, length: int = 64, label: str = "") -> bytes:
    try:
        data = bytes(dev.read(EP_IN, length, timeout=1500))
        print(f"  ← IN  {len(data)}B: {data.hex(' ')} {label}")
        return data
    except usb.core.USBTimeoutError:
        print(f"  ← IN  timeout {label}")
        return b""
    except Exception as e:
        print(f"  ← IN  error: {e} {label}")
        return b""


def red_frame_be() -> bytes:
    import numpy as np
    pixel = np.uint16(0xF800)  # pure red in RGB565 BE
    return np.full(W * H, pixel, dtype=">u2").tobytes()


def white_frame_be() -> bytes:
    import numpy as np
    return np.full(W * H, np.uint16(0xFFFF), dtype=">u2").tobytes()


def pause(seconds: float = 1.5):
    time.sleep(seconds)


# ─────────────────────────────────────────────────────────────────────────────
# PHASE A: Read device status / query commands
# ─────────────────────────────────────────────────────────────────────────────

def phase_a_read_device_status(dev):
    """
    Try common vendor IN control transfers to read device status.
    Many ChiZhu/HuaJie display chips respond to specific query commands
    with a status byte or version string that reveals the protocol.
    """
    print("\n" + "═"*60)
    print("PHASE A: Read device status / version via control IN transfers")
    print("═"*60)

    queries = [
        # (bmRequestType, bRequest, wValue, wIndex, length, label)
        (0xC0, 0x01, 0x0000, 0, 64,  "vendor-IN req=0x01"),
        (0xC0, 0x02, 0x0000, 0, 64,  "vendor-IN req=0x02"),
        (0xC0, 0x06, 0x0000, 0, 64,  "vendor-IN req=0x06"),
        (0xC0, 0x06, 0x0100, 0, 18,  "GET_DESCRIPTOR device"),
        (0xC0, 0x06, 0x0200, 0, 255, "GET_DESCRIPTOR config"),
        (0xC0, 0x40, 0x0000, 0, 64,  "vendor-IN req=0x40"),
        (0xC0, 0x41, 0x0000, 0, 64,  "vendor-IN req=0x41"),
        (0xC0, 0x50, 0x0000, 0, 64,  "vendor-IN req=0x50"),
        (0xC0, 0x51, 0x0000, 0, 64,  "vendor-IN req=0x51"),
        (0xC0, 0x60, 0x0000, 0, 64,  "vendor-IN req=0x60"),
        (0xC0, 0x80, 0x0000, 0, 64,  "vendor-IN req=0x80"),
        (0xC0, 0x90, 0x0000, 0, 64,  "vendor-IN req=0x90"),
        (0xC0, 0xA0, 0x0000, 0, 64,  "vendor-IN req=0xA0"),
        (0xC0, 0xB0, 0x0000, 0, 64,  "vendor-IN req=0xB0"),
        (0xC0, 0xB5, 0x0000, 0, 64,  "vendor-IN req=0xB5"),
        (0xC0, 0xC0, 0x0000, 0, 64,  "vendor-IN req=0xC0"),
        (0xC0, 0xFF, 0x0000, 0, 64,  "vendor-IN req=0xFF"),
        # Class requests
        (0xA1, 0x00, 0x0000, 0, 64,  "class-IN req=0x00"),
        (0xA1, 0x01, 0x0000, 0, 64,  "class-IN req=0x01"),
    ]

    found = []
    for bmt, req, val, idx, length, label in queries:
        resp = ctrl(dev, bmt, req, val, idx, length, label)
        if resp and any(b != 0 for b in resp):
            print(f"  ★ NON-ZERO RESPONSE: {resp.hex(' ')}")
            found.append((label, resp))
        time.sleep(0.05)

    if found:
        print(f"\n  ★ Device responded to {len(found)} query/queries:")
        for lbl, data in found:
            print(f"    {lbl}: {data.hex(' ')}")
    else:
        print("\n  No non-trivial responses to any query.")
    return found


# ─────────────────────────────────────────────────────────────────────────────
# PHASE B: Control OUT init sequences + immediate frame
# ─────────────────────────────────────────────────────────────────────────────

def phase_b_ctrl_init_then_frame(dev):
    """
    Send various control OUT sequences followed immediately by a bulk frame.
    This tests the hypothesis that the display needs a 'wake' command.
    """
    print("\n" + "═"*60)
    print("PHASE B: Control OUT init → immediate bulk frame")
    print("═"*60)

    red = red_frame_be()

    sequences = [
        # Each entry: list of (bmReqType, req, val, idx, data) followed by bulk send
        {
            "name": "req01_wakeup",
            "steps": [(0x40, 0x01, 0x0001, 0, b"")],
        },
        {
            "name": "req01_then_02",
            "steps": [(0x40, 0x01, 0x0000, 0, b""), (0x40, 0x02, 0x0000, 0, b"")],
        },
        {
            "name": "display_on_B0",
            "steps": [(0x40, 0xB0, 0x0001, 0, b"")],
        },
        {
            "name": "display_on_B5",
            "steps": [(0x40, 0xB5, 0x0001, 0, b"")],
        },
        {
            "name": "mode_switch_req01_val0001",
            "steps": [(0x40, 0x01, 0x0001, 0, b""), (0x40, 0x01, 0x0002, 0, b"")],
        },
        {
            "name": "vid_pid_payload",
            "steps": [(0x40, 0x01, 0x0000, 0, bytes([0x87,0xAD,0x70,0xDB]))],
        },
        {
            "name": "resolution_payload_BE",
            "steps": [(0x40, 0x01, 0x0000, 0,
                       struct.pack(">HH", W, H))],
        },
        {
            "name": "resolution_payload_LE",
            "steps": [(0x40, 0x01, 0x0000, 0,
                       struct.pack("<HH", W, H))],
        },
        {
            "name": "init_blob_common",
            "steps": [(0x40, 0x01, 0x0000, 0,
                       bytes([0x87,0xAD,0x70,0xDB, 0x01,0x40,0x00,0xF0, 0x10,0x00]))],
        },
        {
            "name": "req40_display_go",
            "steps": [(0x40, 0x40, 0x0000, 0, b"")],
        },
        {
            "name": "req_0x11_display_on",
            "steps": [(0x40, 0x11, 0x0000, 0, b"")],
        },
        {
            "name": "req_0x29_display_on",  # ILI9341 DISPON = 0x29
            "steps": [(0x40, 0x29, 0x0000, 0, b"")],
        },
    ]

    for seq in sequences:
        print(f"\n  ── {seq['name']}")
        for bmt, req, val, idx, data in seq["steps"]:
            ctrl(dev, bmt, req, val, idx, data, "")
        time.sleep(0.05)
        send_bulk(dev, red, f"red after {seq['name']}")
        read_in(dev, 64)
        pause(1.5)


# ─────────────────────────────────────────────────────────────────────────────
# PHASE C: Alternate interface settings
# ─────────────────────────────────────────────────────────────────────────────

def phase_c_alternate_interfaces(dev):
    """
    Some USB display chips have multiple alternate interface settings.
    Try switching to each and sending a frame.
    """
    print("\n" + "═"*60)
    print("PHASE C: Alternate interface settings")
    print("═"*60)

    red = red_frame_be()

    for alt in range(0, 4):
        try:
            dev.set_interface_altsetting(interface=0, alternate_setting=alt)
            print(f"  Set interface 0 alt={alt} — OK")
            time.sleep(0.1)
            send_bulk(dev, red, f"red on alt={alt}")
            read_in(dev, 64)
            pause(1.5)
        except usb.core.USBError as e:
            print(f"  Alt setting {alt}: {e}")


# ─────────────────────────────────────────────────────────────────────────────
# PHASE D: Known ChiZhu / HuaJie proprietary init sequences
# ─────────────────────────────────────────────────────────────────────────────

def phase_d_proprietary_init(dev):
    """
    ChiZhu Tech (also marketed as HuaJie, CoolGo, various generic OEMs)
    uses a specific init blob sent as a bulk OUT packet BEFORE the frame.

    These init sequences are reverse-engineered from similar devices.
    Trying all known variants.
    """
    print("\n" + "═"*60)
    print("PHASE D: ChiZhu / HuaJie proprietary init sequences")
    print("═"*60)

    red   = red_frame_be()
    white = white_frame_be()

    # Each entry is (name, init_bytes_to_send_before_frame)
    known_inits = [

        # ── ChiZhu variant 1: 64-byte init with magic 0x55 0xAA ─────────────
        ("chizhou_55AA_64b",
         bytes([0x55, 0xAA,
                0x00, 0x00, 0x00, 0x00, 0x01, 0x40, 0x00, 0xF0,
                0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])),

        # ── ChiZhu variant 2: 12-byte init with device VID/PID ──────────────
        ("chizhou_vidpid_init",
         bytes([0x87, 0xAD, 0x70, 0xDB,
                0x01, 0x40, 0x00, 0xF0,   # width=320 height=240 BE
                0x10, 0x00, 0x00, 0x00])), # bit depth=16

        # ── ChiZhu variant 3: capture header as init packet ─────────────────
        ("capture_header_as_init",
         bytes([0x1b,0x00,0x10,0x90,0xd4,0x43,0x0e,0xb2,0xff,0xff,0x00,0x00])),

        # ── AIDA64 LCD smartie 8-byte init ───────────────────────────────────
        ("aida64_8b_init",
         bytes([0x01, 0x00, 0x01, 0x40, 0x00, 0xF0, 0x10, 0x00])),

        # ── ScreenStreamer / generic USB LCD init (16 bytes) ─────────────────
        ("screenstreamer_init",
         bytes([0x55, 0xAA, 0x00, 0x01,
                0x01, 0x40, 0x00, 0xF0,
                0x00, 0x10, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00])),

        # ── 4-byte "start frame" command ─────────────────────────────────────
        ("start_cmd_4b_v1",   bytes([0x00, 0x00, 0x00, 0x01])),
        ("start_cmd_4b_v2",   bytes([0x01, 0x00, 0x00, 0x00])),
        ("start_cmd_4b_v3",   bytes([0xFF, 0xFF, 0xFF, 0xFF])),

        # ── 8-byte frame start with resolution ───────────────────────────────
        ("res_le_8b",  struct.pack("<BBHH", 0x55, 0xAA, W, H) + b"\x10\x00"),
        ("res_be_8b",  struct.pack(">BBHH", 0x55, 0xAA, W, H) + b"\x10\x00"),

        # ── 512-byte init blob (first packet = all zeros padding) ────────────
        ("null_512b_init", bytes(512)),

        # ── CoolGo / XCY USB display init (seen in Linux kernel patches) ─────
        ("coolgo_init",
         bytes([0x10, 0x00, 0x01, 0x40, 0x00, 0xF0,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00])),

        # ── Allwinner / Rockchip USB display bridge ───────────────────────────
        ("allwinner_init",
         bytes([0xAA, 0x55, 0x00, 0x00,
                0x40, 0x01, 0xF0, 0x00,   # W=320 H=240 LE
                0x10, 0x00, 0x00, 0x00])),

        # ── Two-packet sequence: init then frame ─────────────────────────────
        # (handled in the loop below by sending init then frame separately)
    ]

    for name, init_bytes in known_inits:
        print(f"\n  ── {name}")
        # Send init
        send_bulk(dev, init_bytes, f"init")
        read_in(dev, 64)
        time.sleep(0.05)
        # Send red frame
        send_bulk(dev, red, "red frame")
        read_in(dev, 64)
        pause(1.5)
        # Send white frame to confirm if red was shown
        send_bulk(dev, white, "white frame (confirm)")
        read_in(dev, 64)
        pause(1.0)


# ─────────────────────────────────────────────────────────────────────────────
# PHASE E: Frame as single large write (no chunking)
# ─────────────────────────────────────────────────────────────────────────────

def phase_e_single_write(dev):
    """
    Some USB bridge chips need the entire frame in ONE write call,
    not split into 512-byte chunks. Test this directly.
    """
    print("\n" + "═"*60)
    print("PHASE E: Single-write frame (no chunking)")
    print("═"*60)

    red = red_frame_be()

    for chunk_size in [512, 1024, 2048, 4096, 16384, len(red)]:
        label = f"single write chunk={chunk_size}"
        print(f"\n  ── {label}")
        try:
            offset = 0
            while offset < len(red):
                part = red[offset:offset+chunk_size]
                dev.write(EP_OUT, part, timeout=TIMEOUT)
                offset += chunk_size
            print(f"  ✓ Sent {len(red)} bytes in {chunk_size}-byte writes")
        except Exception as e:
            print(f"  ✗ Failed: {e}")
        pause(1.5)


# ─────────────────────────────────────────────────────────────────────────────
# PHASE F: Read the Windows pcapng capture if present
# ─────────────────────────────────────────────────────────────────────────────

def phase_f_check_capture():
    """Remind user to capture Windows traffic — most reliable path."""
    print("\n" + "═"*60)
    print("PHASE F: Windows USB capture (manual step)")
    print("═"*60)
    print("""
  Since no automatic variant worked, the next step is to capture
  exactly what the Windows software sends.

  Two options:

  OPTION 1 — Linux usbmon (preferred, no Windows needed):
  ────────────────────────────────────────────────────────
  1. Install the Windows software in a VM (VirtualBox/QEMU)
     with USB passthrough for device 87ad:70db.
  2. On the Linux HOST, run:
       sudo modprobe usbmon
       lsusb | grep 87ad     ← note bus number, e.g. Bus 001
       sudo wireshark -i usbmon1   ← use bus number
  3. Start the Windows software inside the VM.
  4. In Wireshark filter: usb.idVendor == 0x87ad
  5. Save as: capture_display.pcapng in project root.
  6. Run:  python tests/replay_windows_capture.py capture_display.pcapng

  OPTION 2 — Windows + Wireshark + USBPcap:
  ────────────────────────────────────────────────────────
  1. Install Wireshark + USBPcap on Windows.
  2. Start capture on USBPcap interface.
  3. Open the Thermalright software.
  4. Save as capture_display.pcapng.
  5. Transfer to Linux and run replay script.

  The replay script will send byte-for-byte what Windows sends.
  Once the screen reacts, it will print the exact init sequence.
""")


# ─────────────────────────────────────────────────────────────────────────────
# PHASE G: Interrupt endpoint check
# ─────────────────────────────────────────────────────────────────────────────

def phase_g_check_interrupt(dev):
    """
    Some devices that appear as BULK actually expect an interrupt-based
    handshake on a separate endpoint. Check if there is an interrupt EP
    we haven't used yet.
    """
    print("\n" + "═"*60)
    print("PHASE G: Check for interrupt endpoints and spontaneous data")
    print("═"*60)

    cfg = dev.get_active_configuration()
    for intf in cfg:
        for ep in intf:
            xfer_type = ep.bmAttributes & 0x03
            if xfer_type == 3:  # INTERRUPT
                addr = ep.bEndpointAddress
                direction = "IN" if (addr & 0x80) else "OUT"
                print(f"  Found INTERRUPT EP {addr:#04x} {direction}")
                if direction == "IN":
                    print("  Trying to read interrupt EP...")
                    try:
                        data = bytes(dev.read(addr, 64, timeout=2000))
                        print(f"  ← Interrupt IN: {data.hex(' ')}")
                    except Exception as e:
                        print(f"  ← No interrupt data: {e}")

    # Also try reading EP 0x81 with a longer timeout right after a frame send
    print("\n  Sending red frame then immediately reading EP 0x81 (500ms timeout)...")
    red = red_frame_be()
    send_bulk(dev, red, "red for interrupt check")
    for _ in range(5):
        data = b""
        try:
            data = bytes(dev.read(EP_IN, 512, timeout=500))
        except Exception:
            pass
        if data:
            print(f"  ← EP 0x81 replied: {data.hex(' ')}")
        else:
            print("  ← No response on EP 0x81")
        time.sleep(0.1)


# ─────────────────────────────────────────────────────────────────────────────
# PHASE H: Null / padding sensitivity test
# ─────────────────────────────────────────────────────────────────────────────

def phase_h_padding_test(dev):
    """
    Some USB display bridges require the total transfer to be padded
    to a specific size (e.g., 204864 bytes as seen in the capture).
    204864 - 153600 = 51264 bytes of overhead/padding.
    Try sending exactly 204864 bytes.
    """
    print("\n" + "═"*60)
    print("PHASE H: Capture-size padding test (204864 bytes)")
    print("═"*60)

    red = red_frame_be()   # 153600 bytes

    # Pad to 204864 (captured transfer size)
    TARGET = 204864
    padding_needed = TARGET - len(red)
    print(f"  Frame = {len(red)} bytes")
    print(f"  Target = {TARGET} bytes")
    print(f"  Padding = {padding_needed} bytes")

    # Try: frame + zeros
    padded_zeros = red + bytes(padding_needed)
    print(f"\n  ── Frame + zero padding ({len(padded_zeros)} bytes)")
    send_bulk(dev, padded_zeros, "frame+zeros")
    pause(1.5)

    # Try: zeros + frame (header before data)
    padded_prefix = bytes(padding_needed) + red
    print(f"\n  ── Zero prefix + frame ({len(padded_prefix)} bytes)")
    send_bulk(dev, padded_prefix, "zeros+frame")
    pause(1.5)

    # Try: 51264-byte init blob + frame
    # Split the overhead as: first 512 bytes init command, rest padding
    init_512 = bytes([0x55, 0xAA] + [0x00]*510)
    remaining_pad = bytes(padding_needed - 512)
    padded_struct = init_512 + remaining_pad + red
    print(f"\n  ── Structured header ({len(padded_struct)} bytes)")
    send_bulk(dev, padded_struct, "structured header")
    pause(1.5)

    # Try exactly 51264-byte header (capture overhead)
    header_51264 = bytes(padding_needed)
    padded_header = header_51264 + red
    print(f"\n  ── 51264-byte null header + frame ({len(padded_header)} bytes)")
    send_bulk(dev, padded_header, "51264 null header + frame")
    pause(1.5)


# ─────────────────────────────────────────────────────────────────────────────
# Main
# ─────────────────────────────────────────────────────────────────────────────

def main():
    print("=" * 60)
    print("  TRCC Deep Protocol Prober")
    print("  Device must be connected. Watch screen for ANY reaction.")
    print("=" * 60)

    dev = open_device()

    phase_a_read_device_status(dev)
    phase_b_ctrl_init_then_frame(dev)
    phase_c_alternate_interfaces(dev)
    phase_d_proprietary_init(dev)
    phase_e_single_write(dev)
    phase_g_check_interrupt(dev)
    phase_h_padding_test(dev)
    phase_f_check_capture()

    print("\n" + "="*60)
    print("  Deep probe complete.")
    print("  If screen showed ANY reaction, note the phase and step name.")
    print("  Share the full output of this script for next analysis.")
    print("="*60)


if __name__ == "__main__":
    main()
