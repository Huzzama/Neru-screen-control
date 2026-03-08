"""
Calibration Tab — UI frontend for src/display/autocalibration.py

Provides two calibration modes accessible from the same tab:
  1. Manual step-through  — user presses ← / → to cycle offsets
  2. Auto-cycle           — app cycles automatically, user watches the screen

The backend logic (pattern generation, offset search, persistence) lives
entirely in display.autocalibration.  This file only owns the UI layer.

How to add this tab to MainWindow
----------------------------------
    from ui.calibration_tab import CalibrationTab

    self._cal_tab = CalibrationTab(
        display_controller=self._display,
        config=self._config,
    )
    self._tabs.addTab(self._cal_tab, "Calibrate")
"""

from __future__ import annotations

import threading

import numpy as np
from PIL import Image

from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel,
    QPushButton, QSpinBox, QComboBox, QGroupBox,
    QCheckBox, QProgressBar,
)
from PySide6.QtCore import Qt, QTimer, Signal, QObject
from PySide6.QtGui  import QPixmap, QImage

from .styles import (btn, BTN_ACCENT, BTN_DANGER, combo, spinbox, groupbox,
                     ACCENT, DIM, TEXT, MID_BG)

from display.autocalibration import (
    ManualCalibrationSession,
    run_auto_cycle_calibration,
    save_calibration_result,
    build_offset_label_pattern,
    push_preview_frame,
    DISPLAY_PROFILES, DEFAULT_PROFILE,
    CalibrationResult,
)


# ── Qt signal bridge for cross-thread UI updates ──────────────────────────────

class _Signals(QObject):
    progress = Signal(int, int, int)   # offset, index, total
    finished = Signal(list)            # tested offsets


# ── PIL → QPixmap helper ──────────────────────────────────────────────────────

def _to_pixmap(img: Image.Image) -> QPixmap:
    data = img.tobytes("raw", "RGB")
    qi   = QImage(data, img.width, img.height, QImage.Format_RGB888)
    return QPixmap.fromImage(qi)


# ─────────────────────────────────────────────────────────────────────────────

class CalibrationTab(QWidget):
    """
    UI calibration tab.

    Signals
    -------
    calibration_saved(CalibrationResult)
        Emitted after the user confirms and saves a result.
    """

    calibration_saved = Signal(object)   # CalibrationResult

    def __init__(self, display_controller=None, config=None, parent=None):
        super().__init__(parent)
        self._dc       = display_controller
        self._config   = config
        self._session: ManualCalibrationSession | None = None
        self._sigs     = _Signals()
        self._auto_thread: threading.Thread | None = None
        self._auto_stop    = threading.Event()

        self._build_ui()
        self._load_saved_values()
        self._refresh_preview(0)

        # Wire cross-thread signals
        self._sigs.progress.connect(self._on_auto_progress)
        self._sigs.finished.connect(self._on_auto_finished)

    # ─────────────────────────────────────────────────────────────────────────
    # UI construction
    # ─────────────────────────────────────────────────────────────────────────

    def _build_ui(self):
        root = QVBoxLayout(self)
        root.setContentsMargins(12, 12, 12, 12)
        root.setSpacing(10)

        # Title + hint
        t = QLabel("Display Calibration")
        t.setStyleSheet(f"color:{ACCENT};font-size:15px;font-weight:bold;")
        root.addWidget(t)

        hint = QLabel(
            "Goal: RED band at the top, YELLOW band at the bottom.\n"
            "Unique corners (magenta=TL, cyan=TR, orange=BL, purple=BR) "
            "reveal flips and wrong rotations."
        )
        hint.setStyleSheet(f"color:{DIM};font-size:11px;")
        hint.setWordWrap(True)
        root.addWidget(hint)

        # ── Profile & rotation ────────────────────────────────────────────────
        prof_grp = QGroupBox("Display Profile")
        prof_grp.setStyleSheet(groupbox())
        pl = QVBoxLayout(prof_grp)

        row_p = QHBoxLayout()
        row_p.addWidget(self._lbl("Profile"))
        self._profile_combo = QComboBox()
        self._profile_combo.setStyleSheet(combo())
        for name in DISPLAY_PROFILES:
            self._profile_combo.addItem(name)
        self._profile_combo.setCurrentText(DEFAULT_PROFILE)
        self._profile_combo.currentIndexChanged.connect(self._on_profile_changed)
        row_p.addWidget(self._profile_combo)
        pl.addLayout(row_p)

        row_r = QHBoxLayout()
        row_r.addWidget(self._lbl("Rotation"))
        self._rot_combo = QComboBox()
        self._rot_combo.setStyleSheet(combo())
        for deg in (0, 90, 180, 270):
            self._rot_combo.addItem(f"{deg}°", deg)
        self._rot_combo.setCurrentIndex(3)   # 270° default for physical 90° CCW mount
        self._rot_combo.currentIndexChanged.connect(lambda: self._refresh_preview())
        row_r.addWidget(self._rot_combo)
        pl.addLayout(row_r)

        row_f = QHBoxLayout()
        self._flip_chk = QCheckBox("Flip vertical (flip_y)")
        self._flip_chk.setStyleSheet(f"color:{TEXT};")
        self._flip_chk.stateChanged.connect(lambda: self._refresh_preview())
        row_f.addWidget(self._flip_chk)
        pl.addLayout(row_f)

        root.addWidget(prof_grp)

        # ── Manual calibration ────────────────────────────────────────────────
        man_grp = QGroupBox("Manual Calibration")
        man_grp.setStyleSheet(groupbox())
        ml = QVBoxLayout(man_grp)

        row_m = QHBoxLayout()
        row_m.addWidget(self._lbl("Scan mode"))
        self._scan_combo = QComboBox()
        self._scan_combo.setStyleSheet(combo())
        self._scan_combo.addItem("Coarse (0→320, 12 steps)", "coarse")
        self._scan_combo.addItem("Fine (±40 around center)",  "fine")
        row_m.addWidget(self._scan_combo)
        ml.addLayout(row_m)

        row_fc = QHBoxLayout()
        row_fc.addWidget(self._lbl("Fine center offset"))
        self._fine_center = QSpinBox()
        self._fine_center.setRange(0, 319)
        self._fine_center.setValue(160)
        self._fine_center.setStyleSheet(spinbox())
        row_fc.addWidget(self._fine_center)
        ml.addLayout(row_fc)

        row_nav = QHBoxLayout()
        self._start_btn = QPushButton("▶  Start Session")
        self._start_btn.setStyleSheet(BTN_ACCENT)
        self._start_btn.clicked.connect(self._start_session)

        self._prev_btn = QPushButton("◀  Prev")
        self._prev_btn.setStyleSheet(btn())
        self._prev_btn.setEnabled(False)
        self._prev_btn.clicked.connect(self._prev_offset)

        self._next_btn = QPushButton("Next  ▶")
        self._next_btn.setStyleSheet(btn())
        self._next_btn.setEnabled(False)
        self._next_btn.clicked.connect(self._next_offset)

        for b in (self._start_btn, self._prev_btn, self._next_btn):
            row_nav.addWidget(b)
        ml.addLayout(row_nav)

        row_cur = QHBoxLayout()
        row_cur.addWidget(self._lbl("Offset Y (rows)"))
        self._offset_lbl = QLabel("—")
        self._offset_lbl.setStyleSheet(
            f"color:{ACCENT};font-size:18px;font-weight:bold;")
        self._pos_lbl = QLabel("")
        self._pos_lbl.setStyleSheet(f"color:{DIM};font-size:11px;")
        row_cur.addWidget(self._offset_lbl)
        row_cur.addWidget(self._pos_lbl)
        row_cur.addStretch()
        ml.addLayout(row_cur)

        row_x = QHBoxLayout()
        row_x.addWidget(self._lbl("Offset X (columns)"))
        self._offset_x_spin = QSpinBox()
        self._offset_x_spin.setRange(0, 319)
        self._offset_x_spin.setSingleStep(10)
        self._offset_x_spin.setValue(0)
        self._offset_x_spin.setStyleSheet(spinbox())
        self._offset_x_spin.setToolTip(
            "np.roll(arr, offset_x, axis=1) — horizontal column offset.\n"
            "Try 0, 80, 160, 240 to find the right quadrant.")
        self._offset_x_spin.valueChanged.connect(self._on_offset_x_changed)
        row_x.addWidget(self._offset_x_spin)
        ml.addLayout(row_x)

        row_xq = QHBoxLayout()
        row_xq.addWidget(self._lbl("Quick X →"))
        for v in (0, 80, 160, 240):
            bx = QPushButton(str(v))
            bx.setFixedWidth(52)
            bx.setStyleSheet(btn())
            bx.clicked.connect(lambda _, val=v: self._offset_x_spin.setValue(val))
            row_xq.addWidget(bx)
        row_xq.addStretch()
        ml.addLayout(row_xq)

        row_q = QHBoxLayout()
        row_q.addWidget(self._lbl("Quick Y →"))
        for v in (0, 80, 160, 240):
            bq = QPushButton(str(v))
            bq.setFixedWidth(52)
            bq.setStyleSheet(btn())
            bq.clicked.connect(lambda _, val=v: self._jump(val))
            row_q.addWidget(bq)
        row_q.addStretch()
        ml.addLayout(row_q)

        self._confirm_btn = QPushButton("✓  These offsets are correct — Confirm & Save")
        self._confirm_btn.setStyleSheet(BTN_ACCENT)
        self._confirm_btn.setEnabled(False)
        self._confirm_btn.clicked.connect(self._confirm)
        ml.addWidget(self._confirm_btn)

        self._reset_btn = QPushButton("↺  Reset to Defaults")
        self._reset_btn.setStyleSheet(BTN_DANGER)
        self._reset_btn.setToolTip(
            "Resets Offset Y to 0, Offset X to 0, Rotation to 270°, Flip to off.\n"
            "Use this if calibration gets confusing and you want a clean start.")
        self._reset_btn.clicked.connect(self._reset_calibration)
        ml.addWidget(self._reset_btn)

        root.addWidget(man_grp)

        # ── Auto-cycle ────────────────────────────────────────────────────────
        auto_grp = QGroupBox("Auto-Cycle (watch the physical screen)")
        auto_grp.setStyleSheet(groupbox())
        al = QVBoxLayout(auto_grp)

        row_dw = QHBoxLayout()
        row_dw.addWidget(self._lbl("Dwell per frame (s)"))
        self._dwell_spin = QSpinBox()
        self._dwell_spin.setRange(1, 10)
        self._dwell_spin.setValue(3)
        self._dwell_spin.setStyleSheet(spinbox())
        row_dw.addWidget(self._dwell_spin)
        al.addLayout(row_dw)

        row_auto = QHBoxLayout()
        self._auto_btn = QPushButton("▶  Start Auto-Cycle")
        self._auto_btn.setStyleSheet(BTN_ACCENT)
        self._auto_btn.clicked.connect(self._start_auto)
        self._stop_auto_btn = QPushButton("■  Stop")
        self._stop_auto_btn.setStyleSheet(BTN_DANGER)
        self._stop_auto_btn.setEnabled(False)
        self._stop_auto_btn.clicked.connect(self._stop_auto)
        row_auto.addWidget(self._auto_btn)
        row_auto.addWidget(self._stop_auto_btn)
        al.addLayout(row_auto)

        self._auto_prog = QProgressBar()
        self._auto_prog.setVisible(False)
        self._auto_prog.setStyleSheet(
            f"QProgressBar{{background:{MID_BG};border:1px solid #444;"
            f"border-radius:3px;color:{TEXT};}}"
            f"QProgressBar::chunk{{background:{ACCENT};}}")
        al.addWidget(self._auto_prog)

        root.addWidget(auto_grp)

        # ── Preview ───────────────────────────────────────────────────────────
        prev_grp = QGroupBox("Preview (simulates what the LCD receives)")
        prev_grp.setStyleSheet(groupbox())
        pvl = QVBoxLayout(prev_grp)
        self._preview = QLabel()
        self._preview.setFixedSize(320, 320)
        self._preview.setStyleSheet("border:2px solid #C8A84B;background:#000;")
        self._preview.setAlignment(Qt.AlignCenter)
        pvl.addWidget(self._preview, alignment=Qt.AlignHCenter)
        root.addWidget(prev_grp)

        # Status bar
        self._status = QLabel("")
        self._status.setStyleSheet(f"color:{DIM};font-size:11px;")
        self._status.setWordWrap(True)
        root.addWidget(self._status)

    # ─────────────────────────────────────────────────────────────────────────
    # Helpers
    # ─────────────────────────────────────────────────────────────────────────

    def _lbl(self, t: str) -> QLabel:
        l = QLabel(t)
        l.setStyleSheet(f"color:{DIM};font-size:11px;")
        l.setMinimumWidth(160)
        return l

    def _set_status(self, msg: str):
        self._status.setText(msg)

    @property
    def _rotation(self) -> int:
        return self._rot_combo.currentData()

    @property
    def _flip_y(self) -> bool:
        return self._flip_chk.isChecked()

    @property
    def _profile(self) -> str:
        return self._profile_combo.currentText()

    # ─────────────────────────────────────────────────────────────────────────
    # Preview
    # ─────────────────────────────────────────────────────────────────────────

    def _refresh_preview(self, offset: int = None):
        if offset is None:
            try:
                offset = self._session.current_offset if self._session else 0
            except Exception:
                offset = 0
        offset_x = self._offset_x_spin.value() if hasattr(self, '_offset_x_spin') else 0
        from PIL import ImageOps
        img = build_offset_label_pattern(offset)
        if self._rotation:
            img = img.rotate(self._rotation, expand=False)
        if self._flip_y:
            img = ImageOps.flip(img)
        arr = np.array(img, dtype=np.uint8)
        if offset:
            arr = np.roll(arr, offset, axis=0)
        if offset_x:
            arr = np.roll(arr, offset_x, axis=1)
        self._preview.setPixmap(_to_pixmap(Image.fromarray(arr)))

    # ─────────────────────────────────────────────────────────────────────────
    # Profile changed
    # ─────────────────────────────────────────────────────────────────────────

    def _on_profile_changed(self):
        prof = DISPLAY_PROFILES.get(self._profile, {})
        idx = {0: 0, 90: 1, 180: 2, 270: 3}.get(prof.get("rotation", 270), 3)
        self._rot_combo.setCurrentIndex(idx)
        self._flip_chk.setChecked(bool(prof.get("flip_y", False)))
        self._refresh_preview()

    # ─────────────────────────────────────────────────────────────────────────
    # Manual session
    # ─────────────────────────────────────────────────────────────────────────

    def _start_session(self):
        if self._dc is None:
            self._set_status("No display controller — preview only (no USB).")
        scan   = self._scan_combo.currentData()
        center = self._fine_center.value()

        if self._dc:
            self._dc.set_mode("theme")

        transport = self._dc._transport if self._dc else None
        self._session = ManualCalibrationSession(
            transport=   transport,
            profile=     self._profile,
            scan_mode=   scan,
            fine_center= center,
            offset_x=    self._offset_x_spin.value(),
            show_label=  True,
        )
        offset = self._session.start()
        self._update_offset_display(offset)
        self._prev_btn.setEnabled(True)
        self._next_btn.setEnabled(True)
        self._confirm_btn.setEnabled(True)
        self._set_status(
            f"Session started — {self._session.total} candidates.  "
            "Use Prev / Next to navigate.")

    def _prev_offset(self):
        if not self._session: return
        self._update_offset_display(self._session.step_backward())

    def _next_offset(self):
        if not self._session: return
        self._update_offset_display(self._session.step_forward())

    def _jump(self, val: int):
        if self._session:
            self._update_offset_display(self._session.jump_to_offset(val))
        else:
            self._refresh_preview(val)
            self._offset_lbl.setText(str(val))

    def _update_offset_display(self, offset: int):
        self._offset_lbl.setText(str(offset))
        if self._session:
            self._pos_lbl.setText(
                f"({self._session.position}/{self._session.total})")
        self._refresh_preview(offset)

    def _confirm(self):
        if not self._session:
            return
        result = self._session.confirm(offset_x=self._offset_x_spin.value())
        if self._dc:
            self._dc.set_mode("metrics")
        self._save(result)

    def _reset_calibration(self):
        """Reset all calibration values to safe defaults."""
        self._rot_combo.setCurrentIndex(3)        # 270°
        self._flip_chk.setChecked(False)
        self._offset_x_spin.setValue(0)
        self._offset_lbl.setText("0")
        self._pos_lbl.setText("")
        self._session = None
        self._prev_btn.setEnabled(False)
        self._next_btn.setEnabled(False)
        self._confirm_btn.setEnabled(False)

        # Build a zeroed result and save it
        result = CalibrationResult(
            selected_offset   = 0,
            selected_offset_x = 0,
            rotation          = 270,
            flip_y            = False,
        )
        self._save(result)
        self._refresh_preview(0)
        self._set_status("↺ Reset to defaults — offset=0, rotation=270°, flip=off.")

    def _on_offset_x_changed(self, val: int):
        """When X offset changes, update session and resend immediately."""
        if self._session:
            self._session._offset_x = val
            self._session._send_current()
        self._refresh_preview(self._session.current_offset if self._session else 0)

    # ─────────────────────────────────────────────────────────────────────────
    # Auto-cycle
    # ─────────────────────────────────────────────────────────────────────────

    def _start_auto(self):
        if self._dc is None:
            self._set_status("No display controller attached.")
            return
        self._auto_stop.clear()
        scan   = self._scan_combo.currentData()
        center = self._fine_center.value()
        dwell  = float(self._dwell_spin.value())

        self._auto_prog.setValue(0)
        self._auto_prog.setVisible(True)
        self._auto_btn.setEnabled(False)
        self._stop_auto_btn.setEnabled(True)
        self._set_status("Auto-cycle running — watch the physical screen.")

        # Pause the normal display loop so it doesn't overwrite calibration frames
        self._dc.set_mode("theme")

        def _worker():
            tested = run_auto_cycle_calibration(
                transport=     self._dc._transport,
                profile=       self._profile,
                dwell_seconds= dwell,
                scan_mode=     scan,
                fine_center=   center,
                progress_cb=   lambda off, i, tot:
                                   self._sigs.progress.emit(off, i, tot),
                stop_flag=     self._auto_stop.is_set,
            )
            self._sigs.finished.emit(tested)

        self._auto_thread = threading.Thread(target=_worker, daemon=True)
        self._auto_thread.start()

    def _stop_auto(self):
        self._auto_stop.set()

    def _on_auto_progress(self, offset: int, idx: int, total: int):
        self._auto_prog.setMaximum(total)
        self._auto_prog.setValue(idx)
        self._offset_lbl.setText(str(offset))
        self._refresh_preview(offset)
        self._set_status(f"Auto-cycle: offset={offset}  ({idx}/{total})")

    def _on_auto_finished(self, tested: list):
        self._auto_prog.setVisible(False)
        self._auto_btn.setEnabled(True)
        self._stop_auto_btn.setEnabled(False)
        # Resume normal display loop
        self._dc.set_mode("metrics")
        self._set_status(
            f"Auto-cycle complete.  Tested: {tested}\n"
            "Start a manual session and Quick Jump to confirm the best offset.")

    # ─────────────────────────────────────────────────────────────────────────
    # Save
    # ─────────────────────────────────────────────────────────────────────────

    def _save(self, result: CalibrationResult):
        if self._config:
            save_calibration_result(result, self._config)
            self._set_status(
                f"✓ Saved — offset={result.selected_offset}  "
                f"rotation={result.rotation}°  flip_y={result.flip_y}")
        else:
            self._set_status(
                f"(No config) offset={result.selected_offset}  "
                f"rotation={result.rotation}°  flip_y={result.flip_y}")
        self.calibration_saved.emit(result)

    # ─────────────────────────────────────────────────────────────────────────
    # Startup: pre-load values from config
    # ─────────────────────────────────────────────────────────────────────────

    def _load_saved_values(self):
        if not self._config:
            return
        saved_rotation = self._config.get("rotation", 270)
        saved_flip     = bool(self._config.get("flip_y", False))
        saved_offset   = self._config.get("framebuffer_offset", 0)
        idx = {0: 0, 90: 1, 180: 2, 270: 3}.get(saved_rotation, 3)
        self._rot_combo.setCurrentIndex(idx)
        self._flip_chk.setChecked(saved_flip)
        self._offset_lbl.setText(str(saved_offset))

    # ─────────────────────────────────────────────────────────────────────────
    # Public API
    # ─────────────────────────────────────────────────────────────────────────

    def set_display_controller(self, dc):
        self._dc = dc

    def set_config(self, config):
        self._config = config
        self._load_saved_values()