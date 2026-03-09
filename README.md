# Neru Screen Control – Thermalright LCD Control for Linux

> Open-source Linux driver and GUI for Thermalright USB LCD cooler displays.

![Python](https://img.shields.io/badge/Python-3.10%2B-blue?logo=python&logoColor=white)
![PySide6](https://img.shields.io/badge/UI-PySide6-41cd52?logo=qt&logoColor=white)
![License](https://img.shields.io/badge/License-MIT-yellow)
![Platform](https://img.shields.io/badge/Platform-Linux-orange?logo=linux&logoColor=white)

---

## What is this?

Neru Screen Control lets you drive the small USB LCD display built into Thermalright CPU coolers directly from Linux — no Windows software required.

Show **live sensor data** (CPU/GPU temps, clocks, power draw, RAM usage) or build a fully custom **theme** using the built-in canvas editor, with text labels, metric bars, images, animated GIFs, and video.

---

## Features

- 🖥 **Canvas theme editor** — drag, resize, and layer elements visually
- 📊 **Live metrics** — CPU & GPU temp, usage, frequency, power draw, RAM
- 🎨 **Custom themes** — text, metric bars, images, GIFs, video (streamed, no RAM limit)
- 🎯 **Calibration tab** — fix pixel offset and rotation without guessing
- ⚙ **Settings tab** — autostart on login, run in background, system tray icon
- 🔌 **Auto-detected GPU** — NVIDIA (pynvml) and AMD (pyamdgpuinfo / rocm-smi)
- 🐞 **Debug tab** — test patterns, pixel format override, live USB log
- ❓ **Built-in help** — full documentation inside the app

---

## Download

Go to [**Releases**](https://github.com/Huzzama/Neru-screen-control/releases) and pick the right file for your distro:

| File | For |
|---|---|
| `neru-screen-control-x.x.x-x86_64.AppImage` | Any Linux distro (universal) |
| `neru-screen-control_x.x.x_all.deb` | Ubuntu, Linux Mint, Debian, Pop!_OS |
| AUR: `yay -S neru-screen-control` | Arch, EndeavourOS, Manjaro |

---

## Quick start (from source)

### 1 — Clone
```bash
git clone https://github.com/Huzzama/Neru-screen-control.git
cd Neru-screen-control
```

### 2 — Virtual environment + dependencies
```bash
python3 -m venv venv
source venv/bin/activate
pip install -r requirements.txt
```

### 3 — USB device permission (once, requires sudo)
```bash
sudo cp 99-chizhou-display.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules && sudo udevadm trigger
```
Unplug and replug the USB cable after this step.

> **Tip:** you can also do this from inside the app — **⚙ Settings → USB Device Access → Install (requires password)**.

### 4 — Run
```bash
# Open the full GUI
python main.py --ui

# Background mode only (headless)
python main.py --background

# GUI but start hidden in system tray
python main.py --ui --hidden
```

---

## Autostart

The easiest way is through the app: **⚙ Settings → Start Neru Screen Control automatically when I log in**.

To do it manually:
```bash
mkdir -p ~/.config/systemd/user
cp packaging/shared/neru-screen-control.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable neru-screen-control.service
systemctl --user start  neru-screen-control.service
```

---

## Project structure

```
Neru-screen-control/
├── .github/
│   └── workflows/
│       └── release.yml         ← automated builds on version tag
├── packaging/
│   ├── appimage/               ← AppImage build script + AppRun
│   ├── arch/                   ← PKGBUILD + .SRCINFO for AUR
│   ├── debian/                 ← control, rules, postinst, etc.
│   └── shared/                 ← .desktop file, icons (all sizes), service template
├── src/
│   ├── config/                 ← config loader + display profiles
│   ├── display/                ← USB transport, frame builder, protocol encoder
│   ├── driver/                 ← low-level USB handshake + pixel formats
│   ├── media/                  ← image / GIF / video loaders
│   ├── metrics/                ← CPU + GPU collectors (AMD + NVIDIA)
│   ├── models/                 ← display model definitions
│   ├── service/                ← systemd user-service manager
│   ├── ui/                     ← PySide6 GUI (tabs, canvas editor, properties)
│   ├── controller.py           ← CLI entry point (--ui / --background / --hidden)
│   └── version.py              ← version string
├── tests/
├── icon.png
├── main.py
├── pyrightconfig.json
├── requirements.txt
├── 99-chizhou-display.rules    ← udev rule for non-root USB access
└── README.md
```

---

## Supported displays

| Model | Size | Resolution |
|---|---|---|
| Frozen Warframe | 2.4″ | 320 × 240 |
| Core Matrix | 2.0″ | 320 × 240 |
| Mjolnir Vision | 3.5″ | 640 × 480 |
| Stream Vision | 3.5″ | 640 × 480 |
| Peerless / Guard / Hyper / Elite Vision | 3.95″ | 480 × 480 |
| Core Vision | 2.1″ | 480 × 480 |
| Frozen Guardian / Frozen Vision | 2.88″ | 480 × 480 |
| Trofeo Vision | 6.98″ | 1280 × 480 |
| Leviathan / Rainbow / Wonder Vision | 6.67″ | 2400 × 1080 |

---

## Building packages

```bash
# AppImage (universal)
bash packaging/appimage/build-appimage.sh

# .deb (Debian/Ubuntu/Mint)
cp -r packaging/debian debian
dpkg-buildpackage -us -uc -b

# Arch (local test)
cd packaging/arch && makepkg -si
```

Releases are built automatically by GitHub Actions when you push a version tag:
```bash
git tag v1.0.0 && git push origin v1.0.0
```

---

## GPU support

| Vendor | Primary | Fallback |
|---|---|---|
| NVIDIA | `pynvml` | `nvidia-smi` |
| AMD | `pyamdgpuinfo` | `rocm-smi` |

Both detected automatically at startup — no config needed.

---

## USB protocol

**VID:PID** `87ad:70db` (ChiZhu Tech)

Frame header observed via Wireshark + usbmon:
```
1b 00 10 90 d4 43 0e b2 ff ff 00 00
```
Stored in `src/display/protocol.py` → `HEADER_MAGIC`. Pixel format is `rgb565_be`.

If frames look wrong on an untested model, capture USB traffic from the Windows driver and compare the first bulk OUT packet. Update `HEADER_MAGIC` and `encode_pixels()` in `protocol.py` accordingly.

---

## Contributing

Issues and pull requests are welcome. If you have a Thermalright model not listed above, open an issue and attach a Wireshark capture of the Windows driver sending a frame — that's all that's needed to add support.

---

## License

MIT