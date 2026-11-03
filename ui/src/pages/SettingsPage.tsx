import { openPath } from "@tauri-apps/plugin-opener";
import { Cpu, ExternalLink, FolderOpen, ShieldCheck, Usb } from "lucide-react";
import { useEffect, useState } from "react";

import { Field, NumberInput, Select, SwitchRow, useToast } from "../components/ui";
import { api, errText, type AppInfo, type DetectedScreen, type EngineStatus, type HwAccel, type PixelFormat, type UdevStatus } from "../lib/api";
import { useConfig } from "../lib/hooks";

const PIXEL_FORMATS: PixelFormat[] = ["rgb565_be", "rgb565_le", "bgr565_be", "bgr565_le", "grb565_le", "rgb888", "bgr888"];

export function SettingsPage({ status }: { status: EngineStatus }) {
  const toast = useToast();
  const [config, update] = useConfig();
  const [autostart, setAutostart] = useState(false);
  const [udev, setUdev] = useState<UdevStatus | null>(null);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [screens, setScreens] = useState<DetectedScreen[]>([]);

  const refresh = () => {
    api.autostartGet().then(setAutostart).catch(() => {});
    api.udevStatus().then(setUdev).catch(() => {});
    api.appInfo().then(setInfo).catch(() => {});
    api.detectScreens().then(setScreens).catch(() => {});
  };
  useEffect(refresh, [status.connected]);

  const save = (patch: Parameters<typeof update>[0]) => update(patch).then(() => toast.success("Saved")).catch((e) => toast.error(errText(e)));

  if (!config) return <div className="page muted">Loading…</div>;

  return (
    <div className="page stack" style={{ maxWidth: 900 }}>
      <div className="page-head">
        <div>
          <div className="eyebrow">// preferences</div>
          <h1 className="page-title">Settings</h1>
        </div>
      </div>

      <div className="card">
        <div className="card-title" style={{ marginBottom: 6 }}>
          Startup &amp; window
        </div>
        <SwitchRow
          title="Start on login"
          desc="Adds an autostart entry that launches Neru hidden in the tray, so the screen comes alive with your session."
          on={autostart}
          onChange={(v) =>
            api
              .autostartSet(v)
              .then(() => {
                setAutostart(v);
                toast.success(v ? "Neru will start with your session" : "Autostart disabled");
              })
              .catch((e) => toast.error(errText(e)))
          }
        />
        <SwitchRow
          title="Keep running when the window is closed"
          desc="Closing the window hides it; the screen keeps updating from the tray icon. Quit from the tray menu."
          on={config.app.minimize_on_close}
          onChange={(v) => save((c) => ({ ...c, app: { ...c.app, minimize_on_close: v } }))}
        />
      </div>

      <div className="card">
        <div className="card-title" style={{ marginBottom: 12 }}>
          Screen output
        </div>
        <div className="grid grid-3">
          <Field label="Frames per second" hint="10 is plenty for metrics; 20 for smooth GIFs.">
            <NumberInput value={config.screen.fps} min={1} max={30} onChange={(fps) => save((c) => ({ ...c, screen: { ...c.screen, fps: Math.round(fps) } }))} />
          </Field>
          <Field label="JPEG quality" hint="Thermaltake screens take JPEG frames (50–100).">
            <NumberInput value={config.screen.jpeg_quality} min={50} max={100} onChange={(q) => save((c) => ({ ...c, screen: { ...c.screen, jpeg_quality: Math.round(q) } }))} />
          </Field>
          <Field label="Use this screen" hint="Auto picks the first screen found.">
            <Select
              value={config.screen.model ?? ""}
              onChange={(v) => save((c) => ({ ...c, screen: { ...c.screen, model: v === "" ? null : v } }))}
              options={[{ value: "", label: "Auto" }, ...screens.map((s) => ({ value: s.model.id, label: `${s.model.vendor} ${s.model.name}` }))]}
            />
          </Field>
        </div>
      </div>

      <div className="card">
        <div className="card-head">
          <div className="card-title">
            <Cpu size={18} />
            Hardware acceleration
          </div>
          <span className="badge">NVIDIA · AMD · Intel</span>
        </div>
        <div style={{ maxWidth: 420, marginBottom: 6 }}>
          <Field
            label="Video decoding (backgrounds and video layers)"
            hint={
              info?.ffmpeg
                ? `Decodes with the GPU instead of the CPU. ffmpeg here supports: ${info.hwaccels.length ? info.hwaccels.join(", ") : "no accelerators"}. Running videos restart when this changes.`
                : "Install ffmpeg to play videos; acceleration then uses NVDEC (NVIDIA) or VA-API (AMD/Intel)."
            }
          >
            <Select<HwAccel>
              value={config.app.video_hwaccel}
              options={[
                { value: "off", label: "Software (CPU)" },
                { value: "auto", label: "Auto — try the GPU, fall back to software" },
                { value: "nvidia", label: "NVIDIA (CUDA / NVDEC)" },
                { value: "vaapi", label: "AMD / Intel (VA-API)" },
              ]}
              onChange={(video_hwaccel) => save((c) => ({ ...c, app: { ...c.app, video_hwaccel } }))}
            />
          </Field>
        </div>
        <SwitchRow
          title="GPU rendering for the app window"
          desc={`Lets WebKit draw this window through the GPU (DMA-BUF). Smoother, but NVIDIA on Wayland can show a blank window with it on — turn it back off if that happens. Takes effect after restarting Neru${info ? ` (currently ${info.gpu_window_env ? "on" : "off"})` : ""}.`}
          on={config.app.gpu_window}
          onChange={(v) => save((c) => ({ ...c, app: { ...c.app, gpu_window: v } }))}
        />
      </div>

      <div className="card">
        <div className="card-title" style={{ marginBottom: 12 }}>
          Metrics
        </div>
        <SwitchRow title="Fahrenheit" desc="Temperatures in °F on the screen and in the app." on={config.metrics.fahrenheit} onChange={(v) => save((c) => ({ ...c, metrics: { ...c.metrics, fahrenheit: v } }))} />
        <div style={{ marginTop: 12, maxWidth: 260 }}>
          <Field label="Sensor refresh (ms)">
            <NumberInput value={config.metrics.interval_ms} min={250} max={10000} step={250} onChange={(ms) => save((c) => ({ ...c, metrics: { ...c.metrics, interval_ms: Math.round(ms) } }))} />
          </Field>
        </div>
      </div>

      <div className="card">
        <div className="card-title" style={{ marginBottom: 12 }}>
          RGB
        </div>
        <SwitchRow title="RGB control" desc="Drive lighting with Neru's own drivers (ASUS Aura motherboards, ZOTAC RTX 50 cards, …)." on={config.rgb.enabled} onChange={(v) => save((c) => ({ ...c, rgb: { ...c.rgb, enabled: v } }))} />
        <SwitchRow title="Use OpenRGB for other devices" desc="Optional. Devices Neru has no driver for yet are reached through an installed OpenRGB." on={config.rgb.use_openrgb} onChange={(v) => save((c) => ({ ...c, rgb: { ...c.rgb, use_openrgb: v } }))} />
        <SwitchRow title="Start the OpenRGB server when needed" desc="Runs `openrgb --server` headless if nothing answers on the address below (OpenRGB must be installed)." on={config.rgb.start_server} onChange={(v) => save((c) => ({ ...c, rgb: { ...c.rgb, start_server: v } }))} />
        <div className="grid grid-2" style={{ marginTop: 12 }}>
          <Field label="Server address" hint="host:port of the OpenRGB SDK server.">
            <input className="input mono" defaultValue={config.rgb.server} onBlur={(e) => e.target.value !== config.rgb.server && save((c) => ({ ...c, rgb: { ...c.rgb, server: e.target.value.trim() || "127.0.0.1:6742" } }))} />
          </Field>
          <Field label="Effect updates per second" hint="Lower this if a device lags (USB controllers are slow).">
            <NumberInput value={config.rgb.fps} min={1} max={60} onChange={(fps) => save((c) => ({ ...c, rgb: { ...c.rgb, fps: Math.round(fps) } }))} />
          </Field>
        </div>
      </div>

      <div className="card">
        <div className="card-head">
          <div className="card-title">
            <Usb size={18} />
            Device access
          </div>
          <span className={`badge ${udev?.installed ? "green" : status.needs_udev ? "red" : ""}`}>{udev?.installed ? "rules installed" : status.needs_udev ? "needed" : "not installed"}</span>
        </div>
        <p className="small muted" style={{ marginBottom: 12 }}>
          Linux only lets root talk to USB and I²C devices unless a udev rule grants access. Neru installs one rule file covering every screen and
          RGB device it supports (<code className="small">{udev?.path}</code>) and loads the <code className="small">i2c-dev</code> module GPUs need.
          Asks for your password once; unplug and replug the screen afterwards.
        </p>
        <div className="row-wrap">
          <button
            className="btn primary"
            onClick={() =>
              api
                .installUdev()
                .then(() => {
                  toast.success("Rules installed — replug the screen");
                  refresh();
                })
                .catch((e) => toast.error(errText(e)))
            }
          >
            <ShieldCheck size={15} />
            Install rules
          </button>
          {screens.length > 0 && (
            <span className="small muted">
              Detected: {screens.map((s) => `${s.model.vendor} ${s.model.name} (${s.location}${s.accessible ? "" : ", no access"})`).join("; ")}
            </span>
          )}
        </div>
      </div>

      <details className="card">
        <summary className="card-title" style={{ cursor: "pointer" }}>
          Thermalright panel calibration
        </summary>
        <p className="small muted" style={{ margin: "10px 0 12px" }}>
          Only for ChiZhu-based Thermalright screens: they are dumb framebuffers, so orientation fixes happen in software. Leave at
          defaults unless the picture is shifted or mirrored.
        </p>
        <div className="grid grid-4">
          <Field label="Flip vertically">
            <Select value={config.screen.thermalright.flip_y ? "yes" : "no"} options={[{ value: "no", label: "No" }, { value: "yes", label: "Yes" }]} onChange={(v) => save((c) => ({ ...c, screen: { ...c.screen, thermalright: { ...c.screen.thermalright, flip_y: v === "yes" } } }))} />
          </Field>
          <Field label="Row offset">
            <NumberInput value={config.screen.thermalright.offset_y} onChange={(v) => save((c) => ({ ...c, screen: { ...c.screen, thermalright: { ...c.screen.thermalright, offset_y: Math.round(v) } } }))} />
          </Field>
          <Field label="Column offset">
            <NumberInput value={config.screen.thermalright.offset_x} onChange={(v) => save((c) => ({ ...c, screen: { ...c.screen, thermalright: { ...c.screen.thermalright, offset_x: Math.round(v) } } }))} />
          </Field>
          <Field label="Pixel format">
            <Select value={config.screen.thermalright.pixel_format} options={PIXEL_FORMATS.map((p) => ({ value: p, label: p }))} onChange={(v) => save((c) => ({ ...c, screen: { ...c.screen, thermalright: { ...c.screen.thermalright, pixel_format: v } } }))} />
          </Field>
        </div>
      </details>

      <div className="card">
        <div className="card-title" style={{ marginBottom: 12 }}>
          About
        </div>
        <dl className="kv">
          <dt>Version</dt>
          <dd>{info?.version}</dd>
          <dt>Config</dt>
          <dd className="row" style={{ gap: 8 }}>
            {info?.config_dir}
            <button className="btn ghost icon sm" aria-label="Open config folder" onClick={() => info && openPath(info.config_dir).catch((e) => toast.error(errText(e)))}>
              <FolderOpen size={14} />
            </button>
          </dd>
          <dt>Themes</dt>
          <dd>{info?.themes_dir}</dd>
          <dt>Media</dt>
          <dd>{info?.media_dir}</dd>
          <dt>Assets</dt>
          <dd>{info?.assets_dir ?? "not found — bundled fonts and themes unavailable"}</dd>
          <dt>ffmpeg</dt>
          <dd>{info?.ffmpeg ? "found (video backgrounds available)" : "not found — install it for video backgrounds"}</dd>
          <dt>Source</dt>
          <dd>
            <a href="https://github.com/Huzzama/Neru-screen-control" target="_blank" rel="noreferrer" className="row" style={{ gap: 4, display: "inline-flex" }}>
              github.com/Huzzama/Neru-screen-control <ExternalLink size={12} />
            </a>
          </dd>
        </dl>
      </div>
    </div>
  );
}
