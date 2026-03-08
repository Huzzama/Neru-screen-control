"""
Help / Documentation tab for Neru Screen Control.

Self-contained — no external dependencies beyond PySide6.
All content is plain text / HTML rendered in QTextBrowser widgets
inside a QTabWidget so each section is a click away.
"""

from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel,
    QTabWidget, QTextBrowser, QSizePolicy,
)
from PySide6.QtCore import Qt
from PySide6.QtGui  import QFont

# ── Shared style ──────────────────────────────────────────────────────────────

_DARK   = "#1a1a1a"
_MID    = "#2a2a2a"
_ACCENT = "#00dcdc"
_GOLD   = "#C8A84B"
_DIM    = "#888888"
_TEXT   = "#dddddd"

_BROWSER_CSS = f"""
    QTextBrowser {{
        background: {_MID};
        color: {_TEXT};
        border: none;
        padding: 12px;
        font-size: 13px;
        line-height: 1.6;
    }}
    QScrollBar:vertical {{
        background: {_DARK}; width: 8px; border: none;
    }}
    QScrollBar::handle:vertical {{
        background: #3a3a5a; border-radius: 4px; min-height: 20px;
    }}
"""

_INNER_TAB_CSS = f"""
    QTabWidget::pane {{ border: 1px solid #333; background: {_MID}; }}
    QTabBar::tab {{
        background: #2a2a2a; color: #888;
        padding: 5px 14px; font-size: 12px;
    }}
    QTabBar::tab:selected {{ background: {_DARK}; color: {_GOLD}; }}
"""


def _browser(html: str) -> QTextBrowser:
    b = QTextBrowser()
    b.setOpenExternalLinks(True)
    b.setStyleSheet(_BROWSER_CSS)
    b.setHtml(html)
    b.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Expanding)
    return b


def _h(level: int, text: str) -> str:
    colors = {1: _ACCENT, 2: _GOLD, 3: "#ffffff"}
    c = colors.get(level, _TEXT)
    sizes = {1: "18px", 2: "15px", 3: "13px"}
    sz = sizes.get(level, "13px")
    return (f'<p style="color:{c};font-size:{sz};font-weight:bold;'
            f'margin-top:14px;margin-bottom:4px;">{text}</p>')


def _p(text: str) -> str:
    return f'<p style="color:{_TEXT};margin:4px 0 8px 0;">{text}</p>'


def _note(text: str) -> str:
    return (f'<p style="color:{_DIM};font-size:11px;font-style:italic;'
            f'margin:2px 0 10px 8px;">💡 {text}</p>')


def _step(n: int, title: str, body: str) -> str:
    return (f'<table style="margin:6px 0;" cellpadding="0" cellspacing="0"><tr>'
            f'<td style="color:{_ACCENT};font-size:22px;font-weight:bold;'
            f'padding-right:12px;vertical-align:top;">{n}</td>'
            f'<td><p style="color:#fff;font-weight:bold;margin:0;">{title}</p>'
            f'<p style="color:{_TEXT};margin:2px 0 0 0;">{body}</p></td>'
            f'</tr></table>')


def _row(term: str, desc: str) -> str:
    return (f'<tr>'
            f'<td style="color:{_GOLD};font-weight:bold;padding:4px 14px 4px 0;'
            f'white-space:nowrap;vertical-align:top;">{term}</td>'
            f'<td style="color:{_TEXT};padding:4px 0;">{desc}</td>'
            f'</tr>')


def _table(*rows) -> str:
    return f'<table style="margin:6px 0 12px 0;">{"".join(rows)}</table>'


# ── Section content ───────────────────────────────────────────────────────────

def _html_quickstart() -> str:
    return "".join([
        _h(1, "🚀 Quick Start"),
        _p("Follow these four steps to get your Thermalright LCD display "
           "showing live data within minutes."),

        _step(1, "Connect the display",
              "Plug the USB cable from the cooler into a free USB-A port. "
              "The status indicator in the bottom-left corner of the app "
              "should change from <i>Connecting…</i> to "
              "<b style='color:#00ff80;'>● Connected</b> within a few seconds."),

        _step(2, "Choose or create a theme",
              "Go to the <b>🎨 Themes</b> tab. Select an existing theme from "
              "the drop-down or click <b>+ New</b> to create one. "
              "Add elements — text labels, live metric values, progress bars, "
              "or a background image / GIF / video — by using the "
              "<b>Add</b> buttons on the right panel."),

        _step(3, "Position your elements",
              "Click any element on the canvas to select it. "
              "Drag it to reposition or pull the yellow corner handles to resize. "
              "Use <b>arrow keys</b> to nudge by 1 px, "
              "<b>Shift+Arrow</b> for 10 px. "
              "Lock elements you don't want to move accidentally with "
              "the 🔒 icon in the element list."),

        _step(4, "Send to the display",
              "The preview updates live in the app. The same frame is "
              "sent to the physical display automatically. "
              "If the image looks shifted or rotated, go to the "
              "<b>🎯 Calibrate</b> tab."),

        _h(2, "Keyboard Shortcuts"),
        _table(
            _row("Delete", "Remove selected element"),
            _row("Ctrl + D", "Duplicate selected element"),
            _row("V", "Toggle element visibility"),
            _row("L", "Toggle element lock"),
            _row("Arrow keys", "Nudge element 1 px"),
            _row("Shift + Arrow", "Nudge element 10 px"),
            _row("Ctrl + Scroll", "Zoom canvas in / out"),
        ),
    ])


def _html_themes() -> str:
    return "".join([
        _h(1, "🎨 Themes Tab"),
        _p("The Themes tab is the main layout editor. "
           "Every element lives on a canvas whose size matches the "
           "physical display resolution of the selected model."),

        _h(2, "Element Types"),
        _table(
            _row("Text", "Static label — you type the content. "
                 "Font size is controlled by the bounding-box height: "
                 "drag the bottom handle down to make text bigger."),
            _row("Metric", "Live sensor value (CPU temp, GPU usage, etc.). "
                 "Optionally shows a small label above and the unit suffix. "
                 "Font size also scales with box height."),
            _row("Bar", "Horizontal progress bar bound to any metric. "
                 "Width and height are fully user-controlled."),
            _row("Image / GIF", "Static image or animated GIF. "
                 "The element rectangle defines the display area."),
            _row("Video", "MP4 / AVI / MKV / WebM / MOV. "
                 "Streams directly — no frame limit. "
                 "See <i>Video Limits</i> below."),
        ),

        _h(2, "Canvas Zoom"),
        _p("Use the <b>－ ⊡ ＋</b> buttons below the canvas or "
           "<b>Ctrl + Scroll</b> to zoom. Zoom only affects the editor view — "
           "it has no effect on what is sent to the display."),

        _h(2, "Cross-Model Themes"),
        _p("If you load a theme saved for a different display resolution, "
           "Neru Screen Control will ask whether to <b>Rescale</b> (proportionally move "
           "all elements to the new canvas size) or <b>Keep raw</b> "
           "(keep pixel positions as-is)."),

        _h(2, "Video Limits"),
        _p("Videos are streamed frame-by-frame — no length limit. "
           "The file size limit is <b>200 MB</b>. "
           "For best results, use a clip that is already at or near the "
           "display resolution — Neru Screen Control will resize every frame live, "
           "but smaller source files decode faster."),
        _note("Recommended: encode at the display's native resolution "
              "(e.g. 320×240 for Frozen Warframe) at 30 fps or less."),
    ])


def _html_metrics() -> str:
    return "".join([
        _h(1, "📊 Metrics Tab"),
        _p("The Metrics tab shows live sensor readings and lets you "
           "choose temperature units."),

        _h(2, "Available Metrics"),
        _table(
            _row("CPU Temp", "CPU package temperature in °C or °F "
                 "(controlled by the unit selector)."),
            _row("GPU Temp", "GPU core temperature."),
            _row("CPU Usage", "Total CPU utilisation as a percentage (0–100 %)."),
            _row("GPU Usage", "GPU utilisation as a percentage."),
            _row("CPU Freq", "Current CPU clock frequency in MHz."),
            _row("GPU Freq", "Current GPU clock frequency in MHz."),
            _row("CPU Power", "Estimated CPU power draw in watts. "
                 "May read 0 W if the platform does not expose power sensors."),
            _row("GPU Power", "GPU board power draw in watts."),
            _row("RAM Usage", "System RAM utilisation as a percentage."),
        ),

        _note("Power sensors require platform support. "
              "Intel systems expose CPU power via RAPL; "
              "AMD Ryzen support varies by driver version."),

        _h(2, "Temperature Units"),
        _p("Switch between Celsius and Fahrenheit using the unit dropdowns. "
           "The change takes effect immediately on all metric elements "
           "and the live readout."),
    ])


def _html_calibrate() -> str:
    return "".join([
        _h(1, "🎯 Calibration Tab"),
        _p("Calibration corrects the pixel offset between the framebuffer "
           "sent over USB and the actual LCD panel. Most displays need a "
           "non-zero offset to show the image in the correct position."),

        _h(2, "Understanding Offsets"),
        _table(
            _row("Offset Y", "Shifts the image vertically by rolling the "
                 "framebuffer rows. If the image appears cut off at the "
                 "top or bottom, adjust this."),
            _row("Offset X", "Shifts the image horizontally. "
                 "Common values to try: 0, 80, 160, 240."),
        ),
        _note("Most 320×240 displays need Offset Y = 0 or 80. "
              "Try Quick Jump values first."),

        _h(2, "Rotation"),
        _p("Controls screen orientation. The Frozen Warframe cooler mounts "
           "the display 90° counter-clockwise, so the default is <b>270°</b>. "
           "Change this if your image appears rotated."),

        _h(2, "Flip Vertical"),
        _p("Mirrors the image vertically. Enable this if the image is "
           "upside-down after setting the correct rotation."),

        _h(2, "Manual Calibration Workflow"),
        _step(1, "Start a session",
              "Click <b>▶ Start Session</b>. The display will show a "
              "coloured calibration pattern with corner markers."),
        _step(2, "Navigate offsets",
              "Use <b>◀ Prev</b> / <b>Next ▶</b> or the Quick Jump "
              "buttons (0 / 80 / 160 / 240) to cycle through candidate "
              "offsets. Watch the physical screen."),
        _step(3, "Find the correct position",
              "The correct offset shows: <b style='color:red;'>RED</b> band "
              "at the top, <b style='color:#cccc00;'>YELLOW</b> band at the "
              "bottom. Corner colours (magenta / cyan / orange / purple) "
              "confirm orientation."),
        _step(4, "Confirm and save",
              "Click <b>✓ Confirm &amp; Save</b>. The offset is stored to "
              "config and applied immediately."),

        _h(2, "Reset Calibration"),
        _p("The <b>↺ Reset to Defaults</b> button sets all offsets to 0 "
           "and rotation to 270°. Use this if calibration gets confusing "
           "and you want a clean starting point."),

        _h(2, "Scan Modes"),
        _table(
            _row("Coarse", "Tests 12 evenly-spaced offsets across the full "
                 "range (0 → 320). Use this first."),
            _row("Fine", "Tests offsets within ±40 px of a centre value. "
                 "Use this after Coarse to dial in the exact position."),
        ),

        _h(2, "Auto-Cycle"),
        _p("Automatically steps through all candidate offsets at the "
           "<i>Dwell</i> interval. Useful when you want to watch the "
           "physical screen without touching the keyboard. "
           "After the cycle finishes, note the offset that looked correct "
           "and use Quick Jump to confirm it."),
    ])


def _html_debug() -> str:
    return "".join([
        _h(1, "🐞 Debug Tab"),
        _p("The Debug tab sends test patterns directly to the display "
           "and shows a live log of USB communication."),

        _h(2, "Test Patterns"),
        _table(
            _row("Red / Green / Blue", "Fills the entire panel with a single "
                 "colour. Use these for <b>dead-pixel detection</b> — any "
                 "pixel that does not match the fill colour is defective."),
            _row("Checker", "Alternating black-and-white squares. "
                 "Use this to verify <b>pixel alignment</b> — if the "
                 "squares appear as horizontal or vertical lines instead of "
                 "a grid, the offset is wrong."),
            _row("White", "Full white frame. Use for <b>backlight brightness</b> "
                 "and uniformity checks."),
            _row("Black", "Full black frame. Use for checking "
                 "<b>backlight bleed</b> in a dark room."),
            _row("Gradient", "Smooth colour sweep. Reveals <b>banding</b> "
                 "caused by colour-depth or pixel-format mismatches."),
        ),

        _h(2, "Pixel Format"),
        _p("Controls how pixel colours are encoded before being sent to the "
           "display. The correct value depends on the display hardware."),
        _table(
            _row("rgb565_be", "16-bit colour, big-endian byte order. "
                 "This is the correct format for all known Thermalright "
                 "ChiZhu USB displays."),
            _row("rgb565_le", "16-bit colour, little-endian. "
                 "Try this if colours look inverted or scrambled."),
            _row("rgb888", "24-bit colour. Not used by current hardware "
                 "but available for testing."),
        ),
        _note("If the display shows solid colour garbage, try toggling "
              "between rgb565_be and rgb565_le before adjusting offsets."),

        _h(2, "Log Console"),
        _p("The text area at the bottom shows USB frame timing, error codes, "
           "and driver messages. Copy this output when reporting bugs."),
    ])


def _html_device() -> str:
    return "".join([
        _h(1, "🔌 Device Tab"),
        _p("The Device tab shows the USB connection status and provides "
           "helper commands for Linux udev rules."),

        _h(2, "Connection Status"),
        _table(
            _row("● Connected", "The display was found and the USB endpoint "
                 "is open. Frames are being sent."),
            _row("● Connecting…", "Neru Screen Control is scanning for the display. "
                 "This may take a few seconds after plugging in."),
            _row("● Disconnected", "The display was not found. Check the "
                 "USB cable and try a different port."),
        ),

        _h(2, "USB Details"),
        _p("The Thermalright ChiZhu display uses "
           "<b>VID:PID 87ad:70db</b>. "
           "On Linux, non-root access requires a udev rule. "
           "Click <b>Install udev rule</b> to write the rule and reload udev "
           "automatically."),
        _note("On Windows and macOS no driver installation is required. "
              "The display presents as a generic USB HID / bulk device."),

        _h(2, "Supported Display Models"),
        _table(
            _row("Frozen Warframe", "2.4″ — 320 × 240"),
            _row("Core Matrix", "2.0″ — 320 × 240"),
            _row("Mjolnir / Stream Vision", "3.5″ — 640 × 480"),
            _row("Peerless / Guard / Hyper / Elite / Core / Frozen Vision",
                 "2.1″–3.95″ — 480 × 480"),
            _row("Trofeo Vision", "6.98″ — 1280 × 480"),
            _row("Leviathan / Rainbow / Wonder Vision", "6.67″ — 2400 × 1080"),
        ),
    ])


# ── Tab widget ────────────────────────────────────────────────────────────────

class HelpTab(QWidget):
    """Self-contained Help / Documentation tab."""

    def __init__(self, parent=None):
        super().__init__(parent)
        root = QVBoxLayout(self)
        root.setContentsMargins(0, 0, 0, 0)
        root.setSpacing(0)

        header = QLabel("  ❓ Neru Screen Control – Help")
        header.setStyleSheet(
            f"background:#111;color:{_ACCENT};"
            f"font-size:15px;font-weight:bold;padding:10px 16px;")
        root.addWidget(header)

        inner = QTabWidget()
        inner.setStyleSheet(_INNER_TAB_CSS)
        root.addWidget(inner, 1)

        inner.addTab(_browser(_html_quickstart()), "🚀 Quick Start")
        inner.addTab(_browser(_html_themes()),     "🎨 Themes")
        inner.addTab(_browser(_html_metrics()),    "📊 Metrics")
        inner.addTab(_browser(_html_calibrate()),  "🎯 Calibrate")
        inner.addTab(_browser(_html_debug()),      "🐞 Debug")
        inner.addTab(_browser(_html_device()),     "🔌 Device")