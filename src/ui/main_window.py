"""
Neru Screen Control  (PySide6)

Tabs:
  Display  — mode, rotation, fps, model
  Themes   — *** NEW: canvas editor, live preview, theme persistence ***
  Media    — image / gif / video picker
  Metrics  — temperature units + live readout
  Device   — connection status + udev helper
  Debug    — test patterns, protocol override, log console
"""

import sys
import os
from pathlib import Path
from PIL import Image

from PySide6.QtWidgets import (
    QApplication, QMainWindow, QWidget, QVBoxLayout, QHBoxLayout,
    QPushButton, QLabel, QComboBox, QFileDialog, QGroupBox,
    QFormLayout, QTabWidget, QSpinBox, QTextEdit, QSizePolicy,
    QCheckBox, QSystemTrayIcon, QMenu,
)
from PySide6.QtCore import Qt, QTimer
from PySide6.QtGui  import QPixmap, QImage, QFont, QIcon, QColor, QPainter

sys.path.insert(0, str(Path(__file__).parent.parent))

from config.loader         import Config, DISPLAY_PROFILES
from metrics.collector     import MetricsCollector
from display.collector     import DisplayController
from display.frame_builder import build_metrics_frame
from display.protocol      import SCREEN_WIDTH, SCREEN_HEIGHT

from .theme_editor_tab  import ThemeEditorTab
from .calibration_tab   import CalibrationTab
from .help_tab          import HelpTab
from .settings_tab      import SettingsTab


# ── Style helpers ──────────────────────────────────────────────────────────────

DARK  = "#1a1a1a"
MID   = "#2a2a2a"
LIGHT = "#3a3a3a"
ACC   = "#00dcdc"
GOLD  = "#C8A84B"


def _btn(bg=MID):
    return (f"QPushButton{{background:{bg};color:#fff;"
            f"border:1px solid #444;padding:6px 14px;border-radius:4px;}}"
            f"QPushButton:hover{{background:{LIGHT};}}")


def _combo():
    return f"QComboBox{{background:{MID};color:#fff;padding:4px;border:1px solid #444;}}"


def _tabs_style():
    return (f"QTabWidget::pane{{border:1px solid #333;}}"
            f"QTabBar::tab{{background:{MID};color:#888;padding:6px 16px;}}"
            f"QTabBar::tab:selected{{background:{DARK};color:{GOLD};}}")


# ── Main window ────────────────────────────────────────────────────────────────

class MainWindow(QMainWindow):
    def __init__(self, config_path: str = "config.json",
                 start_hidden: bool = False):
        super().__init__()
        self.setWindowTitle("Neru Screen Control")
        self.setMinimumSize(900, 580)
        self.setStyleSheet(f"background:{DARK};color:#fff;")

        # Load app icon — looks for icon.png next to main.py (project root)
        self._app_icon = self._load_app_icon()
        if not self._app_icon.isNull():
            self.setWindowIcon(self._app_icon)
            QApplication.instance().setWindowIcon(self._app_icon)

        self.cfg = Config(config_path)

        self.metrics = MetricsCollector(
            interval = self.cfg.get("metrics_interval", 1.0),
            cpu_unit = self.cfg.get("cpu_temperature_unit", "celsius"),
            gpu_unit = self.cfg.get("gpu_temperature_unit", "celsius"),
        )
        self.metrics.start()

        self.display = DisplayController(self.metrics, self.cfg.as_dict())
        self.display.start()

        self._build_ui()
        self._build_tray()

        self._timer = QTimer()
        self._timer.timeout.connect(self._refresh)
        self._timer.start(500)

        if start_hidden or self.cfg.get("launch_hidden", False):
            # Don't show window — stay in tray only
            pass
        else:
            self.show()

    # ── Layout ─────────────────────────────────────────────────────────────────

    def _build_ui(self):
        central = QWidget()
        self.setCentralWidget(central)
        root = QHBoxLayout(central)
        root.setContentsMargins(12, 12, 12, 12)
        root.setSpacing(12)

        # Left: small preview + status
        left = QVBoxLayout()
        root.addLayout(left, 0)

        cap = QLabel("Display Preview")
        cap.setAlignment(Qt.AlignCenter)
        cap.setStyleSheet("color:#555;font-size:11px;")
        left.addWidget(cap)

        self._preview = QLabel()
        self._preview.setFixedSize(320, 320)
        self._preview.setStyleSheet("background:#000;border:1px solid #333;")
        self._preview.setAlignment(Qt.AlignCenter)
        left.addWidget(self._preview)

        self._conn_label = QLabel("● Connecting…")
        self._conn_label.setAlignment(Qt.AlignCenter)
        self._conn_label.setStyleSheet("color:#888;font-size:11px;")
        left.addWidget(self._conn_label)

        self._fps_label = QLabel("")
        self._fps_label.setAlignment(Qt.AlignCenter)
        self._fps_label.setStyleSheet("color:#555;font-size:10px;")
        left.addWidget(self._fps_label)
        left.addStretch()

        # Right: tabs
        self._tabs = QTabWidget()
        self._tabs.setStyleSheet(_tabs_style())
        root.addWidget(self._tabs, 1)

        self._tabs.addTab(self._tab_display(),            "🖥  Display")
        self._tabs.addTab(self._build_themes_tab(),        "🎨  Themes")
        self._tabs.addTab(self._tab_metrics(),             "📊  Metrics")
        self._tabs.addTab(self._tab_device(),              "🔌  Device")
        self._tabs.addTab(self._tab_debug(),               "🐞  Debug")
        self._tabs.addTab(self._build_calibration_tab(),   "🎯  Calibrate")
        self._tabs.addTab(self._build_settings_tab(),      "⚙  Settings")
        self._tabs.addTab(HelpTab(),                       "❓  Help")

    # ── Themes tab (new) ───────────────────────────────────────────────────────

    # ── Calibration tab ────────────────────────────────────────────────────────

    def _build_calibration_tab(self) -> QWidget:
        from display.autocalibration import load_calibration_from_config, save_calibration_result
        try:
            result = load_calibration_from_config(self.cfg)
            save_calibration_result(result, self.cfg)
        except Exception as e:
            print(f"[MainWindow] Could not load calibration from config: {e}")

        self._cal_tab = CalibrationTab(
            display_controller = self.display,
            config             = self.cfg,
        )
        self._cal_tab.calibration_saved.connect(self._on_calibration_saved)
        return self._cal_tab

    def _on_calibration_saved(self, result):
        """Keep Display-tab rotation combo in sync after calibration."""
        try:
            idx = [0, 90, 180, 270].index(result.rotation)
            self._rot_combo.setCurrentIndex(idx)
            self.display.set_rotation(result.rotation)
        except Exception:
            pass

    # ── Themes tab ─────────────────────────────────────────────────────────────

    def _build_themes_tab(self) -> QWidget:
        self._last_preview_pil = None

        def _send(pil_img: Image.Image):
            self._last_preview_pil = pil_img
            try:
                self.display.push_frame(pil_img)
            except Exception:
                pass

        def _snap() -> dict:
            return self.metrics.snapshot

        initial_model = self.cfg.get("layout_mode", "Frozen Warframe")
        self._theme_tab = ThemeEditorTab(
            send_frame_cb  = _send,
            metrics_cb     = _snap,
            initial_model  = initial_model,
        )

        try:
            self.metrics.updated.connect(
                lambda: self._theme_tab.update_metrics(self.metrics.snapshot))
        except Exception:
            pass

        return self._theme_tab

    # ── Tab: Display ───────────────────────────────────────────────────────────

    def _tab_display(self):
        w = QWidget(); lay = QFormLayout(w); lay.setSpacing(12)

        self._mode_combo = QComboBox()
        self._mode_combo.addItems(["metrics", "theme"])
        self._mode_combo.setCurrentText(self.cfg.get("display_mode", "metrics"))
        self._mode_combo.currentTextChanged.connect(self._on_mode_changed)
        self._mode_combo.setStyleSheet(_combo())
        self._mode_combo.setToolTip(
            "metrics — displays live sensor data using the built-in layout.\n"
            "theme — displays the custom theme from the Themes editor.")
        lay.addRow("Mode:", self._mode_combo)

        self._rot_combo = QComboBox()
        self._rot_combo.addItems(["0°", "90°", "180°", "270°"])
        self._rot_combo.setCurrentIndex(
            [0, 90, 180, 270].index(self.cfg.get("rotation", 0)))
        self._rot_combo.currentIndexChanged.connect(
            lambda i: (self.cfg.set("rotation", [0,90,180,270][i]),
                       self.display.set_rotation([0,90,180,270][i])))
        self._rot_combo.setStyleSheet(_combo())
        self._rot_combo.setToolTip(
            "Rotates the output image before sending to the display.\n"
            "Most Thermalright coolers mount the panel at 90° CCW,\n"
            "so 270° is the correct default for them.")
        lay.addRow("Rotation:", self._rot_combo)

        self._fps_spin = QSpinBox()
        self._fps_spin.setRange(1, 30)
        self._fps_spin.setValue(self.cfg.get("fps", 10))
        self._fps_spin.valueChanged.connect(lambda v: self.cfg.set("fps", v))
        self._fps_spin.setStyleSheet(f"background:{MID};color:#fff;padding:4px;")
        self._fps_spin.setToolTip(
            "Target frame rate sent to the display (1–30 fps).\n"
            "Higher values use more CPU. 10–15 fps is recommended for sensor displays.")
        lay.addRow("FPS:", self._fps_spin)

        self._model_combo = QComboBox()
        try:
            from models.models import MODEL_NAMES
            self._model_combo.addItems(MODEL_NAMES)
        except ImportError:
            self._model_combo.addItems(list(DISPLAY_PROFILES.keys()))
        self._model_combo.setCurrentText(
            self.cfg.get("layout_mode", "Frozen Warframe"))
        def _on_model(v):
            self.cfg.set("layout_mode", v)
            if hasattr(self, '_theme_tab'):
                self._theme_tab.set_model(v)
        self._model_combo.currentTextChanged.connect(_on_model)
        self._model_combo.setStyleSheet(_combo())
        self._model_combo.setToolTip(
            "Select your Thermalright cooler model.\n"
            "This sets the canvas resolution in the Themes editor\n"
            "and the output frame size sent to the display.")
        lay.addRow("Model:", self._model_combo)

        return w

    # ── Tab: Metrics ───────────────────────────────────────────────────────────

    def _tab_metrics(self):
        w = QWidget(); lay = QFormLayout(w); lay.setSpacing(12)

        self._cpu_unit = QComboBox()
        self._cpu_unit.addItems(["celsius", "fahrenheit"])
        self._cpu_unit.setCurrentText(
            self.cfg.get("cpu_temperature_unit", "celsius"))
        self._cpu_unit.currentTextChanged.connect(
            lambda v: self.cfg.set("cpu_temperature_unit", v))
        self._cpu_unit.setStyleSheet(_combo())
        lay.addRow("CPU temp unit:", self._cpu_unit)

        self._gpu_unit = QComboBox()
        self._gpu_unit.addItems(["celsius", "fahrenheit"])
        self._gpu_unit.setCurrentText(
            self.cfg.get("gpu_temperature_unit", "celsius"))
        self._gpu_unit.currentTextChanged.connect(
            lambda v: self.cfg.set("gpu_temperature_unit", v))
        self._gpu_unit.setStyleSheet(_combo())
        lay.addRow("GPU temp unit:", self._gpu_unit)

        self._metrics_live = QLabel()
        self._metrics_live.setStyleSheet(
            f"color:{ACC};font-family:monospace;font-size:12px;")
        lay.addRow("Live:", self._metrics_live)

        return w

    # ── Tab: Device ────────────────────────────────────────────────────────────

    def _tab_device(self):
        w = QWidget(); lay = QVBoxLayout(w); lay.setSpacing(10)

        self._dev_status = QLabel("Checking…")
        self._dev_status.setWordWrap(True)
        self._dev_status.setStyleSheet("font-family:monospace;font-size:11px;")
        lay.addWidget(self._dev_status)

        b = QPushButton("Copy udev rule to clipboard")
        b.setStyleSheet(_btn())
        b.clicked.connect(self._copy_udev_rule)
        lay.addWidget(b)
        lay.addStretch()
        return w

    # ── Tab: Debug ─────────────────────────────────────────────────────────────

    def _tab_debug(self):
        w = QWidget(); lay = QVBoxLayout(w); lay.setSpacing(10)

        grp = QGroupBox("Test Patterns")
        grp.setStyleSheet(
            f"QGroupBox{{color:#aaa;border:1px solid #333;padding:8px;"
            f"margin-top:6px;}}QGroupBox::title{{subcontrol-origin:margin;"
            f"left:8px;}}")
        glay = QHBoxLayout(grp)
        for label, rgb in [("Red",(255,0,0)), ("Green",(0,255,0)),
                            ("Blue",(0,0,255)), ("White",(255,255,255)),
                            ("Black",(0,0,0))]:
            b = QPushButton(label); b.setStyleSheet(_btn())
            b.clicked.connect(lambda _, c=rgb: self._send_solid(c))
            tips = {
                "Red":   "Solid red — use for dead-pixel detection.",
                "Green": "Solid green — use for dead-pixel detection.",
                "Blue":  "Solid blue — use for dead-pixel detection.",
                "White": "Full white — checks backlight uniformity.",
                "Black": "Full black — checks backlight bleed.",
            }
            b.setToolTip(tips.get(label, ""))
            glay.addWidget(b)
        b_chk = QPushButton("Checker"); b_chk.setStyleSheet(_btn())
        b_chk.setToolTip(
            "Black-and-white checker pattern.\n"
            "Use for pixel alignment: correct offset shows clean squares.\n"
            "Wrong offset shows diagonal lines or stripes.")
        b_chk.clicked.connect(self._send_checker)
        glay.addWidget(b_chk)
        lay.addWidget(grp)

        grp2 = QGroupBox("Protocol Override")
        grp2.setStyleSheet(grp.styleSheet())
        pform = QFormLayout(grp2)

        self._fmt_combo = QComboBox()
        self._fmt_combo.addItems([
            "rgb565_be","rgb565_le","bgr565_be","bgr565_le","rgb888","bgr888"])
        self._fmt_combo.setCurrentText("rgb565_be")
        self._fmt_combo.setStyleSheet(_combo())
        self._fmt_combo.setToolTip(
            "How pixel colours are encoded before sending to the display.\n"
            "rgb565_be — 16-bit big-endian (correct for all known Thermalright displays).\n"
            "rgb565_le — 16-bit little-endian (try this if colours look wrong).\n"
            "rgb888    — 24-bit (not used by current hardware).")
        pform.addRow("Pixel format:", self._fmt_combo)

        self._hdr_check = QCheckBox("Use capture header (1b 00 10 90 …)")
        self._hdr_check.setChecked(True)
        self._hdr_check.setStyleSheet("color:#aaa;")
        pform.addRow("", self._hdr_check)

        b_apply = QPushButton("Apply && send red test")
        b_apply.setStyleSheet(_btn())
        b_apply.clicked.connect(self._apply_protocol_override)
        pform.addRow("", b_apply)
        lay.addWidget(grp2)

        grp3 = QGroupBox("Log")
        grp3.setStyleSheet(grp.styleSheet())
        g3lay = QVBoxLayout(grp3)
        self._log = QTextEdit()
        self._log.setReadOnly(True)
        self._log.setMaximumHeight(120)
        self._log.setFont(QFont("Monospace", 9))
        self._log.setStyleSheet("background:#111;color:#0f0;border:none;")
        g3lay.addWidget(self._log)
        b_clear = QPushButton("Clear log"); b_clear.setStyleSheet(_btn())
        b_clear.clicked.connect(self._log.clear)
        g3lay.addWidget(b_clear, alignment=Qt.AlignRight)
        lay.addWidget(grp3)
        lay.addStretch()
        return w

    # ── Slot handlers ──────────────────────────────────────────────────────────

    def _on_mode_changed(self, mode: str):
        self.cfg.set("display_mode", mode)
        if mode == "theme":
            self._dc_set_theme_mode()
        else:
            self.display.set_mode("metrics")

    def _dc_set_theme_mode(self):
        """Put display controller in idle mode; theme tab pushes frames directly."""
        try:
            self.display.set_mode("theme")
        except Exception:
            pass

    def _copy_udev_rule(self):
        rule = ('SUBSYSTEM=="usb", ATTRS{idVendor}=="87ad", '
                'ATTRS{idProduct}=="70db", MODE="0666"')
        QApplication.clipboard().setText(rule)
        self._dev_status.setText(
            "Copied!\n\nPaste into:\n/etc/udev/rules.d/99-chizhou-display.rules\n\n"
            "Then run:\nsudo udevadm control --reload-rules && sudo udevadm trigger")

    def _send_solid(self, rgb):
        from display.protocol import encode_frame, ACTIVE_FORMAT, ACTIVE_ROTATION, ACTIVE_FLIP_Y
        from PIL import Image
        img    = Image.new("RGB", (320, 320), rgb)
        print(f"[DEBUG _send_solid] fmt={ACTIVE_FORMAT} rotation={ACTIVE_ROTATION} flip_y={ACTIVE_FLIP_Y}")
        packet = encode_frame(img)
        ok     = self.display._transport.send(packet)
        self._log_line(f"Solid {rgb} fmt={ACTIVE_FORMAT} rot={ACTIVE_ROTATION} → {'OK' if ok else 'FAIL'}")

    def _send_checker(self):
        from display.protocol import encode_frame, ACTIVE_FORMAT, ACTIVE_ROTATION, ACTIVE_FLIP_Y
        from PIL import Image, ImageDraw
        img = Image.new("RGB", (320, 320), (0, 0, 0))
        d   = ImageDraw.Draw(img)
        for y in range(0, 320, 20):
            for x in range(0, 320, 20):
                if (x // 20 + y // 20) % 2 == 0:
                    d.rectangle([x, y, x+19, y+19], fill=(255, 255, 255))
        print(f"[DEBUG _send_checker] fmt={ACTIVE_FORMAT} rotation={ACTIVE_ROTATION} flip_y={ACTIVE_FLIP_Y}")
        packet = encode_frame(img)
        ok     = self.display._transport.send(packet)
        self._log_line(f"Checker fmt={ACTIVE_FORMAT} rot={ACTIVE_ROTATION} → {'OK' if ok else 'FAIL'}")

    def _apply_protocol_override(self):
        import display.protocol as proto
        proto.ACTIVE_FORMAT   = self._fmt_combo.currentText()
        proto.ACTIVE_HEADER   = proto.FRAME_HEADER if self._hdr_check.isChecked() else None
        proto.ACTIVE_ROTATION = 0
        proto.ACTIVE_FLIP_Y   = False
        self._send_solid((255, 0, 0))
        self._log_line(f"Protocol override → fmt={proto.ACTIVE_FORMAT} rotation={proto.ACTIVE_ROTATION} flip_y={proto.ACTIVE_FLIP_Y}")

    def _log_line(self, text: str):
        self._log.append(text)

    # ── Refresh timer ──────────────────────────────────────────────────────────

    def _refresh(self):
        snap = self.metrics.snapshot
        mode = self.cfg.get("display_mode", "metrics")

        # Choose the correct PIL image to show in the left preview
        if mode == "theme" and self._last_preview_pil is not None:
            pil = self._last_preview_pil
        else:
            pil = build_metrics_frame(snap, self.cfg.as_dict())
            self._last_preview_pil = pil

        data  = pil.tobytes("raw", "RGB")
        qimg  = QImage(data, pil.width, pil.height, QImage.Format_RGB888)
        self._preview.setPixmap(QPixmap.fromImage(qimg))

        self._metrics_live.setText(
            f"CPU {snap['cpu_temp']}°C  {snap['cpu_usage']}%\n"
            f"GPU {snap['gpu_temp']}°C  {snap['gpu_usage']}%\n"
            f"RAM {snap['ram_usage']}%")

        connected = self.display._transport.connected
        if connected:
            self._conn_label.setText("● Connected")
            self._conn_label.setStyleSheet("color:#0d0;font-size:11px;")
            self._dev_status.setText("✓ Display connected")
            self._dev_status.setStyleSheet("color:#0d0;font-family:monospace;")
        else:
            self._conn_label.setText("● Not connected")
            self._conn_label.setStyleSheet("color:#d44;font-size:11px;")
            self._dev_status.setText(
                "✗ Display not connected\n\nCheck USB cable.\n"
                "Use button below for udev rule.")
            self._dev_status.setStyleSheet("color:#d44;font-family:monospace;")

        if self.display.frames_sent > 0:
            ms = self.display.last_frame_ms
            self._fps_label.setText(
                f"Frame {self.display.frames_sent}  last={ms:.1f}ms  "
                f"{'✓' if self.display.last_send_ok else '✗'}")

    # ── Settings tab ───────────────────────────────────────────────────────────

    def _build_settings_tab(self) -> QWidget:
        self._settings_tab = SettingsTab(config=self.cfg)
        return self._settings_tab

    # ── System tray ────────────────────────────────────────────────────────────

    def _build_tray(self):
        """Build the system tray icon and context menu."""
        # Generate a small coloured icon programmatically
        icon = self._make_tray_icon()
        self._tray = QSystemTrayIcon(icon, self)
        self._tray.setToolTip("Neru Screen Control")

        menu = QMenu()
        menu.setStyleSheet(
            f"QMenu{{background:{DARK};color:#fff;border:1px solid #333;}}"
            f"QMenu::item:selected{{background:{LIGHT};}}")

        act_show = menu.addAction("🖥  Open Neru Screen Control")
        act_show.triggered.connect(self._tray_show)

        menu.addSeparator()

        act_quit = menu.addAction("✕  Quit completely")
        act_quit.triggered.connect(self._quit_completely)

        self._tray.setContextMenu(menu)
        self._tray.activated.connect(self._on_tray_activated)

        # Show tray only if enabled in config
        if self.cfg.get("tray_icon", True):
            self._tray.show()

    @staticmethod
    def _find_icon_path() -> Path | None:
        """Search for icon.png relative to this file and common project roots."""
        candidates = [
            # Project root icon.png (one level above src/)
            Path(__file__).parent.parent / "icon.png",
            # Same folder as main_window.py
            Path(__file__).parent / "icon.png",
            # Current working directory
            Path("icon.png"),
        ]
        for p in candidates:
            if p.is_file():
                return p
        return None

    def _load_app_icon(self) -> QIcon:
        """Load icon.png; fall back to a drawn icon if file not found."""
        icon_path = self._find_icon_path()
        if icon_path:
            icon = QIcon(str(icon_path))
            if not icon.isNull():
                return icon
        return self._draw_fallback_icon()

    def _make_tray_icon(self) -> QIcon:
        """Return the app icon (icon.png or drawn fallback) for the tray."""
        return self._app_icon if hasattr(self, "_app_icon") else self._draw_fallback_icon()

    @staticmethod
    def _draw_fallback_icon() -> QIcon:
        """Draw a 32×32 teal 'N' icon when icon.png is not found."""
        px = QPixmap(32, 32)
        px.fill(QColor(0, 0, 0, 0))
        p = QPainter(px)
        p.setRenderHint(QPainter.Antialiasing)
        p.setBrush(QColor(0, 180, 180))
        p.setPen(QColor(0, 220, 220))
        p.drawRoundedRect(2, 2, 28, 28, 5, 5)
        p.setPen(QColor(255, 255, 255))
        p.setFont(QFont("Arial", 16, QFont.Bold))
        p.drawText(px.rect(), Qt.AlignCenter, "N")
        p.end()
        return QIcon(px)

    def _on_tray_activated(self, reason):
        if reason == QSystemTrayIcon.Trigger:   # single click
            self._tray_show()

    def _tray_show(self):
        self.showNormal()
        self.raise_()
        self.activateWindow()

    def _quit_completely(self):
        """Hard quit — stops all background work and exits."""
        self._do_shutdown()
        QApplication.quit()

    def _do_shutdown(self):
        """Stop timers and background threads."""
        try: self._timer.stop()
        except Exception: pass
        try: self._theme_tab._timer.stop()
        except Exception: pass
        try: self.display.stop()
        except Exception: pass
        try: self.metrics.stop()
        except Exception: pass

    # ── Close event ────────────────────────────────────────────────────────────

    def closeEvent(self, event):
        minimize = self.cfg.get("minimize_on_close", True)
        tray_ok  = self._tray.isSystemTrayAvailable() and self.cfg.get("tray_icon", True)

        if minimize and tray_ok:
            # Hide window, keep everything running, notify user once
            event.ignore()
            self.hide()
            self._tray.showMessage(
                "Neru Screen Control",
                "Running in the background. Right-click the tray icon to open or quit.",
                QSystemTrayIcon.Information,
                3000,
            )
        else:
            # Actually quit
            self._do_shutdown()
            event.accept()


# ── Entry point ────────────────────────────────────────────────────────────────

def launch_ui(config_path: str = "config.json", start_hidden: bool = False):
    app = QApplication.instance() or QApplication(sys.argv)
    # Prevent Qt from quitting when the last window is hidden (tray mode)
    app.setQuitOnLastWindowClosed(False)
    win = MainWindow(config_path, start_hidden=start_hidden)
    sys.exit(app.exec())


if __name__ == "__main__":
    launch_ui()