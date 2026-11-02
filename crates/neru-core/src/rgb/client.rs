//! Blocking OpenRGB SDK client over TCP.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use tracing::{debug, info};

use super::protocol::{self as p, id, Color, Controller, Mode};
use crate::{Error, Result};

pub const DEFAULT_ADDR: &str = "127.0.0.1:6742";
pub const CLIENT_NAME: &str = "Neru Screen Control";

pub struct OpenRgbClient {
    stream: TcpStream,
    pub version: u32,
    list_dirty: bool,
}

impl OpenRgbClient {
    pub fn connect(addr: &str) -> Result<Self> {
        let sock = addr
            .to_socket_addrs()
            .map_err(|e| Error::Protocol(format!("bad OpenRGB address {addr}: {e}")))?
            .next()
            .ok_or_else(|| Error::Protocol(format!("bad OpenRGB address {addr}")))?;
        let stream = TcpStream::connect_timeout(&sock, Duration::from_millis(800))
            .map_err(|e| Error::Protocol(format!("OpenRGB server not reachable at {addr}: {e}")))?;
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(Duration::from_secs(3)))?;
        let mut c = Self { stream, version: 0, list_dirty: false };

        // Version negotiation: servers older than protocol 1 never answer.
        c.send(0, id::REQUEST_PROTOCOL_VERSION, &p::PROTOCOL_VERSION.to_le_bytes())?;
        c.stream.set_read_timeout(Some(Duration::from_millis(1000)))?;
        match c.wait_for(id::REQUEST_PROTOCOL_VERSION) {
            Ok(payload) if payload.len() >= 4 => {
                let server = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
                c.version = server.min(p::PROTOCOL_VERSION);
            }
            _ => c.version = 0,
        }
        c.stream.set_read_timeout(Some(Duration::from_secs(3)))?;
        let mut name = CLIENT_NAME.as_bytes().to_vec();
        name.push(0);
        c.send(0, id::SET_CLIENT_NAME, &name)?;
        info!(addr, version = c.version, "connected to OpenRGB");
        Ok(c)
    }

    fn send(&mut self, dev: u32, pkt_id: u32, payload: &[u8]) -> Result<()> {
        let h = p::header(dev, pkt_id, payload.len() as u32);
        let mut buf = Vec::with_capacity(p::HEADER_LEN + payload.len());
        buf.extend_from_slice(&h);
        buf.extend_from_slice(payload);
        self.stream.write_all(&buf)?;
        Ok(())
    }

    fn recv(&mut self) -> Result<(u32, u32, Vec<u8>)> {
        let mut h = [0u8; p::HEADER_LEN];
        self.stream.read_exact(&mut h)?;
        let (dev, pkt_id, size) = p::parse_header(&h)?;
        if size as usize > p::MAX_PACKET {
            return Err(Error::Protocol(format!("OpenRGB packet too large ({size} bytes)")));
        }
        let mut payload = vec![0u8; size as usize];
        self.stream.read_exact(&mut payload)?;
        Ok((dev, pkt_id, payload))
    }

    /// Read until a packet with `want` arrives; notes device-list updates on the way.
    fn wait_for(&mut self, want: u32) -> Result<Vec<u8>> {
        loop {
            let (_, pkt_id, payload) = self.recv()?;
            if pkt_id == want {
                return Ok(payload);
            }
            if pkt_id == id::DEVICE_LIST_UPDATED {
                self.list_dirty = true;
            } else {
                debug!(pkt_id, "ignoring unexpected OpenRGB packet");
            }
        }
    }

    /// Non-blocking drain of pushed notifications (device list changes).
    pub fn poll_notifications(&mut self) -> Result<()> {
        self.stream.set_nonblocking(true)?;
        let mut h = [0u8; p::HEADER_LEN];
        let res = loop {
            match self.stream.peek(&mut h) {
                Ok(n) if n >= p::HEADER_LEN => {
                    self.stream.set_nonblocking(false)?;
                    let (_, pkt_id, _) = self.recv()?;
                    if pkt_id == id::DEVICE_LIST_UPDATED {
                        self.list_dirty = true;
                    }
                    self.stream.set_nonblocking(true)?;
                }
                Ok(0) => break Err(Error::Protocol("OpenRGB server closed the connection".into())),
                Ok(_) => break Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break Ok(()),
                Err(e) => break Err(e.into()),
            }
        };
        self.stream.set_nonblocking(false)?;
        res
    }

    pub fn take_list_dirty(&mut self) -> bool {
        std::mem::take(&mut self.list_dirty)
    }

    pub fn controller_count(&mut self) -> Result<u32> {
        self.send(0, id::REQUEST_CONTROLLER_COUNT, &[])?;
        let payload = self.wait_for(id::REQUEST_CONTROLLER_COUNT)?;
        if payload.len() < 4 {
            return Err(Error::Protocol("short controller count reply".into()));
        }
        Ok(u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]))
    }

    pub fn controller(&mut self, index: u32) -> Result<Controller> {
        self.send(index, id::REQUEST_CONTROLLER_DATA, &self.version.to_le_bytes())?;
        let payload = self.wait_for(id::REQUEST_CONTROLLER_DATA)?;
        p::parse_controller(&payload, self.version, index)
    }

    pub fn controllers(&mut self) -> Result<Vec<Controller>> {
        let n = self.controller_count()?;
        (0..n).map(|i| self.controller(i)).collect()
    }

    pub fn update_leds(&mut self, dev: u32, colors: &[Color]) -> Result<()> {
        self.send(dev, id::UPDATE_LEDS, &p::encode_update_leds(colors))
    }

    pub fn update_zone_leds(&mut self, dev: u32, zone: u32, colors: &[Color]) -> Result<()> {
        self.send(dev, id::UPDATE_ZONE_LEDS, &p::encode_update_zone_leds(zone, colors))
    }

    pub fn set_custom_mode(&mut self, dev: u32) -> Result<()> {
        self.send(dev, id::SET_CUSTOM_MODE, &[])
    }

    pub fn update_mode(&mut self, dev: u32, mode_idx: u32, mode: &Mode) -> Result<()> {
        self.send(dev, id::UPDATE_MODE, &p::encode_update_mode(mode_idx, mode, self.version))
    }

    pub fn save_mode(&mut self, dev: u32, mode_idx: u32, mode: &Mode) -> Result<()> {
        self.send(dev, id::SAVE_MODE, &p::encode_update_mode(mode_idx, mode, self.version))
    }

    pub fn rescan(&mut self) -> Result<()> {
        self.send(0, id::REQUEST_RESCAN_DEVICES, &[])
    }

    pub fn profiles(&mut self) -> Result<Vec<String>> {
        if self.version < 2 {
            return Ok(Vec::new());
        }
        self.send(0, id::PROFILE_LIST, &[])?;
        let payload = self.wait_for(id::PROFILE_LIST)?;
        p::parse_profile_list(&payload)
    }

    fn profile_op(&mut self, pkt: u32, name: &str) -> Result<()> {
        if self.version < 2 {
            return Err(Error::Protocol("this OpenRGB server has no profile support".into()));
        }
        let mut b = name.as_bytes().to_vec();
        b.push(0);
        self.send(0, pkt, &b)
    }

    pub fn load_profile(&mut self, name: &str) -> Result<()> {
        self.profile_op(id::LOAD_PROFILE, name)
    }

    pub fn save_profile(&mut self, name: &str) -> Result<()> {
        self.profile_op(id::SAVE_PROFILE, name)
    }

    pub fn delete_profile(&mut self, name: &str) -> Result<()> {
        self.profile_op(id::DELETE_PROFILE, name)
    }
}
