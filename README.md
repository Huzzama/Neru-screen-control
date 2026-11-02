# Neru Screen Control

> Linux control for cooler LCD screens, PC RGB lighting and fans — Thermaltake and Thermalright screens, OpenRGB-backed lighting, one app.

![Rust](https://img.shields.io/badge/Rust-2021-orange?logo=rust&logoColor=white)
![Tauri](https://img.shields.io/badge/App-Tauri%202-24c8db?logo=tauri&logoColor=white)
![License](https://img.shields.io/badge/License-MIT-yellow)
![Platform](https://img.shields.io/badge/Platform-Linux-orange?logo=linux&logoColor=white)

> **v2 is being rewritten in Rust + Tauri.** The Python/PySide6 app (v1.x) is in the
> git history and in the [1.x releases](https://github.com/Huzzama/Neru-screen-control/releases).
> This branch currently ships the core library and the `neru` command-line tool;
> the desktop app follows.

## Supported screens

| Screen | USB ID | Resolution | Status |
|---|---|---|---|
| Thermaltake TH360 / TH420 **V3 Ultra ARGB Sync** (pump-head LCD) | `264a:2348` | 720×720 | verified |
| Thermaltake LCD Panel Kit 6" (View 600 TG) | `264a:2347` | 1110×540 | same protocol, untested |
| Thermalright coolers with the ChiZhu USBDISPLAY controller (Frozen Warframe class) | `87ad:70db` | 320×320 | verified (v1 driver, ported) |

Protocol notes: [`docs/PROTOCOLS.md`](docs/PROTOCOLS.md).

## Build from source

Requirements: Rust 1.80+ (`rustup`), `pkg-config`, `libusb`.

```bash
# Arch / CachyOS
sudo pacman -S --needed rustup pkgconf libusb && rustup default stable
# Debian / Ubuntu
sudo apt install build-essential pkg-config libusb-1.0-0-dev && curl https://sh.rustup.rs -sSf | sh

git clone https://github.com/Huzzama/Neru-screen-control.git
cd Neru-screen-control
cargo build --release
./target/release/neru --help
```

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

## Project layout

```
crates/neru-core   drivers (screen/), metrics/, config
crates/neru-cli    the `neru` tool
docs/              protocol notes
packaging/         udev rules, desktop entry, service unit, distro packaging
```

## Contributing

Have a Thermaltake or Thermalright screen that is not listed? Open an issue with
`lsusb` output and, if you can, a USB capture of the official software sending one
frame. See *Adding a screen* in `docs/PROTOCOLS.md`.

## License

MIT
