import { Image as ImageIcon, Plus, Trash2 } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";

import { ColorInput, Field, NumberInput, Select, Toggle } from "../../components/ui";
import { api, type Align, type Background, type Element, type Fit, type MetricKey, type Stop, type Theme } from "../../lib/api";
import { FONT_WEIGHTS, SIZE_PRESETS, TYPE_LABEL } from "../../lib/theme-utils";
import { MediaPicker } from "./MediaPicker";
import type { Editor } from "./useEditor";

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="inspector-section">
      <h4>{title}</h4>
      {children}
    </div>
  );
}

function MetricSelect({ value, onChange, keys, allowNone }: { value: string | null; onChange: (v: string | null) => void; keys: MetricKey[]; allowNone?: boolean }) {
  return (
    <select className="select" value={value ?? ""} onChange={(e) => onChange(e.target.value === "" ? null : e.target.value)}>
      {allowNone && <option value="">— none —</option>}
      {keys.map((k) => (
        <option key={k.key} value={k.key}>
          {k.label} ({k.unit}) · {k.key}
        </option>
      ))}
    </select>
  );
}

function RampEditor({ ramp, onChange }: { ramp: Stop[]; onChange: (r: Stop[]) => void }) {
  return (
    <div className="col" style={{ gap: 6 }}>
      {ramp.map((s, i) => (
        <div key={i} className="ramp-stop">
          <NumberInput value={s.at} onChange={(at) => onChange(ramp.map((x, j) => (j === i ? { ...x, at } : x)))} />
          <ColorInput value={s.color} onChange={(color) => onChange(ramp.map((x, j) => (j === i ? { ...x, color } : x)))} />
          <button className="btn ghost icon sm" aria-label="Remove stop" onClick={() => onChange(ramp.filter((_, j) => j !== i))}>
            <Trash2 size={13} />
          </button>
        </div>
      ))}
      <button className="btn sm" onClick={() => onChange([...ramp, { at: ramp.length ? ramp[ramp.length - 1].at + 20 : 40, color: "#ff3b5c" }])}>
        <Plus size={13} /> Add colour stop
      </button>
      <span className="hint">Colour follows the metric: e.g. 35 → blue, 65 → yellow, 90 → red.</span>
    </div>
  );
}

export function Inspector({ editor, fonts }: { editor: Editor; fonts: string[] }) {
  const { theme, selected, updateElement, patch } = editor;
  const [keys, setKeys] = useState<MetricKey[]>([]);
  const [picker, setPicker] = useState<null | "bg" | "image">(null);
  useEffect(() => {
    api.metricKeys().then(setKeys).catch(() => {});
  }, []);

  const fontOptions = fonts.map((f) => ({ value: f, label: f }));
  const fitOptions: { value: Fit; label: string }[] = [
    { value: "cover", label: "Cover (fill, crop)" },
    { value: "contain", label: "Contain (fit inside)" },
    { value: "stretch", label: "Stretch" },
  ];

  if (!selected) {
    const bg = theme.background;
    const setBg = (b: Background) =>
      patch((t) => {
        t.background = b;
      });
    return (
      <>
        <Section title="Theme">
          <Field label="Name">
            <input
              className="input"
              value={theme.name}
              onChange={(e) =>
                patch((t) => {
                  t.name = e.target.value;
                })
              }
            />
          </Field>
          <Field label="Screen size">
            <Select
              value={`${theme.width}x${theme.height}`}
              options={[
                ...SIZE_PRESETS.map((p) => ({ value: `${p.w}x${p.h}`, label: p.label })),
                ...(SIZE_PRESETS.some((p) => p.w === theme.width && p.h === theme.height) ? [] : [{ value: `${theme.width}x${theme.height}`, label: `${theme.width} × ${theme.height} (custom)` }]),
              ]}
              onChange={(v) => {
                const [w, h] = v.split("x").map(Number);
                patch((t) => {
                  t.width = w;
                  t.height = h;
                });
              }}
            />
          </Field>
          <Field label="Smoothing" hint="Eases bar and gauge motion between sensor updates.">
            <div className="row">
              <input
                className="range"
                type="range"
                min={0}
                max={0.95}
                step={0.05}
                value={theme.smoothing}
                onChange={(e) =>
                  editor.live((t) => {
                    t.smoothing = Number(e.target.value);
                  })
                }
                onPointerUp={editor.commit}
                aria-label="Smoothing"
              />
              <span className="mono small" style={{ width: 34, textAlign: "right" }}>
                {theme.smoothing.toFixed(2)}
              </span>
            </div>
          </Field>
        </Section>
        <Section title="Background">
          <Field label="Type">
            <Select
              value={bg.kind}
              options={[
                { value: "color", label: "Solid colour" },
                { value: "gradient", label: "Gradient" },
                { value: "media", label: "Image / GIF / video" },
              ]}
              onChange={(kind) => {
                if (kind === "color") setBg({ kind: "color", color: bg.kind === "color" ? bg.color : "#0b0b12" });
                else if (kind === "gradient") setBg({ kind: "gradient", from: "#0b0b14", to: "#1a1230", angle: 160 });
                else setBg({ kind: "media", path: "", fit: "cover", opacity: 1 });
              }}
            />
          </Field>
          {bg.kind === "color" && (
            <Field label="Colour">
              <ColorInput value={bg.color} onChange={(color) => setBg({ kind: "color", color })} />
            </Field>
          )}
          {bg.kind === "gradient" && (
            <>
              <div className="fields-2">
                <Field label="From">
                  <ColorInput value={bg.from} onChange={(from) => setBg({ ...bg, from })} />
                </Field>
                <Field label="To">
                  <ColorInput value={bg.to} onChange={(to) => setBg({ ...bg, to })} />
                </Field>
              </div>
              <Field label="Angle" hint="0 = top to bottom, 90 = left to right">
                <NumberInput value={bg.angle} onChange={(angle) => setBg({ ...bg, angle })} />
              </Field>
            </>
          )}
          {bg.kind === "media" && (
            <>
              <Field label="File">
                <div className="row">
                  <input className="input mono" value={bg.path} readOnly placeholder="choose a file…" />
                  <button className="btn icon" aria-label="Choose media" onClick={() => setPicker("bg")}>
                    <ImageIcon size={15} />
                  </button>
                </div>
              </Field>
              <div className="fields-2">
                <Field label="Fit">
                  <Select value={bg.fit} options={fitOptions} onChange={(fit) => setBg({ ...bg, fit })} />
                </Field>
                <Field label="Opacity">
                  <NumberInput value={bg.opacity} min={0} max={1} step={0.05} onChange={(opacity) => setBg({ ...bg, opacity })} />
                </Field>
              </div>
            </>
          )}
        </Section>
        {picker === "bg" && bg.kind === "media" && (
          <MediaPicker
            value={bg.path}
            onClose={() => setPicker(null)}
            onPick={(path) => {
              setBg({ ...bg, path });
              setPicker(null);
            }}
          />
        )}
      </>
    );
  }

  const el = selected;
  const up = (fn: (e: Element) => void) => updateElement(el.id, fn);
  const num = (key: string, label: string, opts?: { step?: number; min?: number; max?: number }) => (
    <Field label={label}>
      <NumberInput
        value={(el as unknown as Record<string, number>)[key]}
        step={opts?.step}
        min={opts?.min}
        max={opts?.max}
        onChange={(v) => up((e) => ((e as unknown as Record<string, number>)[key] = v))}
      />
    </Field>
  );
  const color = (key: string, label: string, allowNone?: boolean) => (
    <Field label={label}>
      <ColorInput value={(el as unknown as Record<string, string>)[key]} allowNone={allowNone} onChange={(v) => up((e) => ((e as unknown as Record<string, string>)[key] = v))} />
    </Field>
  );
  const metric = (key: string, label: string) => (
    <Field label={label}>
      <MetricSelect value={(el as unknown as Record<string, string>)[key]} keys={keys} onChange={(v) => up((e) => ((e as unknown as Record<string, string>)[key] = v ?? "cpu.usage"))} />
    </Field>
  );
  const ramp = (key: "ramp") =>
    "ramp" in el ? (
      <Section title="Colour by value">
        <RampEditor ramp={el.ramp} onChange={(r) => up((e) => ((e as unknown as Record<string, Stop[]>)[key] = r))} />
      </Section>
    ) : null;

  return (
    <>
      <Section title={TYPE_LABEL[el.type]}>
        <div className="fields-2">
          {num("x", el.type === "gauge" ? "Centre X" : "X")}
          {num("y", el.type === "gauge" ? "Centre Y" : "Y")}
          {num("rotation", "Rotation °")}
          {num("opacity", "Opacity", { step: 0.05, min: 0, max: 1 })}
        </div>
        <div className="row small">
          <Toggle on={el.visible} label="Visible" onChange={(v) => up((e) => (e.visible = v))} /> Visible
          <span className="spacer" />
          <Toggle on={el.locked} label="Locked" onChange={(v) => up((e) => (e.locked = v))} /> Locked
        </div>
      </Section>

      {el.type === "text" && (
        <>
          <Section title="Text">
            <Field label="Content" hint="Placeholders: {cpu.temp:0}, {gpu.usage}, {mem.pct}, {time:%H:%M}, {date:%a %d}, {cpu.name}">
              <textarea className="textarea" rows={2} value={el.text} onChange={(e) => up((x) => x.type === "text" && (x.text = e.target.value))} />
            </Field>
            <Field label="Font">
              <Select value={el.font} options={fontOptions.some((f) => f.value === el.font) ? fontOptions : [{ value: el.font, label: `${el.font} (missing)` }, ...fontOptions]} onChange={(font) => up((x) => x.type === "text" && (x.font = font))} />
            </Field>
            <div className="fields-3">
              {num("size", "Size", { min: 4 })}
              <Field label="Weight">
                <Select value={String(el.weight)} options={FONT_WEIGHTS} onChange={(w) => up((x) => x.type === "text" && (x.weight = Number(w)))} />
              </Field>
              {num("letter_spacing", "Spacing")}
            </div>
            <div className="fields-2">
              {color("color", "Colour")}
              <Field label="Align">
                <Select<Align> value={el.align} options={[{ value: "left", label: "Left" }, { value: "center", label: "Centre" }, { value: "right", label: "Right" }]} onChange={(align) => up((x) => x.type === "text" && (x.align = align))} />
              </Field>
            </div>
          </Section>
          <Section title="Colour by value">
            <Field label="Metric">
              <MetricSelect allowNone value={el.ramp_metric} keys={keys} onChange={(v) => up((x) => x.type === "text" && (x.ramp_metric = v))} />
            </Field>
            {el.ramp_metric && <RampEditor ramp={el.ramp} onChange={(r) => up((x) => x.type === "text" && (x.ramp = r))} />}
          </Section>
        </>
      )}

      {el.type === "bar" && (
        <>
          <Section title="Bar">
            {metric("metric", "Metric")}
            <div className="fields-2">
              {num("w", "Width", { min: 4 })}
              {num("h", "Height", { min: 2 })}
              {num("min", "Min")}
              {num("max", "Max")}
              {color("color", "Fill")}
              {color("track", "Track")}
              {num("radius", "Corner radius", { min: 0 })}
              <Field label="Direction">
                <Select value={el.vertical ? "v" : "h"} options={[{ value: "h", label: "Left → right" }, { value: "v", label: "Bottom → top" }]} onChange={(v) => up((x) => x.type === "bar" && (x.vertical = v === "v"))} />
              </Field>
            </div>
          </Section>
          {ramp("ramp")}
        </>
      )}

      {el.type === "gauge" && (
        <>
          <Section title="Gauge">
            {metric("metric", "Metric")}
            <div className="fields-2">
              {num("r", "Radius", { min: 4 })}
              {num("thickness", "Thickness", { min: 1 })}
              {num("min", "Min")}
              {num("max", "Max")}
              {num("start", "Start angle °")}
              {num("sweep", "Sweep °")}
              {color("color", "Fill")}
              {color("track", "Track")}
            </div>
            <div className="row small">
              <Toggle on={el.round_caps} label="Round caps" onChange={(v) => up((x) => x.type === "gauge" && (x.round_caps = v))} /> Round caps
            </div>
            <span className="hint">Start −135 with sweep 270 is the classic three-quarter dial.</span>
          </Section>
          {ramp("ramp")}
        </>
      )}

      {el.type === "rect" && (
        <Section title="Rectangle">
          <div className="fields-2">
            {num("w", "Width", { min: 1 })}
            {num("h", "Height", { min: 1 })}
            {color("fill", "Fill", true)}
            {color("stroke", "Stroke", true)}
            {num("stroke_width", "Stroke width", { min: 0 })}
            {num("radius", "Corner radius", { min: 0 })}
          </div>
        </Section>
      )}

      {el.type === "ellipse" && (
        <Section title="Ellipse">
          <div className="fields-2">
            {num("rx", "Radius X", { min: 1 })}
            {num("ry", "Radius Y", { min: 1 })}
            {color("fill", "Fill", true)}
            {color("stroke", "Stroke", true)}
            {num("stroke_width", "Stroke width", { min: 0 })}
          </div>
        </Section>
      )}

      {el.type === "line" && (
        <Section title="Line">
          <div className="fields-2">
            {num("w", "ΔX")}
            {num("h", "ΔY")}
            {color("stroke", "Colour")}
            {num("stroke_width", "Width", { min: 0.5, step: 0.5 })}
          </div>
          <div className="row small">
            <Toggle on={el.round_caps} label="Round caps" onChange={(v) => up((x) => x.type === "line" && (x.round_caps = v))} /> Round caps
          </div>
        </Section>
      )}

      {el.type === "image" && (
        <>
          <Section title="Image">
            <Field label="File">
              <div className="row">
                <input className="input mono" value={el.path} readOnly placeholder="choose a file…" />
                <button className="btn icon" aria-label="Choose image" onClick={() => setPicker("image")}>
                  <ImageIcon size={15} />
                </button>
              </div>
            </Field>
            <div className="fields-2">
              {num("w", "Width", { min: 1 })}
              {num("h", "Height", { min: 1 })}
            </div>
            <Field label="Fit">
              <Select value={el.fit} options={fitOptions} onChange={(fit) => up((x) => x.type === "image" && (x.fit = fit))} />
            </Field>
          </Section>
          {picker === "image" && (
            <MediaPicker
              value={el.path}
              onClose={() => setPicker(null)}
              onPick={(path) => {
                up((x) => x.type === "image" && (x.path = path));
                setPicker(null);
              }}
            />
          )}
        </>
      )}
    </>
  );
}

export type { Theme };
