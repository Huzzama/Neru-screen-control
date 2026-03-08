"""
CPU metrics collector.
Supports: AMD Ryzen (k10temp), Intel (coretemp), generic Linux thermal zones.
All methods return None on failure — callers must handle None gracefully.
"""

import os
import subprocess
import re
import psutil


# ---------------------------------------------------------------------------
# Temperature
# ---------------------------------------------------------------------------

def _get_cpu_temp_psutil() -> float | None:
    """Primary: psutil sensor abstraction. Works for k10temp (Ryzen) and coretemp (Intel)."""
    try:
        if not hasattr(psutil, "sensors_temperatures"):
            return None
        temps = psutil.sensors_temperatures()
        if not temps:
            return None
        for key in ["k10temp", "coretemp", "cpu_thermal", "acpitz"]:
            if key in temps and temps[key]:
                return float(temps[key][0].current)
    except Exception:
        return None


def _get_cpu_temp_thermal_zone() -> float | None:
    """Fallback: read /sys/class/thermal/thermal_zone0/temp."""
    try:
        with open("/sys/class/thermal/thermal_zone0/temp", "r") as f:
            return float(f.read().strip()) / 1000.0
    except Exception:
        return None


def _get_cpu_temp_hwmon() -> float | None:
    """Fallback: scan /sys/class/hwmon for any CPU-like sensor."""
    try:
        base = "/sys/class/hwmon"
        for entry in os.listdir(base):
            name_path = os.path.join(base, entry, "name")
            if not os.path.isfile(name_path):
                continue
            with open(name_path) as f:
                name = f.read().strip()
            if name in ("k10temp", "coretemp", "cpu_thermal"):
                temp_path = os.path.join(base, entry, "temp1_input")
                if os.path.isfile(temp_path):
                    with open(temp_path) as f:
                        return float(f.read().strip()) / 1000.0
    except Exception:
        return None


_TEMP_CANDIDATES = [_get_cpu_temp_psutil, _get_cpu_temp_thermal_zone, _get_cpu_temp_hwmon]


# ---------------------------------------------------------------------------
# Usage
# ---------------------------------------------------------------------------

def _get_cpu_usage() -> float | None:
    try:
        return psutil.cpu_percent(interval=None)
    except Exception:
        return None


_USAGE_CANDIDATES = [_get_cpu_usage]


# ---------------------------------------------------------------------------
# Frequency
# ---------------------------------------------------------------------------

def _get_cpu_freq_psutil() -> int | None:
    try:
        f = psutil.cpu_freq()
        if f and f.current:
            return int(f.current)
    except Exception:
        return None


def _get_cpu_freq_proc() -> int | None:
    try:
        with open("/proc/cpuinfo", "r") as f:
            for line in f:
                if "cpu MHz" in line:
                    parts = line.split(":")
                    if len(parts) > 1:
                        return int(float(parts[1].strip()))
    except Exception:
        return None


_FREQ_CANDIDATES = [_get_cpu_freq_psutil, _get_cpu_freq_proc]


# ---------------------------------------------------------------------------
# Power  (optional — Ryzen RAPL may not be readable without root)
# ---------------------------------------------------------------------------

def _get_cpu_power_rapl() -> int | None:
    """
    Read AMD/Intel RAPL energy counter and compute watts over a short interval.
    Requires read access to /sys/class/powercap — may need chmod or sudo.
    Returns None silently if unavailable.
    """
    import glob
    import time as _time

    try:
        base = "/sys/class/powercap"
        if not os.path.isdir(base):
            return None

        # Match both intel-rapl and amd-rapl (Ryzen uses amd_energy or amd-rapl)
        patterns = [
            base + "/intel-rapl:0/energy_uj",
            base + "/amd-rapl:0/energy_uj",
        ]
        # Also scan subdirectories
        for d in glob.glob(base + "/*/"):
            candidate = os.path.join(d, "energy_uj")
            if os.path.isfile(candidate):
                patterns.insert(0, candidate)

        energy_file = None
        for p in patterns:
            if os.path.isfile(p):
                energy_file = p
                break

        if energy_file is None:
            return None

        with open(energy_file, "r") as f:
            e1 = int(f.read().strip())
        _time.sleep(0.1)
        with open(energy_file, "r") as f:
            e2 = int(f.read().strip())

        delta = e2 - e1
        if delta < 0:
            delta += 2**32  # counter wrap
        return int(abs((delta / 1_000_000) / 0.1))

    except Exception:
        return None


_POWER_CANDIDATES = [_get_cpu_power_rapl]


# ---------------------------------------------------------------------------
# Public collector class
# ---------------------------------------------------------------------------

def _probe(candidates: list) -> tuple:
    """Try each candidate function. Return (function, initial_value) for first that works."""
    for fn in candidates:
        try:
            result = fn()
            if result is not None:
                return fn, result
        except Exception:
            continue
    return None, None


class CPUMetrics:
    """
    Probes available CPU metric sources once at init, then polls them efficiently.
    All metrics default to 0 if unavailable — nothing raises after init.
    """

    def __init__(self):
        self._fn_temp,  self.temp      = _probe(_TEMP_CANDIDATES)
        self._fn_usage, self.usage     = _probe(_USAGE_CANDIDATES)
        self._fn_freq,  self.frequency = _probe(_FREQ_CANDIDATES)
        self._fn_power, self.power     = _probe(_POWER_CANDIDATES)

        # Safe defaults
        self.temp      = int(self.temp      or 0)
        self.usage     = int(self.usage     or 0)
        self.frequency = int(self.frequency or 0)
        self.power     = int(self.power     or 0)

        print(f"[CPUMetrics] temp={'ok' if self._fn_temp else 'unavailable'} "
              f"usage={'ok' if self._fn_usage else 'unavailable'} "
              f"freq={'ok' if self._fn_freq else 'unavailable'} "
              f"power={'ok' if self._fn_power else 'unavailable (optional)'}")

    def update(self) -> None:
        """Refresh all available metrics. Safe to call in a tight loop."""
        if self._fn_temp:
            try:
                v = self._fn_temp()
                if v is not None:
                    self.temp = int(v)
            except Exception:
                pass

        if self._fn_usage:
            try:
                v = self._fn_usage()
                if v is not None:
                    self.usage = int(v)
            except Exception:
                pass

        if self._fn_freq:
            try:
                v = self._fn_freq()
                if v is not None:
                    self.frequency = int(v)
            except Exception:
                pass

        if self._fn_power:
            try:
                v = self._fn_power()
                if v is not None:
                    self.power = int(v)
            except Exception:
                pass

    def as_dict(self, unit: str = "celsius") -> dict:
        temp = self.temp
        if unit == "fahrenheit":
            temp = int(temp * 9 / 5 + 32)
        return {
            "cpu_temp":      temp,
            "cpu_usage":     self.usage,
            "cpu_frequency": self.frequency,
            "cpu_power":     self.power,
        }
