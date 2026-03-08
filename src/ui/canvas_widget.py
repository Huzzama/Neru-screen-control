"""
Model-driven interactive canvas (PySide6).

Zoom
────
User-controlled zoom (25 % – 400 %) stacks on top of the auto-fit scale.
  set_zoom(factor)   — set zoom (1.0 = fit, 2.0 = 2×, etc.)
  zoom_in / zoom_out — ±1 step from ZOOM_STEPS list

Smart Guides
────────────
Snap + guide threshold: 6 native pixels.
Cyan  = canvas edges / centre.
Orange = other-element edges / centres.

Bounding-box accuracy
─────────────────────
All rects are computed as (x1,y1)→(x2,y2) mapped independently, then
turned into QRect via QRect(p1, p2).  This avoids the rounding error that
appeared when scaling w/h separately from x/y.
"""

from PySide6.QtWidgets import QWidget, QSizePolicy
from PySide6.QtCore    import Qt, Signal, QPoint, QRect, QSize
from PySide6.QtGui     import (QPainter, QPen, QColor, QBrush,
                                QPixmap, QImage, QCursor)
from PIL import Image

# ── Constants ─────────────────────────────────────────────────────────────────

_H    = 6      # handle half-size (screen px)
_HIT  = 9      # hit-test radius  (screen px)
_MIN  = 4      # minimum element size (native px)
SNAP_PX = 6    # snap threshold (native px)

ZOOM_STEPS = [0.25, 0.33, 0.5, 0.67, 0.75, 1.0,
              1.25, 1.5,  2.0, 3.0,  4.0]

_GUIDE_CANVAS = QColor(0,   200, 255, 220)
_GUIDE_ELEM   = QColor(255, 100,   0, 200)

NONE = -1;  BODY = 0
TL=1; TC=2; TR=3; ML=4; MR=5; BL=6; BC=7; BR=8

_CURSORS = {
    BODY: Qt.SizeAllCursor,
    TL: Qt.SizeFDiagCursor, TR: Qt.SizeBDiagCursor,
    BL: Qt.SizeBDiagCursor, BR: Qt.SizeFDiagCursor,
    TC: Qt.SizeVerCursor,   BC: Qt.SizeVerCursor,
    ML: Qt.SizeHorCursor,   MR: Qt.SizeHorCursor,
}


# ── Widget ────────────────────────────────────────────────────────────────────

class CanvasWidget(QWidget):
    element_clicked  = Signal(int)
    element_geometry = Signal(int, int, int, int, int)
    element_resized  = Signal(int)   # emitted on mouseRelease after a resize drag
    zoom_changed     = Signal(float)   # emitted when zoom level changes

    # Base fit: longest edge maps to this many screen pixels
    FIT_PX = 480

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setSizePolicy(QSizePolicy.Fixed, QSizePolicy.Fixed)
        self.setMouseTracking(True)

        # Native display resolution
        self._native_w: int   = 320
        self._native_h: int   = 240

        # _fit_scale  — auto-fit so the canvas fills FIT_PX on longest edge
        # _zoom       — user zoom multiplier (1.0 = fit)
        # _total      — _fit_scale * _zoom  (used for all coordinate math)
        self._fit_scale: float = 1.0
        self._zoom:      float = 1.0
        self._total:     float = 1.0

        # Screen size of the canvas widget
        self._prev_w: int = 320
        self._prev_h: int = 240

        self._pix:     QPixmap | None = None
        self._img_buf: bytes   | None = None
        self._elements: list          = []
        self._selected: int           = NONE
        self._fonts:    dict          = {}    # PIL fonts — set by update_frame

        # Drag state
        self._dragging:    bool   = False
        self._drag_handle: int    = NONE
        self._drag_origin: QPoint = QPoint()
        self._drag_rect:   QRect | None = None
        self._drag_ratio:  float  = 1.0   # locked aspect ratio for this drag

        self._guides: list = []

        self._recalc_scale()

    # ── Public ────────────────────────────────────────────────────────────────

    def set_model(self, width: int, height: int):
        if width == self._native_w and height == self._native_h:
            return
        self._native_w = width
        self._native_h = height
        self._selected = NONE
        self._zoom     = 1.0
        self._recalc_scale()
        self.update()

    def set_zoom(self, factor: float):
        """Set zoom level (1.0 = fit-to-FIT_PX, 2.0 = double, etc.)"""
        self._zoom = max(ZOOM_STEPS[0], min(ZOOM_STEPS[-1], factor))
        self._recalc_scale()
        self.zoom_changed.emit(self._zoom)
        # Notify parent scroll area that our size changed
        if self.parent():
            self.parent().adjustSize()
        self.update()

    def zoom_in(self):
        cur = self._zoom
        nxt = next((z for z in ZOOM_STEPS if z > cur + 1e-6), ZOOM_STEPS[-1])
        self.set_zoom(nxt)

    def zoom_out(self):
        cur = self._zoom
        prv = next((z for z in reversed(ZOOM_STEPS) if z < cur - 1e-6), ZOOM_STEPS[0])
        self.set_zoom(prv)

    def zoom_reset(self):
        self.set_zoom(1.0)

    @property
    def zoom(self) -> float:
        return self._zoom

    def update_frame(self, pil_img: Image.Image, elements: list, fonts: dict = None):
        self._elements = elements
        if fonts is not None:
            self._fonts = fonts
        if self._dragging:
            self.update()
            return
        w, h = self._prev_w, self._prev_h
        if pil_img.width != w or pil_img.height != h:
            pil_img = pil_img.resize((w, h), Image.LANCZOS)
        self._img_buf = pil_img.convert("RGB").tobytes("raw", "RGB")
        qimg = QImage(self._img_buf, w, h, w * 3, QImage.Format_RGB888)
        self._pix = QPixmap.fromImage(qimg.copy())
        self.update()

    def set_selected(self, idx: int):
        self._selected = idx
        self.update()

    def set_elements(self, elements: list):
        self._elements = elements
        self.update()

    # ── Scale / size ──────────────────────────────────────────────────────────

    def _recalc_scale(self):
        """Recompute _fit_scale and _total, resize widget."""
        nw, nh = max(1, self._native_w), max(1, self._native_h)
        self._fit_scale = min(self.FIT_PX / nw, self.FIT_PX / nh, 1.0)
        self._total     = self._fit_scale * self._zoom
        self._prev_w    = max(1, int(nw * self._total))
        self._prev_h    = max(1, int(nh * self._total))
        self.setFixedSize(self._prev_w, self._prev_h)

    # ── Coordinate helpers ────────────────────────────────────────────────────
    # All mapping goes through _s (native→screen) and _n (screen→native).
    # Rects are derived from two *independently* mapped corners so that
    # floating-point rounding in w/h doesn't cause bounding-box drift.

    def _s(self, v: float) -> int:
        """Native px → screen px."""
        return int(round(v * self._total))

    def _n(self, v: int) -> float:
        """Screen px → native px (float, caller rounds as needed)."""
        return v / self._total

    def _el_rect_s(self, el) -> QRect:
        """
        Element rect in screen coords — uses el.measure(fonts) for text/metric
        so the box tightly wraps rendered content rather than stored w/h.
        Bar and Image elements return their stored w/h from measure().
        """
        mw, mh = el.measure(self._fonts) if self._fonts else (el.w, el.h)
        x1 = self._s(el.x)
        y1 = self._s(el.y)
        x2 = self._s(el.x + mw)
        y2 = self._s(el.y + mh)
        return QRect(x1, y1, max(1, x2 - x1), max(1, y2 - y1))

    def _handle_pts(self, el) -> list:
        r  = self._el_rect_s(el)
        x1, y1 = r.x(), r.y()
        x2, y2 = r.x() + r.width(), r.y() + r.height()
        cx, cy  = (x1 + x2) // 2, (y1 + y2) // 2
        return [None,
            (x1, y1), (cx, y1), (x2, y1),
            (x1, cy),           (x2, cy),
            (x1, y2), (cx, y2), (x2, y2),
        ]

    def _hit_handle(self, el, mx, my) -> int:
        for hid in [TL, TC, TR, ML, MR, BL, BC, BR]:
            hx, hy = self._handle_pts(el)[hid]
            if abs(mx - hx) <= _HIT and abs(my - hy) <= _HIT:
                return hid
        return NONE

    def _hit_body(self, el, mx, my) -> bool:
        return self._el_rect_s(el).contains(mx, my)

    # ── Smart Guides ─────────────────────────────────────────────────────────

    def _compute_guides(self, el) -> list:
        guides  = []
        W, H, T = self._native_w, self._native_h, SNAP_PX

        el_l  = el.x;            el_r  = el.x + el.w
        el_cx = el.x + el.w / 2; el_t  = el.y
        el_b  = el.y + el.h;     el_cy = el.y + el.h / 2

        refs_v = [
            (0,       _GUIDE_CANVAS), (W / 2, _GUIDE_CANVAS), (W, _GUIDE_CANVAS),
        ]
        refs_h = [
            (0,       _GUIDE_CANVAS), (H / 2, _GUIDE_CANVAS), (H, _GUIDE_CANVAS),
        ]
        for i, o in enumerate(self._elements):
            if i == self._selected:
                continue
            refs_v += [(o.x, _GUIDE_ELEM), (o.x + o.w / 2, _GUIDE_ELEM),
                       (o.x + o.w, _GUIDE_ELEM)]
            refs_h += [(o.y, _GUIDE_ELEM), (o.y + o.h / 2, _GUIDE_ELEM),
                       (o.y + o.h, _GUIDE_ELEM)]

        snapped_x = False
        for ref, color in refs_v:
            for own in [el_l, el_cx, el_r]:
                if abs(own - ref) <= T:
                    el.x  += ref - own
                    el_l   = el.x;  el_cx = el.x + el.w / 2;  el_r = el.x + el.w
                    guides.append(('v', self._s(ref), color))
                    snapped_x = True; break
            if snapped_x: break

        snapped_y = False
        for ref, color in refs_h:
            for own in [el_t, el_cy, el_b]:
                if abs(own - ref) <= T:
                    el.y  += ref - own
                    el_t   = el.y;  el_cy = el.y + el.h / 2;  el_b = el.y + el.h
                    guides.append(('h', self._s(ref), color))
                    snapped_y = True; break
            if snapped_y: break

        return guides

    # ── Paint ─────────────────────────────────────────────────────────────────

    def paintEvent(self, _):
        p = QPainter(self)
        p.setRenderHint(QPainter.Antialiasing, False)

        # 1. Background
        p.fillRect(0, 0, self._prev_w, self._prev_h, QColor(20, 20, 20))

        # 2. Frame pixmap
        if self._pix:
            p.drawPixmap(0, 0, self._pix)

        # 3. Drag ghost
        if self._dragging and 0 <= self._selected < len(self._elements):
            el = self._elements[self._selected]
            r  = self._el_rect_s(el)
            p.save()
            p.fillRect(r, QColor(255, 255, 255, 30))
            p.setPen(QPen(QColor(255, 220, 0), 2, Qt.SolidLine))
            p.setBrush(Qt.NoBrush)
            p.drawRect(r)
            p.restore()
            self._draw_handles(p, el)

        # 4. Static selection outline
        elif 0 <= self._selected < len(self._elements):
            el     = self._elements[self._selected]
            r      = self._el_rect_s(el)
            locked = getattr(el, 'locked', False)
            outline_color = QColor(220, 80, 80) if locked else QColor(255, 220, 0)
            p.save()
            p.setPen(QPen(outline_color, 2, Qt.DashLine))
            p.setBrush(Qt.NoBrush)
            p.drawRect(r)
            p.restore()
            if locked:
                p.save()
                from PySide6.QtGui import QFont as _QFont
                p.setFont(_QFont("Segoe UI Emoji", 10))
                p.setPen(QColor(220, 80, 80))
                p.drawText(r, Qt.AlignTop | Qt.AlignRight, "🔒")
                p.restore()
            else:
                self._draw_handles(p, el)

        # 5. Smart guides
        p.save()
        for axis, coord, color in self._guides:
            p.setPen(QPen(color, 1, Qt.SolidLine))
            if axis == 'v':
                p.drawLine(coord, 0, coord, self._prev_h)
            else:
                p.drawLine(0, coord, self._prev_w, coord)
        p.restore()

        # 6. Canvas border
        p.save()
        p.setPen(QPen(QColor(200, 168, 75), 2))
        p.setBrush(Qt.NoBrush)
        p.drawRect(0, 0, self._prev_w - 1, self._prev_h - 1)
        p.restore()

        p.end()

    def _draw_handles(self, p: QPainter, el):
        p.save()
        p.setPen(QPen(QColor(15, 15, 15), 1))
        p.setBrush(QBrush(QColor(255, 220, 0)))
        for hid in [TL, TC, TR, ML, MR, BL, BC, BR]:
            hx, hy = self._handle_pts(el)[hid]
            p.drawRect(hx - _H, hy - _H, _H * 2, _H * 2)
        p.restore()

    # ── Mouse ─────────────────────────────────────────────────────────────────

    def mousePressEvent(self, e):
        if e.button() != Qt.LeftButton:
            return
        mx, my = e.x(), e.y()

        # Priority: handles on selected element first (only if not locked)
        if 0 <= self._selected < len(self._elements):
            el  = self._elements[self._selected]
            if not getattr(el, 'locked', False):
                hid = self._hit_handle(el, mx, my)
                if hid != NONE:
                    self._start_drag(hid, el, mx, my); return
                if self._hit_body(el, mx, my):
                    self._start_drag(BODY, el, mx, my); return

        # Try to select topmost element
        for i in range(len(self._elements) - 1, -1, -1):
            el = self._elements[i]
            if self._hit_body(el, mx, my):
                self._selected = i
                self.element_clicked.emit(i)
                # Only start drag if not locked
                if not getattr(el, 'locked', False):
                    self._start_drag(BODY, el, mx, my)
                return

        self._selected = NONE
        self.element_clicked.emit(NONE)

    def wheelEvent(self, e):
        """Ctrl+Wheel = zoom in/out."""
        if e.modifiers() & Qt.ControlModifier:
            if e.angleDelta().y() > 0:
                self.zoom_in()
            else:
                self.zoom_out()
            e.accept()
        else:
            super().wheelEvent(e)

    def mouseMoveEvent(self, e):
        mx, my = e.x(), e.y()

        if not (e.buttons() & Qt.LeftButton):
            self._update_cursor(mx, my)
            return

        if not self._dragging or self._drag_rect is None:
            return

        # Delta in native coords (float → rounded to int for storage)
        dx = round(self._n(mx - self._drag_origin.x()))
        dy = round(self._n(my - self._drag_origin.y()))
        r  = self._drag_rect
        el = self._elements[self._selected]

        if self._drag_handle == BODY:
            el.x = r.x() + dx
            el.y = r.y() + dy
        else:
            # Start from the drag-start rect in native coords
            nx, ny = r.x(), r.y()
            nw, nh = r.width(), r.height()
            h = self._drag_handle

            # ── Apply raw delta to the affected edges ─────────────────────
            if h in (TL, ML, BL): nx = r.x() + dx;  nw = r.width()  - dx
            if h in (TR, MR, BR): nw = r.width()  + dx
            if h in (TL, TC, TR): ny = r.y() + dy;   nh = r.height() - dy
            if h in (BL, BC, BR): nh = r.height() + dy

            # ── Aspect-ratio lock on corner handles ───────────────────────
            # Use the frozen ratio captured at drag start, not live values.
            if h in (TL, TR, BL, BR):
                ratio = self._drag_ratio          # w / h at drag start
                if ratio >= 1.0:
                    # Wider-than-tall: width drives
                    nw   = max(_MIN, nw)
                    new_h = max(_MIN, int(round(nw / ratio)))
                    # Adjust anchor corner so the fixed corner stays put
                    if h in (TL, TR):             # top handles → top moves
                        ny = r.bottom() - new_h
                    nh = new_h
                else:
                    # Taller-than-wide: height drives
                    nh   = max(_MIN, nh)
                    new_w = max(_MIN, int(round(nh * ratio)))
                    if h in (TL, BL):             # left handles → left moves
                        nx = r.right() - new_w
                    nw = new_w

            # ── Clamp minimum size ────────────────────────────────────────
            if nw < _MIN:
                if h in (TL, ML, BL): nx = r.right() - _MIN
                nw = _MIN
            if nh < _MIN:
                if h in (TL, TC, TR): ny = r.bottom() - _MIN
                nh = _MIN

            el.x, el.y, el.w, el.h = nx, ny, nw, nh

        # Font-size tracking for text/metric elements — see _apply_font_size_from_height
        if self._drag_handle != BODY:
            self._apply_font_size_from_height(el)

        self._guides = self._compute_guides(el)
        self.element_geometry.emit(self._selected, el.x, el.y, el.w, el.h)
        self.update()

    def mouseReleaseEvent(self, e):
        if e.button() != Qt.LeftButton:
            return
        was_resize = (self._drag_handle != BODY and self._drag_handle != NONE)
        self._dragging    = False
        self._drag_handle = NONE
        self._drag_rect   = None
        self._guides      = []
        self._update_cursor(e.x(), e.y())
        if 0 <= self._selected < len(self._elements):
            el = self._elements[self._selected]
            self.element_geometry.emit(
                self._selected, el.x, el.y, el.w, el.h)
            if was_resize:
                self.element_resized.emit(self._selected)
        self.update()

    def _apply_font_size_from_height(self, el):
        """
        For TextElement / MetricElement: snap font_size to the largest
        available size that fits within the element's current height.

        The element height drives font size — this is the same model used
        by Figma/Canva for fixed-size text boxes.  Width is unrestricted.

        For BarElement / ImageElement this is a no-op.
        """
        from .elements import TextElement, MetricElement
        if not isinstance(el, (TextElement, MetricElement)):
            return
        if not self._fonts:
            return

        target_h = max(1, el.h)

        # Pick the largest font size whose measured height ≤ target_h.
        # Walk sizes high→low so we find the biggest fit first.
        best = None
        for sz in sorted(self._fonts.keys(), reverse=True):
            font = self._fonts[sz]
            try:
                bb = font.getbbox("Ag")          # representative ascenders/descenders
                fh = max(1, bb[3] - bb[1])
            except AttributeError:
                _, fh = font.getsize("Ag")       # type: ignore[attr-defined]
                fh = max(1, fh)
            if fh <= target_h:
                best = sz
                break

        if best is None:
            best = min(self._fonts.keys())       # smallest available

        if el.font_size != best:
            el.font_size = best
            # Snap el.h to the exact rendered height for the new font size
            # so the box stays tight vertically.  Measure directly from the
            # font object — can't use el.measure() here because user_sized=True
            # would short-circuit back to the old el.h.
            font = self._fonts[best]
            try:
                bb = font.getbbox("Ag")
                el.h = max(1, bb[3] - bb[1]) + 4   # +4 matches _PAD*2 in elements.py
            except AttributeError:
                _, fh = font.getsize("Ag")          # type: ignore[attr-defined]
                el.h = max(1, fh) + 4

    # ── Internals ─────────────────────────────────────────────────────────────

    def _start_drag(self, handle, el, mx, my):
        self._dragging    = True
        self._drag_handle = handle
        self._drag_origin = QPoint(mx, my)
        # Always use measure() as the authoritative size at drag start.
        # For BarElement/ImageElement measure() returns el.w/el.h unchanged.
        # For TextElement/MetricElement it returns the true rendered bounds
        # (or stored w/h if the user has already resized manually).
        mw, mh = el.measure(self._fonts) if self._fonts else (el.w, el.h)
        # Sync el.w/el.h so they match what we're about to resize from
        el.w, el.h = mw, mh
        # Mark as user-sized when grabbing a resize handle so measure()
        # honours el.w/el.h from here on instead of recomputing from fonts.
        if handle != BODY:
            el.user_sized = True
        self._drag_rect  = QRect(el.x, el.y, mw, mh)
        self._drag_ratio = mw / mh if mh else 1.0
        self._guides     = []
        self.setCursor(QCursor(_CURSORS.get(handle, Qt.SizeAllCursor)))

    def _update_cursor(self, mx, my):
        if 0 <= self._selected < len(self._elements):
            el  = self._elements[self._selected]
            hid = self._hit_handle(el, mx, my)
            if hid != NONE:
                self.setCursor(QCursor(_CURSORS[hid])); return
            if self._hit_body(el, mx, my):
                self.setCursor(QCursor(Qt.SizeAllCursor)); return
        self.setCursor(QCursor(Qt.ArrowCursor))