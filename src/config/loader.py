"""
Configuration loader.
Loads config.json, validates keys, fills defaults, saves back.
"""

import json
from pathlib import Path

DEFAULT_CONFIG = {
    "vendor_id":            "0x87AD",
    "product_id":           "0x70DB",
    "display_mode":         "metrics",   # metrics | image | gif | video
    "media_path":           "",          # path to image/gif/video file
    "rotation":             0,           # 0 | 90 | 180 | 270
    "fps":                  10,          # frames per second sent to display
    "metrics_interval":     1.0,         # seconds between metric updates
    "cpu_temperature_unit": "celsius",   # celsius | fahrenheit
    "gpu_temperature_unit": "celsius",
    "layout_mode":          "Frozen Warframe",
    # ── Startup / window behaviour ──────────────────────────────────────────
    "start_on_login":   False,  # enable systemd user service autostart
    "minimize_on_close": True,  # hide window instead of quitting on close
    "launch_hidden":    False,  # start with window hidden (tray only)
    "tray_icon":        True,   # show system tray icon
}

# Known Thermalright LCD models and their screen resolutions
DISPLAY_PROFILES = {
    "Frozen Warframe":  (320, 240),
    "Core Matrix":      (320, 240),
    "Mjolnir Vision":   (640, 480),
    "Peerless Vision":  (480, 480),
    "Stream Vision":    (640, 480),
    "Core Vision":      (480, 480),
    "Frozen Guardian":  (480, 480),
    "Frozen Vision":    (480, 480),
    "Guard Vision":     (480, 480),
    "Hyper Vision":     (480, 480),
    "Elite Vision":     (480, 480),
    "Trofeo Vision":    (1280, 480),
    "Leviathan Vision": (2400, 1080),
    "Rainbow Vision":   (2400, 1080),
    "Wonder Vision":    (2400, 1080),
}


class Config:
    def __init__(self, path: str = "config.json"):
        self.path = Path(path)
        self._data = dict(DEFAULT_CONFIG)
        self.load()

    # ------------------------------------------------------------------ #

    def load(self) -> None:
        if self.path.is_file():
            try:
                with open(self.path, "r") as f:
                    loaded = json.load(f)
                self._data.update(loaded)
            except Exception as e:
                print(f"[Config] Could not load {self.path}: {e}. Using defaults.")
        else:
            print(f"[Config] {self.path} not found. Using defaults.")
            self.save()

    def save(self) -> None:
        try:
            with open(self.path, "w") as f:
                json.dump(self._data, f, indent=4)
        except Exception as e:
            print(f"[Config] Could not save {self.path}: {e}")

    def get(self, key: str, default=None):
        return self._data.get(key, default)

    def set(self, key: str, value) -> None:
        self._data[key] = value
        self.save()

    def as_dict(self) -> dict:
        return dict(self._data)

    def screen_size(self) -> tuple[int, int]:
        layout = self._data.get("layout_mode", "Frozen Warframe")
        return DISPLAY_PROFILES.get(layout, (320, 240))