//! CPU metrics from procfs / sysfs.
//!
//! * temperature — hwmon `k10temp` (Tctl/Tdie), `zenpower`, `coretemp` (Package id 0)
//! * usage       — `/proc/stat` deltas
//! * clock       — max of `cpu*/cpufreq/scaling_cur_freq`, else `/proc/cpuinfo`
//! * power       — RAPL (`/sys/class/powercap/intel-rapl:0`, works on AMD Zen too)
//!                 or hwmon energy counters (`zenergy`, `amd_energy`)

use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use super::hwmon;
use super::CpuMetrics;

#[derive(Debug, Clone, Copy)]
struct StatSample {
    busy: u64,
    total: u64,
}

fn read_stat() -> Option<StatSample> {
    let s = fs::read_to_string("/proc/stat").ok()?;
    let line = s.lines().next()?; // "cpu  user nice system idle iowait irq softirq steal ..."
    let nums: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|t| t.parse().ok())
        .collect();
    if nums.len() < 4 {
        return None;
    }
    let idle = nums[3] + nums.get(4).copied().unwrap_or(0);
    let total: u64 = nums.iter().sum();
    Some(StatSample { busy: total - idle, total })
}

enum EnergySource {
    /// powercap `energy_uj` (µJ)
    Rapl(PathBuf),
    /// hwmon `energyN_input` (µJ)
    Hwmon(PathBuf),
}

impl EnergySource {
    fn read_uj(&self) -> Option<u64> {
        match self {
            EnergySource::Rapl(p) | EnergySource::Hwmon(p) => {
                hwmon::read_trimmed(p)?.parse().ok()
            }
        }
    }
}

pub struct CpuSource {
    name: String,
    temp_path: Option<PathBuf>,
    temp_source: String,
    energy: Option<EnergySource>,
    last_stat: Option<StatSample>,
    last_energy: Option<(u64, Instant)>,
    cpufreq_paths: Vec<PathBuf>,
}

impl CpuSource {
    pub fn new() -> Self {
        let name = cpu_name().unwrap_or_else(|| "CPU".into());
        let (temp_path, temp_source) = find_temp();
        let energy = find_energy();
        let cpufreq_paths = fs::read_dir("/sys/devices/system/cpu")
            .map(|rd| {
                rd.flatten()
                    .filter(|e| {
                        let n = e.file_name().to_string_lossy().to_string();
                        n.starts_with("cpu") && n[3..].chars().all(|c| c.is_ascii_digit())
                    })
                    .map(|e| e.path().join("cpufreq/scaling_cur_freq"))
                    .filter(|p| p.exists())
                    .collect()
            })
            .unwrap_or_default();
        Self {
            name,
            temp_path,
            temp_source,
            energy,
            last_stat: None,
            last_energy: None,
            cpufreq_paths,
        }
    }

    pub fn describe(&self) -> Vec<String> {
        vec![
            format!("cpu: {}", self.name),
            format!("cpu.temp: {}", self.temp_source),
            format!(
                "cpu.power: {}",
                match &self.energy {
                    Some(EnergySource::Rapl(p)) => format!("rapl {}", p.display()),
                    Some(EnergySource::Hwmon(p)) => format!("hwmon {}", p.display()),
                    None => "unavailable".into(),
                }
            ),
            format!("cpu.freq: {} cpufreq nodes", self.cpufreq_paths.len()),
        ]
    }

    pub fn read(&mut self) -> CpuMetrics {
        let now = Instant::now();

        let usage_pct = match (self.last_stat, read_stat()) {
            (Some(prev), Some(cur)) => {
                self.last_stat = Some(cur);
                let dt = cur.total.saturating_sub(prev.total);
                if dt == 0 {
                    None
                } else {
                    Some((cur.busy.saturating_sub(prev.busy) as f32 / dt as f32 * 100.0).clamp(0.0, 100.0))
                }
            }
            (None, Some(cur)) => {
                self.last_stat = Some(cur);
                None
            }
            _ => None,
        };

        let power_w = match (&self.energy, self.last_energy) {
            (Some(src), prev) => match src.read_uj() {
                Some(uj) => {
                    let p = prev.and_then(|(puj, pt)| {
                        let dt = now.duration_since(pt).as_secs_f64();
                        if dt <= 0.0 {
                            return None;
                        }
                        // Counters wrap; ignore a negative delta.
                        let d = uj.checked_sub(puj)?;
                        Some((d as f64 / 1e6 / dt) as f32)
                    });
                    self.last_energy = Some((uj, now));
                    p
                }
                None => None,
            },
            _ => None,
        };

        let temp_c = self.temp_path.as_deref().and_then(hwmon::read_temp_c);

        CpuMetrics {
            name: self.name.clone(),
            temp_c,
            usage_pct,
            freq_mhz: self.freq_mhz(),
            power_w,
        }
    }

    fn freq_mhz(&self) -> Option<f32> {
        let mut max_khz: Option<i64> = None;
        for p in &self.cpufreq_paths {
            if let Some(v) = hwmon::read_i64(p) {
                max_khz = Some(max_khz.map_or(v, |m| m.max(v)));
            }
        }
        if let Some(khz) = max_khz {
            return Some(khz as f32 / 1000.0);
        }
        // Fallback: /proc/cpuinfo "cpu MHz"
        let s = fs::read_to_string("/proc/cpuinfo").ok()?;
        s.lines()
            .filter(|l| l.starts_with("cpu MHz"))
            .filter_map(|l| l.split(':').nth(1)?.trim().parse::<f32>().ok())
            .fold(None, |m: Option<f32>, v| Some(m.map_or(v, |m| m.max(v))))
    }
}

impl Default for CpuSource {
    fn default() -> Self {
        Self::new()
    }
}

fn cpu_name() -> Option<String> {
    let s = fs::read_to_string("/proc/cpuinfo").ok()?;
    let line = s.lines().find(|l| l.starts_with("model name"))?;
    Some(line.split(':').nth(1)?.trim().to_string())
}

fn find_temp() -> (Option<PathBuf>, String) {
    // (driver name, preferred labels)
    const CANDIDATES: &[(&str, &[&str])] = &[
        ("k10temp", &["Tctl", "Tdie"]),
        ("zenpower", &["Tdie", "Tctl"]),
        ("coretemp", &["Package id 0"]),
        ("cpu_thermal", &[]),
        ("acpitz", &[]),
    ];
    let chips = hwmon::chips();
    for (driver, labels) in CANDIDATES {
        for chip in chips.iter().filter(|c| c.name == *driver) {
            if let Some(p) = hwmon::find_temp(&chip.path, labels) {
                let label = hwmon::read_trimmed(
                    &p.with_file_name(
                        p.file_name()
                            .unwrap()
                            .to_string_lossy()
                            .replace("_input", "_label"),
                    ),
                )
                .unwrap_or_default();
                return (Some(p), format!("hwmon {driver} {label}").trim().to_string());
            }
        }
    }
    (None, "unavailable".into())
}

fn find_energy() -> Option<EnergySource> {
    // RAPL package domain (intel_rapl works on AMD Zen as well).
    if let Ok(rd) = fs::read_dir("/sys/class/powercap") {
        let mut doms: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                let n = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                // top-level domains only: "intel-rapl:0", not "intel-rapl:0:0"
                n.starts_with("intel-rapl") && n.matches(':').count() == 1
            })
            .collect();
        doms.sort();
        for d in doms {
            let name = hwmon::read_trimmed(&d.join("name")).unwrap_or_default();
            let e = d.join("energy_uj");
            if name.starts_with("package") && fs::read_to_string(&e).is_ok() {
                return Some(EnergySource::Rapl(e));
            }
        }
    }
    // hwmon energy counters (zenergy / amd_energy)
    for chip in hwmon::chips() {
        if chip.name == "zenergy" || chip.name == "amd_energy" {
            // Prefer a socket/package counter over per-core ones.
            let all = hwmon::inputs(&chip.path, "energy");
            let pick = all
                .iter()
                .find(|(_, l)| l.as_deref().map_or(false, |l| l.to_ascii_lowercase().contains("socket") || l.to_ascii_lowercase().contains("package")))
                .or_else(|| all.last())
                .map(|(p, _)| p.clone());
            if let Some(p) = pick {
                return Some(EnergySource::Hwmon(p));
            }
        }
    }
    None
}
