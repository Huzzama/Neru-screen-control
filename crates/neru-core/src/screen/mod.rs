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
    // Thermalright coolers with the ChiZhu Tech USBDISPLAY controller all
    // share one USB id; the panel size is not readable from the device, so
    // each resolution is a variant the user picks in Settings → "Use this
    // screen" (320×320 is the default). Sizes per model come from Neru 1.x.
    ScreenModel {
        id: "tr-chizhu-320",
        vendor: "Thermalright",
        name: "ChiZhu USBDISPLAY 320×320 (Frozen Warframe class)",
        width: 320,
        height: 320,
        diagonal_in: 2.4,
        transport: Transport::Usb { vid: 0x87ad, pid: 0x70db },
        protocol: Protocol::Chizhu,
        verified: true,
    },
    ScreenModel {
        id: "tr-chizhu-320x240",
        vendor: "Thermalright",
        name: "ChiZhu USBDISPLAY 320×240 (Frozen Warframe 2.4\", Core Matrix 2.0\")",
        width: 320,
        height: 240,
        diagonal_in: 2.4,
        transport: Transport::Usb { vid: 0x87ad, pid: 0x70db },
        protocol: Protocol::Chizhu,
        verified: false,
    },
    ScreenModel {
        id: "tr-chizhu-480",
        vendor: "Thermalright",
        name: "ChiZhu USBDISPLAY 480×480 (Peerless / Guard / Hyper / Elite Vision 3.95\", Core Vision 2.1\", Frozen Guardian / Frozen Vision 2.88\")",
        width: 480,
        height: 480,
        diagonal_in: 3.95,
        transport: Transport::Usb { vid: 0x87ad, pid: 0x70db },
        protocol: Protocol::Chizhu,
        verified: false,
    },
    ScreenModel {
        id: "tr-chizhu-640x480",
        vendor: "Thermalright",
        name: "ChiZhu USBDISPLAY 640×480 (Mjolnir Vision, Stream Vision 3.5\")",
        width: 640,
        height: 480,
        diagonal_in: 3.5,
        transport: Transport::Usb { vid: 0x87ad, pid: 0x70db },
        protocol: Protocol::Chizhu,
        verified: false,
    },
    ScreenModel {
        id: "tr-chizhu-1280x480",
        vendor: "Thermalright",
        name: "ChiZhu USBDISPLAY 1280×480 (Trofeo Vision 6.98\")",
        width: 1280,
        height: 480,
        diagonal_in: 6.98,
        transport: Transport::Usb { vid: 0x87ad, pid: 0x70db },
        protocol: Protocol::Chizhu,
        verified: false,
    },
    ScreenModel {
        id: "tr-chizhu-2400x1080",
        vendor: "Thermalright",
        name: "ChiZhu USBDISPLAY 2400×1080 (Leviathan / Rainbow / Wonder Vision 6.67\")",
        width: 2400,
        height: 1080,
        diagonal_in: 6.67,
        transport: Transport::Usb { vid: 0x87ad, pid: 0x70db },
        protocol: Protocol::Chizhu,
        verified: false,
    },
];

/// Cooler displays Neru recognises but cannot drive yet (other protocol
/// families; the open-source TRCC Linux project covers them). Reported in
/// the app so nobody wonders why a plugged screen stays blank.
pub const RECOGNISED_ONLY: &[(u16, u16, &str)] = &[
    (0x0416, 0x5302, "Thermalright HID LCD (Trofeo Vision LCD, Assassin Spirit 120 Vision, AS120 / BA120 Vision, Frozen Warframe SE / PRO / 360, Elite Vision, LC5)"),
    (0x0418, 0x5303, "Thermalright HID LCD (TARAN ARMS)"),
    (0x0418, 0x5304, "Thermalright HID LCD (TARAN ARMS)"),
    (0x87cd, 0x70db, "Thermalright SCSI LCD (Frozen Horizon Pro, Frozen Magic Pro, Frozen Vision V2, Core / Elite Vision, AK120 / AX120 / PA120 Digital, Wonder Vision)"),
    (0x0416, 0x5406, "Thermalright SCSI LCD (LC1 / LC2 / LC3 / LC5 pump heads)"),
    (0x0402, 0x3922, "Thermalright SCSI LCD (Frozen Warframe / 360 / SE, Elite Vision 360)"),
    (0x0416, 0x5408, "Thermalright LY LCD (Trofeo Vision 9.16\")"),
    (0x0416, 0x5409, "Thermalright LY LCD"),
    (0x0416, 0x8001, "Thermalright LED segment display (AX120 / PA120 Digital, Peerless Assassin 120 Digital, Assassin X 120R, Phantom Spirit 120 Evo, HR10 2280 Pro Digital, Magic Qube)"),
];

/// Recognised-but-unsupported displays currently plugged in.
pub fn detect_recognised_only() -> Vec<String> {
    let Ok(devices) = rusb::devices() else { return Vec::new() };
    let mut out = Vec::new();
    for d in devices.iter() {
        let Ok(desc) = d.device_descriptor() else { continue };
        if let Some((_, _, what)) = RECOGNISED_ONLY.iter().find(|(v, p, _)| *v == desc.vendor_id() && *p == desc.product_id()) {
            out.push(format!("{what} [{:04x}:{:04x}] — not driven by Neru yet; TRCC Linux supports it", desc.vendor_id(), desc.product_id()));
        }
    }
    out
}

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

/// Shell script (run as root) that installs the udev rules at `rules_file`,
/// makes sure the `i2c-dev` module is loaded now and after reboots, and
/// re-triggers udev so already plugged devices pick the rules up.
pub fn udev_install_script(rules_file: &str) -> String {
    let rules = udev_rules();
    format!(
        "set -e\ncat > {rules_file} <<'EOF'\n{rules}EOF\ncat > {} <<'EOF'\n{}EOF\nmodprobe i2c-dev 2>/dev/null || true; modprobe i2c-piix4 2>/dev/null || true; modprobe i2c-i801 2>/dev/null || true\nudevadm control --reload-rules && udevadm trigger",
        crate::i2c::MODULES_LOAD_FILE,
        crate::i2c::MODULES_LOAD_CONTENT
    )
}

/// udev rules granting the current seat access to every known screen.
pub fn udev_rules() -> String {
    let mut s = String::from(
        "# Neru Screen Control — cooler LCD screens and RGB controllers\n\
         # Install: sudo cp 70-neru-screen-control.rules /etc/udev/rules.d/\n\
         #          sudo udevadm control --reload-rules && sudo udevadm trigger\n\
         # Then unplug and replug the USB cable (or reboot).\n\n",
    );
    let mut done: Vec<(u16, u16)> = Vec::new();
    for m in MODELS {
        let (vid, pid) = m.transport.vid_pid();
        if done.contains(&(vid, pid)) {
            continue;
        }
        done.push((vid, pid));
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
    s.push_str(&crate::rgb::native::udev_rules());
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
