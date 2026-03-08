"""
Theme model: a named canvas layout with elements + persistence.
Each theme now stores the model it was designed for (width × height).
"""

import json
from pathlib import Path
from PIL import Image, ImageDraw

from .elements import (CanvasElement, TextElement, MetricElement,
                       BarElement, ImageElement, element_from_dict)

THEMES_FILE = Path("~/.config/neru-screen-control/themes.json").expanduser()

DEFAULT_W = 320
DEFAULT_H = 320


class Theme:
    def __init__(self, name: str = "New Theme",
                 model_name: str = "Frozen Warframe",
                 width: int = DEFAULT_W, height: int = DEFAULT_H):
        self.name       = name
        self.model_name = model_name
        self.width      = width
        self.height     = height
        self.bg_color   = (10, 10, 25)
        self.elements: list[CanvasElement] = []

    def render(self, metrics: dict, fonts: dict,
               target_w: int = None, target_h: int = None) -> Image.Image:
        w = self.width
        h = self.height
        img  = Image.new('RGB', (w, h), self.bg_color)
        draw = ImageDraw.Draw(img)
        for el in self.elements:
            try:
                el.render(draw, img, metrics, fonts)
            except Exception as e:
                print(f"Render error [{el.display_name()}]: {e}")
        if target_w and target_h and (target_w != w or target_h != h):
            img = img.resize((target_w, target_h), Image.LANCZOS)
        return img

    def to_dict(self) -> dict:
        return {
            'name':       self.name,
            'model_name': self.model_name,
            'width':      self.width,
            'height':     self.height,
            'bg_color':   list(self.bg_color),
            'elements':   [e.to_dict() for e in self.elements],
        }

    @classmethod
    def from_dict(cls, d: dict) -> 'Theme':
        t = cls(
            name=       d.get('name', 'Theme'),
            model_name= d.get('model_name', 'Frozen Warframe'),
            width=      d.get('width',  DEFAULT_W),
            height=     d.get('height', DEFAULT_H),
        )
        t.bg_color = tuple(d.get('bg_color', [10, 10, 25]))
        t.elements = [el for ed in d.get('elements', [])
                      if (el := element_from_dict(ed)) is not None]
        return t

    def copy(self) -> 'Theme':
        return Theme.from_dict(self.to_dict())

    def rescale_to(self, new_w: int, new_h: int) -> 'Theme':
        sx = new_w / self.width  if self.width  else 1.0
        sy = new_h / self.height if self.height else 1.0
        copy = self.copy()
        copy.width  = new_w
        copy.height = new_h
        for el in copy.elements:
            el.x = int(el.x * sx)
            el.y = int(el.y * sy)
            el.w = max(8, int(el.w * sx))
            el.h = max(8, int(el.h * sy))
        return copy


def default_theme(model_name: str = "Frozen Warframe",
                  w: int = DEFAULT_W, h: int = DEFAULT_H) -> Theme:
    t = Theme("Metrics", model_name=model_name, width=w, height=h)
    t.bg_color = (10, 10, 25)
    CPU = (0, 180, 255); GPU = (255, 100, 0); DIM = (140, 140, 140)
    sx = w / 320; sy = h / 320

    def _s(v, scale): return max(1, int(v * scale))

    def txt(x, y, s, sz, col):
        el = TextElement()
        el.x = _s(x, sx); el.y = _s(y, sy)
        el.text = s; el.font_size = _s(sz, min(sx, sy))
        el.color = col; el.w = _s(150, sx); el.h = _s(sz + 4, sy)
        return el

    def metric(x, y, key, sz, col, lbl='', show_lbl=False):
        el = MetricElement()
        el.x = _s(x, sx); el.y = _s(y, sy); el.metric = key
        el.font_size = _s(sz, min(sx, sy)); el.color = col
        el.label = lbl; el.show_label = show_lbl
        el.w = _s(150, sx); el.h = _s(sz + 4 + (18 if show_lbl else 0), sy)
        return el

    def bar(x, y, key, col):
        el = BarElement()
        el.x = _s(x, sx); el.y = _s(y, sy); el.metric = key
        el.w = _s(140, sx); el.h = _s(12, sy); el.fg_color = col
        return el

    t.elements += [
        txt(10,  8,   "CPU", 20, CPU), txt(170, 8, "GPU", 20, GPU),
        metric(10,  40,  'cpu_temp',      56, CPU),
        metric(170, 40,  'gpu_temp',      56, GPU),
        bar(10,  138, 'cpu_usage', CPU), bar(170, 138, 'gpu_usage', GPU),
        metric(10,  153, 'cpu_usage',     20, DIM),
        metric(170, 153, 'gpu_usage',     20, DIM),
        metric(10,  195, 'cpu_frequency', 20, CPU, 'FREQ', True),
        metric(170, 195, 'gpu_frequency', 20, GPU, 'FREQ', True),
        metric(10,  248, 'cpu_power',     20, DIM, 'PWR',  True),
        metric(170, 248, 'gpu_power',     20, DIM, 'PWR',  True),
    ]
    return t


def load_themes() -> list:
    try:
        THEMES_FILE.parent.mkdir(parents=True, exist_ok=True)
        if THEMES_FILE.exists():
            data   = json.loads(THEMES_FILE.read_text())
            themes = [Theme.from_dict(d) for d in data]
            if themes: return themes
    except Exception as e:
        print(f"load_themes: {e}")
    return [default_theme()]


def save_themes(themes: list):
    try:
        THEMES_FILE.parent.mkdir(parents=True, exist_ok=True)
        THEMES_FILE.write_text(
            json.dumps([t.to_dict() for t in themes], indent=2))
    except Exception as e:
        print(f"save_themes: {e}")