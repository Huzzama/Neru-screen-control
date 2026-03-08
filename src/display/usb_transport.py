"""
USB transport layer for ChiZhu Tech USBDISPLAY (87ad:70db).

Confirmed per-frame sequence:
  1. ctrl_transfer  CMD_START  (bmRequestType tries 0x40 with bRequest 0x01/0x00/0x02)
  2. bulk write     CMD_TRIG   (4 zero bytes)
  3. bulk write     FRAME_HEADER (208 bytes) + GRB565 pixels
  4. ctrl_transfer  CMD_COMMIT

udev rule (run once as root):
  echo 'SUBSYSTEM=="usb", ATTRS{idVendor}=="87ad", ATTRS{idProduct}=="70db", MODE="0666"' \
    | sudo tee /etc/udev/rules.d/99-chizhou-display.rules
  sudo udevadm control --reload-rules && sudo udevadm trigger
  # Then unplug and replug the USB cable
"""

import time
import usb.core
import usb.util

from display.protocol import CMD_SYNC, CMD_START, CMD_COMMIT, CMD_TRIG

VENDOR_ID  = 0x87AD
PRODUCT_ID = 0x70DB
EP_OUT     = 0x01
EP_IN      = 0x81
CHUNK_SIZE = 512
TIMEOUT_MS = 10000


class USBTransport:
    def __init__(self, vendor_id: int = VENDOR_ID, product_id: int = PRODUCT_ID):
        self.vendor_id  = vendor_id
        self.product_id = product_id
        self.dev        = None
        self._synced    = False
        self._connect()

    # ── Connection ────────────────────────────────────────────────────────────

    def _connect(self) -> bool:
        self.dev = usb.core.find(idVendor=self.vendor_id, idProduct=self.product_id)
        if self.dev is None:
            return False
        try:
            if self.dev.is_kernel_driver_active(0):
                self.dev.detach_kernel_driver(0)
            self.dev.set_configuration()
            print(f"[USB] Connected: {self.vendor_id:#06x}:{self.product_id:#06x}")
            self._synced = False  # need to re-sync on fresh connect
            return True
        except usb.core.USBError as e:
            if e.errno == 13:
                print("[USB] Permission denied — apply udev rule and replug.")
            else:
                print(f"[USB] Connection error: {e}")
            self.dev = None
            return False

    def _ctrl(self, data: bytes) -> bool:
        """Try control transfer with the three known bmRequestType/bRequest combos."""
        for bmt, req in [(0x40, 0x01), (0x40, 0x00), (0x40, 0x02)]:
            try:
                self.dev.ctrl_transfer(bmt, req, 0, 0, data, timeout=2000)
                return True
            except Exception:
                continue
        return False

    def _bulk(self, data: bytes) -> None:
        """Write data in CHUNK_SIZE blocks."""
        for i in range(0, len(data), CHUNK_SIZE):
            self.dev.write(EP_OUT, data[i:i + CHUNK_SIZE], timeout=TIMEOUT_MS)

    def _sync(self) -> None:
        """Send CMD_SYNC x3 on first connect (wakes the display)."""
        for _ in range(3):
            self._ctrl(CMD_SYNC)
            time.sleep(0.05)
        self._synced = True
        print("[USB] Sync complete")

    # ── Public API ────────────────────────────────────────────────────────────

    @property
    def connected(self) -> bool:
        return self.dev is not None

    def send(self, frame_data: bytes) -> bool:
        """
        Send one complete frame using the confirmed sequence:
          CMD_START → CMD_TRIG (bulk) → frame_data (bulk) → CMD_COMMIT
        frame_data should be FRAME_HEADER + GRB565 pixels (from encode_frame()).
        """
        if not self.connected:
            if not self._connect():
                return False

        try:
            if not self._synced:
                self._sync()

            self._ctrl(CMD_START)
            self._bulk(CMD_TRIG)
            self._bulk(frame_data)
            self._ctrl(CMD_COMMIT)
            return True

        except usb.core.USBError as e:
            print(f"[USB] Send error: {e}")
            self.dev = None
            return False

    def read(self, length: int = 64, timeout_ms: int = 1000) -> bytes:
        if not self.connected:
            return b""
        try:
            return bytes(self.dev.read(EP_IN, length, timeout=timeout_ms))
        except Exception:
            return b""

    def close(self) -> None:
        if self.dev is not None:
            try:
                usb.util.dispose_resources(self.dev)
            except Exception:
                pass
            self.dev = None