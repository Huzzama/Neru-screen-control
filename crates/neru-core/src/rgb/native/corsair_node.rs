//! Corsair Lighting Node Pro / Core, Commander Pro (lighting channels),
//! LS100, LT100, Obsidian 1000D, SPEC-OMEGA RGB — the "lighting node"
//! family of USB HID controllers that drive Corsair HD/LL/ML/SP/QL fans and
//! LED strips.
//!
//! Ported from OpenRGB's `CorsairLightingNodeController` (Adam Honse,
//! GPL-2.0-or-later). 64-byte reports; the device answers every command.
//! A channel in software mode falls back to its own rainbow after ~20 s
//! without a commit, so [`NativeDevice::keepalive`] sends one every 5 s.

use std::time::{Duration, Instant};

use tracing::{debug, info};

use super::NativeDevice;
use crate::hidraw::{self, HidRaw};
use crate::rgb::protocol::{color_mode, mode_flags, Color, Controller, Mode, Zone};
use crate::{Error, Result};

pub const VID: u16 = 0x1B1C;
/// (pid, name, lighting channels)
pub const DEVICES: &[(u16, &str, u32)] = &[
    (0x0C1A, "Corsair Lighting Node Core", 1),
    (0x0C0B, "Corsair Lighting Node Pro", 2),
    (0x0C10, "Corsair Commander Pro", 2),
    (0x0C1E, "Corsair LS100 Lighting Kit", 1),
    (0x1D00, "Corsair 1000D Obsidian", 2),
    (0x1D04, "Corsair SPEC-OMEGA RGB", 2),
    (0x0C23, "Corsair LT100", 2),
];

const PKT_FIRMWARE: u8 = 0x02;
const PKT_DIRECT: u8 = 0x32;
const PKT_COMMIT: u8 = 0x33;
const PKT_BEGIN: u8 = 0x34;
const PKT_EFFECT_CONFIG: u8 = 0x35;
const PKT_RESET: u8 = 0x37;
const PKT_PORT_STATE: u8 = 0x38;
const PKT_BRIGHTNESS: u8 = 0x39;

const PORT_HARDWARE: u8 = 0x01;
const PORT_SOFTWARE: u8 = 0x02;
pub const MAX_LEDS_PER_CHANNEL: u32 = 204;
const READ_TIMEOUT_MS: i32 = 15;

const MODE_DIRECT: i32 = 0xFFFF;
const SPEED_SLOW: u32 = 2;
const SPEED_MEDIUM: u32 = 1;
const SPEED_FAST: u32 = 0;

/// (name, value, flags, colour mode, colours)
const MODES: &[(&str, i32, u32, u32, u32)] = &[
    ("Direct", MODE_DIRECT, mode_flags::HAS_PER_LED_COLOR, color_mode::PER_LED, 0),
    ("Rainbow Wave", 0x00, mode_flags::HAS_SPEED | mode_flags::HAS_DIRECTION_LR | mode_flags::HAS_BRIGHTNESS | mode_flags::AUTOMATIC_SAVE, color_mode::NONE, 0),
    ("Color Shift", 0x01, mode_flags::HAS_SPEED | mode_flags::HAS_MODE_SPECIFIC_COLOR | mode_flags::HAS_RANDOM_COLOR | mode_flags::HAS_BRIGHTNESS | mode_flags::AUTOMATIC_SAVE, color_mode::MODE_SPECIFIC, 2),
    ("Color Pulse", 0x02, mode_flags::HAS_SPEED | mode_flags::HAS_MODE_SPECIFIC_COLOR | mode_flags::HAS_RANDOM_COLOR | mode_flags::HAS_BRIGHTNESS | mode_flags::AUTOMATIC_SAVE, color_mode::MODE_SPECIFIC, 2),
    ("Color Wave", 0x03, mode_flags::HAS_SPEED | mode_flags::HAS_DIRECTION_LR | mode_flags::HAS_MODE_SPECIFIC_COLOR | mode_flags::HAS_RANDOM_COLOR | mode_flags::HAS_BRIGHTNESS | mode_flags::AUTOMATIC_SAVE, color_mode::MODE_SPECIFIC, 2),
    ("Static", 0x04, mode_flags::HAS_MODE_SPECIFIC_COLOR | mode_flags::HAS_BRIGHTNESS | mode_flags::AUTOMATIC_SAVE, color_mode::MODE_SPECIFIC, 1),
    ("Temperature", 0x05, mode_flags::HAS_MODE_SPECIFIC_COLOR | mode_flags::HAS_BRIGHTNESS | mode_flags::AUTOMATIC_SAVE, color_mode::MODE_SPECIFIC, 3),
    ("Visor", 0x06, mode_flags::HAS_SPEED | mode_flags::HAS_MODE_SPECIFIC_COLOR | mode_flags::HAS_RANDOM_COLOR | mode_flags::HAS_BRIGHTNESS | mode_flags::AUTOMATIC_SAVE, color_mode::MODE_SPECIFIC, 2),
    ("Marquee", 0x07, mode_flags::HAS_SPEED | mode_flags::HAS_DIRECTION_LR | mode_flags::HAS_MODE_SPECIFIC_COLOR | mode_flags::HAS_RANDOM_COLOR | mode_flags::HAS_BRIGHTNESS | mode_flags::AUTOMATIC_SAVE, color_mode::MODE_SPECIFIC, 1),
    ("Blink", 0x08, mode_flags::HAS_SPEED | mode_flags::HAS_MODE_SPECIFIC_COLOR | mode_flags::HAS_RANDOM_COLOR | mode_flags::HAS_BRIGHTNESS | mode_flags::AUTOMATIC_SAVE, color_mode::MODE_SPECIFIC, 2),
    ("Sequential", 0x09, mode_flags::HAS_SPEED | mode_flags::HAS_DIRECTION_LR | mode_flags::HAS_MODE_SPECIFIC_COLOR | mode_flags::HAS_RANDOM_COLOR | mode_flags::HAS_BRIGHTNESS | mode_flags::AUTOMATIC_SAVE, color_mode::MODE_SPECIFIC, 1),
    ("Rainbow", 0x0A, mode_flags::HAS_SPEED | mode_flags::HAS_BRIGHTNESS | mode_flags::AUTOMATIC_SAVE, color_mode::NONE, 0),
];

fn modes() -> Vec<Mode> {
    MODES
        .iter()
        .map(|&(name, value, flags, cm, ncol)| Mode {
            name: name.into(),
            value,
            flags,
            speed_min: SPEED_SLOW,
            speed_max: SPEED_FAST,
            speed: SPEED_MEDIUM,
            brightness_min: 0,
            brightness_max: 100,
            brightness: 100,
            colors_min: ncol,
            colors_max: ncol,
            direction: 1,
            color_mode: cm,
            colors: vec![Color::new(0xff, 0xc2, 0x0e); ncol as usize],
        })
        .collect()
}

/// Devices found right now: (node, name, channels).
pub fn find() -> Vec<(hidraw::HidNode, &'static str, u32)> {
    let mut out = Vec::new();
    for &(pid, name, channels) in DEVICES {
        for node in hidraw::find(VID, pid) {
            out.push((node, name, channels));
        }
    }
    out
}

pub struct CorsairNode {
    dev: HidRaw,
    name: String,
    firmware: String,
    serial: String,
    location: String,
    channels: u32,
    leds: Vec<u32>,
    modes: Vec<Mode>,
    active_mode: usize,
    colors: Vec<Vec<Color>>,
    software: Vec<bool>,
    last_commit: Instant,
}

impl CorsairNode {
    pub fn open(node: &hidraw::HidNode, name: &str, channels: u32) -> Result<Self> {
        let mut dev = HidRaw::open(&node.path)?;
        let mut pkt = [0u8; 64];
        pkt[0] = PKT_FIRMWARE;
        let reply = dev.exchange(&pkt, 200)?;
        let firmware = if reply.len() >= 4 { format!("{}.{}.{}", reply[1], reply[2], reply[3]) } else { "unknown".into() };
        let serial = hidraw::serial_of(&node.path);
        info!(name, %firmware, node = %node.path.display(), "Corsair lighting node opened");
        Ok(Self {
            dev,
            name: name.to_string(),
            firmware,
            serial,
            location: format!("HID: {}", node.path.display()),
            channels,
            leds: vec![0; channels as usize],
            modes: modes(),
            active_mode: 0,
            colors: vec![Vec::new(); channels as usize],
            software: vec![false; channels as usize],
            last_commit: Instant::now(),
        })
    }

    fn send(&mut self, pkt: &[u8; 64]) -> Result<()> {
        self.dev.exchange(pkt, READ_TIMEOUT_MS)?;
        Ok(())
    }

    fn commit(&mut self) -> Result<()> {
        let mut pkt = [0u8; 64];
        pkt[0] = PKT_COMMIT;
        pkt[1] = 0xFF;
        self.last_commit = Instant::now();
        self.send(&pkt)
    }

    fn port_state(&mut self, channel: u8, state: u8) -> Result<()> {
        let mut pkt = [0u8; 64];
        pkt[0] = PKT_PORT_STATE;
        pkt[1] = channel;
        pkt[2] = state;
        self.send(&pkt)
    }

    fn brightness(&mut self, channel: u8, b: u8) -> Result<()> {
        let mut pkt = [0u8; 64];
        pkt[0] = PKT_BRIGHTNESS;
        pkt[1] = channel;
        pkt[2] = b.min(100);
        self.send(&pkt)
    }

    /// One colour component of up to 50 LEDs.
    fn direct(&mut self, channel: u8, start: u8, data: &[u8], component: u8) -> Result<()> {
        let mut pkt = [0u8; 64];
        pkt[0] = PKT_DIRECT;
        pkt[1] = channel;
        pkt[2] = start;
        pkt[3] = data.len() as u8;
        pkt[4] = component;
        pkt[5..5 + data.len()].copy_from_slice(data);
        self.send(&pkt)
    }

    fn send_channel_leds(&mut self, channel: usize) -> Result<()> {
        let colors = self.colors[channel].clone();
        self.port_state(channel as u8, PORT_SOFTWARE)?;
        for (i, chunk) in colors.chunks(50).enumerate() {
            let start = (i * 50) as u8;
            let r: Vec<u8> = chunk.iter().map(|c| c.r).collect();
            let g: Vec<u8> = chunk.iter().map(|c| c.g).collect();
            let b: Vec<u8> = chunk.iter().map(|c| c.b).collect();
            self.direct(channel as u8, start, &r, 0)?;
            self.direct(channel as u8, start, &g, 1)?;
            self.direct(channel as u8, start, &b, 2)?;
        }
        self.commit()
    }

    fn send_channel_effect(&mut self, channel: usize, m: &Mode) -> Result<()> {
        let n = self.leds[channel] as u8;
        let random = m.color_mode == color_mode::RANDOM;
        let mut c = [0u8; 9];
        if m.color_mode == color_mode::MODE_SPECIFIC {
            for (i, col) in m.colors.iter().take(3).enumerate() {
                c[3 * i] = col.r;
                c[3 * i + 1] = col.g;
                c[3 * i + 2] = col.b;
            }
        }
        let mut pkt = [0u8; 64];
        pkt[0] = PKT_RESET;
        pkt[1] = channel as u8;
        self.send(&pkt)?;
        let mut pkt = [0u8; 64];
        pkt[0] = PKT_BEGIN;
        pkt[1] = channel as u8;
        self.send(&pkt)?;
        self.port_state(channel as u8, PORT_HARDWARE)?;
        let mut pkt = [0u8; 64];
        pkt[0] = PKT_EFFECT_CONFIG;
        pkt[1] = channel as u8;
        pkt[2] = 0;
        pkt[3] = n; // led type field carries the LED count, as the vendor tool does
        pkt[4] = m.value as u8;
        pkt[5] = m.speed.min(2) as u8;
        pkt[6] = if m.direction == 1 { 1 } else { 0 };
        pkt[7] = random as u8;
        pkt[8] = 0;
        pkt[9..18].copy_from_slice(&c);
        self.send(&pkt)?;
        self.commit()
    }
}

impl NativeDevice for CorsairNode {
    fn location(&self) -> String {
        self.location.clone()
    }

    fn describe(&self) -> Controller {
        let mut zones = Vec::new();
        let mut leds = Vec::new();
        let mut colors = Vec::new();
        let mut start = 0u32;
        for (i, &n) in self.leds.iter().enumerate() {
            let name = format!("Corsair RGB Header {}", i + 1);
            for l in 0..n {
                leds.push(format!("{name}, LED {}", l + 1));
            }
            colors.extend(self.colors[i].iter().copied());
            zones.push(Zone { name, kind: 1, leds_min: 0, leds_max: MAX_LEDS_PER_CHANNEL, leds_count: n, start_idx: start, ..Default::default() });
            start += n;
        }
        Controller {
            index: 0,
            kind: 4,
            name: self.name.clone(),
            vendor: "Corsair".into(),
            description: "Corsair Lighting Node (native)".into(),
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
        Ok(())
    }

    fn set_direct(&mut self) -> Result<()> {
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
        if self.active_mode == 0 && self.software[z] && self.colors[z] == new {
            return Ok(());
        }
        self.colors[z] = new;
        if self.active_mode != 0 {
            // A per-channel colour while a hardware mode runs: restate the mode.
            let m = self.modes[self.active_mode].clone();
            return self.send_channel_effect(z, &m);
        }
        self.software[z] = true;
        self.send_channel_leds(z)
    }

    fn set_mode(&mut self, mode_idx: usize, mode: &Mode) -> Result<()> {
        let m = self.modes.get_mut(mode_idx).ok_or_else(|| Error::InvalidValue(format!("no mode {mode_idx}")))?;
        m.speed = mode.speed.min(2);
        m.brightness = mode.brightness.min(100);
        m.direction = mode.direction;
        if m.has(mode_flags::HAS_RANDOM_COLOR) && mode.color_mode == color_mode::RANDOM {
            m.color_mode = color_mode::RANDOM;
        } else if m.has(mode_flags::HAS_MODE_SPECIFIC_COLOR) {
            m.color_mode = color_mode::MODE_SPECIFIC;
        }
        if !mode.colors.is_empty() && m.colors_max > 0 {
            let n = m.colors_max as usize;
            m.colors = mode.colors.iter().copied().cycle().take(n).collect();
        }
        let m = m.clone();
        self.active_mode = mode_idx;
        if m.value == MODE_DIRECT {
            for z in 0..self.channels as usize {
                if self.leds[z] > 0 {
                    self.software[z] = true;
                    self.send_channel_leds(z)?;
                }
            }
        } else {
            for z in 0..self.channels as usize {
                self.software[z] = false;
                if self.leds[z] > 0 {
                    self.send_channel_effect(z, &m)?;
                }
            }
        }
        let b = if m.has(mode_flags::HAS_BRIGHTNESS) { m.brightness as u8 } else { 100 };
        for z in 0..self.channels as u8 {
            self.brightness(z, b)?;
        }
        Ok(())
    }

    fn save_mode(&mut self, mode_idx: usize, mode: &Mode) -> Result<()> {
        // Hardware modes persist on their own (automatic save).
        self.set_mode(mode_idx, mode)
    }

    fn keepalive(&mut self) -> Result<()> {
        if self.last_commit.elapsed() >= Duration::from_secs(5) {
            debug!(name = %self.name, "keepalive commit");
            self.commit()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_match_openrgb_values() {
        let m = modes();
        assert_eq!(m[0].name, "Direct");
        assert_eq!(m.iter().find(|x| x.name == "Static").unwrap().value, 0x04);
        assert_eq!(m.iter().find(|x| x.name == "Rainbow").unwrap().value, 0x0A);
        assert_eq!(m.iter().find(|x| x.name == "Temperature").unwrap().colors.len(), 3);
    }
}
