//! Small helpers over `/sys/class/hwmon`.

use std::fs;
use std::path::{Path, PathBuf};

/// One hwmon chip.
#[derive(Debug, Clone)]
pub struct Chip {
    pub path: PathBuf,
    /// Driver name, e.g. `k10temp`, `coretemp`, `amdgpu`, `nct6798`.
    pub name: String,
}

pub fn chips() -> Vec<Chip> {
    let Ok(rd) = fs::read_dir("/sys/class/hwmon") else {
        return Vec::new();
    };
    let mut v: Vec<Chip> = rd
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let name = read_trimmed(&path.join("name"))?;
            Some(Chip { path, name })
        })
        .collect();
    v.sort_by(|a, b| a.path.cmp(&b.path));
    v
}

pub fn read_trimmed(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

pub fn read_i64(path: &Path) -> Option<i64> {
    read_trimmed(path)?.parse().ok()
}

/// All `<prefix>N_input` files of a chip with their labels (if any).
pub fn inputs(chip: &Path, prefix: &str) -> Vec<(PathBuf, Option<String>)> {
    let Ok(rd) = fs::read_dir(chip) else {
        return Vec::new();
    };
    let mut v: Vec<(PathBuf, Option<String>)> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.starts_with(prefix) || !name.ends_with("_input") {
                return None;
            }
            let stem = name.trim_end_matches("_input");
            let label = read_trimmed(&chip.join(format!("{stem}_label")));
            Some((e.path(), label))
        })
        .collect();
    v.sort();
    v
}

/// Find a `tempN_input` on `chip` whose label is one of `preferred`
/// (in priority order), else the first temp input.
pub fn find_temp(chip: &Path, preferred: &[&str]) -> Option<PathBuf> {
    let all = inputs(chip, "temp");
    for want in preferred {
        if let Some((p, _)) = all
            .iter()
            .find(|(_, l)| l.as_deref().map_or(false, |l| l.eq_ignore_ascii_case(want)))
        {
            return Some(p.clone());
        }
    }
    all.first().map(|(p, _)| p.clone())
}

/// Read a millidegree temperature file as °C.
pub fn read_temp_c(path: &Path) -> Option<f32> {
    read_i64(path).map(|v| v as f32 / 1000.0)
}
