//! Linux `i2c-dev` access: SMBus transactions through `/dev/i2c-N`, and the
//! list of adapters with the PCI device behind each (GPUs, chipset SMBus).
//!
//! Needs the `i2c-dev` kernel module and read/write access to the device
//! node (Neru's udev rule grants it to the logged-in user).

use std::fs::{File, OpenOptions};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

use crate::{Error, Result};

const I2C_SLAVE: libc::c_ulong = 0x0703;
const I2C_SLAVE_FORCE: libc::c_ulong = 0x0706;
const I2C_SMBUS: libc::c_ulong = 0x0720;

const I2C_SMBUS_READ: u8 = 1;
const I2C_SMBUS_WRITE: u8 = 0;
const I2C_SMBUS_BYTE: u32 = 1;
const I2C_SMBUS_BYTE_DATA: u32 = 2;
const I2C_SMBUS_WORD_DATA: u32 = 3;
const I2C_SMBUS_BLOCK_DATA: u32 = 5;
const I2C_SMBUS_I2C_BLOCK_DATA: u32 = 8;
pub const I2C_SMBUS_BLOCK_MAX: usize = 32;

#[repr(C)]
union SmbusData {
    byte: u8,
    word: u16,
    block: [u8; I2C_SMBUS_BLOCK_MAX + 2],
}

#[repr(C)]
struct SmbusIoctlData {
    read_write: u8,
    command: u8,
    size: u32,
    data: *mut SmbusData,
}

/// An open `/dev/i2c-N`.
pub struct I2cDev {
    file: File,
    path: PathBuf,
    addr: Option<u8>,
}

impl I2cDev {
    pub fn open(path: &Path) -> Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                Error::Permission(path.to_path_buf())
            } else {
                Error::Io(e)
            }
        })?;
        Ok(Self { file, path: path.to_path_buf(), addr: None })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn set_addr(&mut self, addr: u8) -> Result<()> {
        if self.addr == Some(addr) {
            return Ok(());
        }
        // SAFETY: plain ioctl on an open fd with an integer argument.
        let r = unsafe { libc::ioctl(self.file.as_raw_fd(), I2C_SLAVE, addr as libc::c_ulong) };
        if r < 0 {
            let e = std::io::Error::last_os_error();
            // A kernel driver owns the address: take it anyway (same as i2c-tools -f).
            if e.raw_os_error() == Some(libc::EBUSY) {
                let r = unsafe { libc::ioctl(self.file.as_raw_fd(), I2C_SLAVE_FORCE, addr as libc::c_ulong) };
                if r < 0 {
                    return Err(Error::Io(std::io::Error::last_os_error()));
                }
            } else {
                return Err(Error::Io(e));
            }
        }
        self.addr = Some(addr);
        Ok(())
    }

    fn xfer(&mut self, addr: u8, read_write: u8, command: u8, size: u32, data: &mut SmbusData) -> Result<()> {
        self.set_addr(addr)?;
        let mut args = SmbusIoctlData { read_write, command, size, data: data as *mut SmbusData };
        // SAFETY: `args` and `data` outlive the call; layout matches <linux/i2c-dev.h>.
        let r = unsafe { libc::ioctl(self.file.as_raw_fd(), I2C_SMBUS, &mut args as *mut SmbusIoctlData) };
        if r < 0 {
            return Err(Error::Io(std::io::Error::last_os_error()));
        }
        Ok(())
    }

    /// Receive one byte with no register (SMBus "read byte"): the cheapest
    /// way to ask whether anything answers at `addr`.
    pub fn read_byte(&mut self, addr: u8) -> Result<u8> {
        let mut data = SmbusData { byte: 0 };
        self.xfer(addr, I2C_SMBUS_READ, 0, I2C_SMBUS_BYTE, &mut data)?;
        // SAFETY: the kernel filled `byte`.
        Ok(unsafe { data.byte })
    }

    pub fn write_word_data(&mut self, addr: u8, reg: u8, value: u16) -> Result<()> {
        let mut data = SmbusData { word: value };
        self.xfer(addr, I2C_SMBUS_WRITE, reg, I2C_SMBUS_WORD_DATA, &mut data)
    }

    /// SMBus block write (length byte on the wire), up to 32 bytes.
    pub fn write_block_data(&mut self, addr: u8, reg: u8, bytes: &[u8]) -> Result<()> {
        let n = bytes.len().min(I2C_SMBUS_BLOCK_MAX);
        let mut block = [0u8; I2C_SMBUS_BLOCK_MAX + 2];
        block[0] = n as u8;
        block[1..1 + n].copy_from_slice(&bytes[..n]);
        let mut data = SmbusData { block };
        self.xfer(addr, I2C_SMBUS_WRITE, reg, I2C_SMBUS_BLOCK_DATA, &mut data)
    }

    pub fn read_byte_data(&mut self, addr: u8, reg: u8) -> Result<u8> {
        let mut data = SmbusData { block: [0; I2C_SMBUS_BLOCK_MAX + 2] };
        self.xfer(addr, I2C_SMBUS_READ, reg, I2C_SMBUS_BYTE_DATA, &mut data)?;
        // SAFETY: the kernel filled `byte` for a BYTE_DATA read.
        Ok(unsafe { data.byte })
    }

    pub fn write_byte_data(&mut self, addr: u8, reg: u8, value: u8) -> Result<()> {
        let mut data = SmbusData { byte: value };
        self.xfer(addr, I2C_SMBUS_WRITE, reg, I2C_SMBUS_BYTE_DATA, &mut data)
    }

    /// Up to 32 bytes from `reg` (an "I2C block" read: no length byte on the wire).
    pub fn read_i2c_block_data(&mut self, addr: u8, reg: u8, len: usize) -> Result<Vec<u8>> {
        let len = len.clamp(1, I2C_SMBUS_BLOCK_MAX);
        let mut block = [0u8; I2C_SMBUS_BLOCK_MAX + 2];
        block[0] = len as u8;
        let mut data = SmbusData { block };
        self.xfer(addr, I2C_SMBUS_READ, reg, I2C_SMBUS_I2C_BLOCK_DATA, &mut data)?;
        // SAFETY: block[0] is the returned length, followed by the bytes.
        let block = unsafe { data.block };
        let n = (block[0] as usize).min(I2C_SMBUS_BLOCK_MAX);
        Ok(block[1..1 + n].to_vec())
    }
}

/// PCI identity of the device an adapter hangs off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PciId {
    pub vendor: u16,
    pub device: u16,
    pub subvendor: u16,
    pub subdevice: u16,
    /// `0000:01:00.0`
    pub slot: String,
}

/// One `/dev/i2c-N`.
#[derive(Debug, Clone)]
pub struct Adapter {
    pub bus: u32,
    pub name: String,
    pub dev: PathBuf,
    pub pci: Option<PciId>,
}

/// PCI ids of the chipset SMBus controllers DRAM and SMBus motherboard RGB
/// hang off (from OpenRGB's `IF_DRAM_SMBUS`): AMD FCH, Intel PCH generations.
pub const CHIPSET_SMBUS: &[(u16, u16)] = &[
    (0x1022, 0x790B), // AMD FCH
    (0x8086, 0x3A30), // ICH10
    (0x8086, 0xA123), // Sunrise Point-H
    (0x8086, 0x2085), // Skylake-X PCU
    (0x8086, 0xA2A3), // Kaby Lake-H
    (0x8086, 0xA323), // Cannon Lake-H
    (0x8086, 0x06A3), // Comet Lake-H
    (0x8086, 0xA3A3), // Comet Lake-V
    (0x8086, 0x43A3), // Tiger Lake
    (0x8086, 0x7AA3), // Alder Lake
    (0x8086, 0x7A23), // Raptor Lake
    (0x8086, 0x7F23), // Arrow Lake
];

impl Adapter {
    pub fn is_nvidia_gpu(&self) -> bool {
        self.name.starts_with("NVIDIA i2c adapter") && self.pci.as_ref().map_or(false, |p| p.vendor == 0x10de)
    }

    /// The motherboard's SMBus (where RAM modules and SMBus-controlled
    /// motherboard lighting live). Needs `i2c-piix4` (AMD) or `i2c-i801`
    /// (Intel) loaded.
    pub fn is_chipset_smbus(&self) -> bool {
        let by_pci = self.pci.as_ref().map_or(false, |p| CHIPSET_SMBUS.contains(&(p.vendor, p.device)));
        by_pci || self.name.starts_with("SMBus PIIX4 adapter") || self.name.starts_with("SMBus I801 adapter")
    }

    /// Adapter owned by a graphics card (any vendor).
    pub fn is_gpu(&self) -> bool {
        self.pci.as_ref().map_or(false, |p| p.vendor == 0x10de || p.vendor == 0x1002)
    }
}

fn read_hex(p: &Path) -> Option<u16> {
    let s = std::fs::read_to_string(p).ok()?;
    u16::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok()
}

/// Walk up from a sysfs path to the nearest PCI device directory.
fn pci_ancestor(mut path: PathBuf) -> Option<PciId> {
    for _ in 0..8 {
        if path.join("vendor").exists() && path.join("device").exists() && path.join("subsystem_vendor").exists() {
            return Some(PciId {
                vendor: read_hex(&path.join("vendor"))?,
                device: read_hex(&path.join("device"))?,
                subvendor: read_hex(&path.join("subsystem_vendor")).unwrap_or(0),
                subdevice: read_hex(&path.join("subsystem_device")).unwrap_or(0),
                slot: path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            });
        }
        if !path.pop() {
            break;
        }
    }
    None
}

/// Every I²C adapter on the system (`/sys/bus/i2c/devices/i2c-*`), sorted by bus number.
pub fn adapters() -> Vec<Adapter> {
    let Ok(rd) = std::fs::read_dir("/sys/bus/i2c/devices") else {
        return Vec::new();
    };
    let mut v: Vec<Adapter> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let bus: u32 = name.strip_prefix("i2c-")?.parse().ok()?;
            let dir = e.path();
            let adapter_name = std::fs::read_to_string(dir.join("name")).ok()?.trim().to_string();
            if adapter_name == "DPMST" {
                return None;
            }
            let real = std::fs::canonicalize(&dir).unwrap_or(dir);
            Some(Adapter { bus, name: adapter_name, dev: PathBuf::from(format!("/dev/i2c-{bus}")), pci: pci_ancestor(real) })
        })
        .collect();
    v.sort_by_key(|a| a.bus);
    v
}

/// Whether `/dev/i2c-*` nodes exist at all (the `i2c-dev` module is loaded).
pub fn i2c_dev_available() -> bool {
    std::fs::read_dir("/dev").map(|rd| rd.flatten().any(|e| e.file_name().to_string_lossy().starts_with("i2c-"))).unwrap_or(false)
}

/// udev rule giving the logged-in user access to I²C adapters (what GPU and
/// DRAM RGB controllers are reached through).
pub fn udev_rules() -> String {
    "\n# I2C adapters: GPU and memory RGB controllers (needs the i2c-dev module)\nSUBSYSTEM==\"i2c-dev\", TAG+=\"uaccess\"\n".to_string()
}

/// `/etc/modules-load.d` entry so `i2c-dev` is there after a reboot.
pub const MODULES_LOAD_FILE: &str = "/etc/modules-load.d/neru-screen-control.conf";
pub const MODULES_LOAD_CONTENT: &str = "# Neru Screen Control: I2C access for GPU / DRAM RGB controllers\ni2c-dev\n# chipset SMBus (RGB RAM, SMBus motherboards): AMD / Intel\ni2c-piix4\ni2c-i801\n";
