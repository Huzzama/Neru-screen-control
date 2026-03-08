"""
ThemeEditorTab — model-driven interactive canvas editor.

The canvas size is always derived from the selected display model
(loaded from thermalright_displays.json).  Element coordinates are stored
in native model pixels, so a 320×240 theme is NOT the same as 640×480.

Cross-model load:  if a theme was saved for a different resolution,
the user is asked whether to rescale or keep raw coords.
"""

import os
from PIL import Image

from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QPushButton,
    QListWidget, QListWidgetItem, QScrollArea, QInputDialog, QMessageBox,
    QColorDialog, QFileDialog, QLineEdit, QComboBox, QSizePolicy,
    QSpacerItem,
)
from PySide6.QtCore    import Qt, QTimer, Signal, Slot
from PySide6.QtGui     import QColor, QKeySequence
from PySide6.QtGui     import QShortcut

from .elements  import (TextElement, MetricElement, BarElement,
                         ImageElement, METRIC_LABELS)
from .theme     import Theme, load_themes, save_themes, default_theme
from .canvas_widget    import CanvasWidget
from .properties_panel import PropertiesPanel
from .el_delegate import ElementDelegate, EYE_ROLE, LOCK_ROLE, KIND_ROLE, OPAC_ROLE
from .styles    import (btn, BTN_ACCENT, BTN_DANGER, combo, lineedit,
                         LIST, SCROLL, DARK_BG, MID_BG, ACCENT, DIM, BORDER,
                         section_label_style)

# Import model registry
try:
    from models.models import DISPLAY_MODELS, MODEL_NAMES, get_model
except ImportError:
    # Fallback if models package not on path
    from dataclasses import dataclass
    @dataclass
    class _M:
        name: str; width: int; height: int; screen_size_inch: float = 2.4
    DISPLAY_MODELS = {"Frozen Warframe": _M("Frozen Warframe", 320, 320)}
    MODEL_NAMES    = list(DISPLAY_MODELS.keys())
    def get_model(n): return DISPLAY_MODELS.get(n, list(DISPLAY_MODELS.values())[0])


def _load_fonts() -> dict:
    from PIL import ImageFont
    candidates = [
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationMono-Bold.ttf",
        "/usr/share/fonts/TTF/DejaVuSansMono-Bold.ttf",
        "/usr/share/fonts/dejavu/DejaVuSansMono-Bold.ttf",
        "/usr/share/fonts/truetype/freefont/FreeMono.ttf",
    ]
    path  = next((c for c in candidates if os.path.exists(c)), None)
    fonts = {}
    for sz in [8, 10, 12, 14, 16, 18, 20, 24, 28, 32, 40, 48, 56, 64, 72, 96, 128]:
        try:
            fonts[sz] = (ImageFont.truetype(path, sz)
                         if path else ImageFont.load_default())
        except Exception:
            fonts[sz] = ImageFont.load_default()
    return fonts


class ThemeEditorTab(QWidget):
    """Full theme editor as a single embeddable tab."""

    frame_ready = Signal(object)   # PIL Image

    def __init__(self, send_frame_cb=None, metrics_cb=None,
                 initial_model: str = "Frozen Warframe", parent=None):
        super().__init__(parent)
        self._send_frame = send_frame_cb
        self._metrics_cb = metrics_cb
        self._fonts      = _load_fonts()
        self._themes     = load_themes()
        self._theme_idx  = 0
        self._selected   = -1
        self._metrics    = {k: 0 for k in METRIC_LABELS.values()}

        # Active model
        mdl = get_model(initial_model)
        self._model_name = mdl.name
        self._canvas_w   = mdl.width
        self._canvas_h   = mdl.height

        self._build_ui()
        self._install_shortcuts()

        self._timer = QTimer(self)
        self._timer.timeout.connect(self._render)
        self._timer.start(100)   # 10 fps

        self._refresh_theme_list()
        self._render()

    # ── Build UI ──────────────────────────────────────────────────────────────

    def _build_ui(self):
        root = QHBoxLayout(self)
        root.setContentsMargins(0, 0, 0, 0)
        root.setSpacing(0)

        root.addWidget(self._build_sidebar())
        root.addWidget(self._build_center(), 1)
        root.addWidget(self._build_right())

    def _slabel(self, text: str) -> QLabel:
        l = QLabel(text); l.setStyleSheet(section_label_style()); return l

    # ── Left sidebar: theme list ───────────────────────────────────────────────

    def _build_sidebar(self) -> QWidget:
        w = QWidget(); w.setFixedWidth(170)
        w.setStyleSheet(
            f"QWidget{{background:{DARK_BG};border-right:1px solid {BORDER};}}")
        vl = QVBoxLayout(w); vl.setContentsMargins(8,12,8,8); vl.setSpacing(5)

        vl.addWidget(self._slabel("THEMES"))

        self._theme_list = QListWidget()
        self._theme_list.setStyleSheet(LIST)
        self._theme_list.currentRowChanged.connect(self._select_theme)
        vl.addWidget(self._theme_list, 1)

        for label, slot in [("＋  New",      self._new_theme),
                             ("⧉  Duplicate", self._dup_theme),
                             ("✕  Delete",    self._del_theme)]:
            b = QPushButton(label); b.setStyleSheet(btn())
            b.clicked.connect(slot); vl.addWidget(b)

        return w

    # ── Centre: model selector + canvas ───────────────────────────────────────

    def _build_center(self) -> QWidget:
        w  = QWidget()
        w.setStyleSheet(f"QWidget{{background:{DARK_BG};}}")
        vl = QVBoxLayout(w); vl.setContentsMargins(16,12,16,12); vl.setSpacing(8)

        # Name + save row
        nr = QHBoxLayout()
        self._name_edit = QLineEdit()
        self._name_edit.setStyleSheet(lineedit() + "font-size:15px;padding:5px;")
        self._name_edit.setPlaceholderText("Theme name…")
        self._name_edit.textChanged.connect(self._on_name_changed)
        nr.addWidget(self._name_edit)
        sv = QPushButton("💾  Save"); sv.setStyleSheet(BTN_ACCENT)
        sv.setFixedWidth(90); sv.clicked.connect(self._save)
        nr.addWidget(sv)
        vl.addLayout(nr)

        # ── Model selector row ────────────────────────────────────────────────
        mr = QHBoxLayout()
        mr.addWidget(QLabel("Display model:"))
        self._model_combo = QComboBox(); self._model_combo.setStyleSheet(combo())
        self._model_combo.addItems(MODEL_NAMES)
        self._model_combo.setCurrentText(self._model_name)
        self._model_combo.currentTextChanged.connect(self._on_model_changed)
        mr.addWidget(self._model_combo, 1)

        # Resolution label
        self._res_lbl = QLabel()
        self._res_lbl.setStyleSheet(f"color:{DIM};font-size:11px;")
        self._update_res_label()
        mr.addWidget(self._res_lbl)
        vl.addLayout(mr)

        # Canvas scroll area — only the canvas widget lives here.
        # Controls are OUTSIDE so they stay visible at any zoom level.
        sa = QScrollArea()
        sa.setWidgetResizable(False)
        sa.setAlignment(Qt.AlignCenter)
        sa.setStyleSheet(
            f"QScrollArea{{background:{MID_BG};border-radius:6px;"
            f"border:1px solid {BORDER};}}"
            f"QScrollBar:vertical{{background:#1a1a2a;width:8px;border:none;}}"
            f"QScrollBar::handle:vertical{{background:#3a3a5a;border-radius:4px;min-height:20px;}}"
            f"QScrollBar:horizontal{{background:#1a1a2a;height:8px;border:none;}}"
            f"QScrollBar::handle:horizontal{{background:#3a3a5a;border-radius:4px;min-width:20px;}}")

        # Canvas sits in a minimal centering container
        canvas_wrap = QWidget()
        canvas_wrap.setStyleSheet(f"background:{MID_BG};")
        cw_lay = QVBoxLayout(canvas_wrap)
        cw_lay.setAlignment(Qt.AlignCenter)
        cw_lay.setContentsMargins(12, 12, 12, 12)

        self._canvas = CanvasWidget()
        self._canvas.set_model(self._canvas_w, self._canvas_h)
        self._canvas.element_clicked.connect(self._on_canvas_click)
        self._canvas.element_geometry.connect(self._on_canvas_geometry)
        self._canvas.element_resized.connect(self._on_element_resized)
        self._canvas.zoom_changed.connect(self._on_zoom_changed)
        cw_lay.addWidget(self._canvas, 0, Qt.AlignCenter)

        sa.setWidget(canvas_wrap)
        vl.addWidget(sa, 1)   # ← scroll area takes all remaining vertical space

        # ── Zoom controls (OUTSIDE scroll area — always visible) ──────────
        zr = QHBoxLayout()
        zr.addStretch()
        for label, slot in [("－", self._canvas.zoom_out),
                             ("⊡", self._canvas.zoom_reset),
                             ("＋", self._canvas.zoom_in)]:
            zb = QPushButton(label); zb.setFixedSize(28, 22)
            zb.setStyleSheet(
                f"QPushButton{{background:#2a2a2a;color:#ccc;"
                f"border:1px solid #444;border-radius:3px;font-size:14px;}}"
                f"QPushButton:hover{{background:#3a3a3a;}}")
            zb.clicked.connect(slot)
            zr.addWidget(zb)
        self._zoom_lbl = QLabel("100%")
        self._zoom_lbl.setStyleSheet(
            f"color:{DIM};font-size:11px;min-width:42px;")
        self._zoom_lbl.setAlignment(Qt.AlignCenter)
        zr.addWidget(self._zoom_lbl)
        zr.addStretch()
        vl.addLayout(zr)

        # ── Rotation + BG colour (OUTSIDE scroll area — always visible) ───
        cr = QHBoxLayout()
        cr.addWidget(QLabel("Rotation:"))
        self._rot = QComboBox(); self._rot.setStyleSheet(combo())
        self._rot.setFixedWidth(75)
        for r in [0, 90, 180, 270]: self._rot.addItem(f"{r}°", r)
        self._rot.setCurrentIndex(3)
        cr.addWidget(self._rot); cr.addStretch()
        cr.addWidget(QLabel("BG:"))
        self._bg_btn = QPushButton()
        self._bg_btn.setFixedSize(48, 24)
        self._bg_btn.setStyleSheet(
            "background:rgb(10,10,25);border:1px solid #555;border-radius:3px;")
        self._bg_btn.clicked.connect(self._pick_bg)
        cr.addWidget(self._bg_btn)
        vl.addLayout(cr)
        return w

    # ── Right panel: add/list/properties ──────────────────────────────────────

    def _build_right(self) -> QWidget:
        w  = QWidget(); w.setFixedWidth(255)
        w.setStyleSheet(
            f"QWidget{{background:{MID_BG};border-left:1px solid {BORDER};}}")
        vl = QVBoxLayout(w); vl.setContentsMargins(8,12,8,8); vl.setSpacing(5)

        vl.addWidget(self._slabel("ADD ELEMENT"))

        for label, slot in [
            ("Tr   Text",          self._add_text),
            ("📊  Metric Value",   self._add_metric),
            ("▬   Progress Bar",   self._add_bar),
            ("🖼   Image / GIF / Video", self._add_image),
        ]:
            b = QPushButton(label); b.setStyleSheet(btn())
            b.clicked.connect(slot); vl.addWidget(b)

        vl.addSpacing(6)
        vl.addWidget(self._slabel("ELEMENTS"))

        self._el_list = QListWidget()
        self._el_list.setStyleSheet(LIST)
        self._el_list.setMaximumHeight(160)
        self._el_list.currentRowChanged.connect(self._on_list_select)
        # Install custom delegate for eye/lock/kind icons
        self._el_delegate = ElementDelegate()
        self._el_list.setItemDelegate(self._el_delegate)
        # Mouse click on eye/lock columns
        self._el_list.viewport().installEventFilter(self)
        vl.addWidget(self._el_list)

        # Row 1: move up/down + delete
        br = QHBoxLayout()
        for lbl, cb in [("▲", self._move_up), ("▼", self._move_down)]:
            b = QPushButton(lbl); b.setStyleSheet(btn()); b.setFixedWidth(36)
            b.clicked.connect(cb); br.addWidget(b)
        db = QPushButton("✕ Delete"); db.setStyleSheet(BTN_DANGER)
        db.clicked.connect(self._delete_el); br.addWidget(db)
        vl.addLayout(br)

        # Row 2: visibility + lock toggles + duplicate
        br2 = QHBoxLayout()
        self._vis_btn = QPushButton("👁 Vis")
        self._vis_btn.setStyleSheet(btn()); self._vis_btn.setToolTip("Toggle visibility (V)")
        self._vis_btn.clicked.connect(self._toggle_visibility)
        br2.addWidget(self._vis_btn)

        self._lock_btn = QPushButton("🔒 Lock")
        self._lock_btn.setStyleSheet(btn()); self._lock_btn.setToolTip("Toggle lock (L)")
        self._lock_btn.clicked.connect(self._toggle_lock)
        br2.addWidget(self._lock_btn)

        dup_btn = QPushButton("⧉ Dup")
        dup_btn.setStyleSheet(btn()); dup_btn.setToolTip("Duplicate (Ctrl+D)")
        dup_btn.clicked.connect(self._dup_element)
        br2.addWidget(dup_btn)
        vl.addLayout(br2)

        vl.addSpacing(6)
        vl.addWidget(self._slabel("PROPERTIES"))

        sa = QScrollArea(); sa.setWidgetResizable(True)
        sa.setStyleSheet("background:transparent;border:none;" + SCROLL)
        self._props = PropertiesPanel()
        self._props.set_canvas_size(self._canvas_w, self._canvas_h)
        self._props.changed.connect(self._render)
        sa.setWidget(self._props)
        vl.addWidget(sa, 1)

        return w

    # ── Model change ──────────────────────────────────────────────────────────

    def _on_model_changed(self, name: str):
        mdl = get_model(name)
        self._model_name = mdl.name
        new_w, new_h = mdl.width, mdl.height

        # Update canvas
        self._canvas_w = new_w
        self._canvas_h = new_h
        self._canvas.set_model(new_w, new_h)   # also resets zoom to 1.0
        self._zoom_lbl.setText("100%")
        self._props.set_canvas_size(new_w, new_h)
        self._update_res_label()

        # Update theme dimensions
        t = self._current()
        if t.width != new_w or t.height != new_h:
            t.width  = new_w
            t.height = new_h
            # Resize any full-canvas image elements to match
            for el in t.elements:
                if isinstance(el, ImageElement) and el.w == t.width and el.h == t.height:
                    el.w = new_w; el.h = new_h

        self._render()

    def set_model(self, name: str):
        """Called externally (e.g. from Display tab model combo)."""
        self._model_combo.setCurrentText(name)

    def _update_res_label(self):
        self._res_lbl.setText(f"{self._canvas_w} × {self._canvas_h}")

    def _on_zoom_changed(self, factor: float):
        pct = int(round(factor * 100))
        self._zoom_lbl.setText(f"{pct}%")

    # ── Theme management ───────────────────────────────────────────────────────

    def _current(self) -> Theme:
        return self._themes[self._theme_idx] if self._themes else Theme()

    def _refresh_theme_list(self):
        self._theme_list.blockSignals(True)
        self._theme_list.clear()
        for t in self._themes: self._theme_list.addItem(t.name)
        self._theme_list.setCurrentRow(self._theme_idx)
        self._theme_list.blockSignals(False)
        t = self._current()
        if hasattr(self, '_name_edit'):
            self._name_edit.blockSignals(True)
            self._name_edit.setText(t.name)
            self._name_edit.blockSignals(False)
        if hasattr(self, '_bg_btn'):
            r,g,b = t.bg_color
            self._bg_btn.setStyleSheet(
                f"background:rgb({r},{g},{b});border:1px solid #555;border-radius:3px;")
        if hasattr(self, '_el_list'):
            self._refresh_el_list()

    def _refresh_el_list(self):
        els = self._current().elements

        if not els:
            self._selected = -1
        elif self._selected < 0:
            self._selected = 0
        elif self._selected >= len(els):
            self._selected = len(els) - 1

        self._el_list.blockSignals(True)
        self._el_list.clear()

        # Kind lookup for the delegate
        _kind_map = {
            'TextElement':   'text',
            'MetricElement': 'metric',
            'BarElement':    'bar',
            'ImageElement':  'image',
        }

        for el in els:
            item = QListWidgetItem(el.display_name())
            item.setData(EYE_ROLE,  getattr(el, 'visible', True))
            item.setData(LOCK_ROLE, getattr(el, 'locked',  False))
            item.setData(KIND_ROLE, _kind_map.get(type(el).__name__, 'text'))
            self._el_list.addItem(item)

        self._el_list.setCurrentRow(self._selected)
        self._el_list.blockSignals(False)

        self._canvas.set_elements(els)
        self._canvas.set_selected(self._selected)
        self._props.set_element(els[self._selected] if 0 <= self._selected < len(els) else None)
        self._update_layer_buttons()

    def _select_theme(self, idx: int):
        if not (0 <= idx < len(self._themes)):
            return
        t = self._themes[idx]

        # Cross-model warning/rescale
        if (t.width != self._canvas_w or t.height != self._canvas_h):
            msg = QMessageBox(self)
            msg.setWindowTitle("Resolution mismatch")
            msg.setText(
                f"Theme \"{t.name}\" was created for "
                f"{t.width}×{t.height}.\n"
                f"Current model is {self._canvas_w}×{self._canvas_h}.\n\n"
                "Rescale element positions to fit current model?")
            msg.addButton("Rescale", QMessageBox.YesRole)
            msg.addButton("Keep raw", QMessageBox.NoRole)
            msg.addButton("Cancel", QMessageBox.RejectRole)
            result = msg.exec()
            if result == 2:   # Cancel
                # Restore list selection
                self._theme_list.blockSignals(True)
                self._theme_list.setCurrentRow(self._theme_idx)
                self._theme_list.blockSignals(False)
                return
            if result == 0:   # Rescale
                self._themes[idx] = t.rescale_to(self._canvas_w, self._canvas_h)

        self._theme_idx = idx
        self._selected  = -1
        self._canvas.set_selected(-1)
        self._props.set_element(None)
        self._refresh_theme_list()
        self._render()

    def _on_name_changed(self, name: str):
        self._current().name = name
        item = self._theme_list.currentItem()
        if item: item.setText(name)

    def _new_theme(self):
        name, ok = QInputDialog.getText(self, "New Theme", "Name:")
        if ok and name.strip():
            t = default_theme(
                model_name=self._model_name,
                w=self._canvas_w, h=self._canvas_h)
            t.name = name.strip()
            self._themes.append(t)
            self._theme_idx = len(self._themes) - 1
            self._refresh_theme_list(); self._render()

    def _dup_theme(self):
        t = self._current().copy(); t.name += " copy"
        self._themes.append(t)
        self._theme_idx = len(self._themes) - 1
        self._refresh_theme_list(); self._render()

    def _del_theme(self):
        if len(self._themes) <= 1:
            QMessageBox.warning(self, "Can't delete", "Need at least one theme.")
            return
        self._themes.pop(self._theme_idx)
        self._theme_idx = max(0, self._theme_idx - 1)
        self._refresh_theme_list(); self._render()

    def _save(self):
        save_themes(self._themes)

    def _pick_bg(self):
        t = self._current()
        c = QColorDialog.getColor(QColor(*t.bg_color), self)
        if c.isValid():
            t.bg_color = (c.red(), c.green(), c.blue())
            self._bg_btn.setStyleSheet(
                f"background:{c.name()};border:1px solid #555;border-radius:3px;")
            self._render()

    # ── Element management ─────────────────────────────────────────────────────

    def _add_el(self, el):
        self._current().elements.append(el)
        self._refresh_el_list()
        idx = len(self._current().elements) - 1
        self._el_list.setCurrentRow(idx)
        self._select_element(idx); self._render()

    def _add_text(self):
        el = TextElement(); el.x = 10; el.y = 10
        mw, mh = el.measure(self._fonts)
        el.w = mw; el.h = mh
        self._add_el(el)

    def _add_metric(self):
        el = MetricElement(); el.x = 10; el.y = 50
        mw, mh = el.measure(self._fonts)
        el.w = mw; el.h = mh
        self._add_el(el)

    def _add_bar(self):
        el = BarElement(); el.x=10; el.y=100
        el.w = max(80, self._canvas_w // 2)
        el.h = max(8, self._canvas_h // 30)
        self._add_el(el)

    def _add_image(self):
        path, _ = QFileDialog.getOpenFileName(
            self, "Select Image / GIF / Video", "",
            "Media (*.png *.jpg *.jpeg *.bmp *.gif *.webp "
            "*.mp4 *.avi *.mkv *.webm *.mov *.wmv)")
        if not path:
            return
        if not self._check_video_size(path):
            return
        el = ImageElement()
        el.path = path
        el.x = 0; el.y = 0
        el.w = self._canvas_w; el.h = self._canvas_h
        self._current().elements.insert(0, el)
        self._refresh_el_list(); self._render()

    def _check_video_size(self, path: str) -> bool:
        """
        If path is a video, check it is within the size limit.
        Shows a warning dialog with model-specific resolution hint.
        Returns True for images/GIFs unconditionally.
        """
        import os
        ext = os.path.splitext(path)[1].lower()
        from .elements import ImageElement as _IE
        if ext not in _IE.VIDEO_EXTS:
            return True
        try:
            size_bytes = os.path.getsize(path)
        except OSError:
            return True
        limit = _IE.VIDEO_MAX_BYTES
        if size_bytes > limit:
            limit_mb  = limit // (1024 * 1024)
            actual_mb = size_bytes / (1024 * 1024)
            rec_res   = f"{self._canvas_w}×{self._canvas_h}"
            QMessageBox.warning(
                self, "Video Too Large",
                f"This video is {actual_mb:.0f} MB, which exceeds the "
                f"{limit_mb} MB limit.\n\n"
                f"Please use a file under {limit_mb} MB.\n\n"
                f"Recommended resolution for your display ({self._model_name}):\n"
                f"  {rec_res} at 30 fps or lower\n\n"
                f"Quick re-encode with ffmpeg:\n"
                f"  ffmpeg -i input.mp4 -vf scale={self._canvas_w}:{self._canvas_h}"
                f" -r 30 output.mp4")
            return False
        return True

    def _delete_el(self):
        idx = self._selected; els = self._current().elements
        if 0 <= idx < len(els):
            els.pop(idx); self._selected = -1
            self._canvas.set_selected(-1); self._props.set_element(None)
            self._refresh_el_list(); self._render()

    def _move_up(self):
        idx = self._selected; els = self._current().elements
        if idx > 0:
            els[idx-1], els[idx] = els[idx], els[idx-1]
            self._selected -= 1
            self._refresh_el_list()
            self._el_list.setCurrentRow(self._selected); self._render()

    def _move_down(self):
        idx = self._selected; els = self._current().elements
        if idx < len(els) - 1:
            els[idx+1], els[idx] = els[idx], els[idx+1]
            self._selected += 1
            self._refresh_el_list()
            self._el_list.setCurrentRow(self._selected); self._render()

    def _toggle_visibility(self):
        """Toggle visible on selected element."""
        els = self._current().elements
        if 0 <= self._selected < len(els):
            el = els[self._selected]
            el.visible = not el.visible
            self._refresh_el_list(); self._render()

    def _toggle_lock(self):
        """Toggle locked on selected element."""
        els = self._current().elements
        if 0 <= self._selected < len(els):
            el = els[self._selected]
            el.locked = not el.locked
            self._refresh_el_list(); self._render()

    def _dup_element(self):
        """Duplicate selected element (Ctrl+D)."""
        els = self._current().elements
        if not (0 <= self._selected < len(els)):
            return
        import copy
        clone = copy.deepcopy(els[self._selected])
        clone.x += 8; clone.y += 8   # offset so it's visible
        ins = self._selected + 1
        els.insert(ins, clone)
        self._selected = ins
        self._refresh_el_list(); self._render()

    def _update_layer_buttons(self):
        """Sync Vis/Lock button labels to the selected element's state."""
        if not hasattr(self, '_vis_btn'):
            return
        els = self._current().elements
        if 0 <= self._selected < len(els):
            el = els[self._selected]
            self._vis_btn.setText("👁 Hide" if el.visible else "👁 Show")
            self._lock_btn.setText("🔓 Unlock" if el.locked else "🔒 Lock")
        else:
            self._vis_btn.setText("👁 Vis")
            self._lock_btn.setText("🔒 Lock")

    def _install_shortcuts(self):
        """Keyboard shortcuts for the editor."""
        sc = lambda key, fn: QShortcut(QKeySequence(key), self).activated.connect(fn)

        sc("Delete",          self._delete_el)
        sc("Ctrl+D",          self._dup_element)
        sc("V",               self._toggle_visibility)
        sc("L",               self._toggle_lock)
        # Arrow nudge — 1px
        sc("Up",    lambda: self._nudge(0,  -1))
        sc("Down",  lambda: self._nudge(0,   1))
        sc("Left",  lambda: self._nudge(-1,  0))
        sc("Right", lambda: self._nudge(1,   0))
        # Shift+Arrow nudge — 10px
        sc("Shift+Up",    lambda: self._nudge(0,  -10))
        sc("Shift+Down",  lambda: self._nudge(0,   10))
        sc("Shift+Left",  lambda: self._nudge(-10,  0))
        sc("Shift+Right", lambda: self._nudge(10,   0))

    def _nudge(self, dx: int, dy: int):
        """Move selected element by (dx, dy) native pixels."""
        els = self._current().elements
        if not (0 <= self._selected < len(els)):
            return
        el = els[self._selected]
        if getattr(el, 'locked', False):
            return
        el.x += dx; el.y += dy
        self._props.set_element(el)   # sync spinboxes
        self._render()

    def eventFilter(self, obj, event):
        """Handle clicks on eye/lock columns in el_list viewport."""
        from PySide6.QtCore import QEvent
        if obj is self._el_list.viewport() and event.type() == QEvent.MouseButtonPress:
            pos   = event.pos()
            item  = self._el_list.itemAt(pos)
            if item is None:
                return False
            row      = self._el_list.row(item)
            row_rect = self._el_list.visualItemRect(item)
            eye_r    = self._el_delegate.eye_rect_for_row(row_rect)
            lock_r   = self._el_delegate.lock_rect_for_row(row_rect)

            els = self._current().elements
            if not (0 <= row < len(els)):
                return False

            if eye_r.contains(pos):
                els[row].visible = not els[row].visible
                self._refresh_el_list(); self._render()
                return True   # swallow click — don't change selection

            if lock_r.contains(pos):
                els[row].locked = not els[row].locked
                self._refresh_el_list(); self._render()
                return True
        return False

    def _select_element(self, idx: int):
        els = self._current().elements

        if idx < 0 or idx >= len(els):
            self._selected = -1
            self._canvas.set_selected(-1)
            self._props.set_element(None)

            self._el_list.blockSignals(True)
            self._el_list.setCurrentRow(-1)
            self._el_list.blockSignals(False)

            self._canvas.update()
            return

        self._selected = idx
        self._canvas.set_elements(els)
        self._canvas.set_selected(idx)
        self._props.set_element(els[idx])

        self._el_list.blockSignals(True)
        self._el_list.setCurrentRow(idx)
        self._el_list.blockSignals(False)

        self._canvas.update()

    def _on_canvas_click(self, idx: int):
        """Called on click — fully sync canvas, list, and properties."""
        els = self._current().elements

        self._selected = idx
        self._canvas.set_elements(els)
        self._canvas.set_selected(idx)
        self._props.set_element(els[idx] if 0 <= idx < len(els) else None)

        self._el_list.blockSignals(True)
        self._el_list.setCurrentRow(idx)
        self._el_list.blockSignals(False)

        self._canvas.update()

    def _on_canvas_geometry(self, idx: int, x: int, y: int, w: int, h: int):
        """
        Fired continuously during drag — only update the properties spinboxes.
        Do NOT trigger a PIL re-render here (that causes the yellow flash).
        """
        els = self._current().elements
        if 0 <= idx < len(els):
            el = els[idx]
            # Directly push values to panel without going through set_element
            # (which would re-build the whole panel)
            try:
                self._props._block(True)
                self._props._x.setValue(el.x)
                self._props._y.setValue(el.y)
                self._props._w.setValue(el.w)
                self._props._h.setValue(el.h)
                self._props._block(False)
            except Exception:
                pass

    def _on_element_resized(self, idx: int):
        """
        Fired once on mouseRelease after a resize drag.
        Rebuilds the properties panel so font_size spinbox reflects any
        snapped value, then triggers a full PIL re-render.
        """
        els = self._current().elements
        if 0 <= idx < len(els):
            self._props.set_element(els[idx])
        self._render()

    def _on_list_select(self, row: int):
        els = self._current().elements

        if row < 0 or row >= len(els):
            self._selected = -1
            self._canvas.set_selected(-1)
            self._props.set_element(None)
            self._canvas.update()
            return

        self._selected = row
        self._canvas.set_elements(els)
        self._canvas.set_selected(row)
        self._props.set_element(els[row])
        self._canvas.update()

    # ── Render ─────────────────────────────────────────────────────────────────

    @Slot()
    def _render(self):
        if self._metrics_cb:
            try: self._metrics.update(self._metrics_cb())
            except Exception: pass

        # Tick animated elements
        for el in self._current().elements:
            if hasattr(el, 'tick'):
                el.tick()

        t   = self._current()
        img = t.render(self._metrics, self._fonts)
        self._canvas.update_frame(img, t.elements, self._fonts)
        self.frame_ready.emit(img)

        if self._send_frame:
            try: self._send_frame(img)
            except Exception: pass

    # ── Public helpers ─────────────────────────────────────────────────────────

    def update_metrics(self, m: dict):
        self._metrics.update(m)

    def current_theme(self) -> Theme:
        return self._current()