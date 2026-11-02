//! Raster media for themes: still images, animated GIFs and videos (via ffmpeg).
//!
//! Every source is decoded once and scaled to its target box; the renderer
//! then just blits the current frame.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{sync_channel, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use image::{AnimationDecoder, RgbaImage};
use resvg::tiny_skia::Pixmap;
use tracing::{debug, warn};

use super::theme::Fit;
use crate::{Error, Result};

/// Convert a straight-alpha RGBA image to a premultiplied tiny-skia pixmap.
pub fn to_pixmap(img: &RgbaImage) -> Pixmap {
    let mut data = img.as_raw().clone();
    for px in data.chunks_exact_mut(4) {
        let a = px[3] as u32;
        if a < 255 {
            px[0] = ((px[0] as u32 * a + 127) / 255) as u8;
            px[1] = ((px[1] as u32 * a + 127) / 255) as u8;
            px[2] = ((px[2] as u32 * a + 127) / 255) as u8;
        }
    }
    Pixmap::from_vec(data, resvg::tiny_skia::IntSize::from_wh(img.width(), img.height()).expect("non-zero image"))
        .expect("pixmap from rgba")
}

/// Scale `img` into a `w`×`h` box following `fit` (transparent padding for `Contain`).
pub fn fit_image(img: &RgbaImage, w: u32, h: u32, fit: Fit) -> RgbaImage {
    use image::imageops::{self, FilterType};
    let (w, h) = (w.max(1), h.max(1));
    if img.width() == w && img.height() == h {
        return img.clone();
    }
    let filter = if img.width() > w { FilterType::Triangle } else { FilterType::CatmullRom };
    match fit {
        Fit::Stretch => imageops::resize(img, w, h, filter),
        Fit::Cover => {
            let scale = (w as f32 / img.width() as f32).max(h as f32 / img.height() as f32);
            let sw = (img.width() as f32 * scale).round().max(1.0) as u32;
            let sh = (img.height() as f32 * scale).round().max(1.0) as u32;
            let scaled = imageops::resize(img, sw, sh, filter);
            imageops::crop_imm(&scaled, (sw - w) / 2, (sh - h) / 2, w, h).to_image()
        }
        Fit::Contain => {
            let scale = (w as f32 / img.width() as f32).min(h as f32 / img.height() as f32);
            let sw = (img.width() as f32 * scale).round().max(1.0) as u32;
            let sh = (img.height() as f32 * scale).round().max(1.0) as u32;
            let scaled = imageops::resize(img, sw, sh, filter);
            let mut out = RgbaImage::from_pixel(w, h, image::Rgba([0, 0, 0, 0]));
            imageops::overlay(&mut out, &scaled, ((w - sw) / 2) as i64, ((h - sh) / 2) as i64);
            out
        }
    }
}

pub enum Source {
    Still(Pixmap),
    Gif {
        frames: Vec<(Pixmap, Duration)>,
        total: Duration,
        started: Instant,
    },
    Video(VideoStream),
}

impl Source {
    /// Decode `path` and scale it to `w`×`h`.
    pub fn load(path: &Path, w: u32, h: u32, fit: Fit) -> Result<Self> {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        match ext.as_str() {
            "gif" => Self::load_gif(path, w, h, fit),
            "mp4" | "mkv" | "webm" | "mov" | "avi" | "m4v" => Ok(Source::Video(VideoStream::start(path, w, h, fit)?)),
            _ => {
                let img = image::open(path)
                    .map_err(|e| Error::Image(format!("{}: {e}", path.display())))?
                    .to_rgba8();
                Ok(Source::Still(to_pixmap(&fit_image(&img, w, h, fit))))
            }
        }
    }

    fn load_gif(path: &Path, w: u32, h: u32, fit: Fit) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        let decoder = image::codecs::gif::GifDecoder::new(std::io::BufReader::new(file))
            .map_err(|e| Error::Image(format!("{}: {e}", path.display())))?;
        let mut frames = Vec::new();
        let mut total = Duration::ZERO;
        for f in decoder.into_frames() {
            let f = f.map_err(|e| Error::Image(format!("{}: {e}", path.display())))?;
            let (num, den) = f.delay().numer_denom_ms();
            let ms = if den == 0 { 100 } else { (num / den).max(20) };
            let d = Duration::from_millis(ms as u64);
            total += d;
            frames.push((to_pixmap(&fit_image(&f.into_buffer(), w, h, fit)), d));
        }
        if frames.is_empty() {
            return Err(Error::Image(format!("{}: GIF has no frames", path.display())));
        }
        if frames.len() == 1 {
            return Ok(Source::Still(frames.remove(0).0));
        }
        debug!(frames = frames.len(), "loaded GIF");
        Ok(Source::Gif { frames, total, started: Instant::now() })
    }

    /// The frame to draw right now.
    pub fn current(&mut self) -> Option<&Pixmap> {
        match self {
            Source::Still(p) => Some(p),
            Source::Gif { frames, total, started } => {
                let mut t = started.elapsed();
                if *total > Duration::ZERO {
                    t = Duration::from_nanos((t.as_nanos() % total.as_nanos()) as u64);
                }
                let mut acc = Duration::ZERO;
                for (p, d) in frames.iter() {
                    acc += *d;
                    if t < acc {
                        return Some(p);
                    }
                }
                frames.last().map(|(p, _)| p)
            }
            Source::Video(v) => v.current(),
        }
    }
}

/// Frames from `ffmpeg` as raw RGBA at a fixed rate, looping forever.
pub struct VideoStream {
    child: Child,
    rx: Receiver<Vec<u8>>,
    w: u32,
    h: u32,
    frame: Option<Pixmap>,
    next_at: Instant,
    interval: Duration,
}

impl VideoStream {
    pub const FPS: u32 = 15;

    pub fn start(path: &Path, w: u32, h: u32, fit: Fit) -> Result<Self> {
        let vf = match fit {
            Fit::Cover => format!("scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h}"),
            Fit::Contain => format!("scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:color=black@0"),
            Fit::Stretch => format!("scale={w}:{h}"),
        };
        let mut child = Command::new("ffmpeg")
            .args([
                "-hide_banner", "-loglevel", "error", "-nostdin",
                "-stream_loop", "-1",
                "-i",
            ])
            .arg(path)
            .args([
                "-an",
                "-vf", &vf,
                "-r", &Self::FPS.to_string(),
                "-f", "rawvideo", "-pix_fmt", "rgba",
                "pipe:1",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    Error::Image("video needs `ffmpeg` installed".into())
                } else {
                    Error::Io(e)
                }
            })?;
        let mut stdout = child.stdout.take().expect("piped stdout");
        let frame_len = (w * h * 4) as usize;
        let (tx, rx) = sync_channel::<Vec<u8>>(2);
        std::thread::Builder::new()
            .name("neru-video".into())
            .spawn(move || {
                loop {
                    let mut buf = vec![0u8; frame_len];
                    if stdout.read_exact(&mut buf).is_err() {
                        break;
                    }
                    if tx.send(buf).is_err() {
                        break;
                    }
                }
            })
            .expect("spawn video reader");
        Ok(Self {
            child,
            rx,
            w,
            h,
            frame: None,
            next_at: Instant::now(),
            interval: Duration::from_secs_f64(1.0 / Self::FPS as f64),
        })
    }

    fn current(&mut self) -> Option<&Pixmap> {
        if self.frame.is_none() || Instant::now() >= self.next_at {
            // Block briefly for the very first frame so a still render shows the video.
            let got = if self.frame.is_none() {
                self.rx.recv_timeout(Duration::from_millis(1500)).map_err(|e| match e {
                    std::sync::mpsc::RecvTimeoutError::Timeout => TryRecvError::Empty,
                    std::sync::mpsc::RecvTimeoutError::Disconnected => TryRecvError::Disconnected,
                })
            } else {
                self.rx.try_recv()
            };
            match got {
                Ok(buf) => {
                    if let Some(img) = RgbaImage::from_raw(self.w, self.h, buf) {
                        self.frame = Some(to_pixmap(&img));
                    }
                    self.next_at = Instant::now() + self.interval;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    if self.frame.is_none() {
                        warn!("video stream ended without frames");
                    }
                }
            }
        }
        self.frame.as_ref()
    }
}

impl Drop for VideoStream {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Hash, PartialEq, Eq, Clone)]
struct Key {
    path: PathBuf,
    w: u32,
    h: u32,
    fit: Fit,
}

/// Loaded sources, keyed by path and target size.
#[derive(Default)]
pub struct MediaCache {
    entries: HashMap<Key, Result<Source>>,
    media_dir: Option<PathBuf>,
}

impl MediaCache {
    pub fn new(media_dir: Option<PathBuf>) -> Self {
        Self { entries: HashMap::new(), media_dir }
    }

    /// Absolute paths are used as-is; bare names resolve inside the media dir.
    pub fn resolve(&self, path: &str) -> PathBuf {
        let p = Path::new(path);
        if p.is_absolute() {
            return p.to_path_buf();
        }
        match &self.media_dir {
            Some(d) => d.join(p),
            None => p.to_path_buf(),
        }
    }

    pub fn get(&mut self, path: &str, w: u32, h: u32, fit: Fit) -> Option<&mut Source> {
        let key = Key { path: self.resolve(path), w, h, fit };
        if !self.entries.contains_key(&key) {
            let loaded = Source::load(&key.path, w, h, fit);
            if let Err(e) = &loaded {
                warn!(path, error = %e, "could not load media");
            }
            self.entries.insert(key.clone(), loaded);
        }
        self.entries.get_mut(&key).and_then(|r| r.as_mut().ok())
    }

    /// Drop everything (e.g. when the theme changes) so files are re-read.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Error message for a path that failed to load, if any.
    pub fn error_for(&self, path: &str) -> Option<String> {
        let resolved = self.resolve(path);
        self.entries
            .iter()
            .find(|(k, v)| k.path == resolved && v.is_err())
            .and_then(|(_, v)| v.as_ref().err().map(|e| e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_modes_produce_box_size() {
        let img = RgbaImage::from_pixel(100, 50, image::Rgba([255, 0, 0, 255]));
        for fit in [Fit::Cover, Fit::Contain, Fit::Stretch] {
            let out = fit_image(&img, 40, 40, fit);
            assert_eq!((out.width(), out.height()), (40, 40));
        }
        // Contain pads with transparency above/below.
        let c = fit_image(&img, 40, 40, Fit::Contain);
        assert_eq!(c.get_pixel(0, 0)[3], 0);
        assert_eq!(c.get_pixel(20, 20)[3], 255);
        // Cover crops: everything opaque.
        let v = fit_image(&img, 40, 40, Fit::Cover);
        assert_eq!(v.get_pixel(0, 0)[3], 255);
    }

    #[test]
    fn premultiplies_alpha() {
        let img = RgbaImage::from_pixel(1, 1, image::Rgba([200, 100, 0, 128]));
        let p = to_pixmap(&img);
        let px = p.pixels()[0];
        assert_eq!(px.alpha(), 128);
        assert!((px.red() as i32 - 100).abs() <= 1);
    }
}
