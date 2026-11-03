//! System-wide numbers: load averages, process count, uptime (`/proc`).

use std::fs;

use super::SysMetrics;

pub fn read() -> SysMetrics {
    let mut m = SysMetrics::default();
    if let Ok(s) = fs::read_to_string("/proc/loadavg") {
        // "0.52 0.58 0.59 2/1234 56789"
        let f: Vec<&str> = s.split_whitespace().collect();
        m.load1 = f.first().and_then(|v| v.parse().ok());
        m.load5 = f.get(1).and_then(|v| v.parse().ok());
        m.load15 = f.get(2).and_then(|v| v.parse().ok());
        m.procs = f.get(3).and_then(|v| v.split('/').nth(1)).and_then(|v| v.parse().ok());
    }
    if let Ok(s) = fs::read_to_string("/proc/uptime") {
        m.uptime_s = s.split_whitespace().next().and_then(|v| v.parse().ok());
    }
    m
}

/// "3d 04:12" / "04:12" style uptime.
pub fn format_uptime(secs: f32) -> String {
    let s = secs.max(0.0) as u64;
    let (d, h, mi) = (s / 86400, (s / 3600) % 24, (s / 60) % 60);
    if d > 0 {
        format!("{d}d {h:02}:{mi:02}")
    } else {
        format!("{h:02}:{mi:02}")
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn formats_uptime() {
        assert_eq!(super::format_uptime(0.0), "00:00");
        assert_eq!(super::format_uptime(3.0 * 86400.0 + 4.0 * 3600.0 + 12.0 * 60.0 + 5.0), "3d 04:12");
    }
}
