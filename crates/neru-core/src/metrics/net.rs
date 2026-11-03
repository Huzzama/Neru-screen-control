//! Network throughput from `/sys/class/net/*/statistics`.
//!
//! Physical interfaces only (those with a `device` link: PCI, USB…), so
//! loopback, bridges, containers and VPN tunnels do not double-count traffic.
//! All physical interfaces are summed; the busiest one is named.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use super::hwmon::read_i64;
use super::NetMetrics;

#[derive(Debug, Clone)]
struct Iface {
    name: String,
    rx: PathBuf,
    tx: PathBuf,
}

#[derive(Debug, Clone, Copy)]
struct Counters {
    rx: u64,
    tx: u64,
}

pub struct NetSource {
    ifaces: Vec<Iface>,
    last: Option<(Vec<Counters>, Instant)>,
    rescan_at: Instant,
}

fn scan() -> Vec<Iface> {
    let Ok(rd) = fs::read_dir("/sys/class/net") else {
        return Vec::new();
    };
    let mut v: Vec<Iface> = rd
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if !is_physical(&p, &name) {
                return None;
            }
            Some(Iface { name, rx: p.join("statistics/rx_bytes"), tx: p.join("statistics/tx_bytes") })
        })
        .collect();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
}

fn is_physical(p: &Path, name: &str) -> bool {
    if name == "lo" {
        return false;
    }
    // Real hardware sits under a bus device; virtual interfaces have no `device`.
    if p.join("device").exists() {
        return true;
    }
    // Wireless interfaces always have one too; USB tethering as well. Keep a
    // few common names that some drivers expose without a device link.
    name.starts_with("eth") || name.starts_with("en") || name.starts_with("wl")
}

impl NetSource {
    pub fn new() -> Self {
        Self { ifaces: scan(), last: None, rescan_at: Instant::now() }
    }

    pub fn describe(&self) -> Vec<String> {
        vec![format!("net: {}", if self.ifaces.is_empty() { "no physical interface".to_string() } else { self.ifaces.iter().map(|i| i.name.as_str()).collect::<Vec<_>>().join(", ") })]
    }

    fn counters(&self) -> Vec<Counters> {
        self.ifaces
            .iter()
            .map(|i| Counters { rx: read_i64(&i.rx).unwrap_or(0).max(0) as u64, tx: read_i64(&i.tx).unwrap_or(0).max(0) as u64 })
            .collect()
    }

    pub fn read(&mut self) -> NetMetrics {
        // Interfaces come and go (USB adapters, Wi-Fi): rescan every 30 s.
        if self.rescan_at.elapsed().as_secs() >= 30 {
            let fresh = scan();
            if fresh.iter().map(|i| &i.name).ne(self.ifaces.iter().map(|i| &i.name)) {
                self.ifaces = fresh;
                self.last = None;
            }
            self.rescan_at = Instant::now();
        }
        let now = Instant::now();
        let cur = self.counters();
        let mut m = NetMetrics { iface: String::new(), down_bps: None, up_bps: None, total_down_mb: None, total_up_mb: None };
        if !cur.is_empty() {
            m.total_down_mb = Some(cur.iter().map(|c| c.rx).sum::<u64>() as f32 / 1048576.0);
            m.total_up_mb = Some(cur.iter().map(|c| c.tx).sum::<u64>() as f32 / 1048576.0);
        }
        if let Some((prev, t)) = &self.last {
            let dt = now.duration_since(*t).as_secs_f64();
            if dt > 0.0 && prev.len() == cur.len() {
                let mut down = 0.0f64;
                let mut up = 0.0f64;
                let mut busiest = (0u64, 0usize);
                for (i, (p, c)) in prev.iter().zip(cur.iter()).enumerate() {
                    let drx = c.rx.saturating_sub(p.rx);
                    let dtx = c.tx.saturating_sub(p.tx);
                    down += drx as f64;
                    up += dtx as f64;
                    if drx + dtx > busiest.0 {
                        busiest = (drx + dtx, i);
                    }
                }
                m.down_bps = Some((down / dt) as f32);
                m.up_bps = Some((up / dt) as f32);
                m.iface = self.ifaces.get(busiest.1).map(|i| i.name.clone()).unwrap_or_default();
            }
        }
        self.last = Some((cur, now));
        m
    }
}

impl Default for NetSource {
    fn default() -> Self {
        Self::new()
    }
}
