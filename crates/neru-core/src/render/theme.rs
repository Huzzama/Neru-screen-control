//! Theme document: what is drawn on the screen.
//!
//! Themes are JSON files in `~/.config/neru-screen-control/themes/`. Every
//! field has a default so old files keep loading when new fields appear.
//!
//! Coordinates are in screen pixels, origin top-left. Text `x`/`y` is the
//! baseline start (or centre / end for `align`); gauges use `x`/`y` as the
//! centre; everything else is the top-left corner.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    pub name: String,
    pub version: u32,
    pub width: u32,
    pub height: u32,
    pub background: Background,
    pub elements: Vec<Element>,
    /// Smoothing factor for bars / gauges, 0 (none) … 0.95 (very slow).
    pub smoothing: f32,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            name: "Untitled".into(),
            version: 1,
            width: 720,
            height: 720,
            background: Background::default(),
            elements: Vec::new(),
            smoothing: 0.5,
        }
    }
}

impl Theme {
    pub fn from_json(s: &str) -> crate::Result<Self> {
        let t: Theme = serde_json::from_str(s)?;
        if t.width == 0 || t.height == 0 {
            return Err(crate::Error::InvalidValue("theme size must be > 0".into()));
        }
        Ok(t)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("theme serialises")
    }

    /// A theme sized for a screen, with a single centred placeholder.
    pub fn blank(width: u32, height: u32) -> Self {
        Self { width, height, ..Default::default() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    /// Fill the box, crop the overflow.
    #[default]
    Cover,
    /// Fit inside the box, keep the aspect ratio.
    Contain,
    /// Stretch to the box.
    Stretch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Background {
    Color { color: String },
    Gradient { from: String, to: String, angle: f32 },
    /// Image, GIF or video file (absolute path, or a name inside the media dir).
    Media {
        path: String,
        #[serde(default)]
        fit: Fit,
        #[serde(default = "one")]
        opacity: f32,
    },
}

impl Default for Background {
    fn default() -> Self {
        Background::Color { color: "#0b0b12".into() }
    }
}

fn one() -> f32 {
    1.0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

/// A colour stop for value-driven colours: `at` is a metric value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stop {
    pub at: f32,
    pub color: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Element {
    pub id: String,
    pub x: f32,
    pub y: f32,
    /// Degrees, clockwise, around the element's own origin.
    pub rotation: f32,
    pub opacity: f32,
    pub visible: bool,
    /// Editor-only: ignore clicks/drags.
    pub locked: bool,
    #[serde(flatten)]
    pub kind: Kind,
}

impl Default for Element {
    fn default() -> Self {
        Self {
            id: String::new(),
            x: 0.0,
            y: 0.0,
            rotation: 0.0,
            opacity: 1.0,
            visible: true,
            locked: false,
            kind: Kind::Text(Text::default()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Kind {
    Text(Text),
    Bar(Bar),
    Gauge(Gauge),
    Rect(Rect),
    Ellipse(Ellipse),
    Line(Line),
    Image(Image),
}

/// Text with placeholders: `{cpu.temp}`, `{cpu.temp:1}` (decimals),
/// `{time:%H:%M}`, `{date:%a %d %b}`, `{cpu.name}`, `{gpu.name}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Text {
    pub text: String,
    pub font: String,
    pub size: f32,
    /// 100–900.
    pub weight: u16,
    pub color: String,
    pub align: Align,
    pub letter_spacing: f32,
    /// Colour by the value of this metric (overrides `color` when set).
    pub ramp_metric: Option<String>,
    pub ramp: Vec<Stop>,
}

impl Default for Text {
    fn default() -> Self {
        Self {
            text: "Text".into(),
            font: "Inter".into(),
            size: 48.0,
            weight: 600,
            color: "#ffffff".into(),
            align: Align::Left,
            letter_spacing: 0.0,
            ramp_metric: None,
            ramp: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bar {
    pub w: f32,
    pub h: f32,
    pub metric: String,
    pub min: f32,
    pub max: f32,
    pub color: String,
    pub track: String,
    pub radius: f32,
    /// Fill bottom→top instead of left→right.
    pub vertical: bool,
    pub ramp: Vec<Stop>,
}

impl Default for Bar {
    fn default() -> Self {
        Self {
            w: 300.0,
            h: 24.0,
            metric: "cpu.usage".into(),
            min: 0.0,
            max: 100.0,
            color: "#ffc20e".into(),
            track: "#26263a".into(),
            radius: 12.0,
            vertical: false,
            ramp: Vec::new(),
        }
    }
}

/// Arc gauge centred on (x, y).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Gauge {
    pub r: f32,
    pub thickness: f32,
    pub metric: String,
    pub min: f32,
    pub max: f32,
    /// Where the arc starts, degrees clockwise from 12 o'clock.
    pub start: f32,
    /// How many degrees the full arc covers.
    pub sweep: f32,
    pub color: String,
    pub track: String,
    pub round_caps: bool,
    pub ramp: Vec<Stop>,
}

impl Default for Gauge {
    fn default() -> Self {
        Self {
            r: 120.0,
            thickness: 22.0,
            metric: "cpu.temp".into(),
            min: 0.0,
            max: 100.0,
            start: -135.0,
            sweep: 270.0,
            color: "#9b30ff".into(),
            track: "#26263a".into(),
            round_caps: true,
            ramp: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rect {
    pub w: f32,
    pub h: f32,
    pub fill: String,
    pub stroke: String,
    pub stroke_width: f32,
    pub radius: f32,
}

impl Default for Rect {
    fn default() -> Self {
        Self {
            w: 200.0,
            h: 100.0,
            fill: "#1a1a2a".into(),
            stroke: "none".into(),
            stroke_width: 0.0,
            radius: 16.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ellipse {
    pub rx: f32,
    pub ry: f32,
    pub fill: String,
    pub stroke: String,
    pub stroke_width: f32,
}

impl Default for Ellipse {
    fn default() -> Self {
        Self { rx: 60.0, ry: 60.0, fill: "#1a1a2a".into(), stroke: "none".into(), stroke_width: 0.0 }
    }
}

/// Line from (x, y) to (x + w, y + h).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Line {
    pub w: f32,
    pub h: f32,
    pub stroke: String,
    pub stroke_width: f32,
    pub round_caps: bool,
}

impl Default for Line {
    fn default() -> Self {
        Self { w: 200.0, h: 0.0, stroke: "#ffffff".into(), stroke_width: 2.0, round_caps: true }
    }
}

/// Image, GIF or video (absolute path, or a name inside the media dir).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Image {
    pub w: f32,
    pub h: f32,
    pub path: String,
    pub fit: Fit,
}

impl Default for Image {
    fn default() -> Self {
        Self { w: 200.0, h: 200.0, path: String::new(), fit: Fit::Cover }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_defaults() {
        let json = r##"{
            "name": "t", "width": 320, "height": 320,
            "background": {"kind": "gradient", "from": "#000", "to": "#222", "angle": 90},
            "elements": [
                {"id": "a", "type": "text", "x": 10, "y": 50, "text": "CPU {cpu.temp:0}°"},
                {"id": "b", "type": "bar", "x": 10, "y": 60, "metric": "gpu.usage", "w": 100},
                {"id": "c", "type": "gauge", "x": 160, "y": 160},
                {"id": "d", "type": "image", "x": 0, "y": 0, "w": 50, "h": 50, "path": "logo.png", "fit": "contain"}
            ]
        }"##;
        let t = Theme::from_json(json).unwrap();
        assert_eq!(t.elements.len(), 4);
        match &t.elements[0].kind {
            Kind::Text(tx) => assert_eq!(tx.size, 48.0),
            _ => panic!(),
        }
        match &t.elements[1].kind {
            Kind::Bar(b) => {
                assert_eq!(b.w, 100.0);
                assert_eq!(b.h, 24.0);
            }
            _ => panic!(),
        }
        let back = Theme::from_json(&t.to_json()).unwrap();
        assert_eq!(back, t);
    }

    #[test]
    fn rejects_zero_size() {
        assert!(Theme::from_json(r#"{"width":0,"height":10}"#).is_err());
    }
}
