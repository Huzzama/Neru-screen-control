import { Cpu, HardDrive, Keyboard, Lightbulb, MemoryStick, Monitor, Mouse, Play, RefreshCw, Save, ScanSearch, Trash2, Zap } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { ColorInput, Field, NumberInput, Select, useToast } from "../components/ui";
import { api, errText, type Effect, type RgbController, type RgbMode, type RgbStatus } from "../lib/api";
import { useConfig } from "../lib/hooks";

const KIND_ICON: Record<string, typeof Cpu> = {
  motherboard: Cpu,
  dram: MemoryStick,
  gpu: Monitor,
  cooler: Zap,
  keyboard: Keyboard,
  mouse: Mouse,
  storage: HardDrive,
};

type SoftKind = "off" | "static" | "breathing" | "rainbow" | "color_cycle" | "temperature";
const SOFT_KINDS: { value: SoftKind | "hardware"; label: string }[] = [
  { value: "static", label: "Static colour" },
  { value: "breathing", label: "Breathing" },
  { value: "rainbow", label: "Rainbow" },
  { value: "color_cycle", label: "Colour cycle" },
  { value: "temperature", label: "Temperature (reactive)" },
  { value: "off", label: "Off" },
];

function defaultEffect(kind: string, modes?: RgbMode[]): Effect {
  switch (kind) {
    case "off":
      return { kind: "off" };
    case "breathing":
      return { kind: "breathing", color: "#ffc20e", period_s: 3 };
    case "rainbow":
      return { kind: "rainbow", speed: 6, spread: 180 };
    case "color_cycle":
      return { kind: "color_cycle", colors: ["#ffc20e", "#9b30ff", "#3dd6ff"], period_s: 6 };
    case "temperature":
      return { kind: "temperature", metric: "cpu.temp", ramp: [{ at: 35, color: "#3dd6ff" }, { at: 65, color: "#ffc20e" }, { at: 85, color: "#ff3b5c" }] };
    case "hardware":
      return { kind: "hardware", mode: modes?.[0]?.name ?? "Static", color: "#ffc20e", speed: null, brightness: null };
    default:
      return { kind: "static", color: "#ffc20e" };
  }
}

/** Editor for one effect. `modes` enables the hardware option (device level). */
function EffectEditor({ value, onChange, modes }: { value: Effect; onChange: (e: Effect) => void; modes?: RgbMode[] }) {
  const kinds = modes && modes.length ? [{ value: "hardware" as const, label: "Hardware mode (runs on the device)" }, ...SOFT_KINDS] : SOFT_KINDS;
  const mode = value.kind === "hardware" ? modes?.find((m) => m.name.toLowerCase() === value.mode.toLowerCase()) : undefined;
  return (
    <div className="col" style={{ gap: 8 }}>
      <Select value={value.kind} options={kinds} onChange={(k) => onChange(defaultEffect(k, modes))} />
      {value.kind === "static" && <ColorInput value={value.color} onChange={(color) => onChange({ ...value, color })} />}
      {value.kind === "breathing" && (
        <div className="fields-2">
          <ColorInput value={value.color} onChange={(color) => onChange({ ...value, color })} />
          <Field label="Period (s)">
            <NumberInput value={value.period_s} min={0.5} step={0.5} onChange={(period_s) => onChange({ ...value, period_s })} />
          </Field>
        </div>
      )}
      {value.kind === "rainbow" && (
        <div className="fields-2">
          <Field label="Speed (cycles/min)">
            <NumberInput value={value.speed} min={0} step={1} onChange={(speed) => onChange({ ...value, speed })} />
          </Field>
          <Field label="Spread (°)">
            <NumberInput value={value.spread} min={0} max={720} step={10} onChange={(spread) => onChange({ ...value, spread })} />
          </Field>
        </div>
      )}
      {value.kind === "color_cycle" && (
        <>
          {value.colors.map((c, i) => (
            <div key={i} className="row">
              <ColorInput value={c} onChange={(nc) => onChange({ ...value, colors: value.colors.map((x, j) => (j === i ? nc : x)) })} />
              <button className="btn ghost icon sm" aria-label="Remove colour" onClick={() => onChange({ ...value, colors: value.colors.filter((_, j) => j !== i) })}>
                <Trash2 size={13} />
              </button>
            </div>
          ))}
          <div className="fields-2">
            <button className="btn sm" onClick={() => onChange({ ...value, colors: [...value.colors, "#ffffff"] })}>
              Add colour
            </button>
            <Field label="Period (s)">
              <NumberInput value={value.period_s} min={0.5} step={0.5} onChange={(period_s) => onChange({ ...value, period_s })} />
            </Field>
          </div>
        </>
      )}
      {value.kind === "temperature" && (
        <>
          <Select
            value={value.metric}
            options={[
              { value: "cpu.temp", label: "CPU temperature" },
              { value: "gpu.temp", label: "GPU temperature" },
              { value: "cpu.usage", label: "CPU load (%)" },
              { value: "gpu.usage", label: "GPU load (%)" },
            ]}
            onChange={(metric) => onChange({ ...value, metric })}
          />
          {value.ramp.map((s, i) => (
            <div key={i} className="ramp-stop">
              <NumberInput value={s.at} onChange={(at) => onChange({ ...value, ramp: value.ramp.map((x, j) => (j === i ? { ...x, at } : x)) })} />
              <ColorInput value={s.color} onChange={(color) => onChange({ ...value, ramp: value.ramp.map((x, j) => (j === i ? { ...x, color } : x)) })} />
              <button className="btn ghost icon sm" aria-label="Remove stop" onClick={() => onChange({ ...value, ramp: value.ramp.filter((_, j) => j !== i) })}>
                <Trash2 size={13} />
              </button>
            </div>
          ))}
          <button className="btn sm" onClick={() => onChange({ ...value, ramp: [...value.ramp, { at: (value.ramp.at(-1)?.at ?? 60) + 10, color: "#ff3b5c" }] })}>
            Add stop
          </button>
        </>
      )}
      {value.kind === "hardware" && modes && (
        <>
          <Select value={value.mode} options={modes.map((m) => ({ value: m.name, label: m.name }))} onChange={(m) => onChange({ ...value, mode: m })} />
          {mode && (mode.flags & (1 << 6) || mode.flags & (1 << 5)) ? <ColorInput value={value.color ?? "#ffc20e"} onChange={(color) => onChange({ ...value, color })} /> : null}
          <div className="fields-2">
            {mode && mode.flags & (1 << 0) ? (
              <Field label={`Speed (${mode.speed_min}–${mode.speed_max})`}>
                <NumberInput value={value.speed ?? mode.speed} min={Math.min(mode.speed_min, mode.speed_max)} max={Math.max(mode.speed_min, mode.speed_max)} onChange={(speed) => onChange({ ...value, speed: Math.round(speed) })} />
              </Field>
            ) : null}
            {mode && mode.flags & (1 << 4) ? (
              <Field label={`Brightness (${mode.brightness_min}–${mode.brightness_max})`}>
                <NumberInput value={value.brightness ?? mode.brightness} min={Math.min(mode.brightness_min, mode.brightness_max)} max={Math.max(mode.brightness_min, mode.brightness_max)} onChange={(brightness) => onChange({ ...value, brightness: Math.round(brightness) })} />
              </Field>
            ) : null}
          </div>
          <span className="hint">Hardware modes keep running after Neru quits. Use “Save to device” to persist across reboots where supported.</span>
        </>
      )}
    </div>
  );
}

function Swatches({ colors }: { colors: string[] }) {
  if (!colors.length) return <span className="hint">no LEDs</span>;
  return (
    <div className="row" style={{ gap: 2, flexWrap: "wrap" }}>
      {colors.map((c, i) => (
        <span key={i} style={{ width: 10, height: 18, borderRadius: 3, background: c, border: "1px solid rgba(255,255,255,.08)" }} />
      ))}
    </div>
  );
}

function ZoneRow({ controller, zone, onSet }: { controller: RgbController; zone: RgbController["zones"][number]; onSet: (zone: number | null, effect: Effect | null) => Promise<void> }) {
  const [draft, setDraft] = useState<Effect | null>(null);
  const current = zone.effect;
  const editing = draft ?? current ?? null;
  return (
    <div style={{ padding: "10px 0", borderTop: "1px solid var(--border)" }}>
      <div className="row" style={{ alignItems: "flex-start" }}>
        <div style={{ width: 180, flex: "none" }}>
          <div className="small" style={{ fontWeight: 600 }}>
            {zone.name}
          </div>
          <div className="hint mono">
            {zone.leds_count} LEDs{zone.matrix ? ` · ${zone.matrix[0]}×${zone.matrix[1]}` : ""}
            {zone.segments.length ? ` · ${zone.segments.join(", ")}` : ""}
          </div>
          <div style={{ marginTop: 6 }}>
            <Swatches colors={zone.preview} />
          </div>
        </div>
        <div style={{ flex: 1, minWidth: 0 }}>
          {editing ? (
            <EffectEditor value={editing} onChange={setDraft} />
          ) : (
            <span className="hint">Follows the device setting{controller.software ? "" : " (hardware mode)"}.</span>
          )}
        </div>
        <div className="col" style={{ gap: 6, flex: "none" }}>
          {editing ? (
            <button className="btn primary sm" disabled={!draft} onClick={() => draft && onSet(zone.index, draft).then(() => setDraft(null))}>
              Apply
            </button>
          ) : (
            <button className="btn sm" onClick={() => setDraft(defaultEffect("static"))}>
              Set effect
            </button>
          )}
          {(current || draft) && (
            <button className="btn ghost sm" onClick={() => onSet(zone.index, null).then(() => setDraft(null))}>
              Clear
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

function DeviceCard({ c, onSet, onSaveToDevice }: { c: RgbController; onSet: (zone: number | null, effect: Effect | null) => Promise<void>; onSaveToDevice: () => void }) {
  const Icon = KIND_ICON[c.kind] ?? Lightbulb;
  const deviceEffect = c.zones.find((z) => z.effect)?.effect ?? null;
  const [draft, setDraft] = useState<Effect | null>(null);
  const activeMode = c.modes[c.active_mode]?.name;
  return (
    <div className="card">
      <div className="card-head">
        <div className="card-title">
          <Icon size={18} />
          {c.name}
          <span className="badge">{c.kind}</span>
          {c.software ? <span className="badge accent">software</span> : activeMode ? <span className="badge violet">{activeMode}</span> : null}
        </div>
        <span className="hint mono">
          {c.vendor && `${c.vendor} · `}
          {c.leds} LEDs · {c.location}
        </span>
      </div>
      <div className="row" style={{ alignItems: "flex-start", gap: 14 }}>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div className="label" style={{ marginBottom: 6 }}>
            Whole device
          </div>
          <EffectEditor value={draft ?? deviceEffect ?? defaultEffect("static")} onChange={setDraft} modes={c.modes} />
        </div>
        <div className="col" style={{ gap: 6, flex: "none", paddingTop: 22 }}>
          <button className="btn primary sm" disabled={!draft} onClick={() => draft && onSet(null, draft).then(() => setDraft(null))}>
            <Play size={13} /> Apply
          </button>
          <button className="btn ghost sm" onClick={() => onSet(null, null).then(() => setDraft(null))}>
            Clear
          </button>
          <button className="btn ghost sm" title="Store the current mode in the device's own memory" onClick={onSaveToDevice}>
            <Save size={13} /> Save to device
          </button>
        </div>
      </div>
      {c.zones.length > 1 && (
        <div style={{ marginTop: 12 }}>
          <div className="label" style={{ marginBottom: 4 }}>
            Zones
          </div>
          {c.zones.map((z) => (
            <ZoneRow key={z.index} controller={c} zone={z} onSet={onSet} />
          ))}
        </div>
      )}
      {c.zones.length === 1 && (
        <div style={{ marginTop: 10 }}>
          <Swatches colors={c.zones[0].preview} />
        </div>
      )}
    </div>
  );
}

export function RgbPage() {
  const toast = useToast();
  const [status, setStatus] = useState<RgbStatus | null>(null);
  const [config, updateConfig] = useConfig();
  const [all, setAll] = useState<Effect>(defaultEffect("static"));
  const [profileName, setProfileName] = useState("");

  useEffect(() => {
    let alive = true;
    api.rgbStatus().then((s) => alive && setStatus(s)).catch(() => {});
    const un = api.onRgbStatus((s) => alive && setStatus(s));
    return () => {
      alive = false;
      un.then((f) => f()).catch(() => {});
    };
  }, []);

  const run = (p: Promise<unknown>, ok?: string): Promise<void> =>
    p
      .then(() => {
        if (ok) toast.success(ok);
      })
      .catch((e) => toast.error(errText(e)));
  const setAssignment = (controller: string) => (zone: number | null, effect: Effect | null) => run(api.rgbSetAssignment(controller, zone, effect), effect ? "Applied" : "Cleared");

  const devices = useMemo(() => status?.controllers ?? [], [status]);
  const enabled = config?.rgb.enabled ?? true;

  return (
    <div className="page stack">
      <div className="page-head">
        <div>
          <div className="eyebrow">// lighting</div>
          <h1 className="page-title">RGB</h1>
          <p className="page-sub">Motherboard, fans, pump, RAM and GPU lighting through OpenRGB. Effects run in Neru; hardware modes run on the devices.</p>
        </div>
        <div className="row">
          <span className={`badge ${status?.connected ? "green" : "red"}`}>{status?.connected ? `OpenRGB · ${devices.length} devices · v${status.protocol}` : "OpenRGB not connected"}</span>
          <button className="btn ghost icon" aria-label="Refresh" onClick={() => run(api.rgbRefresh())}>
            <RefreshCw size={16} />
          </button>
          <button className="btn ghost icon" aria-label="Rescan devices" title="Ask OpenRGB to detect devices again" onClick={() => run(api.rgbRescan(), "Rescanning…")}>
            <ScanSearch size={16} />
          </button>
        </div>
      </div>

      {!enabled && (
        <div className="banner info">
          RGB control is turned off.
          <button className="btn sm" onClick={() => run(updateConfig((c) => ({ ...c, rgb: { ...c.rgb, enabled: true } })))}>
            Turn on
          </button>
        </div>
      )}

      {enabled && status && !status.connected && (
        <div className="banner warn" style={{ alignItems: "flex-start" }}>
          <Lightbulb size={18} style={{ flex: "none", marginTop: 2 }} />
          <div>
            <div>
              <strong>No OpenRGB SDK server at {status.server}.</strong> Neru needs OpenRGB running with its server on (OpenRGB → Settings → “Start server”), or start a headless one here.
            </div>
            {status.error && <div className="hint mono" style={{ marginTop: 4 }}>{status.error}</div>}
          </div>
          <button className="btn primary sm" onClick={() => run(api.rgbStartServer(), "Starting OpenRGB server…")}>
            Start server
          </button>
        </div>
      )}

      {status?.connected && devices.length === 0 && (
        <div className="banner warn">
          OpenRGB is connected but reports no devices. In OpenRGB itself: check the udev rules (motherboard / USB controllers), the <code>i2c-dev</code> module (RAM, GPU), and “Rescan devices”.
        </div>
      )}

      {status?.last_message && <p className="hint">{status.last_message}</p>}

      <div className="grid" style={{ gridTemplateColumns: "minmax(0, 1.4fr) minmax(280px, 1fr)" }}>
        <div className="card">
          <div className="card-head">
            <div className="card-title">
              <Zap size={18} /> Everything at once
            </div>
            <span className="hint">one effect on every device</span>
          </div>
          <EffectEditor value={all} onChange={setAll} />
          <div className="row" style={{ marginTop: 12 }}>
            <button className="btn primary" disabled={!devices.length} onClick={() => run(api.rgbApplyAll(all), "Applied to all devices")}>
              <Play size={15} /> Apply to all
            </button>
            <button className="btn ghost" onClick={() => run(api.rgbClearAll(), "Neru no longer drives any device")}>
              Clear all
            </button>
            <span className="spacer" />
            <Field label={`Brightness ${config?.rgb.brightness ?? 100}%`}>
              <input
                className="range"
                type="range"
                min={0}
                max={100}
                value={config?.rgb.brightness ?? 100}
                onChange={(e) => {
                  const brightness = Number(e.target.value);
                  updateConfig((c) => ({ ...c, rgb: { ...c.rgb, brightness } })).catch(() => {});
                }}
                aria-label="Brightness"
              />
            </Field>
          </div>
        </div>

        <div className="card">
          <div className="card-head">
            <div className="card-title">OpenRGB profiles</div>
            <span className="hint">stored by OpenRGB</span>
          </div>
          {status?.profiles.length ? (
            <div className="col" style={{ gap: 6 }}>
              {status.profiles.map((p) => (
                <div key={p} className="row">
                  <span className="small" style={{ flex: 1 }}>
                    {p}
                  </span>
                  <button className="btn sm" onClick={() => run(api.rgbProfile("load", p), `Loading ${p}`)}>
                    Load
                  </button>
                  <button className="btn ghost icon sm" aria-label={`Delete ${p}`} onClick={() => run(api.rgbProfile("delete", p))}>
                    <Trash2 size={13} />
                  </button>
                </div>
              ))}
            </div>
          ) : (
            <p className="hint">No profiles yet.</p>
          )}
          <div className="row" style={{ marginTop: 10 }}>
            <input className="input" placeholder="New profile name" value={profileName} onChange={(e) => setProfileName(e.target.value)} />
            <button className="btn sm" disabled={!profileName.trim()} onClick={() => run(api.rgbProfile("save", profileName), "Profile saved").then(() => setProfileName(""))}>
              Save current
            </button>
          </div>
        </div>
      </div>

      <div className="eyebrow">// devices</div>
      {devices.map((c) => (
        <DeviceCard key={c.key} c={c} onSet={setAssignment(c.key)} onSaveToDevice={() => run(api.rgbSaveToDevice(c.key), "Saving mode to device…")} />
      ))}
    </div>
  );
}
