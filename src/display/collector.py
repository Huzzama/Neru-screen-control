"""
display/collector.py — render → encode → send pipeline.
Modes: "metrics" | "image" | "gif" | "theme" | "off"

In "theme" mode the internal loop pauses — ThemeEditorTab drives
the screen exclusively via push_frame().
"""

import threading
import time
from PIL import Image, ImageDraw, ImageFont

from display.usb_transport import USBTransport
from display.frame_builder import build_metrics_frame
from display.protocol      import encode_frame


def _build_image_frame(source_image: Image.Image) -> Image.Image:
    return source_image.convert("RGB").resize((320, 320), Image.LANCZOS)


def _build_text_frame(lines: list) -> Image.Image:
    img  = Image.new("RGB", (320, 320), (10, 10, 25))
    draw = ImageDraw.Draw(img)
    font = ImageFont.load_default()
    for i, line in enumerate(lines):
        draw.text((10, 20 + i * 30), line, font=font, fill=(200, 200, 200))
    return img


class DisplayController:

    def __init__(self, metrics_collector, config: dict):
        self._metrics   = metrics_collector
        self._config    = config
        self._transport = USBTransport(
            vendor_id  = int(config.get("vendor_id",  "0x87AD"), 16),
            product_id = int(config.get("product_id", "0x70DB"), 16),
        )
        self._fps      = config.get("fps", 10)
        self._mode     = config.get("display_mode", "metrics")
        self._rotation = config.get("rotation", 270)

        self._static_image = None
        self._gif_frames   = []
        self._gif_index    = 0

        self._running = False
        self._thread  = None
        self._lock    = threading.Lock()

        self.last_frame_ms = 0.0
        self.frames_sent   = 0
        self.last_send_ok  = False

    def set_mode(self, mode: str, image: Image.Image = None,
                 gif_frames: list = None) -> None:
        with self._lock:
            self._mode = mode
            if image is not None:
                self._static_image = image
            if gif_frames is not None:
                self._gif_frames = gif_frames
                self._gif_index  = 0

    def set_rotation(self, degrees: int) -> None:
        with self._lock:
            self._rotation = degrees

    def push_frame(self, pil_img: Image.Image) -> None:
        """Push a pre-rendered PIL image directly (theme editor use only)."""
        with self._lock:
            if self._mode != "theme":
                return
            rotation = self._rotation
        try:
            packet = encode_frame(pil_img, rotation=rotation)
            ok = self._transport.send(packet)
            self.last_send_ok = ok
            self.frames_sent += 1
        except Exception as e:
            print(f"[Display] push_frame error: {e}")

    def start(self) -> None:
        if self._running:
            return
        self._running = True
        self._thread  = threading.Thread(target=self._loop, daemon=True)
        self._thread.start()

    def stop(self) -> None:
        self._running = False

    def _loop(self) -> None:
        interval = 1.0 / max(1, self._fps)
        while self._running:
            t0 = time.monotonic()
            with self._lock:
                mode     = self._mode
                rotation = self._rotation

            if mode == "theme":
                time.sleep(interval)
                continue

            try:
                pil_frame = self._render(mode)
                packet    = encode_frame(pil_frame, rotation=rotation)
                ok        = self._transport.send(packet)
                self.last_send_ok = ok
                self.frames_sent += 1
            except Exception as e:
                print(f"[Display] Render/send error: {e}")
                self.last_send_ok = False

            self.last_frame_ms = (time.monotonic() - t0) * 1000
            sleep = interval - (time.monotonic() - t0)
            if sleep > 0:
                time.sleep(sleep)

    def _render(self, mode: str) -> Image.Image:
        if mode == "metrics":
            snap = self._metrics.snapshot
            return build_metrics_frame(snap, self._config)
        elif mode == "image" and self._static_image:
            return _build_image_frame(self._static_image)
        elif mode == "gif" and self._gif_frames:
            frame = self._gif_frames[self._gif_index % len(self._gif_frames)]
            self._gif_index += 1
            return _build_image_frame(frame)
        else:
            return _build_text_frame(["No signal"])