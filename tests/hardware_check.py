"""
Hardware diagnostic — run this BEFORE any further protocol work.

The goal is to answer: "Is 87ad:70db actually the screen controller,
or is it something else (ARGB controller, fan hub, etc.)?"

USAGE:
    python tests/hardware_check.py
"""

import sys, os, time
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "src"))

import usb.core
import usb.util

VENDOR_ID  = 0x87AD
PRODUCT_ID = 0x70DB


def main():
    print("=" * 60)
    print("  TRCC Hardware Check")
    print("=" * 60)

    # ── 1. List ALL USB devices ───────────────────────────────────────────────
    print("\n[1] All USB devices on this system:\n")
    all_devs = list(usb.core.find(find_all=True))
    for d in all_devs:
        try:
            mfr = d.manufacturer or ""
            prd = d.product      or ""
        except Exception:
            mfr, prd = "", ""
        print(f"    {d.idVendor:#06x}:{d.idProduct:#06x}  {mfr[:30]:<30}  {prd}")

    # ── 2. Full descriptor dump of target device ──────────────────────────────
    dev = usb.core.find(idVendor=VENDOR_ID, idProduct=PRODUCT_ID)
    if dev is None:
        print(f"\nERROR: {VENDOR_ID:#06x}:{PRODUCT_ID:#06x} not found.")
        sys.exit(1)

    print(f"\n[2] Full descriptor for {VENDOR_ID:#06x}:{PRODUCT_ID:#06x}:\n")
    try:
        print(f"    Manufacturer : {dev.manufacturer}")
        print(f"    Product      : {dev.product}")
        print(f"    Serial       : {dev.serial_number}")
    except Exception as e:
        print(f"    (could not read strings: {e})")

    print(f"    bcdUSB       : {dev.bcdUSB:#06x}")
    print(f"    bDeviceClass : {dev.bDeviceClass:#04x}")
    print(f"    bDeviceSubClass: {dev.bDeviceSubClass:#04x}")
    print(f"    bDeviceProtocol: {dev.bDeviceProtocol:#04x}")
    print(f"    bMaxPacketSize0: {dev.bMaxPacketSize0}")
    print(f"    idVendor     : {dev.idVendor:#06x}")
    print(f"    idProduct    : {dev.idProduct:#06x}")
    print(f"    bcdDevice    : {dev.bcdDevice:#06x}")
    print(f"    iManufacturer: {dev.iManufacturer}")
    print(f"    iProduct     : {dev.iProduct}")
    print(f"    iSerialNumber: {dev.iSerialNumber}")
    print(f"    bNumConfigs  : {dev.bNumConfigurations}")

    for cfg_idx in range(dev.bNumConfigurations):
        try:
            cfg = dev[cfg_idx]
        except Exception:
            continue
        print(f"\n    Configuration {cfg.bConfigurationValue}:")
        print(f"      bNumInterfaces : {cfg.bNumInterfaces}")
        print(f"      bmAttributes   : {cfg.bmAttributes:#04x}")
        print(f"      bMaxPower      : {cfg.bMaxPower * 2} mA")
        for intf in cfg:
            print(f"\n      Interface {intf.bInterfaceNumber} alt={intf.bAlternateSetting}:")
            print(f"        bInterfaceClass    : {intf.bInterfaceClass:#04x}  "
                  f"({_class_name(intf.bInterfaceClass)})")
            print(f"        bInterfaceSubClass : {intf.bInterfaceSubClass:#04x}")
            print(f"        bInterfaceProtocol : {intf.bInterfaceProtocol:#04x}")
            for ep in intf:
                direction = "IN " if (ep.bEndpointAddress & 0x80) else "OUT"
                xfer = {0:"CTRL",1:"ISO",2:"BULK",3:"INT"}.get(ep.bmAttributes & 3, "?")
                print(f"        EP {ep.bEndpointAddress:#04x} {direction} {xfer:<5} "
                      f"maxPacket={ep.wMaxPacketSize}  interval={ep.bInterval}")

    # ── 3. Check kernel driver ─────────────────────────────────────────────────
    print("\n[3] Kernel driver status:\n")
    try:
        if dev.is_kernel_driver_active(0):
            print("    Kernel driver IS active on interface 0.")
            print("    This means the OS has claimed the device.")
            print("    The driver name might tell us what the device actually is.")
            # Try to find out which driver
            import subprocess
            try:
                bus = dev.bus
                addr = dev.address
                result = subprocess.run(
                    ["lsusb", "-t"], capture_output=True, text=True
                )
                print(f"\n    lsusb -t output:")
                for line in result.stdout.splitlines():
                    print(f"      {line}")
            except Exception:
                pass
        else:
            print("    No kernel driver active on interface 0.")
    except Exception as e:
        print(f"    Could not check kernel driver: {e}")

    # ── 4. Check /sys for device info ─────────────────────────────────────────
    print("\n[4] Checking /sys/bus/usb/devices/ for device info:\n")
    try:
        import subprocess
        result = subprocess.run(
            ["find", "/sys/bus/usb/devices/", "-name", "idVendor"],
            capture_output=True, text=True
        )
        for path in result.stdout.strip().splitlines():
            try:
                with open(path) as f:
                    vid = f.read().strip()
                if vid == f"{VENDOR_ID:04x}":
                    base = os.path.dirname(path)
                    print(f"    Found at: {base}")
                    for fname in ["idVendor", "idProduct", "product",
                                  "manufacturer", "driver", "bInterfaceClass",
                                  "bDeviceClass", "speed"]:
                        fpath = os.path.join(base, fname)
                        if os.path.exists(fpath):
                            with open(fpath) as f:
                                print(f"      {fname:<22}: {f.read().strip()}")
                    # Check for driver symlink
                    driver_link = os.path.join(base, "driver")
                    if os.path.islink(driver_link):
                        print(f"      driver symlink       : {os.readlink(driver_link)}")
                    # Check for bound interfaces
                    for entry in os.listdir(base):
                        intf_path = os.path.join(base, entry)
                        if os.path.isdir(intf_path):
                            drv = os.path.join(intf_path, "driver")
                            if os.path.islink(drv):
                                print(f"      interface {entry} driver: {os.readlink(drv)}")
            except Exception:
                continue
    except Exception as e:
        print(f"    /sys check failed: {e}")

    # ── 5. Advice ─────────────────────────────────────────────────────────────
    print("\n[5] Interpretation guide:\n")
    print("""
    bInterfaceClass values:
      0x03 = HID (keyboard, mouse, ARGB controller — NOT a display)
      0x08 = Mass Storage
      0x09 = Hub
      0x0A = CDC Data
      0x0E = Video (UVC) — COULD be a display device
      0xFF = Vendor Specific — could be display, ARGB, fan controller, etc.

    If the class is 0x03 (HID):
      → 87ad:70db is NOT the screen. It is the ARGB/fan controller.
      → The actual screen USB device has a different VID:PID.
      → Look for another device in the [1] list above that appeared
        only when the cooler USB cable was plugged in.

    If the class is 0xFF (Vendor Specific) with BULK endpoints:
      → This IS likely the screen — protocol just needs more work.

    CRITICAL: Disconnect the cooler USB cable, run `lsusb`, reconnect,
    run `lsusb` again. The device(s) that APPEARED are the cooler.
    """)

    print("=" * 60)
    print("  Share the full output of this script for analysis.")
    print("=" * 60)


def _class_name(cls: int) -> str:
    return {
        0x00: "Use Interface",
        0x01: "Audio",
        0x02: "CDC",
        0x03: "HID",
        0x05: "Physical",
        0x06: "Image",
        0x07: "Printer",
        0x08: "Mass Storage",
        0x09: "Hub",
        0x0A: "CDC-Data",
        0x0B: "Smart Card",
        0x0D: "Content Security",
        0x0E: "Video (UVC)",
        0x0F: "Personal Healthcare",
        0x10: "Audio/Video",
        0xDC: "Diagnostic",
        0xE0: "Wireless",
        0xEF: "Misc",
        0xFE: "App Specific",
        0xFF: "Vendor Specific",
    }.get(cls, f"Unknown({cls:#04x})")


if __name__ == "__main__":
    main()
