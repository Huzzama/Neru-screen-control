"""
Extracts BULK OUT packets from a Wireshark .pcapng capture and replays
them directly to the ChiZhu display.

This is the most reliable way to get the screen working: replaying exactly
what the Windows driver sent guarantees the screen will react if the USB
transfer itself is correct.

PREREQUISITES:
    pip install pyshark   (requires tshark / Wireshark CLI tools)
    sudo apt install tshark

USAGE:
    python tests/replay_windows_capture.py  capture_display.pcapng
    python tests/replay_windows_capture.py  capture_display.pcapng  --dry-run
    python tests/replay_windows_capture.py  capture_display.pcapng  --first 5
    python tests/replay_windows_capture.py  capture_display.pcapng  --decode-header

OPTIONS:
    --dry-run        Parse and print packets but do not send to device.
    --first N        Only replay the first N bulk OUT packets.
    --decode-header  Print hex + interpretation of first 64 bytes of each packet.
    --delay MS       Delay between packets in milliseconds (default 50).

HOW TO CAPTURE ON LINUX (no Windows needed if you already have the pcapng):
    sudo modprobe usbmon
    # Find the bus number from: lsusb | grep 87ad
    # e.g. "Bus 001 Device 009" → bus 1 → usbmon1
    sudo wireshark -i usbmon1 -k
    # Filter: usb.idVendor == 0x87ad
    # Start the Windows software (in VM with USB passthrough), let it run
    # Save as capture_display.pcapng in project root
"""

import sys
import os
import time
import argparse

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "src"))

VENDOR_ID  = 0x87AD
PRODUCT_ID = 0x70DB
EP_OUT     = 0x01
TIMEOUT    = 5000
CHUNK      = 512


def open_device():
    import usb.core
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
    print(f"Device opened.")
    return dev


def send_to_device(dev, data: bytes) -> None:
    for i in range(0, len(data), CHUNK):
        dev.write(EP_OUT, data[i:i+CHUNK], timeout=TIMEOUT)


def decode_header(data: bytes) -> str:
    """Attempt to interpret the first 16 bytes of a packet."""
    if len(data) < 4:
        return "  (too short)"
    lines = [f"  Hex: {data[:16].hex(' ')}"]
    if len(data) >= 2:
        lines.append(f"  [0:2] magic = {data[0]:02x} {data[1]:02x}")
    if len(data) >= 4:
        import struct
        val_be = struct.unpack(">I", data[:4])[0]
        val_le = struct.unpack("<I", data[:4])[0]
        lines.append(f"  [0:4] as uint32 BE={val_be}  LE={val_le}")
    if len(data) >= 6:
        w_be = (data[2] << 8) | data[3]
        h_be = (data[4] << 8) | data[5]
        w_le = data[2] | (data[3] << 8)
        h_le = data[4] | (data[5] << 8)
        lines.append(f"  [2:6] as WxH BE={w_be}x{h_be}  LE={w_le}x{h_le}")
    lines.append(f"  Total size: {len(data)} bytes")
    expected_565 = 320 * 240 * 2
    expected_888 = 320 * 240 * 3
    if len(data) > expected_565:
        hdr_size = len(data) - expected_565
        lines.append(f"  → If RGB565: header would be {hdr_size} bytes")
    if len(data) > expected_888:
        hdr_size = len(data) - expected_888
        lines.append(f"  → If RGB888: header would be {hdr_size} bytes")
    return "\n".join(lines)


def extract_bulk_out_packets(pcapng_path: str) -> list[bytes]:
    """Extract BULK OUT payload bytes from a pcapng file using pyshark."""
    try:
        import pyshark
    except ImportError:
        print("ERROR: pyshark not installed.")
        print("Install with: pip install pyshark")
        print("Also requires: sudo apt install tshark")
        sys.exit(1)

    print(f"Opening capture: {pcapng_path}")
    packets = []

    cap = pyshark.FileCapture(
        pcapng_path,
        display_filter="usb.transfer_type == 0x03",  # BULK
        keep_packets=False,
    )

    for pkt in cap:
        try:
            usb_layer = pkt.usb
            # Only BULK OUT (direction = host→device)
            endpoint = int(usb_layer.endpoint_address, 16)
            direction = (endpoint & 0x80)
            if direction != 0:
                continue  # skip IN packets

            # Extract data payload
            if hasattr(pkt, "data"):
                raw_hex = pkt.data.data.replace(":", "")
                packets.append(bytes.fromhex(raw_hex))
        except Exception:
            continue

    cap.close()
    print(f"Extracted {len(packets)} BULK OUT packet(s).")
    return packets


def main():
    parser = argparse.ArgumentParser(description="Replay Windows USB capture to ChiZhu display")
    parser.add_argument("pcapng", help="Path to .pcapng capture file")
    parser.add_argument("--dry-run",       action="store_true", help="Parse only, do not send")
    parser.add_argument("--first",         type=int, default=0,  help="Only replay first N packets")
    parser.add_argument("--decode-header", action="store_true",  help="Print header interpretation")
    parser.add_argument("--delay",         type=int, default=50, help="Delay between packets in ms")
    args = parser.parse_args()

    packets = extract_bulk_out_packets(args.pcapng)

    if not packets:
        print("No BULK OUT packets found. Check capture filter or file.")
        sys.exit(1)

    if args.first > 0:
        packets = packets[:args.first]
        print(f"Limiting to first {args.first} packet(s).")

    dev = None
    if not args.dry_run:
        dev = open_device()

    print(f"\nReplaying {len(packets)} packet(s):\n")
    for i, pkt in enumerate(packets):
        size_tag = ""
        if len(pkt) == 320 * 240 * 2:
            size_tag = "  ← exact RGB565 frame"
        elif len(pkt) == 320 * 240 * 2 + 12:
            size_tag = "  ← RGB565 + 12-byte header"
        elif len(pkt) == 320 * 240 * 3:
            size_tag = "  ← exact RGB888 frame"
        elif len(pkt) < 256:
            size_tag = "  ← small control/init packet"

        print(f"Packet {i+1:3d}: {len(pkt):>8} bytes{size_tag}")

        if args.decode_header:
            print(decode_header(pkt))

        if not args.dry_run and dev:
            send_to_device(dev, pkt)
            if args.delay > 0:
                time.sleep(args.delay / 1000)

    print("\nReplay complete.")
    if args.dry_run:
        print("(Dry run — nothing was sent to device)")


if __name__ == "__main__":
    main()
