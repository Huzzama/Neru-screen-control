"""
GPU metrics collector.
Auto-detects NVIDIA (pynvml → nvidia-smi) then AMD (pyamdgpuinfo → rocm-smi).
All methods return None on failure.
"""

import subprocess
import re


# ---------------------------------------------------------------------------
# NVIDIA helpers
# ---------------------------------------------------------------------------

def _nvidia_init():
    """Return (nvmlInit_was_called, handle_or_None)."""
    try:
        from pynvml import nvmlInit, nvmlDeviceGetCount, nvmlDeviceGetHandleByIndex
        nvmlInit()
        if nvmlDeviceGetCount() > 0:
            return True, nvmlDeviceGetHandleByIndex(0)
        return True, None
    except Exception:
        return False, None


def _get_nvidia_temp() -> int | None:
    try:
        from pynvml import nvmlInit, nvmlDeviceGetHandleByIndex, \
            nvmlDeviceGetTemperature, NVML_TEMPERATURE_GPU, nvmlShutdown
        nvmlInit()
        h = nvmlDeviceGetHandleByIndex(0)
        t = nvmlDeviceGetTemperature(h, NVML_TEMPERATURE_GPU)
        nvmlShutdown()
        return int(t)
    except Exception:
        pass
    try:
        out = subprocess.check_output(
            ["nvidia-smi", "--query-gpu=temperature.gpu", "--format=csv,noheader"],
            stderr=subprocess.DEVNULL, timeout=3,
        ).decode().strip()
        return int(out.split("\n")[0].strip())
    except Exception:
        return None


def _get_nvidia_usage() -> int | None:
    try:
        from pynvml import nvmlInit, nvmlDeviceGetHandleByIndex, \
            nvmlDeviceGetUtilizationRates, nvmlShutdown
        nvmlInit()
        h = nvmlDeviceGetHandleByIndex(0)
        u = nvmlDeviceGetUtilizationRates(h).gpu
        nvmlShutdown()
        return int(u)
    except Exception:
        pass
    try:
        out = subprocess.check_output(
            ["nvidia-smi", "--query-gpu=utilization.gpu", "--format=csv,noheader,nounits"],
            stderr=subprocess.DEVNULL, timeout=3,
        ).decode().strip()
        return int(out.split()[0])
    except Exception:
        return None


def _get_nvidia_frequency() -> int | None:
    try:
        from pynvml import nvmlInit, nvmlDeviceGetHandleByIndex, \
            nvmlDeviceGetClockInfo, NVML_CLOCK_GRAPHICS, nvmlShutdown
        nvmlInit()
        h = nvmlDeviceGetHandleByIndex(0)
        clk = nvmlDeviceGetClockInfo(h, NVML_CLOCK_GRAPHICS)
        nvmlShutdown()
        return int(clk)
    except Exception:
        pass
    try:
        out = subprocess.check_output(
            ["nvidia-smi", "--query-gpu=clocks.current.graphics",
             "--format=csv,noheader,nounits"],
            stderr=subprocess.DEVNULL, timeout=3,
        ).decode().strip()
        return int(float(re.sub(r"[^0-9.]", "", out.split("\n")[0])))
    except Exception:
        return None


def _get_nvidia_power() -> int | None:
    try:
        from pynvml import nvmlInit, nvmlDeviceGetHandleByIndex, \
            nvmlDeviceGetPowerUsage, nvmlShutdown
        nvmlInit()
        h = nvmlDeviceGetHandleByIndex(0)
        mw = nvmlDeviceGetPowerUsage(h)
        nvmlShutdown()
        return int(mw / 1000)
    except Exception:
        pass
    try:
        out = subprocess.check_output(
            ["nvidia-smi", "--query-gpu=power.draw",
             "--format=csv,noheader,nounits"],
            stderr=subprocess.DEVNULL, timeout=3,
        ).decode().strip()
        return int(float(re.sub(r"[^0-9.]", "", out.split("\n")[0])))
    except Exception:
        return None


# ---------------------------------------------------------------------------
# AMD helpers
# ---------------------------------------------------------------------------

def _amd_gpu_object():
    try:
        import pyamdgpuinfo
        if pyamdgpuinfo.detect_gpus() > 0:
            return pyamdgpuinfo.get_gpu(0)
    except Exception:
        pass
    return None


def _get_amd_temp(gpu_obj) -> int | None:
    try:
        return int(gpu_obj.query_temperature())
    except Exception:
        return None


def _get_amd_usage(gpu_obj) -> int | None:
    try:
        return int(gpu_obj.query_load() * 100)
    except Exception:
        return None


def _get_amd_frequency(gpu_obj) -> int | None:
    for method in ("query_sclk", "query_mclk", "query_clock"):
        try:
            val = getattr(gpu_obj, method)()
            return int(val / 1_000_000)
        except Exception:
            continue
    return None


def _get_amd_power(gpu_obj) -> int | None:
    for method in ("query_power", "query_power_draw", "query_power_watt"):
        try:
            return int(getattr(gpu_obj, method)())
        except Exception:
            continue
    return None


# ---------------------------------------------------------------------------
# Public collector
# ---------------------------------------------------------------------------

class GPUMetrics:
    """
    Auto-detects GPU backend at init (NVIDIA preferred, then AMD).
    Exposes .temp, .usage, .frequency, .power as integers (0 if unavailable).
    """

    BACKEND_NONE   = "none"
    BACKEND_NVIDIA = "nvidia"
    BACKEND_AMD    = "amd"

    def __init__(self):
        self.backend  = self.BACKEND_NONE
        self._amd_gpu = None

        # -- detect NVIDIA --
        try:
            from pynvml import nvmlInit, nvmlDeviceGetCount
            nvmlInit()
            if nvmlDeviceGetCount() > 0:
                self.backend = self.BACKEND_NVIDIA
        except Exception:
            pass

        # -- detect AMD if no NVIDIA --
        if self.backend == self.BACKEND_NONE:
            self._amd_gpu = _amd_gpu_object()
            if self._amd_gpu is not None:
                self.backend = self.BACKEND_AMD

        # -- wire up polling functions --
        if self.backend == self.BACKEND_NVIDIA:
            self._fn_temp  = _get_nvidia_temp
            self._fn_usage = _get_nvidia_usage
            self._fn_freq  = _get_nvidia_frequency
            self._fn_power = _get_nvidia_power
        elif self.backend == self.BACKEND_AMD:
            self._fn_temp  = lambda: _get_amd_temp(self._amd_gpu)
            self._fn_usage = lambda: _get_amd_usage(self._amd_gpu)
            self._fn_freq  = lambda: _get_amd_frequency(self._amd_gpu)
            self._fn_power = lambda: _get_amd_power(self._amd_gpu)
        else:
            self._fn_temp = self._fn_usage = self._fn_freq = self._fn_power = lambda: None

        # prime initial values
        self.temp      = int(self._safe(self._fn_temp)  or 0)
        self.usage     = int(self._safe(self._fn_usage) or 0)
        self.frequency = int(self._safe(self._fn_freq)  or 0)
        self.power     = int(self._safe(self._fn_power) or 0)

        print(f"[GPUMetrics] backend={self.backend} "
              f"temp={self.temp}°C usage={self.usage}%")

    @staticmethod
    def _safe(fn):
        try:
            return fn()
        except Exception:
            return None

    def update(self) -> None:
        self.temp      = int(self._safe(self._fn_temp)  or self.temp)
        self.usage     = int(self._safe(self._fn_usage) or self.usage)
        self.frequency = int(self._safe(self._fn_freq)  or self.frequency)
        self.power     = int(self._safe(self._fn_power) or self.power)

    def as_dict(self, unit: str = "celsius") -> dict:
        temp = self.temp
        if unit == "fahrenheit":
            temp = int(temp * 9 / 5 + 32)
        return {
            "gpu_temp":      temp,
            "gpu_usage":     self.usage,
            "gpu_frequency": self.frequency,
            "gpu_power":     self.power,
        }
