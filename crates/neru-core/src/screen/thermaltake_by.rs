//! Thermaltake "BY" protocol screens (TH V3 Ultra 720×720, 6" LCD Panel Kit).
//!
//! The pump head runs embedded Linux and talks HTTP-like text over 1024-byte
//! HID reports. Commands are framed and checksummed; live frames are JPEG
//! slices without framing. Protocol notes (public reverse engineering of
//! TT RGB Plus by deniswsrosa and the Telinhas project) live in
//! `docs/PROTOCOLS.md`.
//!
//! Framing of a command:
//!
//! ```text
//! 5A | len_be16 | payload | sum8 | 5A
//! ```
//!
//! * `len` = whole unescaped frame length (payload + 5)
//! * `sum8` = (len bytes + payload) mod 256
//! * between the two `5A` delimiters, `5A` → `5B 01` and `5B` → `5B 02`
//!
//! A live frame is a JPEG (≤ `0xC7FFF` bytes) cut in 1000-byte slices, one
//! per report, each with a 24-byte header:
//!
//! ```text
//! 5C 03 FD | counter (0–59, +1 per frame) | total_be16 | index_be16 | 01 | zeros…
//! ```
//!
//! The screen returns to its own picture after `timeout` seconds (5 by
//! default) without frames, so still images are re-sent periodically.

use std::path::Path;
use std::time::{Duration, Instant};

use image::RgbImage;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use super::{Capabilities, Screen, ScreenModel, ScreenStatus};
use crate::hidraw::HidRaw;
use crate::{Error, Result};

pub const PACKET: usize = 1024;
const FRAME_HEADER: usize = 24;
const FRAME_DATA: usize = PACKET - FRAME_HEADER;
/// TT RGB Plus refuses larger frames.
pub const MAX_JPEG: usize = 0xC7FFF;
/// SeqNumber TT RGB Plus uses for these panels.
const SEQ: u32 = 112;
const FRAME_COUNTER_WRAP: u8 = 60;
const REPLY_TIMEOUT: Duration = Duration::from_secs(3);
/// Re-enable realtime mode if the last frame is older than this (the panel
/// itself gives up after `timeout`, 5 s by default).
const REALTIME_STALE: Duration = Duration::from_secs(4);
const ROTATIONS: &[u16] = &[0, 90, 180, 270];

// ── framing ─────────────────────────────────────────────────────────────────

/// Wrap a message in the 5A-delimited, checksummed, escaped frame.
pub fn frame(payload: &[u8]) -> Vec<u8> {
    let total = payload.len() + 5;
    let mut body = Vec::with_capacity(total + 8);
    body.push((total >> 8) as u8);
    body.push((total & 0xff) as u8);
    body.extend_from_slice(payload);
    let sum: u32 = body.iter().map(|&b| b as u32).sum();
    body.push((sum & 0xff) as u8);

    let mut out = Vec::with_capacity(total + 8);
    out.push(0x5a);
    for b in body {
        match b {
            0x5a => out.extend_from_slice(&[0x5b, 0x01]),
            0x5b => out.extend_from_slice(&[0x5b, 0x02]),
            _ => out.push(b),
        }
    }
    out.push(0x5a);
    out
}

/// Inverse of [`frame`]: the payload of the first frame found in `data`.
pub fn unframe(data: &[u8]) -> Result<Vec<u8>> {
    let start = data
        .iter()
        .position(|&b| b == 0x5a)
        .ok_or_else(|| Error::Protocol("reply has no frame start".into()))?;
    let rel_end = data[start + 1..]
        .iter()
        .position(|&b| b == 0x5a)
        .ok_or_else(|| Error::Protocol("reply has no frame end".into()))?;
    let escaped = &data[start + 1..start + 1 + rel_end];

    let mut body = Vec::with_capacity(escaped.len());
    let mut i = 0;
    while i < escaped.len() {
        if escaped[i] == 0x5b && i + 1 < escaped.len() {
            match escaped[i + 1] {
                0x01 => {
                    body.push(0x5a);
                    i += 2;
                    continue;
                }
                0x02 => {
                    body.push(0x5b);
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }
        body.push(escaped[i]);
        i += 1;
    }
    if body.len() < 3 {
        return Err(Error::Protocol("reply frame too short".into()));
    }
    let (content, sum) = body.split_at(body.len() - 1);
    let expect: u32 = content.iter().map(|&b| b as u32).sum();
    if (expect & 0xff) as u8 != sum[0] {
        return Err(Error::Protocol("reply checksum mismatch".into()));
    }
    Ok(content[2..].to_vec())
}

/// Build the HTTP-like request text for `POST <command> 1`.
pub fn message(verb: &str, body: &str) -> Vec<u8> {
    format!(
        "{verb}\r\nSeqNumber={SEQ}\r\nContentType=json\r\nContentLength={}\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

/// Split a reply into its status line and optional JSON body.
pub fn parse_reply(payload: &[u8]) -> Result<(String, Option<serde_json::Value>)> {
    let text = String::from_utf8_lossy(payload);
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let status = head.lines().next().unwrap_or("").trim().to_string();
    let body = body.trim_end_matches('\0').trim();
    let json = if body.is_empty() {
        None
    } else {
        Some(serde_json::from_str(body).map_err(|e| {
            Error::Protocol(format!("bad JSON in reply ({e}): {body}"))
        })?)
    };
    Ok((status, json))
}

/// Cut a JPEG into frame slices (each exactly `PACKET` bytes).
pub fn image_slices(jpeg: &[u8], counter: u8) -> Result<Vec<Vec<u8>>> {
    if jpeg.is_empty() || jpeg.len() > MAX_JPEG {
        return Err(Error::InvalidValue(format!(
            "JPEG is {} bytes, limit {MAX_JPEG}",
            jpeg.len()
        )));
    }
    let total = jpeg.len().div_ceil(FRAME_DATA);
    let mut out = Vec::with_capacity(total);
    for (idx, chunk) in jpeg.chunks(FRAME_DATA).enumerate() {
        let mut report = vec![0u8; PACKET];
        report[0] = 0x5c;
        report[1] = 0x03;
        report[2] = 0xfd;
        report[3] = counter;
        report[4] = (total >> 8) as u8;
        report[5] = (total & 0xff) as u8;
        report[6] = (idx >> 8) as u8;
        report[7] = (idx & 0xff) as u8;
        report[8] = 0x01;
        report[FRAME_HEADER..FRAME_HEADER + chunk.len()].copy_from_slice(chunk);
        out.push(report);
    }
    Ok(out)
}

// ── status document ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ByVersion {
    #[serde(default)]
    pub app: String,
    #[serde(default)]
    pub firmware: String,
    #[serde(default)]
    pub sdk: String,
    #[serde(default)]
    pub hardware: String,
}

/// Reply to `POST conn 1`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ByStatus {
    #[serde(default, rename = "OS")]
    pub os: String,
    #[serde(default)]
    pub version: ByVersion,
    #[serde(default)]
    pub brightness: u8,
    #[serde(default)]
    pub degree: u16,
    #[serde(default)]
    pub timeout: u32,
    #[serde(default)]
    pub sn: String,
    #[serde(default, rename = "osdState")]
    pub osd_state: i64,
    #[serde(default)]
    pub mode: i64,
    #[serde(default, rename = "bootFinish")]
    pub boot_finish: i64,
}

// ── driver ──────────────────────────────────────────────────────────────────

pub struct ThermaltakeBy {
    hid: HidRaw,
    model: &'static ScreenModel,
    counter: u8,
    realtime: bool,
    last_frame: Option<Instant>,
    pub jpeg_quality: u8,
}

impl ThermaltakeBy {
    pub fn open(model: &'static ScreenModel, node: &Path) -> Result<Self> {
        let hid = HidRaw::open(node)?;
        info!(node = %node.display(), model = model.name, "opened Thermaltake BY screen");
        Ok(Self {
            hid,
            model,
            counter: 0,
            realtime: false,
            last_frame: None,
            jpeg_quality: 85,
        })
    }

    /// Send `POST <command> 1` with a JSON body and wait for the `200` reply.
    pub fn command(&mut self, command: &str, body: &str) -> Result<(String, Option<serde_json::Value>)> {
        self.hid.drain()?;
        let mut packet = frame(&message(&format!("POST {command} 1"), body));
        if packet.len() > PACKET {
            return Err(Error::InvalidValue(format!(
                "command {command} does not fit in one report"
            )));
        }
        packet.resize(PACKET, 0);
        debug!(command, body, "→");
        self.hid.write_report(&packet)?;

        let deadline = Instant::now() + REPLY_TIMEOUT;
        let mut buf = vec![0u8; PACKET + 1];
        while Instant::now() < deadline {
            let Some(n) = self.hid.read_report(&mut buf, 200)? else {
                continue;
            };
            let payload = match unframe(&buf[..n]) {
                Ok(p) => p,
                Err(e) => {
                    debug!(error = %e, "ignoring unframeable report");
                    continue;
                }
            };
            let (status, json) = parse_reply(&payload)?;
            debug!(status, ?json, "←");
            if !status.ends_with(" 200") {
                return Err(Error::Protocol(format!("{command}: {status}")));
            }
            return Ok((status, json));
        }
        Err(Error::Timeout(format!("reply to {command}")))
    }

    /// Full handshake; the screen's status document.
    pub fn conn(&mut self) -> Result<ByStatus> {
        // A new `conn` restarts the session: realtime must be re-enabled.
        self.realtime = false;
        let (_, json) = self.command("conn", "")?;
        let json = json.ok_or_else(|| Error::Protocol("conn reply has no body".into()))?;
        Ok(serde_json::from_value(json)?)
    }

    pub fn set_realtime(&mut self, enable: bool) -> Result<()> {
        self.command("realtimeDisplay", &format!("{{\"enable\": {enable}}}"))?;
        self.realtime = enable;
        Ok(())
    }

    /// Push an already-encoded JPEG of the screen's native size.
    pub fn send_jpeg(&mut self, jpeg: &[u8]) -> Result<()> {
        let stale = self
            .last_frame
            .map_or(true, |t| t.elapsed() > REALTIME_STALE);
        if !self.realtime || stale {
            self.set_realtime(true)?;
        }
        for slice in image_slices(jpeg, self.counter)? {
            self.hid.write_report(&slice)?;
        }
        self.counter = (self.counter + 1) % FRAME_COUNTER_WRAP;
        self.last_frame = Some(Instant::now());
        Ok(())
    }

    /// Encode an RGB image as a baseline JPEG that fits the protocol limit.
    pub fn encode_jpeg(&mut self, frame: &RgbImage) -> Result<Vec<u8>> {
        encode_jpeg(frame, self.model.width, self.model.height, self.jpeg_quality)
    }
}

/// Encode `frame` as a baseline JPEG of `w`×`h`, lowering the quality until
/// it fits [`MAX_JPEG`].
pub fn encode_jpeg(frame: &RgbImage, w: u32, h: u32, quality: u8) -> Result<Vec<u8>> {
    let resized;
    let src = if frame.width() == w && frame.height() == h {
        frame
    } else {
        warn!(
            got = format!("{}×{}", frame.width(), frame.height()),
            want = format!("{w}×{h}"),
            "frame has the wrong size; resizing"
        );
        resized = image::imageops::resize(frame, w, h, image::imageops::FilterType::Triangle);
        &resized
    };
    let mut quality = quality.clamp(10, 100);
    loop {
        let mut out = Vec::with_capacity(96 * 1024);
        let enc = jpeg_encoder::Encoder::new(&mut out, quality);
        enc.encode(src.as_raw(), w as u16, h as u16, jpeg_encoder::ColorType::Rgb)
            .map_err(|e| Error::Image(e.to_string()))?;
        if out.len() <= MAX_JPEG || quality <= 10 {
            return Ok(out);
        }
        quality -= 10;
    }
}

impl Screen for ThermaltakeBy {
    fn model(&self) -> &ScreenModel {
        self.model
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            brightness: true,
            hw_rotations: ROTATIONS,
        }
    }

    fn connect(&mut self) -> Result<ScreenStatus> {
        let s = self.conn()?;
        Ok(ScreenStatus {
            firmware: Some(s.version.firmware.clone()).filter(|f| !f.is_empty()),
            brightness: Some(s.brightness),
            rotation: Some(s.degree),
            idle_timeout_s: Some(s.timeout),
            serial: Some(s.sn.clone()).filter(|v| !v.is_empty()),
            raw: serde_json::to_value(&s).ok(),
        })
    }

    fn show(&mut self, frame: &RgbImage) -> Result<()> {
        let jpeg = self.encode_jpeg(frame)?;
        self.send_jpeg(&jpeg)
    }

    fn set_brightness(&mut self, value: u8) -> Result<()> {
        if value > 100 {
            return Err(Error::InvalidValue(format!("brightness {value} (use 0–100)")));
        }
        self.command("brightness", &format!("{{\"value\": {value}}}"))?;
        Ok(())
    }

    fn set_rotation(&mut self, degrees: u16) -> Result<()> {
        if !ROTATIONS.contains(&degrees) {
            return Err(Error::InvalidValue(format!(
                "rotation {degrees} (use 0, 90, 180 or 270)"
            )));
        }
        self.command("rotate", &format!("{{\"degree\": {degrees}}}"))?;
        Ok(())
    }

    fn keepalive(&self) -> Option<Duration> {
        Some(Duration::from_secs(2))
    }

    fn set_jpeg_quality(&mut self, quality: u8) {
        self.jpeg_quality = quality.clamp(10, 100);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_roundtrip_with_escapes() {
        // Payload containing both escape-worthy bytes.
        let payload = b"ab\x5acd\x5bef";
        let f = frame(payload);
        assert_eq!(f[0], 0x5a);
        assert_eq!(*f.last().unwrap(), 0x5a);
        // No raw 5A inside the delimiters.
        assert!(!f[1..f.len() - 1].contains(&0x5a));
        assert_eq!(unframe(&f).unwrap(), payload);
    }

    #[test]
    fn frame_layout_matches_protocol() {
        let payload = b"hi";
        let f = frame(payload);
        // 5A | 00 07 | 'h' 'i' | sum | 5A   (len = 2 + 5 = 7)
        assert_eq!(f, vec![0x5a, 0x00, 0x07, b'h', b'i', (0x07 + b'h' as u32 + b'i' as u32) as u8, 0x5a]);
    }

    #[test]
    fn brightness_message_has_escaped_length() {
        // The brightness request is exactly 0x5A bytes long, which exercises
        // the length-field escaping noted in the protocol docs.
        let m = message("POST brightness 1", "{\"value\": 30}");
        let f = frame(&m);
        assert!(f.len() > m.len() + 5);
        assert_eq!(unframe(&f).unwrap(), m);
    }

    #[test]
    fn reply_parses_status_and_json() {
        let payload = b"1 200\r\nAckNumber=113\r\nContentType=json\r\nContentLength=15\r\n\r\n{\"brightness\":7}";
        let (status, json) = parse_reply(payload).unwrap();
        assert_eq!(status, "1 200");
        assert_eq!(json.unwrap()["brightness"], 7);
    }

    #[test]
    fn slices_have_headers_and_padding() {
        let jpeg = vec![0xabu8; FRAME_DATA + 10];
        let slices = image_slices(&jpeg, 3).unwrap();
        assert_eq!(slices.len(), 2);
        assert_eq!(&slices[0][..9], &[0x5c, 0x03, 0xfd, 3, 0, 2, 0, 0, 1]);
        assert_eq!(&slices[1][..9], &[0x5c, 0x03, 0xfd, 3, 0, 2, 0, 1, 1]);
        assert_eq!(slices[1][FRAME_HEADER + 10], 0);
        assert!(slices.iter().all(|s| s.len() == PACKET));
        assert!(image_slices(&[], 0).is_err());
    }

    #[test]
    fn jpeg_encoding_fits_limit() {
        let img = RgbImage::from_fn(720, 720, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x ^ y) % 256) as u8]));
        let jpeg = encode_jpeg(&img, 720, 720, 85).unwrap();
        assert!(jpeg.len() <= MAX_JPEG);
        assert_eq!(&jpeg[..2], &[0xff, 0xd8]);
        // A wrong-sized frame is resized rather than rejected.
        let small = RgbImage::new(10, 10);
        assert!(encode_jpeg(&small, 720, 720, 85).is_ok());
    }

    #[test]
    fn status_document_deserializes() {
        let doc = r#"{"OS":"Linux","version":{"app":"V1.0.3","firmware":"V1.0.3","sdk":"V1.2.7","hardware":"V1.0"},"space":{"total":1,"free":1},"brightness":100,"degree":0,"sn":"X","osdState":0,"mode":0,"logo":1,"timeout":5,"bootFinish":1}"#;
        let s: ByStatus = serde_json::from_str(doc).unwrap();
        assert_eq!(s.brightness, 100);
        assert_eq!(s.timeout, 5);
        assert_eq!(s.version.firmware, "V1.0.3");
    }
}
