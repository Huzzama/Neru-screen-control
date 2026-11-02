//! Cooler LCD screens.
//!
//! Every supported screen implements [`Screen`]. Drivers receive a plain RGB
//! image of the screen's native size and do their own encoding (JPEG for the
//! Thermaltake "BY" screens, RGB565 for the Thermalright/ChiZhu screens).

pub mod thermalright;
pub mod thermaltake_by;

use std::fmt;

use image::RgbImage;
use serde::{Deserialize, Serialize};

use crate::Result;

/// How the screen is attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Transport {
    /// USB HID (hidraw), 1024-byte reports.
    Hid { vid: u16, pid: u16 },
    /// Raw USB bulk + control transfers (libusb).
    Usb { vid: u16, pid: u16 },
}

impl Transport {
    pub fn vid_pid(&self) -> (u16, u16) {
        match *self {
            Transport::Hid { vid, pid } | Transport::Usb { vid, pid } => (vid, pid),
        }
    }
}

/// Which protocol family a model speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    /// Thermaltake "BY" protocol: HTTP-like commands + JPEG slices over HID.
    ThermaltakeBy,
    /// ChiZhu Tech USBDISPLAY (Thermalright coolers): RGB565 framebuffer over USB bulk.
    Chizhu,
}

/// Static description of a screen model.
#[derive(Debug, Clone, Serialize)]
pub struct ScreenModel {
    /// Stable identifier, e.g. `tt-th-v3-ultra`.
    pub id: &'static str,
    pub vendor: &'static str,
    pub name: &'static str,
    pub width: u32,
    pub height: u32,
    pub diagonal_in: f32,
    pub transport: Transport,
    pub protocol: Protocol,
    /// Whether this exact model was confirmed on hardware by the project.
    pub verified: bool,
}

impl ScreenModel {
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

/// Every model Neru knows about.
pub const MODELS: &[ScreenModel] = &[
    ScreenModel {
        id: "tt-th-v3-ultra",
        vendor: "Thermaltake",
        name: "TH360 / TH420 V3 Ultra ARGB Sync (3.95\" 720×720)",
        width: 720,
        height: 720,
        diagonal_in: 3.95,
        transport: Transport::Hid { vid: 0x264a, pid: 0x2348 },
        protocol: Protocol::ThermaltakeBy,
        verified: true,
    },
    ScreenModel {
        id: "tt-lcd-panel-6in",
        vendor: "Thermaltake",
        name: "LCD Panel Kit 6\" (View 600 TG, 1110×540)",
        width: 1110,
        height: 540,
        diagonal_in: 6.0,
        transport: Transport::Hid { vid: 0x264a, pid: 0x2347 },
        protocol: Protocol::ThermaltakeBy,
        verified: false,
    },
    ScreenModel {
        id: "tr-chizhu-320",
        vendor: "Thermalright",
        name: "ChiZhu USBDISPLAY (Frozen Warframe class, 320×320)",
        width: 320,
        height: 320,
        diagonal_in: 2.4,
        transport: Transport::Usb { vid: 0x87ad, pid: 0x70db },
        protocol: Protocol::Chizhu,
        verified: true,
    },
];

pub fn model_by_id(id: &str) -> Option<&'static ScreenModel> {
    MODELS.iter().find(|m| m.id == id)
}

/// What a connected screen can do.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Capabilities {
    /// Hardware brightness (0–100).
    pub brightness: bool,
    /// Rotations the hardware accepts (empty → rotation is done in software).
    pub hw_rotations: &'static [u16],
}

/// State reported by the screen after `connect()`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScreenStatus {
    pub firmware: Option<String>,
    pub brightness: Option<u8>,
    pub rotation: Option<u16>,
    /// Seconds without frames before the screen falls back to its own content.
    pub idle_timeout_s: Option<u32>,
    pub serial: Option<String>,
    /// Raw status document when the device provides one.
    pub raw: Option<serde_json::Value>,
}

/// A screen found on the system but not opened yet.
#[derive(Debug, Clone, Serialize)]
pub struct DetectedScreen {
    pub model: ScreenModel,
    /// `/dev/hidrawN` for HID screens, `bus-addr` for USB screens.
    pub location: String,
    /// Whether the current user can open it right now.
    pub accessible: bool,
}

impl fmt::Display for DetectedScreen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (vid, pid) = self.model.transport.vid_pid();
        write!(
            f,
            "{} {} [{vid:04x}:{pid:04x}] at {}{}",
            self.model.vendor,
            self.model.name,
            self.location,
            if self.accessible { "" } else { " (no permission)" }
        )
    }
}

/// A driver for one physical screen.
pub trait Screen: Send {
    fn model(&self) -> &ScreenModel;
    fn capabilities(&self) -> Capabilities;

    /// Handshake with the device. Safe to call again after a reconnect.
    fn connect(&mut self) -> Result<ScreenStatus>;

    /// Push one frame. `frame` must be `model().width × model().height`
    /// (drivers resize anything else, at a cost).
    fn show(&mut self, frame: &RgbImage) -> Result<()>;

    fn set_brightness(&mut self, value: u8) -> Result<()>;

    /// Rotation in degrees (0/90/180/270). Hardware if supported, else software.
    fn set_rotation(&mut self, degrees: u16) -> Result<()>;

    /// Frames are expected at least this often or the device goes idle.
    fn keepalive(&self) -> Option<std::time::Duration> {
        None
    }

    /// JPEG quality for screens that take compressed frames (no-op otherwise).
    fn set_jpeg_quality(&mut self, _quality: u8) {}
}

/// Scan the system for every known screen.
pub fn detect() -> Vec<DetectedScreen> {
    let mut found = Vec::new();
    for model in MODELS {
        match model.transport {
            Transport::Hid { vid, pid } => {
                for node in crate::hidraw::find(vid, pid) {
                    let accessible = std::fs::OpenOptions::new()
                        .read(true)
                        .write(true)
                        .open(&node.path)
                        .is_ok();
                    found.push(DetectedScreen {
                        model: model.clone(),
                        location: node.path.display().to_string(),
                        accessible,
                    });
                }
            }
            Transport::Usb { vid, pid } => {
                for (location, accessible) in thermalright::locate(vid, pid) {
                    found.push(DetectedScreen {
                        model: model.clone(),
                        location,
                        accessible,
                    });
                }
            }
        }
    }
    found
}

/// Open a driver for a detected screen.
pub fn open(detected: &DetectedScreen) -> Result<Box<dyn Screen>> {
    let model = model_by_id(detected.model.id)
        .ok_or_else(|| crate::Error::DeviceNotFound(detected.model.id.to_string()))?;
    match model.protocol {
        Protocol::ThermaltakeBy => Ok(Box::new(thermaltake_by::ThermaltakeBy::open(
            model,
            std::path::Path::new(&detected.location),
        )?)),
        Protocol::Chizhu => Ok(Box::new(thermalright::Chizhu::open(model)?)),
    }
}

/// Detect and open the first accessible screen (or the one whose model id matches).
pub fn open_first(model_id: Option<&str>) -> Result<Box<dyn Screen>> {
    let detected = detect();
    let pick = detected
        .iter()
        .find(|d| model_id.map_or(true, |id| d.model.id == id))
        .ok_or(crate::Error::NoDevice)?;
    if !pick.accessible {
        return Err(crate::Error::Permission(pick.location.clone().into()));
    }
    open(pick)
}

/// udev rules granting the current seat access to every known screen.
pub fn udev_rules() -> String {
    let mut s = String::from(
        "# Neru Screen Control — cooler LCD screens\n\
         # Install: sudo cp 70-neru-screen-control.rules /etc/udev/rules.d/\n\
         #          sudo udevadm control --reload-rules && sudo udevadm trigger\n\
         # Then unplug and replug the USB cable (or reboot).\n\n",
    );
    for m in MODELS {
        let (vid, pid) = m.transport.vid_pid();
        s.push_str(&format!("# {} {}\n", m.vendor, m.name));
        match m.transport {
            Transport::Hid { .. } => s.push_str(&format!(
                "SUBSYSTEM==\"hidraw\", ATTRS{{idVendor}}==\"{vid:04x}\", ATTRS{{idProduct}}==\"{pid:04x}\", MODE=\"0660\", TAG+=\"uaccess\"\n"
            )),
            Transport::Usb { .. } => s.push_str(&format!(
                "SUBSYSTEM==\"usb\", ATTRS{{idVendor}}==\"{vid:04x}\", ATTRS{{idProduct}}==\"{pid:04x}\", MODE=\"0660\", TAG+=\"uaccess\"\n"
            )),
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_ids_are_unique() {
        let mut ids: Vec<_> = MODELS.iter().map(|m| m.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), MODELS.len());
    }

    #[test]
    fn packaged_udev_rules_are_current() {
        // `neru udev > packaging/shared/70-neru-screen-control.rules` after editing MODELS.
        let packaged = include_str!("../../../../packaging/shared/70-neru-screen-control.rules");
        assert_eq!(packaged, udev_rules(), "packaging/shared/70-neru-screen-control.rules is stale");
    }

    #[test]
    fn udev_rules_cover_every_model() {
        let rules = udev_rules();
        for m in MODELS {
            let (vid, pid) = m.transport.vid_pid();
            assert!(rules.contains(&format!("\"{vid:04x}\"")));
            assert!(rules.contains(&format!("\"{pid:04x}\"")));
        }
    }
}
