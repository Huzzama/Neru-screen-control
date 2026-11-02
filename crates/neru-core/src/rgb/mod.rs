//! RGB lighting through OpenRGB.
//!
//! OpenRGB does the hardware work (motherboard ARGB headers, RAM, GPU, …);
//! Neru connects to its SDK server, lists devices and zones, and either picks a
//! device-native mode or drives the LEDs itself with software effects (static,
//! breathing, rainbow, colour cycle, temperature-reactive).

pub mod client;
pub mod effects;
pub mod protocol;

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

pub use client::OpenRgbClient;
pub use effects::{Assignment, Effect};
pub use protocol::{Color, Controller, Mode, Zone};

use crate::metrics::Snapshot;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RgbConfig {
    pub enabled: bool,
    pub server: String,
    /// Software effect updates per second.
    pub fps: u32,
    /// 0–100, applied to software effects.
    pub brightness: u8,
    pub assignments: Vec<Assignment>,
    /// Launch `openrgb --server` when no server answers.
    pub start_server: bool,
}

impl Default for RgbConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            server: client::DEFAULT_ADDR.into(),
            fps: 20,
            brightness: 100,
            assignments: Vec::new(),
            start_server: true,
        }
    }
}

pub enum RgbCommand {
    SetConfig(RgbConfig),
    Refresh,
    Rescan,
    LoadProfile(String),
    SaveProfile(String),
    DeleteProfile(String),
    /// Persist the current mode of a controller to its flash (where supported).
    SaveToDevice(String),
    Stop,
}

#[derive(Debug, Clone, Serialize)]
pub struct ZoneInfo {
    pub index: u32,
    pub name: String,
    pub leds_count: u32,
    pub start_idx: u32,
    pub matrix: Option<(u32, u32)>,
    pub segments: Vec<String>,
    /// Up to 32 sampled colours currently shown (hex).
    pub preview: Vec<String>,
    /// The assignment in effect for this zone, if any.
    pub effect: Option<Effect>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerInfo {
    pub key: String,
    pub index: u32,
    pub name: String,
    pub vendor: String,
    pub kind: &'static str,
    pub description: String,
    pub location: String,
    pub version: String,
    pub serial: String,
    pub active_mode: i32,
    pub modes: Vec<Mode>,
    pub zones: Vec<ZoneInfo>,
    pub leds: usize,
    /// Whether a software effect currently drives this device.
    pub software: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RgbStatus {
    pub enabled: bool,
    pub connected: bool,
    pub server: String,
    pub protocol: u32,
    pub error: Option<String>,
    pub controllers: Vec<ControllerInfo>,
    pub profiles: Vec<String>,
    pub fps_actual: f32,
    pub last_message: Option<String>,
}

pub struct RgbEngine {
    tx: Sender<RgbCommand>,
    status: Arc<Mutex<RgbStatus>>,
    handle: Option<thread::JoinHandle<()>>,
}

impl RgbEngine {
    pub fn start(cfg: RgbConfig, metrics: Arc<RwLock<Snapshot>>) -> Self {
        let status = Arc::new(Mutex::new(RgbStatus { enabled: cfg.enabled, server: cfg.server.clone(), ..Default::default() }));
        let (tx, rx) = channel();
        let worker = Worker::new(cfg, metrics, status.clone(), rx);
        let handle = thread::Builder::new()
            .name("neru-rgb".into())
            .spawn(move || worker.run())
            .expect("spawn rgb thread");
        Self { tx, status, handle: Some(handle) }
    }

    pub fn send(&self, cmd: RgbCommand) {
        let _ = self.tx.send(cmd);
    }

    pub fn status(&self) -> RgbStatus {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn stop(&mut self) {
        let _ = self.tx.send(RgbCommand::Stop);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for RgbEngine {
    fn drop(&mut self) {
        self.stop();
    }
}

struct Worker {
    cfg: RgbConfig,
    metrics: Arc<RwLock<Snapshot>>,
    status: Arc<Mutex<RgbStatus>>,
    rx: Receiver<RgbCommand>,
    client: Option<OpenRgbClient>,
    controllers: Vec<Controller>,
    last_connect: Option<Instant>,
    last_poll: Instant,
    last_preview: Instant,
    /// Controllers already switched to their custom (per-LED) mode.
    custom_mode: HashSet<String>,
    /// Hardware modes already applied: controller key → effect.
    hw_applied: HashMap<String, Effect>,
    /// Latest colours sent per (controller key, zone index) for previews.
    previews: HashMap<(String, u32), Vec<Color>>,
    smooth: HashMap<String, f32>,
    started: Instant,
    frames: u64,
    fps_window: (Instant, u64),
    fps_actual: f32,
}

impl Worker {
    fn new(cfg: RgbConfig, metrics: Arc<RwLock<Snapshot>>, status: Arc<Mutex<RgbStatus>>, rx: Receiver<RgbCommand>) -> Self {
        let now = Instant::now();
        Self {
            cfg,
            metrics,
            status,
            rx,
            client: None,
            controllers: Vec::new(),
            last_connect: None,
            last_poll: now,
            last_preview: now,
            custom_mode: HashSet::new(),
            hw_applied: HashMap::new(),
            previews: HashMap::new(),
            smooth: HashMap::new(),
            started: now,
            frames: 0,
            fps_window: (now, 0),
            fps_actual: 0.0,
        }
    }

    fn set_error(&self, e: Option<String>) {
        if let Ok(mut s) = self.status.lock() {
            s.error = e;
        }
    }

    fn set_message(&self, m: impl Into<String>) {
        if let Ok(mut s) = self.status.lock() {
            s.last_message = Some(m.into());
        }
    }

    /// Effect in force for a zone: zone-specific assignment, else the device-wide one.
    fn effect_for(&self, key: &str, zone: u32) -> Option<&Effect> {
        self.cfg
            .assignments
            .iter()
            .find(|a| a.controller == key && a.zone == Some(zone))
            .or_else(|| self.cfg.assignments.iter().find(|a| a.controller == key && a.zone.is_none()))
            .map(|a| &a.effect)
    }

    fn device_is_software(&self, c: &Controller) -> bool {
        let key = c.key();
        (0..c.zones.len() as u32).any(|z| self.effect_for(&key, z).map_or(false, Effect::is_software))
    }

    fn publish(&self) {
        let controllers: Vec<ControllerInfo> = self
            .controllers
            .iter()
            .map(|c| {
                let key = c.key();
                ControllerInfo {
                    key: key.clone(),
                    index: c.index,
                    name: c.name.clone(),
                    vendor: c.vendor.clone(),
                    kind: c.kind_name(),
                    description: c.description.clone(),
                    location: c.location.clone(),
                    version: c.version.clone(),
                    serial: c.serial.clone(),
                    active_mode: c.active_mode,
                    modes: c.modes.clone(),
                    leds: c.leds.len(),
                    software: self.device_is_software(c),
                    zones: c
                        .zones
                        .iter()
                        .enumerate()
                        .map(|(zi, z)| {
                            let preview = self
                                .previews
                                .get(&(key.clone(), zi as u32))
                                .map(|cols| sample(cols))
                                .unwrap_or_else(|| {
                                    let s = z.start_idx as usize;
                                    let e = (s + z.leds_count as usize).min(c.colors.len());
                                    sample(c.colors.get(s..e).unwrap_or(&[]))
                                });
                            ZoneInfo {
                                index: zi as u32,
                                name: z.name.clone(),
                                leds_count: z.leds_count,
                                start_idx: z.start_idx,
                                matrix: z.matrix,
                                segments: z.segments.iter().map(|s| s.name.clone()).collect(),
                                preview,
                                effect: self.effect_for(&key, zi as u32).cloned(),
                            }
                        })
                        .collect(),
                }
            })
            .collect();
        if let Ok(mut s) = self.status.lock() {
            s.enabled = self.cfg.enabled;
            s.connected = self.client.is_some();
            s.server = self.cfg.server.clone();
            s.protocol = self.client.as_ref().map_or(0, |c| c.version);
            s.controllers = controllers;
            s.fps_actual = self.fps_actual;
        }
    }

    fn connect(&mut self) {
        self.last_connect = Some(Instant::now());
        match OpenRgbClient::connect(&self.cfg.server) {
            Ok(c) => {
                self.client = Some(c);
                self.custom_mode.clear();
                self.hw_applied.clear();
                self.set_error(None);
                self.reload();
            }
            Err(e) => {
                debug!(error = %e, "OpenRGB connect failed");
                self.set_error(Some(e.to_string()));
                self.publish();
            }
        }
    }

    fn disconnect(&mut self, why: String) {
        warn!(%why, "OpenRGB connection dropped");
        self.client = None;
        self.controllers.clear();
        self.custom_mode.clear();
        self.hw_applied.clear();
        self.set_error(Some(why));
        self.publish();
    }

    fn reload(&mut self) {
        let Some(client) = self.client.as_mut() else { return };
        match client.controllers() {
            Ok(list) => {
                info!(devices = list.len(), "OpenRGB device list loaded");
                self.controllers = list;
                let profiles = self.client.as_mut().and_then(|c| c.profiles().ok()).unwrap_or_default();
                if let Ok(mut s) = self.status.lock() {
                    s.profiles = profiles;
                }
                // Modes may have been changed by someone else: re-apply ours.
                self.custom_mode.clear();
                self.hw_applied.clear();
                self.publish();
            }
            Err(e) => self.disconnect(e.to_string()),
        }
    }

    fn handle(&mut self, cmd: RgbCommand) -> bool {
        match cmd {
            RgbCommand::SetConfig(c) => {
                let server_changed = c.server != self.cfg.server;
                let was_enabled = self.cfg.enabled;
                self.cfg = c;
                if server_changed || (!self.cfg.enabled && was_enabled) {
                    self.client = None;
                    self.controllers.clear();
                    self.last_connect = None;
                }
                // Assignments may have changed: hardware modes get re-applied.
                self.hw_applied.clear();
                self.publish();
            }
            RgbCommand::Refresh => {
                if self.client.is_some() {
                    self.reload();
                } else {
                    self.last_connect = None;
                }
            }
            RgbCommand::Rescan => {
                if let Some(c) = self.client.as_mut() {
                    if let Err(e) = c.rescan() {
                        self.disconnect(e.to_string());
                    } else {
                        self.set_message("Rescanning devices…");
                    }
                }
            }
            RgbCommand::LoadProfile(name) => {
                if let Some(c) = self.client.as_mut() {
                    match c.load_profile(&name) {
                        Ok(()) => {
                            self.set_message(format!("Profile {name} loaded"));
                            // The profile changed device modes; drop our software state.
                            self.custom_mode.clear();
                            self.hw_applied.clear();
                            thread::sleep(Duration::from_millis(300));
                            self.reload();
                        }
                        Err(e) => self.set_error(Some(e.to_string())),
                    }
                }
            }
            RgbCommand::SaveProfile(name) => {
                let res = self.client.as_mut().map(|c| c.save_profile(&name));
                match res {
                    Some(Ok(())) => {
                        self.set_message(format!("Profile {name} saved"));
                        thread::sleep(Duration::from_millis(200));
                        self.refresh_profiles();
                    }
                    Some(Err(e)) => self.set_error(Some(e.to_string())),
                    None => {}
                }
            }
            RgbCommand::DeleteProfile(name) => {
                let res = self.client.as_mut().map(|c| c.delete_profile(&name));
                match res {
                    Some(Ok(())) => {
                        thread::sleep(Duration::from_millis(200));
                        self.refresh_profiles();
                    }
                    Some(Err(e)) => self.set_error(Some(e.to_string())),
                    None => {}
                }
            }
            RgbCommand::SaveToDevice(key) => {
                if let Some(ci) = self.controllers.iter().position(|c| c.key() == key) {
                    let c = self.controllers[ci].clone();
                    let idx = c.active_mode.max(0) as usize;
                    if let (Some(client), Some(mode)) = (self.client.as_mut(), c.modes.get(idx)) {
                        match client.save_mode(c.index, idx as u32, mode) {
                            Ok(()) => self.set_message(format!("{}: mode saved to device", c.name)),
                            Err(e) => self.set_error(Some(e.to_string())),
                        }
                    }
                }
            }
            RgbCommand::Stop => return false,
        }
        true
    }

    fn refresh_profiles(&mut self) {
        let profiles = self.client.as_mut().and_then(|c| c.profiles().ok()).unwrap_or_default();
        if let Ok(mut s) = self.status.lock() {
            s.profiles = profiles;
        }
    }

    /// Apply hardware modes that are assigned but not yet sent.
    fn apply_hardware(&mut self) {
        let keys: Vec<(String, Effect, Controller)> = self
            .controllers
            .iter()
            .filter(|c| !self.device_is_software(c))
            .filter_map(|c| {
                let key = c.key();
                let eff = self.cfg.assignments.iter().find(|a| a.controller == key && matches!(a.effect, Effect::Hardware { .. }))?;
                if self.hw_applied.get(&key) == Some(&eff.effect) {
                    return None;
                }
                Some((key, eff.effect.clone(), c.clone()))
            })
            .collect();
        for (key, effect, c) in keys {
            let Effect::Hardware { mode, color, speed, brightness } = &effect else { continue };
            let Some(mi) = c.modes.iter().position(|m| m.name.eq_ignore_ascii_case(mode)) else {
                self.set_error(Some(format!("{}: no mode named {mode}", c.name)));
                self.hw_applied.insert(key, effect.clone());
                continue;
            };
            let mut m = c.modes[mi].clone();
            if let Some(s) = speed {
                if m.has(protocol::mode_flags::HAS_SPEED) {
                    m.speed = (*s).clamp(m.speed_min.min(m.speed_max), m.speed_max.max(m.speed_min));
                }
            }
            if let Some(b) = brightness {
                if m.has(protocol::mode_flags::HAS_BRIGHTNESS) {
                    m.brightness = (*b).clamp(m.brightness_min.min(m.brightness_max), m.brightness_max.max(m.brightness_min));
                }
            }
            let col = color.as_deref().and_then(Color::from_hex);
            if let Some(col) = col {
                if m.has(protocol::mode_flags::HAS_MODE_SPECIFIC_COLOR) {
                    let n = m.colors_min.max(1).max(m.colors.len() as u32) as usize;
                    m.colors = vec![col; n.min(m.colors_max.max(1) as usize)];
                    m.color_mode = protocol::color_mode::MODE_SPECIFIC;
                }
            }
            let Some(client) = self.client.as_mut() else { return };
            let res = client.update_mode(c.index, mi as u32, &m).and_then(|_| {
                if col.is_some() && m.has(protocol::mode_flags::HAS_PER_LED_COLOR) {
                    client.update_leds(c.index, &vec![col.unwrap(); c.leds.len()])
                } else {
                    Ok(())
                }
            });
            match res {
                Ok(()) => {
                    info!(device = %c.name, mode = %m.name, "hardware mode applied");
                    if let Some(cc) = self.controllers.iter_mut().find(|x| x.key() == key) {
                        cc.active_mode = mi as i32;
                        cc.modes[mi] = m;
                    }
                    self.custom_mode.remove(&key);
                    self.hw_applied.insert(key, effect.clone());
                }
                Err(e) => {
                    self.disconnect(e.to_string());
                    return;
                }
            }
        }
    }

    /// One frame of software effects.
    fn frame(&mut self) {
        let t = self.started.elapsed().as_secs_f32();
        let snapshot = self.metrics.read().map(|s| s.clone()).unwrap_or_default();
        let brightness = self.cfg.brightness as f32 / 100.0;
        let mut sent_any = false;

        let plan: Vec<(Controller, Vec<(u32, Effect)>)> = self
            .controllers
            .iter()
            .filter(|c| self.device_is_software(c))
            .map(|c| {
                let key = c.key();
                let zones = (0..c.zones.len() as u32)
                    .filter_map(|z| self.effect_for(&key, z).filter(|e| e.is_software()).map(|e| (z, e.clone())))
                    .collect();
                (c.clone(), zones)
            })
            .collect();

        for (c, zones) in plan {
            let key = c.key();
            if !self.custom_mode.contains(&key) {
                let Some(client) = self.client.as_mut() else { return };
                if let Err(e) = client.set_custom_mode(c.index) {
                    self.disconnect(e.to_string());
                    return;
                }
                self.custom_mode.insert(key.clone());
                self.hw_applied.remove(&key);
            }
            for (zi, effect) in zones {
                let n = c.zones[zi as usize].leds_count as usize;
                if n == 0 {
                    continue;
                }
                let metric = match &effect {
                    Effect::Temperature { metric, .. } => {
                        let raw = snapshot.get(metric);
                        raw.map(|v| {
                            let e = self.smooth.entry(metric.clone()).or_insert(v);
                            *e = *e * 0.7 + v * 0.3;
                            *e
                        })
                    }
                    _ => None,
                };
                let colors: Vec<Color> = effects::render(&effect, t, n, metric).into_iter().map(|c| c.scale(brightness)).collect();
                let Some(client) = self.client.as_mut() else { return };
                if let Err(e) = client.update_zone_leds(c.index, zi, &colors) {
                    self.disconnect(e.to_string());
                    return;
                }
                sent_any = true;
                self.previews.insert((key.clone(), zi), colors);
            }
        }
        if sent_any {
            self.frames += 1;
            self.fps_window.1 += 1;
        }
    }

    fn run(mut self) {
        info!("rgb engine started");
        self.publish();
        loop {
            loop {
                match self.rx.try_recv() {
                    Ok(cmd) => {
                        if !self.handle(cmd) {
                            return;
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return,
                }
            }

            if !self.cfg.enabled {
                if self.client.is_some() {
                    self.client = None;
                    self.controllers.clear();
                    self.publish();
                }
                thread::sleep(Duration::from_millis(200));
                continue;
            }

            if self.client.is_none() {
                if self.last_connect.map_or(true, |t| t.elapsed() > Duration::from_secs(3)) {
                    self.connect();
                }
                thread::sleep(Duration::from_millis(100));
                continue;
            }

            // Device list notifications from the server.
            if self.last_poll.elapsed() > Duration::from_secs(1) {
                self.last_poll = Instant::now();
                let dirty = match self.client.as_mut().map(|c| c.poll_notifications().map(|_| c.take_list_dirty())) {
                    Some(Ok(d)) => d,
                    Some(Err(e)) => {
                        self.disconnect(e.to_string());
                        continue;
                    }
                    None => false,
                };
                if dirty {
                    self.reload();
                }
            }

            self.apply_hardware();
            let t0 = Instant::now();
            self.frame();

            if self.last_preview.elapsed() > Duration::from_millis(500) {
                self.last_preview = Instant::now();
                if self.fps_window.0.elapsed() >= Duration::from_secs(2) {
                    self.fps_actual = self.fps_window.1 as f32 / self.fps_window.0.elapsed().as_secs_f32();
                    self.fps_window = (Instant::now(), 0);
                }
                self.publish();
            }

            let interval = Duration::from_secs_f64(1.0 / self.cfg.fps.clamp(1, 60) as f64);
            let spent = t0.elapsed();
            if interval > spent {
                thread::sleep(interval - spent);
            }
        }
    }
}

fn sample(colors: &[Color]) -> Vec<String> {
    if colors.is_empty() {
        return Vec::new();
    }
    let step = (colors.len() as f32 / 32.0).max(1.0);
    let mut out = Vec::new();
    let mut i = 0.0f32;
    while (i as usize) < colors.len() && out.len() < 32 {
        out.push(colors[i as usize].to_hex());
        i += step;
    }
    out
}
