//! ASRock Polychrome USB motherboards (B550/X570/Z490 and newer boards whose
//! RGB controller is a USB HID device, `26CE:01A2`) and the DeskMini
//! addressable LED strip (`26CE:01A6`).
//!
//! Ported from OpenRGB's `ASRockPolychromeUSBController` (GPL-2.0-or-later).
//! The board exposes up to eight zones (two RGB headers, two or three ARGB
//! headers, PCH, I/O cover, PCB); which ones exist and how many LEDs they
//! carry is read from the controller's configuration tables.

use tracing::{debug, info};

use super::NativeDevice;
use crate::hidraw::{self, HidRaw};
use crate::rgb::protocol::{color_mode, mode_flags, Color, Controller, Mode, Zone};
use crate::{Error, Result};

pub const VID: u16 = 0x26CE;
pub const PIDS: &[(u16, &str)] = &[(0x01A2, "ASRock Polychrome USB"), (0x01A6, "ASRock DeskMini Addressable LED Strip")];

const CMD_SET_ZONE: u8 = 0x10;
const CMD_READ_ZONE: u8 = 0x11;
const CMD_COMMIT: u8 = 0x12;
const CMD_READ_HEADER: u8 = 0x14;
const CMD_WRITE_HEADER: u8 = 0x15;
const CFG_LED_COUNT: u8 = 0x02;
const CFG_RGSWAP: u8 = 0x03;
const ZONE_UNAVAILABLE: u8 = 0x1E;
const CHUNK_FIRST: u8 = 0xE3;
const CHUNK_NEXT: u8 = 0xE4;
pub const MAX_ARGB_LEDS: u32 = 100;

const SPEED_MIN: u32 = 0xFF;
const SPEED_DEFAULT: u32 = 0xE0;
const SPEED_MAX: u32 = 0x00;
const MODE_DIRECT: i32 = 0x0F;

const ZONE_NAMES: [&str; 8] = ["RGB LED 1 Header", "RGB LED 2 Header", "Addressable Header 1", "Addressable Header 2", "PCH", "IO Cover", "PCB", "Addressable Header 3 / Audio"];
const ADDRESSABLE: [bool; 8] = [false, false, true, true, false, false, false, true];

/// (name, value, flags, colour mode)
const MODES: &[(&str, i32, u32, u32)] = &[
    ("Direct", MODE_DIRECT, mode_flags::HAS_PER_LED_COLOR, color_mode::PER_LED),
    ("Off", 0x00, 0, color_mode::NONE),
    ("Static", 0x01, mode_flags::HAS_PER_LED_COLOR, color_mode::PER_LED),
    ("Breathing", 0x02, mode_flags::HAS_SPEED | mode_flags::HAS_PER_LED_COLOR, color_mode::PER_LED),
    ("Strobe", 0x03, mode_flags::HAS_SPEED | mode_flags::HAS_PER_LED_COLOR, color_mode::PER_LED),
    ("Spectrum Cycle", 0x04, mode_flags::HAS_SPEED, color_mode::NONE),
    ("Random", 0x05, mode_flags::HAS_SPEED, color_mode::NONE),
    ("Wave", 0x07, mode_flags::HAS_SPEED, color_mode::NONE),
    ("Spring", 0x08, mode_flags::HAS_SPEED, color_mode::NONE),
    ("Stack", 0x09, mode_flags::HAS_SPEED, color_mode::NONE),
    ("Cram", 0x0A, mode_flags::HAS_SPEED, color_mode::NONE),
    ("Scan", 0x0B, mode_flags::HAS_SPEED, color_mode::NONE),
    ("Neon", 0x0C, 0, color_mode::NONE),
    ("Water", 0x0D, mode_flags::HAS_SPEED, color_mode::NONE),
    ("Rainbow", 0x0E, mode_flags::HAS_SPEED, color_mode::NONE),
];

fn modes() -> Vec<Mode> {
    MODES
        .iter()
        .map(|&(name, value, flags, cm)| Mode {
            name: name.into(),
            value,
            flags,
            speed_min: SPEED_MIN,
            speed_max: SPEED_MAX,
            speed: SPEED_DEFAULT,
            color_mode: cm,
            ..Default::default()
        })
        .collect()
}

#[derive(Debug, Clone)]
struct ZoneDef {
    /// Index in the controller's zone table (0–7).
    kind: u8,
    leds: u32,
    rgswap: bool,
}

pub fn find() -> Vec<(hidraw::HidNode, &'static str)> {
    let mut out = Vec::new();
    for &(pid, name) in PIDS {
        for node in hidraw::find(VID, pid) {
            out.push((node, name));
        }
    }
    out
}

pub struct Polychrome {
    dev: HidRaw,
    name: String,
    serial: String,
    location: String,
    zones: Vec<ZoneDef>,
    modes: Vec<Mode>,
    active_mode: usize,
    colors: Vec<Vec<Color>>,
    speed: u8,
}

impl Polychrome {
    pub fn open(node: &hidraw::HidNode, product: &str) -> Result<Self> {
        let mut dev = HidRaw::open(&node.path)?;
        dev.drain()?;
        let board = std::fs::read_to_string("/sys/class/dmi/id/board_name").map(|s| s.trim().to_string()).unwrap_or_default();
        let name = if product.contains("DeskMini") || board.is_empty() { product.to_string() } else { format!("ASRock {board}") };
        let led_counts = Self::read_header(&mut dev, CFG_LED_COUNT)?;
        let rgswap = Self::read_header(&mut dev, CFG_RGSWAP)?[0];
        let present = Self::read_header(&mut dev, 0x01)?[0];
        let mut zones = Vec::new();
        for z in 0..8usize {
            if led_counts[z] == ZONE_UNAVAILABLE || (present >> z) & 1 == 0 {
                continue;
            }
            zones.push(ZoneDef { kind: z as u8, leds: led_counts[z] as u32, rgswap: (rgswap >> z) & 1 == 1 });
        }
        if zones.is_empty() {
            return Err(Error::Protocol("controller reports no zones".into()));
        }
        // Current colours and mode from the first zone.
        let mut colors = Vec::new();
        let mut active_mode = 2;
        let mut speed = SPEED_DEFAULT as u8;
        for (i, z) in zones.iter().enumerate() {
            let mut pkt = [0u8; 64];
            pkt[0] = CMD_READ_ZONE;
            pkt[2] = z.kind;
            let reply = dev.exchange(&pkt, 200)?;
            let c = if reply.len() >= 9 {
                let (a, b, bl) = (reply[0x05], reply[0x06], reply[0x07]);
                if i == 0 {
                    let mode_val = reply[0x04];
                    let v = if mode_val != 0xE2 && mode_val != 0xE3 && mode_val < 0x0F { mode_val as i32 } else { MODE_DIRECT };
                    active_mode = MODES.iter().position(|m| m.1 == v).unwrap_or(2);
                    speed = reply[0x08];
                }
                if z.rgswap { Color::new(a, b, bl) } else { Color::new(b, a, bl) }
            } else {
                Color::BLACK
            };
            colors.push(vec![c; z.leds as usize]);
        }
        let serial = hidraw::serial_of(&node.path);
        info!(%name, zones = zones.len(), node = %node.path.display(), "ASRock Polychrome USB opened");
        Ok(Self { dev, name, serial, location: format!("HID: {}", node.path.display()), zones, modes: modes(), active_mode, colors, speed })
    }

    fn read_header(dev: &mut HidRaw, cfg: u8) -> Result<[u8; 8]> {
        let mut pkt = [0u8; 64];
        pkt[0] = CMD_READ_HEADER;
        pkt[2] = cfg;
        let reply = dev.exchange(&pkt, 300)?;
        if reply.len() < 12 {
            return Err(Error::Protocol(format!("short reply to config read 0x{cfg:02X}")));
        }
        let mut out = [0u8; 8];
        out.copy_from_slice(&reply[4..12]);
        Ok(out)
    }

    fn write_header(&mut self, cfg: u8, data: &[u8]) -> Result<()> {
        let mut pkt = [0u8; 64];
        pkt[0] = CMD_WRITE_HEADER;
        pkt[2] = cfg;
        pkt[3..3 + data.len()].copy_from_slice(data);
        self.dev.exchange(&pkt, 300)?;
        Ok(())
    }

    fn write_rgswap(&mut self, enable: bool) -> Result<()> {
        let mut bits = 0u8;
        if enable {
            for z in &self.zones {
                if z.rgswap {
                    bits |= 1 << z.kind;
                }
            }
        }
        self.write_header(CFG_RGSWAP, &[bits])
    }

    /// One zone in a hardware mode with one colour.
    fn write_zone(&mut self, zi: usize, mode: u8, speed: u8, c: Color) -> Result<()> {
        let z = self.zones[zi].clone();
        let mut pkt = [0u8; 64];
        pkt[0] = CMD_SET_ZONE;
        pkt[2] = z.kind;
        pkt[3] = mode;
        if z.rgswap {
            pkt[4] = c.r;
            pkt[5] = c.g;
        } else {
            pkt[4] = c.g;
            pkt[5] = c.r;
        }
        pkt[6] = c.b;
        pkt[7] = speed;
        pkt[8] = 0xFF;
        self.dev.exchange(&pkt, 100)?;
        Ok(())
    }

    /// Every LED of every zone (direct mode), in chunks.
    fn write_all(&mut self) -> Result<()> {
        let mut data: Vec<u8> = Vec::new();
        let mut max_leds = 0usize;
        for (i, z) in self.zones.iter().enumerate() {
            for c in &self.colors[i] {
                if z.rgswap {
                    data.extend_from_slice(&[c.r, c.g, c.b]);
                } else {
                    data.extend_from_slice(&[c.g, c.r, c.b]);
                }
            }
            max_leds += if ADDRESSABLE[z.kind as usize] { MAX_ARGB_LEDS as usize } else { z.leds as usize };
        }
        data.resize((max_leds + 8) * 3, 0);
        let mut first = true;
        let mut idx = 0usize;
        while idx < data.len() {
            let mut pkt = [0u8; 64];
            pkt[0] = CMD_SET_ZONE;
            pkt[2] = 0xFF;
            pkt[3] = if first { CHUNK_FIRST } else { CHUNK_NEXT };
            pkt[0x3F] = 0x65;
            let (offset, cap) = if first {
                pkt[6] = 0xFF;
                (8usize, 54usize)
            } else {
                (4usize, 57usize)
            };
            let n = cap.min(data.len() - idx);
            pkt[offset..offset + n].copy_from_slice(&data[idx..idx + n]);
            idx += n;
            self.dev.exchange(&pkt, 100)?;
            first = false;
        }
        Ok(())
    }
}

impl NativeDevice for Polychrome {
    fn location(&self) -> String {
        self.location.clone()
    }

    fn describe(&self) -> Controller {
        let mut zones = Vec::new();
        let mut leds = Vec::new();
        let mut colors = Vec::new();
        let mut start = 0u32;
        for (i, z) in self.zones.iter().enumerate() {
            let name = ZONE_NAMES[z.kind as usize].to_string();
            for l in 0..z.leds {
                leds.push(format!("{name}, LED {}", l + 1));
            }
            colors.extend(self.colors[i].iter().copied());
            let addressable = ADDRESSABLE[z.kind as usize];
            zones.push(Zone {
                name,
                kind: 1,
                leds_min: if addressable { 0 } else { z.leds },
                leds_max: if addressable { MAX_ARGB_LEDS } else { z.leds },
                leds_count: z.leds,
                start_idx: start,
                ..Default::default()
            });
            start += z.leds;
        }
        Controller {
            index: 0,
            kind: 0,
            name: self.name.clone(),
            vendor: "ASRock".into(),
            description: "ASRock Polychrome USB (native)".into(),
            version: String::new(),
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
        let zi = zone as usize;
        let z = self.zones.get(zi).cloned().ok_or_else(|| Error::InvalidValue(format!("no zone {zone}")))?;
        if !ADDRESSABLE[z.kind as usize] {
            return Err(Error::InvalidValue("this zone has a fixed size".into()));
        }
        let n = leds.min(MAX_ARGB_LEDS);
        self.zones[zi].leds = n;
        self.colors[zi].resize(n as usize, Color::BLACK);
        // Tell the controller the new LED count table.
        let mut table = [ZONE_UNAVAILABLE; 8];
        for z in &self.zones {
            table[z.kind as usize] = z.leds as u8;
        }
        self.write_header(CFG_LED_COUNT, &table)
    }

    fn set_direct(&mut self) -> Result<()> {
        if self.active_mode != 0 {
            self.write_rgswap(false)?;
            for zi in 0..self.zones.len() {
                if self.zones[zi].leds > 0 {
                    let c = self.colors[zi].first().copied().unwrap_or(Color::BLACK);
                    let sp = self.speed;
                    self.write_zone(zi, MODE_DIRECT as u8, sp, c)?;
                }
            }
            self.active_mode = 0;
        }
        Ok(())
    }

    fn zone_leds(&mut self, zone: u32, colors: &[Color]) -> Result<()> {
        let zi = zone as usize;
        if zi >= self.zones.len() {
            return Err(Error::InvalidValue(format!("no zone {zone}")));
        }
        let n = self.zones[zi].leds as usize;
        if n == 0 {
            return Ok(());
        }
        let mut new: Vec<Color> = colors.iter().copied().take(n).collect();
        new.resize(n, Color::BLACK);
        if self.colors[zi] == new {
            return Ok(());
        }
        self.colors[zi] = new;
        if self.active_mode == 0 {
            self.write_all()
        } else {
            let m = self.modes[self.active_mode].clone();
            let c = self.colors[zi][0];
            let sp = self.speed;
            self.write_zone(zi, m.value as u8, sp, c)
        }
    }

    fn set_mode(&mut self, mode_idx: usize, mode: &Mode) -> Result<()> {
        let m = self.modes.get_mut(mode_idx).ok_or_else(|| Error::InvalidValue(format!("no mode {mode_idx}")))?;
        m.speed = mode.speed.min(0xFF);
        let m = m.clone();
        self.speed = m.speed as u8;
        self.active_mode = mode_idx;
        if m.value == MODE_DIRECT {
            self.write_rgswap(false)?;
        } else {
            self.write_rgswap(true)?;
        }
        if let Some(c) = mode.colors.first() {
            if mode.color_mode == color_mode::MODE_SPECIFIC {
                for z in self.colors.iter_mut() {
                    for led in z.iter_mut() {
                        *led = *c;
                    }
                }
            }
        }
        for zi in 0..self.zones.len() {
            if self.zones[zi].leds == 0 {
                continue;
            }
            let c = self.colors[zi].first().copied().unwrap_or(Color::BLACK);
            let sp = self.speed;
            self.write_zone(zi, m.value as u8, sp, c)?;
        }
        if m.value == MODE_DIRECT {
            self.write_all()?;
        }
        debug!(mode = %m.name, "Polychrome mode set");
        Ok(())
    }

    fn save_mode(&mut self, mode_idx: usize, mode: &Mode) -> Result<()> {
        self.set_mode(mode_idx, mode)?;
        let mut pkt = [0u8; 64];
        pkt[0] = CMD_COMMIT;
        self.dev.exchange(&pkt, 200)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables() {
        assert_eq!(modes()[0].value, MODE_DIRECT);
        assert_eq!(ZONE_NAMES.len(), 8);
        assert!(ADDRESSABLE[2] && !ADDRESSABLE[4]);
    }
}
