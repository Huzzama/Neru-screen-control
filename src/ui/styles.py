"""Shared colour/style constants for the TRCC UI (PySide6)."""

DARK_BG  = "#1a1a1a"
MID_BG   = "#2a2a2a"
LIGHT_BG = "#3a3a3a"
ACCENT   = "#C8A84B"   # gold — Thermalright brand colour
ACCENT2  = "#00dcdc"   # cyan — live-metric highlight
TEXT     = "#e0e0e0"
DIM      = "#777"
BORDER   = "#3a3a3a"

# ── Widget style helpers ───────────────────────────────────────────────────────

def btn(text="", bg=MID_BG):
    return (f"QPushButton{{background:{bg};color:{TEXT};"
            f"border:1px solid #444;padding:5px 12px;border-radius:4px;}}"
            f"QPushButton:hover{{background:{LIGHT_BG};border-color:{ACCENT};}}"
            f"QPushButton:pressed{{background:#222;}}")

BTN_ACCENT = (f"QPushButton{{background:{ACCENT};color:#1a1a1a;"
              f"border:none;padding:5px 12px;border-radius:4px;font-weight:bold;}}"
              f"QPushButton:hover{{background:#d4b45a;}}")

BTN_DANGER = (f"QPushButton{{background:{MID_BG};color:#e06060;"
              f"border:1px solid #553333;padding:5px 12px;border-radius:4px;}}"
              f"QPushButton:hover{{border-color:#e06060;}}")

def combo():
    return (f"QComboBox{{background:{MID_BG};color:{TEXT};"
            f"border:1px solid #444;padding:4px 8px;border-radius:3px;}}"
            f"QComboBox::drop-down{{border:none;width:18px;}}"
            f"QComboBox QAbstractItemView{{background:{MID_BG};color:{TEXT};"
            f"selection-background-color:{ACCENT};selection-color:#1a1a1a;}}")

def spinbox():
    return (f"QSpinBox,QDoubleSpinBox{{background:{MID_BG};color:{TEXT};"
            f"border:1px solid #444;border-radius:3px;padding:2px 4px;}}")

def lineedit():
    return (f"QLineEdit{{background:{MID_BG};color:{TEXT};"
            f"border:1px solid #444;border-radius:3px;padding:3px 6px;}}"
            f"QLineEdit:focus{{border-color:{ACCENT};}}")

def groupbox():
    return (f"QGroupBox{{color:{DIM};border:1px solid {BORDER};"
            f"border-radius:4px;margin-top:8px;padding-top:8px;font-size:11px;}}"
            f"QGroupBox::title{{subcontrol-origin:margin;left:8px;}}")

LIST = (f"QListWidget{{background:{MID_BG};color:{TEXT};"
        f"border:1px solid {BORDER};border-radius:4px;outline:none;}}"
        f"QListWidget::item{{padding:4px 8px;}}"
        f"QListWidget::item:selected{{background:{ACCENT};color:#1a1a1a;}}"
        f"QListWidget::item:hover{{background:{LIGHT_BG};}}")

SCROLL = (f"QScrollArea{{background:transparent;border:none;}}"
          f"QScrollBar:vertical{{background:{DARK_BG};width:6px;border-radius:3px;}}"
          f"QScrollBar::handle:vertical{{background:#555;border-radius:3px;min-height:20px;}}"
          f"QScrollBar::add-line:vertical,QScrollBar::sub-line:vertical{{height:0;}}")

APP = (f"QMainWindow,QWidget{{background:{DARK_BG};color:{TEXT};}}"
       f"QLabel{{color:{TEXT};}}"
       f"QToolTip{{background:{MID_BG};color:{TEXT};border:1px solid {BORDER};padding:4px;}}"
       + SCROLL)

def section_label_style():
    return f"color:{DIM};font-size:10px;letter-spacing:2px;padding:2px 0;"