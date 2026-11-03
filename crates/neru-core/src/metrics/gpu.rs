//! GPU metrics.
//!
//! * AMD    — `amdgpu` sysfs (`/sys/class/drm/cardN/device`): hwmon temps,
//!            `gpu_busy_percent`, `power1_average`, `pp_dpm_sclk/mclk`, VRAM.
//! * NVIDIA — NVML through `nvml-wrapper` (loaded at runtime; absent → skipped).
//!
//! The first discrete GPU wins; integrated GPUs are used only if nothing else exists.

use std::fs;
use std::path::{Path, PathBuf};

use super::hwmon;
use super::{GpuMetrics, GpuVendor};

enum Backend {
    Amd(AmdGpu),
    #[cfg(feature = "nvidia")]
    Nvidia(NvidiaGpu),
    None,
}

pub struct GpuSource {
    backend: Backend,
}

impl GpuSource {
    pub fn new() -> Self {
        #[cfg(feature = "nvidia")]
        if let Some(nv) = NvidiaGpu::new() {
            return Self { backend: Backend::Nvidia(nv) };
        }
        if let Some(amd) = AmdGpu::find() {
            return Self { backend: Backend::Amd(amd) };
        }
        Self { backend: Backend::None }
    }

    pub fn describe(&self) -> Vec<String> {
        match &self.backend {
            Backend::Amd(a) => vec![format!("gpu: amdgpu {}", a.device.display())],
            #[cfg(feature = "nvidia")]
            Backend::Nvidia(_) => vec!["gpu: nvidia (NVML)".into()],
            Backend::None => vec!["gpu: none detected".into()],
        }
    }

    pub fn read(&mut self) -> Option<GpuMetrics> {
        match &mut self.backend {
            Backend::Amd(a) => Some(a.read()),
            #[cfg(feature = "nvidia")]
            Backend::Nvidia(n) => Some(n.read()),
            Backend::None => None,
        }
    }
}

impl Default for GpuSource {
    fn default() -> Self {
        Self::new()
    }
}

// ── AMD ─────────────────────────────────────────────────────────────────────

struct AmdGpu {
    device: PathBuf,
    hwmon: Option<PathBuf>,
    temp: Option<PathBuf>,
    hotspot: Option<PathBuf>,
    name: String,
}

impl AmdGpu {
    fn find() -> Option<Self> {
        let rd = fs::read_dir("/sys/class/drm").ok()?;
        let mut cards: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                let n = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                n.starts_with("card") && !n.contains('-') && p.join("device/vendor").exists()
            })
            .collect();
        cards.sort();
        let mut fallback: Option<Self> = None;
        for card in cards {
            let device = card.join("device");
            let vendor = hwmon::read_trimmed(&device.join("vendor")).unwrap_or_default();
            if vendor != "0x1002" {
                continue;
            }
            let gpu = Self::open(device);
            // Discrete cards expose VRAM; APUs usually do not have a meaningful total.
            let discrete = hwmon::read_i64(&gpu.device.join("mem_info_vram_total")).map_or(false, |v| v > 1_500_000_000);
            if discrete {
                return Some(gpu);
            }
            fallback.get_or_insert(gpu);
        }
        fallback
    }

    fn open(device: PathBuf) -> Self {
        let hwmon_dir = fs::read_dir(device.join("hwmon"))
            .ok()
            .and_then(|rd| rd.flatten().map(|e| e.path()).next());
        let temp = hwmon_dir
            .as_deref()
            .and_then(|h| hwmon::find_temp(h, &["edge", "junction"]));
        let hotspot = hwmon_dir.as_deref().and_then(|h| {
            hwmon::inputs(h, "temp")
                .into_iter()
                .find(|(_, l)| l.as_deref().map_or(false, |l| l.eq_ignore_ascii_case("junction")))
                .map(|(p, _)| p)
        });
        let name = hwmon::read_trimmed(&device.join("product_name"))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "AMD Radeon".into());
        Self { device, hwmon: hwmon_dir, temp, hotspot, name }
    }

    fn read(&self) -> GpuMetrics {
        let d = &self.device;
        let power_w = self.hwmon.as_deref().and_then(|h| {
            hwmon::read_i64(&h.join("power1_average"))
                .or_else(|| hwmon::read_i64(&h.join("power1_input")))
                .map(|uw| uw as f32 / 1e6)
        });
        let (fan_pct, fan_rpm) = match self.hwmon.as_deref() {
            Some(h) => {
                let pwm = hwmon::read_i64(&h.join("pwm1")).map(|v| v as f32 / 255.0 * 100.0);
                let rpm = hwmon::read_i64(&h.join("fan1_input")).map(|v| v as f32);
                (pwm, rpm)
            }
            None => (None, None),
        };
        GpuMetrics {
            vendor: Some(GpuVendor::Amd),
            name: self.name.clone(),
            temp_c: self.temp.as_deref().and_then(hwmon::read_temp_c),
            hotspot_c: self.hotspot.as_deref().and_then(hwmon::read_temp_c),
            usage_pct: hwmon::read_i64(&d.join("gpu_busy_percent")).map(|v| v as f32),
            core_mhz: current_dpm_mhz(&d.join("pp_dpm_sclk")),
            mem_mhz: current_dpm_mhz(&d.join("pp_dpm_mclk")),
            power_w,
            vram_used_mb: hwmon::read_i64(&d.join("mem_info_vram_used")).map(|b| b as f32 / 1048576.0),
            vram_total_mb: hwmon::read_i64(&d.join("mem_info_vram_total")).map(|b| b as f32 / 1048576.0),
            fan_pct,
            fan_rpm,
        }
    }
}

/// Parse a `pp_dpm_*` table and return the active level ("0: 500Mhz *").
fn current_dpm_mhz(path: &Path) -> Option<f32> {
    let s = fs::read_to_string(path).ok()?;
    let line = s.lines().find(|l| l.trim_end().ends_with('*'))?;
    let mhz = line
        .split(':')
        .nth(1)?
        .trim()
        .trim_end_matches('*')
        .trim()
        .to_ascii_lowercase();
    mhz.trim_end_matches("mhz").trim().parse().ok()
}

// ── NVIDIA ──────────────────────────────────────────────────────────────────

#[cfg(feature = "nvidia")]
struct NvidiaGpu {
    nvml: nvml_wrapper::Nvml,
    name: String,
}

#[cfg(feature = "nvidia")]
impl NvidiaGpu {
    fn new() -> Option<Self> {
        let nvml = nvml_wrapper::Nvml::init().ok()?;
        let name = nvml.device_by_index(0).ok()?.name().unwrap_or_else(|_| "NVIDIA".into());
        Some(Self { nvml, name })
    }

    fn read(&self) -> GpuMetrics {
        use nvml_wrapper::enum_wrappers::device::{Clock, TemperatureSensor};
        let mut m = GpuMetrics {
            vendor: Some(GpuVendor::Nvidia),
            name: self.name.clone(),
            ..Default::default()
        };
        let Ok(dev) = self.nvml.device_by_index(0) else {
            return m;
        };
        m.temp_c = dev.temperature(TemperatureSensor::Gpu).ok().map(|t| t as f32);
        m.usage_pct = dev.utilization_rates().ok().map(|u| u.gpu as f32);
        m.core_mhz = dev.clock_info(Clock::Graphics).ok().map(|c| c as f32);
        m.mem_mhz = dev.clock_info(Clock::Memory).ok().map(|c| c as f32);
        m.power_w = dev.power_usage().ok().map(|mw| mw as f32 / 1000.0);
        m.fan_pct = dev.fan_speed(0).ok().map(|p| p as f32);
        if let Ok(mem) = dev.memory_info() {
            m.vram_used_mb = Some(mem.used as f32 / 1048576.0);
            m.vram_total_mb = Some(mem.total as f32 / 1048576.0);
        }
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dpm_table() {
        let dir = std::env::temp_dir().join(format!("neru-dpm-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("pp_dpm_sclk");
        fs::write(&p, "0: 500Mhz\n1: 1200Mhz *\n2: 2500Mhz\n").unwrap();
        assert_eq!(current_dpm_mhz(&p), Some(1200.0));
        fs::remove_dir_all(&dir).unwrap();
    }
}
