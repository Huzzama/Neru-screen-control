"""
Video player — extracts frames from a video file using OpenCV.
Returns a list of PIL Images that the DisplayController can loop.
"""

from pathlib import Path
from PIL import Image


def load_video_frames(path: str | Path,
                      max_frames: int = 300,
                      every_n: int = 1) -> list[Image.Image]:
    """
    Extract frames from a video file.

    Args:
        path:       Path to video file (.mp4, .avi, .gif, etc.)
        max_frames: Maximum number of frames to extract (to limit RAM usage).
        every_n:    Extract every N-th frame (e.g. 2 = half frame rate).

    Returns:
        List of RGB PIL Images.
    """
    try:
        import cv2
    except ImportError:
        print("[VideoPlayer] opencv-python not installed. "
              "Install with: pip install opencv-python")
        return []

    cap = cv2.VideoCapture(str(path))
    if not cap.isOpened():
        print(f"[VideoPlayer] Could not open: {path}")
        return []

    frames = []
    idx    = 0
    while len(frames) < max_frames:
        ret, bgr = cap.read()
        if not ret:
            break
        if idx % every_n == 0:
            rgb = cv2.cvtColor(bgr, cv2.COLOR_BGR2RGB)
            frames.append(Image.fromarray(rgb))
        idx += 1

    cap.release()
    print(f"[VideoPlayer] Loaded {len(frames)} frames from {Path(path).name}")
    return frames
