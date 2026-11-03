# Screen protocols

What Neru sends to each supported cooler screen. Both drivers live in
`crates/neru-core/src/screen/`.

## Thermaltake "BY" screens — `thermaltake_by.rs`

| | |
|---|---|
| Devices | TH360 / TH420 V3 Ultra ARGB Sync pump head (`264a:2348`, 720×720, 3.95"), LCD Panel Kit 6" (`264a:2347`, 1110×540) |
| Transport | USB HID, one interface, interrupt IN `0x81` / OUT `0x01`, 1024-byte reports, **no report ID** |
| Controller | Embedded Linux (app/firmware/sdk versions are reported on `conn`) |
| Frames | JPEG, native resolution, ≤ `0xC7FFF` bytes |

The protocol was recovered from TT RGB Plus by the
[thermaltake-6inch-lcd-linux](https://github.com/deniswsrosa/thermaltake-6inch-lcd-linux) (MIT)
and [Telinhas](https://github.com/ninjairocode/telinhas-project) projects and
re-implemented here from their notes. Nothing from Thermaltake's binaries is used.

### Commands

One command per 1024-byte report, HTTP-like text with CRLF line endings:

```
POST conn 1\r\n
SeqNumber=112\r\n
ContentType=json\r\n
ContentLength=<n>\r\n
\r\n
<json body>
```

wrapped in a frame:

```
5A | len_be16 | payload | sum8 | 5A
```

* `len` = whole unescaped frame length (payload + 5)
* `sum8` = sum of the two length bytes and the payload, mod 256
* between the two `5A` delimiters, `5A` → `5B 01` and `5B` → `5B 02`
* the report is zero-padded to 1024 bytes; hidraw needs a leading `0x00`
  report-number byte on write (usbhid strips it)

The screen answers on the IN endpoint with the same framing: `1 200`,
`AckNumber=113`, and an optional JSON body.

| Command | Body | Reply / effect |
|---|---|---|
| `POST conn 1` | — | status JSON: `OS`, `version{app,firmware,sdk,hardware}`, `space`, `brightness`, `degree`, `sn`, `osdState`, `mode`, `logo`, `timeout`, `bootFinish` |
| `POST realtimeDisplay 1` | `{"enable": true}` | switch to live frames |
| `POST brightness 1` | `{"value": 0–100}` | persists across power cycles |
| `POST rotate 1` | `{"degree": 0/90/180/270}` | persists |

Other commands seen in the official app and **not sent by Neru**:
`displayInSleep`, `power`, `config`, `sysinfoDisplay`, `fanLCDSet`,
`waterBlockScreen`, `mediaDelete`, `recovery`, `factoryTest`, `transport`,
`transported` (the last four write storage / firmware — never send them).

### Live frames

After `realtimeDisplay`, a frame is a JPEG cut into 1000-byte slices, one per
report, **without** the `5A` framing and with no reply:

```
[0..3]  5C 03 FD
[3]     frame counter, 0–59, +1 per frame
[4..6]  total slices, big-endian u16
[6..8]  slice index, big-endian u16
[8]     01
[9..24] zero
[24..]  1000 bytes of JPEG (last slice zero-padded)
```

The screen goes back to its own picture after `timeout` seconds (5 by
default) without frames, so still images are re-sent every 2 s. Animations at
20 fps have been confirmed smooth; a 720×720 frame of ~23 KB takes ~5 ms to
write.

### Access

```
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="264a", ATTRS{idProduct}=="2348", MODE="0660", TAG+="uaccess"
```

## Thermalright screens — ChiZhu Tech USBDISPLAY — `thermalright.rs`

| | |
|---|---|
| Devices | Thermalright coolers with the ChiZhu controller (`87ad:70db`); confirmed with a 320×320 panel |
| Transport | USB bulk OUT `0x01` (512-byte chunks) + vendor control transfers |
| Frames | raw framebuffer, RGB565 big-endian, row-major, 208-byte header |

Recovered in Neru 1.x from a Windows USB capture (Wireshark + usbmon). The
first capture showed `1b 00 10 90 d4 43 0e b2 ff ff 00 00` at the head of the
stream; the header below is what the driver sends per frame.

The same controller sits in many Thermalright coolers with different panels;
the size is not readable over USB, so Neru keeps one `ScreenModel` per
resolution (the list from Neru 1.x) and the user picks the variant in
Settings → *Use this screen* (default 320×320):

| Resolution | Models (diagonal) | Model id |
|---|---|---|
| 320×320 | Frozen Warframe class, the original Neru hardware (2.4") | `tr-chizhu-320` |
| 320×240 | Frozen Warframe (2.4"), Core Matrix (2.0") | `tr-chizhu-320x240` |
| 480×480 | Peerless / Guard / Hyper / Elite Vision (3.95"), Core Vision (2.1"), Frozen Guardian / Frozen Vision (2.88") | `tr-chizhu-480` |
| 640×480 | Mjolnir Vision, Stream Vision (3.5") | `tr-chizhu-640x480` |
| 1280×480 | Trofeo Vision (6.98") | `tr-chizhu-1280x480` |
| 2400×1080 | Leviathan / Rainbow / Wonder Vision (6.67") | `tr-chizhu-2400x1080` |

Other Thermalright controllers are **recognised but not driven** (listed in
`RECOGNISED_ONLY`, shown on the Dashboard): HID LCD `0416:5302` /
`0418:5303` / `0418:5304`, SCSI-over-USB `87cd:70db` / `0416:5406` /
`0402:3922`, chunked-bulk "LY" `0416:5408` / `0416:5409`, LED segment
`0416:8001`. Their protocols are keyed on handshake bytes per panel (FBL / PM
codes, JPEG vs RGB565, rotation tables) and are documented in the GPL-3.0
[TRCC Linux](https://github.com/Lexonight1/thermalright-trcc-linux) project,
which drives them today; porting them without hardware to test was not
attempted. Contributions with a capture are welcome.

### Per-frame sequence

1. control transfer `CMD_START` = `01 00`
2. bulk write `CMD_TRIG` = `00 00 00 00`
3. bulk write `HEADER (208 bytes) + pixels`
4. control transfer `CMD_COMMIT` = `09 00`

Control transfers use `bmRequestType = 0x40`, `wValue = wIndex = 0`, and
`bRequest` `0x01` (fallbacks `0x00`, `0x02`). On the first connection
`CMD_SYNC` = `33 FF` + 62 zero bytes is sent three times, 50 ms apart.

### Header (little-endian u32 fields, everything else zero)

| Offset | Value |
|---|---|
| `0x00` | `0x78563412` |
| `0x04` | `3` |
| `0x08` | width |
| `0x0C` | width |
| `0x10` | height |
| `0x38` | `2` |
| `0x3C` | width × height × 2 (pixel bytes) |
| `0xC4` | `0x2000` |

### Calibration

The panel is a dumb framebuffer, so orientation is applied in software before
encoding: rotation (counter-clockwise, 0/90/180/270), vertical flip, and a
row/column "framebuffer offset" (`numpy.roll` semantics) for panels that start
scanning at a non-zero line. Alternative pixel orders (`rgb565_le`, `bgr565_*`,
`grb565_le`, `rgb888`, `bgr888`) are kept for untested models.

### Access

```
SUBSYSTEM=="usb", ATTRS{idVendor}=="87ad", ATTRS{idProduct}=="70db", MODE="0660", TAG+="uaccess"
```

## Adding a screen

1. Add a `ScreenModel` to `MODELS` in `screen/mod.rs` (id, size, VID:PID, protocol).
2. If it speaks one of the two protocols above, that is all: `neru devices`
   will list it and `neru test` will drive it.
3. Otherwise add a module implementing the `Screen` trait and a `Protocol`
   variant. Keep the driver free of UI concerns; it receives a finished RGB
   frame of the screen's native size.
