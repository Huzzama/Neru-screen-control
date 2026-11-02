//! On-disk configuration: `~/.config/neru-screen-control/config.json`
//! plus `themes/` and `media/` next to it.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::rgb::RgbConfig;
use crate::screen::thermalright::Calibration;
use crate::{Error, Result};

pub const APP_ID: &str = "neru-screen-control";

#[derive(Debug, Clone)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub config_file: PathBuf,
    pub themes_dir: PathBuf,
    pub media_dir: PathBuf,
}

impl Paths {
    pub fn default_paths() -> Self {
        let base = directories::ProjectDirs::from("", "", APP_ID)
            .map(|d| d.config_dir().to_path_buf())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join(APP_ID)))
            .unwrap_or_else(|| PathBuf::from(".").join(APP_ID));
        Self::at(base)
    }

    pub fn at(config_dir: PathBuf) -> Self {
        Self {
            config_file: config_dir.join("config.json"),
            themes_dir: config_dir.join("themes"),
            media_dir: config_dir.join("media"),
            config_dir,
        }
    }

    pub fn ensure(&self) -> Result<()> {
        fs::create_dir_all(&self.themes_dir)?;
        fs::create_dir_all(&self.media_dir)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScreenConfig {
    /// Pin a model id (see `screen::MODELS`); `None` = first screen found.
    pub model: Option<String>,
    /// 0–100 (hardware brightness where supported).
    pub brightness: u8,
    /// 0 / 90 / 180 / 270.
    pub rotation: u16,
    /// Target frames per second for the render loop.
    pub fps: u32,
    /// JPEG quality for screens that take JPEG frames.
    pub jpeg_quality: u8,
    /// Theme file name inside `themes/` to show.
    pub active_theme: Option<String>,
    /// Software calibration for Thermalright / ChiZhu panels.
    pub thermalright: Calibration,
}

impl Default for ScreenConfig {
    fn default() -> Self {
        Self {
            model: None,
            brightness: 100,
            rotation: 0,
            fps: 10,
            jpeg_quality: 85,
            active_theme: None,
            thermalright: Calibration::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MetricsConfig {
    pub interval_ms: u64,
    pub fahrenheit: bool,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self { interval_ms: 1000, fahrenheit: false }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppBehavior {
    pub start_on_login: bool,
    pub launch_hidden: bool,
    pub tray_icon: bool,
    pub minimize_on_close: bool,
}

impl Default for AppBehavior {
    fn default() -> Self {
        Self {
            start_on_login: false,
            launch_hidden: false,
            tray_icon: true,
            minimize_on_close: true,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub screen: ScreenConfig,
    pub metrics: MetricsConfig,
    pub app: AppBehavior,
    pub rgb: RgbConfig,
}

impl AppConfig {
    pub fn load(path: &Path) -> Result<Self> {
        match fs::read_to_string(path) {
            Ok(s) => serde_json::from_str(&s).map_err(|e| Error::Config(format!("{}: {e}", path.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_forward_compat() {
        let dir = std::env::temp_dir().join(format!("neru-cfg-{}", std::process::id()));
        let p = dir.join("config.json");
        let mut c = AppConfig::default();
        c.screen.brightness = 42;
        c.save(&p).unwrap();
        assert_eq!(AppConfig::load(&p).unwrap(), c);
        // Unknown keys and missing sections are tolerated.
        fs::write(&p, r#"{"screen":{"fps":5,"future_key":1},"something_new":{}}"#).unwrap();
        let c2 = AppConfig::load(&p).unwrap();
        assert_eq!(c2.screen.fps, 5);
        assert_eq!(c2.screen.brightness, 100);
        fs::remove_dir_all(&dir).unwrap();
    }
}
