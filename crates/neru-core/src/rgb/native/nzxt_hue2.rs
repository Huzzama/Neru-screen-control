//! NZXT Hue 2 family: Hue 2 / Hue 2 Ambient, Smart Device V2, RGB & Fan
//! Controller (all revisions), RGB Controller, Kraken X3/Z3 pump ring + logo,
//! Kraken 2024 Elite, Hue 2 motherboard controllers.
//!
//! Ported from OpenRGB's `NZXTHue2Controller` (Adam Honse, GPL-2.0-or-later).
//! The controller reports what is plugged into each channel (fans, strips,
//! combs), so zone sizes come from the device; 64-byte reports whose first
//! byte is the command (no report ID).

use tracing::{debug, info};

use super::NativeDevice;
use crate::hidraw::{self, HidRaw};
use crate::rgb::protocol::{color_mode, mode_flags, Color, Controller, Mode, Zone};
use crate::{Error, Result};

pub const VID: u16 = 0x1E71;
/// (pid, name, rgb channels, fan channels, 2023 effect packets)
pub const DEVICES: &[(u16, &str, u32, u32, bool)] = &[
    (0x2001, "NZXT Hue 2", 4, 0, false),
    (0x2002, "NZXT Hue 2 Ambient", 2, 0, false),
    (0x2005, "NZXT Hue 2 Motherboard", 2, 3, false),
    (0x200B, "NZXT Hue 2 Motherboard", 2, 3, false),
    (0x2006, "NZXT Smart Device V2", 2, 3, false),
    (0x200D, "NZXT Smart Device V2", 2, 3, false),
    (0x200F, "NZXT Smart Device V2", 2, 3, false),
    (0x2007, "NZXT Kraken X3 Series", 3, 0, false),
    (0x2014, "NZXT Kraken X3 Series RGB", 3, 0, false),
    (0x3012, "NZXT Kraken 2024 Elite Series RGB", 2, 2, true),
    (0x2009, "NZXT RGB & Fan Controller", 2, 3, false),
    (0x2010, "NZXT RGB & Fan Controller", 2, 3, false),
    (0x200E, "NZXT RGB & Fan Controller", 2, 3, false),
    (0x2011, "NZXT RGB & Fan Controller", 6, 3, false),
    (0x2019, "NZXT RGB & Fan Controller", 6, 3, false),
    (0x2020, "NZXT RGB & Fan Controller", 6, 3, false),
    (0x201F, "NZXT RGB & Fan Controller", 6, 3, false),
    (0x2022, "NZXT RGB & Fan Controller 2024", 6, 3, false),
    (0x201B, "NZXT B650E Motherboard", 6, 3, false),
    (0x2012, "NZXT RGB Controller", 3, 0, false),
    (0x2021, "NZXT RGB Controller", 3, 0, false),
];
const KRAKEN_X3_PIDS: &[u16] = &[0x2007, 0x2014];

const SPEED_SLOWEST: u32 = 0;
const SPEED_NORMAL: u32 = 2;
const SPEED_FASTEST: u32 = 4;
const MODE_DIRECT: i32 = 0xFFFF;
pub const MAX_LEDS_PER_CHANNEL: u32 = 40;

/// (name, value, flags, colour mode, colours min, colours max)
const MODES: &[(&str, i32, u32, u32, u32, u32)] = &[
    ("Direct", MODE_DIRECT, mode_flags::HAS_PER_LED_COLOR, color_mode::PER_LED, 0, 0),
    ("Static", 0x00, mode_flags::HAS_MODE_SPECIFIC_COLOR, color_mode::MODE_SPECIFIC, 1, 1),
    ("Fading", 0x01, mode_flags::HAS_SPEED | mode_flags::HAS_MODE_SPECIFIC_COLOR, color_mode::MODE_SPECIFIC, 1, 8),
    ("Rainbow Wave", 0x02, mode_flags::HAS_SPEED | mode_flags::HAS_DIRECTION_LR, color_mode::NONE, 0, 0),
    ("Marquee", 0x03, mode_flags::HAS_SPEED | mode_flags::HAS_DIRECTION_LR | mode_flags::HAS_MODE_SPECIFIC_COLOR, color_mode::MODE_SPECIFIC, 1, 1),
    ("Cover Marquee", 0x04, mode_flags::HAS_SPEED | mode_flags::HAS_DIRECTION_LR | mode_flags::HAS_MODE_SPECIFIC_COLOR, color_mode::MODE_SPECIFIC, 1, 8),
    ("Alternating", 0x05, mode_flags::HAS_SPEED | mode_flags::HAS_DIRECTION_LR | mode_flags::HAS_MODE_SPECIFIC_COLOR, color_mode::MODE_SPECIFIC, 1, 2),
    ("Pulsing", 0x06, mode_flags::HAS_SPEED | mode_flags::HAS_MODE_SPECIFIC_COLOR, color_mode::MODE_SPECIFIC, 1, 8),
    ("Breathing", 0x07, mode_flags::HAS_SPEED | mode_flags::HAS_MODE_SPECIFIC_COLOR, color_mode::MODE_SPECIFIC, 1, 8),
    ("Candle", 0x08, mode_flags::HAS_SPEED | mode_flags::HAS_MODE_SPECIFIC_COLOR, color_mode::MODE_SPECIFIC, 1, 8),
    ("Starry Night", 0x09, mode_flags::HAS_SPEED | mode_flags::HAS_MODE_SPECIFIC_COLOR, color_mode::MODE_SPECIFIC, 1, 1),
    ("Rainbow Flow", 0x0B, mode_flags::HAS_SPEED | mode_flags::HAS_DIRECTION_LR, color_mode::NONE, 0, 0),
    ("Super Rainbow", 0x0C, mode_flags::HAS_SPEED | mode_flags::HAS_DIRECTION_LR, color_mode::NONE, 0, 0),
    ("Rainbow Pulse", 0x0D, mode_flags::HAS_SPEED | mode_flags::HAS_DIRECTION_LR, color_mode::NONE, 0, 0),
];

fn modes() -> Vec<Mode> {
    MODES
        .iter()
        .map(|&(name, value, flags, cm, cmin, cmax)| Mode {
            name: name.into(),
            value,
            flags,
            speed_min: SPEED_SLOWEST,
            speed_max: SPEED_FASTEST,
            speed: SPEED_NORMAL,
            colors_min: cmin,
            colors_max: cmax,
            color_mode: cm,
            colors: vec![Color::new(0xff, 0xc2, 0x0e); cmin as usize],
            ..Default::default()
        })
        .collect()
}

/// LEDs per accessory id reported in the device list.
fn accessory(id: u8) -> (u32, &'static str) {
    match id {
        0x01 => (10, "Hue 1 strip"),
        0x02 => (8, "Aer 1 fan"),
        0x04 => (10, "Hue 2 strip (10 LEDs)"),
        0x05 => (8, "Hue 2 strip (8 LEDs)"),
        0x06 => (6, "Hue 2 strip (6 LEDs)"),
        0x08 => (14, "Hue 2 Cable Comb"),
        0x09 => (15, "Hue 2 Underglow 300 mm"),
        0x0A => (10, "Hue 2 Underglow 200 mm"),
        0x0B => (8, "Aer 2 fan 120 mm"),
        0x0C => (8, "Aer 2 fan 140 mm"),
        0x10 => (8, "Kraken X3 ring"),
        0x11 => (1, "Kraken X3 logo"),
        0x13 => (18, "F120 RGB fan"),
        0x14 => (18, "F140 RGB fan"),
        0x15 => (20, "F120 RGB Duo fan"),
        0x16 => (20, "F140 RGB Duo fan"),
        0x17 => (8, "F120 RGB Core fan"),
        0x18 => (8, "F140 RGB Core fan"),
        0x19 => (8, "F120 RGB Core fan (case)"),
        0x1C => (16, "F280 RGB Core fan (case)"),
        0x1D => (24, "F360 RGB Core fan (case)"),
        0x1E => (24, "Kraken Elite ring"),
        0x1F => (24, "F420 RGB"),
        _ => (0, ""),
    }
}

pub fn find() -> Vec<(hidraw::HidNode, &'static (u16, &'static str, u32, u32, bool))> {
    let mut out = Vec::new();
    for d in DEVICES {
        for node in hidraw::find(VID, d.0) {
            out.push((node, d));
        }
    }
    out
}

pub struct NzxtHue2 {
    dev: HidRaw,
    name: String,
    pid: u16,
    firmware: String,
    serial: String,
    location: String,
    channels: u32,
    effects_2023: bool,
    /// LEDs per channel (from the device list, user-adjustable).
    leds: Vec<u32>,
    accessories: Vec<Vec<String>>,
    modes: Vec<Mode>,
    active_mode: usize,
    colors: Vec<Vec<Color>>,
    /// Per channel: the colours in `colors` were sent as a direct frame (so an
    /// identical frame can be skipped). Cleared by every mode change.
    direct_sent: Vec<bool>,
}

impl NzxtHue2 {
    pub fn open(node: &hidraw::HidNode, d: &(u16, &str, u32, u32, bool)) -> Result<Self> {
        let (pid, name, channels, _fans, effects_2023) = *d;
        let mut dev = HidRaw::open(&node.path)?;
        dev.drain()?;
        // Firmware: 10 01 → 11 01 … [0x11..0x13]
        let mut firmware = "unknown".to_string();
        let mut pkt = [0u8; 64];
        pkt[0] = 0x10;
        pkt[1] = 0x01;
        dev.write_raw(&pkt)?;
        let mut buf = [0u8; 64];
        for _ in 0..10 {
            match dev.read_report(&mut buf, 500)? {
                Some(n) if n == 64 && buf[0] == 0x11 && buf[1] == 0x01 => {
                    firmware = format!("{}.{}.{}", buf[0x11], buf[0x12], buf[0x13]);
                    break;
                }
                Some(_) => continue,
                None => break,
            }
        }
        // Device list: 20 03 → 21 03, six accessory ids per channel from 0x0F.
        let mut pkt = [0u8; 64];
        pkt[0] = 0x20;
        pkt[1] = 0x03;
        dev.write_raw(&pkt)?;
        let mut list = [0u8; 64];
        let mut got = false;
        for _ in 0..10 {
            match dev.read_report(&mut list, 1000)? {
                Some(n) if n == 64 && list[0] == 0x21 && list[1] == 0x03 => {
                    got = true;
                    break;
                }
                Some(_) => continue,
                None => break,
            }
        }
        if !got {
            return Err(Error::Protocol("no answer to the device-list request".into()));
        }
        let mut leds = Vec::new();
        let mut accessories = Vec::new();
        for ch in 0..channels as usize {
            let start = 0x0F + 6 * ch;
            let mut n = 0;
            let mut names = Vec::new();
            for i in 0..6 {
                let (count, what) = accessory(list[start + i]);
                if count > 0 {
                    names.push(what.to_string());
                }
                n += count;
            }
            leds.push(n);
            accessories.push(names);
        }
        let serial = hidraw::serial_of(&node.path);
        info!(name, %firmware, ?leds, node = %node.path.display(), "NZXT Hue 2 device opened");
        Ok(Self {
            dev,
            name: name.to_string(),
            pid,
            firmware,
            serial,
            location: format!("HID: {}", node.path.display()),
            channels,
            effects_2023,
            colors: leds.iter().map(|&n| vec![Color::BLACK; n as usize]).collect(),
            direct_sent: vec![false; leds.len()],
            leds,
            accessories,
            modes: modes(),
            active_mode: 0,
        })
    }

    fn zone_name(&self, i: usize) -> String {
        if KRAKEN_X3_PIDS.contains(&self.pid) {
            match i {
                0 => "RGB Header".into(),
                1 => "Ring".into(),
                2 => "Logo".into(),
                _ => format!("Channel {}", i + 1),
            }
        } else {
            format!("Hue 2 Channel {}", i + 1)
        }
    }

    fn send_direct(&mut self, channel: usize) -> Result<()> {
        let colors = self.colors[channel].clone();
        let mut data: Vec<u8> = Vec::with_capacity(colors.len() * 3);
        for c in &colors {
            data.extend_from_slice(&[c.g, c.r, c.b]);
        }
        let first = colors.len().min(20);
        let mut pkt = [0u8; 64];
        pkt[0] = 0x22;
        pkt[1] = 0x10;
        pkt[2] = 1 << channel;
        pkt[4..4 + first * 3].copy_from_slice(&data[..first * 3]);
        self.dev.write_raw(&pkt)?;
        if colors.len() > 20 {
            let rest = (colors.len() - 20).min(20);
            let mut pkt = [0u8; 64];
            pkt[0] = 0x22;
            pkt[1] = 0x11;
            pkt[2] = 1 << channel;
            pkt[4..4 + rest * 3].copy_from_slice(&data[60..60 + rest * 3]);
            self.dev.write_raw(&pkt)?;
        }
        // Apply.
        let mut pkt = [0u8; 64];
        pkt[0] = 0x22;
        pkt[1] = 0xA0;
        pkt[2] = 1 << channel;
        pkt[4] = 0x01;
        pkt[7] = 0x28;
        pkt[0x0A] = 0x80;
        pkt[0x0C] = 0x32;
        pkt[0x0F] = 0x01;
        self.dev.write_raw(&pkt)
    }

    fn send_effect(&mut self, channel: usize, m: &Mode) -> Result<()> {
        let mut data = [0u8; 24];
        let n = m.colors.len().min(8);
        for (i, c) in m.colors.iter().take(8).enumerate() {
            data[3 * i] = c.g;
            data[3 * i + 1] = c.r;
            data[3 * i + 2] = c.b;
        }
        let direction = m.direction == 0; // OpenRGB: LEFT → direction flag set
        let speed = m.speed.min(4) as u8;
        let mut pkt = [0u8; 64];
        if self.effects_2023 {
            let mut speed_data = [0x32u8, 0x00];
            let mut modifier = 0u8;
            let table: Option<&[[u8; 2]; 5]> = match m.value {
                0x01 => {
                    modifier = 0x08;
                    Some(&[[0x50, 0], [0x3C, 0], [0x28, 0], [0x14, 0], [0x0A, 0]])
                }
                0x02 | 0x0B | 0x0C | 0x0D => Some(&[[0x5E, 1], [0x2C, 1], [0xFA, 0], [0x96, 0], [0x50, 0]]),
                0x05 => Some(&[[0x40, 6], [0x14, 5], [0xE8, 3], [0x20, 3], [0x58, 2]]),
                0x06 | 0x09 => {
                    if m.value == 0x06 {
                        modifier = 0x08;
                    }
                    Some(&[[0x19, 0], [0x14, 0], [0x0F, 0], [0x07, 0], [0x04, 0]])
                }
                0x07 => {
                    modifier = 0x08;
                    Some(&[[0x28, 0], [0x1E, 0], [0x14, 0], [0x0A, 0], [0x04, 0]])
                }
                _ => None,
            };
            if let Some(t) = table {
                speed_data = t[speed as usize];
            }
            pkt[0] = 0x2A;
            pkt[1] = 0x04;
            pkt[2] = 1 << channel;
            pkt[3] = pkt[2];
            pkt[4] = m.value as u8;
            pkt[5] = speed_data[0];
            pkt[6] = speed_data[1];
            pkt[7..7 + n * 3].copy_from_slice(&data[..n * 3]);
            pkt[0x37] = if direction { 0x02 } else { 0x00 };
            pkt[0x38] = n as u8;
            pkt[0x39] = modifier;
            pkt[0x3A] = 0x08;
            pkt[0x3B] = 0x03;
        } else {
            pkt[0] = 0x28;
            pkt[1] = 0x03;
            pkt[2] = 1 << channel;
            pkt[3] = 0x28;
            pkt[4] = m.value as u8;
            pkt[5] = speed;
            pkt[6] = 1;
            pkt[7] = direction as u8;
            pkt[8] = n as u8;
            pkt[0x0A..0x0A + n * 3].copy_from_slice(&data[..n * 3]);
        }
        debug!(channel, mode = %m.name, "NZXT effect");
        self.dev.write_raw(&pkt)
    }
}

impl NativeDevice for NzxtHue2 {
    fn location(&self) -> String {
        self.location.clone()
    }

    fn describe(&self) -> Controller {
        let mut zones = Vec::new();
        let mut leds = Vec::new();
        let mut colors = Vec::new();
        let mut start = 0u32;
        for (i, &n) in self.leds.iter().enumerate() {
            let name = self.zone_name(i);
            for l in 0..n {
                leds.push(format!("{name}, LED {}", l + 1));
            }
            colors.extend(self.colors[i].iter().copied());
            let segments = self.accessories[i]
                .iter()
                .map(|a| crate::rgb::protocol::Segment { name: a.clone(), kind: 1, start_idx: 0, leds_count: 0 })
                .collect();
            zones.push(Zone { name, kind: 1, leds_min: 0, leds_max: MAX_LEDS_PER_CHANNEL, leds_count: n, start_idx: start, segments, ..Default::default() });
            start += n;
        }
        Controller {
            index: 0,
            kind: if KRAKEN_X3_PIDS.contains(&self.pid) || self.pid == 0x3012 { 3 } else { 4 },
            name: self.name.clone(),
            vendor: "NZXT".into(),
            description: "NZXT Hue 2 controller (native)".into(),
            version: self.firmware.clone(),
            serial: self.serial.clone(),
            location: self.location.clone(),
            active_mode: self.active_mode as i32,
            modes: self.modes.clone(),
            zones,
            leds,
            colors,
            ..Default::default()
        }
    }

    fn resize_zone(&mut self, zone: u32, leds: u32) -> Result<()> {
        let z = zone as usize;
        if z >= self.channels as usize {
            return Err(Error::InvalidValue(format!("no channel {zone}")));
        }
        let n = leds.min(MAX_LEDS_PER_CHANNEL);
        self.leds[z] = n;
        self.colors[z].resize(n as usize, Color::BLACK);
        self.direct_sent[z] = false;
        Ok(())
    }

    fn set_direct(&mut self) -> Result<()> {
        if self.active_mode != 0 {
            // Leaving a hardware effect: every channel must be sent again,
            // even when its colours have not changed.
            self.direct_sent.iter_mut().for_each(|d| *d = false);
        }
        self.active_mode = 0;
        Ok(())
    }

    fn zone_leds(&mut self, zone: u32, colors: &[Color]) -> Result<()> {
        let z = zone as usize;
        if z >= self.channels as usize {
            return Err(Error::InvalidValue(format!("no channel {zone}")));
        }
        let n = self.leds[z] as usize;
        if n == 0 {
            return Ok(());
        }
        let mut new: Vec<Color> = colors.iter().copied().take(n).collect();
        new.resize(n, Color::BLACK);
        if self.colors[z] == new && self.active_mode == 0 && self.direct_sent[z] {
            return Ok(());
        }
        self.colors[z] = new;
        if self.active_mode == 0 {
            self.send_direct(z)?;
            self.direct_sent[z] = true;
            Ok(())
        } else {
            self.direct_sent[z] = false;
            let mut m = self.modes[self.active_mode].clone();
            if m.has(mode_flags::HAS_MODE_SPECIFIC_COLOR) {
                m.colors = vec![self.colors[z][0]];
            }
            self.send_effect(z, &m)
        }
    }

    fn set_mode(&mut self, mode_idx: usize, mode: &Mode) -> Result<()> {
        let m = self.modes.get_mut(mode_idx).ok_or_else(|| Error::InvalidValue(format!("no mode {mode_idx}")))?;
        m.speed = mode.speed.min(4);
        m.direction = mode.direction;
        if m.colors_max > 0 && !mode.colors.is_empty() {
            let n = mode.colors.len().clamp(m.colors_min.max(1) as usize, m.colors_max as usize);
            m.colors = mode.colors.iter().copied().cycle().take(n).collect();
        }
        let m = m.clone();
        self.active_mode = mode_idx;
        self.direct_sent.iter_mut().for_each(|d| *d = false);
        for z in 0..self.channels as usize {
            if self.leds[z] == 0 {
                continue;
            }
            if m.value == MODE_DIRECT {
                self.send_direct(z)?;
            } else {
                self.send_effect(z, &m)?;
            }
        }
        Ok(())
    }

    fn save_mode(&mut self, mode_idx: usize, mode: &Mode) -> Result<()> {
        self.set_mode(mode_idx, mode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accessories_and_modes() {
        assert_eq!(accessory(0x0B).0, 8);
        assert_eq!(accessory(0x1D).0, 24);
        assert_eq!(accessory(0x00).0, 0);
        let m = modes();
        assert_eq!(m.len(), 14);
        assert_eq!(m.iter().find(|x| x.name == "Breathing").unwrap().value, 0x07);
    }
}
