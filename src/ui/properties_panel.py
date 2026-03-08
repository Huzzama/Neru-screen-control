"""Properties panel: edits the selected CanvasElement (PySide6)."""

import os
from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QPushButton,
    QLineEdit, QSpinBox, QComboBox, QGroupBox,
    QFileDialog, QColorDialog, QCheckBox
)
from PySide6.QtCore import Signal
from PySide6.QtGui  import QColor

from .elements import (CanvasElement, TextElement, MetricElement,
                       BarElement, ImageElement, METRIC_LABELS)
from .styles   import (btn, combo, spinbox, lineedit, groupbox,
                       ACCENT, DIM, MID_BG, BORDER)


class PropertiesPanel(QWidget):
    changed = Signal()

    def __init__(self, parent=None):
        super().__init__(parent)
        self._el          = None
        self._canvas_w    = 9999
        self._canvas_h    = 9999
        self._loading     = False   # ← blocks changed during set_element

        root = QVBoxLayout(self)
        root.setContentsMargins(8, 8, 8, 8)
        root.setSpacing(6)

        self._title = QLabel("No element selected")
        self._title.setStyleSheet(
            f"color:{ACCENT};font-weight:bold;font-size:13px;")
        root.addWidget(self._title)

        grp = QGroupBox("Position && Size")
        grp.setStyleSheet(groupbox())
        gl  = QVBoxLayout(grp)
        r1  = QHBoxLayout(); r2 = QHBoxLayout()
        self._x = self._sp(0, 9999)
        self._y = self._sp(0, 9999)
        self._w = self._sp(1, 9999)
        self._h = self._sp(1, 9999)
        for lbl, sp, row in [("X", self._x, r1), ("Y", self._y, r1),
                              ("W", self._w, r2), ("H", self._h, r2)]:
            row.addWidget(QLabel(lbl)); row.addWidget(sp)
        gl.addLayout(r1); gl.addLayout(r2)
        root.addWidget(grp)

        self._dyn = QWidget()
        self._dl  = QVBoxLayout(self._dyn)
        self._dl.setContentsMargins(0, 0, 0, 0); self._dl.setSpacing(4)
        root.addWidget(self._dyn)
        root.addStretch()

        for sp in [self._x, self._y, self._w, self._h]:
            sp.valueChanged.connect(self._apply_geom)

    # ── Public ────────────────────────────────────────────────────────────────

    def set_canvas_size(self, w: int, h: int):
        self._canvas_w = w; self._canvas_h = h
        self._x.setRange(0, w - 1); self._y.setRange(0, h - 1)
        self._w.setRange(1, w);     self._h.setRange(1, h)

    def set_element(self, el):
        self._loading = True          # ← silence changed during construction
        try:
            self._el = el
            self._block(True)
            if el is None:
                self._title.setText("No element selected")
                self._clear_dyn(); return

            self._title.setText(type(el).__name__)
            self._x.setValue(el.x); self._y.setValue(el.y)
            self._w.setValue(el.w); self._h.setValue(el.h)
            self._clear_dyn()

            if isinstance(el, TextElement):     self._build_text(el)
            elif isinstance(el, MetricElement): self._build_metric(el)
            elif isinstance(el, BarElement):    self._build_bar(el)
            elif isinstance(el, ImageElement):  self._build_image(el)
        finally:
            self._block(False)
            self._loading = False     # ← re-enable changed

    def _emit_changed(self):
        """Emit changed only when not in the middle of loading an element."""
        if not self._loading:
            self.changed.emit()

    # ── Helpers ───────────────────────────────────────────────────────────────

    def _sp(self, mn, mx):
        s = QSpinBox(); s.setRange(mn, mx); s.setStyleSheet(spinbox())
        return s

    def _lbl(self, t):
        l = QLabel(t); l.setStyleSheet(f"color:{DIM};font-size:11px;"); return l

    def _color_btn(self, color, cb):
        r, g, b = color
        bw = QPushButton(); bw.setFixedSize(44, 24)
        bw.setStyleSheet(
            f"background:rgb({r},{g},{b});border:1px solid #555;border-radius:3px;")
        def pick():
            c = QColorDialog.getColor(QColor(r, g, b), self)
            if c.isValid():
                col = (c.red(), c.green(), c.blue())
                cb(col)
                bw.setStyleSheet(
                    f"background:{c.name()};border:1px solid #555;border-radius:3px;")
                self._emit_changed()
        bw.clicked.connect(pick)
        return bw

    def _color_row(self, label, color, cb):
        row = QWidget(); hl = QHBoxLayout(row)
        hl.setContentsMargins(0, 2, 0, 2)
        hl.addWidget(self._lbl(label)); hl.addStretch()
        hl.addWidget(self._color_btn(color, cb))
        self._dl.addWidget(row)

    def _metric_combo(self, current, cb):
        c = QComboBox(); c.setStyleSheet(combo())
        vals = list(METRIC_LABELS.values())
        # Block signals while populating so addItem/setCurrentIndex don't fire
        c.blockSignals(True)
        for k, v in METRIC_LABELS.items():
            c.addItem(k, v)
        if current in vals:
            c.setCurrentIndex(vals.index(current))
        c.blockSignals(False)
        # Only connect AFTER population is done
        c.currentIndexChanged.connect(
            lambda i: (cb(c.itemData(i)), self._emit_changed()))
        return c

    def _clear_dyn(self):
        while self._dl.count():
            item = self._dl.takeAt(0)
            if item.widget(): item.widget().deleteLater()

    # ── Element builders ──────────────────────────────────────────────────────

    def _build_text(self, el):
        self._dl.addWidget(self._lbl("Text"))
        te = QLineEdit(el.text); te.setStyleSheet(lineedit())
        te.blockSignals(True); te.setText(el.text); te.blockSignals(False)
        te.textChanged.connect(lambda v: (setattr(el, 'text', v), self._emit_changed()))
        self._dl.addWidget(te)

        self._dl.addWidget(self._lbl("Font size"))
        fs = QSpinBox(); fs.setRange(8, 256); fs.setStyleSheet(spinbox())
        fs.blockSignals(True); fs.setValue(el.font_size); fs.blockSignals(False)
        fs.valueChanged.connect(lambda v: (setattr(el, 'font_size', v), self._emit_changed()))
        self._dl.addWidget(fs)

        self._color_row("Color", el.color, lambda c: setattr(el, 'color', c))

    def _build_metric(self, el):
        self._dl.addWidget(self._lbl("Metric"))
        self._dl.addWidget(self._metric_combo(
            el.metric, lambda v: setattr(el, 'metric', v)))

        self._dl.addWidget(self._lbl("Label text"))
        le = QLineEdit(); le.setStyleSheet(lineedit())
        le.blockSignals(True); le.setText(el.label); le.blockSignals(False)
        le.textChanged.connect(lambda v: (setattr(el, 'label', v), self._emit_changed()))
        self._dl.addWidget(le)

        for text, attr in [("Show label", "show_label"), ("Show unit", "show_unit")]:
            chk = QCheckBox(text)
            chk.blockSignals(True); chk.setChecked(getattr(el, attr)); chk.blockSignals(False)
            chk.toggled.connect(lambda v, a=attr: (setattr(el, a, v), self._emit_changed()))
            self._dl.addWidget(chk)

        self._dl.addWidget(self._lbl("Font size"))
        fs = QSpinBox(); fs.setRange(8, 256); fs.setStyleSheet(spinbox())
        fs.blockSignals(True); fs.setValue(el.font_size); fs.blockSignals(False)
        fs.valueChanged.connect(lambda v: (setattr(el, 'font_size', v), self._emit_changed()))
        self._dl.addWidget(fs)

        self._color_row("Value color", el.color,
                        lambda c: setattr(el, 'color', c))
        self._color_row("Label color", el.label_color,
                        lambda c: setattr(el, 'label_color', c))

    def _build_bar(self, el):
        self._dl.addWidget(self._lbl("Metric"))
        self._dl.addWidget(self._metric_combo(
            el.metric, lambda v: setattr(el, 'metric', v)))

        self._dl.addWidget(self._lbl("Max value"))
        mx = QSpinBox(); mx.setRange(1, 10000); mx.setStyleSheet(spinbox())
        mx.blockSignals(True); mx.setValue(el.max_val); mx.blockSignals(False)
        mx.valueChanged.connect(lambda v: (setattr(el, 'max_val', v), self._emit_changed()))
        self._dl.addWidget(mx)

        self._color_row("Bar color", el.fg_color,
                        lambda c: setattr(el, 'fg_color', c))
        self._color_row("Background", el.bg_color,
                        lambda c: setattr(el, 'bg_color', c))

    def _build_image(self, el):
        lbl = QLabel(os.path.basename(el.path) if el.path else "No file")
        lbl.setStyleSheet(f"color:{DIM};font-size:11px;")
        b = QPushButton("Browse…"); b.setStyleSheet(btn())
        def browse():
            p, _ = QFileDialog.getOpenFileName(
                self, "Select Image / GIF / Video", "",
                "Media (*.png *.jpg *.jpeg *.bmp *.gif *.webp "
                "*.mp4 *.avi *.mkv *.webm *.mov *.wmv)")
            if not p:
                return
            # Video size guard
            from .elements import ImageElement as _IE
            import os as _os
            ext = _os.path.splitext(p)[1].lower()
            if ext in _IE.VIDEO_EXTS:
                try:
                    sz = _os.path.getsize(p)
                except OSError:
                    sz = 0
                if sz > _IE.VIDEO_MAX_BYTES:
                    from PySide6.QtWidgets import QMessageBox
                    limit_mb  = _IE.VIDEO_MAX_BYTES // (1024 * 1024)
                    actual_mb = sz / (1024 * 1024)
                    QMessageBox.warning(
                        self, "Video Too Large",
                        f"This video is {actual_mb:.0f} MB, which exceeds "
                        f"the {limit_mb} MB limit.\n\n"
                        f"Please use a shorter or lower-resolution clip.\n"
                        f"Tip: trim to ~10–30 seconds at 480p or lower.")
                    return
            el.path = p
            el._close_cap()
            el._src_frames = []; el._rnd_frames = []
            el._last_path  = ''; el._rnd_size   = (0, 0)
            el._is_video   = False; el._frame_idx = 0
            el._cached_frame = None; el._last_frame_idx = -1
            lbl.setText(os.path.basename(p))
            self._emit_changed()
        b.clicked.connect(browse)
        self._dl.addWidget(b); self._dl.addWidget(lbl)

    # ── Internal ──────────────────────────────────────────────────────────────

    def _block(self, b):
        for w in [self._x, self._y, self._w, self._h]:
            w.blockSignals(b)

    def _apply_geom(self):
        if self._loading or not self._el:
            return
        self._el.x = self._x.value(); self._el.y = self._y.value()
        self._el.w = self._w.value(); self._el.h = self._h.value()
        self._emit_changed()