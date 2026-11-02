//! OpenRGB SDK network protocol (versions 0–4): packet framing and the
//! device description format. Everything is little-endian.
//!
//! Reference: OpenRGB's `NetworkProtocol.h` and `RGBController.cpp`.

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// Highest protocol version this client speaks.
pub const PROTOCOL_VERSION: u32 = 4;
pub const MAGIC: &[u8; 4] = b"ORGB";
pub const HEADER_LEN: usize = 16;
pub const MAX_PACKET: usize = 8 * 1024 * 1024;

pub mod id {
    pub const REQUEST_CONTROLLER_COUNT: u32 = 0;
    pub const REQUEST_CONTROLLER_DATA: u32 = 1;
    pub const REQUEST_PROTOCOL_VERSION: u32 = 40;
    pub const SET_CLIENT_NAME: u32 = 50;
    pub const DEVICE_LIST_UPDATED: u32 = 100;
    pub const REQUEST_RESCAN_DEVICES: u32 = 140;
    pub const PROFILE_LIST: u32 = 150;
    pub const SAVE_PROFILE: u32 = 151;
    pub const LOAD_PROFILE: u32 = 152;
    pub const DELETE_PROFILE: u32 = 153;
    pub const UPDATE_LEDS: u32 = 1050;
    pub const UPDATE_ZONE_LEDS: u32 = 1051;
    pub const UPDATE_SINGLE_LED: u32 = 1052;
    pub const SET_CUSTOM_MODE: u32 = 1100;
    pub const UPDATE_MODE: u32 = 1101;
    pub const SAVE_MODE: u32 = 1102;
}

/// Mode flags (`MODE_FLAG_*`).
pub mod mode_flags {
    pub const HAS_SPEED: u32 = 1 << 0;
    pub const HAS_DIRECTION_LR: u32 = 1 << 1;
    pub const HAS_DIRECTION_UD: u32 = 1 << 2;
    pub const HAS_DIRECTION_HV: u32 = 1 << 3;
    pub const HAS_BRIGHTNESS: u32 = 1 << 4;
    pub const HAS_PER_LED_COLOR: u32 = 1 << 5;
    pub const HAS_MODE_SPECIFIC_COLOR: u32 = 1 << 6;
    pub const HAS_RANDOM_COLOR: u32 = 1 << 7;
    pub const MANUAL_SAVE: u32 = 1 << 8;
    pub const AUTOMATIC_SAVE: u32 = 1 << 9;
}

/// `MODE_COLORS_*`.
pub mod color_mode {
    pub const NONE: u32 = 0;
    pub const PER_LED: u32 = 1;
    pub const MODE_SPECIFIC: u32 = 2;
    pub const RANDOM: u32 = 3;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0 };

    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// OpenRGB packs colours as `0x00BBGGRR`.
    pub fn from_u32(v: u32) -> Self {
        Self { r: (v & 0xff) as u8, g: ((v >> 8) & 0xff) as u8, b: ((v >> 16) & 0xff) as u8 }
    }

    pub fn to_u32(self) -> u32 {
        self.r as u32 | (self.g as u32) << 8 | (self.b as u32) << 16
    }

    pub fn from_hex(s: &str) -> Option<Self> {
        let c = crate::render::values::parse_color(s)?;
        Some(Self { r: c[0], g: c[1], b: c[2] })
    }

    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    pub fn scale(self, f: f32) -> Self {
        let f = f.clamp(0.0, 1.0);
        Self { r: (self.r as f32 * f) as u8, g: (self.g as f32 * f) as u8, b: (self.b as f32 * f) as u8 }
    }

    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let m = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Self { r: m(self.r, other.r), g: m(self.g, other.g), b: m(self.b, other.b) }
    }

    /// HSV → RGB, h in degrees, s and v in 0..1.
    pub fn hsv(h: f32, s: f32, v: f32) -> Self {
        let h = h.rem_euclid(360.0) / 60.0;
        let i = h.floor();
        let f = h - i;
        let p = v * (1.0 - s);
        let q = v * (1.0 - s * f);
        let t = v * (1.0 - s * (1.0 - f));
        let (r, g, b) = match i as i32 {
            0 => (v, t, p),
            1 => (q, v, p),
            2 => (p, v, t),
            3 => (p, q, v),
            4 => (t, p, v),
            _ => (v, p, q),
        };
        Self { r: (r * 255.0).round() as u8, g: (g * 255.0).round() as u8, b: (b * 255.0).round() as u8 }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Mode {
    pub name: String,
    pub value: i32,
    pub flags: u32,
    pub speed_min: u32,
    pub speed_max: u32,
    pub brightness_min: u32,
    pub brightness_max: u32,
    pub colors_min: u32,
    pub colors_max: u32,
    pub speed: u32,
    pub brightness: u32,
    pub direction: u32,
    pub color_mode: u32,
    pub colors: Vec<Color>,
}

impl Mode {
    pub fn has(&self, flag: u32) -> bool {
        self.flags & flag != 0
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    pub name: String,
    pub kind: u32,
    pub start_idx: u32,
    pub leds_count: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Zone {
    pub name: String,
    /// 0 single, 1 linear, 2 matrix.
    pub kind: u32,
    pub leds_min: u32,
    pub leds_max: u32,
    pub leds_count: u32,
    pub matrix: Option<(u32, u32)>,
    pub segments: Vec<Segment>,
    /// Index of the zone's first LED in the controller's LED list.
    pub start_idx: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Controller {
    pub index: u32,
    /// `device_type` enum: 0 motherboard, 1 dram, 2 gpu, 3 cooler, 4 ledstrip, 5 keyboard,
    /// 6 mouse, 7 mousemat, 8 headset, 9 headset stand, 10 gamepad, 11 light, 12 speaker,
    /// 13 virtual, 14 storage, 15 case, 16 microphone, 17 accessory, 18 keypad, 19 unknown.
    pub kind: u32,
    pub name: String,
    pub vendor: String,
    pub description: String,
    pub version: String,
    pub serial: String,
    pub location: String,
    pub active_mode: i32,
    pub modes: Vec<Mode>,
    pub zones: Vec<Zone>,
    pub leds: Vec<String>,
    pub colors: Vec<Color>,
}

impl Controller {
    /// Stable key across reconnects (indices change when devices come and go).
    pub fn key(&self) -> String {
        format!("{}|{}|{}", self.name, self.location, self.serial)
    }

    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            0 => "motherboard",
            1 => "dram",
            2 => "gpu",
            3 => "cooler",
            4 => "ledstrip",
            5 => "keyboard",
            6 => "mouse",
            7 => "mousemat",
            8 => "headset",
            9 => "headset_stand",
            10 => "gamepad",
            11 => "light",
            12 => "speaker",
            13 => "virtual",
            14 => "storage",
            15 => "case",
            16 => "microphone",
            17 => "accessory",
            18 => "keypad",
            _ => "unknown",
        }
    }

    /// The mode to use for software effects: "Direct", else any per-LED mode.
    pub fn direct_mode(&self) -> Option<usize> {
        self.modes
            .iter()
            .position(|m| m.name.eq_ignore_ascii_case("direct") && m.has(mode_flags::HAS_PER_LED_COLOR))
            .or_else(|| self.modes.iter().position(|m| m.has(mode_flags::HAS_PER_LED_COLOR)))
    }
}

// ── packets ─────────────────────────────────────────────────────────────────

pub fn header(dev: u32, pkt_id: u32, size: u32) -> [u8; HEADER_LEN] {
    let mut h = [0u8; HEADER_LEN];
    h[..4].copy_from_slice(MAGIC);
    h[4..8].copy_from_slice(&dev.to_le_bytes());
    h[8..12].copy_from_slice(&pkt_id.to_le_bytes());
    h[12..16].copy_from_slice(&size.to_le_bytes());
    h
}

pub fn parse_header(h: &[u8]) -> Result<(u32, u32, u32)> {
    if h.len() < HEADER_LEN || &h[..4] != MAGIC {
        return Err(Error::Protocol("bad OpenRGB packet magic".into()));
    }
    let u = |i: usize| u32::from_le_bytes([h[i], h[i + 1], h[i + 2], h[i + 3]]);
    Ok((u(4), u(8), u(12)))
}

// ── reader ──────────────────────────────────────────────────────────────────

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.pos + n > self.data.len() {
            return Err(Error::Protocol(format!(
                "truncated controller description (need {n} bytes at {}, have {})",
                self.pos,
                self.data.len()
            )));
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn u16(&mut self) -> Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }

    /// u16 length (including NUL) + bytes.
    fn string(&mut self) -> Result<String> {
        let len = self.u16()? as usize;
        let b = self.take(len)?;
        let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
        Ok(String::from_utf8_lossy(&b[..end]).to_string())
    }

    fn color(&mut self) -> Result<Color> {
        Ok(Color::from_u32(self.u32()?))
    }
}

fn parse_mode(r: &mut Reader, v: u32) -> Result<Mode> {
    let mut m = Mode { name: r.string()?, ..Default::default() };
    if v < 6 {
        m.value = r.i32()?;
    }
    m.flags = r.u32()?;
    m.speed_min = r.u32()?;
    m.speed_max = r.u32()?;
    if v >= 3 {
        m.brightness_min = r.u32()?;
        m.brightness_max = r.u32()?;
    }
    m.colors_min = r.u32()?;
    m.colors_max = r.u32()?;
    m.speed = r.u32()?;
    if v >= 3 {
        m.brightness = r.u32()?;
    }
    m.direction = r.u32()?;
    m.color_mode = r.u32()?;
    let n = r.u16()? as usize;
    m.colors = (0..n).map(|_| r.color()).collect::<Result<_>>()?;
    Ok(m)
}

/// Serialise a mode the way the server expects it in `UPDATE_MODE` (same
/// layout as the description, for protocol `v`).
pub fn encode_mode(m: &Mode, v: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(64 + m.colors.len() * 4);
    put_string(&mut out, &m.name);
    if v < 6 {
        out.extend_from_slice(&m.value.to_le_bytes());
    }
    out.extend_from_slice(&m.flags.to_le_bytes());
    out.extend_from_slice(&m.speed_min.to_le_bytes());
    out.extend_from_slice(&m.speed_max.to_le_bytes());
    if v >= 3 {
        out.extend_from_slice(&m.brightness_min.to_le_bytes());
        out.extend_from_slice(&m.brightness_max.to_le_bytes());
    }
    out.extend_from_slice(&m.colors_min.to_le_bytes());
    out.extend_from_slice(&m.colors_max.to_le_bytes());
    out.extend_from_slice(&m.speed.to_le_bytes());
    if v >= 3 {
        out.extend_from_slice(&m.brightness.to_le_bytes());
    }
    out.extend_from_slice(&m.direction.to_le_bytes());
    out.extend_from_slice(&m.color_mode.to_le_bytes());
    out.extend_from_slice(&(m.colors.len() as u16).to_le_bytes());
    for c in &m.colors {
        out.extend_from_slice(&c.to_u32().to_le_bytes());
    }
    out
}

pub fn put_string(out: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    out.extend_from_slice(&((bytes.len() + 1) as u16).to_le_bytes());
    out.extend_from_slice(bytes);
    out.push(0);
}

fn parse_zone(r: &mut Reader, v: u32) -> Result<Zone> {
    let mut z = Zone { name: r.string()?, ..Default::default() };
    z.kind = r.u32()?;
    z.leds_min = r.u32()?;
    z.leds_max = r.u32()?;
    z.leds_count = r.u32()?;
    let matrix_len = r.u16()? as usize;
    if matrix_len > 0 {
        let h = r.u32()?;
        let w = r.u32()?;
        let cells = (h as usize).saturating_mul(w as usize);
        if 8 + cells * 4 != matrix_len {
            return Err(Error::Protocol("matrix map size mismatch".into()));
        }
        r.take(cells * 4)?;
        z.matrix = Some((w, h));
    }
    if v >= 4 {
        let n = r.u16()? as usize;
        for _ in 0..n {
            z.segments.push(Segment { name: r.string()?, kind: r.u32()?, start_idx: r.u32()?, leds_count: r.u32()? });
        }
    }
    Ok(z)
}

/// Parse the payload of a `REQUEST_CONTROLLER_DATA` reply (after its leading
/// `data_size` u32) for negotiated protocol `v`.
pub fn parse_controller(payload: &[u8], v: u32, index: u32) -> Result<Controller> {
    let mut r = Reader::new(payload);
    let _data_size = r.u32()?;
    let mut c = Controller { index, ..Default::default() };
    c.kind = r.u32()?;
    c.name = r.string()?;
    if v >= 1 {
        c.vendor = r.string()?;
    }
    c.description = r.string()?;
    c.version = r.string()?;
    c.serial = r.string()?;
    c.location = r.string()?;
    let num_modes = r.u16()? as usize;
    c.active_mode = r.i32()?;
    for _ in 0..num_modes {
        c.modes.push(parse_mode(&mut r, v)?);
    }
    let num_zones = r.u16()? as usize;
    let mut start = 0u32;
    for _ in 0..num_zones {
        let mut z = parse_zone(&mut r, v)?;
        z.start_idx = start;
        start += z.leds_count;
        c.zones.push(z);
    }
    let num_leds = r.u16()? as usize;
    for _ in 0..num_leds {
        c.leds.push(r.string()?);
        if v < 6 {
            r.u32()?; // led value
        }
    }
    let num_colors = r.u16()? as usize;
    c.colors = (0..num_colors).map(|_| r.color()).collect::<Result<_>>()?;
    Ok(c)
}

/// Payload for `UPDATE_LEDS`.
pub fn encode_update_leds(colors: &[Color]) -> Vec<u8> {
    let size = 4 + 2 + colors.len() * 4;
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(&(size as u32).to_le_bytes());
    out.extend_from_slice(&(colors.len() as u16).to_le_bytes());
    for c in colors {
        out.extend_from_slice(&c.to_u32().to_le_bytes());
    }
    out
}

/// Payload for `UPDATE_ZONE_LEDS`.
pub fn encode_update_zone_leds(zone: u32, colors: &[Color]) -> Vec<u8> {
    let size = 4 + 4 + 2 + colors.len() * 4;
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(&(size as u32).to_le_bytes());
    out.extend_from_slice(&zone.to_le_bytes());
    out.extend_from_slice(&(colors.len() as u16).to_le_bytes());
    for c in colors {
        out.extend_from_slice(&c.to_u32().to_le_bytes());
    }
    out
}

/// Payload for `UPDATE_MODE` / `SAVE_MODE`.
pub fn encode_update_mode(mode_idx: u32, mode: &Mode, v: u32) -> Vec<u8> {
    let body = encode_mode(mode, v);
    let size = 4 + 4 + body.len();
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(&(size as u32).to_le_bytes());
    out.extend_from_slice(&mode_idx.to_le_bytes());
    out.extend_from_slice(&body);
    out
}

/// Parse a `PROFILE_LIST` reply.
pub fn parse_profile_list(payload: &[u8]) -> Result<Vec<String>> {
    let mut r = Reader::new(payload);
    let _size = r.u32()?;
    let n = r.u16()? as usize;
    (0..n).map(|_| r.string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(out: &mut Vec<u8>, v: &str) {
        put_string(out, v);
    }

    /// Build a protocol-4 description the way the server would.
    fn sample(v: u32) -> Vec<u8> {
        let mut d = Vec::new();
        d.extend_from_slice(&0u32.to_le_bytes()); // data_size placeholder
        d.extend_from_slice(&0u32.to_le_bytes()); // type motherboard
        s(&mut d, "ASUS ROG STRIX");
        if v >= 1 {
            s(&mut d, "ASUS");
        }
        s(&mut d, "ASUS Aura USB");
        s(&mut d, "1.0");
        s(&mut d, "");
        s(&mut d, "HID: /dev/hidraw3");
        d.extend_from_slice(&2u16.to_le_bytes()); // modes
        d.extend_from_slice(&1i32.to_le_bytes()); // active
        let mut m = Mode { name: "Direct".into(), value: 0, flags: mode_flags::HAS_PER_LED_COLOR, color_mode: color_mode::PER_LED, ..Default::default() };
        d.extend_from_slice(&encode_mode(&m, v));
        m = Mode { name: "Breathing".into(), value: 2, flags: mode_flags::HAS_SPEED | mode_flags::HAS_MODE_SPECIFIC_COLOR, speed_min: 1, speed_max: 5, speed: 3, colors_min: 1, colors_max: 1, color_mode: color_mode::MODE_SPECIFIC, colors: vec![Color::new(255, 0, 0)], ..Default::default() };
        d.extend_from_slice(&encode_mode(&m, v));
        d.extend_from_slice(&2u16.to_le_bytes()); // zones
        s(&mut d, "Aura Mainboard");
        d.extend_from_slice(&1u32.to_le_bytes());
        d.extend_from_slice(&2u32.to_le_bytes());
        d.extend_from_slice(&2u32.to_le_bytes());
        d.extend_from_slice(&2u32.to_le_bytes());
        d.extend_from_slice(&0u16.to_le_bytes()); // no matrix
        if v >= 4 {
            d.extend_from_slice(&0u16.to_le_bytes()); // segments
        }
        s(&mut d, "Aura Addressable 1");
        d.extend_from_slice(&1u32.to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes());
        d.extend_from_slice(&120u32.to_le_bytes());
        d.extend_from_slice(&30u32.to_le_bytes());
        d.extend_from_slice(&(8u16 + 4).to_le_bytes()); // matrix 1x1
        d.extend_from_slice(&1u32.to_le_bytes());
        d.extend_from_slice(&1u32.to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes());
        if v >= 4 {
            d.extend_from_slice(&1u16.to_le_bytes());
            s(&mut d, "Fan 1");
            d.extend_from_slice(&1u32.to_le_bytes());
            d.extend_from_slice(&0u32.to_le_bytes());
            d.extend_from_slice(&10u32.to_le_bytes());
        }
        d.extend_from_slice(&32u16.to_le_bytes()); // leds
        for i in 0..32 {
            s(&mut d, &format!("LED {i}"));
            d.extend_from_slice(&0u32.to_le_bytes());
        }
        d.extend_from_slice(&32u16.to_le_bytes()); // colors
        for i in 0..32u32 {
            d.extend_from_slice(&(i * 0x010101).to_le_bytes());
        }
        let len = d.len() as u32;
        d[..4].copy_from_slice(&len.to_le_bytes());
        d
    }

    #[test]
    fn parses_v4_description() {
        let c = parse_controller(&sample(4), 4, 7).unwrap();
        assert_eq!(c.index, 7);
        assert_eq!(c.name, "ASUS ROG STRIX");
        assert_eq!(c.vendor, "ASUS");
        assert_eq!(c.modes.len(), 2);
        assert_eq!(c.modes[1].speed, 3);
        assert_eq!(c.modes[1].colors[0], Color::new(255, 0, 0));
        assert_eq!(c.zones.len(), 2);
        assert_eq!(c.zones[1].leds_count, 30);
        assert_eq!(c.zones[1].start_idx, 2);
        assert_eq!(c.zones[1].segments[0].name, "Fan 1");
        assert_eq!(c.leds.len(), 32);
        assert_eq!(c.colors[3], Color::new(3, 3, 3));
        assert_eq!(c.direct_mode(), Some(0));
    }

    #[test]
    fn parses_v1_description() {
        let c = parse_controller(&sample(1), 1, 0).unwrap();
        assert_eq!(c.zones[1].segments.len(), 0);
        assert_eq!(c.modes[0].brightness_max, 0);
    }

    #[test]
    fn truncated_is_an_error() {
        let d = sample(4);
        assert!(parse_controller(&d[..d.len() - 10], 4, 0).is_err());
    }

    #[test]
    fn colours_pack_as_bgr() {
        let c = Color::new(0x11, 0x22, 0x33);
        assert_eq!(c.to_u32(), 0x0033_2211);
        assert_eq!(Color::from_u32(0x0033_2211), c);
        assert_eq!(Color::hsv(0.0, 1.0, 1.0), Color::new(255, 0, 0));
        assert_eq!(Color::hsv(120.0, 1.0, 1.0), Color::new(0, 255, 0));
    }

    #[test]
    fn update_leds_layout() {
        let p = encode_update_leds(&[Color::new(1, 2, 3)]);
        assert_eq!(p, vec![10, 0, 0, 0, 1, 0, 1, 2, 3, 0]);
        let z = encode_update_zone_leds(2, &[Color::new(1, 2, 3)]);
        assert_eq!(z[..4], [14, 0, 0, 0]);
        assert_eq!(z[4..8], [2, 0, 0, 0]);
    }

    #[test]
    fn header_roundtrip() {
        let h = header(3, id::UPDATE_LEDS, 10);
        assert_eq!(parse_header(&h).unwrap(), (3, 1050, 10));
        assert!(parse_header(b"NOPE000000000000").is_err());
    }
}
