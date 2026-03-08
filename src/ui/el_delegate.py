"""
Custom QStyledItemDelegate for the element list in ThemeEditorTab.

Row layout (left → right):
  [eye] [lock] [kind icon] [name ............] [opacity bar bottom strip]

Data roles (set via QListWidgetItem.setData):
  EYE_ROLE   Qt.UserRole+1  bool   — element visible?
  LOCK_ROLE  Qt.UserRole+2  bool   — element locked?
  KIND_ROLE  Qt.UserRole+3  str    — 'text'|'metric'|'bar'|'image'
  OPAC_ROLE  Qt.UserRole+4  float  — future: 0-1 opacity (reserved)

Hit-test helpers:
  eye_rect_for_row(row_rect)   → QRect  — clickable eye area
  lock_rect_for_row(row_rect)  → QRect  — clickable lock area
"""
from __future__ import annotations

from PySide6.QtWidgets import QStyledItemDelegate, QStyle
from PySide6.QtCore    import Qt, QRect, QSize
from PySide6.QtGui     import (
    QPainter, QColor, QPen, QFont, QFontMetrics,
)

# ── Data roles ────────────────────────────────────────────────────────────────

EYE_ROLE  = Qt.UserRole + 1
LOCK_ROLE = Qt.UserRole + 2
KIND_ROLE = Qt.UserRole + 3
OPAC_ROLE = Qt.UserRole + 4

# ── Layout constants ──────────────────────────────────────────────────────────

ROW_H   = 38
ICON_W  = 22    # width of each icon cell (eye / lock)
PAD     = 4     # left edge padding

# Kind → (emoji, accent colour)
_KIND_MAP = {
    'text':   ('T',  QColor(200, 160,  80)),
    'metric': ('📊', QColor( 80, 200, 200)),
    'bar':    ('▬',  QColor( 80, 200, 140)),
    'image':  ('🖼', QColor( 80, 140, 200)),
}


class ElementDelegate(QStyledItemDelegate):

    def sizeHint(self, option, index) -> QSize:
        return QSize(option.rect.width(), ROW_H)

    def paint(self, painter: QPainter, option, index):
        painter.save()
        r        = option.rect
        selected = bool(option.state & QStyle.State_Selected)
        visible  = bool(index.data(EYE_ROLE))
        locked   = bool(index.data(LOCK_ROLE))
        kind     = (index.data(KIND_ROLE) or 'text').lower()

        # ── Row background ────────────────────────────────────────────────
        if selected:
            bg = QColor(30, 60, 120)
        elif index.row() % 2 == 0:
            bg = QColor(38, 38, 50)
        else:
            bg = QColor(32, 32, 44)
        painter.fillRect(r, bg)

        x = r.x() + PAD

        # ── Eye icon ──────────────────────────────────────────────────────
        eye_r = QRect(x, r.y() + (ROW_H - 16) // 2, 16, 16)
        if visible:
            # open eye: outer ellipse + pupil
            painter.setBrush(QColor(140, 190, 255) if selected else QColor(100, 160, 220))
            painter.setPen(Qt.NoPen)
            painter.drawEllipse(eye_r.adjusted(1, 4, -1, -4))
            painter.setBrush(QColor(20, 40, 80) if selected else QColor(30, 30, 55))
            painter.drawEllipse(eye_r.adjusted(5, 7, -5, -7))
        else:
            # closed eye: horizontal strike-through
            painter.setPen(QPen(QColor(80, 80, 100), 2))
            mid = eye_r.center().y()
            painter.drawLine(eye_r.left() + 1, mid, eye_r.right() - 1, mid)
        x += ICON_W

        # ── Lock icon ─────────────────────────────────────────────────────
        lock_r = QRect(x, r.y() + (ROW_H - 16) // 2, 16, 16)
        if locked:
            painter.setFont(QFont("Segoe UI Emoji", 9))
            painter.setPen(QColor(220, 175, 75))
            painter.drawText(lock_r, Qt.AlignCenter, "🔒")
        else:
            painter.setPen(QPen(QColor(55, 55, 70), 1))
            painter.setBrush(Qt.NoBrush)
            painter.drawRoundedRect(lock_r.adjusted(3, 3, -3, -3), 1, 1)
        x += ICON_W

        # ── Kind icon ─────────────────────────────────────────────────────
        icon_str, icon_col = _KIND_MAP.get(kind, ('◻', QColor(140, 140, 140)))
        kind_r = QRect(x, r.y() + (ROW_H - 16) // 2, 18, 16)
        painter.setFont(QFont("Segoe UI Emoji", 9))
        painter.setPen(icon_col)
        painter.drawText(kind_r, Qt.AlignCenter, icon_str)
        x += 22

        # ── Name ──────────────────────────────────────────────────────────
        name     = index.data(Qt.DisplayRole) or ""
        name_r   = QRect(x, r.y(), r.right() - x - 4, ROW_H)
        name_col = (QColor(220, 230, 255) if selected
                    else QColor(140, 140, 155) if not visible
                    else QColor(185, 185, 200))
        fn = QFont("Segoe UI", 9)
        painter.setFont(fn)
        painter.setPen(name_col)
        fm     = QFontMetrics(fn)
        elided = fm.elidedText(name, Qt.ElideRight, name_r.width())
        painter.drawText(name_r, Qt.AlignVCenter | Qt.AlignLeft, elided)

        # ── Opacity micro-bar (bottom strip) ──────────────────────────────
        opac = index.data(OPAC_ROLE)
        if opac is not None:
            bar_x = r.x() + PAD + ICON_W * 2 + 22
            bar_w = max(4, r.right() - bar_x - 4)
            bar_r = QRect(bar_x, r.bottom() - 2, bar_w, 2)
            painter.fillRect(bar_r, QColor(28, 28, 40))
            filled = QRect(bar_r.x(), bar_r.y(),
                           int(bar_r.width() * max(0., min(1., float(opac)))),
                           bar_r.height())
            painter.fillRect(filled,
                             QColor(60, 120, 220) if selected else QColor(65, 90, 155))

        # ── Row separator ─────────────────────────────────────────────────
        painter.setPen(QPen(QColor(25, 25, 36), 1))
        painter.drawLine(r.bottomLeft(), r.bottomRight())

        painter.restore()

    # ── Hit-test helpers ──────────────────────────────────────────────────────

    def eye_rect_for_row(self, row_rect: QRect) -> QRect:
        x = row_rect.x() + PAD
        return QRect(x, row_rect.y() + (ROW_H - 16) // 2, 16, 16)

    def lock_rect_for_row(self, row_rect: QRect) -> QRect:
        x = row_rect.x() + PAD + ICON_W
        return QRect(x, row_rect.y() + (ROW_H - 16) // 2, 16, 16)