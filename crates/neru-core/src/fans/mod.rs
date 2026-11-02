//! Fan control through Linux hwmon (`/sys/class/hwmon`).
//!
//! * discovery of PWM channels (`pwmN`, `pwmN_enable`, `fanN_input`) and
//!   temperature sources on every chip (nct6775/nct6798 Super I/O, amdgpu,
//!   k10temp, nvme, …)
//! * fan curves (temperature → duty %) with hysteresis-free linear
//!   interpolation and slew limiting
//! * a controller that applies a [`FanConfig`] — needs write access to
//!   sysfs, so it runs inside the `neru-fand` root helper; the app talks to
//!   it over a Unix socket ([`daemon`])

pub mod daemon;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::metrics::hwmon;
use crate::{Error, Result};

pub const DEFAULT_HWMON_ROOT: &str = "/sys/class/hwmon";

/// A temperature we can drive fans from: `k10temp:Tctl`, `amdgpu:edge`, `nvme:Composite`…
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sensor {
    /// `chip:label` (or `chip:tempN` when unlabelled).
    pub id: String,
    pub chip: String,
    pub label: String,
    pub temp_c: Option<f32>,
    #[serde(skip)]
    pub path: PathBuf,
}

/// One PWM output and the tachometer paired with it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Channel {
    /// `chip:N` — stable across reboots as long as the chip set is the same.
    pub id: String,
    pub chip: String,
    pub index: u32,
    pub label: String,
    /// 0–255 as reported by the chip.
    pub pwm: Option<u32>,
    pub rpm: Option<u32>,
    /// `pwmN_enable`: 0 = off/full, 1 = manual, 2+ = automatic (chip/BIOS curve).
    pub enable: Option<u32>,
    pub writable: bool,
    #[serde(skip)]
    pub pwm_path: PathBuf,
    #[serde(skip)]
    pub enable_path: PathBuf,
    #[serde(skip)]
    pub rpm_path: Option<PathBuf>,
}

impl Channel {
    pub fn duty_pct(&self) -> Option<f32> {
        self.pwm.map(|p| p as f32 / 255.0 * 100.0)
    }
}

/// Everything found under hwmon.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Inventory {
    pub sensors: Vec<Sensor>,
    pub channels: Vec<Channel>,
}

fn chip_key(chips: &[hwmon::Chip], chip: &hwmon::Chip) -> String {
    // Disambiguate repeated driver names (two nvme drives, two GPUs) by order.
    let same: Vec<&hwmon::Chip> = chips.iter().filter(|c| c.name == chip.name).collect();
    if same.len() <= 1 {
        chip.name.clone()
    } else {
        let n = same.iter().position(|c| c.path == chip.path).unwrap_or(0);
        format!("{}#{}", chip.name, n + 1)
    }
}

fn read_u32(p: &Path) -> Option<u32> {
    hwmon::read_i64(p).and_then(|v| u32::try_from(v).ok())
}

fn is_writable(p: &Path) -> bool {
    fs::OpenOptions::new().write(true).open(p).is_ok()
}

/// Scan `root` (normally `/sys/class/hwmon`).
pub fn discover(root: &Path) -> Inventory {
    let chips = chips_at(root);
    let mut inv = Inventory::default();
    for chip in &chips {
        let key = chip_key(&chips, chip);
        for (path, label) in hwmon::inputs(&chip.path, "temp") {
            let stem = path.file_name().unwrap().to_string_lossy().trim_end_matches("_input").to_string();
            let label = label.unwrap_or_else(|| stem.clone());
            inv.sensors.push(Sensor {
                id: format!("{key}:{label}"),
                chip: key.clone(),
                label,
                temp_c: hwmon::read_temp_c(&path),
                path,
            });
        }
        let Ok(rd) = fs::read_dir(&chip.path) else { continue };
        let mut pwms: Vec<(u32, PathBuf)> = rd
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                let idx: u32 = name.strip_prefix("pwm")?.parse().ok()?;
                Some((idx, e.path()))
            })
            .collect();
        pwms.sort();
        for (idx, pwm_path) in pwms {
            let enable_path = chip.path.join(format!("pwm{idx}_enable"));
            let rpm_path = chip.path.join(format!("fan{idx}_input"));
            let label = hwmon::read_trimmed(&chip.path.join(format!("fan{idx}_label")))
                .or_else(|| hwmon::read_trimmed(&chip.path.join(format!("pwm{idx}_label"))))
                .unwrap_or_else(|| format!("Fan {idx}"));
            inv.channels.push(Channel {
                id: format!("{key}:{idx}"),
                chip: key.clone(),
                index: idx,
                label,
                pwm: read_u32(&pwm_path),
                rpm: read_u32(&rpm_path),
                enable: read_u32(&enable_path),
                writable: is_writable(&pwm_path),
                rpm_path: rpm_path.exists().then_some(rpm_path),
                pwm_path,
                enable_path,
            });
        }
    }
    inv
}

fn chips_at(root: &Path) -> Vec<hwmon::Chip> {
    let Ok(rd) = fs::read_dir(root) else { return Vec::new() };
    let mut v: Vec<hwmon::Chip> = rd
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let name = hwmon::read_trimmed(&path.join("name"))?;
            Some(hwmon::Chip { path, name })
        })
        .collect();
    v.sort_by(|a, b| a.path.cmp(&b.path));
    v
}

// ── configuration ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ChannelMode {
    /// Leave it to the BIOS / chip (restores the original `pwmN_enable`).
    Auto,
    /// Fixed duty, percent.
    Fixed { pct: f32 },
    /// Temperature → duty curve.
    Curve {
        /// `Sensor::id`.
        sensor: String,
        /// Sorted (°C, %) points; linear interpolation between, clamped outside.
        points: Vec<[f32; 2]>,
        /// Never go below this duty (keeps fans from stalling).
        #[serde(default = "default_min_pct")]
        min_pct: f32,
        /// Max % the duty may change per second (0 = instant).
        #[serde(default = "default_slew")]
        slew_pct_per_s: f32,
    },
}

fn default_min_pct() -> f32 {
    20.0
}
fn default_slew() -> f32 {
    10.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelConfig {
    pub id: String,
    pub name: Option<String>,
    pub mode: ChannelMode,
}

impl Default for ChannelConfig {
    fn default() -> Self {
        Self { id: String::new(), name: None, mode: ChannelMode::Auto }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FanConfig {
    pub channels: Vec<ChannelConfig>,
}

impl FanConfig {
    pub fn load(path: &Path) -> Result<Self> {
        match fs::read_to_string(path) {
            Ok(s) => serde_json::from_str(&s).map_err(|e| Error::Config(format!("{}: {e}", path.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(d) = path.parent() {
            fs::create_dir_all(d)?;
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        fs::rename(tmp, path)?;
        Ok(())
    }
}

/// Duty for `temp` on a curve (points sorted by temperature).
pub fn curve_eval(points: &[[f32; 2]], temp: f32) -> f32 {
    let mut pts: Vec<[f32; 2]> = points.to_vec();
    pts.sort_by(|a, b| a[0].partial_cmp(&b[0]).unwrap_or(std::cmp::Ordering::Equal));
    match pts.len() {
        0 => 100.0,
        1 => pts[0][1],
        _ => {
            if temp <= pts[0][0] {
                return pts[0][1];
            }
            for w in pts.windows(2) {
                let (a, b) = (w[0], w[1]);
                if temp <= b[0] {
                    let t = if b[0] > a[0] { (temp - a[0]) / (b[0] - a[0]) } else { 1.0 };
                    return a[1] + (b[1] - a[1]) * t;
                }
            }
            pts[pts.len() - 1][1]
        }
    }
    .clamp(0.0, 100.0)
}

/// Ready-made curves (°C → %).
pub fn presets() -> Vec<(&'static str, Vec<[f32; 2]>)> {
    vec![
        ("Silent", vec![[30.0, 20.0], [50.0, 30.0], [65.0, 45.0], [75.0, 65.0], [85.0, 100.0]]),
        ("Balanced", vec![[30.0, 25.0], [50.0, 40.0], [65.0, 60.0], [75.0, 80.0], [85.0, 100.0]]),
        ("Performance", vec![[30.0, 40.0], [50.0, 60.0], [65.0, 80.0], [75.0, 100.0]]),
    ]
}

// ── controller (runs as root) ───────────────────────────────────────────────

/// Current state of one channel as applied by the controller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelState {
    pub id: String,
    pub target_pct: Option<f32>,
    pub sensor_temp: Option<f32>,
    pub error: Option<String>,
}

pub struct Controller {
    root: PathBuf,
    config: FanConfig,
    inventory: Inventory,
    /// Original `pwmN_enable` per channel id, restored on release.
    saved_enable: HashMap<String, u32>,
    /// Last duty written (for slew limiting).
    last_duty: HashMap<String, (f32, Instant)>,
    pub states: Vec<ChannelState>,
    last_scan: Instant,
}

impl Controller {
    pub fn new(root: PathBuf, config: FanConfig) -> Self {
        let inventory = discover(&root);
        Self {
            root,
            config,
            inventory,
            saved_enable: HashMap::new(),
            last_duty: HashMap::new(),
            states: Vec::new(),
            last_scan: Instant::now(),
        }
    }

    pub fn inventory(&self) -> &Inventory {
        &self.inventory
    }

    pub fn config(&self) -> &FanConfig {
        &self.config
    }

    /// Replace the configuration; channels that left manual control get the BIOS back.
    pub fn set_config(&mut self, config: FanConfig) -> Result<()> {
        let released: Vec<String> = self
            .config
            .channels
            .iter()
            .filter(|old| !matches!(old.mode, ChannelMode::Auto))
            .filter(|old| {
                config
                    .channels
                    .iter()
                    .find(|n| n.id == old.id)
                    .map_or(true, |n| matches!(n.mode, ChannelMode::Auto))
            })
            .map(|c| c.id.clone())
            .collect();
        for id in released {
            self.restore(&id);
        }
        self.config = config;
        Ok(())
    }

    fn channel(&self, id: &str) -> Option<&Channel> {
        self.inventory.channels.iter().find(|c| c.id == id)
    }

    fn write(path: &Path, value: u32) -> Result<()> {
        fs::write(path, value.to_string()).map_err(|e| Error::Io(e))
    }

    /// Take manual control of a channel (remembering what the BIOS had set).
    fn take(&mut self, id: &str) -> Result<()> {
        let Some(ch) = self.channel(id).cloned() else {
            return Err(Error::DeviceNotFound(id.to_string()));
        };
        if !self.saved_enable.contains_key(id) {
            let current = read_u32(&ch.enable_path).unwrap_or(2);
            self.saved_enable.insert(id.to_string(), current);
            if current != 1 {
                Self::write(&ch.enable_path, 1)?;
                info!(channel = id, was = current, "manual fan control enabled");
            }
        }
        Ok(())
    }

    /// Give a channel back to the BIOS.
    pub fn restore(&mut self, id: &str) {
        if let (Some(ch), Some(orig)) = (self.channel(id).cloned(), self.saved_enable.remove(id)) {
            let target = if orig == 1 { 2 } else { orig };
            if let Err(e) = Self::write(&ch.enable_path, target) {
                warn!(channel = id, error = %e, "could not restore automatic fan control");
            } else {
                info!(channel = id, mode = target, "automatic fan control restored");
            }
            self.last_duty.remove(id);
        }
    }

    /// Give every channel back to the BIOS (shutdown path).
    pub fn release_all(&mut self) {
        let ids: Vec<String> = self.saved_enable.keys().cloned().collect();
        for id in ids {
            self.restore(&id);
        }
    }

    fn sensor_temp(&self, sensor_id: &str) -> Option<f32> {
        self.inventory.sensors.iter().find(|s| s.id == sensor_id).and_then(|s| hwmon::read_temp_c(&s.path))
    }

    /// One control step (call about once a second).
    pub fn tick(&mut self) {
        if self.last_scan.elapsed().as_secs() >= 30 {
            // Pick up hotplugged chips without losing saved enable values.
            self.inventory = discover(&self.root);
            self.last_scan = Instant::now();
        }
        let now = Instant::now();
        let mut states = Vec::new();
        let cfg = self.config.clone();
        for cc in &cfg.channels {
            let mut st = ChannelState { id: cc.id.clone(), target_pct: None, sensor_temp: None, error: None };
            let target = match &cc.mode {
                ChannelMode::Auto => None,
                ChannelMode::Fixed { pct } => Some(pct.clamp(0.0, 100.0)),
                ChannelMode::Curve { sensor, points, min_pct, slew_pct_per_s } => {
                    let temp = self.sensor_temp(sensor);
                    st.sensor_temp = temp;
                    let want = match temp {
                        Some(t) => curve_eval(points, t).max(*min_pct),
                        None => {
                            st.error = Some(format!("sensor {sensor} unavailable — running at 100%"));
                            100.0
                        }
                    };
                    // Slew limit.
                    let limited = match self.last_duty.get(&cc.id) {
                        Some((last, at)) if *slew_pct_per_s > 0.0 => {
                            let max_step = slew_pct_per_s * now.duration_since(*at).as_secs_f32().max(0.05);
                            last + (want - last).clamp(-max_step, max_step)
                        }
                        _ => want,
                    };
                    Some(limited)
                }
            };
            if let Some(pct) = target {
                st.target_pct = Some(pct);
                let res = self.take(&cc.id).and_then(|_| {
                    let ch = self.channel(&cc.id).cloned().ok_or_else(|| Error::DeviceNotFound(cc.id.clone()))?;
                    Self::write(&ch.pwm_path, (pct / 100.0 * 255.0).round().clamp(0.0, 255.0) as u32)
                });
                match res {
                    Ok(()) => {
                        self.last_duty.insert(cc.id.clone(), (pct, now));
                    }
                    Err(e) => st.error = Some(e.to_string()),
                }
            }
            states.push(st);
        }
        self.states = states;
    }

    /// Fresh readings for the UI (RPM, PWM, temps).
    pub fn snapshot(&self) -> Inventory {
        discover(&self.root)
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        self.release_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_hwmon() -> PathBuf {
        let root = std::env::temp_dir().join(format!("neru-hwmon-{}-{}", std::process::id(), rand_suffix()));
        let chip = root.join("hwmon3");
        fs::create_dir_all(&chip).unwrap();
        fs::write(chip.join("name"), "nct6798\n").unwrap();
        fs::write(chip.join("pwm1"), "128\n").unwrap();
        fs::write(chip.join("pwm1_enable"), "5\n").unwrap();
        fs::write(chip.join("fan1_input"), "1200\n").unwrap();
        fs::write(chip.join("fan1_label"), "CPU Fan\n").unwrap();
        fs::write(chip.join("pwm2"), "255\n").unwrap();
        fs::write(chip.join("pwm2_enable"), "5\n").unwrap();
        fs::write(chip.join("fan2_input"), "900\n").unwrap();
        fs::write(chip.join("temp1_input"), "45000\n").unwrap();
        fs::write(chip.join("temp1_label"), "SYSTIN\n").unwrap();
        let cpu = root.join("hwmon1");
        fs::create_dir_all(&cpu).unwrap();
        fs::write(cpu.join("name"), "k10temp\n").unwrap();
        fs::write(cpu.join("temp1_input"), "61500\n").unwrap();
        fs::write(cpu.join("temp1_label"), "Tctl\n").unwrap();
        root
    }

    fn rand_suffix() -> u64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().subsec_nanos() as u64
    }

    #[test]
    fn discovers_channels_and_sensors() {
        let root = fake_hwmon();
        let inv = discover(&root);
        assert_eq!(inv.channels.len(), 2);
        assert_eq!(inv.channels[0].id, "nct6798:1");
        assert_eq!(inv.channels[0].label, "CPU Fan");
        assert_eq!(inv.channels[0].rpm, Some(1200));
        assert_eq!(inv.channels[0].enable, Some(5));
        assert!((inv.channels[0].duty_pct().unwrap() - 50.2).abs() < 0.1);
        assert!(inv.sensors.iter().any(|s| s.id == "k10temp:Tctl" && s.temp_c == Some(61.5)));
        assert!(inv.sensors.iter().any(|s| s.id == "nct6798:SYSTIN"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn curve_interpolates_and_clamps() {
        let pts = vec![[30.0, 20.0], [70.0, 100.0]];
        assert_eq!(curve_eval(&pts, 10.0), 20.0);
        assert_eq!(curve_eval(&pts, 50.0), 60.0);
        assert_eq!(curve_eval(&pts, 90.0), 100.0);
        assert_eq!(curve_eval(&[], 50.0), 100.0);
    }

    #[test]
    fn controller_takes_and_restores() {
        let root = fake_hwmon();
        let cfg = FanConfig {
            channels: vec![ChannelConfig {
                id: "nct6798:1".into(),
                name: None,
                mode: ChannelMode::Curve { sensor: "k10temp:Tctl".into(), points: vec![[40.0, 30.0], [80.0, 100.0]], min_pct: 20.0, slew_pct_per_s: 0.0 },
            }],
        };
        let mut c = Controller::new(root.clone(), cfg);
        c.tick();
        let chip = root.join("hwmon3");
        assert_eq!(fs::read_to_string(chip.join("pwm1_enable")).unwrap().trim(), "1");
        // 61.5 °C on 40→30 / 80→100 = 67.6 % ≈ 172
        let pwm: u32 = fs::read_to_string(chip.join("pwm1")).unwrap().trim().parse().unwrap();
        assert!((170..=174).contains(&pwm), "pwm {pwm}");
        assert_eq!(c.states[0].sensor_temp, Some(61.5));
        // Switching back to Auto restores the BIOS mode (5).
        c.set_config(FanConfig { channels: vec![ChannelConfig { id: "nct6798:1".into(), name: None, mode: ChannelMode::Auto }] }).unwrap();
        assert_eq!(fs::read_to_string(chip.join("pwm1_enable")).unwrap().trim(), "5");
        drop(c);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_sensor_runs_full_speed() {
        let root = fake_hwmon();
        let cfg = FanConfig {
            channels: vec![ChannelConfig {
                id: "nct6798:2".into(),
                name: None,
                mode: ChannelMode::Curve { sensor: "nope:x".into(), points: vec![[40.0, 30.0]], min_pct: 20.0, slew_pct_per_s: 0.0 },
            }],
        };
        let mut c = Controller::new(root.clone(), cfg);
        c.tick();
        assert_eq!(fs::read_to_string(root.join("hwmon3/pwm2")).unwrap().trim(), "255");
        assert!(c.states[0].error.is_some());
        drop(c);
        fs::remove_dir_all(root).unwrap();
    }
}
