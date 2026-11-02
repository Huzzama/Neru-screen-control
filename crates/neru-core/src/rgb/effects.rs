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
    /// A device-native mode (runs on the hardware without Neru).
    Hardware { mode: String, color: Option<String>, speed: Option<u32>, brightness: Option<u32> },
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

/// Colours for `n` LEDs of a zone at time `t` seconds.
/// `metric` is the current value for temperature effects.
pub fn render(effect: &Effect, t: f32, n: usize, metric: Option<f32>) -> Vec<Color> {
    let n = n.max(1);
    match effect {
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
    fn effects_serialise_tagged() {
        let s = serde_json::to_string(&Effect::Rainbow { speed: 5.0, spread: 180.0 }).unwrap();
        assert!(s.contains("\"kind\":\"rainbow\""));
        let back: Effect = serde_json::from_str(&s).unwrap();
        assert!(matches!(back, Effect::Rainbow { .. }));
    }
}
