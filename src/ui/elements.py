"""Canvas element data model for the TRCC screen editor.

Bounding-box contract
─────────────────────
Every element exposes:

    measure(fonts: dict) -> (w: int, h: int)

This returns the TRUE rendered size in native pixels.
The canvas always calls measure() for hit-testing and selection drawing
instead of relying on the stored self.w / self.h.

For BarElement and ImageElement, measure() just returns (self.w, self.h)
because those elements ARE exactly their stored rectangle.

For TextElement and MetricElement, measure() uses PIL font metrics to
compute the actual rendered text extents so the bounding box is tight.
"""

import os
from PIL import Image, ImageDraw, ImageFont

# ── Metric registry ────────────────────────────────────────────────────────────

METRIC_LABELS = {
    'CPU Temp':   'cpu_temp',
    'GPU Temp':   'gpu_temp',
    'CPU Usage':  'cpu_usage',
    'GPU Usage':  'gpu_usage',
    'CPU Freq':   'cpu_frequency',
    'GPU Freq':   'gpu_frequency',
    'CPU Power':  'cpu_power',
    'GPU Power':  'gpu_power',
    'RAM Usage':  'ram_usage',
}

METRIC_UNITS = {
    'cpu_temp': '°C',  'gpu_temp': '°C',
    'cpu_usage': '%',  'gpu_usage': '%',
    'cpu_frequency': 'MHz', 'gpu_frequency': 'MHz',
    'cpu_power': 'W',  'gpu_power': 'W',
    'ram_usage': '%',
}

# Sample values used for measuring MetricElement at design time
_SAMPLE_METRIC = {
    'cpu_temp': 75, 'gpu_temp': 70,
    'cpu_usage': 50, 'gpu_usage': 30,
    'cpu_frequency': 3500, 'gpu_frequency': 1800,
    'cpu_power': 65, 'gpu_power': 120,
    'ram_usage': 60,
}

_PAD = 2   # px of padding added around measured text bounds


def _measure_text(text: str, font) -> tuple[int, int]:
    """Return (width, height) of text rendered with the given PIL font."""
    try:
        bb = font.getbbox(text)          # (left, top, right, bottom)
        return max(1, bb[2] - bb[0]), max(1, bb[3] - bb[1])
    except AttributeError:
        # Older Pillow fallback
        w, h = font.getsize(text)        # type: ignore[attr-defined]
        return max(1, w), max(1, h)


# ── Base ───────────────────────────────────────────────────────────────────────

class CanvasElement:
    def __init__(self):
        self.x = 0;  self.y = 0
        self.w = 100; self.h = 40
        self.visible   = True
        self.locked    = False
        self.user_sized = False   # True once the user has manually resized

    def measure(self, fonts: dict) -> tuple[int, int]:
        """Return the TRUE rendered (w, h) in native pixels.
        Default: use stored w/h (correct for bar / image).
        Text-based subclasses override this."""
        return self.w, self.h

    def render(self, draw: ImageDraw.Draw, img: Image.Image,
               metrics: dict, fonts: dict):
        pass

    def tick(self): pass

    def to_dict(self) -> dict:
        return {'type': self.__class__.__name__,
                'x': self.x, 'y': self.y, 'w': self.w, 'h': self.h,
                'visible': self.visible, 'locked': self.locked,
                'user_sized': self.user_sized}

    @classmethod
    def from_dict(cls, d: dict):
        o = cls()
        o.x = d.get('x', 0); o.y = d.get('y', 0)
        o.w = d.get('w', 100); o.h = d.get('h', 40)
        o.visible    = d.get('visible', True)
        o.locked     = d.get('locked',  False)
        o.user_sized = d.get('user_sized', False)
        return o

    def display_name(self): return self.__class__.__name__


# ── Text ───────────────────────────────────────────────────────────────────────

class TextElement(CanvasElement):
    def __init__(self):
        super().__init__()
        self.text      = 'Text'
        self.font_size = 24
        self.color     = (255, 255, 255)

    # ── Measure ───────────────────────────────────────────────────────────────

    def measure(self, fonts: dict) -> tuple[int, int]:
        # Once the user has manually resized, honour their dimensions exactly.
        if self.user_sized:
            return self.w, self.h
        font = fonts.get(self.font_size, list(fonts.values())[-1])
        tw, th = _measure_text(self.text or ' ', font)
        return tw + _PAD * 2, th + _PAD * 2

    # ── Render ────────────────────────────────────────────────────────────────

    def render(self, draw, img, metrics, fonts):
        if not self.visible: return
        font = fonts.get(self.font_size, list(fonts.values())[-1])
        draw.text((self.x, self.y), self.text, font=font, fill=self.color)

    # ── Serialise ─────────────────────────────────────────────────────────────

    def to_dict(self):
        d = super().to_dict()
        d.update({'text': self.text, 'font_size': self.font_size,
                  'color': list(self.color)})
        return d

    @classmethod
    def from_dict(cls, d):
        o = super().from_dict(d)
        o.text      = d.get('text', 'Text')
        o.font_size = d.get('font_size', 24)
        o.color     = tuple(d.get('color', [255, 255, 255]))
        return o

    def display_name(self): return f'Text: "{self.text}"'


# ── Metric value ───────────────────────────────────────────────────────────────

class MetricElement(CanvasElement):
    def __init__(self):
        super().__init__()
        self.metric      = 'cpu_temp'
        self.label       = ''
        self.show_label  = True
        self.show_unit   = True
        self.font_size   = 32
        self.color       = (0, 200, 255)
        self.label_color = (150, 150, 150)

    # ── Measure ───────────────────────────────────────────────────────────────

    def measure(self, fonts: dict) -> tuple[int, int]:
        if self.user_sized:
            return self.w, self.h
        font  = fonts.get(self.font_size, list(fonts.values())[-1])
        small = fonts.get(14, list(fonts.values())[0])

        # Value line — use a representative sample value
        sample = _SAMPLE_METRIC.get(self.metric, 99)
        unit   = METRIC_UNITS.get(self.metric, '') if self.show_unit else ''
        val_str = f"{sample}{unit}"
        vw, vh = _measure_text(val_str, font)

        total_w = vw
        total_h = vh

        # Label line
        if self.show_label and self.label:
            lw, lh = _measure_text(self.label, small)
            total_w = max(total_w, lw)
            total_h += lh + 2   # 2px gap between label and value

        return total_w + _PAD * 2, total_h + _PAD * 2

    # ── Render ────────────────────────────────────────────────────────────────

    def render(self, draw, img, metrics, fonts):
        if not self.visible: return
        val   = metrics.get(self.metric, 0)
        unit  = METRIC_UNITS.get(self.metric, '') if self.show_unit else ''
        font  = fonts.get(self.font_size, list(fonts.values())[-1])
        small = fonts.get(14, list(fonts.values())[0])
        y = self.y
        if self.show_label and self.label:
            draw.text((self.x, y), self.label, font=small, fill=self.label_color)
            y += 18
        draw.text((self.x, y), f"{val}{unit}", font=font, fill=self.color)

    # ── Serialise ─────────────────────────────────────────────────────────────

    def to_dict(self):
        d = super().to_dict()
        d.update({'metric': self.metric, 'label': self.label,
                  'show_label': self.show_label, 'show_unit': self.show_unit,
                  'font_size': self.font_size, 'color': list(self.color),
                  'label_color': list(self.label_color)})
        return d

    @classmethod
    def from_dict(cls, d):
        o = super().from_dict(d)
        o.metric      = d.get('metric', 'cpu_temp')
        o.label       = d.get('label', '')
        o.show_label  = d.get('show_label', True)
        o.show_unit   = d.get('show_unit', True)
        o.font_size   = d.get('font_size', 32)
        o.color       = tuple(d.get('color', [0, 200, 255]))
        o.label_color = tuple(d.get('label_color', [150, 150, 150]))
        return o

    def display_name(self): return f'Metric: {self.metric}'


# ── Progress bar ───────────────────────────────────────────────────────────────

class BarElement(CanvasElement):
    """Stored w/h IS the visual size — measure() just returns them."""

    def __init__(self):
        super().__init__()
        self.metric   = 'cpu_usage'; self.w = 140; self.h = 12
        self.fg_color = (0, 180, 255); self.bg_color = (40, 40, 60)
        self.max_val  = 100

    # measure() inherited from CanvasElement → returns (self.w, self.h) ✓

    def render(self, draw, img, metrics, fonts):
        if not self.visible: return
        val = metrics.get(self.metric, 0)
        draw.rectangle([self.x, self.y, self.x+self.w, self.y+self.h],
                       fill=self.bg_color)
        fw = int(self.w * min(1.0, max(0.0, val / self.max_val)))
        if fw > 0:
            draw.rectangle([self.x, self.y, self.x+fw, self.y+self.h],
                           fill=self.fg_color)

    def to_dict(self):
        d = super().to_dict()
        d.update({'metric': self.metric, 'fg_color': list(self.fg_color),
                  'bg_color': list(self.bg_color), 'max_val': self.max_val})
        return d

    @classmethod
    def from_dict(cls, d):
        o = super().from_dict(d)
        o.metric   = d.get('metric', 'cpu_usage')
        o.fg_color = tuple(d.get('fg_color', [0, 180, 255]))
        o.bg_color = tuple(d.get('bg_color', [40, 40, 60]))
        o.max_val  = d.get('max_val', 100)
        return o

    def display_name(self): return f'Bar: {self.metric}'


# ── Image / GIF / Video ────────────────────────────────────────────────────────

class ImageElement(CanvasElement):
    """
    Static images, animated GIFs, and video files (via OpenCV).

    Images / GIFs  — fully pre-loaded into _src_frames (small, fine in RAM).
    Videos         — streamed: cv2.VideoCapture stays open, frames decoded
                     on demand.  Only a 1-frame decode cache is kept so RAM
                     usage is O(1) regardless of video length.
    """

    GIF_EXTS   = {'.gif'}
    VIDEO_EXTS = {'.mp4', '.avi', '.mkv', '.webm', '.mov', '.wmv'}

    # Maximum video file size accepted before even opening the file.
    VIDEO_MAX_BYTES = 200 * 1024 * 1024   # 200 MB

    def __init__(self):
        super().__init__()
        self.path = ''; self.w = 320; self.h = 320

        # ── GIF / static frame store ──────────────────────────────────────
        self._src_frames:  list  = []   # PIL RGBA frames (images & GIFs only)
        self._frame_idx:   int   = 0    # current frame index (images/GIFs/video)
        self._last_path:   str   = ''
        self._rnd_frames:  list  = []   # resized copies cache (images/GIFs only)
        self._rnd_size:    tuple = (0, 0)

        # ── Video streaming state ─────────────────────────────────────────
        self._is_video:    bool  = False
        self._cap                = None   # cv2.VideoCapture (open while in use)
        self._total_frames: int  = 0      # total frame count reported by cv2
        self._last_frame_idx: int = -1    # frame index of _cached_frame
        self._cached_frame       = None   # PIL RGBA — last decoded video frame

    # ── Loading ───────────────────────────────────────────────────────────────

    def _close_cap(self):
        if self._cap is not None:
            try:
                self._cap.release()
            except Exception:
                pass
            self._cap = None

    def _load_source(self):
        self._close_cap()
        self._src_frames  = []
        self._rnd_frames  = []
        self._rnd_size    = (0, 0)
        self._frame_idx   = 0
        self._is_video    = False
        self._total_frames = 0
        self._last_frame_idx = -1
        self._cached_frame   = None

        if not self.path:
            self._last_path = self.path
            return

        ext = os.path.splitext(self.path)[1].lower()

        # ── File-size guard for videos ────────────────────────────────────
        if ext in self.VIDEO_EXTS:
            try:
                size_bytes = os.path.getsize(self.path)
            except OSError:
                size_bytes = 0
            if size_bytes > self.VIDEO_MAX_BYTES:
                limit_mb  = self.VIDEO_MAX_BYTES // (1024 * 1024)
                actual_mb = size_bytes / (1024 * 1024)
                print(f"ImageElement: video too large "
                      f"({actual_mb:.0f} MB > {limit_mb} MB limit). "
                      f"Skipping: {self.path}")
                self._last_path = self.path
                return

        try:
            if ext in self.GIF_EXTS:
                self._src_frames = self._load_gif_src()
            elif ext in self.VIDEO_EXTS:
                self._open_video_stream()
            else:
                self._src_frames = self._load_static_src()
        except Exception as e:
            print(f"ImageElement load error ({self.path}): {e}")

        self._last_path = self.path

    def _load_static_src(self) -> list:
        return [Image.open(self.path).convert('RGBA')]

    def _load_gif_src(self) -> list:
        frames = []
        with Image.open(self.path) as gif:
            try:
                while True:
                    frames.append(gif.convert('RGBA').copy())
                    gif.seek(gif.tell() + 1)
            except EOFError:
                pass
        return frames or self._load_static_src()

    def _open_video_stream(self):
        """
        Open the video file for streaming.  No frames are decoded here —
        _decode_video_frame() fetches them on demand.
        """
        try:
            import cv2
        except ImportError:
            print("ImageElement: pip install opencv-python for video support.")
            return

        cap = cv2.VideoCapture(self.path)
        if not cap.isOpened():
            print(f"ImageElement: cv2 could not open: {self.path}")
            cap.release()
            return

        total = int(cap.get(cv2.CAP_PROP_FRAME_COUNT))
        if total <= 0:
            # Some containers don't report frame count — estimate from duration
            fps = cap.get(cv2.CAP_PROP_FPS) or 30.0
            dur = cap.get(cv2.CAP_PROP_POS_MSEC)   # 0 at start, not useful
            # Fall back to a large sentinel so we just play until read() fails
            total = 999_999

        self._cap          = cap
        self._is_video     = True
        self._total_frames = total
        print(f"ImageElement: streaming video — {total} frames — {self.path}")

    def _decode_video_frame(self, idx: int):
        """
        Decode frame at position `idx` from the open capture.
        Uses a 1-frame cache so repeated calls with the same idx are free.
        Seeks only when necessary (forward sequential reads never seek).
        Returns a PIL RGBA Image or None on failure.
        """
        if self._cap is None:
            return None
        if idx == self._last_frame_idx and self._cached_frame is not None:
            return self._cached_frame

        try:
            import cv2
        except ImportError:
            return None

        # Seek if we're not already at the right position
        current_pos = int(self._cap.get(cv2.CAP_PROP_POS_FRAMES))
        if current_pos != idx:
            self._cap.set(cv2.CAP_PROP_POS_FRAMES, idx)

        ret, bgr = self._cap.read()
        if not ret:
            # End of stream — wrap to 0 next tick
            return self._cached_frame   # return last good frame

        tw = max(8, self.w)
        th = max(8, self.h)
        if bgr.shape[1] != tw or bgr.shape[0] != th:
            bgr = cv2.resize(bgr, (tw, th), interpolation=cv2.INTER_AREA)
        rgb   = cv2.cvtColor(bgr, cv2.COLOR_BGR2RGB)
        frame = Image.fromarray(rgb).convert('RGBA')

        self._cached_frame   = frame
        self._last_frame_idx = idx
        return frame

    # ── Render-size cache (GIFs / images only) ────────────────────────────────

    def _get_render_frames(self, w: int, h: int) -> list:
        if not self._src_frames:
            return []
        if (w, h) != self._rnd_size:
            self._rnd_frames = [
                f.resize((w, h), Image.LANCZOS)
                for f in self._src_frames
            ]
            self._rnd_size = (w, h)
        return self._rnd_frames

    # ── Frame access ──────────────────────────────────────────────────────────

    def _current_frame(self):
        if self.path != self._last_path:
            self._load_source()

        if self._is_video:
            # Wrap frame index at actual stream length
            if self._total_frames > 0:
                idx = self._frame_idx % self._total_frames
            else:
                idx = self._frame_idx
            frame = self._decode_video_frame(idx)
            if frame is None and self._cached_frame:
                return self._cached_frame
            return frame

        # GIF / static
        w, h = max(1, self.w), max(1, self.h)
        frames = self._get_render_frames(w, h)
        if not frames:
            return None
        return frames[self._frame_idx % len(frames)]

    def tick(self):
        if self._is_video:
            self._frame_idx += 1
            # Wrap at total frames so we loop correctly
            if self._total_frames > 0 and self._frame_idx >= self._total_frames:
                self._frame_idx = 0
        else:
            total = len(self._src_frames)
            if total > 1:
                self._frame_idx = (self._frame_idx + 1) % total

    def render(self, draw, img, metrics, fonts):
        if not self.visible or not self.path:
            return
        frame = self._current_frame()
        if frame is None:
            return
        cx = max(0, self.x)
        cy = max(0, self.y)
        ox = cx - self.x
        oy = cy - self.y
        if ox > 0 or oy > 0:
            frame = frame.crop((ox, oy, frame.width, frame.height))
        if frame.width <= 0 or frame.height <= 0:
            return
        try:
            img.paste(frame, (cx, cy), frame)
        except Exception:
            try:
                img.paste(frame, (cx, cy))
            except Exception as e:
                print(f"ImageElement paste error: {e}")

    def __del__(self):
        self._close_cap()

    def to_dict(self):
        d = super().to_dict(); d['path'] = self.path; return d

    @classmethod
    def from_dict(cls, d):
        o = super().from_dict(d); o.path = d.get('path', ''); return o

    def display_name(self):
        name = os.path.basename(self.path) or 'none'
        ext  = os.path.splitext(name)[1].lower()
        tag  = '🎞' if ext in (self.VIDEO_EXTS | self.GIF_EXTS) else '🖼'
        return f'{tag} {name}'


# ── Registry ───────────────────────────────────────────────────────────────────

ELEMENT_TYPES: dict = {
    'TextElement':   TextElement,
    'MetricElement': MetricElement,
    'BarElement':    BarElement,
    'ImageElement':  ImageElement,
}

def element_from_dict(d: dict):
    cls = ELEMENT_TYPES.get(d.get('type', ''))
    return cls.from_dict(d) if cls else None