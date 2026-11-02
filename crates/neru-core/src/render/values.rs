//! Metric values for themes: placeholder substitution, smoothing, colour ramps.

use std::collections::HashMap;

use crate::metrics::Snapshot;

use super::theme::Stop;

/// Everything a theme can read while rendering.
pub struct Values<'a> {
    pub snapshot: &'a Snapshot,
    pub fahrenheit: bool,
    /// Smoothed values for bars/gauges (keyed by metric).
    pub smoothed: &'a HashMap<String, f32>,
    pub now: chrono::DateTime<chrono::Local>,
}

impl Values<'_> {
    /// Raw value for a metric key (temperatures converted if `fahrenheit`).
    pub fn get(&self, key: &str) -> Option<f32> {
        let v = self.snapshot.get(key)?;
        Some(if self.fahrenheit && key.ends_with(".temp") { v * 9.0 / 5.0 + 32.0 } else { v })
    }

    /// Value for bars/gauges: smoothed when available.
    pub fn get_smooth(&self, key: &str) -> Option<f32> {
        self.smoothed.get(key).copied().or_else(|| self.get(key))
    }

    fn get_string(&self, key: &str) -> Option<String> {
        match key {
            "cpu.name" => Some(short_cpu_name(&self.snapshot.cpu.name)),
            "gpu.name" => self.snapshot.gpu.as_ref().map(|g| g.name.clone()),
            _ => None,
        }
    }

    /// Replace `{key}`, `{key:decimals}`, `{time:fmt}`, `{date:fmt}`.
    pub fn substitute(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len() + 8);
        let mut rest = text;
        while let Some(start) = rest.find('{') {
            out.push_str(&rest[..start]);
            let after = &rest[start + 1..];
            match after.find('}') {
                Some(end) => {
                    let inner = &after[..end];
                    out.push_str(&self.expand(inner));
                    rest = &after[end + 1..];
                }
                None => {
                    out.push_str(&rest[start..]);
                    rest = "";
                }
            }
        }
        out.push_str(rest);
        out
    }

    fn expand(&self, inner: &str) -> String {
        let (key, arg) = inner.split_once(':').map_or((inner, None), |(k, a)| (k, Some(a)));
        let key = key.trim();
        match key {
            "time" => self.now.format(arg.unwrap_or("%H:%M")).to_string(),
            "date" => self.now.format(arg.unwrap_or("%a %d %b")).to_string(),
            _ => {
                if let Some(s) = self.get_string(key) {
                    return s;
                }
                let decimals: usize = arg.and_then(|a| a.trim().parse().ok()).unwrap_or(0);
                match self.get(key) {
                    Some(v) => format!("{v:.decimals$}"),
                    None => "--".into(),
                }
            }
        }
    }
}

/// "AMD Ryzen 7 7800X3D 8-Core Processor" → "Ryzen 7 7800X3D".
pub fn short_cpu_name(full: &str) -> String {
    let mut s = full.to_string();
    for junk in ["(R)", "(TM)", "(tm)", "CPU", "Processor", "with Radeon Graphics"] {
        s = s.replace(junk, " ");
    }
    if let Some(i) = s.find("-Core") {
        // drop "8-Core"
        let head = s[..i].trim_end();
        let cut = head.rfind(' ').map_or(0, |p| p);
        s = format!("{} {}", &s[..cut], &s[i + 5..]);
    }
    for brand in ["AMD ", "Intel "] {
        if let Some(stripped) = s.trim_start().strip_prefix(brand) {
            s = stripped.to_string();
        }
    }
    if let Some(i) = s.find('@') {
        s.truncate(i);
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Exponential smoothing of every numeric metric, `factor` in 0..1 (0 = none).
pub fn smooth_into(target: &mut HashMap<String, f32>, snapshot: &Snapshot, fahrenheit: bool, factor: f32) {
    let factor = factor.clamp(0.0, 0.95);
    for (key, _, _) in Snapshot::KEYS {
        let Some(mut v) = snapshot.get(key) else { continue };
        if fahrenheit && key.ends_with(".temp") {
            v = v * 9.0 / 5.0 + 32.0;
        }
        let e = target.entry((*key).to_string()).or_insert(v);
        *e = *e * factor + v * (1.0 - factor);
    }
}

/// Parse `#rgb`, `#rrggbb`, `#rrggbbaa`.
pub fn parse_color(s: &str) -> Option<[u8; 4]> {
    let h = s.trim().strip_prefix('#')?;
    let v = |i: usize, n: usize| u8::from_str_radix(&h[i..i + n], 16).ok();
    match h.len() {
        3 => Some([v(0, 1)? * 17, v(1, 1)? * 17, v(2, 1)? * 17, 255]),
        6 => Some([v(0, 2)?, v(2, 2)?, v(4, 2)?, 255]),
        8 => Some([v(0, 2)?, v(2, 2)?, v(4, 2)?, v(6, 2)?]),
        _ => None,
    }
}

pub fn color_to_hex(c: [u8; 4]) -> String {
    if c[3] == 255 {
        format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
    } else {
        format!("#{:02x}{:02x}{:02x}{:02x}", c[0], c[1], c[2], c[3])
    }
}

/// Colour for `value` on a ramp (stops sorted by `at`); `fallback` when the
/// ramp is empty or unparsable.
pub fn ramp_color(ramp: &[Stop], value: Option<f32>, fallback: &str) -> String {
    let (Some(v), false) = (value, ramp.is_empty()) else {
        return fallback.to_string();
    };
    let mut stops: Vec<(f32, [u8; 4])> = ramp
        .iter()
        .filter_map(|s| Some((s.at, parse_color(&s.color)?)))
        .collect();
    if stops.is_empty() {
        return fallback.to_string();
    }
    stops.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    if v <= stops[0].0 {
        return color_to_hex(stops[0].1);
    }
    for w in stops.windows(2) {
        let (a, b) = (w[0], w[1]);
        if v <= b.0 {
            let t = if b.0 > a.0 { (v - a.0) / (b.0 - a.0) } else { 1.0 };
            let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
            return color_to_hex([mix(a.1[0], b.1[0]), mix(a.1[1], b.1[1]), mix(a.1[2], b.1[2]), mix(a.1[3], b.1[3])]);
        }
    }
    color_to_hex(stops[stops.len() - 1].1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::{CpuMetrics, GpuMetrics};

    fn snap() -> Snapshot {
        Snapshot {
            cpu: CpuMetrics {
                name: "AMD Ryzen 7 7800X3D 8-Core Processor".into(),
                temp_c: Some(61.26),
                usage_pct: Some(12.5),
                ..Default::default()
            },
            gpu: Some(GpuMetrics { name: "Radeon RX 7800 XT".into(), temp_c: None, ..Default::default() }),
            ..Default::default()
        }
    }

    #[test]
    fn substitutes_placeholders() {
        let s = snap();
        let smoothed = HashMap::new();
        let v = Values { snapshot: &s, fahrenheit: false, smoothed: &smoothed, now: chrono::Local::now() };
        assert_eq!(v.substitute("CPU {cpu.temp}° / {cpu.temp:1}° {cpu.usage:0}%"), "CPU 61° / 61.3° 12%");
        assert_eq!(v.substitute("{gpu.temp}"), "--");
        assert_eq!(v.substitute("{cpu.name} | {gpu.name}"), "Ryzen 7 7800X3D | Radeon RX 7800 XT");
        assert_eq!(v.substitute("plain {unclosed"), "plain {unclosed");
        assert!(!v.substitute("{time:%H:%M}").contains('{'));
        let vf = Values { snapshot: &s, fahrenheit: true, smoothed: &smoothed, now: chrono::Local::now() };
        assert_eq!(vf.substitute("{cpu.temp:0}"), "142");
    }

    #[test]
    fn ramps_interpolate() {
        let ramp = vec![
            Stop { at: 40.0, color: "#0000ff".into() },
            Stop { at: 80.0, color: "#ff0000".into() },
        ];
        assert_eq!(ramp_color(&ramp, Some(0.0), "#fff"), "#0000ff");
        assert_eq!(ramp_color(&ramp, Some(60.0), "#fff"), "#800080");
        assert_eq!(ramp_color(&ramp, Some(99.0), "#fff"), "#ff0000");
        assert_eq!(ramp_color(&ramp, None, "#fff"), "#fff");
        assert_eq!(ramp_color(&[], Some(1.0), "#abc"), "#abc");
    }

    #[test]
    fn smoothing_moves_toward_target() {
        let mut m = HashMap::new();
        let s = snap();
        smooth_into(&mut m, &s, false, 0.5);
        assert_eq!(m["cpu.temp"], 61.26);
        let mut s2 = s.clone();
        s2.cpu.temp_c = Some(81.26);
        smooth_into(&mut m, &s2, false, 0.5);
        assert!((m["cpu.temp"] - 71.26).abs() < 0.01);
    }

    #[test]
    fn parses_colors() {
        assert_eq!(parse_color("#fff"), Some([255, 255, 255, 255]));
        assert_eq!(parse_color("#ffc20e"), Some([255, 194, 14, 255]));
        assert_eq!(parse_color("#ffc20e80"), Some([255, 194, 14, 128]));
        assert_eq!(parse_color("red"), None);
    }
}
