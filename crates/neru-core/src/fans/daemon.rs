//! The `neru-fand` root helper: a Unix socket speaking newline-delimited JSON.
//!
//! The desktop app (running as the user) cannot write to `/sys/class/hwmon`,
//! so a small daemon started by systemd does it. It owns the [`Controller`],
//! applies the curves every second even when the app is closed, and hands
//! every channel back to the BIOS when it stops.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

use super::{ChannelState, Controller, FanConfig, Inventory};
use crate::{Error, Result};

pub const SOCKET_PATH: &str = "/run/neru-fand.sock";
pub const CONFIG_PATH: &str = "/etc/neru-screen-control/fans.json";
pub const UNIT_NAME: &str = "neru-fand.service";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    Status,
    Set { config: FanConfig },
    Release,
    Version,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusReply {
    pub version: String,
    pub uptime_s: u64,
    pub inventory: Inventory,
    pub config: FanConfig,
    pub states: Vec<ChannelState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reply {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<StatusReply>,
}

/// The systemd unit that runs the helper.
pub fn unit_file(exec: &str) -> String {
    format!(
        "[Unit]\n\
         Description=Neru Screen Control fan helper (hwmon PWM)\n\
         Documentation=https://github.com/Huzzama/Neru-screen-control\n\
         After=multi-user.target\n\n\
         [Service]\n\
         Type=simple\n\
         ExecStart={exec}\n\
         Restart=always\n\
         RestartSec=2\n\
         # Fans go back to the BIOS when the helper stops (see Controller::drop).\n\
         KillSignal=SIGTERM\n\
         TimeoutStopSec=5\n\n\
         [Install]\n\
         WantedBy=multi-user.target\n"
    )
}

/// Run the daemon until SIGTERM/SIGINT (`stop` is set by the signal handler).
pub fn serve(socket: &Path, config_path: &Path, hwmon_root: PathBuf, stop: Arc<AtomicBool>) -> Result<()> {
    let config = match FanConfig::load(config_path) {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "fan config unreadable, starting with no channels");
            FanConfig::default()
        }
    };
    let controller = Arc::new(Mutex::new(Controller::new(hwmon_root, config)));
    let started = Instant::now();

    if socket.exists() {
        let _ = std::fs::remove_file(socket);
    }
    if let Some(d) = socket.parent() {
        std::fs::create_dir_all(d)?;
    }
    let listener = UnixListener::bind(socket)?;
    // Any local user may talk to the helper (like CoolerControl's localhost API).
    std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o666))?;
    listener.set_nonblocking(true)?;
    info!(socket = %socket.display(), "neru-fand listening");

    let mut last_tick = Instant::now() - Duration::from_secs(5);
    while !stop.load(Ordering::Relaxed) {
        if last_tick.elapsed() >= Duration::from_secs(1) {
            last_tick = Instant::now();
            if let Ok(mut c) = controller.lock() {
                c.tick();
            }
        }
        match listener.accept() {
            Ok((stream, _)) => {
                let c = controller.clone();
                let cp = config_path.to_path_buf();
                let up = started.elapsed().as_secs();
                std::thread::spawn(move || handle(stream, c, cp, up));
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => {
                error!(error = %e, "accept failed");
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    }
    info!("neru-fand stopping; restoring BIOS fan control");
    if let Ok(mut c) = controller.lock() {
        c.release_all();
    }
    let _ = std::fs::remove_file(socket);
    Ok(())
}

fn handle(stream: UnixStream, controller: Arc<Mutex<Controller>>, config_path: PathBuf, uptime_s: u64) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut reader = BufReader::new(match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    });
    let mut writer = stream;
    let mut line = String::new();
    while reader.read_line(&mut line).map_or(false, |n| n > 0) {
        let reply = match serde_json::from_str::<Request>(line.trim()) {
            Ok(req) => process(req, &controller, &config_path, uptime_s),
            Err(e) => Reply { ok: false, error: Some(format!("bad request: {e}")), status: None },
        };
        let mut out = serde_json::to_string(&reply).unwrap_or_else(|_| "{\"ok\":false}".into());
        out.push('\n');
        if writer.write_all(out.as_bytes()).is_err() {
            break;
        }
        line.clear();
    }
}

fn status_of(c: &Controller, uptime_s: u64) -> StatusReply {
    StatusReply {
        version: env!("CARGO_PKG_VERSION").into(),
        uptime_s,
        inventory: c.snapshot(),
        config: c.config().clone(),
        states: c.states.clone(),
    }
}

fn process(req: Request, controller: &Arc<Mutex<Controller>>, config_path: &Path, uptime_s: u64) -> Reply {
    let mut c = match controller.lock() {
        Ok(c) => c,
        Err(_) => return Reply { ok: false, error: Some("controller poisoned".into()), status: None },
    };
    match req {
        Request::Version => Reply { ok: true, error: None, status: Some(status_of(&c, uptime_s)) },
        Request::Status => Reply { ok: true, error: None, status: Some(status_of(&c, uptime_s)) },
        Request::Set { config } => {
            if let Err(e) = c.set_config(config.clone()) {
                return Reply { ok: false, error: Some(e.to_string()), status: None };
            }
            if let Err(e) = config.save(config_path) {
                warn!(error = %e, "fan config not persisted");
            }
            c.tick();
            Reply { ok: true, error: None, status: Some(status_of(&c, uptime_s)) }
        }
        Request::Release => {
            c.release_all();
            Reply { ok: true, error: None, status: Some(status_of(&c, uptime_s)) }
        }
    }
}

/// One round trip to the helper.
pub fn call(socket: &Path, req: &Request) -> Result<Reply> {
    let stream = UnixStream::connect(socket).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound || e.kind() == std::io::ErrorKind::ConnectionRefused {
            Error::DeviceNotFound(format!("fan helper not running ({})", socket.display()))
        } else {
            Error::Io(e)
        }
    })?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let mut w = stream.try_clone()?;
    let mut line = serde_json::to_string(req)?;
    line.push('\n');
    w.write_all(line.as_bytes())?;
    let mut reader = BufReader::new(stream);
    let mut resp = String::new();
    reader.read_line(&mut resp)?;
    if resp.trim().is_empty() {
        return Err(Error::Protocol("empty reply from fan helper".into()));
    }
    Ok(serde_json::from_str(resp.trim())?)
}

/// Is the helper reachable?
pub fn available(socket: &Path) -> bool {
    socket.exists() && call(socket, &Request::Version).map_or(false, |r| r.ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serve_and_call_roundtrip() {
        let dir = std::env::temp_dir().join(format!("neru-fand-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let hw = dir.join("hwmon");
        std::fs::create_dir_all(hw.join("hwmon0")).unwrap();
        std::fs::write(hw.join("hwmon0/name"), "nct6798").unwrap();
        std::fs::write(hw.join("hwmon0/pwm1"), "100").unwrap();
        std::fs::write(hw.join("hwmon0/pwm1_enable"), "5").unwrap();
        std::fs::write(hw.join("hwmon0/temp1_input"), "50000").unwrap();
        let sock = dir.join("sock");
        let cfg = dir.join("fans.json");
        let stop = Arc::new(AtomicBool::new(false));
        let (s2, sock2, cfg2) = (stop.clone(), sock.clone(), cfg.clone());
        let h = std::thread::spawn(move || serve(&sock2, &cfg2, hw, s2).unwrap());
        for _ in 0..50 {
            if sock.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let r = call(&sock, &Request::Status).unwrap();
        assert!(r.ok);
        assert_eq!(r.status.unwrap().inventory.channels[0].id, "nct6798:1");
        let set = Request::Set {
            config: FanConfig { channels: vec![super::super::ChannelConfig { id: "nct6798:1".into(), name: None, mode: super::super::ChannelMode::Fixed { pct: 40.0 } }] },
        };
        let r = call(&sock, &set).unwrap();
        assert!(r.ok, "{:?}", r.error);
        assert_eq!(std::fs::read_to_string(dir.join("hwmon/hwmon0/pwm1")).unwrap().trim(), "102");
        assert!(cfg.exists());
        stop.store(true, Ordering::Relaxed);
        h.join().unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("hwmon/hwmon0/pwm1_enable")).unwrap().trim(), "5");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
