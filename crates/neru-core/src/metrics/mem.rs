//! RAM from `/proc/meminfo`.

use super::MemMetrics;

pub fn read() -> MemMetrics {
    let Ok(s) = std::fs::read_to_string("/proc/meminfo") else {
        return MemMetrics::default();
    };
    parse(&s)
}

fn parse(s: &str) -> MemMetrics {
    let mut total_kb = 0f32;
    let mut avail_kb = 0f32;
    for line in s.lines() {
        let mut it = line.split_whitespace();
        match (it.next(), it.next()) {
            (Some("MemTotal:"), Some(v)) => total_kb = v.parse().unwrap_or(0.0),
            (Some("MemAvailable:"), Some(v)) => avail_kb = v.parse().unwrap_or(0.0),
            _ => {}
        }
    }
    let total_mb = total_kb / 1024.0;
    let used_mb = (total_kb - avail_kb).max(0.0) / 1024.0;
    MemMetrics {
        used_mb,
        total_mb,
        pct: if total_mb > 0.0 { used_mb / total_mb * 100.0 } else { 0.0 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_meminfo() {
        let m = parse("MemTotal:       32768000 kB\nMemFree:        1000 kB\nMemAvailable:   16384000 kB\n");
        assert!((m.total_mb - 32000.0).abs() < 0.01);
        assert!((m.used_mb - 16000.0).abs() < 0.01);
        assert!((m.pct - 50.0).abs() < 0.01);
    }
}
