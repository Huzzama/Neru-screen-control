# TRCC — Thermalright Cooler Control Center (Linux)

Open-source Linux driver and GUI for Thermalright LCD coolers.

---

## Folder structure

```
TRCC/
├── src/
│   ├── config/             ← config loader + display profiles
│   ├── display/            ← USB transport, frame builder, protocol encoder
│   ├── media/              ← image / GIF / video loaders
│   ├── metrics/            ← CPU + GPU metric collectors (AMD + NVIDIA)
│   ├── ui/                 ← PySide6 GUI
│   ├── device_configs/     ← keep existing JSON device configs
│   └── controller.py       ← main entry point
├── config.json
├── requirements.txt
├── trcc.service            ← systemd unit (edit paths before installing)
└── 99-chizhou-display.rules
```

---

## Files from the old project — what to do

| Old file | Action |
|---|---|
| `src/controller.py` | **Replace** with new `src/controller.py` |
| `src/metrics.py` | **Delete** — replaced by `src/metrics/` package |
| `src/get_amd_power.py` | **Delete** — logic merged into `src/metrics/cpu.py` |
| `src/displayer.py` | **Delete** — replaced by `src/display/` package |
| `src/config.py` | **Delete** — replaced by `src/config/loader.py` |
| `src/device_configurations.py` | **Keep** — still used for LED config |
| `src/device_configs/*.json` | **Keep** — Thermalright device layouts |
| `src/led_display_ui.py` | **Keep** for now — LED color UI (ARGB fans) |
| `src/utils.py` | **Keep** — color interpolation helpers |

---

## Setup

```bash
# 1. Create virtual environment
python3 -m venv .venv
source .venv/bin/activate

# 2. Install dependencies
pip install -r requirements.txt

# 3. Install udev rule (once, as root)
sudo cp 99-chizhou-display.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules && sudo udevadm trigger
# Unplug and replug the USB cable

# 4. Run headless
python src/controller.py

# 5. Run with GUI
python src/controller.py --ui

# 6. Run with explicit config
python src/controller.py config.json --ui
```

---

## Autostart (systemd)

```bash
# Edit trcc.service — replace USER and paths
mkdir -p ~/.config/systemd/user
cp trcc.service ~/.config/systemd/user/trcc.service
systemctl --user enable trcc.service
systemctl --user start  trcc.service
systemctl --user status trcc.service
```

---

## Supported display models

| Model | Resolution |
|---|---|
| Frozen Warframe | 320 × 240 |
| Core Matrix | 320 × 240 |
| Mjolnir Vision | 640 × 480 |
| Peerless Vision | 480 × 480 |
| Stream Vision | 640 × 480 |
| Core Vision | 480 × 480 |
| Frozen Guardian | 480 × 480 |
| Frozen Vision | 480 × 480 |
| Guard / Hyper / Elite Vision | 480 × 480 |
| Trofeo Vision | 1280 × 480 |
| Leviathan / Rainbow / Wonder Vision | 2400 × 1080 |

---

## Protocol status

The USB protocol header observed in Wireshark capture:
```
1b 00 10 90 d4 43 0e b2 ff ff 00 00
```
This is stored in `src/display/protocol.py` → `HEADER_MAGIC`.

**If frames don't display correctly:** capture USB traffic from the Windows
software with Wireshark + usbmon and inspect the first bulk OUT packet.
Update `HEADER_MAGIC` and `build_frame()` in `protocol.py` accordingly.

---

## GPU support

- **NVIDIA**: uses `pynvml` (preferred) → falls back to `nvidia-smi`
- **AMD**:    uses `pyamdgpuinfo` → falls back to `rocm-smi`
- Both detected automatically at startup — no config needed.
