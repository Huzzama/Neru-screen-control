// Typed wrappers over the Tauri commands. Types mirror the Rust structs
// (serde output), so keep them in sync with crates/neru-core.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// ── Status / metrics ────────────────────────────────────────────────────────
export interface ScreenSummary {
  model_id: string;
  vendor: string;
  name: string;
  width: number;
  height: number;
  location: string;
  firmware: string | null;
  serial: string | null;
  brightness: boolean;
  hw_rotation: boolean;
}

export interface EngineStatus {
  connected: boolean;
  screen: ScreenSummary | null;
  detected: string[];
  needs_udev: boolean;
  last_error: string | null;
  paused: boolean;
  frames: number;
  fps_actual: number;
  render_ms: number;
  send_ms: number;
  theme_name: string;
  theme_width: number;
  theme_height: number;
  media_errors: [string, string][];
}

export interface CpuMetrics {
  name: string;
  temp_c: number | null;
  usage_pct: number | null;
  freq_mhz: number | null;
  power_w: number | null;
}

export interface GpuMetrics {
  vendor: "amd" | "nvidia" | "intel" | null;
  name: string;
  temp_c: number | null;
  usage_pct: number | null;
  core_mhz: number | null;
  mem_mhz: number | null;
  power_w: number | null;
  vram_used_mb: number | null;
  vram_total_mb: number | null;
}

export interface Snapshot {
  uptime_s: number;
  cpu: CpuMetrics;
  gpu: GpuMetrics | null;
  mem: { used_mb: number; total_mb: number; pct: number };
}

// ── Config ──────────────────────────────────────────────────────────────────
export type PixelFormat = "rgb565_be" | "rgb565_le" | "bgr565_be" | "bgr565_le" | "grb565_le" | "rgb888" | "bgr888";

export interface AppConfig {
  screen: {
    model: string | null;
    brightness: number;
    rotation: number;
    fps: number;
    jpeg_quality: number;
    active_theme: string | null;
    thermalright: { rotation: number; flip_y: boolean; offset_y: number; offset_x: number; pixel_format: PixelFormat };
  };
  metrics: { interval_ms: number; fahrenheit: boolean };
  app: { start_on_login: boolean; launch_hidden: boolean; tray_icon: boolean; minimize_on_close: boolean };
}

// ── Themes ──────────────────────────────────────────────────────────────────
export type Fit = "cover" | "contain" | "stretch";
export type Align = "left" | "center" | "right";
export interface Stop { at: number; color: string }

export type Background =
  | { kind: "color"; color: string }
  | { kind: "gradient"; from: string; to: string; angle: number }
  | { kind: "media"; path: string; fit: Fit; opacity: number };

interface ElementBase {
  id: string;
  x: number;
  y: number;
  rotation: number;
  opacity: number;
  visible: boolean;
  locked: boolean;
}

export interface TextEl extends ElementBase {
  type: "text";
  text: string;
  font: string;
  size: number;
  weight: number;
  color: string;
  align: Align;
  letter_spacing: number;
  ramp_metric: string | null;
  ramp: Stop[];
}
export interface BarEl extends ElementBase {
  type: "bar";
  w: number; h: number; metric: string; min: number; max: number;
  color: string; track: string; radius: number; vertical: boolean; ramp: Stop[];
}
export interface GaugeEl extends ElementBase {
  type: "gauge";
  r: number; thickness: number; metric: string; min: number; max: number;
  start: number; sweep: number; color: string; track: string; round_caps: boolean; ramp: Stop[];
}
export interface RectEl extends ElementBase {
  type: "rect";
  w: number; h: number; fill: string; stroke: string; stroke_width: number; radius: number;
}
export interface EllipseEl extends ElementBase {
  type: "ellipse";
  rx: number; ry: number; fill: string; stroke: string; stroke_width: number;
}
export interface LineEl extends ElementBase {
  type: "line";
  w: number; h: number; stroke: string; stroke_width: number; round_caps: boolean;
}
export interface ImageEl extends ElementBase {
  type: "image";
  w: number; h: number; path: string; fit: Fit;
}
export type Element = TextEl | BarEl | GaugeEl | RectEl | EllipseEl | LineEl | ImageEl;
export type ElementType = Element["type"];

export interface Theme {
  name: string;
  version: number;
  width: number;
  height: number;
  background: Background;
  elements: Element[];
  smoothing: number;
}

export interface ThemeInfo {
  id: string;
  name: string;
  width: number;
  height: number;
  bundled: boolean;
  path: string;
  elements: number;
}

export interface DetectedScreen {
  model: { id: string; vendor: string; name: string; width: number; height: number; diagonal_in: number; verified: boolean };
  location: string;
  accessible: boolean;
}

export interface MediaInfo { name: string; path: string; size: number; kind: "image" | "gif" | "video" | "other" }
export interface UdevStatus { installed: boolean; path: string; needed: boolean }
export interface AppInfo {
  version: string; config_dir: string; themes_dir: string; media_dir: string; assets_dir: string | null;
  exe: string; background_mode: boolean; ffmpeg: boolean;
}
export interface MetricKey { key: string; label: string; unit: string }
export interface PreviewFrame { data: string; width: number; height: number; seq: number }

// ── Calls ───────────────────────────────────────────────────────────────────
export const api = {
  status: () => invoke<EngineStatus>("get_status"),
  metrics: () => invoke<Snapshot>("get_metrics"),
  config: () => invoke<AppConfig>("get_config"),
  setConfig: (config: AppConfig) => invoke<void>("set_config", { config }),
  themes: () => invoke<ThemeInfo[]>("list_themes"),
  loadTheme: (id: string) => invoke<Theme>("load_theme", { id }),
  saveTheme: (id: string, theme: Theme) => invoke<void>("save_theme", { id, theme }),
  createTheme: (theme: Theme) => invoke<string>("create_theme", { theme }),
  duplicateTheme: (id: string, name?: string) => invoke<string>("duplicate_theme", { id, name: name ?? null }),
  deleteTheme: (id: string) => invoke<void>("delete_theme", { id }),
  applyTheme: (id: string) => invoke<void>("apply_theme", { id }),
  previewTheme: (theme: Theme) => invoke<void>("preview_theme", { theme }),
  setPreview: (enabled: boolean) => invoke<void>("set_preview", { enabled }),
  setPause: (paused: boolean) => invoke<void>("set_pause", { paused }),
  reconnect: () => invoke<void>("reconnect"),
  detectScreens: () => invoke<DetectedScreen[]>("detect_screens"),
  fonts: () => invoke<string[]>("list_fonts"),
  media: () => invoke<MediaInfo[]>("list_media"),
  importMedia: (path: string) => invoke<string>("import_media", { path }),
  deleteMedia: (name: string) => invoke<void>("delete_media", { name }),
  udevStatus: () => invoke<UdevStatus>("udev_status"),
  installUdev: () => invoke<void>("install_udev"),
  autostartGet: () => invoke<boolean>("autostart_get"),
  autostartSet: (enabled: boolean) => invoke<void>("autostart_set", { enabled }),
  appInfo: () => invoke<AppInfo>("app_info"),
  metricKeys: () => invoke<MetricKey[]>("metric_keys"),
  thumbnail: (id: string, max = 320) => invoke<string>("theme_thumbnail", { id, max }),
  quit: () => invoke<void>("quit_app"),
  onPreview: (cb: (f: PreviewFrame) => void): Promise<UnlistenFn> => listen<PreviewFrame>("preview-frame", (e) => cb(e.payload)),
  onStatus: (cb: (s: EngineStatus) => void): Promise<UnlistenFn> => listen<EngineStatus>("status", (e) => cb(e.payload)),
};

export function errText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message);
  return String(e);
}
