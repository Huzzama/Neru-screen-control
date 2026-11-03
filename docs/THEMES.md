# Theme format

A theme is a JSON file (`~/.config/neru-screen-control/themes/*.json`; the
bundled ones live in `assets/themes/`). The editor in the app writes these
files; you can also edit them by hand and run them with `neru theme run`.

```json
{
  "name": "Neru 720",
  "width": 720,
  "height": 720,
  "background": { "kind": "gradient", "from": "#0b0b14", "to": "#1a1230", "angle": 160 },
  "smoothing": 0.5,
  "elements": [ … ]
}
```

* Coordinates are screen pixels, origin top-left.
* A theme designed for another screen size is scaled proportionally when shown.
* `smoothing` (0–0.95) eases bar and gauge movement between metric updates.

## Background

| `kind` | Fields |
|---|---|
| `color` | `color` |
| `gradient` | `from`, `to`, `angle` (0 = top→bottom, 90 = left→right) |
| `media` | `path` (image / GIF / video), `fit` (`cover`, `contain`, `stretch`), `opacity` |

Media paths are absolute, or names inside `~/.config/neru-screen-control/media/`.
Video needs `ffmpeg` installed (frames are decoded at 15 fps and looped).

## Elements

Common fields: `id`, `type`, `x`, `y`, `rotation` (degrees), `opacity`, `visible`, `locked`.
Elements are drawn in order (last on top).

### `text`

`x`/`y` is the baseline start (centre / end with `align`).

| Field | Default | |
|---|---|---|
| `text` | | placeholders below |
| `font` | `Inter` | any family from `neru fonts` |
| `size` | 48 | px |
| `weight` | 600 | 100–900 |
| `color` | `#ffffff` | |
| `align` | `left` | `left`, `center`, `right` |
| `letter_spacing` | 0 | px |
| `ramp_metric`, `ramp` | | colour the text by a metric (see ramps) |

Placeholders inside `text`:

| | |
|---|---|
| `{cpu.temp}` | value, no decimals |
| `{cpu.temp:1}` | one decimal |
| `{net.down:speed}` | bytes/s with an automatic unit: `12.3 MB/s` (`speed.0` = no decimals) |
| `{net.down:bits}` | bytes/s as bits: `98.4 Mbps` |
| `{mem.used:size}` | MB with an automatic unit: `12.3 GB` |
| `{disk.used:bytes}` | bytes with an automatic unit (`KB`, `MB`, `GB`, `TB`) |
| `{cpu.usage:pct}` | `62%` |
| `{sys.uptime:uptime}` or `{uptime}` | `3d 04:12` |
| `{time:%H:%M}` | current time, strftime format |
| `{date:%A %d %B}` | current date |
| `{cpu.name}`, `{gpu.name}`, `{net.iface}` | hardware names / busiest interface |

Metric keys (the app lists them under *Insert a metric…*; `neru metrics` shows live values):

| Group | Keys |
|---|---|
| CPU | `cpu.temp` °C, `cpu.usage` %, `cpu.usage_max` (busiest core) %, `cpu.freq` MHz, `cpu.power` W, `cpu.cores` |
| GPU | `gpu.temp`, `gpu.hotspot` (AMD junction), `gpu.usage`, `gpu.freq`, `gpu.memfreq`, `gpu.power`, `gpu.fan` %, `gpu.fan_rpm` (AMD), `gpu.vram` MB, `gpu.vram_total`, `gpu.vram_pct` |
| RAM | `mem.used` MB, `mem.avail`, `mem.total`, `mem.pct`, `mem.swap_used`, `mem.swap_pct` |
| Network | `net.down`, `net.up` (bytes/s over all physical interfaces), `net.down_mbps`, `net.up_mbps`, `net.total_down`, `net.total_up` (MB since boot) |
| Disk | `disk.read`, `disk.write` (bytes/s, all physical drives), `disk.pct`, `disk.used` GB, `disk.total` (root filesystem), `disk.temp` (NVMe / drivetemp) |
| Fans | `fan.1` … `fan.N` RPM (every hwmon fan, in `neru metrics` order), `fan.cpu`, `fan.pump` (guessed from labels), `fan.max` |
| System | `sys.load1`, `sys.load5`, `sys.load15`, `sys.procs`, `sys.uptime` s |

Temperatures are °C (°F when the app is set to Fahrenheit). A missing value prints `--`.

### `bar`

`w`, `h`, `metric`, `min`, `max`, `color`, `track`, `radius`, `vertical`, `ramp`.

### `gauge`

Arc centred on `x`/`y`: `r`, `thickness`, `metric`, `min`, `max`,
`start` (degrees clockwise from 12 o'clock, e.g. `-135`), `sweep` (e.g. `270`),
`color`, `track`, `round_caps`, `ramp`.

### `graph`

The last `seconds` of a metric in a `w`×`h` box at `x`/`y`; time runs left to
right with the newest value pinned to the right edge. Neru keeps an hour of
history for every metric, sampled at the sensor interval.

| Field | Default | |
|---|---|---|
| `metric` | `cpu.usage` | |
| `seconds` | 60 | window length |
| `min`, `max` | 0, 100 | vertical scale |
| `auto_scale` | false | ignore `min`/`max`, fit the samples (use for speeds) |
| `style` | `area` | `line`, `area` (filled), `bars` |
| `color` | `#3dd6ff` | line / bars |
| `fill` | `#3dd6ff40` | area under the line; `#rrggbbaa` alpha is honoured, `none` disables |
| `track` | `#15152280` | box background, `none` for transparent |
| `stroke_width` | 3 | |
| `radius` | 12 | box corner radius |
| `smooth` | true | spline instead of straight segments |
| `ramp` | | colour by the current value |

Colours anywhere in a theme may carry alpha (`#rrggbbaa`).

### `rect`, `ellipse`, `line`

`rect`: `w`, `h`, `fill`, `stroke`, `stroke_width`, `radius`.
`ellipse`: `rx`, `ry`, `fill`, `stroke`, `stroke_width` (top-left at `x`/`y`).
`line`: from (`x`, `y`) to (`x + w`, `y + h`); `stroke`, `stroke_width`, `round_caps`.

### `image`

`w`, `h`, `path`, `fit`. PNG / JPEG / WebP / BMP, animated GIF, or video.

## Ramps

A `ramp` is a list of `{ "at": value, "color": "#rrggbb" }` stops; the colour is
interpolated by the metric's current value:

```json
"ramp": [ { "at": 35, "color": "#3dd6ff" }, { "at": 65, "color": "#ffc20e" }, { "at": 90, "color": "#ff3b5c" } ]
```

## Fonts

Bundled (always available): Inter (400/600/700), JetBrains Mono (400/700),
Bebas Neue, Rajdhani (400/500/700). System fonts are available too; a theme that
uses a font the machine lacks falls back to Inter.

## Tools

```bash
neru theme ls                                  # bundled themes
neru theme render my.json -o frame.png --fake  # one frame, made-up metrics (no screen needed)
neru theme render my.json -o frame.png --bench 50
neru theme run my.json --fps 10                # live on the screen
```
