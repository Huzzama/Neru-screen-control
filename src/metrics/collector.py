"""
Polls CPU and GPU metrics on a background thread.
Access the latest values via the .snapshot property.
"""

import threading
import time
import psutil

from metrics.cpu import CPUMetrics
from metrics.gpu import GPUMetrics

try:
    from PySide6.QtCore import QObject, Signal
    class _Base(QObject):
        updated = Signal()
    _HAS_QT = True
except ImportError:
    class _Base:
        pass
    _HAS_QT = False


class MetricsCollector(_Base):
    def __init__(self, interval: float = 1.0,
                 cpu_unit: str = "celsius",
                 gpu_unit: str = "celsius"):
        if _HAS_QT:
            super().__init__()
        self._interval = interval
        self._cpu_unit = cpu_unit
        self._gpu_unit = gpu_unit
        self._cpu      = CPUMetrics()
        self._gpu      = GPUMetrics()
        self._lock     = threading.Lock()
        self._running  = False
        self._thread   = None
        self._data: dict = self._empty()

    def start(self) -> None:
        if self._running:
            return
        self._running = True
        self._thread  = threading.Thread(target=self._loop, daemon=True)
        self._thread.start()

    def stop(self) -> None:
        self._running = False

    @property
    def snapshot(self) -> dict:
        with self._lock:
            return dict(self._data)

    def _empty(self) -> dict:
        return {
            'cpu_temp': 0, 'gpu_temp': 0,
            'cpu_usage': 0, 'gpu_usage': 0,
            'cpu_frequency': 0, 'gpu_frequency': 0,
            'cpu_power': 0, 'gpu_power': 0,
            'ram_usage': 0,
        }

    def _loop(self) -> None:
        while self._running:
            t0   = time.monotonic()
            data = self._poll()
            with self._lock:
                self._data = data
            if _HAS_QT:
                try:
                    self.updated.emit()
                except Exception:
                    pass
            sleep = self._interval - (time.monotonic() - t0)
            if sleep > 0:
                time.sleep(sleep)

    def _poll(self) -> dict:
        self._cpu.update()
        self._gpu.update()
        data = {}
        data.update(self._cpu.as_dict(unit=self._cpu_unit))
        data.update(self._gpu.as_dict(unit=self._gpu_unit))
        data['ram_usage'] = round(psutil.virtual_memory().percent)
        return data