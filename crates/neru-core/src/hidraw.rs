//! Minimal Linux hidraw access (no hidapi dependency).
//!
//! Devices are located through sysfs (`/sys/class/hidraw/*/device/uevent`)
//! and opened as plain files. Only what the cooler screens need: write one
//! output report, read one input report with a timeout.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

use crate::{Error, Result};

/// A hidraw node that matched a VID:PID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HidNode {
    pub path: PathBuf,
    pub vid: u16,
    pub pid: u16,
    /// `HID_NAME` from uevent (what the device calls itself).
    pub name: String,
}

/// Enumerate hidraw nodes for one vendor and any of `pids`.
pub fn find_any(vid: u16, pids: &[u16]) -> Vec<HidNode> {
    let mut out = Vec::new();
    for &pid in pids {
        out.extend(find(vid, pid));
    }
    out
}

/// USB serial number of the device behind a hidraw node (from sysfs).
pub fn serial_of(node: &Path) -> String {
    let Some(name) = node.file_name() else { return String::new() };
    let dev = Path::new("/sys/class/hidraw").join(name).join("device");
    // hidraw → hid device → usb interface → usb device (which has `serial`).
    let Ok(real) = fs::canonicalize(&dev) else { return String::new() };
    let mut p = real;
    for _ in 0..4 {
        if let Ok(s) = fs::read_to_string(p.join("serial")) {
            return s.trim().to_string();
        }
        if !p.pop() {
            break;
        }
    }
    String::new()
}

/// Enumerate hidraw nodes for one VID:PID (sorted by node name).
pub fn find(vid: u16, pid: u16) -> Vec<HidNode> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/class/hidraw") else {
        return out;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for sys in paths {
        let Ok(uevent) = fs::read_to_string(sys.join("device/uevent")) else {
            continue;
        };
        let Some((v, p)) = parse_hid_id(&uevent) else {
            continue;
        };
        if v != vid || p != pid {
            continue;
        }
        let name = uevent
            .lines()
            .find_map(|l| l.strip_prefix("HID_NAME="))
            .unwrap_or("")
            .to_string();
        let Some(node) = sys.file_name() else { continue };
        out.push(HidNode {
            path: Path::new("/dev").join(node),
            vid,
            pid,
            name,
        });
    }
    out
}

/// Whether the node's HID report descriptor declares `usage_page`
/// (a 16-bit "Usage Page" item: `06 lo hi`).
pub fn has_usage_page(node: &Path, usage_page: u16) -> bool {
    let Some(name) = node.file_name() else { return false };
    let desc = Path::new("/sys/class/hidraw").join(name).join("device/report_descriptor");
    let Ok(bytes) = fs::read(desc) else { return false };
    let [lo, hi] = usage_page.to_le_bytes();
    bytes.windows(3).any(|w| w == [0x06, lo, hi])
}

/// Parse `HID_ID=0003:0000264A:00002348` → (0x264A, 0x2348).
fn parse_hid_id(uevent: &str) -> Option<(u16, u16)> {
    let id = uevent.lines().find_map(|l| l.strip_prefix("HID_ID="))?;
    let mut parts = id.split(':');
    let _bus = parts.next()?;
    let vid = u32::from_str_radix(parts.next()?.trim(), 16).ok()?;
    let pid = u32::from_str_radix(parts.next()?.trim(), 16).ok()?;
    Some((vid as u16, pid as u16))
}

/// An open hidraw device.
pub struct HidRaw {
    file: File,
    path: PathBuf,
}

impl HidRaw {
    pub fn open(path: &Path) -> Result<Self> {
        match OpenOptions::new().read(true).write(true).open(path) {
            Ok(file) => Ok(Self { file, path: path.to_path_buf() }),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                Err(Error::Permission(path.to_path_buf()))
            }
            Err(e) => Err(e.into()),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Write one output report for a device **without** report IDs.
    ///
    /// usbhid strips a leading report-number byte, so the report is written
    /// as `0x00` + data.
    pub fn write_report(&mut self, report: &[u8]) -> Result<()> {
        let mut buf = Vec::with_capacity(report.len() + 1);
        buf.push(0);
        buf.extend_from_slice(report);
        let n = self.file.write(&buf)?;
        if n != buf.len() {
            return Err(Error::Protocol(format!(
                "short hidraw write: {n} of {} bytes",
                buf.len()
            )));
        }
        Ok(())
    }

    /// Write bytes exactly as given (for devices with report IDs, the first
    /// byte is the report ID).
    pub fn write_raw(&mut self, data: &[u8]) -> Result<()> {
        let n = self.file.write(data)?;
        if n != data.len() {
            return Err(Error::Protocol(format!("short hidraw write: {n} of {} bytes", data.len())));
        }
        Ok(())
    }

    /// Wait up to `timeout_ms` for an input report. Returns `None` on timeout.
    pub fn read_report(&mut self, buf: &mut [u8], timeout_ms: i32) -> Result<Option<usize>> {
        let mut pfd = libc::pollfd {
            fd: self.file.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: pfd is a valid, initialised pollfd and we pass nfds = 1.
        let rc = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
        if rc < 0 {
            let e = std::io::Error::last_os_error();
            if e.kind() == std::io::ErrorKind::Interrupted {
                return Ok(None);
            }
            return Err(e.into());
        }
        if rc == 0 || pfd.revents & libc::POLLIN == 0 {
            return Ok(None);
        }
        let n = self.file.read(buf)?;
        Ok(Some(n))
    }

    /// Discard any input reports that are already queued.
    pub fn drain(&mut self) -> Result<()> {
        let mut scratch = [0u8; 4096];
        while self.read_report(&mut scratch, 0)?.is_some() {}
        Ok(())
    }

    /// Write a report (`0x00` + data, for devices without report IDs) and
    /// wait up to `timeout_ms` for the reply; the reply bytes are returned
    /// (empty on timeout).
    pub fn exchange(&mut self, report: &[u8], timeout_ms: i32) -> Result<Vec<u8>> {
        self.write_report(report)?;
        let mut buf = vec![0u8; 256];
        match self.read_report(&mut buf, timeout_ms)? {
            Some(n) => {
                buf.truncate(n);
                Ok(buf)
            }
            None => Ok(Vec::new()),
        }
    }

    /// Same as [`HidRaw::exchange`] for devices whose first data byte is the
    /// command (no report-ID byte is prepended).
    pub fn exchange_raw(&mut self, data: &[u8], timeout_ms: i32) -> Result<Vec<u8>> {
        self.write_raw(data)?;
        let mut buf = vec![0u8; 256];
        match self.read_report(&mut buf, timeout_ms)? {
            Some(n) => {
                buf.truncate(n);
                Ok(buf)
            }
            None => Ok(Vec::new()),
        }
    }

    /// Send a feature report (`HIDIOCSFEATURE`); `data[0]` is the report ID.
    pub fn send_feature_report(&mut self, data: &[u8]) -> Result<()> {
        // _IOC(_IOC_WRITE | _IOC_READ, 'H', 0x06, len)
        let req: libc::c_ulong = (3u64 << 30) | ((data.len() as u64) << 16) | ((b'H' as u64) << 8) | 0x06;
        // SAFETY: `data` is a valid buffer of the length encoded in the request.
        let r = unsafe { libc::ioctl(self.file.as_raw_fd(), req as _, data.as_ptr()) };
        if r < 0 {
            return Err(Error::Io(std::io::Error::last_os_error()));
        }
        Ok(())
    }

    /// Read a feature report (`HIDIOCGFEATURE`); `buf[0]` must hold the report ID.
    pub fn get_feature_report(&mut self, buf: &mut [u8]) -> Result<usize> {
        let req: libc::c_ulong = (3u64 << 30) | ((buf.len() as u64) << 16) | ((b'H' as u64) << 8) | 0x07;
        // SAFETY: `buf` is a valid, writable buffer of the encoded length.
        let r = unsafe { libc::ioctl(self.file.as_raw_fd(), req as _, buf.as_mut_ptr()) };
        if r < 0 {
            return Err(Error::Io(std::io::Error::last_os_error()));
        }
        Ok(r as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hid_id() {
        let ue = "DRIVER=hid-generic\nHID_ID=0003:0000264A:00002348\nHID_NAME=ThermalTake USB Device\n";
        assert_eq!(parse_hid_id(ue), Some((0x264a, 0x2348)));
        assert_eq!(parse_hid_id("nothing"), None);
    }
}
