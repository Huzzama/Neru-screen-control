"""Static image loader. Returns PIL Images ready for the frame builder."""

from pathlib import Path
from PIL import Image


def load_image(path: str | Path) -> Image.Image:
    """Load any image format Pillow supports. Returns RGB PIL Image."""
    img = Image.open(path)
    return img.convert("RGB")
