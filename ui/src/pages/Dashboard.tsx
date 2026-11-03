import { Cpu, Gauge, MemoryStick, MonitorSmartphone, Pause, Play, RefreshCw, ShieldAlert, Thermometer } from "lucide-react";
import { useEffect, useState } from "react";

import type { Page } from "../App";
import { Segmented, fmt, useToast } from "../components/ui";
import { api, errText, type EngineStatus, type ThemeInfo } from "../lib/api";
import { useConfig, useMetrics, usePreview } from "../lib/hooks";

function Tile({ label, value, unit, sub, pct, violet }: { label: string; value: string; unit?: string; sub?: string; pct?: number | null; violet?: boolean }) {
  return (
    <div className="tile">
      <span className="tile-label">{label}</span>
      <span className="tile-value">
        {value}
        {unit && <small>{unit}</small>}
      </span>
      {sub && <span className="tile-sub">{sub}</span>}
      {pct != null && (
        <div className={`meter ${violet ? "violet" : ""}`}>
          <span style={{ width: `${Math.max(0, Math.min(100, pct))}%` }} />
        </div>
      )}
    </div>
  );
}

export function Dashboard({ status, goTo }: { status: EngineStatus; goTo: (p: Page) => void }) {
  const toast = useToast();
  const metrics = useMetrics(1000);
  const preview = usePreview(true);
  const [config, updateConfig] = useConfig();
  const [themes, setThemes] = useState<ThemeInfo[]>([]);
  const [brightness, setBrightness] = useState<number | null>(null);

  useEffect(() => {
    api.themes().then(setThemes).catch(() => {});
  }, [status.theme_name]);

  useEffect(() => {
    if (config && brightness === null) setBrightness(config.screen.brightness);
  }, [config, brightness]);

  const cpu = metrics?.cpu;
  const gpu = metrics?.gpu;
  const mem = metrics?.mem;
  const net = metrics?.net;
  const disk = metrics?.disk;
  // Spinning fans first; headers without a tach signal (0 rpm) go last.
  const fans = [...(metrics?.fans ?? [])].sort((a, b) => Number((b.rpm ?? 0) > 0) - Number((a.rpm ?? 0) > 0));
  const speed = (bps: number | null | undefined) => {
    if (bps == null) return "--";
    const units = ["B/s", "KB/s", "MB/s", "GB/s"];
    let v = Math.max(0, bps);
    let i = 0;
    while (v >= 1024 && i < units.length - 1) {
      v /= 1024;
      i += 1;
    }
    return `${v.toFixed(i === 0 ? 0 : v < 10 ? 2 : v < 100 ? 1 : 0)} ${units[i]}`;
  };
  const f = config?.metrics.fahrenheit ?? false;
  const temp = (c: number | null | undefined) => (c == null ? "--" : f ? ((c * 9) / 5 + 32).toFixed(0) : c.toFixed(0));
  const tUnit = f ? "°F" : "°C";

  const screen = status.screen;

  return (
    <div className="page stack">
      <div className="page-head">
        <div>
          <div className="eyebrow">// overview</div>
          <h1 className="page-title">Dashboard</h1>
        </div>
      </div>

      {status.needs_udev && (
        <div className="banner warn">
          <ShieldAlert size={18} />
          <span>A screen was found but this user cannot open it. Install the device access rules (asks for your password), then unplug and replug the screen.</span>
          <button
            className="btn primary sm"
            onClick={() =>
              api
                .installUdev()
                .then(() => toast.success("Rules installed — replug the screen's USB cable"))
                .catch((e) => toast.error(errText(e)))
            }
          >
            Install rules
          </button>
        </div>
      )}
      {!status.connected && !status.needs_udev && status.last_error && (
        <div className="banner error">
          <ShieldAlert size={18} />
          <span className="mono small">{status.last_error}</span>
        </div>
      )}

      <div className="grid" style={{ gridTemplateColumns: "minmax(0, 1.4fr) minmax(300px, 1fr)" }}>
        <div className="card">
          <div className="card-head">
            <div className="card-title">
              <MonitorSmartphone size={18} />
              Screen
            </div>
            <span className={`badge ${status.connected ? "green" : status.needs_udev ? "accent" : ""}`}>
              {status.connected ? (status.paused ? "paused" : "live") : status.needs_udev ? "no access" : "not connected"}
            </span>
          </div>
          {screen ? (
            <dl className="kv">
              <dt>Model</dt>
              <dd>
                {screen.vendor} {screen.name}
              </dd>
              <dt>Resolution</dt>
              <dd>
                {screen.width} × {screen.height}
              </dd>
              <dt>Device</dt>
              <dd>{screen.location}</dd>
              {screen.firmware && (
                <>
                  <dt>Firmware</dt>
                  <dd>{screen.firmware}</dd>
                </>
              )}
              <dt>Output</dt>
              <dd>
                {status.fps_actual.toFixed(1)} fps · render {status.render_ms.toFixed(1)} ms · send {status.send_ms.toFixed(1)} ms · {status.frames} frames
              </dd>
            </dl>
          ) : (
            <p className="muted small">
              {status.detected.length ? `Seen on the bus: ${status.detected.join(", ")}` : "Plug in a supported Thermaltake or Thermalright screen. The app reconnects automatically."}
            </p>
          )}
          {(status.recognised_only ?? []).length > 0 && (
            <div className="banner info small" style={{ marginTop: 10 }}>
              {status.recognised_only.map((r) => (
                <div key={r}>{r}</div>
              ))}
            </div>
          )}

          <div className="row-wrap" style={{ marginTop: 16 }}>
            <button className="btn" disabled={!status.connected} onClick={() => api.setPause(!status.paused).catch((e) => toast.error(errText(e)))}>
              {status.paused ? <Play size={15} /> : <Pause size={15} />}
              {status.paused ? "Resume" : "Pause"}
            </button>
            <button className="btn ghost" onClick={() => api.reconnect().then(() => toast.info("Reconnecting…")).catch((e) => toast.error(errText(e)))}>
              <RefreshCw size={15} />
              Reconnect
            </button>
          </div>

          <div className="grid grid-2" style={{ marginTop: 18 }}>
            <div className="field">
              <label>Brightness {screen && !screen.brightness ? "(not supported by this screen)" : ""}</label>
              <div className="row">
                <input
                  className="range"
                  type="range"
                  min={0}
                  max={100}
                  disabled={!screen?.brightness}
                  value={brightness ?? 100}
                  onChange={(e) => setBrightness(Number(e.target.value))}
                  onMouseUp={() => brightness != null && updateConfig((c) => ({ ...c, screen: { ...c.screen, brightness } })).catch((e) => toast.error(errText(e)))}
                  onKeyUp={() => brightness != null && updateConfig((c) => ({ ...c, screen: { ...c.screen, brightness } })).catch((e) => toast.error(errText(e)))}
                  aria-label="Brightness"
                />
                <span className="mono small" style={{ width: 36, textAlign: "right" }}>
                  {brightness ?? "--"}
                </span>
              </div>
            </div>
            <div className="field">
              <label>Rotation</label>
              <Segmented
                ariaLabel="Rotation"
                value={config?.screen.rotation ?? 0}
                options={[0, 90, 180, 270].map((v) => ({ value: v, label: `${v}°` }))}
                onChange={(rotation) => updateConfig((c) => ({ ...c, screen: { ...c.screen, rotation } })).catch((e) => toast.error(errText(e)))}
              />
            </div>
          </div>

          <div className="field" style={{ marginTop: 14 }}>
            <label>Theme on screen</label>
            <div className="row">
              <select
                className="select"
                value={config?.screen.active_theme ?? ""}
                onChange={(e) => api.applyTheme(e.target.value).then(() => toast.success("Theme applied")).catch((err) => toast.error(errText(err)))}
              >
                {!config?.screen.active_theme && <option value="">— default —</option>}
                {themes.map((t) => (
                  <option key={t.id} value={t.id}>
                    {t.name} ({t.width}×{t.height}){t.bundled ? " · bundled" : ""}
                  </option>
                ))}
              </select>
              <button className="btn" onClick={() => goTo("screen")}>
                Edit themes
              </button>
            </div>
          </div>
        </div>

        <div className="card">
          <div className="card-head">
            <div className="card-title">Live preview</div>
            <span className="dim small mono">
              {status.theme_width}×{status.theme_height}
            </span>
          </div>
          <div className="preview-frame" style={{ aspectRatio: `${status.theme_width} / ${status.theme_height}` }}>
            {preview ? <img src={preview.data} alt="What the screen shows right now" /> : <span className="preview-empty">Rendering…</span>}
          </div>
          {status.media_errors.length > 0 && (
            <div className="banner error small" style={{ marginTop: 10 }}>
              Media missing: {status.media_errors.map(([p]) => p).join(", ")}
            </div>
          )}
        </div>
      </div>

      <div>
        <div className="eyebrow" style={{ marginBottom: 10 }}>
          // live metrics
        </div>
        <div className="grid grid-4">
          <Tile label="CPU temp" value={temp(cpu?.temp_c)} unit={tUnit} sub={cpu?.name} pct={cpu?.temp_c ?? null} />
          <Tile label="CPU load" value={fmt(cpu?.usage_pct)} unit="%" sub={`${fmt(cpu?.freq_mhz)} MHz · ${fmt(cpu?.power_w)} W`} pct={cpu?.usage_pct ?? null} />
          <Tile label="GPU temp" value={temp(gpu?.temp_c)} unit={tUnit} sub={gpu?.name ?? "no GPU detected"} pct={gpu?.temp_c ?? null} violet />
          <Tile label="GPU load" value={fmt(gpu?.usage_pct)} unit="%" sub={`${fmt(gpu?.core_mhz)} MHz · ${fmt(gpu?.power_w)} W`} pct={gpu?.usage_pct ?? null} violet />
          <Tile label="RAM" value={fmt(mem?.pct)} unit="%" sub={`${fmt(mem?.used_mb)} / ${fmt(mem?.total_mb)} MB`} pct={mem?.pct ?? null} />
          <Tile
            label="VRAM"
            value={gpu?.vram_used_mb != null && gpu.vram_total_mb ? fmt((gpu.vram_used_mb / gpu.vram_total_mb) * 100) : "--"}
            unit="%"
            sub={`${fmt(gpu?.vram_used_mb)} / ${fmt(gpu?.vram_total_mb)} MB`}
            pct={gpu?.vram_used_mb != null && gpu.vram_total_mb ? (gpu.vram_used_mb / gpu.vram_total_mb) * 100 : null}
            violet
          />
          <Tile label="Network" value={speed(net?.down_bps)} sub={`↑ ${speed(net?.up_bps)}${net?.iface ? ` · ${net.iface}` : ""}`} pct={null} />
          <Tile label="Disk" value={fmt(disk?.pct)} unit="%" sub={`R ${speed(disk?.read_bps)} · W ${speed(disk?.write_bps)}${disk?.temp_c != null ? ` · ${temp(disk.temp_c)}${tUnit}` : ""}`} pct={disk?.pct ?? null} />
          <div className="tile">
            <span className="tile-label">Fans</span>
            {fans.length === 0 ? (
              <div className="small muted">{gpu?.fan_pct != null ? `GPU fan ${fmt(gpu.fan_pct)}%` : "no fan sensors (hwmon)"}</div>
            ) : (
              fans.slice(0, 4).map((fan) => (
                <div key={fan.id} className="row small" style={{ gap: 6, justifyContent: "space-between" }}>
                  <span className="muted" style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{fan.label}</span>
                  <span className="mono" title={fan.rpm === 0 ? "no tach signal (empty header or a hub that does not report speed)" : undefined}>{fan.rpm == null ? "--" : fan.rpm === 0 ? "no tach" : `${fan.rpm.toFixed(0)} rpm`}</span>
                </div>
              ))
            )}
            {fans.length > 4 && <div className="small muted">+{fans.length - 4} more in the editor</div>}
          </div>
          <div className="tile">
            <span className="tile-label">Sensors</span>
            <div className="row small muted" style={{ gap: 6 }}>
              <Thermometer size={14} /> {cpu?.temp_c != null ? "CPU temp ok" : "CPU temp missing"}
            </div>
            <div className="row small muted" style={{ gap: 6 }}>
              <Cpu size={14} /> {cpu?.power_w != null ? "CPU power ok" : "CPU power missing"}
            </div>
            <div className="row small muted" style={{ gap: 6 }}>
              <Gauge size={14} /> {gpu ? `GPU via ${gpu.vendor ?? "?"}` : "no GPU backend"}
            </div>
            <div className="row small muted" style={{ gap: 6 }}>
              <MemoryStick size={14} /> RAM ok
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
