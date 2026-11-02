# RGB lighting

Neru does not talk to RGB hardware itself — [OpenRGB](https://openrgb.org) does.
OpenRGB supports hundreds of devices (motherboard ARGB headers, RAM, GPUs,
fan controllers, keyboards…) and exposes them through an SDK server on TCP
port 6742. Neru connects to that server, shows every device and zone, and
either picks a device-native mode or drives the LEDs itself.

```
 Neru ──SDK (6742)──▶ OpenRGB ──hidraw / i2c / usb──▶ motherboard, RAM, GPU, …
```

## Setup

1. Install OpenRGB (`sudo pacman -S openrgb`, or from openrgb.org for other distros).
2. Make sure OpenRGB itself sees your hardware: run it once, check the device
   list, and fix anything missing there (see troubleshooting below). Neru can
   only control what OpenRGB detects.
3. Turn on the SDK server: OpenRGB → **Settings → General → Start server**, or
   let Neru start a headless one (`openrgb --server`) — Settings → RGB →
   "Start the OpenRGB server when needed" (on by default).

## Software effects vs hardware modes

| | Software effects (Neru) | Hardware modes (device) |
|---|---|---|
| What | static, breathing, rainbow, colour cycle, **temperature-reactive** | the modes the device firmware offers (Static, Breathing, Rainbow, Spectrum…) |
| Per zone | yes | no (whole device) |
| Needs Neru running | yes (LEDs go dark or keep the last frame when it quits) | no |
| Survives reboot | re-applied when Neru starts | with **Save to device** where the firmware supports it |

Temperature effects read the same sensors as the screen themes (`cpu.temp`,
`gpu.temp`, …) and map them through a colour ramp.

`neru rgb devices` lists what the server reports; `neru rgb set 0 ff0000`
sets a device (or `--zone 1`) to a colour — handy for checking a device
responds at all.

## Troubleshooting (what OpenRGB needs on Linux)

**Motherboard / USB controllers not listed (e.g. ASUS Aura USB `0b05:19af`)**

OpenRGB opens them through `/dev/hidraw*`, which is root-only until its udev
rules are installed:

```bash
ls /usr/lib/udev/rules.d/60-openrgb.rules /etc/udev/rules.d/60-openrgb.rules   # one must exist
sudo udevadm control --reload-rules && sudo udevadm trigger                     # after installing
ls -l /dev/hidraw*                                                              # should show mode 0660 + your seat (uaccess)
```

The Arch/CachyOS `openrgb` package ships the rules; Flatpak and AppImage
builds do **not** — copy `60-openrgb.rules` from
<https://openrgb.org/udev.html> to `/etc/udev/rules.d/` and replug / reboot.
Also check OpenRGB → Settings → Supported Devices that the controller is not
disabled, and that no other program (e.g. a Wine Armoury Crate) holds the device.

**RAM / GPU not listed**

They are reached over SMBus/I²C: `sudo modprobe i2c-dev` (make it permanent in
`/etc/modules-load.d/`), and on Intel boards `i2c-i801`. Some AMD boards need
`i2c-piix4`. The udev rules also grant `/dev/i2c-*`.

**Fans on a Thermaltake TH V3 Ultra / other "ARGB Sync" coolers**

Their lighting is wired to the motherboard's 5 V ARGB header, so it appears
as a zone of the motherboard (e.g. *Aura Addressable 1*), not as a separate
device. Set the zone's LED count in OpenRGB if the header reports fewer LEDs
than you have (resizable zones).

**OpenRGB says "no devices" but the GUI shows them**

The SDK server and the GUI are the same process; make sure only one OpenRGB
instance runs (Neru's headless server and a GUI instance will fight over the
hardware).
