//! System metrics (CPU / GPU / RAM) read from sysfs, procfs and NVML.
//!
//! [`Collector`] polls on a background thread and keeps the latest
//! [`Snapshot`] behind an `Arc<RwLock>`; readers never block on I/O.

pub mod cpu;
pub mod gpu;
pub mod hwmon;
pub mod mem;

use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CpuMetrics {
    pub name: String,
    pub temp_c: Option<f32>,
    pub usage_pct: Option<f32>,
    pub freq_mhz: Option<f32>,
    pub power_w: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GpuVendor {
    Amd,
    Nvidia,
    Intel,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct GpuMetrics {
    pub vendor: Option<GpuVendor>,
    pub name: String,
    pub temp_c: Option<f32>,
    pub usage_pct: Option<f32>,
    pub core_mhz: Option<f32>,
    pub mem_mhz: Option<f32>,
    pub power_w: Option<f32>,
    pub vram_used_mb: Option<f32>,
    pub vram_total_mb: Option<f32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct MemMetrics {
    pub used_mb: f32,
    pub total_mb: f32,
    pub pct: f32,
}

/// One reading of everything.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    /// Seconds since the collector started.
    pub uptime_s: f64,
    pub cpu: CpuMetrics,
    pub gpu: Option<GpuMetrics>,
    pub mem: MemMetrics,
}

impl Snapshot {
    /// Flat lookup used by theme bindings: `cpu.temp`, `gpu.usage`, `mem.pct`, …
    pub fn get(&self, key: &str) -> Option<f32> {
        let gpu = self.gpu.as_ref();
        match key {
            "cpu.temp" => self.cpu.temp_c,
            "cpu.usage" => self.cpu.usage_pct,
            "cpu.freq" => self.cpu.freq_mhz,
            "cpu.power" => self.cpu.power_w,
            "gpu.temp" => gpu.and_then(|g| g.temp_c),
            "gpu.usage" => gpu.and_then(|g| g.usage_pct),
            "gpu.freq" => gpu.and_then(|g| g.core_mhz),
            "gpu.memfreq" => gpu.and_then(|g| g.mem_mhz),
            "gpu.power" => gpu.and_then(|g| g.power_w),
            "gpu.vram" => gpu.and_then(|g| g.vram_used_mb),
            "gpu.vram_total" => gpu.and_then(|g| g.vram_total_mb),
            "gpu.vram_pct" => gpu.and_then(|g| Some(g.vram_used_mb? / g.vram_total_mb? * 100.0)),
            "mem.used" => Some(self.mem.used_mb),
            "mem.total" => Some(self.mem.total_mb),
            "mem.pct" => Some(self.mem.pct),
            _ => None,
        }
    }

    /// Every key [`Snapshot::get`] understands, with a human label and unit.
    pub const KEYS: &'static [(&'static str, &'static str, &'static str)] = &[
        ("cpu.temp", "CPU temperature", "°C"),
        ("cpu.usage", "CPU usage", "%"),
        ("cpu.freq", "CPU clock", "MHz"),
        ("cpu.power", "CPU power", "W"),
        ("gpu.temp", "GPU temperature", "°C"),
        ("gpu.usage", "GPU usage", "%"),
        ("gpu.freq", "GPU clock", "MHz"),
        ("gpu.memfreq", "GPU memory clock", "MHz"),
        ("gpu.power", "GPU power", "W"),
        ("gpu.vram", "VRAM used", "MB"),
        ("gpu.vram_total", "VRAM total", "MB"),
        ("gpu.vram_pct", "VRAM used", "%"),
        ("mem.used", "RAM used", "MB"),
        ("mem.total", "RAM total", "MB"),
        ("mem.pct", "RAM used", "%"),
    ];
}

/// Everything needed to take one reading; holds the per-source state
/// (previous counters) so deltas can be computed.
pub struct Sources {
    cpu: cpu::CpuSource,
    gpu: gpu::GpuSource,
    started: Instant,
}

impl Sources {
    pub fn new() -> Self {
        Self {
            cpu: cpu::CpuSource::new(),
            gpu: gpu::GpuSource::new(),
            started: Instant::now(),
        }
    }

    pub fn read(&mut self) -> Snapshot {
        Snapshot {
            uptime_s: self.started.elapsed().as_secs_f64(),
            cpu: self.cpu.read(),
            gpu: self.gpu.read(),
            mem: mem::read(),
        }
    }

    /// Human-readable description of where each metric comes from.
    pub fn describe(&self) -> Vec<String> {
        let mut v = self.cpu.describe();
        v.extend(self.gpu.describe());
        v
    }
}

impl Default for Sources {
    fn default() -> Self {
        Self::new()
    }
}

/// Background poller.
pub struct Collector {
    latest: Arc<RwLock<Snapshot>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Collector {
    pub fn start(interval: Duration) -> Self {
        let latest = Arc::new(RwLock::new(Snapshot::default()));
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (l, s) = (latest.clone(), stop.clone());
        let handle = thread::Builder::new()
            .name("neru-metrics".into())
            .spawn(move || {
                let mut sources = Sources::new();
                // First read primes the delta counters.
                let _ = sources.read();
                thread::sleep(Duration::from_millis(250));
                while !s.load(std::sync::atomic::Ordering::Relaxed) {
                    let snap = sources.read();
                    if let Ok(mut w) = l.write() {
                        *w = snap;
                    }
                    thread::sleep(interval);
                }
            })
            .expect("spawn metrics thread");
        Self { latest, stop, handle: Some(handle) }
    }

    pub fn latest(&self) -> Snapshot {
        self.latest.read().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn shared(&self) -> Arc<RwLock<Snapshot>> {
        self.latest.clone()
    }
}

impl Drop for Collector {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}
