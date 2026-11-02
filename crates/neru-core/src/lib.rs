//! neru-core — everything in Neru Screen Control that is not UI.
//!
//! * [`screen`]  — cooler LCD drivers (Thermaltake "BY" HID screens, Thermalright/ChiZhu USB screens)
//! * [`metrics`] — CPU / GPU / RAM readings from sysfs, procfs and NVML
//! * [`config`]  — on-disk configuration (`~/.config/neru-screen-control/`)

pub mod config;
pub mod error;
pub mod hidraw;
pub mod metrics;
pub mod screen;

pub use error::{Error, Result};
