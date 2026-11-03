//! The render loop: metrics → renderer → screen, on its own thread.
//!
//! The engine keeps running without a screen (so the editor preview works
//! with nothing plugged in) and reconnects when a screen appears.

use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tracing::{debug, error, info, warn};

use crate::metrics::{Collector, History, Snapshot};
use crate::render::media::HwAccel;
use crate::render::{FontBook, Renderer, Theme};
use crate::screen::{self, Screen};
use crate::Result;

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub fps: u32,
    pub brightness: u8,
    pub rotation: u16,
    pub jpeg_quality: u8,
    /// Pin a model id; `None` = first screen found.
    pub model: Option<String>,
    pub fahrenheit: bool,
    pub metrics_interval: Duration,
    /// Preview JPEGs per second handed to the sink (0 = never).
    pub preview_fps: u32,
    pub preview_quality: u8,
    pub media_dir: Option<std::path::PathBuf>,
    /// Hardware-accelerated video decoding for media layers.
    pub hwaccel: HwAccel,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            fps: 10,
            brightness: 100,
            rotation: 0,
            jpeg_quality: 85,
            model: None,
            fahrenheit: false,
            metrics_interval: Duration::from_secs(1),
            preview_fps: 8,
            preview_quality: 70,
            media_dir: None,
            hwaccel: HwAccel::Off,
        }
    }
}

pub enum Command {
    /// Replace the theme (media is reloaded).
    SetTheme(Theme),
    /// Replace the theme while editing (decoded media is kept).
    UpdateTheme(Theme),
    SetBrightness(u8),
    SetRotation(u16),
    SetFps(u32),
    SetJpegQuality(u8),
    SetFahrenheit(bool),
    /// Video decoding acceleration (restarts running videos).
    SetHwAccel(HwAccel),
    /// Stop pushing frames; the screen falls back to its own picture.
    Pause(bool),
    /// Produce preview frames for the UI.
    Preview(bool),
    /// Pin a model id (`None` = first screen found) and reconnect.
    SetModel(Option<String>),
    Reconnect,
    Stop,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ScreenSummary {
    pub model_id: String,
    pub vendor: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub location: String,
    pub firmware: Option<String>,
    pub serial: Option<String>,
    pub brightness: bool,
    pub hw_rotation: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct EngineStatus {
    pub connected: bool,
    pub screen: Option<ScreenSummary>,
    /// Screens seen on the bus (including ones without permission).
    pub detected: Vec<String>,
    /// Cooler displays Neru recognises but has no driver for.
    #[serde(default)]
    pub recognised_only: Vec<String>,
    pub needs_udev: bool,
    pub last_error: Option<String>,
    pub paused: bool,
    pub frames: u64,
    pub fps_actual: f32,
    pub render_ms: f32,
    pub send_ms: f32,
    pub theme_name: String,
    pub theme_width: u32,
    pub theme_height: u32,
    pub media_errors: Vec<(String, String)>,
}

/// A preview frame for the UI.
pub struct PreviewFrame {
    pub jpeg: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub seq: u64,
}

pub type PreviewSink = Box<dyn Fn(PreviewFrame) + Send>;

pub struct Engine {
    tx: Sender<Command>,
    status: Arc<Mutex<EngineStatus>>,
    metrics: Arc<RwLock<Snapshot>>,
    history: Arc<RwLock<History>>,
    handle: Option<thread::JoinHandle<()>>,
    _collector: Collector,
}

impl Engine {
    pub fn start(cfg: EngineConfig, theme: Theme, fonts: Arc<FontBook>, sink: Option<PreviewSink>) -> Result<Self> {
        let collector = Collector::start(cfg.metrics_interval);
        let metrics = collector.shared();
        let history = collector.history();
        let status = Arc::new(Mutex::new(EngineStatus {
            theme_name: theme.name.clone(),
            theme_width: theme.width,
            theme_height: theme.height,
            ..Default::default()
        }));
        let (tx, rx) = channel();
        let worker = Worker::new(cfg, theme, fonts, metrics.clone(), history.clone(), status.clone(), rx, sink)?;
        let handle = thread::Builder::new()
            .name("neru-engine".into())
            .spawn(move || worker.run())
            .expect("spawn engine thread");
        Ok(Self { tx, status, metrics, history, handle: Some(handle), _collector: collector })
    }

    pub fn send(&self, cmd: Command) {
        let _ = self.tx.send(cmd);
    }

    pub fn status(&self) -> EngineStatus {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn metrics(&self) -> Snapshot {
        self.metrics.read().map(|s| s.clone()).unwrap_or_default()
    }

    /// The live metrics shared with other engines (RGB effects read it too).
    pub fn metrics_shared(&self) -> Arc<RwLock<Snapshot>> {
        self.metrics.clone()
    }

    /// Recent values of every metric (graphs, dashboards).
    pub fn history(&self) -> Arc<RwLock<History>> {
        self.history.clone()
    }

    pub fn stop(&mut self) {
        let _ = self.tx.send(Command::Stop);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop();
    }
}

struct Worker {
    cfg: EngineConfig,
    /// The theme as designed (may be a different size than the screen).
    theme: Theme,
    renderer: Renderer,
    fonts: Arc<FontBook>,
    metrics: Arc<RwLock<Snapshot>>,
    status: Arc<Mutex<EngineStatus>>,
    rx: Receiver<Command>,
    sink: Option<PreviewSink>,
    screen: Option<Box<dyn Screen>>,
    last_connect_attempt: Option<Instant>,
    paused: bool,
    preview: bool,
    preview_seq: u64,
    last_preview: Instant,
    frames: u64,
    fps_window: (Instant, u64),
}

impl Worker {
    #[allow(clippy::too_many_arguments)]
    fn new(
        cfg: EngineConfig,
        theme: Theme,
        fonts: Arc<FontBook>,
        metrics: Arc<RwLock<Snapshot>>,
        history: Arc<RwLock<History>>,
        status: Arc<Mutex<EngineStatus>>,
        rx: Receiver<Command>,
        sink: Option<PreviewSink>,
    ) -> Result<Self> {
        let mut renderer = Renderer::new(theme.clone(), fonts.db.clone(), cfg.media_dir.clone())?;
        renderer.fahrenheit = cfg.fahrenheit;
        renderer.set_history(history);
        renderer.set_hwaccel(cfg.hwaccel);
        Ok(Self {
            cfg,
            theme,
            renderer,
            fonts,
            metrics,
            status,
            rx,
            sink,
            screen: None,
            last_connect_attempt: None,
            paused: false,
            preview: false,
            preview_seq: 0,
            last_preview: Instant::now(),
            frames: 0,
            fps_window: (Instant::now(), 0),
        })
    }

    fn set_status(&self, f: impl FnOnce(&mut EngineStatus)) {
        if let Ok(mut s) = self.status.lock() {
            f(&mut s);
        }
    }

    /// Fit the designed theme to the connected screen (or keep it as is).
    fn apply_theme(&mut self, live: bool) {
        let target = self
            .screen
            .as_ref()
            .map(|s| (s.model().width, s.model().height))
            .unwrap_or((self.theme.width, self.theme.height));
        let t = if target != (self.theme.width, self.theme.height) {
            Renderer::scale_theme(&self.theme, target.0, target.1)
        } else {
            self.theme.clone()
        };
        let res = if live { self.renderer.update_theme(t) } else { self.renderer.set_theme(t) };
        if let Err(e) = res {
            error!(error = %e, "theme rejected");
            self.set_status(|s| s.last_error = Some(e.to_string()));
        }
        let (name, w, h) = (self.theme.name.clone(), self.renderer.theme().width, self.renderer.theme().height);
        self.set_status(|s| {
            s.theme_name = name;
            s.theme_width = w;
            s.theme_height = h;
        });
    }

    fn try_connect(&mut self) {
        self.last_connect_attempt = Some(Instant::now());
        let detected = screen::detect();
        let needs_udev = detected.iter().any(|d| !d.accessible);
        // Variants of one physical screen share a location: list it once.
        let mut names: Vec<String> = Vec::new();
        let mut seen: Vec<(u16, u16, String)> = Vec::new();
        for d in &detected {
            let (vid, pid) = d.model.transport.vid_pid();
            let k = (vid, pid, d.location.clone());
            if !seen.contains(&k) {
                seen.push(k);
                names.push(d.to_string());
            }
        }
        let recognised_only = screen::detect_recognised_only();
        self.set_status(|s| {
            s.detected = names;
            s.recognised_only = recognised_only;
            s.needs_udev = needs_udev;
        });
        let Some(pick) = detected
            .iter()
            .find(|d| d.accessible && self.cfg.model.as_deref().map_or(true, |m| d.model.id == m))
        else {
            return;
        };
        match screen::open(pick).and_then(|mut s| s.connect().map(|st| (s, st))) {
            Ok((mut s, st)) => {
                info!(screen = %pick, "screen connected");
                s.set_jpeg_quality(self.cfg.jpeg_quality);
                if s.capabilities().brightness {
                    if let Err(e) = s.set_brightness(self.cfg.brightness) {
                        warn!(error = %e, "brightness not applied");
                    }
                }
                if st.rotation != Some(self.cfg.rotation) {
                    if let Err(e) = s.set_rotation(self.cfg.rotation) {
                        warn!(error = %e, "rotation not applied");
                    }
                }
                let caps = s.capabilities();
                let m = s.model().clone();
                let summary = ScreenSummary {
                    model_id: m.id.to_string(),
                    vendor: m.vendor.to_string(),
                    name: m.name.to_string(),
                    width: m.width,
                    height: m.height,
                    location: pick.location.clone(),
                    firmware: st.firmware.clone(),
                    serial: st.serial.clone(),
                    brightness: caps.brightness,
                    hw_rotation: !caps.hw_rotations.is_empty(),
                };
                self.screen = Some(s);
                self.set_status(|st| {
                    st.connected = true;
                    st.screen = Some(summary);
                    st.last_error = None;
                });
                self.apply_theme(false);
            }
            Err(e) => {
                warn!(error = %e, "screen open failed");
                self.set_status(|s| s.last_error = Some(e.to_string()));
            }
        }
    }

    fn disconnect(&mut self, why: String) {
        warn!(%why, "screen disconnected");
        self.screen = None;
        self.set_status(|s| {
            s.connected = false;
            s.screen = None;
            s.last_error = Some(why);
        });
        self.apply_theme(false);
    }

    fn handle(&mut self, cmd: Command) -> bool {
        match cmd {
            Command::SetTheme(t) => {
                self.theme = t;
                self.apply_theme(false);
            }
            Command::UpdateTheme(t) => {
                self.theme = t;
                self.apply_theme(true);
            }
            Command::SetBrightness(b) => {
                self.cfg.brightness = b;
                if let Some(s) = self.screen.as_mut() {
                    if s.capabilities().brightness {
                        if let Err(e) = s.set_brightness(b) {
                            self.set_status(|st| st.last_error = Some(e.to_string()));
                        }
                    }
                }
            }
            Command::SetRotation(r) => {
                self.cfg.rotation = r;
                if let Some(s) = self.screen.as_mut() {
                    if let Err(e) = s.set_rotation(r) {
                        self.set_status(|st| st.last_error = Some(e.to_string()));
                    }
                }
            }
            Command::SetFps(f) => self.cfg.fps = f.clamp(1, 60),
            Command::SetJpegQuality(q) => {
                self.cfg.jpeg_quality = q;
                if let Some(s) = self.screen.as_mut() {
                    s.set_jpeg_quality(q);
                }
            }
            Command::SetFahrenheit(f) => {
                self.cfg.fahrenheit = f;
                self.renderer.fahrenheit = f;
            }
            Command::SetHwAccel(h) => {
                self.cfg.hwaccel = h;
                self.renderer.set_hwaccel(h);
            }
            Command::Pause(p) => {
                self.paused = p;
                self.set_status(|s| s.paused = p);
            }
            Command::Preview(p) => self.preview = p,
            Command::SetModel(m) => {
                self.cfg.model = m;
                return self.handle(Command::Reconnect);
            }
            Command::Reconnect => {
                self.screen = None;
                self.last_connect_attempt = None;
                self.set_status(|s| {
                    s.connected = false;
                    s.screen = None;
                });
            }
            Command::Stop => return false,
        }
        true
    }

    fn run(mut self) {
        info!(fonts = self.fonts.db.len(), "engine started");
        self.apply_theme(false);
        loop {
            // Commands.
            loop {
                match self.rx.try_recv() {
                    Ok(cmd) => {
                        if !self.handle(cmd) {
                            info!("engine stopped");
                            return;
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return,
                }
            }

            // (Re)connect every 2 s while nothing is open.
            if self.screen.is_none()
                && self.last_connect_attempt.map_or(true, |t| t.elapsed() > Duration::from_secs(2))
            {
                self.try_connect();
            }

            let interval = Duration::from_secs_f64(1.0 / self.cfg.fps.max(1) as f64);
            let want_preview = self.preview
                && self.cfg.preview_fps > 0
                && self.last_preview.elapsed() >= Duration::from_secs_f64(1.0 / self.cfg.preview_fps as f64);
            let want_screen = self.screen.is_some() && !self.paused;

            if !want_screen && !want_preview {
                // Nothing to draw for; idle cheaply but keep handling commands.
                thread::sleep(Duration::from_millis(50));
                continue;
            }

            let t0 = Instant::now();
            let snapshot = self.metrics.read().map(|s| s.clone()).unwrap_or_default();
            let frame = match self.renderer.render(&snapshot) {
                Ok(f) => f.clone(),
                Err(e) => {
                    error!(error = %e, "render failed");
                    self.set_status(|s| s.last_error = Some(e.to_string()));
                    thread::sleep(interval);
                    continue;
                }
            };
            let render_ms = t0.elapsed().as_secs_f32() * 1000.0;

            let mut send_ms = 0.0;
            if want_screen {
                let t1 = Instant::now();
                let res = self.screen.as_mut().map(|s| s.show(&frame));
                send_ms = t1.elapsed().as_secs_f32() * 1000.0;
                match res {
                    Some(Ok(())) => {
                        self.frames += 1;
                    }
                    Some(Err(e)) => {
                        self.disconnect(e.to_string());
                    }
                    None => {}
                }
            }

            if want_preview {
                if let Some(sink) = &self.sink {
                    let mut jpeg = Vec::with_capacity(64 * 1024);
                    let enc = jpeg_encoder::Encoder::new(&mut jpeg, self.cfg.preview_quality);
                    if enc
                        .encode(frame.as_raw(), frame.width() as u16, frame.height() as u16, jpeg_encoder::ColorType::Rgb)
                        .is_ok()
                    {
                        self.preview_seq += 1;
                        sink(PreviewFrame { jpeg, width: frame.width(), height: frame.height(), seq: self.preview_seq });
                    }
                }
                self.last_preview = Instant::now();
            }

            // Stats.
            self.fps_window.1 += 1;
            if self.fps_window.0.elapsed() >= Duration::from_secs(2) {
                let fps = self.fps_window.1 as f32 / self.fps_window.0.elapsed().as_secs_f32();
                self.fps_window = (Instant::now(), 0);
                let frames = self.frames;
                let media_errors = self.renderer.media_errors();
                self.set_status(|s| {
                    s.fps_actual = fps;
                    s.frames = frames;
                    s.render_ms = render_ms;
                    s.send_ms = send_ms;
                    s.media_errors = media_errors;
                });
                debug!(fps, render_ms, send_ms, "engine stats");
            }

            let spent = t0.elapsed();
            if interval > spent {
                thread::sleep(interval - spent);
            }
        }
    }
}
