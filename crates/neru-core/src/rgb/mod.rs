//! RGB lighting.
//!
//! Two sources of devices, shown and driven the same way:
//!
//! * **native** drivers in [`native`] (Neru talks to the hardware itself);
//! * **OpenRGB** (optional) through its SDK server, for everything Neru has no
//!   driver for yet. A device handled natively is hidden from the OpenRGB list.
//!
//! Each device either runs a hardware mode or is driven per LED by software
//! effects (static, breathing, rainbow, colour cycle, temperature-reactive).

pub mod client;
pub mod effects;
pub mod native;
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
    /// Also use OpenRGB for devices Neru has no driver for.
    pub use_openrgb: bool,
    /// Launch `openrgb --server` when no server answers.
    pub start_server: bool,
    /// LED counts of resizable zones (ARGB headers): `"<controller key>#<zone>"` → LEDs.
    pub zone_sizes: HashMap<String, u32>,
}

impl Default for RgbConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            server: client::DEFAULT_ADDR.into(),
            fps: 20,
            brightness: 100,
            assignments: Vec::new(),
            use_openrgb: true,
            start_server: true,
            zone_sizes: HashMap::new(),
        }
    }
}

impl RgbConfig {
    /// Upgrade keys saved by older versions (they embedded `/dev/hidrawN`,
    /// which changes between boots and made assignments and LED counts
    /// "forget" themselves). Idempotent.
    pub fn migrate(&mut self) -> bool {
        let mut changed = false;
        let mut seen: HashSet<(String, Option<u32>)> = HashSet::new();
        let mut kept = Vec::with_capacity(self.assignments.len());
        for mut a in std::mem::take(&mut self.assignments) {
            let k = protocol::migrate_key(&a.controller);
            if k != a.controller {
                changed = true;
                a.controller = k;
            }
            if seen.insert((a.controller.clone(), a.zone)) {
                kept.push(a);
            } else {
                changed = true;
            }
        }
        self.assignments = kept;
        let mut sizes = HashMap::new();
        for (k, v) in std::mem::take(&mut self.zone_sizes) {
            let nk = match k.rsplit_once('#') {
                Some((key, zone)) => format!("{}#{zone}", protocol::migrate_key(key)),
                None => k.clone(),
            };
            if nk != k {
                changed = true;
            }
            sizes.entry(nk).or_insert(v);
        }
        self.zone_sizes = sizes;
        changed
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
    /// Re-probe native devices.
    RescanNative,
    Stop,
}

#[derive(Debug, Clone, Serialize)]
pub struct ZoneInfo {
    pub index: u32,
    pub name: String,
    pub leds_count: u32,
    pub leds_min: u32,
    pub leds_max: u32,
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
    /// Driven by Neru itself (not through OpenRGB).
    pub native: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RgbStatus {
    pub enabled: bool,
    /// Connected to an OpenRGB server.
    pub connected: bool,
    pub use_openrgb: bool,
    pub native_devices: usize,
    pub native_errors: Vec<String>,
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
    native: native::NativeHub,
    client: Option<OpenRgbClient>,
    /// Native devices first, then OpenRGB ones (minus duplicates).
    controllers: Vec<Controller>,
    orgb_controllers: Vec<Controller>,
    last_connect: Option<Instant>,
    last_poll: Instant,
    last_preview: Instant,
    last_keepalive: Instant,
    custom_mode: HashSet<String>,
    hw_applied: HashMap<String, Effect>,
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
            native: native::NativeHub::default(),
            client: None,
            controllers: Vec::new(),
            orgb_controllers: Vec::new(),
            last_connect: None,
            last_poll: now,
            last_preview: now,
            last_keepalive: now,
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

    /// Rebuild the merged list from both sources.
    fn merge(&mut self) {
        let mut list = self.native.controllers();
        let native_locs = self.native.locations();
        list.extend(self.orgb_controllers.iter().filter(|c| !native_locs.contains(&c.location)).cloned());
        Controller::assign_ordinals(&mut list);
        self.controllers = list;
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
                    native: c.native,
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
                                leds_min: z.leds_min,
                                leds_max: z.leds_max,
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
            s.use_openrgb = self.cfg.use_openrgb;
            s.server = self.cfg.server.clone();
            s.protocol = self.client.as_ref().map_or(0, |c| c.version);
            s.native_devices = self.native.controllers().len();
            s.native_errors = self.native.errors.clone();
            s.controllers = controllers;
            s.fps_actual = self.fps_actual;
        }
    }

    fn scan_native(&mut self) {
        self.native.scan(&self.cfg.zone_sizes);
        let native_keys: Vec<String> = self.controllers.iter().filter(|c| c.native).map(|c| c.key()).collect();
        self.custom_mode.retain(|k| !native_keys.contains(k));
        self.hw_applied.clear();
        self.merge();
        self.publish();
    }

    fn connect(&mut self) {
        self.last_connect = Some(Instant::now());
        match OpenRgbClient::connect(&self.cfg.server) {
            Ok(c) => {
                self.client = Some(c);
                self.set_error(None);
                self.reload();
            }
            Err(e) => {
                debug!(error = %e, "OpenRGB connect failed");
                if self.native.is_empty() {
                    self.set_error(Some(e.to_string()));
                }
                self.publish();
            }
        }
    }

    fn disconnect(&mut self, why: String) {
        warn!(%why, "OpenRGB connection dropped");
        self.client = None;
        self.orgb_controllers.clear();
        self.custom_mode.retain(|k| self.controllers.iter().any(|c| c.native && &c.key() == k));
        self.merge();
        self.set_error(Some(why));
        self.publish();
    }

    fn reload(&mut self) {
        let Some(client) = self.client.as_mut() else { return };
        match client.controllers() {
            Ok(list) => {
                info!(devices = list.len(), "OpenRGB device list loaded");
                self.orgb_controllers = list;
                // Apply configured sizes to resizable OpenRGB zones.
                let sizes: Vec<(u32, u32, u32)> = self
                    .orgb_controllers
                    .iter()
                    .flat_map(|c| {
                        let key = c.key();
                        let sizes = &self.cfg.zone_sizes;
                        c.zones.iter().enumerate().filter_map(move |(zi, z)| {
                            let n = *sizes.get(&format!("{key}#{zi}"))?;
                            (z.leds_max > z.leds_min && n != z.leds_count).then_some((c.index, zi as u32, n))
                        })
                    })
                    .collect();
                if !sizes.is_empty() {
                    if let Some(c) = self.client.as_mut() {
                        for (dev, zone, n) in &sizes {
                            let _ = c.resize_zone(*dev, *zone, *n);
                        }
                        if let Ok(l) = c.controllers() {
                            self.orgb_controllers = l;
                        }
                    }
                }
                let profiles = self.client.as_mut().and_then(|c| c.profiles().ok()).unwrap_or_default();
                if let Ok(mut s) = self.status.lock() {
                    s.profiles = profiles;
                }
                self.custom_mode.clear();
                self.hw_applied.clear();
                self.merge();
                self.publish();
            }
            Err(e) => self.disconnect(e.to_string()),
        }
    }

    // ── per-device dispatch ─────────────────────────────────────────────────

    fn fail(&mut self, c: &Controller, e: crate::Error) {
        if c.native {
            warn!(device = %c.name, error = %e, "native RGB device error");
            self.set_error(Some(format!("{}: {e}", c.name)));
            self.scan_native();
        } else {
            self.disconnect(e.to_string());
        }
    }

    fn dev_direct(&mut self, c: &Controller) -> crate::Result<()> {
        if c.native {
            self.native.device(c.index).ok_or(crate::Error::NoDevice)?.set_direct()
        } else {
            self.client.as_mut().ok_or(crate::Error::NoDevice)?.set_custom_mode(c.index)
        }
    }

    fn dev_zone(&mut self, c: &Controller, zone: u32, colors: &[Color]) -> crate::Result<()> {
        if c.native {
            self.native.device(c.index).ok_or(crate::Error::NoDevice)?.zone_leds(zone, colors)
        } else {
            self.client.as_mut().ok_or(crate::Error::NoDevice)?.update_zone_leds(c.index, zone, colors)
        }
    }

    fn dev_mode(&mut self, c: &Controller, idx: usize, m: &Mode, save: bool) -> crate::Result<()> {
        if c.native {
            let d = self.native.device(c.index).ok_or(crate::Error::NoDevice)?;
            if save {
                d.save_mode(idx, m)
            } else {
                d.set_mode(idx, m)
            }
        } else {
            let cl = self.client.as_mut().ok_or(crate::Error::NoDevice)?;
            if save {
                cl.save_mode(c.index, idx as u32, m)
            } else {
                cl.update_mode(c.index, idx as u32, m)
            }
        }
    }

    fn handle(&mut self, cmd: RgbCommand) -> bool {
        match cmd {
            RgbCommand::SetConfig(c) => {
                let server_changed = c.server != self.cfg.server;
                let sizes_changed = c.zone_sizes != self.cfg.zone_sizes;
                let orgb_off = !c.use_openrgb || !c.enabled;
                self.cfg = c;
                if server_changed || orgb_off {
                    self.client = None;
                    self.orgb_controllers.clear();
                    self.last_connect = None;
                }
                if sizes_changed {
                    self.native.apply_sizes(&self.cfg.zone_sizes);
                    self.custom_mode.clear();
                    if self.client.is_some() {
                        self.reload();
                    }
                }
                self.hw_applied.clear();
                self.merge();
                self.publish();
            }
            RgbCommand::Refresh => {
                if self.client.is_some() {
                    self.reload();
                } else {
                    self.last_connect = None;
                }
                self.merge();
                self.publish();
            }
            RgbCommand::RescanNative => self.scan_native(),
            RgbCommand::Rescan => {
                self.scan_native();
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
                if let Some(c) = self.controllers.iter().find(|c| c.key() == key).cloned() {
                    let idx = c.active_mode.max(0) as usize;
                    if let Some(mode) = c.modes.get(idx).cloned() {
                        match self.dev_mode(&c, idx, &mode, true) {
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

    fn apply_hardware(&mut self) {
        let todo: Vec<(String, Effect, Controller)> = self
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
        for (key, effect, c) in todo {
            let Effect::Hardware { mode, color, speed, brightness, direction } = &effect else { continue };
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
            if let Some(d) = direction {
                if m.has(protocol::mode_flags::HAS_DIRECTION_LR) || m.has(protocol::mode_flags::HAS_DIRECTION_UD) || m.has(protocol::mode_flags::HAS_DIRECTION_HV) {
                    m.direction = *d;
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
            let res = self.dev_mode(&c, mi, &m, false).and_then(|_| {
                if let (Some(col), true) = (col, m.has(protocol::mode_flags::HAS_PER_LED_COLOR)) {
                    for zi in 0..c.zones.len() as u32 {
                        let n = c.zones[zi as usize].leds_count as usize;
                        if n > 0 {
                            self.dev_zone(&c, zi, &vec![col; n])?;
                        }
                    }
                }
                Ok(())
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
                    self.fail(&c, e);
                    return;
                }
            }
        }
    }

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
                if let Err(e) = self.dev_direct(&c) {
                    self.fail(&c, e);
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
                    Effect::Temperature { metric, .. } => snapshot.get(metric).map(|v| {
                        let e = self.smooth.entry(metric.clone()).or_insert(v);
                        *e = *e * 0.7 + v * 0.3;
                        *e
                    }),
                    _ => None,
                };
                let colors: Vec<Color> = effects::render(&effect, t, n, metric).into_iter().map(|c| c.scale(brightness)).collect();
                if let Err(e) = self.dev_zone(&c, zi, &colors) {
                    self.fail(&c, e);
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
        if self.cfg.enabled {
            self.scan_native();
        }
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
                thread::sleep(Duration::from_millis(200));
                continue;
            }

            if self.cfg.use_openrgb {
                if self.client.is_none() {
                    if self.last_connect.map_or(true, |t| t.elapsed() > Duration::from_secs(3)) {
                        self.connect();
                    }
                } else if self.last_poll.elapsed() > Duration::from_secs(1) {
                    self.last_poll = Instant::now();
                    let dirty = match self.client.as_mut().map(|c| c.poll_notifications().map(|_| c.take_list_dirty())) {
                        Some(Ok(d)) => d,
                        Some(Err(e)) => {
                            self.disconnect(e.to_string());
                            false
                        }
                        None => false,
                    };
                    if dirty {
                        self.reload();
                    }
                }
            }

            self.apply_hardware();
            let t0 = Instant::now();
            self.frame();
            if self.last_keepalive.elapsed() >= Duration::from_secs(1) {
                self.last_keepalive = Instant::now();
                self.native.keepalive();
            }

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
