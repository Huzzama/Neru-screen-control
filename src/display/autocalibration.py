"""
Auto-calibration engine for ChiZhu / Thermalright USB LCD displays.

Responsibilities
----------------
* Generate visually unambiguous calibration test patterns.
* Provide a structured candidate-offset search strategy (coarse → fine).
* Send calibration frames through the existing encode + transport pipeline.
* Return a CalibrationResult dataclass that can be persisted to config.json
  and used to update the display.protocol ACTIVE_* globals at runtime.

Architecture
------------
This module is the BACKEND for calibration.
The UI (CalibrationTab) calls it and owns the event loop / user interaction.
A minimal CLI entry point is included for development use only.

Why np.roll is the right fix
-----------------------------
The ChiZhu controller stores its framebuffer as a flat ring buffer.
The display starts scanning from a hardware-fixed internal pointer, NOT from
byte 0 of the data you send.  If you send rows 0..319 without compensation,
the display reads them starting from its internal pointer (e.g. row 160),
wrapping around — producing the classic "image cut in half" artifact.

Setting framebuffer_offset = 160 means we pre-roll the row array so that
what lands at the hardware's internal pointer *is* our row 0.

Why you see 4 quadrants instead of 2 halves after a bad roll
-------------------------------------------------------------
A 2-way split  →  offset is 0 (or H)      — no compensation applied
A 4-way split  →  offset ≈ H/4 or 3H/4   — wrong compensation value
Correct image  →  offset == hardware start row

Each wrong roll value produces a characteristic pattern.  The coarse scan
exploits this: by showing the user patterns at offset 0, 80, 160, 240 we can
pin down the correct quadrant in four frames, then fine-tune within ±40.

Physical rotation note
-----------------------
This device is mounted 90° counter-clockwise in the cooler chassis.
The calibration pattern includes an arrow and "TOP" label so the user can
immediately see whether rotation is also needed.
Default profile for VID:PID 87ad:70db sets rotation=270 (PIL rotates CCW,
so 270° CCW = 90° CW, which corrects a physical 90° CCW mount).
"""

from __future__ import annotations

import time
from dataclasses import dataclass, field, asdict
from typing import Callable, Optional

import numpy as np
from PIL import Image, ImageDraw, ImageFont


# ─────────────────────────────────────────────────────────────────────────────
# Display profile defaults
# ─────────────────────────────────────────────────────────────────────────────

#: Known Thermalright / ChiZhu profiles.
#: framebuffer_offset = 0 until calibrated (autocalibration fills this in).
DISPLAY_PROFILES: dict[str, dict] = {
    "Frozen Warframe": {
        "width":               320,
        "height":              320,
        "rotation":            270,
        "flip_y":              False,
        "pixel_format":        "rgb565_be",
        "framebuffer_offset":  0,    # vertical (row) offset   — axis=0
        "framebuffer_offset_x": 0,   # horizontal (col) offset — axis=1
    },
}

DEFAULT_PROFILE = "Frozen Warframe"


# ─────────────────────────────────────────────────────────────────────────────
# Result dataclass
# ─────────────────────────────────────────────────────────────────────────────

@dataclass
class CalibrationResult:
    """
    Returned by every calibration run.
    Can be serialised to dict and saved directly into config.json.
    """
    profile_name:       str
    width:              int
    height:             int
    selected_offset:    int    # vertical framebuffer_offset   (axis=0)
    selected_offset_x:  int    # horizontal framebuffer_offset_x (axis=1)
    rotation:           int
    flip_y:             bool
    pixel_format:       str
    tested_offsets:     list[int] = field(default_factory=list)
    notes:              str = ""

    def as_config_patch(self) -> dict:
        return {
            "framebuffer_offset":   self.selected_offset,
            "framebuffer_offset_x": self.selected_offset_x,
            "rotation":             self.rotation,
            "flip_y":               self.flip_y,
            "pixel_format":         self.pixel_format,
        }

    def as_dict(self) -> dict:
        return asdict(self)


# ─────────────────────────────────────────────────────────────────────────────
# Calibration pattern generator
# ─────────────────────────────────────────────────────────────────────────────

def build_calibration_pattern(width: int = 320, height: int = 320) -> Image.Image:
    """
    Generate a highly diagnostic test image.

    Layout (top → bottom when correct):
      ┌──────────────────────────────┐
      │  RED band  — "1 TOP ↓"       │
      │  ─────────────────────────── │
      │  GREEN band — "2"            │
      │  ─────────────────────────── │
      │  BLUE band — "3"             │
      │  ─────────────────────────── │
      │  YELLOW band — "4 BOT"       │
      └──────────────────────────────┘

    Corner markers (unique color per corner) make flips immediately visible.
    Thick black separators every quarter make quadrant counting easy.
    The ↓ arrow in the TOP band removes any ambiguity about which end is up.
    """
    img  = Image.new("RGB", (width, height), (0, 0, 0))
    draw = ImageDraw.Draw(img)
    font = _load_font(28)
    font_sm = _load_font(14)

    q = height // 4
    bands = [
        # (y_start, fill_color, label, text_color)
        (0,     (200,  30,  30), "1  TOP  \u2193", (255, 255, 255)),
        (q,     ( 30, 180,  30), "2  MID-A",       (  0,   0,   0)),
        (q*2,   ( 30,  80, 220), "3  MID-B",       (255, 255, 255)),
        (q*3,   (220, 200,  20), "4  BOT",         (  0,   0,   0)),
    ]

    for y0, color, label, tcol in bands:
        y1 = y0 + q
        draw.rectangle([0, y0, width, y1], fill=color)
        # Large band label centered
        draw.text((width // 2 - 60, y0 + q // 2 - 18), label,
                  font=font, fill=tcol)
        # Row number at left edge
        draw.text((6, y0 + 4), f"row {y0}", font=font_sm, fill=tcol)
        draw.text((6, y0 + 20), f"row {y0 + q - 1}", font=font_sm, fill=tcol)

    # Thick black separators between bands
    sep_w = 4
    for y in (q, q*2, q*3):
        draw.rectangle([0, y - sep_w//2, width, y + sep_w//2], fill=(0, 0, 0))

    # Unique corner markers — immediately reveal flips
    c = 18
    draw.rectangle([0, 0,       c, c],           fill=(255,   0, 255))  # TL magenta
    draw.rectangle([width-c, 0, width, c],        fill=(  0, 255, 255))  # TR cyan
    draw.rectangle([0, height-c, c, height],      fill=(255, 128,   0))  # BL orange
    draw.rectangle([width-c, height-c, width, height], fill=(128,   0, 255))  # BR purple

    # Thin horizontal tick every 40 rows
    for y in range(0, height, 40):
        draw.line([(0, y), (20, y)], fill=(0, 0, 0), width=1)
        draw.line([(width-20, y), (width, y)], fill=(0, 0, 0), width=1)

    return img


def build_offset_label_pattern(offset: int,
                                width: int = 320,
                                height: int = 320) -> Image.Image:
    """
    Overlay the current offset value on the calibration pattern.
    Useful when cycling through offsets automatically so the user
    can read which offset they are looking at on the physical screen.
    """
    base = build_calibration_pattern(width, height)
    draw = ImageDraw.Draw(base)
    font = _load_font(32)
    label = f"offset={offset}"
    # White text with black shadow for readability on any band color
    draw.text((width//2 - 70 + 2, height//2 - 18 + 2), label,
              font=font, fill=(0, 0, 0))
    draw.text((width//2 - 70, height//2 - 18), label,
              font=font, fill=(255, 255, 255))
    return base


def _load_font(size: int) -> ImageFont.FreeTypeFont | ImageFont.ImageFont:
    candidates = [
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
        "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/dejavu/DejaVuSans-Bold.ttf",
    ]
    for path in candidates:
        try:
            return ImageFont.truetype(path, size)
        except Exception:
            continue
    return ImageFont.load_default()


# ─────────────────────────────────────────────────────────────────────────────
# Offset candidate strategy
# ─────────────────────────────────────────────────────────────────────────────

def candidate_offsets_coarse(height: int = 320) -> list[int]:
    """
    Coarse scan: tests 12 evenly-spaced candidates across the full height.
    Designed to identify which *quadrant* the correct offset lives in.

    At H=320 the hardware offset is likely one of: 0, 80, 160, 240.
    We include nearby values to catch slight variations between units.
    """
    step = height // 8
    candidates = sorted(set(range(0, height, step)))
    # Always include the four "classic" candidates for 320px displays
    for v in (0, 80, 160, 240):
        if v not in candidates:
            candidates.append(v)
    return sorted(candidates)


def candidate_offsets_fine(center: int, height: int = 320,
                           radius: int = 40, step: int = 5) -> list[int]:
    """
    Fine scan: tests offsets within ±radius of a known-close center value.
    Use after a coarse scan narrows down the rough region.

    Example: center=160, radius=40, step=5
    → [120, 125, 130, ..., 195, 200]
    """
    start = max(0, center - radius)
    stop  = min(height - 1, center + radius)
    return list(range(start, stop + 1, step))


# ─────────────────────────────────────────────────────────────────────────────
# Frame sending
# ─────────────────────────────────────────────────────────────────────────────

def send_calibration_frame(transport,
                            image:     Image.Image,
                            offset:    int,
                            offset_x:  int  = 0,
                            rotation:  int  = 0,
                            fmt:       str  = "rgb565_be",
                            flip_y:    bool = False) -> bool:
    """Send one calibration frame with both row (offset) and column (offset_x) compensation."""
    from display.protocol import encode_frame
    try:
        packet = encode_frame(image,
                              rotation=rotation,
                              fmt=fmt,
                              flip_y=flip_y,
                              framebuffer_offset=offset,
                              framebuffer_offset_x=offset_x)
        return transport.send(packet)
    except Exception as e:
        print(f"[autocalibration] send_calibration_frame error: {e}")
        return False


# ─────────────────────────────────────────────────────────────────────────────
# Manual calibration (UI-driven)
# ─────────────────────────────────────────────────────────────────────────────

class ManualCalibrationSession:
    """
    Stateful calibration session for use by the UI (CalibrationTab).

    The UI calls step_forward() / step_backward() while the user watches
    the physical display.  When the image looks correct, the UI calls
    confirm() to lock in the offset.

    Example UI usage
    ----------------
        session = ManualCalibrationSession(transport, profile="Frozen Warframe")
        session.start()

        # User presses "Next" button:
        current = session.step_forward()
        label   = f"offset={current}"

        # User sees the correct image and presses "Confirm":
        result = session.confirm()
        save_calibration_result(result, config)
    """

    def __init__(self,
                 transport,
                 profile:      str  = DEFAULT_PROFILE,
                 scan_mode:    str  = "coarse",
                 fine_center:  int  = 160,
                 offset_x:     int  = 0,
                 show_label:   bool = True):
        prof = DISPLAY_PROFILES.get(profile, DISPLAY_PROFILES[DEFAULT_PROFILE])
        self._transport  = transport
        self._profile    = profile
        self._w          = prof["width"]
        self._h          = prof["height"]
        self._rotation   = prof["rotation"]
        self._flip_y     = prof["flip_y"]
        self._fmt        = prof["pixel_format"]
        self._offset_x   = offset_x
        self._show_label = show_label

        if scan_mode == "fine":
            self._candidates = candidate_offsets_fine(fine_center, self._h)
        else:
            self._candidates = candidate_offsets_coarse(self._h)

        self._index     = 0
        self._pattern   = build_calibration_pattern(self._w, self._h)
        self._confirmed = False
        self._result: Optional[CalibrationResult] = None

    # ── Navigation ────────────────────────────────────────────────────────────

    def start(self) -> int:
        """Send the first candidate and return its offset value."""
        self._index = 0
        return self._send_current()

    def step_forward(self) -> int:
        """Advance to next candidate, send it, return its offset."""
        self._index = (self._index + 1) % len(self._candidates)
        return self._send_current()

    def step_backward(self) -> int:
        """Go back to previous candidate, send it, return its offset."""
        self._index = (self._index - 1) % len(self._candidates)
        return self._send_current()

    def jump_to_offset(self, offset: int) -> int:
        """Jump directly to a specific offset value (adds it if not in list)."""
        if offset not in self._candidates:
            self._candidates.append(offset)
            self._candidates.sort()
        self._index = self._candidates.index(offset)
        return self._send_current()

    @property
    def current_offset(self) -> int:
        return self._candidates[self._index]

    @property
    def candidates(self) -> list[int]:
        return list(self._candidates)

    @property
    def total(self) -> int:
        return len(self._candidates)

    @property
    def position(self) -> int:
        return self._index + 1   # 1-based for display

    # ── Confirmation ──────────────────────────────────────────────────────────

    def confirm(self, offset: int = None, offset_x: int = None) -> CalibrationResult:
        if offset   is None: offset   = self.current_offset
        if offset_x is None: offset_x = self._offset_x
        self._confirmed = True
        self._result = CalibrationResult(
            profile_name=      self._profile,
            width=             self._w,
            height=            self._h,
            selected_offset=   offset,
            selected_offset_x= offset_x,
            rotation=          self._rotation,
            flip_y=            self._flip_y,
            pixel_format=      self._fmt,
            tested_offsets=    list(self._candidates[:self._index + 1]),
            notes=             f"Manual — offset_y={offset} offset_x={offset_x}",
        )
        return self._result

    # ── Internal ──────────────────────────────────────────────────────────────

    def _send_current(self) -> int:
        offset = self.current_offset
        img = (build_offset_label_pattern(offset, self._w, self._h)
               if self._show_label else self._pattern)
        send_calibration_frame(self._transport, img, offset,
                               offset_x=self._offset_x,
                               rotation=self._rotation,
                               fmt=self._fmt,
                               flip_y=self._flip_y)
        return offset


# ─────────────────────────────────────────────────────────────────────────────
# Auto-cycle calibration (UI-assisted, no console interaction)
# ─────────────────────────────────────────────────────────────────────────────

def run_auto_cycle_calibration(
        transport,
        profile:          str            = DEFAULT_PROFILE,
        dwell_seconds:    float          = 1.5,
        scan_mode:        str            = "coarse",
        fine_center:      int            = 160,
        progress_cb:      Callable[[int, int, int], None] = None,
        stop_flag:        Callable[[], bool]              = None,
) -> list[int]:
    """
    Automatically cycle through all candidate offsets, pausing dwell_seconds
    on each.  The user watches the physical screen and notes which offset
    produces a correct image, then manually confirms it via the UI.

    This function is BLOCKING — run it in a thread.

    Parameters
    ----------
    transport       : USBTransport
    profile         : display profile name
    dwell_seconds   : how long to hold each frame on screen
    scan_mode       : "coarse" | "fine"
    fine_center     : center for fine scan
    progress_cb     : optional callback(current_offset, index, total)
                      called each time a new frame is sent — use to update UI
    stop_flag       : optional callable → bool; return True to abort the cycle

    Returns
    -------
    list[int] : the offsets that were tested (in order)
    """
    prof = DISPLAY_PROFILES.get(profile, DISPLAY_PROFILES[DEFAULT_PROFILE])
    w, h       = prof["width"], prof["height"]
    rotation   = prof["rotation"]
    flip_y     = prof["flip_y"]
    fmt        = prof["pixel_format"]

    if scan_mode == "fine":
        candidates = candidate_offsets_fine(fine_center, h)
    else:
        candidates = candidate_offsets_coarse(h)

    tested = []
    for i, offset in enumerate(candidates):
        if stop_flag and stop_flag():
            print(f"[autocalibration] Cycle aborted at offset={offset}")
            break

        img = build_offset_label_pattern(offset, w, h)
        ok  = send_calibration_frame(transport, img, offset,
                                     rotation=rotation, fmt=fmt, flip_y=flip_y)
        tested.append(offset)
        print(f"[autocalibration] offset={offset:3d}  sent={'ok' if ok else 'FAIL'}"
              f"  ({i+1}/{len(candidates)})")

        if progress_cb:
            try:
                progress_cb(offset, i + 1, len(candidates))
            except Exception:
                pass

        time.sleep(dwell_seconds)

    return tested


# ─────────────────────────────────────────────────────────────────────────────
# Persist result
# ─────────────────────────────────────────────────────────────────────────────

def save_calibration_result(result: CalibrationResult, config) -> None:
    """
    Write the calibration result into the running Config object and persist
    it to disk (config.json).

    Also updates display.protocol ACTIVE_* globals immediately so the running
    app uses the new values without requiring a restart.

    Parameters
    ----------
    result : CalibrationResult
    config : Config   (src/config/loader.py)
    """
    patch = result.as_config_patch()
    for key, value in patch.items():
        config.set(key, value)   # Config.set() saves to disk automatically

    # Hot-patch the encoder globals so live rendering uses new values instantly
    try:
        import display.protocol as proto
        proto.ACTIVE_FRAMEBUFFER_OFFSET   = result.selected_offset
        proto.ACTIVE_FRAMEBUFFER_OFFSET_X = result.selected_offset_x
        proto.ACTIVE_ROTATION             = result.rotation
        proto.ACTIVE_FLIP_Y               = result.flip_y
        print(f"[autocalibration] Protocol globals updated: "
              f"offset_y={result.selected_offset} offset_x={result.selected_offset_x} "
              f"rotation={result.rotation} flip_y={result.flip_y}")
    except Exception as e:
        print(f"[autocalibration] Could not update protocol globals: {e}")

    print(f"[autocalibration] Saved to config: {patch}")


def load_calibration_from_config(config, profile: str = DEFAULT_PROFILE) -> CalibrationResult:
    prof = DISPLAY_PROFILES.get(profile, DISPLAY_PROFILES[DEFAULT_PROFILE])
    return CalibrationResult(
        profile_name=      profile,
        width=             prof["width"],
        height=            prof["height"],
        selected_offset=   config.get("framebuffer_offset",   prof["framebuffer_offset"]),
        selected_offset_x= config.get("framebuffer_offset_x", prof["framebuffer_offset_x"]),
        rotation=          config.get("rotation",             prof["rotation"]),
        flip_y=            bool(config.get("flip_y",          prof["flip_y"])),
        pixel_format=      config.get("pixel_format",         prof["pixel_format"]),
        tested_offsets=    [],
        notes=             "Loaded from config",
    )


# ─────────────────────────────────────────────────────────────────────────────
# Convenience: quick single-frame push (for CalibrationTab preview)
# ─────────────────────────────────────────────────────────────────────────────

def push_preview_frame(transport,
                       offset:   int,
                       offset_x: int  = 0,
                       rotation: int  = 270,
                       fmt:      str  = "rgb565_be",
                       flip_y:   bool = False,
                       label:    bool = True) -> bool:
    img = (build_offset_label_pattern(offset)
           if label else build_calibration_pattern())
    return send_calibration_frame(transport, img, offset, offset_x=offset_x,
                                  rotation=rotation, fmt=fmt, flip_y=flip_y)


# ─────────────────────────────────────────────────────────────────────────────
# CLI entry point — development use only, not part of distributed app
# ─────────────────────────────────────────────────────────────────────────────

if __name__ == "__main__":
    """
    Minimal CLI for bench testing.
    Run from project root:
        python -m display.autocalibration [--fine --center 160]

    This is NOT the app entry point.  The app runs via main.py → UI.
    """
    import argparse
    import sys
    import os

    # Add src/ to path so imports resolve when run directly
    sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", ".."))

    parser = argparse.ArgumentParser(description="TRCC Display Autocalibration CLI")
    parser.add_argument("--fine",   action="store_true", help="Fine scan mode")
    parser.add_argument("--center", type=int, default=160,
                        help="Center offset for fine scan (default: 160)")
    parser.add_argument("--dwell",  type=float, default=1.5,
                        help="Seconds per frame (default: 1.5)")
    parser.add_argument("--profile", default=DEFAULT_PROFILE,
                        help=f"Display profile (default: {DEFAULT_PROFILE!r})")
    args = parser.parse_args()

    # Import here so the module itself doesn't hard-depend on USB at import time
    try:
        from display.usb_transport import USBTransport
    except ImportError:
        print("ERROR: Could not import USBTransport.  "
              "Run from project src/ directory.")
        sys.exit(1)

    transport = USBTransport()
    if not transport.connected:
        print("ERROR: No display found.  Check USB connection and udev rules.")
        sys.exit(1)

    scan_mode = "fine" if args.fine else "coarse"
    print(f"Starting {scan_mode} calibration scan "
          f"(profile={args.profile!r}, dwell={args.dwell}s)")
    print("Watch the physical display.  Note which offset makes the image correct.")
    print("Press Ctrl+C to stop early.\n")

    try:
        tested = run_auto_cycle_calibration(
            transport,
            profile=       args.profile,
            dwell_seconds= args.dwell,
            scan_mode=     scan_mode,
            fine_center=   args.center,
            progress_cb=   lambda off, i, tot: print(
                f"  [{i:2d}/{tot}] offset={off:3d}"),
        )
    except KeyboardInterrupt:
        print("\nScan interrupted.")
        sys.exit(0)

    print(f"\nTested offsets: {tested}")
    try:
        chosen = int(input("Enter the offset that looked correct: "))
    except (ValueError, EOFError):
        print("No offset entered.  Exiting without saving.")
        sys.exit(0)

    result = CalibrationResult(
        profile_name=    args.profile,
        width=           320,
        height=          320,
        selected_offset= chosen,
        rotation=        DISPLAY_PROFILES[args.profile]["rotation"],
        flip_y=          DISPLAY_PROFILES[args.profile]["flip_y"],
        pixel_format=    DISPLAY_PROFILES[args.profile]["pixel_format"],
        tested_offsets=  tested,
        notes=           "CLI calibration",
    )
    print(f"\nCalibration result: {result.as_config_patch()}")
    print("To save, call save_calibration_result(result, config) from the app.")