# Fan control

Neru drives fans through the kernel's **hwmon** interface — the same
`/sys/class/hwmon/*/pwmN` files `fancontrol` and CoolerControl use. Nothing
proprietary: if the kernel exposes a PWM channel, Neru can set it.

```
 app (user) ──/run/neru-fand.sock──▶ neru-fand (root, systemd) ──▶ /sys/class/hwmon/…/pwmN
```

## The helper

Writing to sysfs needs root, so the curves run in `neru-fand`, a ~1 MB
helper installed as a systemd service (Fans page → **Install helper**, asks
for your password once). It:

* applies the curves once a second, app open or not, from boot;
* remembers each channel's original `pwmN_enable` and writes it back when a
  channel is set to *BIOS*, when the service stops, and when the config is
  cleared — the motherboard's own curve takes over again;
* runs a channel at **100 %** if its temperature sensor disappears;
* keeps its configuration in `/etc/neru-screen-control/fans.json`.

Remove it with **Remove helper** (or `systemctl disable --now neru-fand`).

## Modes

| Mode | Behaviour |
|---|---|
| BIOS | untouched: the motherboard's curve |
| Fixed | constant duty |
| Curve | duty follows a temperature source through (°C, %) points; `min %` floor; `max change %/s` slew limit so fans don't pulse |

Temperature sources are every hwmon temperature on the system: `k10temp:Tctl`
(AMD CPU), `coretemp:Package id 0` (Intel), `amdgpu:edge` / `junction`
(AMD GPU), `nvme:Composite`, the Super I/O's own inputs (`nct6798:SYSTIN`…),
`asusec:*` on ASUS boards with `asus_ec_sensors`.

## When no channels show up

The PWM channels live on the motherboard's Super I/O chip and need its
driver. On ASUS AM4/AM5 boards that is `nct6775` (chip NCT6798/NCT6799):

```bash
sudo modprobe nct6775            # loads if the board is on the kernel's ASUS WMI list (most are, kernel ≥ 6.1)
ls /sys/class/hwmon/*/pwm1       # channels appear
```

If `modprobe` fails with "ACPI resource conflict", the firmware claims the
chip. Either add `acpi_enforce_resources=lax` to the kernel command line
(works, mildly unsupported) or check for a newer kernel that lists your board.
Make the module permanent in `/etc/modules-load.d/nct6775.conf`.

Fans plugged into an AIO's own controller (not the motherboard headers) are
not hwmon devices and cannot be controlled this way; the TH V3 Ultra's fan
hub takes PWM from the motherboard header it is plugged into, so it shows up
as that header's channel.

## Safety notes

* Curves start at the current duty and move at most `max change %/s`, so
  applying a curve never slams a fan to 0 %.
* `min %` defaults to 20 % — set a fan to *Fixed 0* only if it is a pump
  header you are sure about (most AIO pumps should stay on BIOS/100 %).
* The helper and the BIOS never fight: a channel is either manual (helper) or
  automatic (BIOS), and the helper only touches channels you configured.
