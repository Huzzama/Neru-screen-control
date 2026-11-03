//! System metrics (CPU / GPU / RAM / network / disks / fans / system) read
//! from sysfs, procfs and NVML.
//!
//! [`Collector`] polls on a background thread and keeps the latest
//! [`Snapshot`] behind an `Arc<RwLock>`; readers never block on I/O. It also
//! records every numeric metric into a [`History`] so themes can draw graphs.

pub mod cpu;
pub mod disk;
pub mod fans;
pub mod gpu;
pub mod hwmon;
pub mod mem;
pub mod net;
pub mod sys;

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CpuMetrics {
    pub name: String,
    pub temp_c: Option<f32>,
    pub usage_pct: Option<f32>,
    /// Usage of the busiest core.
    #[serde(default)]
    pub usage_max_pct: Option<f32>,
    pub freq_mhz: Option<f32>,
    pub power_w: Option<f32>,
    /// Logical CPUs.
    #[serde(default)]
    pub cores: u32,
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
    /// Junction / hotspot temperature where the driver reports it.
    #[serde(default)]
    pub hotspot_c: Option<f32>,
    pub usage_pct: Option<f32>,
    pub core_mhz: Option<f32>,
    pub mem_mhz: Option<f32>,
    pub power_w: Option<f32>,
    pub vram_used_mb: Option<f32>,
    pub vram_total_mb: Option<f32>,
    #[serde(default)]
    pub fan_pct: Option<f32>,
    #[serde(default)]
    pub fan_rpm: Option<f32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct MemMetrics {
    pub used_mb: f32,
    pub total_mb: f32,
    pub pct: f32,
    #[serde(default)]
    pub avail_mb: f32,
    #[serde(default)]
    pub swap_used_mb: f32,
    #[serde(default)]
    pub swap_total_mb: f32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct NetMetrics {
    /// Busiest physical interface during the last interval.
    pub iface: String,
    /// Bytes per second over all physical interfaces.
    pub down_bps: Option<f32>,
    pub up_bps: Option<f32>,
    pub total_down_mb: Option<f32>,
    pub total_up_mb: Option<f32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DiskMetrics {
    /// Bytes per second over all physical block devices.
    pub read_bps: Option<f32>,
    pub write_bps: Option<f32>,
    /// Root filesystem.
    pub used_gb: Option<f32>,
    pub total_gb: Option<f32>,
    pub pct: Option<f32>,
    pub temp_c: Option<f32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SysMetrics {
    pub load1: Option<f32>,
    pub load5: Option<f32>,
    pub load15: Option<f32>,
    pub procs: Option<f32>,
    /// Seconds since boot.
    pub uptime_s: Option<f32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct FanReading {
    /// `<chip>:fanN`
    pub id: String,
    pub label: String,
    pub rpm: Option<f32>,
}

/// One reading of everything.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    /// Seconds since the collector started.
    pub uptime_s: f64,
    pub cpu: CpuMetrics,
    pub gpu: Option<GpuMetrics>,
    pub mem: MemMetrics,
    #[serde(default)]
    pub net: NetMetrics,
    #[serde(default)]
    pub disk: DiskMetrics,
    #[serde(default)]
    pub sys: SysMetrics,
    #[serde(default)]
    pub fans: Vec<FanReading>,
}

/// Keys whose values are temperatures (converted when Fahrenheit is on).
pub fn is_temperature_key(key: &str) -> bool {
    key.ends_with(".temp") || key.ends_with(".hotspot")
}

impl Snapshot {
    /// Flat lookup used by theme bindings: `cpu.temp`, `gpu.usage`, `mem.pct`, …
    pub fn get(&self, key: &str) -> Option<f32> {
        let gpu = self.gpu.as_ref();
        match key {
            "cpu.temp" => self.cpu.temp_c,
            "cpu.usage" => self.cpu.usage_pct,
            "cpu.usage_max" => self.cpu.usage_max_pct,
            "cpu.freq" => self.cpu.freq_mhz,
            "cpu.power" => self.cpu.power_w,
            "cpu.cores" => Some(self.cpu.cores as f32),
            "gpu.temp" => gpu.and_then(|g| g.temp_c),
            "gpu.hotspot" => gpu.and_then(|g| g.hotspot_c),
            "gpu.usage" => gpu.and_then(|g| g.usage_pct),
            "gpu.freq" => gpu.and_then(|g| g.core_mhz),
            "gpu.memfreq" => gpu.and_then(|g| g.mem_mhz),
            "gpu.power" => gpu.and_then(|g| g.power_w),
            "gpu.vram" => gpu.and_then(|g| g.vram_used_mb),
            "gpu.vram_total" => gpu.and_then(|g| g.vram_total_mb),
            "gpu.vram_pct" => gpu.and_then(|g| Some(g.vram_used_mb? / g.vram_total_mb? * 100.0)),
            "gpu.fan" => gpu.and_then(|g| g.fan_pct),
            "gpu.fan_rpm" => gpu.and_then(|g| g.fan_rpm),
            "mem.used" => Some(self.mem.used_mb),
            "mem.total" => Some(self.mem.total_mb),
            "mem.pct" => Some(self.mem.pct),
            "mem.avail" => Some(self.mem.avail_mb),
            "mem.swap_used" => Some(self.mem.swap_used_mb),
            "mem.swap_pct" => (self.mem.swap_total_mb > 0.0).then(|| self.mem.swap_used_mb / self.mem.swap_total_mb * 100.0),
            "net.down" => self.net.down_bps,
            "net.up" => self.net.up_bps,
            "net.down_mbps" => self.net.down_bps.map(|b| b * 8.0 / 1e6),
            "net.up_mbps" => self.net.up_bps.map(|b| b * 8.0 / 1e6),
            "net.total_down" => self.net.total_down_mb,
            "net.total_up" => self.net.total_up_mb,
            "disk.read" => self.disk.read_bps,
            "disk.write" => self.disk.write_bps,
            "disk.used" => self.disk.used_gb,
            "disk.total" => self.disk.total_gb,
            "disk.pct" => self.disk.pct,
            "disk.temp" => self.disk.temp_c,
            "sys.load1" => self.sys.load1,
            "sys.load5" => self.sys.load5,
            "sys.load15" => self.sys.load15,
            "sys.procs" => self.sys.procs,
            "sys.uptime" => self.sys.uptime_s,
            "fan.max" => self.fans.iter().filter_map(|f| f.rpm).fold(None, |m: Option<f32>, v| Some(m.map_or(v, |m| m.max(v)))),
            "fan.cpu" => self.fan_by_label(&["cpu"]),
            "fan.pump" => self.fan_by_label(&["pump", "aio", "water"]),
            _ => {
                let n: usize = key.strip_prefix("fan.")?.parse().ok()?;
                self.fans.get(n.checked_sub(1)?)?.rpm
            }
        }
    }

    fn fan_by_label(&self, words: &[&str]) -> Option<f32> {
        self.fans
            .iter()
            .find(|f| {
                let l = f.label.to_ascii_lowercase();
                words.iter().any(|w| l.contains(w))
            })
            .and_then(|f| f.rpm)
    }

    /// Every fixed key [`Snapshot::get`] understands, with a human label and unit.
    /// Fan channels are dynamic: see [`Snapshot::keys`].
    pub const KEYS: &'static [(&'static str, &'static str, &'static str)] = &[
        ("cpu.temp", "CPU temperature", "°C"),
        ("cpu.usage", "CPU usage", "%"),
        ("cpu.usage_max", "CPU busiest core", "%"),
        ("cpu.freq", "CPU clock", "MHz"),
        ("cpu.power", "CPU power", "W"),
        ("gpu.temp", "GPU temperature", "°C"),
        ("gpu.hotspot", "GPU hotspot", "°C"),
        ("gpu.usage", "GPU usage", "%"),
        ("gpu.freq", "GPU clock", "MHz"),
        ("gpu.memfreq", "GPU memory clock", "MHz"),
        ("gpu.power", "GPU power", "W"),
        ("gpu.fan", "GPU fan", "%"),
        ("gpu.fan_rpm", "GPU fan speed", "RPM"),
        ("gpu.vram", "VRAM used", "MB"),
        ("gpu.vram_total", "VRAM total", "MB"),
        ("gpu.vram_pct", "VRAM used", "%"),
        ("mem.used", "RAM used", "MB"),
        ("mem.avail", "RAM available", "MB"),
        ("mem.total", "RAM total", "MB"),
        ("mem.pct", "RAM used", "%"),
        ("mem.swap_used", "Swap used", "MB"),
        ("mem.swap_pct", "Swap used", "%"),
        ("net.down", "Download", "B/s"),
        ("net.up", "Upload", "B/s"),
        ("net.down_mbps", "Download", "Mbps"),
        ("net.up_mbps", "Upload", "Mbps"),
        ("net.total_down", "Downloaded since boot", "MB"),
        ("net.total_up", "Uploaded since boot", "MB"),
        ("disk.read", "Disk read", "B/s"),
        ("disk.write", "Disk write", "B/s"),
        ("disk.pct", "Root disk used", "%"),
        ("disk.used", "Root disk used", "GB"),
        ("disk.total", "Root disk size", "GB"),
        ("disk.temp", "Drive temperature", "°C"),
        ("fan.cpu", "CPU fan", "RPM"),
        ("fan.pump", "Pump", "RPM"),
        ("fan.max", "Fastest fan", "RPM"),
        ("sys.load1", "Load average (1 min)", ""),
        ("sys.load5", "Load average (5 min)", ""),
        ("sys.load15", "Load average (15 min)", ""),
        ("sys.procs", "Processes", ""),
        ("sys.uptime", "Uptime", "s"),
        ("cpu.cores", "CPU threads", ""),
    ];

    /// [`Snapshot::KEYS`] plus one `fan.N` entry per fan found on this system.
    pub fn keys(&self) -> Vec<(String, String, String)> {
        let mut v: Vec<(String, String, String)> = Self::KEYS.iter().map(|(k, l, u)| (k.to_string(), l.to_string(), u.to_string())).collect();
        for (i, f) in self.fans.iter().enumerate() {
            v.push((format!("fan.{}", i + 1), format!("Fan: {}", f.label), "RPM".into()));
        }
        v
    }
}

/// Recent values of every numeric metric, for graphs.
///
/// Samples are `(seconds since the collector started, value)`; the newest
/// is last. Capacity is bounded (an hour at one sample per second).
#[derive(Debug, Default)]
pub struct History {
    series: HashMap<String, VecDeque<(f64, f32)>>,
    pub cap: usize,
}

impl History {
    pub const DEFAULT_CAP: usize = 3600;

    pub fn new(cap: usize) -> Self {
        Self { series: HashMap::new(), cap: cap.max(2) }
    }

    pub fn push(&mut self, snap: &Snapshot) {
        let cap = self.cap.max(2);
        for (key, _, _) in snap.keys() {
            let Some(v) = snap.get(&key) else { continue };
            let s = self.series.entry(key).or_default();
            s.push_back((snap.uptime_s, v));
            while s.len() > cap {
                s.pop_front();
            }
        }
    }

    /// Samples of `key` from the last `seconds`, as `(age in seconds, value)`
    /// with the oldest first (ages are positive, 0 = now).
    pub fn window(&self, key: &str, seconds: f32, now: f64) -> Vec<(f32, f32)> {
        let Some(s) = self.series.get(key) else { return Vec::new() };
        let seconds = seconds.max(1.0) as f64;
        s.iter()
            .filter(|(t, _)| now - t <= seconds)
            .map(|(t, v)| ((now - t) as f32, *v))
            .collect()
    }

    pub fn len(&self, key: &str) -> usize {
        self.series.get(key).map_or(0, |s| s.len())
    }

    pub fn is_empty(&self) -> bool {
        self.series.values().all(|s| s.is_empty())
    }
}

/// Everything needed to take one reading; holds the per-source state
/// (previous counters) so deltas can be computed.
pub struct Sources {
    cpu: cpu::CpuSource,
    gpu: gpu::GpuSource,
    net: net::NetSource,
    disk: disk::DiskSource,
    fans: fans::FanSource,
    started: Instant,
}

impl Sources {
    pub fn new() -> Self {
        Self {
            cpu: cpu::CpuSource::new(),
            gpu: gpu::GpuSource::new(),
            net: net::NetSource::new(),
            disk: disk::DiskSource::new(),
            fans: fans::FanSource::new(),
            started: Instant::now(),
        }
    }

    pub fn read(&mut self) -> Snapshot {
        Snapshot {
            uptime_s: self.started.elapsed().as_secs_f64(),
            cpu: self.cpu.read(),
            gpu: self.gpu.read(),
            mem: mem::read(),
            net: self.net.read(),
            disk: self.disk.read(),
            sys: sys::read(),
            fans: self.fans.read(),
        }
    }

    /// Human-readable description of where each metric comes from.
    pub fn describe(&self) -> Vec<String> {
        let mut v = self.cpu.describe();
        v.extend(self.gpu.describe());
        v.extend(self.net.describe());
        v.extend(self.disk.describe());
        v.extend(self.fans.describe());
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
    history: Arc<RwLock<History>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Collector {
    pub fn start(interval: Duration) -> Self {
        let latest = Arc::new(RwLock::new(Snapshot::default()));
        let history = Arc::new(RwLock::new(History::new(History::DEFAULT_CAP)));
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (l, h, s) = (latest.clone(), history.clone(), stop.clone());
        let handle = thread::Builder::new()
            .name("neru-metrics".into())
            .spawn(move || {
                let mut sources = Sources::new();
                // First read primes the delta counters.
                let _ = sources.read();
                thread::sleep(Duration::from_millis(250));
                while !s.load(std::sync::atomic::Ordering::Relaxed) {
                    let snap = sources.read();
                    if let Ok(mut hist) = h.write() {
                        hist.push(&snap);
                    }
                    if let Ok(mut w) = l.write() {
                        *w = snap;
                    }
                    thread::sleep(interval);
                }
            })
            .expect("spawn metrics thread");
        Self { latest, history, stop, handle: Some(handle) }
    }

    pub fn latest(&self) -> Snapshot {
        self.latest.read().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn shared(&self) -> Arc<RwLock<Snapshot>> {
        self.latest.clone()
    }

    pub fn history(&self) -> Arc<RwLock<History>> {
        self.history.clone()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fan_keys_are_dynamic() {
        let mut s = Snapshot::default();
        s.fans.push(FanReading { id: "nct:fan1".into(), label: "CPU Fan".into(), rpm: Some(900.0) });
        s.fans.push(FanReading { id: "nct:fan2".into(), label: "Pump".into(), rpm: Some(2500.0) });
        assert_eq!(s.get("fan.1"), Some(900.0));
        assert_eq!(s.get("fan.2"), Some(2500.0));
        assert_eq!(s.get("fan.3"), None);
        assert_eq!(s.get("fan.cpu"), Some(900.0));
        assert_eq!(s.get("fan.pump"), Some(2500.0));
        assert_eq!(s.get("fan.max"), Some(2500.0));
        assert!(s.keys().iter().any(|(k, l, _)| k == "fan.2" && l.contains("Pump")));
    }

    #[test]
    fn history_windows_by_age() {
        let mut h = History::new(10);
        for i in 0..20 {
            let mut s = Snapshot::default();
            s.uptime_s = i as f64;
            s.cpu.usage_pct = Some(i as f32);
            h.push(&s);
        }
        assert_eq!(h.len("cpu.usage"), 10);
        let w = h.window("cpu.usage", 5.0, 19.0);
        assert_eq!(w.len(), 6);
        assert_eq!(w.first().map(|x| x.1), Some(14.0));
        assert_eq!(w.last(), Some(&(0.0, 19.0)));
        assert!(h.window("gpu.temp", 5.0, 19.0).is_empty());
    }

    #[test]
    fn temperature_keys() {
        assert!(is_temperature_key("cpu.temp"));
        assert!(is_temperature_key("gpu.hotspot"));
        assert!(!is_temperature_key("cpu.usage"));
    }
}
