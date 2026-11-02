//! Theme renderer: theme + metrics → RGB frame.
//!
//! Vector elements become SVG and are rasterised with resvg; images, GIFs
//! and videos are decoded once and blitted as raster layers. Elements are
//! drawn in order, so an image above a bar covers it and vice versa.

pub mod media;
pub mod svg;
pub mod theme;
pub mod values;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use image::RgbImage;
use resvg::tiny_skia::{Pixmap, PixmapPaint, Transform};
use tracing::{debug, info, warn};

pub use theme::Theme;

use self::media::MediaCache;
use self::theme::{Background, Kind};
use self::values::Values;
use crate::metrics::Snapshot;
use crate::{Error, Result};

/// Loaded fonts shared by every renderer (and listed by the editor).
pub struct FontBook {
    pub db: Arc<resvg::usvg::fontdb::Database>,
    pub bundled_dir: Option<PathBuf>,
}

impl FontBook {
    /// Load the bundled fonts (if a directory is given or can be found) and
    /// the system fonts.
    pub fn load(bundled: Option<&Path>) -> Self {
        let mut db = resvg::usvg::fontdb::Database::new();
        let bundled_dir = bundled.map(Path::to_path_buf).or_else(find_bundled_fonts);
        if let Some(dir) = &bundled_dir {
            db.load_fonts_dir(dir);
            info!(dir = %dir.display(), faces = db.len(), "loaded bundled fonts");
        } else {
            warn!("bundled fonts not found; set NERU_ASSETS to the assets directory");
        }
        db.load_system_fonts();
        db.set_sans_serif_family("Inter");
        db.set_monospace_family("JetBrains Mono");
        Self { db: Arc::new(db), bundled_dir }
    }

    /// Family names available to themes, sorted, bundled ones first.
    pub fn families(&self) -> Vec<String> {
        let mut bundled: Vec<String> = Vec::new();
        let mut system: Vec<String> = Vec::new();
        for face in self.db.faces() {
            let Some((fam, _)) = face.families.first() else { continue };
            let is_bundled = match (&self.bundled_dir, &face.source) {
                (Some(dir), resvg::usvg::fontdb::Source::File(p)) => p.starts_with(dir),
                _ => false,
            };
            let list = if is_bundled { &mut bundled } else { &mut system };
            if !list.contains(fam) {
                list.push(fam.clone());
            }
        }
        bundled.sort();
        system.sort();
        system.retain(|f| !bundled.contains(f));
        bundled.extend(system);
        bundled
    }
}

static ASSETS_OVERRIDE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Tell the library where the bundled `assets/` directory is (the desktop app
/// calls this with its resource directory). Only the first call counts.
pub fn set_assets_dir(dir: PathBuf) {
    let _ = ASSETS_OVERRIDE.set(dir);
}

/// Where the bundled `assets/` directory lives: [`set_assets_dir`],
/// `$NERU_ASSETS`, next to the executable (dev builds), or the system install
/// location.
pub fn find_assets_dir() -> Option<PathBuf> {
    if let Some(p) = ASSETS_OVERRIDE.get() {
        if p.is_dir() {
            return Some(p.clone());
        }
    }
    if let Some(p) = std::env::var_os("NERU_ASSETS") {
        let p = PathBuf::from(p);
        if p.is_dir() {
            return Some(p);
        }
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(Path::to_path_buf);
        // target/debug/neru → ../../assets ; also /usr/bin/neru → /usr/share/…
        for _ in 0..4 {
            if let Some(d) = &dir {
                candidates.push(d.join("assets"));
                dir = d.parent().map(Path::to_path_buf);
            }
        }
    }
    candidates.push(PathBuf::from("/usr/share/neru-screen-control/assets"));
    candidates.push(PathBuf::from("/usr/local/share/neru-screen-control/assets"));
    candidates.push(PathBuf::from("assets"));
    candidates.into_iter().find(|c| c.join("fonts").is_dir())
}

fn find_bundled_fonts() -> Option<PathBuf> {
    find_assets_dir().map(|a| a.join("fonts"))
}

/// Theme files shipped with the app (`assets/themes/*.json`), sorted.
pub fn bundled_themes() -> Vec<PathBuf> {
    let Some(dir) = find_assets_dir().map(|a| a.join("themes")) else {
        return Vec::new();
    };
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().map_or(false, |e| e == "json"))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// Read and validate a theme file.
pub fn load_theme(path: &Path) -> Result<Theme> {
    let s = std::fs::read_to_string(path).map_err(|e| Error::Config(format!("{}: {e}", path.display())))?;
    Theme::from_json(&s).map_err(|e| Error::Config(format!("{}: {e}", path.display())))
}

/// Timing of the last render, for the UI.
#[derive(Debug, Clone, Copy, Default)]
pub struct RenderStats {
    pub total_ms: f32,
    pub svg_ms: f32,
    pub media_ms: f32,
}

pub struct Renderer {
    theme: Theme,
    fonts: Arc<resvg::usvg::fontdb::Database>,
    media: MediaCache,
    smoothed: HashMap<String, f32>,
    canvas: Pixmap,
    out: RgbImage,
    pub fahrenheit: bool,
    pub stats: RenderStats,
}

impl Renderer {
    pub fn new(theme: Theme, fonts: Arc<resvg::usvg::fontdb::Database>, media_dir: Option<PathBuf>) -> Result<Self> {
        let canvas = Pixmap::new(theme.width, theme.height)
            .ok_or_else(|| Error::InvalidValue("theme size must be > 0".into()))?;
        Ok(Self {
            out: RgbImage::new(theme.width, theme.height),
            theme,
            fonts,
            media: MediaCache::new(media_dir),
            smoothed: HashMap::new(),
            canvas,
            fahrenheit: false,
            stats: RenderStats::default(),
        })
    }

    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    pub fn set_theme(&mut self, theme: Theme) -> Result<()> {
        if theme.width != self.theme.width || theme.height != self.theme.height {
            self.canvas = Pixmap::new(theme.width, theme.height)
                .ok_or_else(|| Error::InvalidValue("theme size must be > 0".into()))?;
            self.out = RgbImage::new(theme.width, theme.height);
        }
        self.media.clear();
        self.theme = theme;
        Ok(())
    }

    /// Replace the theme without dropping decoded media (live editing).
    pub fn update_theme(&mut self, theme: Theme) -> Result<()> {
        if theme.width != self.theme.width || theme.height != self.theme.height {
            return self.set_theme(theme);
        }
        self.theme = theme;
        Ok(())
    }

    /// Resize a theme designed for another screen: scales every coordinate.
    pub fn scale_theme(theme: &Theme, width: u32, height: u32) -> Theme {
        let sx = width as f32 / theme.width as f32;
        let sy = height as f32 / theme.height as f32;
        let s = sx.min(sy);
        let mut t = theme.clone();
        t.width = width;
        t.height = height;
        for el in &mut t.elements {
            el.x *= sx;
            el.y *= sy;
            match &mut el.kind {
                Kind::Text(x) => x.size *= s,
                Kind::Bar(b) => {
                    b.w *= sx;
                    b.h *= sy;
                    b.radius *= s;
                }
                Kind::Gauge(g) => {
                    g.r *= s;
                    g.thickness *= s;
                }
                Kind::Rect(r) => {
                    r.w *= sx;
                    r.h *= sy;
                    r.radius *= s;
                }
                Kind::Ellipse(e) => {
                    e.rx *= sx;
                    e.ry *= sy;
                }
                Kind::Line(l) => {
                    l.w *= sx;
                    l.h *= sy;
                }
                Kind::Image(i) => {
                    i.w *= sx;
                    i.h *= sy;
                }
            }
        }
        t
    }

    /// Media files that failed to load (path → error), for the editor.
    pub fn media_errors(&self) -> Vec<(String, String)> {
        let mut v = Vec::new();
        if let Background::Media { path, .. } = &self.theme.background {
            if let Some(e) = self.media.error_for(path) {
                v.push((path.clone(), e));
            }
        }
        for el in &self.theme.elements {
            if let Kind::Image(img) = &el.kind {
                if let Some(e) = self.media.error_for(&img.path) {
                    v.push((img.path.clone(), e));
                }
            }
        }
        v
    }

    /// The editor preview: the whole theme as one SVG string with current values.
    pub fn preview_svg(&self, snapshot: &Snapshot) -> String {
        let values = Values {
            snapshot,
            fahrenheit: self.fahrenheit,
            smoothed: &self.smoothed,
            now: chrono::Local::now(),
        };
        svg::full_document(&self.theme, &values)
    }

    /// Render one frame.
    pub fn render(&mut self, snapshot: &Snapshot) -> Result<&RgbImage> {
        let t0 = Instant::now();
        values::smooth_into(&mut self.smoothed, snapshot, self.fahrenheit, self.theme.smoothing);
        let (w, h) = (self.theme.width, self.theme.height);
        let mut svg_ms = 0.0f32;
        let mut media_ms = 0.0f32;

        // Background.
        self.canvas.fill(resvg::tiny_skia::Color::BLACK);
        let mut pending = String::new();
        match &self.theme.background {
            Background::Media { path, fit, opacity } => {
                let t = Instant::now();
                let path = path.clone();
                let (fit, opacity) = (*fit, *opacity);
                if let Some(src) = self.media.get(&path, w, h, fit) {
                    if let Some(p) = src.current() {
                        let paint = PixmapPaint { opacity, ..Default::default() };
                        self.canvas.draw_pixmap(0, 0, p.as_ref(), &paint, Transform::identity(), None);
                    }
                }
                media_ms += t.elapsed().as_secs_f32() * 1000.0;
            }
            _ => {
                if let Some(bg) = svg::background_svg(&self.theme) {
                    pending.push_str(&bg);
                }
            }
        }

        // Elements, in order, flushing the SVG run whenever a raster is drawn.
        let now = chrono::Local::now();
        let elements = std::mem::take(&mut self.theme.elements);
        for el in &elements {
            if let Kind::Image(img) = &el.kind {
                if !el.visible || img.path.is_empty() {
                    continue;
                }
                if !pending.is_empty() {
                    let t = Instant::now();
                    self.flush_svg(&pending);
                    svg_ms += t.elapsed().as_secs_f32() * 1000.0;
                    pending.clear();
                }
                let t = Instant::now();
                let (bw, bh) = (img.w.round().max(1.0) as u32, img.h.round().max(1.0) as u32);
                let path = img.path.clone();
                if let Some(src) = self.media.get(&path, bw, bh, img.fit) {
                    if let Some(p) = src.current() {
                        let paint = PixmapPaint { opacity: el.opacity.clamp(0.0, 1.0), ..Default::default() };
                        let transform = if el.rotation.abs() > 0.001 {
                            Transform::from_rotate_at(el.rotation, el.x, el.y)
                        } else {
                            Transform::identity()
                        };
                        self.canvas.draw_pixmap(el.x.round() as i32, el.y.round() as i32, p.as_ref(), &paint, transform, None);
                    }
                }
                media_ms += t.elapsed().as_secs_f32() * 1000.0;
                continue;
            }
            let values = Values { snapshot, fahrenheit: self.fahrenheit, smoothed: &self.smoothed, now };
            if let Some(s) = svg::element_svg(el, &values) {
                pending.push_str(&s);
            }
        }
        self.theme.elements = elements;
        if !pending.is_empty() {
            let t = Instant::now();
            self.flush_svg(&pending);
            svg_ms += t.elapsed().as_secs_f32() * 1000.0;
        }

        // Premultiplied RGBA → RGB (the canvas is opaque).
        let src = self.canvas.data();
        let dst = self.out.as_mut();
        for (d, s) in dst.chunks_exact_mut(3).zip(src.chunks_exact(4)) {
            d[0] = s[0];
            d[1] = s[1];
            d[2] = s[2];
        }
        self.stats = RenderStats {
            total_ms: t0.elapsed().as_secs_f32() * 1000.0,
            svg_ms,
            media_ms,
        };
        Ok(&self.out)
    }

    fn flush_svg(&mut self, body: &str) {
        let doc = svg::document(&self.theme, body);
        let mut opt = resvg::usvg::Options::default();
        opt.fontdb = self.fonts.clone();
        opt.font_family = "Inter".into();
        match resvg::usvg::Tree::from_str(&doc, &opt) {
            Ok(tree) => resvg::render(&tree, Transform::identity(), &mut self.canvas.as_mut()),
            Err(e) => {
                warn!(error = %e, "svg parse failed");
                debug!(svg = doc, "offending svg");
            }
        }
    }
}
