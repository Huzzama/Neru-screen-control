//! ZOTAC GeForce RTX 50 ("Blackwell") graphics cards — the RGB controller
//! sits on the GPU's own I²C bus at address 0x4B.
//!
//! Ported from OpenRGB's `ZotacBlackwellGPUController` (Eder Sánchez,
//! GPL-2.0-or-later): sixteen registers (0x20–0x2F) describe one zone —
//! index, mode, two colours, brightness, speed, direction — written one byte
//! at a time with a 3 ms pause, then a commit (0x17). The card has no direct
//! per-LED mode; software effects run by rewriting the "Static" colour,
//! which takes ~60 ms per zone, so writes go through a background thread
//! that always sends the newest colour and drops the frames in between.
//!
//! Requires the NVIDIA driver's I²C adapters (`/dev/i2c-*`, module `i2c-dev`).

use std::collections::BTreeMap;
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use tracing::{debug, info, warn};

use super::NativeDevice;
use crate::i2c::{self, Adapter, I2cDev};
use crate::rgb::protocol::{color_mode, mode_flags, Color, Controller, Mode, Zone};
use crate::{Error, Result};

pub const ADDR: u8 = 0x4B;
const REG_FIXED: u8 = 0x20;
const REG_COMMIT: u8 = 0x17;
const REG_VERSION: u8 = 0xF0;
/// Any readable register proves the controller is there.
const REG_PROBE: u8 = 0x10;
const WRITE_DELAY: Duration = Duration::from_micros(3000);
const COMMIT_DELAY: Duration = Duration::from_micros(10_000);

pub const NVIDIA_VENDOR: u16 = 0x10DE;
pub const ZOTAC_SUBVENDOR: u16 = 0x19DA;

/// Cards with a known zone layout: (PCI device, subsystem device, name, zones).
pub const CARDS: &[(u16, u16, &str, &[&str])] = &[
    (0x2C02, 0x1762, "ZOTAC GAMING GeForce RTX 5080 AMP Extreme INFINITY", &["Logo", "Side Bar", "Infinity Mirror"]),
    (0x2B85, 0x1761, "ZOTAC GAMING GeForce RTX 5090 SOLID OC", &["ZOTAC Gaming", "Logo"]),
    (0x2B87, 0x2761, "ZOTAC GAMING GeForce RTX 5090 D SOLID OC", &["ZOTAC Gaming", "Logo"]),
];

/// Every RTX 50 GPU id; other ZOTAC boards with one of these get probed with
/// a generic three-zone layout.
pub const RTX50_DEVICES: &[(u16, &str)] = &[
    (0x2D05, "RTX 5060"),
    (0x2D04, "RTX 5060 Ti"),
    (0x2F04, "RTX 5070"),
    (0x2C05, "RTX 5070 Ti"),
    (0x2C02, "RTX 5080"),
    (0x2B85, "RTX 5090"),
    (0x2B87, "RTX 5090 D"),
    (0x2B8C, "RTX 5090 D"),
];

/// Hardware modes: (name, register value, flags, colour mode).
const MODES: &[(&str, i32, u32, u32)] = &[
    ("Static", 0x01, mode_flags::AUTOMATIC_SAVE | mode_flags::HAS_BRIGHTNESS | mode_flags::HAS_PER_LED_COLOR, color_mode::PER_LED),
    ("Breathe", 0x02, mode_flags::AUTOMATIC_SAVE | mode_flags::HAS_BRIGHTNESS | mode_flags::HAS_SPEED | mode_flags::HAS_PER_LED_COLOR, color_mode::PER_LED),
    ("Fade", 0x03, mode_flags::AUTOMATIC_SAVE | mode_flags::HAS_SPEED, color_mode::NONE),
    ("Wink", 0x04, mode_flags::AUTOMATIC_SAVE | mode_flags::HAS_BRIGHTNESS | mode_flags::HAS_SPEED | mode_flags::HAS_PER_LED_COLOR, color_mode::PER_LED),
    ("Glide", 0x08, mode_flags::AUTOMATIC_SAVE | mode_flags::HAS_BRIGHTNESS | mode_flags::HAS_SPEED | mode_flags::HAS_DIRECTION_LR | mode_flags::HAS_PER_LED_COLOR, color_mode::PER_LED),
    ("Prism", 0x09, mode_flags::AUTOMATIC_SAVE | mode_flags::HAS_SPEED | mode_flags::HAS_DIRECTION_LR, color_mode::NONE),
    ("Bokeh", 0x0A, mode_flags::AUTOMATIC_SAVE | mode_flags::HAS_BRIGHTNESS | mode_flags::HAS_SPEED | mode_flags::HAS_PER_LED_COLOR, color_mode::PER_LED),
    ("Beacon", 0x0B, mode_flags::AUTOMATIC_SAVE | mode_flags::HAS_BRIGHTNESS | mode_flags::HAS_SPEED | mode_flags::HAS_PER_LED_COLOR, color_mode::PER_LED),
    ("Tandem", 0x18, mode_flags::AUTOMATIC_SAVE | mode_flags::HAS_BRIGHTNESS | mode_flags::HAS_SPEED | mode_flags::HAS_PER_LED_COLOR, color_mode::PER_LED),
    ("Tidal", 0x19, mode_flags::AUTOMATIC_SAVE | mode_flags::HAS_BRIGHTNESS | mode_flags::HAS_SPEED | mode_flags::HAS_DIRECTION_LR | mode_flags::HAS_PER_LED_COLOR, color_mode::PER_LED),
    ("Astra", 0x20, mode_flags::AUTOMATIC_SAVE | mode_flags::HAS_BRIGHTNESS | mode_flags::HAS_SPEED | mode_flags::HAS_PER_LED_COLOR, color_mode::PER_LED),
    ("Cosmic", 0x21, mode_flags::AUTOMATIC_SAVE | mode_flags::HAS_BRIGHTNESS | mode_flags::HAS_SPEED | mode_flags::HAS_PER_LED_COLOR, color_mode::PER_LED),
    ("Volta", 0x22, mode_flags::AUTOMATIC_SAVE | mode_flags::HAS_BRIGHTNESS | mode_flags::HAS_SPEED | mode_flags::HAS_PER_LED_COLOR, color_mode::PER_LED),
];

fn modes() -> Vec<Mode> {
    MODES
        .iter()
        .map(|&(name, value, flags, cm)| Mode {
            name: name.into(),
            value,
            flags,
            speed_min: 0,
            speed_max: 100,
            brightness_min: 0,
            brightness_max: 100,
            colors_min: 0,
            colors_max: 0,
            speed: 20,
            brightness: 100,
            direction: 0,
            color_mode: cm,
            colors: Vec::new(),
        })
        .collect()
}

/// The sixteen zone registers, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ZoneRegs([u8; 16]);

impl ZoneRegs {
    fn new(zone: u8, mode: u8, c1: Color, c2: Color, brightness: u8, speed: u8, direction: u8) -> Self {
        Self([0x00, zone, mode, c1.r, c1.g, c1.b, c2.r, c2.g, c2.b, brightness, speed, direction, 0, 0, 0, 0])
    }
}

enum Job {
    Write { zone: u8, regs: ZoneRegs, reply: Option<Sender<Result<()>>> },
    Stop,
}

/// Owns the bus; runs on its own thread so slow writes never stall effects.
fn writer(mut dev: I2cDev, rx: Receiver<Job>, last_error: Arc<Mutex<Option<String>>>) {
    let mut last_written: BTreeMap<u8, ZoneRegs> = BTreeMap::new();
    while let Ok(first) = rx.recv() {
        // Coalesce everything queued: only the newest registers per zone matter.
        let mut batch: BTreeMap<u8, (ZoneRegs, Vec<Sender<Result<()>>>)> = BTreeMap::new();
        let mut push = |job: Job| -> bool {
            match job {
                Job::Write { zone, regs, reply } => {
                    let e = batch.entry(zone).or_insert((regs, Vec::new()));
                    e.0 = regs;
                    if let Some(r) = reply {
                        e.1.push(r);
                    }
                    true
                }
                Job::Stop => false,
            }
        };
        if !push(first) {
            return;
        }
        loop {
            match rx.try_recv() {
                Ok(job) => {
                    if !push(job) {
                        return;
                    }
                }
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        for (zone, (regs, replies)) in batch {
            let res = if last_written.get(&zone) == Some(&regs) && replies.is_empty() {
                Ok(())
            } else {
                let r = write_zone(&mut dev, regs);
                if r.is_ok() {
                    last_written.insert(zone, regs);
                } else {
                    last_written.remove(&zone);
                }
                r
            };
            if let Err(e) = &res {
                warn!(error = %e, zone, "ZOTAC GPU write failed");
                if let Ok(mut l) = last_error.lock() {
                    *l = Some(e.to_string());
                }
            }
            for r in replies {
                let _ = r.send(res.as_ref().map(|_| ()).map_err(|e| Error::Protocol(e.to_string())));
            }
        }
    }
}

fn write_zone(dev: &mut I2cDev, regs: ZoneRegs) -> Result<()> {
    for (i, v) in regs.0.iter().enumerate() {
        dev.write_byte_data(ADDR, REG_FIXED + i as u8, *v)?;
        thread::sleep(WRITE_DELAY);
    }
    dev.write_byte_data(ADDR, REG_COMMIT, 0x01)?;
    thread::sleep(COMMIT_DELAY);
    Ok(())
}

/// The firmware/board version: a 32-byte ASCII string at 0xF0.
fn read_version(dev: &mut I2cDev) -> String {
    let Ok(raw) = dev.read_i2c_block_data(ADDR, REG_VERSION, i2c::I2C_SMBUS_BLOCK_MAX) else {
        return "unknown".into();
    };
    let mut len = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
    while len > 0 && (raw[len - 1] == 0xFF || raw[len - 1] == b' ') {
        len -= 1;
    }
    if len == 0 || raw[..len].iter().any(|&b| !(0x20..=0x7E).contains(&b)) {
        return "unknown".into();
    }
    String::from_utf8_lossy(&raw[..len]).to_string()
}

/// Whether a ZOTAC RTX 50 card is on the PCI bus at all (used for hints when
/// its I²C adapters cannot be reached).
pub fn card_present() -> bool {
    let Ok(rd) = std::fs::read_dir("/sys/bus/pci/devices") else { return false };
    rd.flatten().any(|e| {
        let p = e.path();
        let hex = |n: &str| std::fs::read_to_string(p.join(n)).ok().and_then(|s| u16::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok());
        hex("vendor") == Some(NVIDIA_VENDOR) && hex("subsystem_vendor") == Some(ZOTAC_SUBVENDOR) && hex("device").map_or(false, |d| RTX50_DEVICES.iter().any(|x| x.0 == d))
    })
}

/// A ZOTAC card found on the bus.
#[derive(Debug, Clone)]
pub struct Found {
    pub adapter: Adapter,
    pub name: String,
    pub zones: Vec<String>,
    /// Layout taken from the table (false = generic guess).
    pub known: bool,
}

/// Probe every NVIDIA I²C adapter that belongs to a ZOTAC RTX 50 card.
/// One entry per card (the first adapter that answers wins).
pub fn find() -> Vec<Found> {
    let mut out: Vec<Found> = Vec::new();
    for a in i2c::adapters() {
        let Some(pci) = a.pci.clone() else { continue };
        if !a.is_nvidia_gpu() || pci.subvendor != ZOTAC_SUBVENDOR {
            continue;
        }
        if out.iter().any(|f| f.adapter.pci.as_ref().map(|p| &p.slot) == Some(&pci.slot)) {
            continue;
        }
        let (name, zones, known) = match CARDS.iter().find(|c| c.0 == pci.device && c.1 == pci.subdevice) {
            Some(&(_, _, name, zones)) => (name.to_string(), zones.iter().map(|z| z.to_string()).collect(), true),
            None => match RTX50_DEVICES.iter().find(|d| d.0 == pci.device) {
                Some(&(_, gen)) => (
                    format!("ZOTAC GeForce {gen} (sub-device {:04X})", pci.subdevice),
                    vec!["Zone 1".to_string(), "Zone 2".to_string(), "Zone 3".to_string()],
                    false,
                ),
                None => continue,
            },
        };
        let mut dev = match I2cDev::open(&a.dev) {
            Ok(d) => d,
            Err(e) => {
                debug!(dev = %a.dev.display(), error = %e, "cannot open i2c adapter");
                continue;
            }
        };
        match dev.read_byte_data(ADDR, REG_PROBE) {
            Ok(_) => {
                debug!(dev = %a.dev.display(), %name, "ZOTAC RGB controller answered");
                out.push(Found { adapter: a, name, zones, known });
            }
            Err(e) => debug!(dev = %a.dev.display(), error = %e, "no ZOTAC controller on this adapter"),
        }
    }
    out
}

pub struct ZotacGpu {
    name: String,
    version: String,
    location: String,
    zone_names: Vec<String>,
    known_layout: bool,
    modes: Vec<Mode>,
    active_mode: usize,
    colors: Vec<Color>,
    tx: Sender<Job>,
    handle: Option<thread::JoinHandle<()>>,
    last_error: Arc<Mutex<Option<String>>>,
}

impl ZotacGpu {
    pub fn open(found: &Found) -> Result<Self> {
        let mut dev = I2cDev::open(&found.adapter.dev)?;
        dev.read_byte_data(ADDR, REG_PROBE)?;
        let version = read_version(&mut dev);
        let location = format!("I2C: {} ({}), address 0x{ADDR:02X}", found.adapter.dev.display(), found.adapter.name);
        info!(name = %found.name, %version, location = %location, "ZOTAC GPU RGB controller opened");
        let (tx, rx) = channel();
        let last_error = Arc::new(Mutex::new(None));
        let le = last_error.clone();
        let handle = thread::Builder::new()
            .name("neru-zotac-i2c".into())
            .spawn(move || writer(dev, rx, le))
            .map_err(Error::Io)?;
        Ok(Self {
            name: found.name.clone(),
            version,
            location,
            zone_names: found.zones.clone(),
            known_layout: found.known,
            modes: modes(),
            active_mode: 0,
            colors: vec![Color::new(0xff, 0xc2, 0x0e); found.zones.len()],
            tx,
            handle: Some(handle),
            last_error,
        })
    }

    fn regs_for(&self, zone: usize) -> ZoneRegs {
        let m = &self.modes[self.active_mode];
        let c1 = if m.color_mode == color_mode::PER_LED { self.colors[zone] } else { Color::BLACK };
        ZoneRegs::new(zone as u8, m.value as u8, c1, Color::BLACK, m.brightness.min(100) as u8, m.speed.min(100) as u8, m.direction.min(1) as u8)
    }

    fn take_error(&self) -> Result<()> {
        match self.last_error.lock().ok().and_then(|mut l| l.take()) {
            Some(e) => Err(Error::Protocol(e)),
            None => Ok(()),
        }
    }

    /// Queue a zone write and, when `wait`, block for its result.
    fn send_zone(&self, zone: usize, wait: bool) -> Result<()> {
        let regs = self.regs_for(zone);
        if wait {
            let (reply_tx, reply_rx) = channel();
            self.tx.send(Job::Write { zone: zone as u8, regs, reply: Some(reply_tx) }).map_err(|_| Error::Protocol("ZOTAC writer stopped".into()))?;
            reply_rx.recv_timeout(Duration::from_secs(3)).map_err(|_| Error::Timeout("ZOTAC GPU write".into()))?
        } else {
            self.tx.send(Job::Write { zone: zone as u8, regs, reply: None }).map_err(|_| Error::Protocol("ZOTAC writer stopped".into()))?;
            self.take_error()
        }
    }

    fn send_all(&self, wait: bool) -> Result<()> {
        for z in 0..self.zone_names.len() {
            self.send_zone(z, wait)?;
        }
        Ok(())
    }
}

impl Drop for ZotacGpu {
    fn drop(&mut self) {
        let _ = self.tx.send(Job::Stop);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl NativeDevice for ZotacGpu {
    fn location(&self) -> String {
        self.location.clone()
    }

    fn describe(&self) -> Controller {
        let zones = self
            .zone_names
            .iter()
            .enumerate()
            .map(|(i, n)| Zone { name: n.clone(), kind: 0, leds_min: 1, leds_max: 1, leds_count: 1, start_idx: i as u32, ..Default::default() })
            .collect();
        Controller {
            index: 0,
            kind: 2,
            name: self.name.clone(),
            vendor: "ZOTAC".into(),
            description: if self.known_layout {
                "ZOTAC RTX 50 series GPU (native)".into()
            } else {
                "ZOTAC RTX 50 series GPU (native, zone names guessed — report your card)".into()
            },
            version: self.version.clone(),
            serial: String::new(),
            location: self.location.clone(),
            active_mode: self.active_mode as i32,
            modes: self.modes.clone(),
            zones,
            leds: self.zone_names.iter().map(|n| format!("{n} LED")).collect(),
            colors: self.colors.clone(),
            ..Default::default()
        }
    }

    fn resize_zone(&mut self, _zone: u32, _leds: u32) -> Result<()> {
        Err(Error::InvalidValue("ZOTAC zones have a fixed size".into()))
    }

    fn set_direct(&mut self) -> Result<()> {
        // No direct mode: software effects rewrite the Static colour.
        self.active_mode = 0;
        self.modes[0].brightness = 100;
        Ok(())
    }

    fn zone_leds(&mut self, zone: u32, colors: &[Color]) -> Result<()> {
        let z = zone as usize;
        if z >= self.zone_names.len() {
            return Err(Error::InvalidValue(format!("no zone {zone}")));
        }
        let Some(c) = colors.first() else { return Ok(()) };
        if self.colors[z] == *c {
            return self.take_error();
        }
        self.colors[z] = *c;
        self.send_zone(z, false)
    }

    fn set_mode(&mut self, mode_idx: usize, mode: &Mode) -> Result<()> {
        let m = self.modes.get_mut(mode_idx).ok_or_else(|| Error::InvalidValue(format!("no mode {mode_idx}")))?;
        m.brightness = mode.brightness.min(100);
        m.speed = mode.speed.min(100);
        m.direction = mode.direction;
        if mode.color_mode == color_mode::MODE_SPECIFIC {
            if let Some(c) = mode.colors.first() {
                for z in self.colors.iter_mut() {
                    *z = *c;
                }
            }
        }
        self.active_mode = mode_idx;
        self.send_all(true)
    }

    fn save_mode(&mut self, mode_idx: usize, mode: &Mode) -> Result<()> {
        // Every mode write is stored by the firmware.
        self.set_mode(mode_idx, mode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zone_registers_follow_the_layout() {
        let r = ZoneRegs::new(2, 0x09, Color::new(1, 2, 3), Color::new(4, 5, 6), 100, 20, 1);
        assert_eq!(r.0, [0, 2, 0x09, 1, 2, 3, 4, 5, 6, 100, 20, 1, 0, 0, 0, 0]);
    }

    #[test]
    fn modes_have_static_first_with_per_led_colour() {
        let m = modes();
        assert_eq!(m[0].name, "Static");
        assert!(m[0].has(mode_flags::HAS_PER_LED_COLOR));
        assert!(m.iter().any(|x| x.name == "Prism" && x.has(mode_flags::HAS_DIRECTION_LR)));
    }

    #[test]
    fn known_cards_are_rtx50() {
        for (dev, _, _, zones) in CARDS {
            assert!(RTX50_DEVICES.iter().any(|d| d.0 == *dev));
            assert!(!zones.is_empty());
        }
    }
}
