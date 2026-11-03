//! Neru's own RGB drivers — no OpenRGB needed for these devices.
//!
//! Each driver exposes its device in the same shape as an OpenRGB controller
//! (modes, zones, LEDs) so the RGB engine and the UI treat both sources alike.
//! Drivers are ported from OpenRGB (GPL-2.0-or-later) unless noted.
//!
//! | Driver | Devices | Bus |
//! |---|---|---|
//! | [`aura_usb`]  | ASUS Aura USB motherboards (fixed LEDs + ARGB headers) | HID |
//! | [`zotac_gpu`] | ZOTAC GeForce RTX 50 cards | I²C (`/dev/i2c-*`) |
//! | [`ene_smbus`] | ENE / ASUS Aura SMBus: RGB RAM, older ASUS boards, ASUS graphics cards | SMBus |
//! | [`corsair_node`] | Corsair Lighting Node Pro / Core, Commander Pro, LS100, LT100, 1000D, SPEC-OMEGA | HID |
//! | [`nzxt_hue2`] | NZXT Hue 2, Smart Device V2, RGB & Fan Controller, Kraken X3 / Elite ring | HID |
//! | [`asrock_polychrome`] | ASRock Polychrome USB motherboards, DeskMini strip | HID |

pub mod asrock_polychrome;
pub mod aura_usb;
pub mod corsair_node;
pub mod ene_smbus;
pub mod nzxt_hue2;
pub mod zotac_gpu;

use std::collections::HashMap;

use tracing::{info, warn};

use super::protocol::{Color, Controller, Mode};
use crate::Result;

/// One device driven natively.
pub trait NativeDevice: Send {
    /// Stable location string (`HID: /dev/hidrawN`, `I2C: …`).
    fn location(&self) -> String;
    fn describe(&self) -> Controller;
    /// Set the LED count of a resizable zone (ARGB headers).
    fn resize_zone(&mut self, zone: u32, leds: u32) -> Result<()>;
    /// Switch to per-LED (direct) control.
    fn set_direct(&mut self) -> Result<()>;
    fn zone_leds(&mut self, zone: u32, colors: &[Color]) -> Result<()>;
    fn set_mode(&mut self, mode_idx: usize, mode: &Mode) -> Result<()>;
    /// Apply and persist to the device's flash (where supported).
    fn save_mode(&mut self, mode_idx: usize, mode: &Mode) -> Result<()>;
    /// Called about once a second; devices that fall back to their own
    /// animation without traffic (Corsair Lighting Node) send a commit here.
    fn keepalive(&mut self) -> Result<()> {
        Ok(())
    }
}

/// Every native device found on the system.
#[derive(Default)]
pub struct NativeHub {
    devices: Vec<Box<dyn NativeDevice>>,
    pub errors: Vec<String>,
}

impl NativeHub {
    /// Probe all drivers. `zone_sizes` maps `"<controller key>#<zone>"` to an LED count.
    pub fn scan(&mut self, zone_sizes: &HashMap<String, u32>) {
        self.devices.clear();
        self.errors.clear();
        for node in aura_usb::find() {
            match aura_usb::AuraMainboard::open(&node) {
                Ok(d) => self.devices.push(Box::new(d)),
                Err(e) => {
                    warn!(node = %node.display(), error = %e, "Aura mainboard not usable");
                    self.errors.push(format!("ASUS Aura ({}): {e}", node.display()));
                }
            }
        }
        let zotac = zotac_gpu::find();
        let zotac_found = !zotac.is_empty();
        for found in zotac {
            match zotac_gpu::ZotacGpu::open(&found) {
                Ok(d) => self.devices.push(Box::new(d)),
                Err(e) => {
                    warn!(dev = %found.adapter.dev.display(), error = %e, "ZOTAC GPU not usable");
                    self.errors.push(format!("{} ({}): {e}", found.name, found.adapter.dev.display()));
                }
            }
        }
        for (node, name, channels) in corsair_node::find() {
            match corsair_node::CorsairNode::open(&node, name, channels) {
                Ok(d) => self.devices.push(Box::new(d)),
                Err(e) => {
                    warn!(node = %node.path.display(), error = %e, "Corsair lighting node not usable");
                    self.errors.push(format!("{name} ({}): {e}", node.path.display()));
                }
            }
        }
        for (node, d) in nzxt_hue2::find() {
            match nzxt_hue2::NzxtHue2::open(&node, d) {
                Ok(dev) => self.devices.push(Box::new(dev)),
                Err(e) => {
                    warn!(node = %node.path.display(), error = %e, "NZXT device not usable");
                    self.errors.push(format!("{} ({}): {e}", d.1, node.path.display()));
                }
            }
        }
        for (node, name) in asrock_polychrome::find() {
            match asrock_polychrome::Polychrome::open(&node, name) {
                Ok(dev) => self.devices.push(Box::new(dev)),
                Err(e) => {
                    warn!(node = %node.path.display(), error = %e, "ASRock Polychrome not usable");
                    self.errors.push(format!("{name} ({}): {e}", node.path.display()));
                }
            }
        }
        for found in ene_smbus::find() {
            match ene_smbus::EneDevice::open(&found) {
                Ok(d) => self.devices.push(Box::new(d)),
                Err(e) => {
                    warn!(dev = %found.adapter.dev.display(), addr = found.addr, error = %e, "ENE SMBus device not usable");
                    self.errors.push(format!("{} (0x{:02X} on {}): {e}", found.name, found.addr, found.adapter.dev.display()));
                }
            }
        }
        if !zotac_found && zotac_gpu::card_present() {
            if !crate::i2c::i2c_dev_available() {
                self.errors.push("ZOTAC GPU: the i2c-dev kernel module is not loaded (Settings → Device access → Install rules, or `sudo modprobe i2c-dev`)".into());
            } else if !crate::i2c::adapters().iter().any(|a| a.is_nvidia_gpu()) {
                self.errors.push("ZOTAC GPU: no NVIDIA I2C adapters — the proprietary NVIDIA driver (or nvidia-open) is needed; nouveau does not expose them".into());
            } else if crate::i2c::adapters().iter().filter(|a| a.is_nvidia_gpu()).all(|a| crate::i2c::I2cDev::open(&a.dev).is_err()) {
                self.errors.push("ZOTAC GPU: no access to /dev/i2c-* — install the udev rules (Settings → Device access), then log out and in".into());
            } else {
                self.errors.push("ZOTAC GPU found on the PCI bus but its RGB controller did not answer at 0x4B; run `neru rgb i2c` and open an issue with the output".into());
            }
        }
        self.apply_sizes(zone_sizes);
        info!(devices = self.devices.len(), "native RGB devices");
    }

    pub fn apply_sizes(&mut self, zone_sizes: &HashMap<String, u32>) {
        let keys: Vec<String> = self.controllers().iter().map(|c| c.key()).collect();
        for (d, key) in self.devices.iter_mut().zip(keys) {
            let c = d.describe();
            for (zi, z) in c.zones.iter().enumerate() {
                if z.leds_max <= z.leds_min {
                    continue;
                }
                if let Some(&n) = zone_sizes.get(&format!("{key}#{zi}")) {
                    if n != z.leds_count {
                        if let Err(e) = d.resize_zone(zi as u32, n) {
                            warn!(error = %e, "zone resize failed");
                        }
                    }
                }
            }
        }
    }

    pub fn controllers(&self) -> Vec<Controller> {
        let mut list: Vec<Controller> = self
            .devices
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let mut c = d.describe();
                c.index = i as u32;
                c.native = true;
                c
            })
            .collect();
        Controller::assign_ordinals(&mut list);
        list
    }

    pub fn device(&mut self, index: u32) -> Option<&mut Box<dyn NativeDevice>> {
        self.devices.get_mut(index as usize)
    }

    /// Give every device a chance to keep its link alive.
    pub fn keepalive(&mut self) {
        for d in self.devices.iter_mut() {
            if let Err(e) = d.keepalive() {
                warn!(device = %d.location(), error = %e, "keepalive failed");
            }
        }
    }

    pub fn locations(&self) -> Vec<String> {
        self.devices.iter().map(|d| d.location()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.devices.is_empty()
    }
}

/// Every HID (vendor, product) a native driver opens.
pub fn hid_ids() -> Vec<(u16, u16, &'static str)> {
    let mut v: Vec<(u16, u16, &'static str)> = aura_usb::MAINBOARD_PIDS.iter().map(|&(pid, _)| (aura_usb::VID, pid, "ASUS Aura USB motherboard")).collect();
    v.extend(corsair_node::DEVICES.iter().map(|&(pid, name, _)| (corsair_node::VID, pid, name)));
    v.extend(nzxt_hue2::DEVICES.iter().map(|&(pid, name, _, _, _)| (nzxt_hue2::VID, pid, name)));
    v.extend(asrock_polychrome::PIDS.iter().map(|&(pid, name)| (asrock_polychrome::VID, pid, name)));
    v
}

/// udev rules for every device a native driver can open.
pub fn udev_rules() -> String {
    let mut s = String::from("\n# RGB controllers driven natively by Neru\n");
    let mut last = "";
    for (vid, pid, name) in hid_ids() {
        if name != last {
            s.push_str(&format!("# {name}\n"));
            last = name;
        }
        s.push_str(&format!("SUBSYSTEM==\"hidraw\", ATTRS{{idVendor}}==\"{vid:04x}\", ATTRS{{idProduct}}==\"{pid:04x}\", MODE=\"0660\", TAG+=\"uaccess\"\n"));
    }
    s.push_str(&crate::i2c::udev_rules());
    s
}
