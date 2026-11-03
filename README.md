# Neru Screen Control

> Cooler LCD screens, PC RGB lighting and fan curves — one Linux app, no vendor software.

![Rust](https://img.shields.io/badge/Rust-2021-orange?logo=rust&logoColor=white)
![Tauri](https://img.shields.io/badge/App-Tauri%202-24c8db?logo=tauri&logoColor=white)
![React](https://img.shields.io/badge/UI-React%20%2B%20TypeScript-61dafb?logo=react&logoColor=white)
![License](https://img.shields.io/badge/License-GPL--3.0-blue)
![Platform](https://img.shields.io/badge/Platform-Linux-orange?logo=linux&logoColor=white)

Neru started as a Linux driver for the little USB LCD on Thermalright coolers.
Version 2 is a rewrite in Rust: it drives Thermaltake and Thermalright cooler
screens with live system metrics and your own themes, lights up the PC with its
own RGB drivers (OpenRGB optional for the rest), and runs fan curves — from a
tray icon, on any distro.

![Dashboard](docs/screenshots/dashboard.png)

| Theme editor | RGB | Fans |
|---|---|---|
| ![](docs/screenshots/editor.png) | ![](docs/screenshots/rgb.png) | ![](docs/screenshots/fans.png) |

## Features

* 🖥 **Live theme editor** — drag, resize and layer text, bars, arc gauges,
  **history graphs**, shapes, images, animated GIFs and video; ready-made
  widgets (CPU/GPU temperature, usage bars, network ↓↑ graphs, clock, fans…).
  Every change shows on the screen as you make it.
* 📊 **Metrics** — CPU temperature / load / per-core peak / clock / power, GPU
  temperature / hotspot / load / clocks / power / fan / VRAM, RAM and swap,
  network up/down, disk throughput / usage / temperature, every hwmon fan and
  pump RPM, load average, uptime — with an hour of history for graphs. No agents, no root.
* 🎨 **RGB** — Neru's own drivers for ASUS Aura USB motherboards (and their ARGB
  headers), ENE/ASUS RGB RAM, ASUS graphics cards, ZOTAC RTX 50 cards, Corsair
  Lighting Node / Commander Pro, NZXT Hue 2 / Smart Device V2 / Kraken, ASRock
  Polychrome USB; anything else through [OpenRGB](https://openrgb.org) if it is
  installed. Software effects (static, gradient, breathing, rainbow, wave, chase,
  twinkle, candle, strobe, **temperature-reactive**…) or the devices' own modes.
* 🌀 **Fan curves** — any hwmon PWM channel from any temperature, applied by a
  small root helper that keeps working with the app closed.
* ⚙ **Settings** — autostart on login, run in the tray, hardware video decoding
  (NVIDIA / AMD / Intel), Fahrenheit, screen calibration, **Check for updates**
  (Settings → About; a newer release is also announced once in the sidebar).
* ❓ **Built-in help** — a how-to page inside the app.
* 🧰 **`neru` CLI** — everything the app does with hardware, scriptable.

## Install

One command, any distro (x86_64):

```sh
curl -fsSL https://raw.githubusercontent.com/Huzzama/Neru-screen-control/main/install.sh | sh
```

It downloads the latest AppImage into `~/.local/share/neru-screen-control`, adds
`neru-screen-control` to `~/.local/bin` and to your app menu, and installs the
USB / I²C access rules (asks for your password). Run it again to update; remove
with `… | sh -s -- --uninstall`.

| File | For |
|---|---|
| `neru-screen-control_x.y.z_amd64.AppImage` | Any Linux distro ([Releases](https://github.com/Huzzama/Neru-screen-control/releases)) |
| `neru-screen-control_x.y.z_amd64.deb` | Ubuntu, Linux Mint, Debian, Pop!_OS (`npm run build` makes it) |
| `packaging/arch/PKGBUILD` | Arch, CachyOS, EndeavourOS, Manjaro (`cd packaging/arch && makepkg -si`) |

After installing, unplug and replug the cooler's USB cable once. Optional:
OpenRGB (devices Neru has no driver for yet), `ffmpeg` (video backgrounds),
**Fans → Install helper** for fan curves.

## Supported displays

| Screen | USB ID | Resolution | Status |
|---|---|---|---|
| Thermaltake TH360 / TH420 **V3 Ultra ARGB Sync** (pump-head LCD, 3.95") | `264a:2348` | 720×720 | verified |
| Thermaltake LCD Panel Kit 6" (View 600 TG) | `264a:2347` | 1110×540 | same protocol, untested |
| Thermalright Frozen Warframe class (ChiZhu USBDISPLAY), 2.4" | `87ad:70db` | 320×320 | verified (the original Neru 1.x hardware) |
| Thermalright Frozen Warframe 2.4", Core Matrix 2.0" | `87ad:70db` | 320×240 | variant — pick it in Settings |
| Thermalright Peerless / Guard / Hyper / Elite Vision 3.95", Core Vision 2.1", Frozen Guardian / Frozen Vision 2.88" | `87ad:70db` | 480×480 | variant — pick it in Settings |
| Thermalright Mjolnir Vision, Stream Vision 3.5" | `87ad:70db` | 640×480 | variant — pick it in Settings |
| Thermalright Trofeo Vision 6.98" | `87ad:70db` | 1280×480 | variant — pick it in Settings |
| Thermalright Leviathan / Rainbow / Wonder Vision 6.67" | `87ad:70db` | 2400×1080 | variant — pick it in Settings |

All ChiZhu-based Thermalright screens share one USB id and the panel size
cannot be read from the device, so Neru lists the resolutions as variants:
320×320 is used by default, the others are chosen in **Settings → Use this
screen**. Thermalright models on other controllers (HID `0416:5302`, SCSI
`87cd:70db` / `0402:3922` / `0416:5406`, LY `0416:5408`) are recognised and
reported in the app but not driven yet — the open-source
[TRCC Linux](https://github.com/Lexonight1/thermalright-trcc-linux) project
covers them. Protocol notes and how to add a screen: [`docs/PROTOCOLS.md`](docs/PROTOCOLS.md).

## RGB support

| Driver | Devices | Bus |
|---|---|---|
| ASUS Aura USB | ASUS motherboards with the Aura USB controller (`0b05:18f3/1939/19af/1aa6/1bed`): onboard LEDs + every 5 V ARGB header | HID |
| ENE SMBus | ENE / ASUS Aura RGB RAM (Trident Z RGB, T-Force Delta, Geil Super Luce, ASUS DIMMs…), SMBus Aura motherboards, ASUS ROG Strix / TUF / KO / Dual graphics cards (173 models) | I²C |
| ZOTAC Blackwell | ZOTAC GeForce RTX 50 (AMP Extreme INFINITY, SOLID OC…) | I²C |
| Corsair Lighting Node | Lighting Node Pro / Core, Commander Pro, LS100, LT100, 1000D Obsidian, SPEC-OMEGA RGB | HID |
| NZXT Hue 2 | Hue 2 / Hue 2 Ambient, Smart Device V2, RGB & Fan Controller (all revisions), RGB Controller, Kraken X3 / Z3 ring + logo, Kraken 2024 Elite | HID |
| ASRock Polychrome USB | ASRock boards with the USB Polychrome controller, DeskMini LED strip | HID |
| OpenRGB (optional) | Everything else OpenRGB supports, through its SDK server | — |

The ASUS Aura USB and ZOTAC drivers were verified on real hardware; the others
are faithful ports of OpenRGB's drivers that still need testers — please open
an issue with the output of `neru rgb native` either way. [`docs/RGB.md`](docs/RGB.md)

## GPU support

| Vendor | Metrics source | Needs |
|---|---|---|
| NVIDIA | NVML (`libnvidia-ml.so`, loaded at runtime) — temperature, load, clocks, power, fan, VRAM | the proprietary or `nvidia-open` driver |
| AMD | `amdgpu` sysfs + hwmon — temperature, hotspot, load, clocks, power, fan, VRAM | nothing (in-kernel driver) |
| Intel | — | not yet |

Both are detected automatically at startup; the first discrete GPU wins. GPU
RGB (ZOTAC, ASUS cards) rides on the card's own I²C adapters, which need the
`i2c-dev` module (the installer loads it).

## Build from source

Requirements: Rust 1.80+ (`rustup`), Node.js 20+, and the Tauri Linux libraries.

```bash
# Arch / CachyOS
sudo pacman -S --needed rustup nodejs npm base-devel pkgconf libusb webkit2gtk-4.1 gtk3 libayatana-appindicator librsvg
rustup default stable
# Debian / Ubuntu
sudo apt install build-essential curl pkg-config libusb-1.0-0-dev libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev nodejs npm
curl https://sh.rustup.rs -sSf | sh

git clone https://github.com/Huzzama/Neru-screen-control.git
cd Neru-screen-control
npm install                 # UI dependencies + the Tauri CLI

npm run dev                 # the app, with hot reload of the UI
npm run build               # release bundles → target/release/bundle/{deb,appimage}/
cargo build --release -p neru-cli && ./target/release/neru --help   # the CLI only
```

Config, themes and media live in `~/.config/neru-screen-control/`.
`neru-screen-control --background` starts hidden in the tray (what the
autostart entry uses).

## The `neru` command line

```bash
neru devices                  # screens found and whether you can open them
neru probe                    # handshake: firmware, brightness, rotation, idle timeout
neru test                     # test pattern (red top-left, green top-right, blue bottom-left, yellow bottom-right)
neru show wallpaper.png       # any image; GIFs animate
neru brightness 60 · neru rotate 90
neru metrics --watch          # CPU / GPU / RAM / network / disk / fan readings
neru theme ls                 # bundled themes
neru theme render t.json -o frame.png --fake   # one frame, no screen needed
neru theme run t.json         # live on the screen
neru fonts
neru rgb native               # Neru's own RGB drivers: devices and zones
neru rgb native --color ff0000 --leds 24   # paint everything (sanity check)
neru rgb i2c                  # I2C adapters + PCI ids (GPU / RAM RGB lives here)
neru rgb devices              # what the OpenRGB server reports (optional)
neru rgb set 0 ff0000 --zone 1
neru udev --install
```

Set `NERU_LOG=debug` for verbose logs (also for the app).

## Themes

JSON files you can write by hand (format: [`docs/THEMES.md`](docs/THEMES.md)) or
build in the editor. Placeholders such as `{cpu.temp:0}°`, `{gpu.usage}%`,
`{net.down:speed}`, `{fan.cpu:0} rpm`, `{time:%H:%M}`, `{uptime}` pull live
values; graphs draw the last minute (or hour) of any metric; ramps colour
anything by temperature. Bundled fonts: Inter, JetBrains Mono, Bebas Neue,
Rajdhani (OFL) — plus every font on your system.

## USB protocol

**Thermalright / ChiZhu Tech USBDISPLAY — VID:PID `87ad:70db`.** Raw USB bulk
transfers (libusb), found with Wireshark + usbmon on the Windows driver:

1. control transfer `0x01 0x00` (start)
2. bulk OUT `00 00 00 00` (trigger)
3. bulk OUT a 208-byte frame header followed by the pixels, in 512-byte chunks
4. control transfer `0x09 0x00` (commit)

The frame header starts `12 34 56 78 | 03 00 00 00 | width | width | height`
(little-endian 32-bit fields) with `02 00 00 00` at 0x38, the pixel byte count
at 0x3C and `00 20 00 00` at 0xC4 — so the same controller drives every panel
size by changing the header. Pixels are **RGB565 big-endian**, row-major, no
transposition. The very first capture of the Windows driver showed the bytes
`1b 00 10 90 d4 43 0e b2 ff ff 00 00` at the start of the stream; the 208-byte
header above is what a frame actually needs. If frames look wrong on an
untested model, capture USB traffic from the Windows driver and compare the
first bulk OUT packet — the pixel format (`rgb565_be/le`, `bgr565`, `rgb888`…),
rotation and row/column offset can be overridden in Settings → Thermalright
panel calibration without touching code.

**Thermaltake "BY" protocol — VID `264a`.** USB HID, 1024-byte reports.
Commands are HTTP-like text framed as `5A <len> <payload> <sum8> 5A` (with
`5A → 5B 01` / `5B → 5B 02` escaping): `POST conn 1` returns a JSON status
(firmware, brightness, rotation, idle timeout), `POST brightness/rotate …` set
them, and frames are JPEGs sent in 1000-byte slices (`5C 03 FD counter total idx 01`).
The screen falls back to its own content after 5 s without frames, so Neru
resends every 2 s when idle. Full notes: [`docs/PROTOCOLS.md`](docs/PROTOCOLS.md).

## Project layout

```
crates/neru-core   drivers (screen/), metrics/, render/ (themes), engine, rgb/ (+ native drivers), fans/, i2c, config
crates/neru-cli    the `neru` tool
crates/neru-fand   the root fan helper (systemd service)
src-tauri/         the desktop app (Tauri 2): commands, tray, background mode
ui/                the app's UI (React + TypeScript + Vite)
assets/            bundled fonts (OFL) and themes
docs/              protocols, theme format, RGB and fan guides
packaging/         udev rule, modules-load, desktop entry, systemd unit, deb post-install, AUR PKGBUILD
.github/workflows  CI and the AppImage release build
```

## Contributing

* A Thermalright model not listed above, or one of the recognised-but-undriven
  families? Open an issue with `lsusb` and, if you can, a Wireshark capture of
  the Windows driver sending a frame — that is all that is needed to add support.
* An RGB device Neru lists as OpenRGB-only? Most OpenRGB controllers port in an
  afternoon — see *Adding a driver* in `docs/RGB.md`.
* `cargo test --workspace` and `npm --prefix ui run build` must pass (CI runs both).

## Credits

* The Thermaltake "BY" protocol was recovered publicly by
  [deniswsrosa/thermaltake-6inch-lcd-linux](https://github.com/deniswsrosa/thermaltake-6inch-lcd-linux)
  and [Telinhas](https://github.com/ninjairocode/telinhas-project); Neru
  re-implements it from their notes.
* Native RGB drivers are ported from [OpenRGB](https://openrgb.org)
  (GPL-2.0-or-later) — thanks to its contributors for years of reverse
  engineering. OpenRGB itself remains an optional backend over its network SDK.
* Fonts: Inter, JetBrains Mono, Bebas Neue, Rajdhani under the SIL Open Font License.

## License

GPL-3.0-or-later — see [LICENSE](LICENSE). Neru 2.x includes code ported from
OpenRGB (GPL-2.0-or-later). Neru 1.x (Python, MIT) lives on in the git history
and the 1.x releases.
