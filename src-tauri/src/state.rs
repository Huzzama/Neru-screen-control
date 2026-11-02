//! Shared application state: config, engine, theme store.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine as _;
use serde::Serialize;
use tauri::menu::{CheckMenuItem, MenuItem};
use tauri::{AppHandle, Emitter, Wry};
use tracing::{info, warn};

use neru_core::config::{AppConfig, Paths};
use neru_core::engine::{Command, Engine, EngineConfig, EngineStatus, PreviewFrame};
use neru_core::render::{FontBook, Theme};
use neru_core::themes::ThemeStore;

pub struct TrayItems {
    pub pause: CheckMenuItem<Wry>,
    pub status: MenuItem<Wry>,
}

pub struct AppState {
    pub paths: Paths,
    pub config: Mutex<AppConfig>,
    pub engine: Mutex<Engine>,
    pub fonts: Arc<FontBook>,
    pub themes: ThemeStore,
    pub quitting: AtomicBool,
    pub background_mode: bool,
    pub tray_items: Mutex<Option<TrayItems>>,
    last_tray_text: Mutex<String>,
}

#[derive(Serialize, Clone)]
struct PreviewPayload {
    data: String,
    width: u32,
    height: u32,
    seq: u64,
}

impl AppState {
    pub fn new(app: AppHandle, background_mode: bool) -> Result<Self, Box<dyn std::error::Error>> {
        let paths = Paths::default_paths();
        paths.ensure()?;
        let config = match AppConfig::load(&paths.config_file) {
            Ok(c) => c,
            Err(e) => {
                warn!(error = %e, "config unreadable, using defaults");
                AppConfig::default()
            }
        };
        info!(dir = %paths.config_dir.display(), "config loaded");

        let fonts = Arc::new(FontBook::load(None));
        let themes = ThemeStore::new(paths.themes_dir.clone());

        let theme = Self::startup_theme(&themes, &config);
        let engine_cfg = Self::engine_config(&config, &paths);
        let sink: neru_core::engine::PreviewSink = Box::new(move |f: PreviewFrame| {
            let data = format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(&f.jpeg));
            let _ = app.emit("preview-frame", PreviewPayload { data, width: f.width, height: f.height, seq: f.seq });
        });
        let engine = Engine::start(engine_cfg, theme, fonts.clone(), Some(sink))?;

        Ok(Self {
            paths,
            config: Mutex::new(config),
            engine: Mutex::new(engine),
            fonts,
            themes,
            quitting: AtomicBool::new(false),
            background_mode,
            tray_items: Mutex::new(None),
            last_tray_text: Mutex::new(String::new()),
        })
    }

    fn startup_theme(themes: &ThemeStore, config: &AppConfig) -> Theme {
        if let Some(id) = &config.screen.active_theme {
            match themes.load(id) {
                Ok(t) => return t,
                Err(e) => warn!(theme = id, error = %e, "active theme failed to load"),
            }
        }
        // No theme chosen yet: the bundled 720 one, else the first bundled, else empty.
        let list = themes.list();
        list.iter()
            .find(|i| i.bundled && i.width == 720 && i.height == 720)
            .or_else(|| list.iter().find(|i| i.bundled))
            .and_then(|i| themes.load(&i.id).ok())
            .unwrap_or_else(|| Theme::blank(720, 720))
    }

    /// When no theme is pinned in the config, follow the connected screen:
    /// pick the bundled theme that matches its size.
    pub fn auto_theme_for_screen(&self, width: u32, height: u32) {
        let pinned = self.config.lock().map(|c| c.screen.active_theme.is_some()).unwrap_or(true);
        if pinned {
            return;
        }
        let current = self.status();
        if current.theme_width == width && current.theme_height == height {
            return;
        }
        let list = self.themes.list();
        if let Some(info) = list.iter().find(|i| i.bundled && i.width == width && i.height == height) {
            if let Ok(t) = self.themes.load(&info.id) {
                info!(theme = %info.name, "auto-selected bundled theme for the connected screen");
                self.send(Command::SetTheme(t));
            }
        }
    }

    pub fn engine_config(config: &AppConfig, paths: &Paths) -> EngineConfig {
        EngineConfig {
            fps: config.screen.fps.clamp(1, 60),
            brightness: config.screen.brightness.min(100),
            rotation: config.screen.rotation,
            jpeg_quality: config.screen.jpeg_quality.clamp(10, 100),
            model: config.screen.model.clone(),
            fahrenheit: config.metrics.fahrenheit,
            metrics_interval: Duration::from_millis(config.metrics.interval_ms.clamp(250, 10_000)),
            media_dir: Some(paths.media_dir.clone()),
            ..Default::default()
        }
    }

    pub fn send(&self, cmd: Command) {
        if let Ok(e) = self.engine.lock() {
            e.send(cmd);
        }
    }

    pub fn status(&self) -> EngineStatus {
        self.engine.lock().map(|e| e.status()).unwrap_or_default()
    }

    pub fn set_pause(&self, paused: bool) {
        self.send(Command::Pause(paused));
    }

    pub fn minimize_on_close(&self) -> bool {
        self.config.lock().map(|c| c.app.minimize_on_close).unwrap_or(true)
    }

    /// Apply a new config to the running engine and persist it.
    pub fn apply_config(&self, new: AppConfig) -> Result<(), String> {
        let old = self.config.lock().map_err(|e| e.to_string())?.clone();
        if new.screen.brightness != old.screen.brightness {
            self.send(Command::SetBrightness(new.screen.brightness.min(100)));
        }
        if new.screen.rotation != old.screen.rotation {
            self.send(Command::SetRotation(new.screen.rotation));
        }
        if new.screen.fps != old.screen.fps {
            self.send(Command::SetFps(new.screen.fps));
        }
        if new.screen.jpeg_quality != old.screen.jpeg_quality {
            self.send(Command::SetJpegQuality(new.screen.jpeg_quality));
        }
        if new.metrics.fahrenheit != old.metrics.fahrenheit {
            self.send(Command::SetFahrenheit(new.metrics.fahrenheit));
        }
        if new.screen.model != old.screen.model {
            self.send(Command::SetModel(new.screen.model.clone()));
        }
        new.save(&self.paths.config_file).map_err(|e| e.to_string())?;
        *self.config.lock().map_err(|e| e.to_string())? = new;
        Ok(())
    }

    pub fn update_tray(&self, _app: &AppHandle, status: &EngineStatus) {
        let text = match &status.screen {
            Some(s) if status.connected => format!("{} · {:.0} fps", s.name, status.fps_actual),
            _ if status.needs_udev => "Screen found — install udev rules".to_string(),
            _ => "No screen connected".to_string(),
        };
        let mut last = match self.last_tray_text.lock() {
            Ok(l) => l,
            Err(_) => return,
        };
        if let Ok(items) = self.tray_items.lock() {
            if let Some(items) = items.as_ref() {
                if *last != text {
                    let _ = items.status.set_text(&text);
                    *last = text;
                }
                let _ = items.pause.set_checked(status.paused);
            }
        }
    }

    pub fn shutdown(&self) {
        if let Ok(mut e) = self.engine.lock() {
            e.stop();
        }
    }
}
