"""
Neru Screen Control Settings Tab — Startup, Service, Device Access, Window behaviour.

Reads and writes:
  config keys:  start_on_login, minimize_on_close, launch_hidden, tray_icon
  systemd user service via service.manager.ServiceManager
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QPushButton,
    QCheckBox, QGroupBox, QTextEdit, QSizePolicy, QFrame,
)
from PySide6.QtCore  import Qt, QTimer, Signal
from PySide6.QtGui   import QFont, QColor

# ── Style constants (duplicated to keep this file self-contained) ─────────────

_DARK   = "#1a1a1a"
_MID    = "#2a2a2a"
_ACCENT = "#00dcdc"
_GOLD   = "#C8A84B"
_DIM    = "#888888"
_TEXT   = "#dddddd"
_GREEN  = "#00cc66"
_RED    = "#dd4444"
_ORANGE = "#dd8800"

def _grp(title: str) -> QGroupBox:
    g = QGroupBox(title)
    g.setStyleSheet(
        f"QGroupBox{{color:{_GOLD};border:1px solid #444;border-radius:4px;"
        f"margin-top:8px;padding:10px 8px 8px 8px;font-weight:bold;}}"
        f"QGroupBox::title{{subcontrol-origin:margin;left:10px;"
        f"padding:0 4px;color:{_GOLD};}}")
    return g

def _btn(text: str, color: str = _MID) -> QPushButton:
    b = QPushButton(text)
    b.setStyleSheet(
        f"QPushButton{{background:{color};color:#fff;border:1px solid #555;"
        f"padding:6px 16px;border-radius:4px;font-size:12px;}}"
        f"QPushButton:hover{{background:#3a3a3a;}}"
        f"QPushButton:disabled{{background:#222;color:#555;}}")
    return b

def _chk(text: str, tip: str = "") -> QCheckBox:
    c = QCheckBox(text)
    c.setStyleSheet(f"color:{_TEXT};font-size:12px;")
    if tip:
        c.setToolTip(tip)
    return c

def _lbl(text: str, dim: bool = False) -> QLabel:
    l = QLabel(text)
    l.setStyleSheet(f"color:{_DIM if dim else _TEXT};font-size:12px;")
    l.setWordWrap(True)
    return l

def _sep() -> QFrame:
    f = QFrame()
    f.setFrameShape(QFrame.HLine)
    f.setStyleSheet("color:#333;")
    return f

def _status_dot(ok: bool | None) -> str:
    if ok is True:  return f'<span style="color:{_GREEN};">●</span>'
    if ok is False: return f'<span style="color:{_RED};">●</span>'
    return f'<span style="color:{_ORANGE};">●</span>'


# ── Settings Tab ──────────────────────────────────────────────────────────────

class SettingsTab(QWidget):
    """
    Settings tab — one-stop shop for startup, service, and device access.

    Signals
    -------
    setting_changed(key, value)
        Emitted whenever a checkbox flips so MainWindow can update config.
    """

    setting_changed = Signal(str, object)

    def __init__(self, config=None, parent=None):
        super().__init__(parent)
        self._cfg = config
        self._svc = None   # ServiceManager class — loaded lazily via _svc_manager()

        root = QVBoxLayout(self)
        root.setContentsMargins(16, 16, 16, 16)
        root.setSpacing(14)

        root.addWidget(self._build_startup_group())
        root.addWidget(self._build_service_group())
        root.addWidget(self._build_device_group())
        root.addStretch()

        # Poll service status every 3 s
        self._poll = QTimer()
        self._poll.timeout.connect(self._refresh_service_status)
        self._poll.start(3000)
        self._refresh_service_status()

    # ── Config helpers ────────────────────────────────────────────────────────

    def _cfg_get(self, key: str, default=False):
        if self._cfg:
            return self._cfg.get(key, default)
        return default

    def _cfg_set(self, key: str, value):
        if self._cfg:
            self._cfg.set(key, value)
        self.setting_changed.emit(key, value)

    # ── Lazy service manager ──────────────────────────────────────────────────

    def _svc_manager(self):
        if self._svc is None:
            try:
                # Works whether run from src/ or installed
                from service.manager import ServiceManager
            except ImportError:
                try:
                    sys.path.insert(0, str(Path(__file__).parent.parent))
                    from service.manager import ServiceManager
                except ImportError:
                    return None
            self._svc = ServiceManager
        return self._svc

    # ── Startup group ─────────────────────────────────────────────────────────

    def _build_startup_group(self) -> QGroupBox:
        grp = _grp("⚙  Startup & Window Behaviour")
        vl  = QVBoxLayout(grp)
        vl.setSpacing(8)

        self._chk_start_login = _chk(
            "Start Neru Screen Control automatically when I log in",
            "Installs and enables a systemd user service that launches\n"
            "Neru Screen Control in background mode at login.")
        self._chk_start_login.setChecked(self._cfg_get("start_on_login"))
        self._chk_start_login.toggled.connect(self._on_start_login)
        vl.addWidget(self._chk_start_login)

        self._chk_minimize = _chk(
            "Keep running in background when window is closed",
            "Closing the window will hide it instead of quitting.\n"
            "The display keeps updating. Reopen from the system tray or launcher.")
        self._chk_minimize.setChecked(self._cfg_get("minimize_on_close", True))
        self._chk_minimize.toggled.connect(
            lambda v: self._cfg_set("minimize_on_close", v))
        vl.addWidget(self._chk_minimize)

        self._chk_hidden = _chk(
            "Launch hidden (no window on startup)",
            "When autostart is enabled, Open Neru Screen Control without showing the window.\n"
            "Access it later from the system tray.")
        self._chk_hidden.setChecked(self._cfg_get("launch_hidden"))
        self._chk_hidden.toggled.connect(
            lambda v: self._cfg_set("launch_hidden", v))
        vl.addWidget(self._chk_hidden)

        self._chk_tray = _chk(
            "Show system tray icon",
            "Adds a tray icon so you can show/hide the window\n"
            "and quit completely from the notification area.")
        self._chk_tray.setChecked(self._cfg_get("tray_icon", True))
        self._chk_tray.toggled.connect(
            lambda v: self._cfg_set("tray_icon", v))
        vl.addWidget(self._chk_tray)

        return grp

    # ── Service group ─────────────────────────────────────────────────────────

    def _build_service_group(self) -> QGroupBox:
        grp = _grp("🔧  Background Service  (systemd user)")
        vl  = QVBoxLayout(grp)
        vl.setSpacing(8)

        # Status line
        self._svc_status_lbl = QLabel("Status: checking…")
        self._svc_status_lbl.setStyleSheet(f"font-size:12px;color:{_DIM};")
        vl.addWidget(self._svc_status_lbl)

        # Button row
        br = QHBoxLayout()
        self._btn_start   = _btn("▶  Start",   "#1a6640")
        self._btn_stop    = _btn("■  Stop",    "#663333")
        self._btn_restart = _btn("↺  Restart", _MID)
        self._btn_install = _btn("📥  Install unit", _MID)
        self._btn_remove  = _btn("🗑  Remove unit",  "#444")

        self._btn_start.setToolTip("Start the background service now.")
        self._btn_stop.setToolTip("Stop the background service.\n"
                                   "The display will stop updating.")
        self._btn_restart.setToolTip("Stop then start the service.")
        self._btn_install.setToolTip(
            "Write the systemd unit file to ~/.config/systemd/user/.\n"
            "Does not start or enable it automatically.")
        self._btn_remove.setToolTip(
            "Stop, disable, and delete the unit file.")

        self._btn_start.clicked.connect(self._svc_start)
        self._btn_stop.clicked.connect(self._svc_stop)
        self._btn_restart.clicked.connect(self._svc_restart)
        self._btn_install.clicked.connect(self._svc_install)
        self._btn_remove.clicked.connect(self._svc_remove)

        for b in (self._btn_start, self._btn_stop, self._btn_restart,
                  self._btn_install, self._btn_remove):
            br.addWidget(b)
        br.addStretch()
        vl.addLayout(br)

        # Log output
        self._svc_log = QTextEdit()
        self._svc_log.setReadOnly(True)
        self._svc_log.setMaximumHeight(80)
        self._svc_log.setFont(QFont("Monospace", 9))
        self._svc_log.setStyleSheet(
            f"background:#111;color:#0f0;border:1px solid #333;")
        vl.addWidget(self._svc_log)

        return grp

    # ── Device access group ───────────────────────────────────────────────────

    def _build_device_group(self) -> QGroupBox:
        grp = _grp("🔌  USB Device Access  (udev rule)")
        vl  = QVBoxLayout(grp)
        vl.setSpacing(8)

        self._udev_status_lbl = QLabel()
        vl.addWidget(self._udev_status_lbl)

        vl.addWidget(_lbl(
            "Without this rule, Neru Screen Control needs root to talk to the USB display.\n"
            "Install it once — it persists across reboots.", dim=True))

        br2 = QHBoxLayout()
        self._btn_udev_install = _btn("🔑  Install (requires password)", "#1a4466")
        self._btn_udev_install.setToolTip(
            "Uses pkexec (graphical sudo) to write:\n"
            "/etc/udev/rules.d/99-chizhu-display.rules\n"
            "and reload udev.")
        self._btn_udev_copy = _btn("📋  Copy rule text", _MID)
        self._btn_udev_copy.setToolTip("Copy the rule to clipboard so you can install it manually.")

        self._btn_udev_install.clicked.connect(self._udev_install)
        self._btn_udev_copy.clicked.connect(self._udev_copy)
        br2.addWidget(self._btn_udev_install)
        br2.addWidget(self._btn_udev_copy)
        br2.addStretch()
        vl.addLayout(br2)

        self._udev_log = QTextEdit()
        self._udev_log.setReadOnly(True)
        self._udev_log.setMaximumHeight(60)
        self._udev_log.setFont(QFont("Monospace", 9))
        self._udev_log.setStyleSheet(
            f"background:#111;color:#aaa;border:1px solid #333;")
        vl.addWidget(self._udev_log)

        self._refresh_udev_status()
        return grp

    # ── Service actions ───────────────────────────────────────────────────────

    def _svc_start(self):
        sm = self._svc_manager()
        if not sm: return self._log_svc("systemd not available on this system.")
        ok, msg = sm.start()
        self._log_svc(("✓ " if ok else "✗ ") + msg)
        QTimer.singleShot(800, self._refresh_service_status)

    def _svc_stop(self):
        sm = self._svc_manager()
        if not sm: return self._log_svc("systemd not available.")
        ok, msg = sm.stop()
        self._log_svc(("✓ " if ok else "✗ ") + msg)
        QTimer.singleShot(800, self._refresh_service_status)

    def _svc_restart(self):
        sm = self._svc_manager()
        if not sm: return self._log_svc("systemd not available.")
        ok, msg = sm.restart()
        self._log_svc(("✓ " if ok else "✗ ") + msg)
        QTimer.singleShot(800, self._refresh_service_status)

    def _svc_install(self):
        sm = self._svc_manager()
        if not sm: return self._log_svc("systemd not available.")
        ok, msg = sm.install()
        self._log_svc(("✓ " if ok else "✗ ") + msg)
        QTimer.singleShot(500, self._refresh_service_status)

    def _svc_remove(self):
        sm = self._svc_manager()
        if not sm: return self._log_svc("systemd not available.")
        ok, msg = sm.uninstall()
        self._log_svc(("✓ " if ok else "✗ ") + msg)
        QTimer.singleShot(500, self._refresh_service_status)

    # ── Service status refresh ────────────────────────────────────────────────

    def _refresh_service_status(self):
        sm = self._svc_manager()
        if not sm:
            self._svc_status_lbl.setText(
                "Status: <span style='color:#888;'>systemd not available</span>")
            return

        s = sm.status()
        if not s.installed:
            dot  = _status_dot(None)
            text = f"{dot} <b>Not installed</b> — unit file not found"
            self._btn_start.setEnabled(False)
            self._btn_stop.setEnabled(False)
            self._btn_restart.setEnabled(False)
            self._btn_remove.setEnabled(False)
        else:
            dot  = _status_dot(s.active)
            run  = f"<b style='color:{'#00cc66' if s.active else '#dd4444'};'>" \
                   f"{'Running' if s.active else 'Stopped'}</b>"
            auto = (f"  •  autostart "
                    f"<b style='color:#00cc66;'>enabled</b>"
                    if s.enabled else
                    f"  •  autostart "
                    f"<b style='color:#888;'>disabled</b>")
            text = f"{dot} {run}{auto}  ({s.sub_state})"
            self._btn_start.setEnabled(not s.active)
            self._btn_stop.setEnabled(s.active)
            self._btn_restart.setEnabled(True)
            self._btn_remove.setEnabled(True)

        self._svc_status_lbl.setText(f"Status: {text}")

        # Sync the autostart checkbox without re-triggering signals
        if sm:
            self._chk_start_login.blockSignals(True)
            self._chk_start_login.setChecked(s.enabled if s.installed else False)
            self._chk_start_login.blockSignals(False)

    # ── udev actions ─────────────────────────────────────────────────────────

    def _udev_install(self):
        sm = self._svc_manager()
        if not sm:
            self._log_udev("Service manager not available.")
            return
        ok, msg = sm.install_udev_rule()
        self._log_udev(("✓ " if ok else "✗ ") + msg)
        self._refresh_udev_status()

    def _udev_copy(self):
        from PySide6.QtWidgets import QApplication
        sm = self._svc_manager()
        text = sm.udev_manual_instructions() if sm else (
            'SUBSYSTEM=="usb", ATTRS{idVendor}=="87ad", '
            'ATTRS{idProduct}=="70db", MODE="0666", TAG+="uaccess"')
        QApplication.clipboard().setText(text)
        self._log_udev("✓ Copied to clipboard.")

    def _refresh_udev_status(self):
        sm = self._svc_manager()
        if sm and sm.is_udev_installed():
            self._udev_status_lbl.setText(
                f"<span style='color:{_GREEN};'>● udev rule installed</span>  "
                f"— non-root USB access is active.")
            self._btn_udev_install.setEnabled(False)
            self._btn_udev_install.setText("✓  Already installed")
        else:
            self._udev_status_lbl.setText(
                f"<span style='color:{_ORANGE};'>● udev rule not installed</span>  "
                f"— root or rule required.")
            self._btn_udev_install.setEnabled(True)
            self._btn_udev_install.setText("🔑  Install (requires password)")

    # ── Autostart toggle ──────────────────────────────────────────────────────

    def _on_start_login(self, enabled: bool):
        self._cfg_set("start_on_login", enabled)
        sm = self._svc_manager()
        if not sm:
            self._log_svc("systemd not available — cannot set autostart.")
            return
        if enabled:
            ok, msg = sm.enable_autostart()
        else:
            ok, msg = sm.disable_autostart()
        self._log_svc(("✓ " if ok else "✗ ") + msg)
        QTimer.singleShot(600, self._refresh_service_status)

    # ── Log helpers ───────────────────────────────────────────────────────────

    def _log_svc(self, msg: str):
        self._svc_log.append(msg)

    def _log_udev(self, msg: str):
        self._udev_log.append(msg)

    # ── Public API ────────────────────────────────────────────────────────────

    def set_config(self, cfg):
        self._cfg = cfg
        self._chk_start_login.setChecked(cfg.get("start_on_login", False))
        self._chk_minimize.setChecked(cfg.get("minimize_on_close", True))
        self._chk_hidden.setChecked(cfg.get("launch_hidden", False))
        self._chk_tray.setChecked(cfg.get("tray_icon", True))