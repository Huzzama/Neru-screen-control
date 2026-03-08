import usb.core
import usb.util
import time
from .protocol import (
    VENDOR_ID, PRODUCT_ID, EP_BULK_OUT, BULK_CHUNK,
    CMD_SYNC, CMD_START, CMD_COMMIT, CMD_BULK_TRIGGER,
    FRAME_HEADER, encode_frame
)


class DisplayTransport:
    """
    Handles USB communication with the Thermalright Frozen Warframe LCD.

    Usage:
        transport = DisplayTransport()
        transport.connect()
        while True:
            transport.send_frame(pil_image)
    """

    def __init__(self):
        self.dev = None
        self._synced = False

    def connect(self):
        """Find and open the USB device. Raises RuntimeError if not found."""
        self.dev = usb.core.find(idVendor=VENDOR_ID, idProduct=PRODUCT_ID)
        if self.dev is None:
            raise RuntimeError(
                f"Device not found: {VENDOR_ID:04x}:{PRODUCT_ID:04x}\n"
                "Check USB connection and udev rules."
            )
        try:
            if self.dev.is_kernel_driver_active(0):
                self.dev.detach_kernel_driver(0)
        except Exception:
            pass
        self.dev.set_configuration()
        self._synced = False
        print(f"Connected to {VENDOR_ID:04x}:{PRODUCT_ID:04x}")

    def disconnect(self):
        """Release the USB device."""
        if self.dev:
            try:
                usb.util.release_interface(self.dev, 0)
                usb.util.dispose_resources(self.dev)
            except Exception:
                pass
            self.dev = None
            self._synced = False

    def is_connected(self):
        return self.dev is not None

    def _ctrl(self, data):
        """Send a control transfer, trying multiple bmRequestType/bRequest combos."""
        for bmt, req in [(0x40, 0x01), (0x40, 0x00), (0x40, 0x02)]:
            try:
                self.dev.ctrl_transfer(bmt, req, 0, 0, data, timeout=2000)
                return True
            except Exception:
                continue
        return False

    def _bulk(self, data):
        """Send bulk data in BULK_CHUNK-sized packets."""
        for i in range(0, len(data), BULK_CHUNK):
            self.dev.write(EP_BULK_OUT, data[i:i + BULK_CHUNK], timeout=10000)

    def sync(self):
        """Send the SYNC command to wake/initialize the screen."""
        for _ in range(3):
            self._ctrl(CMD_SYNC)
            time.sleep(0.1)
        self._synced = True

    def send_frame(self, img):
        """
        Send a single frame to the display.

        Args:
            img: PIL Image — will be resized/rotated/encoded automatically

        The sequence is:
            CMD_START → CMD_BULK_TRIGGER → HEADER+PIXELS → CMD_COMMIT
        """
        if not self._synced:
            self.sync()

        pixels = encode_frame(img)
        self._ctrl(CMD_START)
        self._bulk(CMD_BULK_TRIGGER)
        self._bulk(FRAME_HEADER + pixels)
        self._ctrl(CMD_COMMIT)

    def reconnect(self, retries=10, delay=2.0):
        """Try to reconnect after a disconnection."""
        self.disconnect()
        for i in range(retries):
            try:
                self.connect()
                self.sync()
                print("Reconnected.")
                return True
            except RuntimeError:
                print(f"Reconnect attempt {i+1}/{retries}...")
                time.sleep(delay)
        return False