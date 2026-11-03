# RGB lighting

Neru drives lighting hardware two ways, and shows both in the same list:

* **Native drivers** (`crates/neru-core/src/rgb/native/`) — Neru talks to the
  device itself, no other software involved. Ported from OpenRGB's reverse
  engineering (GPL-2.0-or-later; Neru is GPL-3.0).
* **OpenRGB** (optional) — devices Neru has no driver for yet are reached
  through an installed [OpenRGB](https://openrgb.org) over its SDK server
  (TCP 6742). A device handled natively is hidden from the OpenRGB list so it
  is never driven twice.

```
 Neru ──hidraw──▶ ASUS Aura USB motherboard (+ ARGB headers: fans, strips, the cooler's ring)
 Neru ──/dev/i2c-N (NVIDIA adapter)──▶ ZOTAC RTX 50 GPU
 Neru ──SDK 6742──▶ OpenRGB ──▶ everything else (RAM, other GPUs, keyboards…)
```

## Native drivers

| Driver | Devices | Bus | Zones | Direct (per-LED) | Tested |
|---|---|---|---|---|---|
| `aura_usb` | ASUS Aura USB motherboards (`0b05:18f3/1939/19af/1aa6/1bed`) | HID | fixed board LEDs + each 5 V ARGB header (resizable, up to 120 LEDs) | yes | yes (ROG STRIX B850-A) |
| `zotac_gpu` | ZOTAC GeForce RTX 50: 5080 AMP Extreme INFINITY, 5090 SOLID OC, 5090 D SOLID OC (other ZOTAC RTX 50 cards are probed with a generic 3-zone layout) | I²C `0x4B` on the card's own adapter | 2–3 single-LED zones (Logo, Side Bar, Infinity Mirror…) | no — software effects rewrite the *Static* colour (~60 ms per zone, writes are coalesced on a background thread) | yes (RTX 5080) |
| `ene_smbus` | ENE / ASUS Aura SMBus: RGB RAM (remapped from 0x77 to 0x70…), older ASUS boards (0x40/0x4E/0x4F), 173 ASUS graphics cards (0x67 on the card's adapter) | SMBus (`i2c-piix4` / `i2c-i801`) + I²C | from the chip's config table (DRAM, RGB Header, Audio, PCIe…) | yes | needs testers |
| `corsair_node` | Corsair Lighting Node Pro / Core, Commander Pro, LS100, LT100, 1000D Obsidian, SPEC-OMEGA RGB | HID | 1–2 lighting channels (resizable, up to 204 LEDs) | yes (keepalive commit every 5 s) | needs testers |
| `nzxt_hue2` | NZXT Hue 2 / Ambient, Smart Device V2, RGB & Fan Controller (all revisions), RGB Controller, Kraken X3 / Z3 ring + logo, Kraken 2024 Elite | HID | one per channel, sized from the accessories the hub reports (fans, strips, combs) | yes | needs testers |
| `asrock_polychrome` | ASRock Polychrome USB boards (`26ce:01a2`), DeskMini LED strip (`26ce:01a6`) | HID | RGB headers, ARGB headers (resizable, up to 100), PCH, I/O cover, PCB — whatever the board reports | yes | needs testers |

Drivers marked *needs testers* are faithful ports of OpenRGB's code written
without the hardware at hand: packet layouts are checked against the original
byte for byte, but nothing beats a real device. `neru rgb native` shows what a
driver found; please report both successes and failures.

Requirements, installed by **Settings → Device access → Install rules**
(`neru udev --install`, the deb/AUR packages and `install.sh` do the same):

* `/etc/udev/rules.d/70-neru-screen-control.rules` — hidraw access for the
  boards, `uaccess` on `/dev/i2c-*`.
* `/etc/modules-load.d/neru-screen-control.conf` — loads `i2c-dev` (creates
  the `/dev/i2c-*` nodes) plus the chipset SMBus drivers `i2c-piix4` (AMD) and
  `i2c-i801` (Intel) that RGB RAM lives on. GPU I²C adapters come from the
  NVIDIA driver (proprietary or `nvidia-open`); nouveau does not expose them.

`neru rgb native` lists the devices Neru found itself (`--color ff0000 --leds 24`
paints everything to check); `neru rgb i2c` lists the I²C adapters with the
PCI ids behind each — attach it to an issue if your ZOTAC card is not picked up.

### Adding a driver

Implement `NativeDevice` (`describe`, `set_direct`, `zone_leds`, `set_mode`,
`save_mode`, `resize_zone`) and add the probe to `NativeHub::scan`. The
`Controller` you describe uses OpenRGB's vocabulary (modes with flags, zones,
LEDs) so the UI and the effect engine need no changes. Most OpenRGB
controllers port in an afternoon; keep the SPDX header and credit the author.

## Software effects vs hardware modes

| | Software effects (Neru) | Hardware modes (device) |
|---|---|---|
| What | static, **gradient**, breathing, rainbow, colour cycle, **wave**, **chase**, **twinkle**, **candle**, **strobe**, **temperature-reactive** | the modes the firmware offers (Aura: Static, Breathing, Spectrum Cycle, Rainbow…; ZOTAC: Static, Breathe, Fade, Wink, Glide, Prism, Bokeh, Beacon, Tandem, Tidal, Astra, Cosmic, Volta) |
| Per zone | yes | whole device (colour per zone where the mode allows) |
| Needs Neru running | yes (LEDs keep the last frame when it quits) | no |
| Survives reboot | re-applied when Neru starts | with **Save to device** where the firmware supports it (ZOTAC saves every write) |
| Rate | Settings → RGB → updates per second (20 default) | — |

Effect parameters (all colours `#rrggbb`):

| Effect | Parameters |
|---|---|
| `static` | `color` |
| `gradient` | `colors[]`, `speed` (scroll, cycles/min, 0 = still), `repeat` (times the gradient fits in the zone) |
| `breathing` | `color`, `period_s` |
| `rainbow` | `speed` (cycles/min), `spread` (degrees of hue across the zone) |
| `color_cycle` | `colors[]`, `period_s` |
| `wave` | `color`, `background`, `period_s`, `width` (0–1 of the zone), `bounce` |
| `chase` | `color`, `background`, `period_s`, `count` (runs), `width` (LEDs per run) |
| `twinkle` | `color`, `background`, `density` (0–1), `period_s` |
| `candle` | `color`, `intensity` (0–1) |
| `strobe` | `color`, `hz`, `duty` (0–1) |
| `temperature` | `metric` (any theme metric key: `cpu.temp`, `gpu.temp`, `disk.temp`, `mem.pct`…), `ramp` |
| `hardware` | `mode` (name), `color`, `speed`, `brightness`, `direction` (0 left, 1 right) |

Assignments live in the config as `rgb.assignments` (`controller` key, optional
`zone`, `effect`), so a hand-edited config works too.

## Zones and LED counts

Addressable headers do not know how many LEDs hang off them. Set the count
per header in the RGB page (stored as `rgb.zone_sizes`); a fan usually has
8–24 LEDs, a strip 30–60. Too few leaves the end of a strip dark; too many is
harmless. Fans on a Thermaltake TH V3 Ultra / "ARGB Sync" coolers are wired
to the motherboard's 5 V ARGB header, so they show up as that header's zone,
not as their own device.

## Troubleshooting

**Native device missing** — press *Rescan*. Check `neru rgb native` from a
terminal: a `permission denied` means the udev rules are not active (install,
then replug the device or log out and in). For a ZOTAC card, `neru rgb i2c`
must show `NVIDIA i2c adapter … [ok]` lines; `no /dev/i2c-*` means
`sudo modprobe i2c-dev`; `no access` means the udev rule; if the adapters are
there but nothing answers at `0x4B`, open an issue with the output (the card
may use the older ZOTAC protocol — RTX 30/40 cards do).

**OpenRGB devices missing** — OpenRGB itself must see them: its own udev
rules (`60-openrgb.rules`), `i2c-dev` / `i2c-piix4` / `i2c-i801` for RAM and
GPUs, and only one OpenRGB instance running (Neru's headless server and a GUI
instance fight over the hardware). Neru's "Start server" button runs
`openrgb --server --noautoconnect`.

**Effects lag** — lower *updates per second* in Settings → RGB. ZOTAC cards
are inherently slow (I²C byte writes with mandatory pauses); Neru drops frames
for them rather than stalling other devices.
