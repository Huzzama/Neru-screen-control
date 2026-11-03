//! Disk metrics: throughput from `/proc/diskstats` (physical block devices
//! only), root filesystem usage from `statvfs`, drive temperature from the
//! `nvme` / `drivetemp` hwmon drivers.

use std::ffi::CString;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use super::hwmon;
use super::DiskMetrics;

#[derive(Debug, Clone, Copy, Default)]
struct Sectors {
    read: u64,
    written: u64,
}

pub struct DiskSource {
    devices: Vec<String>,
    temp: Option<PathBuf>,
    temp_source: String,
    last: Option<(Sectors, Instant)>,
}

/// Whole block devices backed by real hardware (`/sys/block/<dev>/device`).
fn physical_devices() -> Vec<String> {
    let Ok(rd) = fs::read_dir("/sys/block") else {
        return Vec::new();
    };
    let mut v: Vec<String> = rd
        .flatten()
        .filter(|e| e.path().join("device").exists())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    v.sort();
    v
}

/// Sum of sectors read / written over `devices` from a `/proc/diskstats` dump.
fn parse_diskstats(s: &str, devices: &[String]) -> Sectors {
    let mut out = Sectors::default();
    for line in s.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 10 {
            continue;
        }
        if !devices.iter().any(|d| d == f[2]) {
            continue;
        }
        out.read += f[5].parse::<u64>().unwrap_or(0);
        out.written += f[9].parse::<u64>().unwrap_or(0);
    }
    out
}

fn find_temp() -> (Option<PathBuf>, String) {
    for chip in hwmon::chips() {
        if chip.name == "nvme" || chip.name == "drivetemp" {
            // nvme exposes "Composite" first; drivetemp a single temp1.
            if let Some(p) = hwmon::find_temp(&chip.path, &["Composite"]) {
                return (Some(p), format!("hwmon {}", chip.name));
            }
        }
    }
    (None, "unavailable".into())
}

/// Used / total bytes of the filesystem at `path`.
pub fn usage(path: &str) -> Option<(u64, u64)> {
    let c = CString::new(path).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c` is a valid NUL-terminated path and `st` is a properly sized out-param.
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return None;
    }
    let frsize = if st.f_frsize > 0 { st.f_frsize as u64 } else { st.f_bsize as u64 };
    let total = st.f_blocks as u64 * frsize;
    let free = st.f_bfree as u64 * frsize;
    Some((total.saturating_sub(free), total))
}

impl DiskSource {
    pub fn new() -> Self {
        let (temp, temp_source) = find_temp();
        Self { devices: physical_devices(), temp, temp_source, last: None }
    }

    pub fn describe(&self) -> Vec<String> {
        vec![format!("disk: {}", self.devices.join(", ")), format!("disk.temp: {}", self.temp_source)]
    }

    pub fn read(&mut self) -> DiskMetrics {
        let now = Instant::now();
        let mut m = DiskMetrics::default();
        if let Ok(s) = fs::read_to_string("/proc/diskstats") {
            let cur = parse_diskstats(&s, &self.devices);
            if let Some((prev, t)) = self.last {
                let dt = now.duration_since(t).as_secs_f64();
                if dt > 0.0 {
                    m.read_bps = Some((cur.read.saturating_sub(prev.read) as f64 * 512.0 / dt) as f32);
                    m.write_bps = Some((cur.written.saturating_sub(prev.written) as f64 * 512.0 / dt) as f32);
                }
            }
            self.last = Some((cur, now));
        }
        if let Some((used, total)) = usage("/") {
            m.used_gb = Some(used as f32 / 1e9);
            m.total_gb = Some(total as f32 / 1e9);
            m.pct = if total > 0 { Some(used as f32 / total as f32 * 100.0) } else { None };
        }
        m.temp_c = self.temp.as_deref().and_then(hwmon::read_temp_c);
        m
    }
}

impl Default for DiskSource {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sums_only_listed_devices() {
        let s = "   8       0 sda 100 0 2048 0 50 0 1024 0 0 0 0\n 259       0 nvme0n1 10 0 512 0 5 0 256 0 0 0 0\n   7       0 loop0 1 0 999 0 0 0 0 0 0 0 0\n";
        let t = parse_diskstats(s, &["sda".into(), "nvme0n1".into()]);
        assert_eq!(t.read, 2560);
        assert_eq!(t.written, 1280);
    }

    #[test]
    fn root_usage_is_sane() {
        let (used, total) = usage("/").expect("statvfs /");
        assert!(total > 0 && used <= total);
    }
}
