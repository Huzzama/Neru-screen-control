//! ASUS Aura USB motherboard controller (ROG / TUF / Prime / Strix boards,
//! X570 / B550 / X670 / B650 / X870 / Z590+ …).
//!
//! Ported from OpenRGB's `AsusAuraUSBController` / `AsusAuraMainboardController`
//! (GPL-2.0-or-later, © OpenRGB contributors). Protocol summary:
//!
//! * HID, report ID `0xEC`, 65-byte reports (report ID + 64 bytes)
//! * `EC 82` → firmware string (reply `xx 02 <16 bytes>`)
//! * `EC B0` → config table (reply `xx 30 .. ..` + 60 bytes from offset 4):
//!   `[0x02]` = number of ARGB headers, `[0x1B]` = fixed mainboard LEDs,
//!   `[0x1D]` = 12 V RGB headers
//! * `EC 52 53 00 01` once after opening ("gen1" mode)
//! * `EC 35 <effect ch> 00 <shutdown> <mode>` — select a mode on a channel
//! * `EC 36 <mask hi> <mask lo> <shutdown> <rgb…>` — mode colour for fixed LEDs
//! * `EC 3F 55` — commit (persist to flash)
//! * `EC 40 <direct ch | 0x80 apply | 0x10 overflow> <offset> <count> <rgb×20>`
//!   — direct per-LED colours, 20 LEDs per report; the fixed zone uses direct
//!   channel 4, ARGB header *n* uses direct channel *n*.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tracing::{debug, info};

use super::NativeDevice;
use crate::hidraw::HidRaw;
use crate::rgb::protocol::{color_mode, mode_flags, Color, Controller, Mode, Zone};
use crate::{Error, Result};

pub const VID: u16 = 0x0b05;

/// (PID, required HID usage page) — usage page 0 = any.
pub const MAINBOARD_PIDS: &[(u16, u16)] = &[
    (0x18f3, 0),
    (0x1939, 0),
    (0x19af, 0xff72),
    (0x1aa6, 0),
    (0x1bed, 0),
];

/// Max LEDs on one addressable header.
pub const ARGB_MAX_LEDS: u32 = 120;
const REPORT_ID: u8 = 0xec;
const REPORT_LEN: usize = 65;
const LEDS_PER_PACKET: usize = 20;
const DIRECT_FIXED_CHANNEL: u8 = 0x04;
const MODE_DIRECT: u8 = 0xff;

#[derive(Debug, Clone, Copy, PartialEq)]
enum ChannelKind {
    Fixed,
    Addressable,
}

#[derive(Debug, Clone)]
struct Channel {
    kind: ChannelKind,
    effect_channel: u8,
    direct_channel: u8,
    /// Fixed: LEDs reported by the board. Addressable: configured by the user.
    leds: u32,
    /// Fixed: how many of `leds` are 12 V RGB headers.
    headers: u32,
}

/// The modes the firmware offers (name, value, has a colour).
const MODES: &[(&str, u8, bool)] = &[
    ("Direct", MODE_DIRECT, false),
    ("Off", 0, false),
    ("Static", 1, true),
    ("Breathing", 2, true),
    ("Flashing", 3, true),
    ("Spectrum Cycle", 4, false),
    ("Rainbow", 5, false),
    ("Spectrum Cycle Breathing", 6, false),
    ("Chase Fade", 7, true),
    ("Spectrum Cycle Chase Fade", 8, false),
    ("Chase", 9, true),
    ("Spectrum Cycle Chase", 10, false),
    ("Spectrum Cycle Wave", 11, false),
    ("Chase Rainbow Pulse", 12, false),
    ("Random Flicker", 13, false),
    ("Gentle Transition", 16, false),
    ("Wave Propagation", 17, false),
    ("Wave Propagation Pause", 18, false),
    ("Red Pulse", 19, false),
];

pub struct AuraMainboard {
    hid: HidRaw,
    node: PathBuf,
    name: String,
    firmware: String,
    serial: String,
    channels: Vec<Channel>,
    active_mode: i32,
    modes: Vec<Mode>,
    direct_ready: bool,
    colors: Vec<Color>,
}

/// Every hidraw node that looks like an Aura USB mainboard controller.
pub fn find() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for &(pid, page) in MAINBOARD_PIDS {
        for node in crate::hidraw::find(VID, pid) {
            if page == 0 || crate::hidraw::has_usage_page(&node.path, page) {
                out.push(node.path);
            }
        }
    }
    out
}

fn board_name() -> String {
    let read = |f: &str| std::fs::read_to_string(format!("/sys/class/dmi/id/{f}")).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    read("board_name").map(|b| format!("ASUS {b}")).unwrap_or_else(|| "ASUS Aura Motherboard".into())
}

fn serial_of(node: &Path) -> String {
    let Some(name) = node.file_name() else { return String::new() };
    std::fs::read_to_string(Path::new("/sys/class/hidraw").join(name).join("device/uevent"))
        .ok()
        .and_then(|u| u.lines().find_map(|l| l.strip_prefix("HID_UNIQ=")).map(str::to_string))
        .unwrap_or_default()
}

impl AuraMainboard {
    pub fn open(node: &Path) -> Result<Self> {
        let hid = HidRaw::open(node)?;
        let mut dev = Self {
            hid,
            node: node.to_path_buf(),
            name: board_name(),
            firmware: String::new(),
            serial: serial_of(node),
            channels: Vec::new(),
            active_mode: 0,
            modes: Vec::new(),
            direct_ready: false,
            colors: Vec::new(),
        };
        dev.firmware = dev.read_firmware().unwrap_or_default();
        let table = dev.read_config_table()?;
        dev.channels = Self::channels_from_table(&table, &dev.firmware);
        dev.modes = MODES
            .iter()
            .map(|&(name, value, has_color)| Mode {
                name: name.into(),
                value: value as i32,
                flags: if value == MODE_DIRECT {
                    mode_flags::HAS_PER_LED_COLOR
                } else if has_color {
                    mode_flags::HAS_MODE_SPECIFIC_COLOR | mode_flags::MANUAL_SAVE
                } else {
                    mode_flags::MANUAL_SAVE
                },
                colors_min: has_color as u32,
                colors_max: has_color as u32,
                color_mode: if value == MODE_DIRECT {
                    color_mode::PER_LED
                } else if has_color {
                    color_mode::MODE_SPECIFIC
                } else {
                    color_mode::NONE
                },
                colors: if has_color { vec![Color::new(255, 0, 0)] } else { Vec::new() },
                ..Default::default()
            })
            .collect();
        dev.send(&[0x52, 0x53, 0x00, 0x01])?; // gen1
        info!(
            node = %node.display(),
            firmware = %dev.firmware,
            channels = dev.channels.len(),
            "ASUS Aura USB mainboard opened"
        );
        dev.resize_colors();
        Ok(dev)
    }

    fn channels_from_table(table: &[u8; 60], firmware: &str) -> Vec<Channel> {
        let mut fixed_leds = table[0x1b] as u32;
        let mut rgb_headers = table[0x1d] as u32;
        let argb_headers = table[0x02];
        if fixed_leds < rgb_headers {
            rgb_headers = 0;
        }
        let mut out = Vec::new();
        let mut effect_channel = 0u8;
        if fixed_leds > 0 {
            // AULA3-AR32 firmware numbers the ARGB effect channels from zero.
            let from_zero = firmware.starts_with("AULA3-AR32");
            out.push(Channel {
                kind: ChannelKind::Fixed,
                effect_channel: if from_zero { argb_headers } else { effect_channel },
                direct_channel: DIRECT_FIXED_CHANNEL,
                leds: fixed_leds,
                headers: rgb_headers,
            });
            if !from_zero {
                effect_channel += 1;
            }
        } else {
            fixed_leds = 0;
        }
        let _ = fixed_leds;
        for i in 0..argb_headers {
            out.push(Channel { kind: ChannelKind::Addressable, effect_channel, direct_channel: i, leds: 0, headers: 0 });
            effect_channel += 1;
        }
        out
    }

    fn send(&mut self, payload: &[u8]) -> Result<()> {
        let mut buf = [0u8; REPORT_LEN];
        buf[0] = REPORT_ID;
        buf[1..1 + payload.len()].copy_from_slice(payload);
        self.hid.write_raw(&buf)
    }

    /// Send a request and wait for a reply whose byte 1 is `want`.
    fn request(&mut self, payload: &[u8], want: u8) -> Result<[u8; REPORT_LEN]> {
        self.hid.drain()?;
        self.send(payload)?;
        let deadline = Instant::now() + Duration::from_millis(1500);
        let mut buf = [0u8; REPORT_LEN];
        while Instant::now() < deadline {
            if let Some(n) = self.hid.read_report(&mut buf, 200)? {
                if n >= 2 && buf[1] == want {
                    return Ok(buf);
                }
                debug!(got = buf[1], want, "ignoring Aura reply");
            }
        }
        Err(Error::Timeout(format!("Aura reply 0x{want:02x}")))
    }

    fn read_firmware(&mut self) -> Result<String> {
        let r = self.request(&[0x82], 0x02)?;
        let raw = &r[2..18];
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        Ok(String::from_utf8_lossy(&raw[..end]).trim().to_string())
    }

    fn read_config_table(&mut self) -> Result<[u8; 60]> {
        let r = self.request(&[0xb0], 0x30)?;
        let mut t = [0u8; 60];
        t.copy_from_slice(&r[4..64]);
        debug!(table = ?t, "Aura config table");
        Ok(t)
    }

    fn resize_colors(&mut self) {
        let total: u32 = self.channels.iter().map(|c| c.leds).sum();
        self.colors.resize(total as usize, Color::BLACK);
    }

    fn effect(&mut self, effect_channel: u8, mode: u8, shutdown: bool) -> Result<()> {
        self.send(&[0x35, effect_channel, 0x00, shutdown as u8, mode])
    }

    fn mode_color(&mut self, start_led: u32, count: u32, color: Color, shutdown: bool) -> Result<()> {
        // 16-bit mask of the LEDs this colour applies to (fixed LEDs only fit).
        if start_led + count > 16 || count == 0 {
            return Ok(());
        }
        let mask: u16 = (((1u32 << count) - 1) << start_led) as u16;
        let mut p = vec![0x36, (mask >> 8) as u8, (mask & 0xff) as u8, shutdown as u8];
        p.resize(4 + 3 * start_led as usize, 0);
        for _ in 0..count {
            p.extend_from_slice(&[color.r, color.g, color.b]);
        }
        p.truncate(REPORT_LEN - 1);
        self.send(&p)
    }

    fn direct(&mut self, direct_channel: u8, colors: &[Color]) -> Result<()> {
        let total = colors.len();
        let mut done = 0usize;
        let mut offset: usize = 0;
        let mut overflow = false;
        while done < total {
            let n = (total - done).min(LEDS_PER_PACKET);
            let last = done + n >= total;
            let mut cmd = direct_channel;
            if last {
                cmd |= 0x80;
            }
            if overflow {
                cmd |= 0x10;
            }
            let mut p = vec![0x40, cmd, (offset & 0xff) as u8, n as u8];
            for c in &colors[done..done + n] {
                p.extend_from_slice(&[c.r, c.g, c.b]);
            }
            self.send(&p)?;
            done += n;
            offset += n;
            if done > 255 {
                overflow = true;
                offset = done - 256;
            }
        }
        Ok(())
    }

    fn apply_mode(&mut self, mode: &Mode, shutdown: bool) -> Result<()> {
        let value = mode.value as u8;
        let color = mode.colors.first().copied().unwrap_or(Color::BLACK);
        let channels = self.channels.clone();
        let mut start = 0u32;
        for ch in &channels {
            let active = ch.kind == ChannelKind::Fixed || ch.leds > 0;
            if active && (!shutdown || ch.kind == ChannelKind::Fixed) {
                self.effect(ch.effect_channel, value, shutdown)?;
                if value != MODE_DIRECT && mode.color_mode == color_mode::MODE_SPECIFIC && ch.kind == ChannelKind::Fixed {
                    self.mode_color(start, ch.leds, color, shutdown)?;
                }
            }
            // Addressable channels count as one LED in the colour mask.
            start += if ch.kind == ChannelKind::Fixed { ch.leds } else { 1 };
        }
        Ok(())
    }
}

impl NativeDevice for AuraMainboard {
    fn location(&self) -> String {
        format!("HID: {}", self.node.display())
    }

    fn describe(&self) -> Controller {
        let mut zones = Vec::new();
        let mut leds = Vec::new();
        let mut start = 0u32;
        let mut argb = 0;
        for ch in &self.channels {
            let (name, min, max) = match ch.kind {
                ChannelKind::Fixed => ("Aura Mainboard".to_string(), ch.leds, ch.leds),
                ChannelKind::Addressable => {
                    argb += 1;
                    (format!("Addressable RGB Header {argb}"), 0, ARGB_MAX_LEDS)
                }
            };
            for i in 0..ch.leds {
                let mainboard_leds = ch.leds - ch.headers;
                leds.push(if ch.kind == ChannelKind::Fixed && i >= mainboard_leds {
                    format!("{name}, RGB Header {}", i - mainboard_leds + 1)
                } else {
                    format!("{name}, LED {}", i + 1)
                });
            }
            zones.push(Zone { name, kind: 1, leds_min: min, leds_max: max, leds_count: ch.leds, start_idx: start, ..Default::default() });
            start += ch.leds;
        }
        Controller {
            index: 0,
            kind: 0,
            name: self.name.clone(),
            vendor: "ASUS".into(),
            description: "ASUS Aura USB Mainboard (native)".into(),
            version: self.firmware.clone(),
            serial: self.serial.clone(),
            location: self.location(),
            active_mode: self.active_mode,
            modes: self.modes.clone(),
            zones,
            leds,
            colors: self.colors.clone(),
            ..Default::default()
        }
    }

    fn resize_zone(&mut self, zone: u32, leds: u32) -> Result<()> {
        let ch = self.channels.get_mut(zone as usize).ok_or_else(|| Error::InvalidValue(format!("no zone {zone}")))?;
        if ch.kind != ChannelKind::Addressable {
            return Err(Error::InvalidValue("this zone has a fixed size".into()));
        }
        ch.leds = leds.min(ARGB_MAX_LEDS);
        self.direct_ready = false;
        self.resize_colors();
        Ok(())
    }

    fn set_direct(&mut self) -> Result<()> {
        let channels = self.channels.clone();
        for ch in channels.iter().filter(|c| c.kind == ChannelKind::Fixed || c.leds > 0) {
            self.effect(ch.effect_channel, MODE_DIRECT, false)?;
        }
        self.active_mode = 0;
        self.direct_ready = true;
        Ok(())
    }

    fn zone_leds(&mut self, zone: u32, colors: &[Color]) -> Result<()> {
        if !self.direct_ready {
            self.set_direct()?;
        }
        let ch = self.channels.get(zone as usize).cloned().ok_or_else(|| Error::InvalidValue(format!("no zone {zone}")))?;
        if ch.leds == 0 {
            return Ok(());
        }
        let mut c: Vec<Color> = colors.iter().take(ch.leds as usize).copied().collect();
        c.resize(ch.leds as usize, *colors.last().unwrap_or(&Color::BLACK));
        self.direct(ch.direct_channel, &c)?;
        let start: u32 = self.channels[..zone as usize].iter().map(|x| x.leds).sum();
        for (i, col) in c.iter().enumerate() {
            if let Some(slot) = self.colors.get_mut(start as usize + i) {
                *slot = *col;
            }
        }
        Ok(())
    }

    fn set_mode(&mut self, mode_idx: usize, mode: &Mode) -> Result<()> {
        self.apply_mode(mode, false)?;
        self.active_mode = mode_idx as i32;
        self.direct_ready = mode.value as u8 == MODE_DIRECT;
        if let Some(m) = self.modes.get_mut(mode_idx) {
            *m = mode.clone();
        }
        Ok(())
    }

    fn save_mode(&mut self, mode_idx: usize, mode: &Mode) -> Result<()> {
        self.set_mode(mode_idx, mode)?;
        self.apply_mode(mode, true)?;
        self.send(&[0x3f, 0x55])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_from_typical_table() {
        // 3 ARGB headers, 4 fixed LEDs of which 1 is a 12 V header.
        let mut t = [0u8; 60];
        t[0x02] = 3;
        t[0x1b] = 4;
        t[0x1d] = 1;
        let ch = AuraMainboard::channels_from_table(&t, "AULA3-6K75-0207");
        assert_eq!(ch.len(), 4);
        assert_eq!(ch[0].kind, ChannelKind::Fixed);
        assert_eq!((ch[0].effect_channel, ch[0].direct_channel, ch[0].leds), (0, 4, 4));
        assert_eq!((ch[1].effect_channel, ch[1].direct_channel), (1, 0));
        assert_eq!((ch[3].effect_channel, ch[3].direct_channel), (3, 2));
        // AR32 firmware puts the mainboard effect channel after the ARGB headers.
        let ch = AuraMainboard::channels_from_table(&t, "AULA3-AR32-0213");
        assert_eq!(ch[0].effect_channel, 3);
        assert_eq!(ch[1].effect_channel, 0);
    }
}
