"""
Neru Screen Control systemd user-service manager.

All operations target the *user* scope (systemctl --user).
No root privileges required.

Public API
----------
ServiceManager.status()          → ServiceStatus dataclass
ServiceManager.install()         → bool
ServiceManager.uninstall()       → bool
ServiceManager.start()           → bool
ServiceManager.stop()            → bool
ServiceManager.restart()         → bool
ServiceManager.enable_autostart() → bool
ServiceManager.disable_autostart() → bool
ServiceManager.is_udev_installed() → bool
ServiceManager.install_udev_rule() → bool  (needs pkexec / sudo)
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path


# ── Constants ─────────────────────────────────────────────────────────────────

SERVICE_NAME   = "neru-screen-control.service"
UDEV_RULE_FILE = "99-chizhu-display.rules"
UDEV_RULE_DIR  = Path("/etc/udev/rules.d")
UDEV_RULE_TEXT = (
    'SUBSYSTEM=="usb", ATTRS{idVendor}=="87ad", '
    'ATTRS{idProduct}=="70db", MODE="0666", TAG+="uaccess"\n'
)

SERVICE_TEMPLATE = """\
[Unit]
Description=Neru Screen Control – Background Service
Documentation=https://github.com/your-org/neru-screen-control
After=graphical-session.target

[Service]
Type=simple
ExecStart={exec_start}
Restart=on-failure
RestartSec=5
StandardOutput=journal
StandardError=journal
Environment=PYTHONUNBUFFERED=1

[Install]
WantedBy=default.target
"""


# ── Status dataclass ──────────────────────────────────────────────────────────

@dataclass
class ServiceStatus:
    installed:   bool  = False
    active:      bool  = False   # currently running
    enabled:     bool  = False   # starts at login
    unit_path:   str   = ""
    active_state: str  = "unknown"   # active / inactive / failed / activating
    sub_state:   str   = "unknown"   # running / dead / exited …
    error:       str   = ""


# ── Helpers ───────────────────────────────────────────────────────────────────

def _run(*args, capture=True, input_text=None) -> tuple[int, str, str]:
    """Run a command, return (returncode, stdout, stderr)."""
    try:
        r = subprocess.run(
            list(args),
            capture_output=capture,
            text=True,
            input=input_text,
            timeout=15,
        )
        return r.returncode, r.stdout.strip(), r.stderr.strip()
    except FileNotFoundError as e:
        return 127, "", str(e)
    except subprocess.TimeoutExpired:
        return 1, "", "timeout"
    except Exception as e:
        return 1, "", str(e)


def _systemctl(*args) -> tuple[int, str, str]:
    return _run("systemctl", "--user", *args)


def _user_service_dir() -> Path:
    """~/.config/systemd/user/"""
    base = Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config"))
    d = base / "systemd" / "user"
    d.mkdir(parents=True, exist_ok=True)
    return d


def _unit_path() -> Path:
    return _user_service_dir() / SERVICE_NAME


def _exec_start() -> str:
    """
    Build the ExecStart line.

    Priority:
    1. If we're inside an AppImage → use $APPIMAGE env var
    2. If a 'trcc' script is on PATH → use that
    3. Fall back to 'python <absolute path to main.py>'
    """
    appimage = os.environ.get("APPIMAGE")
    if appimage:
        return f"{appimage} --background"

    neru_bin = shutil.which("trcc")
    if neru_bin:
        return f"{neru_bin} --background"

    # Locate main.py relative to this file (src/service/manager.py → src/../main.py)
    here   = Path(__file__).resolve()
    main   = here.parent.parent.parent / "main.py"
    python = sys.executable
    return f"{python} {main} --background"


# ── Manager ───────────────────────────────────────────────────────────────────

class ServiceManager:
    """Manages the Neru Screen Control systemd user service."""

    # ── Status ────────────────────────────────────────────────────────────────

    @staticmethod
    def status() -> ServiceStatus:
        s = ServiceStatus()
        unit = _unit_path()
        s.unit_path = str(unit)
        s.installed = unit.exists()

        if not s.installed:
            return s

        # Query active + enabled state
        rc, out, err = _systemctl("show", SERVICE_NAME,
                                   "--property=ActiveState,SubState,UnitFileState")
        if rc != 0:
            s.error = err or f"systemctl show exited {rc}"
            return s

        props: dict[str, str] = {}
        for line in out.splitlines():
            if "=" in line:
                k, _, v = line.partition("=")
                props[k.strip()] = v.strip()

        s.active_state = props.get("ActiveState", "unknown")
        s.sub_state    = props.get("SubState",    "unknown")
        s.active       = s.active_state == "active"
        s.enabled      = props.get("UnitFileState", "") in ("enabled", "enabled-runtime")
        return s

    # ── Install / uninstall ───────────────────────────────────────────────────

    @staticmethod
    def install() -> tuple[bool, str]:
        """Write the unit file. Does NOT start or enable."""
        unit = _unit_path()
        content = SERVICE_TEMPLATE.format(exec_start=_exec_start())
        try:
            unit.write_text(content)
        except OSError as e:
            return False, f"Could not write unit file: {e}"

        rc, _, err = _systemctl("daemon-reload")
        if rc != 0:
            return False, f"daemon-reload failed: {err}"

        return True, f"Unit installed: {unit}"

    @staticmethod
    def uninstall() -> tuple[bool, str]:
        """Stop, disable, and remove the unit file."""
        ServiceManager.stop()
        ServiceManager.disable_autostart()
        unit = _unit_path()
        if unit.exists():
            try:
                unit.unlink()
            except OSError as e:
                return False, f"Could not remove unit: {e}"
        _systemctl("daemon-reload")
        return True, "Unit removed."

    # ── Start / stop / restart ────────────────────────────────────────────────

    @staticmethod
    def start() -> tuple[bool, str]:
        if not _unit_path().exists():
            ok, msg = ServiceManager.install()
            if not ok:
                return False, msg
        rc, _, err = _systemctl("start", SERVICE_NAME)
        return (rc == 0), (err if rc != 0 else "Service started.")

    @staticmethod
    def stop() -> tuple[bool, str]:
        rc, _, err = _systemctl("stop", SERVICE_NAME)
        return (rc == 0), (err if rc != 0 else "Service stopped.")

    @staticmethod
    def restart() -> tuple[bool, str]:
        if not _unit_path().exists():
            return ServiceManager.start()
        rc, _, err = _systemctl("restart", SERVICE_NAME)
        return (rc == 0), (err if rc != 0 else "Service restarted.")

    # ── Autostart ─────────────────────────────────────────────────────────────

    @staticmethod
    def enable_autostart() -> tuple[bool, str]:
        if not _unit_path().exists():
            ok, msg = ServiceManager.install()
            if not ok:
                return False, msg
        rc, _, err = _systemctl("enable", SERVICE_NAME)
        return (rc == 0), (err if rc != 0 else "Autostart enabled.")

    @staticmethod
    def disable_autostart() -> tuple[bool, str]:
        rc, _, err = _systemctl("disable", SERVICE_NAME)
        return (rc == 0), (err if rc != 0 else "Autostart disabled.")

    # ── udev rule ─────────────────────────────────────────────────────────────

    @staticmethod
    def is_udev_installed() -> bool:
        return (UDEV_RULE_DIR / UDEV_RULE_FILE).exists()

    @staticmethod
    def install_udev_rule() -> tuple[bool, str]:
        """
        Write the udev rule using pkexec (graphical sudo prompt).
        Falls back to a plain copy if running as root.
        """
        dest = UDEV_RULE_DIR / UDEV_RULE_FILE

        if os.geteuid() == 0:
            # Already root
            try:
                dest.write_text(UDEV_RULE_TEXT)
                _run("udevadm", "control", "--reload-rules")
                _run("udevadm", "trigger")
                return True, "udev rule installed."
            except OSError as e:
                return False, str(e)

        # Write to a temp file then move with pkexec
        import tempfile
        with tempfile.NamedTemporaryFile(mode="w", suffix=".rules",
                                         delete=False) as tf:
            tf.write(UDEV_RULE_TEXT)
            tmp = tf.name

        pkexec = shutil.which("pkexec")
        if not pkexec:
            return False, (
                "pkexec not found. Run manually:\n"
                f"  sudo cp {tmp} {dest}\n"
                "  sudo udevadm control --reload-rules\n"
                "  sudo udevadm trigger"
            )

        rc, _, err = _run(pkexec, "sh", "-c",
                           f"cp {tmp} {dest} && "
                           "udevadm control --reload-rules && "
                           "udevadm trigger")
        try:
            os.unlink(tmp)
        except OSError:
            pass

        if rc == 0:
            return True, "udev rule installed."
        return False, f"pkexec failed (rc={rc}): {err}"

    @staticmethod
    def udev_rule_text() -> str:
        return UDEV_RULE_TEXT.strip()

    @staticmethod
    def udev_manual_instructions() -> str:
        return (
            f"Save the following to {UDEV_RULE_DIR / UDEV_RULE_FILE}:\n\n"
            f"  {UDEV_RULE_TEXT.strip()}\n\n"
            "Then run:\n"
            "  sudo udevadm control --reload-rules\n"
            "  sudo udevadm trigger\n\n"
            "Or click 'Install (requires password)' for automatic installation."
        )