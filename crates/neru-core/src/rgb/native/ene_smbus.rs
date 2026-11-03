//! ENE (ASUS Aura) SMBus RGB controllers: RGB RAM modules (G.Skill Trident Z
//! RGB, T-Force Delta, Geil Super Luce, ASUS/ENE DIMMs…), SMBus-controlled
//! ASUS motherboards (older Aura boards at 0x40/0x4E/0x4F) and ASUS graphics
//! cards (ROG Strix / TUF / KO / Dual…).
//!
//! Ported from OpenRGB's `ENESMBusController` (Adam Honse, GPL-2.0-or-later).
//! The chip exposes a 16-bit register space through two SMBus commands:
//! write the register address as a word at command 0x00, then read the value
//! at command 0x81 / write it at 0x01 (or a block at 0x03).
//!
//! RAM modules all power up at 0x77 and are moved to their own addresses by
//! writing the slot index + new address through 0x77 (what every Aura-style
//! tool does at start). Access needs `/dev/i2c-*` (module `i2c-dev`) plus the
//! chipset SMBus driver (`i2c-piix4` on AMD, `i2c-i801` on Intel).

use std::thread;
use std::time::Duration;

use tracing::{debug, info};

use super::NativeDevice;
use crate::i2c::{self, Adapter, I2cDev};
use crate::rgb::protocol::{color_mode, mode_flags, Color, Controller, Mode, Zone};
use crate::{Error, Result};

const REG_DEVICE_NAME: u16 = 0x1000;
const REG_MICRON_CHECK: u16 = 0x1030;
const REG_CONFIG_TABLE: u16 = 0x1C00;
const REG_COLORS_DIRECT: u16 = 0x8000;
const REG_COLORS_EFFECT: u16 = 0x8010;
const REG_DIRECT: u16 = 0x8020;
const REG_MODE: u16 = 0x8021;
const REG_SPEED: u16 = 0x8022;
const REG_DIRECTION: u16 = 0x8023;
const REG_APPLY: u16 = 0x80A0;
const REG_SLOT_INDEX: u16 = 0x80F8;
const REG_I2C_ADDRESS: u16 = 0x80F9;
const REG_COLORS_DIRECT_V2: u16 = 0x8100;
const REG_COLORS_EFFECT_V2: u16 = 0x8160;

const APPLY_VAL: u8 = 0x01;
const SAVE_VAL: u8 = 0xAA;
const NUM_CFG_ZONES: usize = 8;

const MODE_OFF: i32 = 0;
const MODE_STATIC: i32 = 1;
const MODE_BREATHING: i32 = 2;
const MODE_FLASHING: i32 = 3;
const MODE_SPECTRUM_CYCLE: i32 = 4;
const MODE_RAINBOW: i32 = 5;
const MODE_SPECTRUM_CYCLE_BREATHING: i32 = 6;
const MODE_CHASE_FADE: i32 = 7;
const MODE_SPECTRUM_CYCLE_CHASE_FADE: i32 = 8;
const MODE_CHASE: i32 = 9;
const MODE_SPECTRUM_CYCLE_CHASE: i32 = 10;
const MODE_RANDOM_FLICKER: i32 = 13;
const MODE_DOUBLE_FADE: i32 = 14;
/// Pseudo mode value for direct control.
const MODE_DIRECT: i32 = 0xFFFF;

const SPEED_SLOWEST: u32 = 0x04;
const SPEED_NORMAL: u32 = 0x02;
const SPEED_FASTEST: u32 = 0x00;

/// Addresses RAM modules get remapped to, in order.
pub const RAM_ADDRESSES: &[u8] = &[0x70, 0x71, 0x72, 0x73, 0x74, 0x75, 0x76, 0x77, 0x4F, 0x66, 0x67, 0x39, 0x3A, 0x3B, 0x3C, 0x3D];
/// Older ASUS boards with SMBus Aura chips.
pub const MOBO_ADDRESSES: &[u8] = &[0x40, 0x4E, 0x4F];
pub const ASUS_SUBVENDOR: u16 = 0x1043;

include!("ene_gpu_table.rs");

// ── register access ─────────────────────────────────────────────────────────

fn reg_read(dev: &mut I2cDev, addr: u8, reg: u16) -> Result<u8> {
    dev.write_word_data(addr, 0x00, reg.swap_bytes())?;
    dev.read_byte_data(addr, 0x81)
}

fn reg_write(dev: &mut I2cDev, addr: u8, reg: u16, val: u8) -> Result<()> {
    dev.write_word_data(addr, 0x00, reg.swap_bytes())?;
    dev.write_byte_data(addr, 0x01, val)
}

fn reg_write_block(dev: &mut I2cDev, addr: u8, reg: u16, data: &[u8]) -> Result<()> {
    dev.write_word_data(addr, 0x00, reg.swap_bytes())?;
    if dev.write_block_data(addr, 0x03, data).is_err() {
        // Some SMBus controllers reject block writes: fall back to bytes.
        for b in data {
            dev.write_byte_data(addr, 0x01, *b)?;
        }
    }
    Ok(())
}

/// OpenRGB's `TestForENESMBusController`: something answers, registers
/// 0xA0..0xB0 read back as 0..15, and it is not a Micron SPD.
pub fn probe(dev: &mut I2cDev, addr: u8) -> bool {
    if dev.read_byte(addr).is_err() && dev.read_byte_data(addr, 0x00).is_err() {
        return false;
    }
    for i in 0xA0u8..0xB0 {
        match dev.read_byte_data(addr, i) {
            Ok(v) if v == i - 0xA0 => {}
            _ => return false,
        }
    }
    let mut buf = [0u8; 16];
    for (i, b) in buf.iter_mut().enumerate() {
        *b = reg_read(dev, addr, REG_MICRON_CHECK + i as u16).unwrap_or(0);
    }
    if buf.starts_with(b"Micron\0") {
        debug!(addr = format!("0x{addr:02X}"), "ENE probe: Micron device, skipping");
        return false;
    }
    true
}

/// Where a device came from (names and device types differ).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Dram,
    Motherboard,
    Gpu,
}

#[derive(Debug, Clone)]
pub struct Found {
    pub adapter: Adapter,
    pub addr: u8,
    pub name: String,
    pub kind: Kind,
}

/// Move every RAM module waiting at 0x77 to its own address (OpenRGB's
/// `DetectENESMBusDRAMControllers`). Only on the chipset SMBus.
fn remap_dram(dev: &mut I2cDev) {
    let mut idx: isize = -1;
    for slot in 0..8u8 {
        if dev.read_byte(0x77).is_err() {
            break;
        }
        // Next address in the list with nothing on it.
        loop {
            idx += 1;
            if idx as usize >= RAM_ADDRESSES.len() {
                break;
            }
            if dev.read_byte(RAM_ADDRESSES[idx as usize]).is_err() {
                break;
            }
        }
        if idx as usize >= RAM_ADDRESSES.len() {
            break;
        }
        let target = RAM_ADDRESSES[idx as usize];
        debug!(slot, addr = format!("0x{target:02X}"), "ENE DRAM: remapping module");
        let _ = reg_write(dev, 0x77, REG_SLOT_INDEX, slot);
        let _ = reg_write(dev, 0x77, REG_I2C_ADDRESS, target << 1);
        thread::sleep(Duration::from_millis(1));
    }
}

/// Probe the chipset SMBus for RAM modules and Aura motherboard chips, and
/// GPU adapters for the cards in [`GPU_CARDS`].
pub fn find() -> Vec<Found> {
    let mut out = Vec::new();
    let adapters = i2c::adapters();
    let board = std::fs::read_to_string("/sys/class/dmi/id/board_name").map(|s| s.trim().to_string()).unwrap_or_default();
    let board_vendor = std::fs::read_to_string("/sys/class/dmi/id/board_vendor").map(|s| s.trim().to_ascii_lowercase()).unwrap_or_default();
    for a in &adapters {
        if a.is_chipset_smbus() {
            let mut dev = match I2cDev::open(&a.dev) {
                Ok(d) => d,
                Err(e) => {
                    debug!(dev = %a.dev.display(), error = %e, "cannot open SMBus adapter");
                    continue;
                }
            };
            remap_dram(&mut dev);
            for &addr in RAM_ADDRESSES {
                if probe(&mut dev, addr) {
                    out.push(Found { adapter: a.clone(), addr, name: "ENE DRAM".into(), kind: Kind::Dram });
                }
                thread::sleep(Duration::from_millis(1));
            }
            // Aura SMBus motherboards (ASUS only, like OpenRGB).
            let asus = board_vendor.contains("asus") || a.pci.as_ref().map_or(true, |p| p.subvendor == ASUS_SUBVENDOR || p.subvendor == 0 || p.subvendor == 0xFFFF);
            if asus {
                for &addr in MOBO_ADDRESSES {
                    if out.iter().any(|f| f.addr == addr && f.adapter.bus == a.bus) {
                        continue;
                    }
                    if probe(&mut dev, addr) {
                        out.push(Found { adapter: a.clone(), addr, name: format!("ASUS {}", if board.is_empty() { "Aura SMBus".into() } else { board.clone() }), kind: Kind::Motherboard });
                    }
                    thread::sleep(Duration::from_millis(1));
                }
            }
        } else if a.is_gpu() {
            let Some(pci) = &a.pci else { continue };
            let Some(&(_, _, _, _, addr, name)) = GPU_CARDS.iter().find(|c| c.0 == pci.vendor && c.1 == pci.device && c.2 == pci.subvendor && c.3 == pci.subdevice) else { continue };
            if out.iter().any(|f| f.kind == Kind::Gpu && f.adapter.pci.as_ref().map(|p| &p.slot) == Some(&pci.slot)) {
                continue;
            }
            let mut dev = match I2cDev::open(&a.dev) {
                Ok(d) => d,
                Err(_) => continue,
            };
            if probe(&mut dev, addr) {
                out.push(Found { adapter: a.clone(), addr, name: name.to_string(), kind: Kind::Gpu });
            }
        }
    }
    out
}

// ── the device ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct ZoneDef {
    name: String,
    leds: u32,
}

pub struct EneDevice {
    dev: I2cDev,
    addr: u8,
    name: String,
    version: String,
    kind: Kind,
    location: String,
    direct_reg: u16,
    effect_reg: u16,
    zones: Vec<ZoneDef>,
    led_count: usize,
    modes: Vec<Mode>,
    active_mode: usize,
    colors: Vec<Color>,
    direct: bool,
}

fn channel_name(code: u8) -> &'static str {
    match code {
        0x84 => "Audio",
        0x88 => "Backplate",
        0x85 => "Back I/O",
        0x83 | 0x82 => "Center",
        0x8A | 0x05 | 0x0E => "DRAM",
        0x8B => "PCIe",
        0x86 | 0x91 => "RGB Header",
        0x87 => "RGB Header 2",
        _ => "Unknown",
    }
}

fn modes(double_fade: bool) -> Vec<Mode> {
    let base = |name: &str, value: i32, flags: u32, cm: u32| Mode {
        name: name.into(),
        value,
        flags,
        speed_min: SPEED_SLOWEST,
        speed_max: SPEED_FASTEST,
        speed: SPEED_NORMAL,
        color_mode: cm,
        ..Default::default()
    };
    let speed_flag = mode_flags::HAS_SPEED;
    let mut v = vec![
        base("Direct", MODE_DIRECT, mode_flags::HAS_PER_LED_COLOR, color_mode::PER_LED),
        base("Off", MODE_OFF, mode_flags::MANUAL_SAVE, color_mode::NONE),
        base("Static", MODE_STATIC, mode_flags::HAS_PER_LED_COLOR | mode_flags::MANUAL_SAVE, color_mode::PER_LED),
        base("Breathing", MODE_BREATHING, mode_flags::HAS_RANDOM_COLOR | mode_flags::HAS_PER_LED_COLOR | speed_flag | mode_flags::MANUAL_SAVE, color_mode::PER_LED),
        base("Flashing", MODE_FLASHING, mode_flags::HAS_PER_LED_COLOR | speed_flag | mode_flags::MANUAL_SAVE, color_mode::PER_LED),
        base("Spectrum Cycle", MODE_SPECTRUM_CYCLE, speed_flag | mode_flags::MANUAL_SAVE, color_mode::NONE),
        base("Rainbow", MODE_RAINBOW, speed_flag | mode_flags::HAS_DIRECTION_LR | mode_flags::MANUAL_SAVE, color_mode::NONE),
        base("Chase Fade", MODE_CHASE_FADE, mode_flags::HAS_RANDOM_COLOR | mode_flags::HAS_PER_LED_COLOR | speed_flag | mode_flags::HAS_DIRECTION_LR | mode_flags::MANUAL_SAVE, color_mode::PER_LED),
        base("Chase", MODE_CHASE, mode_flags::HAS_RANDOM_COLOR | mode_flags::HAS_PER_LED_COLOR | speed_flag | mode_flags::HAS_DIRECTION_LR | mode_flags::MANUAL_SAVE, color_mode::PER_LED),
        base("Random Flicker", MODE_RANDOM_FLICKER, speed_flag | mode_flags::MANUAL_SAVE, color_mode::NONE),
    ];
    if double_fade {
        v.push(base("Double Fade", MODE_DOUBLE_FADE, speed_flag, color_mode::NONE));
    }
    v
}

impl EneDevice {
    pub fn open(found: &Found) -> Result<Self> {
        let mut dev = I2cDev::open(&found.adapter.dev)?;
        let addr = found.addr;
        let mut name_buf = [0u8; 16];
        for (i, b) in name_buf.iter_mut().enumerate() {
            *b = reg_read(&mut dev, addr, REG_DEVICE_NAME + i as u16)?;
        }
        let version = String::from_utf8_lossy(&name_buf[..15]).trim_end_matches('\0').trim().to_string();
        let mut table = [0u8; 64];
        for (i, b) in table.iter_mut().enumerate() {
            *b = reg_read(&mut dev, addr, REG_CONFIG_TABLE + i as u16)?;
        }
        // Register layout and LED count depend on the firmware generation.
        let (direct_reg, effect_reg, channel_cfg, led_count_idx): (u16, u16, usize, usize) = match version.as_str() {
            "LED-0116" | "DIMM_LED-0102" | "DIMM_LED-0103" | "AUMA0-E8K4-0101" => (REG_COLORS_DIRECT, REG_COLORS_EFFECT, 0x13, 0x02),
            "AUDA0-E6K5-0101" => (REG_COLORS_DIRECT_V2, REG_COLORS_EFFECT_V2, 0x13, 0x02),
            "AUMA0-E6K5-0106" | "AUMA0-E6K5-0105" | "AUMA0-E6K5-0104" => (REG_COLORS_DIRECT_V2, REG_COLORS_EFFECT_V2, 0x1B, 0x02),
            "AUMA0-E6K5-0107" | "AUMA0-E6K5-1110" | "AUMA0-E6K5-1111" | "AUMA0-E6K5-1107" | "AUMA0-E6K5-1113" | "AUMA0-E6K5-1114" => (REG_COLORS_DIRECT_V2, REG_COLORS_EFFECT_V2, 0x1B, 0x03),
            "AUMA0-E6K5-0008" => (REG_COLORS_DIRECT_V2, REG_COLORS_EFFECT_V2, 0x13, 0x03),
            _ => return Err(Error::Protocol(format!("unknown ENE firmware {version:?} at 0x{addr:02X} — not touching it"))),
        };
        let led_count = table[led_count_idx] as usize;
        if led_count == 0 {
            return Err(Error::Protocol(format!("ENE {version} at 0x{addr:02X} reports no LEDs")));
        }
        let double_fade = version == "AUDA0-E6K5-0101" && table[channel_cfg..channel_cfg + NUM_CFG_ZONES].contains(&0x0E);
        // Zones: one per channel name, LED counts of same-named channels merged.
        let mut zones: Vec<ZoneDef> = Vec::new();
        for z in 0..NUM_CFG_ZONES {
            let n = table[0x03 + z] as u32;
            if n == 0 {
                continue;
            }
            let name = channel_name(table[channel_cfg + z]).to_string();
            match zones.iter_mut().find(|zz| zz.name == name) {
                Some(zz) => zz.leds += n,
                None => zones.push(ZoneDef { name, leds: n }),
            }
        }
        let total: u32 = zones.iter().map(|z| z.leds).sum();
        if total as usize != led_count {
            // Trust the LED count register; put the remainder in one zone.
            zones = vec![ZoneDef { name: if found.kind == Kind::Dram { "DRAM".into() } else { "LEDs".into() }, leds: led_count as u32 }];
        }
        let location = format!("I2C: {} ({}), address 0x{addr:02X}", found.adapter.dev.display(), found.adapter.name);
        // Current state.
        let direct = reg_read(&mut dev, addr, REG_DIRECT).unwrap_or(0) != 0;
        let mode_val = reg_read(&mut dev, addr, REG_MODE).unwrap_or(MODE_STATIC as u8) as i32;
        let modes = modes(double_fade);
        let active_mode = if direct {
            0
        } else {
            let mapped = match mode_val {
                MODE_SPECTRUM_CYCLE_CHASE => MODE_CHASE,
                MODE_SPECTRUM_CYCLE_BREATHING => MODE_BREATHING,
                MODE_SPECTRUM_CYCLE_CHASE_FADE => MODE_CHASE_FADE,
                v => v,
            };
            modes.iter().position(|m| m.value == mapped).unwrap_or(2)
        };
        let mut colors = Vec::with_capacity(led_count);
        let reg = if direct { direct_reg } else { effect_reg };
        for led in 0..led_count as u16 {
            let r = reg_read(&mut dev, addr, reg + 3 * led).unwrap_or(0);
            let b = reg_read(&mut dev, addr, reg + 3 * led + 1).unwrap_or(0);
            let g = reg_read(&mut dev, addr, reg + 3 * led + 2).unwrap_or(0);
            colors.push(Color::new(r, g, b));
        }
        info!(name = %found.name, %version, leds = led_count, zones = zones.len(), %location, "ENE SMBus device opened");
        Ok(Self {
            dev,
            addr,
            name: found.name.clone(),
            version,
            kind: found.kind,
            location,
            direct_reg,
            effect_reg,
            zones,
            led_count,
            modes,
            active_mode,
            colors,
            direct,
        })
    }

    fn write_colors(&mut self, reg: u16, leds: std::ops::Range<usize>) -> Result<()> {
        for led in leds {
            let c = self.colors[led];
            // Registers hold R, B, G.
            reg_write_block(&mut self.dev, self.addr, reg + 3 * led as u16, &[c.r, c.b, c.g])?;
        }
        Ok(())
    }

    fn zone_range(&self, zone: u32) -> Result<std::ops::Range<usize>> {
        let mut start = 0usize;
        for (i, z) in self.zones.iter().enumerate() {
            if i == zone as usize {
                return Ok(start..start + z.leds as usize);
            }
            start += z.leds as usize;
        }
        Err(Error::InvalidValue(format!("no zone {zone}")))
    }

    fn apply_mode(&mut self, mode_idx: usize) -> Result<()> {
        let m = self.modes[mode_idx].clone();
        if m.value == MODE_DIRECT {
            reg_write(&mut self.dev, self.addr, REG_DIRECT, 1)?;
            reg_write(&mut self.dev, self.addr, REG_APPLY, APPLY_VAL)?;
            self.direct = true;
        } else {
            let mut value = m.value;
            if m.color_mode == color_mode::RANDOM {
                value = match value {
                    MODE_CHASE => MODE_SPECTRUM_CYCLE_CHASE,
                    MODE_BREATHING => MODE_SPECTRUM_CYCLE_BREATHING,
                    MODE_CHASE_FADE => MODE_SPECTRUM_CYCLE_CHASE_FADE,
                    v => v,
                };
            }
            let speed = if m.has(mode_flags::HAS_SPEED) { m.speed.min(4) as u8 } else { 0 };
            let direction = if m.has(mode_flags::HAS_DIRECTION_LR) && m.direction == 1 { 1 } else { 0 };
            reg_write(&mut self.dev, self.addr, REG_MODE, value as u8)?;
            reg_write(&mut self.dev, self.addr, REG_SPEED, speed)?;
            reg_write(&mut self.dev, self.addr, REG_DIRECTION, direction)?;
            reg_write(&mut self.dev, self.addr, REG_APPLY, APPLY_VAL)?;
            reg_write(&mut self.dev, self.addr, REG_DIRECT, 0)?;
            reg_write(&mut self.dev, self.addr, REG_APPLY, APPLY_VAL)?;
            self.direct = false;
        }
        self.active_mode = mode_idx;
        Ok(())
    }
}

impl NativeDevice for EneDevice {
    fn location(&self) -> String {
        self.location.clone()
    }

    fn describe(&self) -> Controller {
        let mut zones = Vec::new();
        let mut leds = Vec::new();
        let mut start = 0u32;
        for z in &self.zones {
            for i in 0..z.leds {
                leds.push(format!("{} LED {}", z.name, i + 1));
            }
            zones.push(Zone { name: z.name.clone(), kind: if z.leds > 1 { 1 } else { 0 }, leds_min: z.leds, leds_max: z.leds, leds_count: z.leds, start_idx: start, ..Default::default() });
            start += z.leds;
        }
        let (kind, vendor) = match self.kind {
            Kind::Dram => (1, if self.version.contains("DIMM_LED") || self.version.contains("AUDA") { "ENE" } else { "ASUS" }),
            Kind::Motherboard => (0, "ASUS"),
            Kind::Gpu => (2, "ASUS"),
        };
        Controller {
            index: 0,
            kind,
            name: self.name.clone(),
            vendor: vendor.into(),
            description: "ENE SMBus device (native)".into(),
            version: self.version.clone(),
            serial: String::new(),
            location: self.location.clone(),
            active_mode: self.active_mode as i32,
            modes: self.modes.clone(),
            zones,
            leds,
            colors: self.colors.clone(),
            ..Default::default()
        }
    }

    fn resize_zone(&mut self, _zone: u32, _leds: u32) -> Result<()> {
        Err(Error::InvalidValue("ENE zones have a fixed size".into()))
    }

    fn set_direct(&mut self) -> Result<()> {
        if !self.direct {
            self.apply_mode(0)?;
        }
        Ok(())
    }

    fn zone_leds(&mut self, zone: u32, colors: &[Color]) -> Result<()> {
        let range = self.zone_range(zone)?;
        let mut changed = false;
        for (i, led) in range.clone().enumerate() {
            if let Some(c) = colors.get(i) {
                if self.colors[led] != *c {
                    self.colors[led] = *c;
                    changed = true;
                }
            }
        }
        if !changed {
            return Ok(());
        }
        if self.direct {
            self.write_colors(self.direct_reg, range)
        } else {
            self.write_colors(self.effect_reg, range)?;
            reg_write(&mut self.dev, self.addr, REG_APPLY, APPLY_VAL)
        }
    }

    fn set_mode(&mut self, mode_idx: usize, mode: &Mode) -> Result<()> {
        let m = self.modes.get_mut(mode_idx).ok_or_else(|| Error::InvalidValue(format!("no mode {mode_idx}")))?;
        m.speed = mode.speed.min(SPEED_SLOWEST);
        m.direction = mode.direction;
        m.color_mode = if m.has(mode_flags::HAS_RANDOM_COLOR) && mode.color_mode == color_mode::RANDOM { color_mode::RANDOM } else { m.color_mode };
        if let Some(c) = mode.colors.first() {
            if mode.color_mode == color_mode::MODE_SPECIFIC {
                for led in self.colors.iter_mut() {
                    *led = *c;
                }
            }
        }
        self.apply_mode(mode_idx)?;
        if !self.direct && self.led_count > 0 {
            self.write_colors(self.effect_reg, 0..self.led_count)?;
            reg_write(&mut self.dev, self.addr, REG_APPLY, APPLY_VAL)?;
        }
        Ok(())
    }

    fn save_mode(&mut self, mode_idx: usize, mode: &Mode) -> Result<()> {
        self.set_mode(mode_idx, mode)?;
        reg_write(&mut self.dev, self.addr, REG_APPLY, SAVE_VAL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_table_is_sane() {
        assert!(GPU_CARDS.len() > 100);
        assert!(GPU_CARDS.iter().all(|c| c.2 == ASUS_SUBVENDOR || c.2 != 0));
        assert!(GPU_CARDS.iter().any(|c| c.5.contains("RTX 5080")));
    }

    #[test]
    fn mode_table() {
        let m = modes(false);
        assert_eq!(m[0].name, "Direct");
        assert_eq!(m.len(), 10);
        assert_eq!(modes(true).len(), 11);
        assert_eq!(channel_name(0x8A), "DRAM");
    }
}
