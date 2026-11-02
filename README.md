# Neru Screen Control

> Linux control for cooler LCD screens, PC RGB lighting and fans — Thermaltake and Thermalright screens, OpenRGB-backed lighting, one app.

![Rust](https://img.shields.io/badge/Rust-2021-orange?logo=rust&logoColor=white)
![Tauri](https://img.shields.io/badge/App-Tauri%202-24c8db?logo=tauri&logoColor=white)
![License](https://img.shields.io/badge/License-MIT-yellow)
![Platform](https://img.shields.io/badge/Platform-Linux-orange?logo=linux&logoColor=white)

> **v2 is a rewrite in Rust + Tauri.** The Python/PySide6 app (v1.x) is in the
> git history and in the [1.x releases](https://github.com/Huzzama/Neru-screen-control/releases).
> v2 ships the desktop app (screen themes editor, live preview, tray, RGB through
> OpenRGB), the core library and the `neru` command-line tool. Fan control is next.

## The app

* **Dashboard** — screen status, brightness and rotation, live preview, CPU / GPU / RAM tiles.
* **Screen** — theme gallery and a live editor: text with metric placeholders, bars,
  arc gauges, shapes, images, animated GIFs and video. Every edit is pushed to the
  screen as you make it.
* **RGB** — every device OpenRGB knows (motherboard ARGB headers → cooler fans and
  pump, RAM, GPU…): per-zone software effects (static, breathing, rainbow, colour
  cycle, temperature-reactive) or device-native modes, OpenRGB profiles. See
  [`docs/RGB.md`](docs/RGB.md).
* **Settings** — start on login (tray), frame rate, JPEG quality, Fahrenheit, udev
  rules installer, Thermalright calibration.
* Runs in the tray; `neru-screen-control --background` starts hidden (what the
  autostart entry uses).

## Supported screens

| Screen | USB ID | Resolution | Status |
|---|---|---|---|
| Thermaltake TH360 / TH420 **V3 Ultra ARGB Sync** (pump-head LCD) | `264a:2348` | 720×720 | verified |
| Thermaltake LCD Panel Kit 6" (View 600 TG) | `264a:2347` | 1110×540 | same protocol, untested |
| Thermalright coolers with the ChiZhu USBDISPLAY controller (Frozen Warframe class) | `87ad:70db` | 320×320 | verified (v1 driver, ported) |

Protocol notes: [`docs/PROTOCOLS.md`](docs/PROTOCOLS.md).

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
npm run build               # release build → target/release/bundle/{deb,appimage}/
cargo build --release -p neru-cli && ./target/release/neru --help   # the CLI only
```

`ffmpeg` is optional (video backgrounds). Config, themes and media live in
`~/.config/neru-screen-control/`.

## Device access (once)

```bash
./target/release/neru udev --install     # asks for your password, writes /etc/udev/rules.d/70-neru-screen-control.rules
```

Then unplug and replug the screen's USB cable (or reboot).

## Test your screen

```bash
neru devices            # what was found and whether you can open it
neru probe              # handshake: firmware, brightness, rotation, idle timeout
neru test               # test pattern (red=top-left, green=top-right, blue=bottom-left, yellow=bottom-right)
neru show wallpaper.png # any image; GIFs animate
neru brightness 60      # Thermaltake screens
neru rotate 90
neru metrics --watch    # CPU / GPU / RAM readings the themes can use
```

Set `NERU_LOG=debug` to see every command and reply.

## Themes

Themes are JSON (text, bars, arc gauges, shapes, images, GIFs, video) rendered
by the app and sent to the screen. Format: [`docs/THEMES.md`](docs/THEMES.md).

```bash
neru theme ls                                        # bundled themes (assets/themes)
neru theme render assets/themes/neru-720.json -o frame.png --fake   # preview without a screen
neru theme run assets/themes/neru-720.json           # live metrics on the screen, Ctrl-C to stop
neru fonts                                           # font families themes can use
neru rgb devices                                     # what the OpenRGB server reports
neru rgb set 0 ff0000 --zone 1                       # paint a zone (sanity check)
```

Video backgrounds need `ffmpeg`. RGB needs OpenRGB with its SDK server (`docs/RGB.md`).

## Project layout

```
crates/neru-core   drivers (screen/), metrics/, render/ (themes), engine (render loop), config, theme store
crates/neru-cli    the `neru` tool
src-tauri/         the desktop app (Tauri 2): commands, tray, background mode
ui/                the app's UI (React + TypeScript + Vite)
assets/            bundled fonts (OFL) and themes
docs/              protocol and theme format notes
packaging/         udev rules, desktop entry, distro packaging
```

## Contributing

Have a Thermaltake or Thermalright screen that is not listed? Open an issue with
`lsusb` output and, if you can, a USB capture of the official software sending one
frame. See *Adding a screen* in `docs/PROTOCOLS.md`.

## License

MIT
