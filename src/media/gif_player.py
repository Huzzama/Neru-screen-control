"""
GIF player — extracts all frames from an animated GIF into a list of PIL Images.
The DisplayController iterates through this list at render time.
"""

from pathlib import Path
from PIL import Image


def load_gif_frames(path: str | Path) -> list[Image.Image]:
    """
    Load all frames of an animated GIF.
    Returns a list of RGB PIL Images.
    Non-animated images return a single-element list.
    """
    frames = []
    with Image.open(path) as gif:
        try:
            while True:
                frames.append(gif.convert("RGB").copy())
                gif.seek(gif.tell() + 1)
        except EOFError:
            pass
    return frames if frames else [Image.new("RGB", (1, 1))]


def get_gif_duration_ms(path: str | Path) -> int:
    """Return per-frame duration in ms (uses first frame's info, default 100ms)."""
    try:
        with Image.open(path) as gif:
            return gif.info.get("duration", 100)
    except Exception:
        return 100
