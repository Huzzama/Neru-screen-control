//! Thermalright cooler screens — ChiZhu Tech "USBDISPLAY" (87ad:70db).
//!
//! Port of the driver that shipped in Neru 1.x (Python/pyusb), confirmed on
//! hardware with a 320×320 panel. Per-frame USB sequence:
//!
//! 1. control transfer `CMD_START` (`01 00`)
//! 2. bulk OUT `CMD_TRIG` (4 zero bytes)
//! 3. bulk OUT 208-byte frame header + pixels (512-byte chunks)
//! 4. control transfer `CMD_COMMIT` (`09 00`)
//!
//! Control transfers use `bmRequestType = 0x40` and whichever `bRequest` of
//! `0x01 / 0x00 / 0x02` the firmware accepts first. On the first connection
//! `CMD_SYNC` (`33 FF` + 62 zeros) is sent three times to wake the panel.
//!
//! The panel is a dumb framebuffer: orientation (rotation / flip) and the
//! "framebuffer offset" calibration are applied in software before encoding.

use std::time::Duration;

use image::RgbImage;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use super::{Capabilities, Screen, ScreenModel, ScreenStatus};
use crate::{Error, Result};

const EP_OUT: u8 = 0x01;
const CHUNK: usize = 512;
const CTRL_TIMEOUT: Duration = Duration::from_secs(2);
const BULK_TIMEOUT: Duration = Duration::from_secs(10);

const CMD_START: [u8; 2] = [0x01, 0x00];
const CMD_COMMIT: [u8; 2] = [0x09, 0x00];
const CMD_TRIG: [u8; 4] = [0; 4];

fn cmd_sync() -> [u8; 64] {
    let mut c = [0u8; 64];
    c[0] = 0x33;
    c[1] = 0xff;
    c
}

/// Pixel layout the panel expects. `Rgb565Be` is the confirmed one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PixelFormat {
    #[default]
    Rgb565Be,
    Rgb565Le,
    Bgr565Be,
    Bgr565Le,
    Grb565Le,
    Rgb888,
    Bgr888,
}

impl PixelFormat {
    pub const ALL: &'static [PixelFormat] = &[
        PixelFormat::Rgb565Be,
        PixelFormat::Rgb565Le,
        PixelFormat::Bgr565Be,
        PixelFormat::Bgr565Le,
        PixelFormat::Grb565Le,
        PixelFormat::Rgb888,
        PixelFormat::Bgr888,
    ];

    pub fn bytes_per_pixel(self) -> usize {
        match self {
            PixelFormat::Rgb888 | PixelFormat::Bgr888 => 3,
            _ => 2,
        }
    }
}

/// Software calibration (what the old "Calibration" tab adjusted).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Calibration {
    /// Counter-clockwise rotation in degrees: 0 / 90 / 180 / 270.
    pub rotation: u16,
    pub flip_y: bool,
    /// Row shift (wraps around), fixes panels that start scanning at row N.
    pub offset_y: i32,
    /// Column shift (wraps around).
    pub offset_x: i32,
    pub pixel_format: PixelFormat,
}

impl Default for Calibration {
    fn default() -> Self {
        Self {
            rotation: 0,
            flip_y: false,
            offset_y: 0,
            offset_x: 0,
            pixel_format: PixelFormat::Rgb565Be,
        }
    }
}

/// The 208-byte header that precedes every frame (values recovered from a
/// Windows USB capture).
pub fn frame_header(width: u32, height: u32, bytes_per_pixel: usize) -> [u8; 0xd0] {
    let mut h = [0u8; 0xd0];
    let put = |h: &mut [u8; 0xd0], off: usize, v: u32| h[off..off + 4].copy_from_slice(&v.to_le_bytes());
    put(&mut h, 0x00, 0x7856_3412);
    put(&mut h, 0x04, 0x0000_0003);
    put(&mut h, 0x08, width);
    put(&mut h, 0x0c, width);
    put(&mut h, 0x10, height);
    put(&mut h, 0x38, 0x0000_0002);
    put(&mut h, 0x3c, width * height * bytes_per_pixel as u32);
    put(&mut h, 0xc4, 0x0000_2000);
    h
}

/// Apply calibration and encode pixels (no header).
pub fn encode_pixels(frame: &RgbImage, width: u32, height: u32, cal: &Calibration) -> Vec<u8> {
    use image::imageops;

    let mut img = if frame.width() == width && frame.height() == height {
        frame.clone()
    } else {
        imageops::resize(frame, width, height, imageops::FilterType::Lanczos3)
    };

    // PIL's rotate(deg) is counter-clockwise; image's rotate90 is clockwise.
    img = match cal.rotation % 360 {
        90 => imageops::rotate270(&img),
        180 => imageops::rotate180(&img),
        270 => imageops::rotate90(&img),
        _ => img,
    };
    // The old driver rotated with expand=False, so the canvas stays width×height.
    if img.width() != width || img.height() != height {
        img = imageops::resize(&img, width, height, imageops::FilterType::Triangle);
    }
    if cal.flip_y {
        imageops::flip_vertical_in_place(&mut img);
    }
    if cal.offset_y != 0 || cal.offset_x != 0 {
        img = roll(&img, cal.offset_x, cal.offset_y);
    }

    let (w, h) = (img.width() as usize, img.height() as usize);
    let raw = img.as_raw();
    let mut out = Vec::with_capacity(w * h * cal.pixel_format.bytes_per_pixel());
    for px in raw.chunks_exact(3) {
        let (r, g, b) = (px[0] as u16, px[1] as u16, px[2] as u16);
        match cal.pixel_format {
            PixelFormat::Rgb565Be => out.extend_from_slice(&pack565(r, g, b).to_be_bytes()),
            PixelFormat::Rgb565Le => out.extend_from_slice(&pack565(r, g, b).to_le_bytes()),
            PixelFormat::Bgr565Be => out.extend_from_slice(&pack565(b, g, r).to_be_bytes()),
            PixelFormat::Bgr565Le => out.extend_from_slice(&pack565(b, g, r).to_le_bytes()),
            PixelFormat::Grb565Le => out.extend_from_slice(&pack565(g, r, b).to_le_bytes()),
            PixelFormat::Rgb888 => out.extend_from_slice(&[r as u8, g as u8, b as u8]),
            PixelFormat::Bgr888 => out.extend_from_slice(&[b as u8, g as u8, r as u8]),
        }
    }
    out
}

#[inline]
fn pack565(hi: u16, mid: u16, lo: u16) -> u16 {
    ((hi >> 3) << 11) | ((mid >> 2) << 5) | (lo >> 3)
}

/// numpy.roll equivalent: shift rows by `dy` and columns by `dx`, wrapping.
fn roll(img: &RgbImage, dx: i32, dy: i32) -> RgbImage {
    let (w, h) = (img.width() as i64, img.height() as i64);
    let mut out = RgbImage::new(img.width(), img.height());
    for y in 0..h {
        for x in 0..w {
            let sx = (x - dx as i64).rem_euclid(w) as u32;
            let sy = (y - dy as i64).rem_euclid(h) as u32;
            out.put_pixel(x as u32, y as u32, *img.get_pixel(sx, sy));
        }
    }
    out
}

/// Find USB devices with this VID:PID → (location, accessible).
pub fn locate(vid: u16, pid: u16) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let Ok(devices) = rusb::devices() else {
        return out;
    };
    for dev in devices.iter() {
        let Ok(desc) = dev.device_descriptor() else { continue };
        if desc.vendor_id() != vid || desc.product_id() != pid {
            continue;
        }
        let location = format!("usb {}-{}", dev.bus_number(), dev.address());
        let accessible = dev.open().is_ok();
        out.push((location, accessible));
    }
    out
}

pub struct Chizhu {
    handle: rusb::DeviceHandle<rusb::GlobalContext>,
    model: &'static ScreenModel,
    synced: bool,
    pub calibration: Calibration,
    header: [u8; 0xd0],
    /// `bRequest` that worked last time, tried first.
    ctrl_request: u8,
}

impl Chizhu {
    pub fn open(model: &'static ScreenModel) -> Result<Self> {
        let (vid, pid) = model.transport.vid_pid();
        let handle = rusb::open_device_with_vid_pid(vid, pid)
            .ok_or_else(|| Error::DeviceNotFound(format!("{vid:04x}:{pid:04x}")))?;
        if handle.kernel_driver_active(0).unwrap_or(false) {
            if let Err(e) = handle.detach_kernel_driver(0) {
                warn!(error = %e, "could not detach kernel driver");
            }
        }
        if let Err(e) = handle.set_active_configuration(1) {
            debug!(error = %e, "set_active_configuration(1) failed (usually harmless)");
        }
        match handle.claim_interface(0) {
            Ok(()) => {}
            Err(rusb::Error::Access) => {
                return Err(Error::Permission(format!("usb {vid:04x}:{pid:04x}").into()))
            }
            Err(e) => return Err(e.into()),
        }
        info!(model = model.name, "opened ChiZhu USB screen");
        let cal = Calibration::default();
        Ok(Self {
            handle,
            model,
            synced: false,
            header: frame_header(model.width, model.height, cal.pixel_format.bytes_per_pixel()),
            calibration: cal,
            ctrl_request: 0x01,
        })
    }

    pub fn set_calibration(&mut self, cal: Calibration) {
        self.header = frame_header(self.model.width, self.model.height, cal.pixel_format.bytes_per_pixel());
        self.calibration = cal;
    }

    fn ctrl(&mut self, data: &[u8]) -> Result<()> {
        let mut order = vec![self.ctrl_request];
        for r in [0x01u8, 0x00, 0x02] {
            if r != self.ctrl_request {
                order.push(r);
            }
        }
        let mut last = None;
        for req in order {
            match self.handle.write_control(0x40, req, 0, 0, data, CTRL_TIMEOUT) {
                Ok(_) => {
                    self.ctrl_request = req;
                    return Ok(());
                }
                Err(e) => last = Some(e),
            }
        }
        Err(last.map(Error::Usb).unwrap_or_else(|| Error::Protocol("control transfer failed".into())))
    }

    fn bulk(&mut self, data: &[u8]) -> Result<()> {
        for chunk in data.chunks(CHUNK) {
            self.handle.write_bulk(EP_OUT, chunk, BULK_TIMEOUT)?;
        }
        Ok(())
    }

    fn sync(&mut self) -> Result<()> {
        let c = cmd_sync();
        for _ in 0..3 {
            self.ctrl(&c)?;
            std::thread::sleep(Duration::from_millis(50));
        }
        self.synced = true;
        debug!("sync complete");
        Ok(())
    }

    /// Send already-encoded pixels (header is prepended here).
    pub fn send_pixels(&mut self, pixels: &[u8]) -> Result<()> {
        if !self.synced {
            self.sync()?;
        }
        let mut packet = Vec::with_capacity(self.header.len() + pixels.len());
        packet.extend_from_slice(&self.header);
        packet.extend_from_slice(pixels);
        self.ctrl(&CMD_START)?;
        self.bulk(&CMD_TRIG)?;
        self.bulk(&packet)?;
        self.ctrl(&CMD_COMMIT)?;
        Ok(())
    }
}

impl Screen for Chizhu {
    fn model(&self) -> &ScreenModel {
        self.model
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            brightness: false,
            hw_rotations: &[],
        }
    }

    fn connect(&mut self) -> Result<ScreenStatus> {
        self.synced = false;
        self.sync()?;
        Ok(ScreenStatus {
            rotation: Some(self.calibration.rotation),
            ..Default::default()
        })
    }

    fn show(&mut self, frame: &RgbImage) -> Result<()> {
        let pixels = encode_pixels(frame, self.model.width, self.model.height, &self.calibration);
        self.send_pixels(&pixels)
    }

    fn set_brightness(&mut self, _value: u8) -> Result<()> {
        Err(Error::InvalidValue("this screen has no brightness control".into()))
    }

    fn set_rotation(&mut self, degrees: u16) -> Result<()> {
        if ![0, 90, 180, 270].contains(&degrees) {
            return Err(Error::InvalidValue(format!("rotation {degrees} (use 0, 90, 180 or 270)")));
        }
        self.calibration.rotation = degrees;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_matches_python_driver() {
        let h = frame_header(320, 320, 2);
        assert_eq!(&h[0..4], &[0x12, 0x34, 0x56, 0x78]);
        assert_eq!(&h[4..8], &3u32.to_le_bytes());
        assert_eq!(&h[8..12], &320u32.to_le_bytes());
        assert_eq!(&h[0x3c..0x40], &204800u32.to_le_bytes());
        assert_eq!(&h[0xc4..0xc8], &0x2000u32.to_le_bytes());
        assert_eq!(h.len(), 208);
    }

    #[test]
    fn rgb565_be_packs_like_numpy() {
        let mut img = RgbImage::new(2, 1);
        img.put_pixel(0, 0, image::Rgb([255, 0, 0]));
        img.put_pixel(1, 0, image::Rgb([0, 255, 0]));
        let px = encode_pixels(&img, 2, 1, &Calibration::default());
        // red = 0xF800, green = 0x07E0 (big-endian)
        assert_eq!(px, vec![0xf8, 0x00, 0x07, 0xe0]);
    }

    #[test]
    fn roll_wraps_like_numpy() {
        let mut img = RgbImage::new(3, 1);
        for x in 0..3 {
            img.put_pixel(x, 0, image::Rgb([x as u8, 0, 0]));
        }
        let r = roll(&img, 1, 0);
        let xs: Vec<u8> = (0..3).map(|x| r.get_pixel(x, 0)[0]).collect();
        assert_eq!(xs, vec![2, 0, 1]); // np.roll([0,1,2], 1) == [2,0,1]
    }

    #[test]
    fn rotation_90_is_counter_clockwise() {
        // 2×1 image: left red, right green. Rotating 90° CCW (PIL semantics)
        // puts red at the bottom-left and green at the top-left.
        let mut img = RgbImage::new(2, 2);
        img.put_pixel(0, 0, image::Rgb([255, 0, 0]));
        img.put_pixel(1, 0, image::Rgb([0, 255, 0]));
        let cal = Calibration { rotation: 90, ..Default::default() };
        let px = encode_pixels(&img, 2, 2, &cal);
        // row 0: (0,0) and (1,0); row 1: (0,1) and (1,1)
        let p = |i: usize| u16::from_be_bytes([px[i * 2], px[i * 2 + 1]]);
        assert_eq!(p(0), 0x07e0); // green top-left
        assert_eq!(p(2), 0xf800); // red bottom-left
    }
}
