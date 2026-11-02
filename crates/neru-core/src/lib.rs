//! neru-core — everything in Neru Screen Control that is not UI.
//!
//! * [`screen`]  — cooler LCD drivers (Thermaltake "BY" HID screens, Thermalright/ChiZhu USB screens)
//! * [`metrics`] — CPU / GPU / RAM readings from sysfs, procfs and NVML
//! * [`render`]  — theme model and renderer (SVG via resvg + raster media)
//! * [`engine`]  — the loop that renders a theme and pushes it to a screen
//! * [`rgb`]     — OpenRGB SDK client, lighting effects and the RGB engine
//! * [`fans`]    — hwmon fan curves and the `neru-fand` root helper protocol
//! * [`config`]  — on-disk configuration (`~/.config/neru-screen-control/`)

pub mod autostart;
pub mod config;
pub mod engine;
pub mod error;
pub mod fans;
pub mod hidraw;
pub mod metrics;
pub mod render;
pub mod rgb;
pub mod screen;
pub mod themes;

pub use error::{Error, Result};
