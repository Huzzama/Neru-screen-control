//! Tauri commands called from the UI (`invoke("name", { … })`).

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use serde::Serialize;
use tauri::{AppHandle, State};

use neru_core::config::AppConfig;
use neru_core::engine::{Command, EngineStatus};
use neru_core::metrics::Snapshot;
use neru_core::render::Theme;
use neru_core::screen::{self, DetectedScreen};
use neru_core::themes::ThemeInfo;

use crate::state::AppState;

type Res<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[tauri::command]
pub fn get_status(state: State<AppState>) -> EngineStatus {
    state.status()
}

#[tauri::command]
pub fn get_metrics(state: State<AppState>) -> Snapshot {
    state.engine.lock().map(|e| e.metrics()).unwrap_or_default()
}

#[tauri::command]
pub fn get_config(state: State<AppState>) -> AppConfig {
    state.config.lock().map(|c| c.clone()).unwrap_or_default()
}

#[tauri::command]
pub fn set_config(state: State<AppState>, config: AppConfig) -> Res<()> {
    state.apply_config(config)
}

#[tauri::command]
pub fn list_themes(state: State<AppState>) -> Vec<ThemeInfo> {
    state.themes.list()
}

#[tauri::command]
pub fn load_theme(state: State<AppState>, id: String) -> Res<Theme> {
    state.themes.load(&id).map_err(err)
}

#[tauri::command]
pub fn save_theme(state: State<AppState>, id: String, theme: Theme) -> Res<()> {
    state.themes.save(&id, &theme).map_err(err)?;
    // If it is the active theme, the screen follows the saved version.
    let active = state.config.lock().map(|c| c.screen.active_theme.clone()).unwrap_or(None);
    if active.as_deref() == Some(id.as_str()) {
        state.send(Command::SetTheme(theme));
    }
    Ok(())
}

#[tauri::command]
pub fn create_theme(state: State<AppState>, theme: Theme) -> Res<String> {
    state.themes.create(&theme).map_err(err)
}

#[tauri::command]
pub fn duplicate_theme(state: State<AppState>, id: String, name: Option<String>) -> Res<String> {
    state.themes.duplicate(&id, name.as_deref()).map_err(err)
}

#[tauri::command]
pub fn delete_theme(state: State<AppState>, id: String) -> Res<()> {
    state.themes.delete(&id).map_err(err)?;
    let mut cfg = state.config.lock().map_err(err)?.clone();
    if cfg.screen.active_theme.as_deref() == Some(id.as_str()) {
        cfg.screen.active_theme = None;
        state.apply_config(cfg)?;
    }
    Ok(())
}

/// Make `id` the theme shown on the screen (persisted).
#[tauri::command]
pub fn apply_theme(state: State<AppState>, id: String) -> Res<()> {
    let theme = state.themes.load(&id).map_err(err)?;
    state.send(Command::SetTheme(theme));
    let mut cfg = state.config.lock().map_err(err)?.clone();
    cfg.screen.active_theme = Some(id);
    state.apply_config(cfg)
}

/// Show an unsaved theme on the screen and in the preview (live editing).
#[tauri::command]
pub fn preview_theme(state: State<AppState>, theme: Theme) {
    state.send(Command::UpdateTheme(theme));
}

#[tauri::command]
pub fn set_preview(state: State<AppState>, enabled: bool) {
    state.send(Command::Preview(enabled));
}

#[tauri::command]
pub fn set_pause(state: State<AppState>, paused: bool) {
    state.set_pause(paused);
}

#[tauri::command]
pub fn reconnect(state: State<AppState>) {
    state.send(Command::Reconnect);
}

#[tauri::command]
pub fn detect_screens() -> Vec<DetectedScreen> {
    screen::detect()
}

#[tauri::command]
pub fn list_fonts(state: State<AppState>) -> Vec<String> {
    state.fonts.families()
}

#[derive(Serialize)]
pub struct MediaInfo {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub kind: &'static str,
}

fn media_kind(p: &Path) -> &'static str {
    match p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).as_deref() {
        Some("gif") => "gif",
        Some("mp4" | "mkv" | "webm" | "mov" | "avi" | "m4v") => "video",
        Some("png" | "jpg" | "jpeg" | "webp" | "bmp") => "image",
        _ => "other",
    }
}

#[tauri::command]
pub fn list_media(state: State<AppState>) -> Vec<MediaInfo> {
    let mut v: Vec<MediaInfo> = std::fs::read_dir(&state.paths.media_dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let p = e.path();
                    let kind = media_kind(&p);
                    if kind == "other" || !p.is_file() {
                        return None;
                    }
                    Some(MediaInfo {
                        name: p.file_name()?.to_string_lossy().to_string(),
                        path: p.display().to_string(),
                        size: e.metadata().map(|m| m.len()).unwrap_or(0),
                        kind,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    v
}

/// Copy a file into the media directory; returns the name to use in themes.
#[tauri::command]
pub fn import_media(state: State<AppState>, path: String) -> Res<String> {
    let src = PathBuf::from(&path);
    if media_kind(&src) == "other" {
        return Err("unsupported file type (use PNG, JPEG, WebP, BMP, GIF or a video)".into());
    }
    let name = src
        .file_name()
        .ok_or("bad file name")?
        .to_string_lossy()
        .to_string();
    std::fs::create_dir_all(&state.paths.media_dir).map_err(err)?;
    let mut dest = state.paths.media_dir.join(&name);
    let mut final_name = name.clone();
    if dest.exists() {
        let stem = src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let ext = src.extension().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let mut n = 2;
        loop {
            final_name = format!("{stem}-{n}.{ext}");
            dest = state.paths.media_dir.join(&final_name);
            if !dest.exists() {
                break;
            }
            n += 1;
        }
    }
    std::fs::copy(&src, &dest).map_err(err)?;
    Ok(final_name)
}

#[tauri::command]
pub fn delete_media(state: State<AppState>, name: String) -> Res<()> {
    if name.contains('/') || name.contains("..") {
        return Err("bad name".into());
    }
    std::fs::remove_file(state.paths.media_dir.join(name)).map_err(err)
}

#[derive(Serialize)]
pub struct UdevStatus {
    pub installed: bool,
    pub path: String,
    /// A screen is present but cannot be opened.
    pub needed: bool,
}

const UDEV_FILE: &str = "/etc/udev/rules.d/70-neru-screen-control.rules";

#[tauri::command]
pub fn udev_status(state: State<AppState>) -> UdevStatus {
    let installed = Path::new(UDEV_FILE).exists() || Path::new("/usr/lib/udev/rules.d/70-neru-screen-control.rules").exists();
    UdevStatus { installed, path: UDEV_FILE.into(), needed: state.status().needs_udev }
}

#[tauri::command]
pub fn install_udev() -> Res<()> {
    let rules = screen::udev_rules();
    let script = format!(
        "set -e; cat > {UDEV_FILE} <<'EOF'\n{rules}EOF\nudevadm control --reload-rules && udevadm trigger"
    );
    let status = std::process::Command::new("pkexec")
        .args(["sh", "-c", &script])
        .status()
        .map_err(|e| format!("could not run pkexec (is polkit installed?): {e}"))?;
    if !status.success() {
        return Err(format!("installation failed ({status})"));
    }
    Ok(())
}

#[tauri::command]
pub fn autostart_get() -> bool {
    neru_core::autostart::is_enabled()
}

#[tauri::command]
pub fn autostart_set(state: State<AppState>, enabled: bool) -> Res<()> {
    let exe = std::env::var("APPIMAGE")
        .ok()
        .or_else(|| std::env::current_exe().ok().map(|p| p.display().to_string()))
        .ok_or("cannot determine the executable path")?;
    neru_core::autostart::set_enabled(enabled, &exe).map_err(err)?;
    let mut cfg = state.config.lock().map_err(err)?.clone();
    cfg.app.start_on_login = enabled;
    state.apply_config(cfg)
}

#[derive(Serialize)]
pub struct AppInfo {
    pub version: String,
    pub config_dir: String,
    pub themes_dir: String,
    pub media_dir: String,
    pub assets_dir: Option<String>,
    pub exe: String,
    pub background_mode: bool,
    pub ffmpeg: bool,
}

#[tauri::command]
pub fn app_info(state: State<AppState>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        config_dir: state.paths.config_dir.display().to_string(),
        themes_dir: state.paths.themes_dir.display().to_string(),
        media_dir: state.paths.media_dir.display().to_string(),
        assets_dir: neru_core::render::find_assets_dir().map(|p| p.display().to_string()),
        exe: std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default(),
        background_mode: state.background_mode,
        ffmpeg: which("ffmpeg"),
    }
}

fn which(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
        .unwrap_or(false)
}

#[derive(Serialize)]
pub struct MetricKey {
    pub key: &'static str,
    pub label: &'static str,
    pub unit: &'static str,
}

#[tauri::command]
pub fn metric_keys() -> Vec<MetricKey> {
    Snapshot::KEYS
        .iter()
        .map(|(key, label, unit)| MetricKey { key, label, unit })
        .collect()
}

/// One frame of a theme as a small JPEG data URL (for the gallery).
#[tauri::command]
pub fn theme_thumbnail(state: State<AppState>, id: String, max: u32) -> Res<String> {
    use base64::Engine as _;
    let theme = state.themes.load(&id).map_err(err)?;
    let snapshot = state.engine.lock().map(|e| e.metrics()).unwrap_or_default();
    let mut r = neru_core::render::Renderer::new(theme, state.fonts.db.clone(), Some(state.paths.media_dir.clone())).map_err(err)?;
    r.fahrenheit = state.config.lock().map(|c| c.metrics.fahrenheit).unwrap_or(false);
    let frame = r.render(&snapshot).map_err(err)?;
    let max = max.clamp(32, 1024);
    let scale = (max as f32 / frame.width().max(frame.height()) as f32).min(1.0);
    let (w, h) = (((frame.width() as f32 * scale) as u32).max(1), ((frame.height() as f32 * scale) as u32).max(1));
    let small = image::imageops::resize(frame, w, h, image::imageops::FilterType::Triangle);
    let mut jpeg = Vec::new();
    jpeg_encoder::Encoder::new(&mut jpeg, 80)
        .encode(small.as_raw(), w as u16, h as u16, jpeg_encoder::ColorType::Rgb)
        .map_err(err)?;
    Ok(format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(&jpeg)))
}

#[tauri::command]
pub fn quit_app(app: AppHandle, state: State<AppState>) {
    state.quitting.store(true, Ordering::Relaxed);
    state.shutdown();
    app.exit(0);
}
