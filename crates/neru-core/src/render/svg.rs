//! Theme elements → SVG markup.
//!
//! The same markup is shown by the editor in the browser and rasterised by
//! resvg for the screen, so only widely supported SVG 1.1 features are used.

use std::fmt::Write;

use super::theme::{Align, Background, Element, GraphStyle, Kind, Theme};
use super::values::{parse_color, ramp_color, Values};

pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

fn num(v: f32) -> String {
    let r = (v * 100.0).round() / 100.0;
    if r == 0.0 {
        return "0".into();
    }
    if r.fract() == 0.0 {
        format!("{}", r as i64)
    } else {
        format!("{r}")
    }
}

/// `attr="#rrggbb" attr-opacity="a"` for a colour that may carry alpha
/// (`#rrggbbaa`), so the markup stays SVG 1.1. `none` and unparsable
/// strings pass through unchanged.
fn paint(attr: &str, color: &str) -> String {
    match parse_color(color) {
        Some([r, g, b, a]) if a < 255 => format!(" {attr}=\"#{r:02x}{g:02x}{b:02x}\" {attr}-opacity=\"{}\"", num(a as f32 / 255.0)),
        _ => format!(" {attr}=\"{}\"", escape(color)),
    }
}

/// Catmull-Rom spline through `pts` as an SVG path (`M … C …`).
fn smooth_path(pts: &[(f32, f32)]) -> String {
    let mut d = String::new();
    if pts.is_empty() {
        return d;
    }
    let _ = write!(d, "M {} {}", num(pts[0].0), num(pts[0].1));
    if pts.len() == 1 {
        return d;
    }
    for i in 0..pts.len() - 1 {
        let p0 = pts[i.saturating_sub(1)];
        let p1 = pts[i];
        let p2 = pts[i + 1];
        let p3 = pts[(i + 2).min(pts.len() - 1)];
        let c1 = (p1.0 + (p2.0 - p0.0) / 6.0, p1.1 + (p2.1 - p0.1) / 6.0);
        let c2 = (p2.0 - (p3.0 - p1.0) / 6.0, p2.1 - (p3.1 - p1.1) / 6.0);
        let _ = write!(d, " C {} {} {} {} {} {}", num(c1.0), num(c1.1), num(c2.0), num(c2.1), num(p2.0), num(p2.1));
    }
    d
}

fn line_path(pts: &[(f32, f32)]) -> String {
    let mut d = String::new();
    for (i, p) in pts.iter().enumerate() {
        let _ = write!(d, "{}{} {}", if i == 0 { "M " } else { " L " }, num(p.0), num(p.1));
    }
    d
}

/// Fraction 0..1 of `value` between `min` and `max`.
fn frac(value: Option<f32>, min: f32, max: f32) -> f32 {
    let v = value.unwrap_or(min);
    if (max - min).abs() < f32::EPSILON {
        return 0.0;
    }
    ((v - min) / (max - min)).clamp(0.0, 1.0)
}

/// Point on a circle; angle in degrees clockwise from 12 o'clock.
fn polar(cx: f32, cy: f32, r: f32, deg: f32) -> (f32, f32) {
    let a = (deg - 90.0).to_radians();
    (cx + r * a.cos(), cy + r * a.sin())
}

/// SVG arc path from `start` sweeping `sweep` degrees (clockwise).
pub fn arc_path(cx: f32, cy: f32, r: f32, start: f32, sweep: f32) -> String {
    let sweep = sweep.clamp(-359.99, 359.99);
    if sweep.abs() < 0.01 {
        let (x, y) = polar(cx, cy, r, start);
        return format!("M {} {}", num(x), num(y));
    }
    let (x1, y1) = polar(cx, cy, r, start);
    let (x2, y2) = polar(cx, cy, r, start + sweep);
    let large = if sweep.abs() > 180.0 { 1 } else { 0 };
    let dir = if sweep >= 0.0 { 1 } else { 0 };
    format!(
        "M {} {} A {} {} 0 {large} {dir} {} {}",
        num(x1),
        num(y1),
        num(r),
        num(r),
        num(x2),
        num(y2)
    )
}

/// The background as SVG (`None` for media backgrounds, which are rasters).
pub fn background_svg(theme: &Theme) -> Option<String> {
    let (w, h) = (theme.width as f32, theme.height as f32);
    match &theme.background {
        Background::Color { color } => Some(format!(
            "<rect width=\"{}\" height=\"{}\" fill=\"{}\"/>",
            num(w),
            num(h),
            escape(color)
        )),
        Background::Gradient { from, to, angle } => {
            // Gradient direction from angle (0 = top→bottom, 90 = left→right).
            let a = angle.to_radians();
            let (dx, dy) = (a.sin(), -a.cos());
            let (x1, y1, x2, y2) = (0.5 - dx / 2.0, 0.5 - dy / 2.0, 0.5 + dx / 2.0, 0.5 + dy / 2.0);
            Some(format!(
                "<defs><linearGradient id=\"bg\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"><stop offset=\"0\" stop-color=\"{}\"/><stop offset=\"1\" stop-color=\"{}\"/></linearGradient></defs><rect width=\"{}\" height=\"{}\" fill=\"url(#bg)\"/>",
                num(x1), num(y1), num(x2), num(y2), escape(from), escape(to), num(w), num(h)
            ))
        }
        Background::Media { .. } => None,
    }
}

/// One element as SVG. Returns `None` for raster elements (images).
pub fn element_svg(el: &Element, values: &Values<'_>) -> Option<String> {
    if !el.visible || el.opacity <= 0.0 {
        return None;
    }
    let mut s = String::new();
    let transform = if el.rotation.abs() > 0.001 {
        format!(" transform=\"rotate({} {} {})\"", num(el.rotation), num(el.x), num(el.y))
    } else {
        String::new()
    };
    let opacity = if el.opacity < 1.0 { format!(" opacity=\"{}\"", num(el.opacity)) } else { String::new() };
    let _ = write!(s, "<g id=\"{}\"{transform}{opacity}>", escape(&el.id));

    match &el.kind {
        Kind::Text(t) => {
            let anchor = match t.align {
                Align::Left => "start",
                Align::Center => "middle",
                Align::Right => "end",
            };
            let color = match &t.ramp_metric {
                Some(m) => ramp_color(&t.ramp, values.get(m), &t.color),
                None => t.color.clone(),
            };
            let spacing = if t.letter_spacing.abs() > 0.001 {
                format!(" letter-spacing=\"{}\"", num(t.letter_spacing))
            } else {
                String::new()
            };
            let _ = write!(
                s,
                "<text x=\"{}\" y=\"{}\" font-family=\"{}\" font-size=\"{}\" font-weight=\"{}\" fill=\"{}\" text-anchor=\"{anchor}\"{spacing} xml:space=\"preserve\">{}</text>",
                num(el.x),
                num(el.y),
                escape(&t.font),
                num(t.size),
                t.weight,
                escape(&color),
                escape(&values.substitute(&t.text))
            );
        }
        Kind::Bar(b) => {
            let f = frac(values.get_smooth(&b.metric), b.min, b.max);
            let color = ramp_color(&b.ramp, values.get_smooth(&b.metric), &b.color);
            let r = b.radius.min(b.w / 2.0).min(b.h / 2.0);
            let _ = write!(
                s,
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\"{}/>",
                num(el.x),
                num(el.y),
                num(b.w),
                num(b.h),
                num(r),
                paint("fill", &b.track)
            );
            if f > 0.0 {
                // Clip the fill to the rounded track so partial fills keep the shape.
                let clip = format!("clip-{}", escape(&el.id));
                let (fx, fy, fw, fh) = if b.vertical {
                    (el.x, el.y + b.h * (1.0 - f), b.w, b.h * f)
                } else {
                    (el.x, el.y, b.w * f, b.h)
                };
                let _ = write!(
                    s,
                    "<clipPath id=\"{clip}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\"/></clipPath><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{} clip-path=\"url(#{clip})\"/>",
                    num(el.x), num(el.y), num(b.w), num(b.h), num(r),
                    num(fx), num(fy), num(fw), num(fh), paint("fill", &color)
                );
            }
        }
        Kind::Gauge(g) => {
            let v = values.get_smooth(&g.metric);
            let f = frac(v, g.min, g.max);
            let color = ramp_color(&g.ramp, v, &g.color);
            let cap = if g.round_caps { "round" } else { "butt" };
            let r = g.r.max(1.0);
            let _ = write!(
                s,
                "<path d=\"{}\" fill=\"none\"{} stroke-width=\"{}\" stroke-linecap=\"{cap}\"/>",
                arc_path(el.x, el.y, r, g.start, g.sweep),
                paint("stroke", &g.track),
                num(g.thickness)
            );
            if f > 0.0 {
                let _ = write!(
                    s,
                    "<path d=\"{}\" fill=\"none\"{} stroke-width=\"{}\" stroke-linecap=\"{cap}\"/>",
                    arc_path(el.x, el.y, r, g.start, g.sweep * f),
                    paint("stroke", &color),
                    num(g.thickness)
                );
            }
        }
        Kind::Graph(g) => {
            let (w, h) = (g.w.max(1.0), g.h.max(1.0));
            let r = g.radius.min(w / 2.0).min(h / 2.0).max(0.0);
            let clip = format!("clip-{}", escape(&el.id));
            let _ = write!(
                s,
                "<clipPath id=\"{clip}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\"/></clipPath>",
                num(el.x), num(el.y), num(w), num(h), num(r)
            );
            if g.track != "none" {
                let _ = write!(
                    s,
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\"{}/>",
                    num(el.x), num(el.y), num(w), num(h), num(r), paint("fill", &g.track)
                );
            }
            let samples = values.window(&g.metric);
            let current = values.get_smooth(&g.metric).or_else(|| samples.last().map(|p| p.1));
            let color = ramp_color(&g.ramp, current, &g.color);
            let seconds = g.seconds.max(1.0);
            // Scale.
            let (lo, hi) = if g.auto_scale {
                let vals = samples.iter().map(|p| p.1).chain(current);
                let (mut lo, mut hi) = vals.fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)));
                if lo == f32::MAX {
                    (0.0, 1.0)
                } else {
                    if hi - lo < 1e-3 {
                        hi = lo + 1.0;
                    }
                    let pad = (hi - lo) * 0.08;
                    lo = if lo >= 0.0 && lo - pad < 0.0 { 0.0 } else { lo - pad };
                    (lo, hi + pad)
                }
            } else {
                (g.min, g.max)
            };
            let span = if (hi - lo).abs() < 1e-6 { 1.0 } else { hi - lo };
            let inset = g.stroke_width / 2.0;
            let yof = |v: f32| el.y + inset + (h - 2.0 * inset) * (1.0 - ((v - lo) / span).clamp(0.0, 1.0));
            let xof = |age: f32| el.x + w * (1.0 - (age / seconds).clamp(0.0, 1.0));
            // Points oldest → newest, then the current value pinned to the right edge.
            let mut pts: Vec<(f32, f32)> = samples.iter().map(|&(age, v)| (xof(age), yof(v))).collect();
            if let Some(c) = current {
                pts.push((el.x + w, yof(c)));
            }
            if pts.len() == 1 {
                pts.insert(0, (el.x, pts[0].1));
            }
            let _ = write!(s, "<g clip-path=\"url(#{clip})\">");
            if pts.len() >= 2 {
                match g.style {
                    GraphStyle::Bars => {
                        let n = pts.len() as f32;
                        let bw = (w / n).max(1.0);
                        let gap = if bw > 3.0 { 1.0 } else { 0.0 };
                        let bottom = el.y + h;
                        for p in &pts {
                            let bh = (bottom - p.1).max(0.0);
                            if bh <= 0.0 {
                                continue;
                            }
                            let _ = write!(
                                s,
                                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{}/>",
                                num(p.0 - bw + gap / 2.0), num(p.1), num((bw - gap).max(0.5)), num(bh), paint("fill", &color)
                            );
                        }
                    }
                    GraphStyle::Line | GraphStyle::Area => {
                        let d = if g.smooth { smooth_path(&pts) } else { line_path(&pts) };
                        if g.style == GraphStyle::Area && g.fill != "none" {
                            let bottom = el.y + h;
                            let (first, last) = (pts[0], pts[pts.len() - 1]);
                            let _ = write!(
                                s,
                                "<path d=\"{d} L {} {} L {} {} Z\"{} stroke=\"none\"/>",
                                num(last.0), num(bottom), num(first.0), num(bottom), paint("fill", &g.fill)
                            );
                        }
                        let _ = write!(
                            s,
                            "<path d=\"{d}\" fill=\"none\"{} stroke-width=\"{}\" stroke-linejoin=\"round\" stroke-linecap=\"round\"/>",
                            paint("stroke", &color), num(g.stroke_width)
                        );
                    }
                }
            }
            s.push_str("</g>");
        }
        Kind::Rect(r) => {
            let _ = write!(
                s,
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\"{}{} stroke-width=\"{}\"/>",
                num(el.x),
                num(el.y),
                num(r.w),
                num(r.h),
                num(r.radius),
                paint("fill", &r.fill),
                paint("stroke", &r.stroke),
                num(r.stroke_width)
            );
        }
        Kind::Ellipse(e) => {
            let _ = write!(
                s,
                "<ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\"{}{} stroke-width=\"{}\"/>",
                num(el.x + e.rx),
                num(el.y + e.ry),
                num(e.rx),
                num(e.ry),
                paint("fill", &e.fill),
                paint("stroke", &e.stroke),
                num(e.stroke_width)
            );
        }
        Kind::Line(l) => {
            let cap = if l.round_caps { "round" } else { "butt" };
            let _ = write!(
                s,
                "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"{}\" stroke-linecap=\"{cap}\"/>",
                num(el.x),
                num(el.y),
                num(el.x + l.w),
                num(el.y + l.h),
                escape(&l.stroke),
                num(l.stroke_width)
            );
        }
        Kind::Image(_) => return None,
    }
    s.push_str("</g>");
    Some(s)
}

/// Wrap a body in the document element.
pub fn document(theme: &Theme, body: &str) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">{body}</svg>",
        w = theme.width,
        h = theme.height
    )
}

/// The whole theme as one SVG for the editor preview (images are referenced
/// by `href`, which the editor rewrites to something it can load).
pub fn full_document(theme: &Theme, values: &Values<'_>) -> String {
    let mut body = background_svg(theme).unwrap_or_default();
    for el in &theme.elements {
        if let Kind::Image(img) = &el.kind {
            if el.visible {
                let _ = write!(
                    body,
                    "<image id=\"{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" href=\"{}\" preserveAspectRatio=\"{}\" opacity=\"{}\"/>",
                    escape(&el.id),
                    num(el.x),
                    num(el.y),
                    num(img.w),
                    num(img.h),
                    escape(&img.path),
                    match img.fit {
                        super::theme::Fit::Cover => "xMidYMid slice",
                        super::theme::Fit::Contain => "xMidYMid meet",
                        super::theme::Fit::Stretch => "none",
                    },
                    num(el.opacity)
                );
            }
            continue;
        }
        if let Some(s) = element_svg(el, values) {
            body.push_str(&s);
        }
    }
    document(theme, &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arc_endpoints() {
        // Quarter turn from 12 o'clock on a unit circle ends at 3 o'clock.
        let p = arc_path(0.0, 0.0, 10.0, 0.0, 90.0);
        assert!(p.starts_with("M 0 -10 A 10 10 0 0 1 10 0"), "{p}");
        // > 180° sets the large-arc flag.
        assert!(arc_path(0.0, 0.0, 10.0, -135.0, 270.0).contains(" 1 1 "));
    }

    #[test]
    fn escapes_text() {
        assert_eq!(escape("a<b&\"c\""), "a&lt;b&amp;&quot;c&quot;");
    }

    #[test]
    fn alpha_colours_become_opacity() {
        assert_eq!(paint("fill", "#3dd6ff40"), " fill=\"#3dd6ff\" fill-opacity=\"0.25\"");
        assert_eq!(paint("fill", "#3dd6ff"), " fill=\"#3dd6ff\"");
        assert_eq!(paint("stroke", "none"), " stroke=\"none\"");
    }

    #[test]
    fn graph_draws_history() {
        use super::super::theme::{Element, Graph, Kind};
        use crate::metrics::Snapshot;
        use std::collections::HashMap;
        let mut snap = Snapshot::default();
        snap.cpu.usage_pct = Some(50.0);
        let smoothed = HashMap::new();
        let mut windows: super::super::values::Windows = HashMap::new();
        windows.insert("cpu.usage".into(), vec![(30.0, 10.0), (20.0, 90.0), (10.0, 40.0)]);
        let values = Values { snapshot: &snap, fahrenheit: false, smoothed: &smoothed, now: chrono::Local::now(), windows: Some(&windows) };
        let el = Element { id: "g1".into(), x: 10.0, y: 20.0, kind: Kind::Graph(Graph { w: 200.0, h: 100.0, seconds: 60.0, ..Default::default() }), ..Default::default() };
        let svg = element_svg(&el, &values).unwrap();
        assert!(svg.contains("clip-g1"), "{svg}");
        assert!(svg.contains(" C "), "smooth path expected: {svg}");
        assert!(svg.contains("fill-opacity"), "{svg}");
        // The newest point is pinned at the right edge (x = 210).
        assert!(svg.contains("210 "), "{svg}");
        let bars = Element { kind: Kind::Graph(Graph { style: GraphStyle::Bars, smooth: false, ..Default::default() }), ..el.clone() };
        let svg = element_svg(&bars, &values).unwrap();
        assert!(svg.matches("<rect").count() >= 4, "{svg}");
        // Without history: a flat line at the current value.
        let values = Values { snapshot: &snap, fahrenheit: false, smoothed: &smoothed, now: chrono::Local::now(), windows: None };
        let svg = element_svg(&el, &values).unwrap();
        assert!(svg.contains("<path"), "{svg}");
    }
}
