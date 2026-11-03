//! Lighting effects computed by Neru and pushed to devices in direct mode,
//! plus device-native ("hardware") modes.

use serde::{Deserialize, Serialize};

use super::protocol::Color;
use crate::render::theme::Stop;
use crate::render::values::ramp_color;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Effect {
    /// LEDs off (black, direct mode).
    Off,
    Static { color: String },
    Breathing { color: String, period_s: f32 },
    /// Hue sweep along the zone. `speed` = full cycles per minute,
    /// `spread` = how many degrees of hue the zone spans.
    Rainbow { speed: f32, spread: f32 },
    /// Fade through a list of colours.
    ColorCycle { colors: Vec<String>, period_s: f32 },
    /// Colour follows a metric (e.g. `cpu.temp`) through a ramp.
    Temperature { metric: String, ramp: Vec<Stop> },
    /// Colours spread along the zone and blended into each other (looping).
    /// `speed` scrolls the gradient (cycles per minute, 0 = still); `repeat`
    /// is how many times it fits in the zone.
    Gradient { colors: Vec<String>, speed: f32, repeat: f32 },
    /// A bright pulse travelling along the zone over a background colour.
    /// `width` is the pulse size as a fraction of the zone; `bounce` sends it
    /// back and forth instead of wrapping around.
    Wave { color: String, background: String, period_s: f32, width: f32, bounce: bool },
    /// `count` runs of `width` lit LEDs chasing along the zone.
    Chase { color: String, background: String, period_s: f32, count: u32, width: u32 },
    /// Random LEDs light up and fade out; `density` 0–1 is how many at once.
    Twinkle { color: String, background: String, density: f32, period_s: f32 },
    /// Flash on/off `hz` times per second; `duty` 0–1 is the on fraction.
    Strobe { color: String, hz: f32, duty: f32 },
    /// Candle / fire flicker around a colour; `intensity` 0–1.
    Candle { color: String, intensity: f32 },
    /// A device-native mode (runs on the hardware without Neru).
    Hardware {
        mode: String,
        color: Option<String>,
        speed: Option<u32>,
        brightness: Option<u32>,
        /// 0 left/up, 1 right/down (modes with a direction).
        #[serde(default)]
        direction: Option<u32>,
    },
}

impl Default for Effect {
    fn default() -> Self {
        Effect::Static { color: "#ffc20e".into() }
    }
}

impl Effect {
    pub fn is_software(&self) -> bool {
        !matches!(self, Effect::Hardware { .. })
    }

    /// Default temperature ramp (°C): cool blue → yellow → red.
    pub fn default_temperature(metric: &str) -> Self {
        Effect::Temperature {
            metric: metric.into(),
            ramp: vec![
                Stop { at: 35.0, color: "#3dd6ff".into() },
                Stop { at: 65.0, color: "#ffc20e".into() },
                Stop { at: 85.0, color: "#ff3b5c".into() },
            ],
        }
    }
}

/// An effect for one zone of one device (`zone: None` = every zone).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Assignment {
    /// `Controller::key()`.
    pub controller: String,
    pub zone: Option<u32>,
    pub effect: Effect,
}

/// Deterministic pseudo-random number in 0..1 for an (LED, cycle) pair, so
/// effects need no state between frames.
fn hash01(i: u32, k: u32) -> f32 {
    let mut x = i.wrapping_mul(0x9E37_79B9) ^ k.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x = x.wrapping_mul(0x297A_2D39);
    x ^= x >> 15;
    (x & 0x00FF_FFFF) as f32 / 16_777_216.0
}

fn hex(s: &str) -> Color {
    Color::from_hex(s).unwrap_or(Color::BLACK)
}

/// Colour at `u` (0..1, looping) of an evenly spaced, wrapping colour list.
fn gradient_at(cs: &[Color], u: f32) -> Color {
    match cs.len() {
        0 => Color::BLACK,
        1 => cs[0],
        m => {
            let pos = u.rem_euclid(1.0) * m as f32;
            let i = pos.floor() as usize % m;
            cs[i].lerp(cs[(i + 1) % m], pos - pos.floor())
        }
    }
}

/// Colours for `n` LEDs of a zone at time `t` seconds.
/// `metric` is the current value for temperature effects.
pub fn render(effect: &Effect, t: f32, n: usize, metric: Option<f32>) -> Vec<Color> {
    let n = n.max(1);
    match effect {
        Effect::Gradient { colors, speed, repeat } => {
            let cs: Vec<Color> = colors.iter().filter_map(|c| Color::from_hex(c)).collect();
            let shift = t * speed / 60.0;
            let repeat = repeat.max(0.01);
            (0..n).map(|i| gradient_at(&cs, i as f32 / n as f32 * repeat + shift)).collect()
        }
        Effect::Wave { color, background, period_s, width, bounce } => {
            let (c, bg) = (hex(color), hex(background));
            let p = period_s.max(0.2);
            let width = width.clamp(0.02, 1.0);
            let phase = (t / p).rem_euclid(1.0);
            let head = if *bounce {
                // Triangle wave, keeping the pulse fully inside the zone.
                let tri = 1.0 - (phase * 2.0 - 1.0).abs();
                width / 2.0 + tri * (1.0 - width)
            } else {
                phase
            };
            (0..n)
                .map(|i| {
                    let u = (i as f32 + 0.5) / n as f32;
                    let mut d = (u - head).abs();
                    if !*bounce {
                        d = d.min(1.0 - d); // wrap around
                    }
                    let k = (1.0 - d / (width / 2.0)).clamp(0.0, 1.0);
                    bg.lerp(c, k * k)
                })
                .collect()
        }
        Effect::Chase { color, background, period_s, count, width } => {
            let (c, bg) = (hex(color), hex(background));
            let p = period_s.max(0.2);
            let count = (*count).max(1) as f32;
            let width = (*width).max(1) as f32;
            let spacing = n as f32 / count;
            let pos = (t / p).rem_euclid(1.0) * n as f32;
            (0..n)
                .map(|i| {
                    let r = (i as f32 - pos).rem_euclid(spacing);
                    // Full for the run, a soft tail over the last LED.
                    let k = if r < width - 1.0 { 1.0 } else { (width - r).clamp(0.0, 1.0) };
                    bg.lerp(c, k)
                })
                .collect()
        }
        Effect::Twinkle { color, background, density, period_s } => {
            let (c, bg) = (hex(color), hex(background));
            let p = period_s.max(0.2);
            let density = density.clamp(0.0, 1.0);
            (0..n as u32)
                .map(|i| {
                    let u = t / p + hash01(i, 0);
                    let k = u.floor();
                    let f = u - k;
                    let on = hash01(i, k as u32 + 1) < density;
                    let level = if on { (std::f32::consts::PI * f).sin() } else { 0.0 };
                    bg.lerp(c, level)
                })
                .collect()
        }
        Effect::Strobe { color, hz, duty } => {
            let c = hex(color);
            let hz = hz.clamp(0.1, 30.0);
            let on = (t * hz).rem_euclid(1.0) < duty.clamp(0.02, 0.98);
            vec![if on { c } else { Color::BLACK }; n]
        }
        Effect::Candle { color, intensity } => {
            let c = hex(color);
            let amp = intensity.clamp(0.0, 1.0);
            (0..n as u32)
                .map(|i| {
                    let ph = hash01(i, 7) * 6.28;
                    // Slow sway plus a faster, per-LED sparkle; never fully dark.
                    let slow = 0.5 + 0.5 * (t * 1.7 + ph).sin() * (t * 0.9 + ph * 2.0).cos();
                    let step = (t * 12.0 + hash01(i, 3) * 10.0).floor() as u32;
                    let noise = hash01(i, step);
                    let f = 1.0 - amp * (0.25 * slow + 0.45 * noise);
                    c.scale(f.clamp(0.1, 1.0))
                })
                .collect()
        }
        Effect::Off => vec![Color::BLACK; n],
        Effect::Hardware { .. } => vec![Color::BLACK; n],
        Effect::Static { color } => vec![Color::from_hex(color).unwrap_or(Color::BLACK); n],
        Effect::Breathing { color, period_s } => {
            let c = Color::from_hex(color).unwrap_or(Color::BLACK);
            let p = period_s.max(0.2);
            // Smooth 0..1..0, never fully dark.
            let phase = (t / p) * std::f32::consts::TAU;
            let f = 0.15 + 0.85 * (0.5 - 0.5 * phase.cos());
            vec![c.scale(f); n]
        }
        Effect::Rainbow { speed, spread } => {
            let base = t * speed * 6.0; // cycles/min → degrees/s
            (0..n)
                .map(|i| {
                    let h = base + spread * (i as f32 / n as f32);
                    Color::hsv(h, 1.0, 1.0)
                })
                .collect()
        }
        Effect::ColorCycle { colors, period_s } => {
            let cs: Vec<Color> = colors.iter().filter_map(|c| Color::from_hex(c)).collect();
            if cs.is_empty() {
                return vec![Color::BLACK; n];
            }
            if cs.len() == 1 {
                return vec![cs[0]; n];
            }
            let p = period_s.max(0.2);
            let pos = (t / p).rem_euclid(1.0) * cs.len() as f32;
            let i = pos.floor() as usize % cs.len();
            let frac = pos - pos.floor();
            let c = cs[i].lerp(cs[(i + 1) % cs.len()], frac);
            vec![c; n]
        }
        Effect::Temperature { ramp, .. } => {
            let hex = ramp_color(ramp, metric, "#3dd6ff");
            vec![Color::from_hex(&hex).unwrap_or(Color::BLACK); n]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_and_off() {
        assert_eq!(render(&Effect::Static { color: "#ff0000".into() }, 0.0, 3, None), vec![Color::new(255, 0, 0); 3]);
        assert_eq!(render(&Effect::Off, 1.0, 2, None), vec![Color::BLACK; 2]);
    }

    #[test]
    fn breathing_stays_lit_and_peaks() {
        let e = Effect::Breathing { color: "#ffffff".into(), period_s: 2.0 };
        let dark = render(&e, 0.0, 1, None)[0];
        let bright = render(&e, 1.0, 1, None)[0];
        assert!(dark.r > 20 && dark.r < 60, "{dark:?}");
        assert_eq!(bright, Color::new(255, 255, 255));
    }

    #[test]
    fn rainbow_spreads_hue() {
        let e = Effect::Rainbow { speed: 10.0, spread: 360.0 };
        let c = render(&e, 0.0, 4, None);
        assert_eq!(c[0], Color::new(255, 0, 0));
        assert_ne!(c[1], c[0]);
    }

    #[test]
    fn temperature_follows_ramp() {
        let e = Effect::default_temperature("cpu.temp");
        assert_eq!(render(&e, 0.0, 1, Some(20.0))[0], Color::new(0x3d, 0xd6, 0xff));
        assert_eq!(render(&e, 0.0, 1, Some(90.0))[0], Color::new(0xff, 0x3b, 0x5c));
    }

    #[test]
    fn cycle_interpolates() {
        let e = Effect::ColorCycle { colors: vec!["#000000".into(), "#ffffff".into()], period_s: 2.0 };
        let mid = render(&e, 0.5, 1, None)[0]; // halfway through the first segment
        assert!((mid.r as i32 - 128).abs() <= 1, "{mid:?}");
    }

    #[test]
    fn gradient_blends_and_scrolls() {
        let e = Effect::Gradient { colors: vec!["#ff0000".into(), "#0000ff".into()], speed: 0.0, repeat: 1.0 };
        let c = render(&e, 0.0, 4, None);
        assert_eq!(c[0], Color::new(255, 0, 0));
        assert!(c[2].b > c[1].b, "{c:?}");
        let moving = Effect::Gradient { colors: vec!["#ff0000".into(), "#0000ff".into()], speed: 60.0, repeat: 1.0 };
        assert_ne!(render(&moving, 0.25, 4, None)[0], Color::new(255, 0, 0));
        // A full cycle later it is back where it started.
        assert_eq!(render(&moving, 1.0, 4, None)[0], Color::new(255, 0, 0));
    }

    #[test]
    fn wave_and_chase_light_a_subset() {
        let w = Effect::Wave { color: "#ffffff".into(), background: "#000000".into(), period_s: 4.0, width: 0.2, bounce: false };
        let c = render(&w, 0.0, 10, None);
        let lit = c.iter().filter(|x| x.r > 0).count();
        assert!(lit >= 1 && lit < 10, "{c:?}");
        let ch = Effect::Chase { color: "#ffffff".into(), background: "#000000".into(), period_s: 2.0, count: 2, width: 2 };
        let c = render(&ch, 0.0, 10, None);
        assert_eq!(c.iter().filter(|x| x.r == 255).count(), 4, "{c:?}");
    }

    #[test]
    fn twinkle_strobe_candle_are_bounded() {
        let tw = Effect::Twinkle { color: "#ffffff".into(), background: "#101010".into(), density: 0.5, period_s: 1.0 };
        for t in [0.0, 0.3, 0.7, 1.9] {
            for c in render(&tw, t, 32, None) {
                assert!(c.r >= 16);
            }
        }
        assert_eq!(hash01(3, 9), hash01(3, 9));
        assert_ne!(hash01(3, 9), hash01(4, 9));
        let st = Effect::Strobe { color: "#ff0000".into(), hz: 1.0, duty: 0.5 };
        assert_eq!(render(&st, 0.1, 1, None)[0], Color::new(255, 0, 0));
        assert_eq!(render(&st, 0.6, 1, None)[0], Color::BLACK);
        let ca = Effect::Candle { color: "#ff8000".into(), intensity: 1.0 };
        for c in render(&ca, 2.5, 8, None) {
            assert!(c.r >= 25 && c.b == 0, "{c:?}");
        }
    }

    #[test]
    fn effects_serialise_tagged() {
        let s = serde_json::to_string(&Effect::Rainbow { speed: 5.0, spread: 180.0 }).unwrap();
        assert!(s.contains("\"kind\":\"rainbow\""));
        let back: Effect = serde_json::from_str(&s).unwrap();
        assert!(matches!(back, Effect::Rainbow { .. }));
    }
}
