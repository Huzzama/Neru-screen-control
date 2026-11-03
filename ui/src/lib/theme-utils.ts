import type { Element, ElementType, Stop, Theme } from "./api";

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
    case "graph":
      return { ...base, id: newId("graph"), type, x: w / 2 - 150 * s, y: h / 2 - 50 * s, w: Math.round(300 * s), h: Math.round(100 * s), metric: "cpu.usage", seconds: 60, min: 0, max: 100, auto_scale: false, style: "area", color: "#3dd6ff", fill: "#3dd6ff40", track: "#15152280", stroke_width: Math.max(1, Math.round(3 * s)), radius: Math.round(12 * s), smooth: true, ramp: [] };
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
  graph: "Graph",
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
    case "graph":
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
    if (key === "uptime" || arg === "uptime") return "3d 04:12";
    if (key.endsWith(".name") || key === "net.iface") return "Ryzen 7 7800X3D";
    if (arg === "speed") return "12.3 MB/s";
    if (arg === "bits") return "98.4 Mbps";
    if (arg === "size" || arg === "bytes") return "12.3 GB";
    if (arg === "pct") return "62%";
    const d = arg ? parseInt(arg, 10) || 0 : 0;
    return (62.5).toFixed(d);
  });
}

export function bbox(el: Element): BBox {
  switch (el.type) {
    case "text":
      return measureTextBox(el);
    case "bar":
    case "graph":
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
    if ((el.type === "bar" || el.type === "gauge" || el.type === "graph") && !el.ramp) el.ramp = [];
    if (el.type === "graph") {
      el.seconds ??= 60;
      el.min ??= 0;
      el.max ??= 100;
      el.auto_scale ??= false;
      el.style ??= "area";
      el.color ??= "#3dd6ff";
      el.fill ??= "#3dd6ff40";
      el.track ??= "#15152280";
      el.stroke_width ??= 3;
      el.radius ??= 12;
      el.smooth ??= true;
    }
    return el;
  });
  return out;
}

export function blankTheme(w: number, h: number, name = "New theme"): Theme {
  return { name, version: 1, width: w, height: h, background: { kind: "color", color: "#0b0b12" }, elements: [], smoothing: 0.5 };
}

// ── Ready-made widgets ──────────────────────────────────────────────────────

export interface Preset {
  id: string;
  label: string;
  hint: string;
  build: (w: number, h: number) => Element[];
}

const RAMP_TEMP: Stop[] = [
  { at: 40, color: "#3dd6ff" },
  { at: 70, color: "#ffc20e" },
  { at: 85, color: "#ff3b5c" },
];

function text(w: number, h: number, dx: number, dy: number, content: string, size: number, extra: Partial<Extract<Element, { type: "text" }>> = {}): Element {
  const s = Math.min(w, h) / 720;
  const el = makeElement("text", w, h) as Extract<Element, { type: "text" }>;
  return { ...el, x: w / 2 + dx * s, y: h / 2 + dy * s, text: content, size: Math.round(size * s), ...extra };
}

/** Widgets that drop several elements at once, centred on the theme. */
export const PRESETS: Preset[] = [
  {
    id: "cpu_temp",
    label: "CPU temperature",
    hint: "Big number coloured by heat",
    build: (w, h) => [text(w, h, 0, -40, "CPU", 28, { weight: 500, color: "#9a9ab5", letter_spacing: 4 }), text(w, h, 0, 50, "{cpu.temp:0}°", 110, { weight: 700, ramp_metric: "cpu.temp", ramp: RAMP_TEMP })],
  },
  {
    id: "gpu_temp",
    label: "GPU temperature",
    hint: "Big number coloured by heat",
    build: (w, h) => [text(w, h, 0, -40, "GPU", 28, { weight: 500, color: "#9a9ab5", letter_spacing: 4 }), text(w, h, 0, 50, "{gpu.temp:0}°", 110, { weight: 700, ramp_metric: "gpu.temp", ramp: RAMP_TEMP })],
  },
  {
    id: "cpu_usage_bar",
    label: "CPU usage bar",
    hint: "Label, percentage and a bar",
    build: (w, h) => {
      const s = Math.min(w, h) / 720;
      const bar = makeElement("bar", w, h) as Extract<Element, { type: "bar" }>;
      return [
        text(w, h, -150, -16, "CPU", 24, { align: "left", weight: 600, color: "#9a9ab5" }),
        text(w, h, 150, -16, "{cpu.usage:0}%", 24, { align: "right", weight: 600 }),
        { ...bar, x: w / 2 - 150 * s, y: h / 2 - 4 * s, w: Math.round(300 * s), h: Math.round(14 * s), metric: "cpu.usage", color: "#ffc20e" },
      ];
    },
  },
  {
    id: "ram_bar",
    label: "RAM usage bar",
    hint: "Used / total and a bar",
    build: (w, h) => {
      const s = Math.min(w, h) / 720;
      const bar = makeElement("bar", w, h) as Extract<Element, { type: "bar" }>;
      return [
        text(w, h, -150, -16, "RAM", 24, { align: "left", weight: 600, color: "#9a9ab5" }),
        text(w, h, 150, -16, "{mem.used:size} / {mem.total:size}", 22, { align: "right", weight: 500 }),
        { ...bar, x: w / 2 - 150 * s, y: h / 2 - 4 * s, w: Math.round(300 * s), h: Math.round(14 * s), metric: "mem.pct", color: "#9b30ff" },
      ];
    },
  },
  {
    id: "cpu_gauge",
    label: "CPU temperature dial",
    hint: "Three-quarter gauge with the value inside",
    build: (w, h) => {
      const g = makeElement("gauge", w, h) as Extract<Element, { type: "gauge" }>;
      return [{ ...g, metric: "cpu.temp", min: 20, max: 95, ramp: RAMP_TEMP }, text(w, h, 0, 22, "{cpu.temp:0}°", 64, { weight: 700 }), text(w, h, 0, 70, "CPU", 20, { weight: 500, color: "#9a9ab5", letter_spacing: 3 })];
    },
  },
  {
    id: "cpu_graph",
    label: "CPU usage graph",
    hint: "Last 60 seconds",
    build: (w, h) => {
      const s = Math.min(w, h) / 720;
      const g = makeElement("graph", w, h) as Extract<Element, { type: "graph" }>;
      return [text(w, h, -150, -62, "CPU  {cpu.usage:0}%", 22, { align: "left", weight: 600, color: "#9a9ab5" }), { ...g, x: w / 2 - 150 * s, y: h / 2 - 50 * s, w: Math.round(300 * s), h: Math.round(100 * s), metric: "cpu.usage" }];
    },
  },
  {
    id: "gpu_graph",
    label: "GPU usage graph",
    hint: "Last 60 seconds",
    build: (w, h) => {
      const s = Math.min(w, h) / 720;
      const g = makeElement("graph", w, h) as Extract<Element, { type: "graph" }>;
      return [text(w, h, -150, -62, "GPU  {gpu.usage:0}%", 22, { align: "left", weight: 600, color: "#9a9ab5" }), { ...g, x: w / 2 - 150 * s, y: h / 2 - 50 * s, w: Math.round(300 * s), h: Math.round(100 * s), metric: "gpu.usage", color: "#9b30ff", fill: "#9b30ff40" }];
    },
  },
  {
    id: "net_graphs",
    label: "Network ↓ ↑ graphs",
    hint: "Download and upload speed, auto-scaled",
    build: (w, h) => {
      const s = Math.min(w, h) / 720;
      const g = makeElement("graph", w, h) as Extract<Element, { type: "graph" }>;
      const base = { ...g, x: w / 2 - 150 * s, w: Math.round(300 * s), h: Math.round(70 * s), auto_scale: true, seconds: 120 };
      return [
        text(w, h, -150, -92, "↓ {net.down:speed}", 22, { align: "left", weight: 600, color: "#3dd6ff" }),
        { ...base, id: newId("graph"), y: h / 2 - 80 * s, metric: "net.down" },
        text(w, h, -150, 8, "↑ {net.up:speed}", 22, { align: "left", weight: 600, color: "#ffc20e" }),
        { ...base, id: newId("graph"), y: h / 2 + 20 * s, metric: "net.up", color: "#ffc20e", fill: "#ffc20e40" },
      ];
    },
  },
  {
    id: "disk_io",
    label: "Disk activity",
    hint: "Read / write speed and root usage",
    build: (w, h) => {
      const s = Math.min(w, h) / 720;
      const bar = makeElement("bar", w, h) as Extract<Element, { type: "bar" }>;
      return [
        text(w, h, -150, -30, "R {disk.read:speed}", 22, { align: "left", weight: 600, color: "#3dd6ff" }),
        text(w, h, 150, -30, "W {disk.write:speed}", 22, { align: "right", weight: 600, color: "#ffc20e" }),
        text(w, h, -150, 12, "Disk {disk.pct:0}%", 20, { align: "left", weight: 500, color: "#9a9ab5" }),
        { ...bar, x: w / 2 - 150 * s, y: h / 2 + 22 * s, w: Math.round(300 * s), h: Math.round(10 * s), metric: "disk.pct", color: "#9a9ab5" },
      ];
    },
  },
  {
    id: "clock",
    label: "Clock & date",
    hint: "Time with the date underneath",
    build: (w, h) => [text(w, h, 0, 30, "{time:%H:%M}", 140, { weight: 700, font: "Bebas Neue", letter_spacing: 4 }), text(w, h, 0, 80, "{date:%A %d %B}", 26, { weight: 500, color: "#9a9ab5" })],
  },
  {
    id: "fans",
    label: "Fan & pump speed",
    hint: "CPU fan and pump RPM from the motherboard",
    build: (w, h) => [
      text(w, h, -150, -10, "FAN", 20, { align: "left", weight: 500, color: "#9a9ab5", letter_spacing: 3 }),
      text(w, h, -150, 36, "{fan.cpu:0} rpm", 36, { align: "left", weight: 700 }),
      text(w, h, 150, -10, "PUMP", 20, { align: "right", weight: 500, color: "#9a9ab5", letter_spacing: 3 }),
      text(w, h, 150, 36, "{fan.pump:0} rpm", 36, { align: "right", weight: 700 }),
    ],
  },
  {
    id: "clocks",
    label: "CPU & GPU clocks",
    hint: "Current frequencies and power",
    build: (w, h) => [
      text(w, h, -150, -10, "{cpu.freq:0} MHz", 30, { align: "left", weight: 700 }),
      text(w, h, -150, 22, "CPU · {cpu.power:0} W", 18, { align: "left", weight: 500, color: "#9a9ab5" }),
      text(w, h, 150, -10, "{gpu.freq:0} MHz", 30, { align: "right", weight: 700 }),
      text(w, h, 150, 22, "GPU · {gpu.power:0} W", 18, { align: "right", weight: 500, color: "#9a9ab5" }),
    ],
  },
  {
    id: "uptime",
    label: "Uptime & load",
    hint: "Time since boot and the 1-minute load",
    build: (w, h) => [text(w, h, 0, 0, "up {uptime}", 34, { weight: 600 }), text(w, h, 0, 36, "load {sys.load1:2} · {sys.procs:0} processes", 20, { weight: 500, color: "#9a9ab5" })],
  },
];
