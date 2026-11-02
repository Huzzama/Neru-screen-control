import type { Element, ElementType, Theme } from "./api";

export const SIZE_PRESETS: { label: string; w: number; h: number }[] = [
  { label: "720 × 720 — Thermaltake TH V3 Ultra", w: 720, h: 720 },
  { label: "320 × 320 — Thermalright (ChiZhu)", w: 320, h: 320 },
  { label: "1110 × 540 — Thermaltake 6\" panel", w: 1110, h: 540 },
  { label: "480 × 480", w: 480, h: 480 },
  { label: "640 × 480", w: 640, h: 480 },
];

export const FONT_WEIGHTS = [
  { value: "400", label: "Regular" },
  { value: "500", label: "Medium" },
  { value: "600", label: "Semibold" },
  { value: "700", label: "Bold" },
];

let counter = 0;
export function newId(type: string): string {
  counter += 1;
  return `${type}-${Date.now().toString(36)}${counter}`;
}

/** A fresh element of `type`, sized for a `w`×`h` theme, centred. */
export function makeElement(type: ElementType, w: number, h: number): Element {
  const s = Math.min(w, h) / 720; // scale relative to the 720 design
  const base = { rotation: 0, opacity: 1, visible: true, locked: false };
  switch (type) {
    case "text":
      return { ...base, id: newId("text"), type, x: w / 2, y: h / 2, text: "CPU {cpu.temp:0}°", font: "Inter", size: Math.round(64 * s), weight: 700, color: "#ffffff", align: "center", letter_spacing: 0, ramp_metric: null, ramp: [] };
    case "bar":
      return { ...base, id: newId("bar"), type, x: w / 2 - 150 * s, y: h / 2 - 8 * s, w: Math.round(300 * s), h: Math.round(16 * s), metric: "cpu.usage", min: 0, max: 100, color: "#ffc20e", track: "#1f1d33", radius: Math.round(8 * s), vertical: false, ramp: [] };
    case "gauge":
      return { ...base, id: newId("gauge"), type, x: w / 2, y: h / 2, r: Math.round(120 * s), thickness: Math.round(22 * s), metric: "cpu.temp", min: 20, max: 95, start: -135, sweep: 270, color: "#9b30ff", track: "#1f1d33", round_caps: true, ramp: [] };
    case "rect":
      return { ...base, id: newId("rect"), type, x: w / 2 - 100 * s, y: h / 2 - 50 * s, w: Math.round(200 * s), h: Math.round(100 * s), fill: "#1a1a2a", stroke: "none", stroke_width: 0, radius: Math.round(16 * s) };
    case "ellipse":
      return { ...base, id: newId("ellipse"), type, x: w / 2 - 60 * s, y: h / 2 - 60 * s, rx: Math.round(60 * s), ry: Math.round(60 * s), fill: "#1a1a2a", stroke: "none", stroke_width: 0 };
    case "line":
      return { ...base, id: newId("line"), type, x: w / 2 - 100 * s, y: h / 2, w: Math.round(200 * s), h: 0, stroke: "#ffffff", stroke_width: 2, round_caps: true };
    case "image":
      return { ...base, id: newId("image"), type, x: w / 2 - 100 * s, y: h / 2 - 100 * s, w: Math.round(200 * s), h: Math.round(200 * s), path: "", fit: "cover" };
  }
}

export const TYPE_LABEL: Record<ElementType, string> = {
  text: "Text",
  bar: "Bar",
  gauge: "Gauge",
  rect: "Rectangle",
  ellipse: "Ellipse",
  line: "Line",
  image: "Image",
};

export function elementLabel(el: Element): string {
  switch (el.type) {
    case "text":
      return el.text.length > 24 ? el.text.slice(0, 24) + "…" : el.text || "Text";
    case "bar":
    case "gauge":
      return `${TYPE_LABEL[el.type]} · ${el.metric}`;
    case "image":
      return el.path ? el.path.split("/").pop() ?? el.path : "Image (no file)";
    default:
      return TYPE_LABEL[el.type];
  }
}

export interface BBox { x: number; y: number; w: number; h: number }

let measureSvg: SVGSVGElement | null = null;
let measureText: SVGTextElement | null = null;

/** Measure text the way the browser lays it out (same fonts as the renderer). */
export function measureTextBox(el: Extract<Element, { type: "text" }>, sample?: string): BBox {
  if (!measureSvg) {
    measureSvg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    measureSvg.setAttribute("aria-hidden", "true");
    measureSvg.style.cssText = "position:absolute;left:-10000px;top:-10000px;width:1px;height:1px;overflow:hidden";
    measureText = document.createElementNS("http://www.w3.org/2000/svg", "text");
    measureSvg.appendChild(measureText);
    document.body.appendChild(measureSvg);
  }
  const t = measureText!;
  t.setAttribute("x", String(el.x));
  t.setAttribute("y", String(el.y));
  t.setAttribute("font-family", el.font);
  t.setAttribute("font-size", String(el.size));
  t.setAttribute("font-weight", String(el.weight));
  t.setAttribute("text-anchor", el.align === "left" ? "start" : el.align === "center" ? "middle" : "end");
  t.setAttribute("letter-spacing", String(el.letter_spacing || 0));
  t.setAttribute("xml:space", "preserve");
  t.textContent = sample ?? previewText(el.text);
  try {
    const b = t.getBBox();
    if (b.width > 0 || b.height > 0) return { x: b.x, y: b.y, w: Math.max(b.width, 4), h: Math.max(b.height, 4) };
  } catch {
    /* detached */
  }
  const w = Math.max(4, el.text.length * el.size * 0.55);
  const x = el.align === "left" ? el.x : el.align === "center" ? el.x - w / 2 : el.x - w;
  return { x, y: el.y - el.size * 0.8, w, h: el.size };
}

/** Placeholder values so measured widths resemble the rendered ones. */
export function previewText(text: string): string {
  return text.replace(/\{([^}:]+)(?::([^}]*))?\}/g, (_m, key: string, arg?: string) => {
    if (key === "time") return "12:34";
    if (key === "date") return "Monday 02 November";
    if (key.endsWith(".name")) return "Ryzen 7 7800X3D";
    const d = arg ? parseInt(arg, 10) || 0 : 0;
    return (62.5).toFixed(d);
  });
}

export function bbox(el: Element): BBox {
  switch (el.type) {
    case "text":
      return measureTextBox(el);
    case "bar":
    case "rect":
    case "image":
      return { x: el.x, y: el.y, w: el.w, h: el.h };
    case "gauge": {
      const r = el.r + el.thickness / 2;
      return { x: el.x - r, y: el.y - r, w: r * 2, h: r * 2 };
    }
    case "ellipse":
      return { x: el.x, y: el.y, w: el.rx * 2, h: el.ry * 2 };
    case "line": {
      const x = Math.min(el.x, el.x + el.w);
      const y = Math.min(el.y, el.y + el.h);
      return { x: x - 4, y: y - 4, w: Math.abs(el.w) + 8, h: Math.abs(el.h) + 8 };
    }
  }
}

export function clone<T>(v: T): T {
  return structuredClone(v);
}

/** Fill in defaults for fields a hand-written theme file may omit. */
export function normalizeTheme(t: Theme): Theme {
  const out = clone(t);
  out.version ??= 1;
  out.smoothing ??= 0.5;
  out.background ??= { kind: "color", color: "#0b0b12" };
  out.elements = (out.elements ?? []).map((raw) => {
    const r = raw as Partial<Element> & { type: ElementType };
    const el = { ...r, rotation: r.rotation ?? 0, opacity: r.opacity ?? 1, visible: r.visible ?? true, locked: r.locked ?? false } as Element;
    if (!el.id) el.id = newId(el.type);
    if (el.type === "text") {
      el.font ??= "Inter";
      el.size ??= 48;
      el.weight ??= 600;
      el.color ??= "#ffffff";
      el.align ??= "left";
      el.letter_spacing ??= 0;
      el.ramp_metric ??= null;
      el.ramp ??= [];
    }
    if ((el.type === "bar" || el.type === "gauge") && !el.ramp) el.ramp = [];
    return el;
  });
  return out;
}

export function blankTheme(w: number, h: number, name = "New theme"): Theme {
  return { name, version: 1, width: w, height: h, background: { kind: "color", color: "#0b0b12" }, elements: [], smoothing: 0.5 };
}
