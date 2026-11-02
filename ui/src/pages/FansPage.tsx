import { Fan, Plus, RefreshCw, ShieldCheck, Thermometer, Trash2, Wrench } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { Field, NumberInput, Segmented, Select, fmt, useToast } from "../components/ui";
import { api, errText, type FanChannel, type FanChannelConfig, type FanChannelMode, type FanStatus } from "../lib/api";

// ── curve editor ────────────────────────────────────────────────────────────

const T_MIN = 20;
const T_MAX = 100;

function CurveEditor({ points, onChange, temp }: { points: [number, number][]; onChange: (p: [number, number][]) => void; temp?: number | null }) {
  const ref = useRef<SVGSVGElement>(null);
  const drag = useRef<number | null>(null);
  const W = 400;
  const H = 170;
  const px = (t: number) => ((t - T_MIN) / (T_MAX - T_MIN)) * W;
  const py = (d: number) => H - (d / 100) * H;
  const sorted = [...points].sort((a, b) => a[0] - b[0]);
  const path = sorted.map((p, i) => `${i ? "L" : "M"} ${px(p[0])} ${py(p[1])}`).join(" ");
  const full = sorted.length ? `M 0 ${py(sorted[0][1])} ${path} L ${W} ${py(sorted[sorted.length - 1][1])}` : "";

  const toData = (e: React.PointerEvent): [number, number] => {
    const r = ref.current!.getBoundingClientRect();
    const x = ((e.clientX - r.left) / r.width) * W;
    const y = ((e.clientY - r.top) / r.height) * H;
    return [Math.round(T_MIN + (x / W) * (T_MAX - T_MIN)), Math.round(100 - (y / H) * 100)];
  };

  return (
    <svg
      ref={ref}
      viewBox={`0 0 ${W} ${H}`}
      style={{ width: "100%", height: "auto", display: "block", background: "var(--bg)", borderRadius: 8, border: "1px solid var(--border-strong)", touchAction: "none" }}
      onPointerMove={(e) => {
        if (drag.current === null) return;
        const [t, d] = toData(e);
        onChange(points.map((p, i) => (i === drag.current ? [Math.max(T_MIN, Math.min(T_MAX, t)), Math.max(0, Math.min(100, d))] : p)));
      }}
      onPointerUp={() => (drag.current = null)}
      onPointerLeave={() => (drag.current = null)}
      onDoubleClick={(e) => {
        const r = ref.current!.getBoundingClientRect();
        const x = ((e.clientX - r.left) / r.width) * W;
        const y = ((e.clientY - r.top) / r.height) * H;
        onChange([...points, [Math.round(T_MIN + (x / W) * (T_MAX - T_MIN)), Math.round(100 - (y / H) * 100)]]);
      }}
    >
      {[25, 50, 75].map((d) => (
        <line key={d} x1={0} x2={W} y1={py(d)} y2={py(d)} stroke="var(--border)" strokeWidth={1} />
      ))}
      {[40, 60, 80].map((t) => (
        <line key={t} x1={px(t)} x2={px(t)} y1={0} y2={H} stroke="var(--border)" strokeWidth={1} />
      ))}
      {[40, 60, 80].map((t) => (
        <text key={t} x={px(t) + 3} y={H - 4} fontSize={10} fill="var(--text-muted)" fontFamily="var(--font-mono)">
          {t}°
        </text>
      ))}
      {[25, 50, 75].map((d) => (
        <text key={d} x={3} y={py(d) - 3} fontSize={10} fill="var(--text-muted)" fontFamily="var(--font-mono)">
          {d}%
        </text>
      ))}
      {temp != null && <line x1={px(temp)} x2={px(temp)} y1={0} y2={H} stroke="var(--violet)" strokeWidth={1.5} strokeDasharray="4 3" />}
      <path d={full} fill="none" stroke="var(--accent)" strokeWidth={2.5} strokeLinejoin="round" />
      {points.map((p, i) => (
        <circle
          key={i}
          cx={px(p[0])}
          cy={py(p[1])}
          r={7}
          fill="var(--accent)"
          stroke="#000"
          strokeWidth={1.5}
          style={{ cursor: "grab" }}
          onPointerDown={(e) => {
            (e.target as Element).setPointerCapture(e.pointerId);
            drag.current = i;
          }}
        />
      ))}
    </svg>
  );
}

// ── channel card ────────────────────────────────────────────────────────────

function ChannelCard({ ch, cfg, status, onChange }: { ch: FanChannel; cfg: FanChannelConfig; status: FanStatus; onChange: (c: FanChannelConfig) => void }) {
  const state = status.states.find((s) => s.id === ch.id);
  const mode = cfg.mode;
  const sensors = status.inventory.sensors;
  const sensorTemp = mode.mode === "curve" ? sensors.find((s) => s.id === mode.sensor)?.temp_c : null;
  const setMode = (m: FanChannelMode) => onChange({ ...cfg, mode: m });
  const duty = ch.pwm != null ? (ch.pwm / 255) * 100 : null;

  return (
    <div className="card">
      <div className="card-head">
        <div className="card-title">
          <Fan size={18} />
          <input className="input" style={{ width: 200, padding: "4px 8px" }} value={cfg.name ?? ch.label} onChange={(e) => onChange({ ...cfg, name: e.target.value })} aria-label="Fan name" />
          <span className="badge">{ch.chip}</span>
          {!ch.writable && !status.helper && <span className="badge red">read-only</span>}
        </div>
        <div className="row mono small muted">
          <span>{ch.rpm != null ? `${ch.rpm} RPM` : "no tach"}</span>
          <span>·</span>
          <span>{duty != null ? `${duty.toFixed(0)}% duty` : "--"}</span>
          {state?.target_pct != null && (
            <>
              <span>·</span>
              <span style={{ color: "var(--accent)" }}>target {state.target_pct.toFixed(0)}%</span>
            </>
          )}
        </div>
      </div>
      {state?.error && <div className="banner error small" style={{ marginBottom: 10 }}>{state.error}</div>}
      <div className="row" style={{ marginBottom: 12 }}>
        <Segmented
          ariaLabel="Mode"
          value={mode.mode}
          options={[
            { value: "auto", label: "BIOS" },
            { value: "fixed", label: "Fixed" },
            { value: "curve", label: "Curve" },
          ]}
          onChange={(m) => {
            if (m === "auto") setMode({ mode: "auto" });
            else if (m === "fixed") setMode({ mode: "fixed", pct: duty != null ? Math.round(duty) : 50 });
            else setMode({ mode: "curve", sensor: sensors[0]?.id ?? "", points: status.presets[1]?.[1] ?? [[30, 25], [85, 100]], min_pct: 20, slew_pct_per_s: 10 });
          }}
        />
        <span className="hint">{mode.mode === "auto" ? "The motherboard's own curve (BIOS) controls this fan." : mode.mode === "fixed" ? "Constant speed." : "Speed follows a temperature."}</span>
      </div>
      {mode.mode === "fixed" && (
        <div className="row">
          <input className="range" type="range" min={0} max={100} value={mode.pct} onChange={(e) => setMode({ ...mode, pct: Number(e.target.value) })} aria-label="Duty" />
          <span className="mono small" style={{ width: 44, textAlign: "right" }}>
            {mode.pct}%
          </span>
        </div>
      )}
      {mode.mode === "curve" && (
        <div className="grid" style={{ gridTemplateColumns: "minmax(0, 1.3fr) minmax(220px, 1fr)", gap: 14 }}>
          <div>
            <CurveEditor points={mode.points as [number, number][]} onChange={(points) => setMode({ ...mode, points })} temp={sensorTemp} />
            <div className="row-wrap" style={{ marginTop: 8 }}>
              {status.presets.map(([name, pts]) => (
                <button key={name} className="btn sm" onClick={() => setMode({ ...mode, points: pts })}>
                  {name}
                </button>
              ))}
              <button className="btn ghost sm" onClick={() => setMode({ ...mode, points: [...mode.points, [60, 60]] })}>
                <Plus size={13} /> Point
              </button>
              <span className="hint">drag points · double-click to add</span>
            </div>
          </div>
          <div className="col">
            <Field label="Temperature source">
              <Select value={mode.sensor} options={sensors.map((s) => ({ value: s.id, label: `${s.id} (${fmt(s.temp_c, 0, "°")})` }))} onChange={(sensor) => setMode({ ...mode, sensor })} />
            </Field>
            <div className="fields-2">
              <Field label="Minimum %">
                <NumberInput value={mode.min_pct} min={0} max={100} onChange={(min_pct) => setMode({ ...mode, min_pct })} />
              </Field>
              <Field label="Max change %/s">
                <NumberInput value={mode.slew_pct_per_s} min={0} max={100} onChange={(slew_pct_per_s) => setMode({ ...mode, slew_pct_per_s })} />
              </Field>
            </div>
            <div className="col" style={{ gap: 4 }}>
              {[...mode.points]
                .map((p, i) => [p, i] as const)
                .sort((a, b) => a[0][0] - b[0][0])
                .map(([p, i]) => (
                  <div key={i} className="row mono small">
                    <span style={{ width: 54 }}>{p[0]}°C</span>
                    <span style={{ width: 54 }}>{p[1]}%</span>
                    <button className="btn ghost icon sm" aria-label="Remove point" disabled={mode.points.length <= 2} onClick={() => setMode({ ...mode, points: mode.points.filter((_, j) => j !== i) })}>
                      <Trash2 size={12} />
                    </button>
                  </div>
                ))}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

// ── page ────────────────────────────────────────────────────────────────────

export function FansPage() {
  const toast = useToast();
  const [status, setStatus] = useState<FanStatus | null>(null);
  const [draft, setDraft] = useState<FanChannelConfig[] | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(() => api.fansStatus().then(setStatus).catch((e) => toast.error(errText(e))), [toast]);
  useEffect(() => {
    refresh();
    const t = setInterval(refresh, 2000);
    return () => clearInterval(t);
  }, [refresh]);

  const configs: FanChannelConfig[] = useMemo(() => {
    if (!status) return [];
    return status.inventory.channels.map((ch) => {
      const fromDraft = draft?.find((c) => c.id === ch.id);
      const fromHelper = status.config.channels.find((c) => c.id === ch.id);
      return fromDraft ?? fromHelper ?? { id: ch.id, name: null, mode: { mode: "auto" } };
    });
  }, [status, draft]);

  const dirty = draft != null && JSON.stringify(configs) !== JSON.stringify(status?.inventory.channels.map((ch) => status.config.channels.find((c) => c.id === ch.id) ?? { id: ch.id, name: null, mode: { mode: "auto" } }));

  const apply = () => {
    setBusy(true);
    api
      .fansSetConfig({ channels: configs.filter((c) => c.mode.mode !== "auto" || c.name) })
      .then(() => {
        toast.success("Fan curves applied");
        setDraft(null);
        return refresh();
      })
      .catch((e) => toast.error(errText(e)))
      .finally(() => setBusy(false));
  };

  if (!status) return <div className="page muted">Loading…</div>;

  return (
    <div className="page stack">
      <div className="page-head">
        <div>
          <div className="eyebrow">// cooling</div>
          <h1 className="page-title">Fans</h1>
          <p className="page-sub">Fan and pump curves driven by any temperature the system exposes. Curves run in a small root helper, so they keep working when this window is closed.</p>
        </div>
        <div className="row">
          <span className={`badge ${status.helper ? "green" : "red"}`}>{status.helper ? `helper ${status.helper_version ?? ""}` : "helper not running"}</span>
          <button className="btn ghost icon" aria-label="Refresh" onClick={refresh}>
            <RefreshCw size={16} />
          </button>
        </div>
      </div>

      {!status.helper && (
        <div className="banner warn" style={{ alignItems: "flex-start" }}>
          <Wrench size={18} style={{ flex: "none", marginTop: 2 }} />
          <div>
            <strong>Writing fan speeds needs root.</strong> Install the <code>neru-fand</code> helper once (a systemd service started at boot, ~1 MB). It hands every fan back to the BIOS when it stops.
            {status.error && <div className="hint mono" style={{ marginTop: 4 }}>{status.error}</div>}
            {!status.helper_binary && <div className="hint" style={{ marginTop: 4 }}>The helper binary was not found next to the app — build it with <code>cargo build --release -p neru-fand</code>.</div>}
          </div>
          <button
            className="btn primary sm"
            disabled={!status.helper_binary}
            onClick={() =>
              api
                .fansInstallHelper()
                .then((m) => {
                  toast.success(m);
                  setTimeout(refresh, 1500);
                })
                .catch((e) => toast.error(errText(e)))
            }
          >
            <ShieldCheck size={14} /> Install helper
          </button>
        </div>
      )}

      {status.inventory.channels.length === 0 && (
        <div className="empty">
          <h3>No PWM fan channels found</h3>
          <span>
            Your motherboard's Super I/O chip is not exposed by the kernel. On ASUS boards try <code>sudo modprobe nct6775</code>; if that fails with a resource conflict, add{" "}
            <code>acpi_enforce_resources=lax</code> to the kernel command line. Fans wired to the AIO's own controller are not visible here.
          </span>
        </div>
      )}

      {status.inventory.sensors.length > 0 && (
        <div className="row-wrap">
          <Thermometer size={14} className="muted" />
          {status.inventory.sensors.map((s) => (
            <span key={s.id} className="badge" title={s.id}>
              {s.id} {fmt(s.temp_c, 0, "°")}
            </span>
          ))}
        </div>
      )}

      {status.inventory.channels.map((ch) => (
        <ChannelCard key={ch.id} ch={ch} cfg={configs.find((c) => c.id === ch.id)!} status={status} onChange={(c) => setDraft(configs.map((x) => (x.id === c.id ? c : x)))} />
      ))}

      {status.inventory.channels.length > 0 && (
        <div className="row" style={{ position: "sticky", bottom: 0, background: "var(--bg)", padding: "12px 0" }}>
          <button className="btn primary" disabled={!status.helper || busy || !dirty} onClick={apply}>
            Apply to fans
          </button>
          <button className="btn ghost" disabled={!dirty} onClick={() => setDraft(null)}>
            Discard changes
          </button>
          <span className="spacer" />
          {status.helper && (
            <button className="btn ghost sm" onClick={() => api.fansRelease().then(() => (toast.info("All fans back to BIOS control"), refresh())).catch((e) => toast.error(errText(e)))}>
              Release all to BIOS
            </button>
          )}
          {status.helper && (
            <button className="btn ghost sm danger" onClick={() => api.fansUninstallHelper().then((m) => (toast.success(m), refresh())).catch((e) => toast.error(errText(e)))}>
              Remove helper
            </button>
          )}
        </div>
      )}
    </div>
  );
}
