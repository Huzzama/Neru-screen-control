//! Fan and pump speeds from every hwmon chip (`fanN_input`, RPM).
//!
//! Exposed to themes as `fan.1` … `fan.N` (in discovery order, stable while
//! the chips stay the same), plus `fan.cpu` / `fan.pump` guessed from labels
//! and `fan.max`. GPU fans live under `gpu.fan`. Names given to channels on
//! the Fans page (`/etc/neru-screen-control/fans.json`) replace the chip's
//! labels, so `fan.cpu` follows a header you called "CPU".
//!
//! A header that reports 0 RPM has no tach signal: nothing plugged in, or a
//! hub / splitter that does not pass it through (PWM still works).

use std::path::PathBuf;
use std::time::Instant;

use super::hwmon;
use super::FanReading;

#[derive(Debug, Clone)]
struct FanInput {
    id: String,
    label: String,
    path: PathBuf,
}

pub struct FanSource {
    inputs: Vec<FanInput>,
    names_checked: Option<Instant>,
}

/// `chip:N` → user name from the fan configuration, when it exists.
fn user_names() -> Vec<(String, String)> {
    let Ok(s) = std::fs::read_to_string(crate::fans::daemon::CONFIG_PATH) else { return Vec::new() };
    let Ok(cfg) = serde_json::from_str::<crate::fans::FanConfig>(&s) else { return Vec::new() };
    cfg.channels.into_iter().filter_map(|c| Some((c.id, c.name?))).filter(|(_, n)| !n.trim().is_empty()).collect()
}

fn scan() -> Vec<FanInput> {
    let mut v = Vec::new();
    for chip in hwmon::chips() {
        // GPU fans are reported with the GPU; skip them here.
        if chip.name == "amdgpu" || chip.name == "nvidia" {
            continue;
        }
        for (path, label) in hwmon::inputs(&chip.path, "fan") {
            let stem = path.file_name().map(|n| n.to_string_lossy().trim_end_matches("_input").to_string()).unwrap_or_default();
            let id = format!("{}:{stem}", chip.name);
            let label = label.filter(|l| !l.is_empty()).unwrap_or_else(|| format!("{} {stem}", chip.name));
            v.push(FanInput { id, label, path });
        }
    }
    v
}

impl FanSource {
    pub fn new() -> Self {
        let mut s = Self { inputs: scan(), names_checked: None };
        s.apply_names();
        s
    }

    /// Replace labels with the names from the Fans page (`<chip>:fanN` ↔ `<chip>:N`).
    fn apply_names(&mut self) {
        self.names_checked = Some(Instant::now());
        let names = user_names();
        if names.is_empty() {
            return;
        }
        for f in self.inputs.iter_mut() {
            let channel_id = f.id.replacen(":fan", ":", 1);
            if let Some((_, n)) = names.iter().find(|(id, _)| *id == channel_id) {
                f.label = n.clone();
            }
        }
    }

    pub fn describe(&self) -> Vec<String> {
        vec![format!("fans: {}", if self.inputs.is_empty() { "none".to_string() } else { self.inputs.iter().map(|f| f.id.as_str()).collect::<Vec<_>>().join(", ") })]
    }

    pub fn read(&mut self) -> Vec<FanReading> {
        if self.names_checked.map_or(true, |t| t.elapsed().as_secs() >= 30) {
            self.apply_names();
        }
        self.inputs
            .iter()
            .map(|f| FanReading {
                id: f.id.clone(),
                label: f.label.clone(),
                rpm: hwmon::read_i64(&f.path).map(|v| v as f32),
            })
            .collect()
    }
}

impl Default for FanSource {
    fn default() -> Self {
        Self::new()
    }
}
