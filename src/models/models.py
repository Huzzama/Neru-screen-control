"""
Single source of truth for Thermalright display hardware specs.
Loads from thermalright_displays.json (co-located with this file).
"""

import json
from dataclasses import dataclass
from pathlib import Path

_JSON_PATH = Path(__file__).parent / "thermalright_displays.json"


@dataclass
class DisplayModel:
    name:             str
    screen_size_inch: float
    width:            int
    height:           int

    @property
    def aspect_ratio(self) -> float:
        return self.width / self.height if self.height else 1.0

    def __repr__(self):
        return (f"DisplayModel({self.name!r}, "
                f"{self.width}×{self.height}, {self.screen_size_inch}\")")


def _load() -> dict[str, DisplayModel]:
    try:
        data = json.loads(_JSON_PATH.read_text())
        return {
            name: DisplayModel(
                name=name,
                screen_size_inch=float(spec.get("screen_size_inch", 2.4)),
                width=int(spec["width"]),
                height=int(spec["height"]),
            )
            for name, spec in data.items()
        }
    except Exception as e:
        print(f"[models] Could not load {_JSON_PATH}: {e}")
        # Fallback: only Frozen Warframe
        return {
            "Frozen Warframe": DisplayModel(
                name="Frozen Warframe",
                screen_size_inch=2.4,
                width=320,
                height=240,
            )
        }


# Module-level registry — import this everywhere
DISPLAY_MODELS: dict[str, DisplayModel] = _load()
MODEL_NAMES:    list[str]               = list(DISPLAY_MODELS.keys())


def get_model(name: str) -> DisplayModel:
    """Return model by name, falling back to first available."""
    return DISPLAY_MODELS.get(name, next(iter(DISPLAY_MODELS.values())))