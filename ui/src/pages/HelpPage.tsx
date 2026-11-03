import { BookOpen, ExternalLink, Fan, Lightbulb, MonitorSmartphone, Settings, Terminal, Wrench } from "lucide-react";
import { useState, type ReactNode } from "react";

import type { Page } from "../App";

const DOCS = "https://github.com/Huzzama/Neru-screen-control";

type Topic = { id: string; title: string; icon: typeof BookOpen; body: ReactNode };

function Kbd({ children }: { children: ReactNode }) {
  return <span className="kbd">{children}</span>;
}

function Code({ children }: { children: ReactNode }) {
  return <code className="small mono">{children}</code>;
}

export function HelpPage({ goTo }: { goTo: (p: Page) => void }) {
  const [open, setOpen] = useState<string>("start");

  const topics: Topic[] = [
    {
      id: "start",
      title: "Getting started",
      icon: BookOpen,
      body: (
        <>
          <p>
            Neru does three things: it draws live system information on your cooler's LCD, it controls the RGB lighting of your PC, and it runs fan curves. Each has its own page in the sidebar. The screen status at the bottom of the sidebar tells you whether a screen is connected.
          </p>
          <ol className="help-steps">
            <li>
              <strong>Give Neru access to the USB devices.</strong> Linux only lets root talk to USB hardware until a udev rule says otherwise. Go to <button className="link" onClick={() => goTo("settings")}>Settings → Device access → Install rules</button>, enter your password, then unplug and replug the cooler's USB cable once (or reboot). The sidebar pill turns green when the screen is found.
            </li>
            <li>
              <strong>Pick a theme.</strong> <button className="link" onClick={() => goTo("screen")}>Screen</button> lists bundled themes; “Apply” sends one to the LCD. “Edit a copy” opens it in the editor, where every change shows up on the screen as you make it.
            </li>
            <li>
              <strong>Light things up.</strong> <button className="link" onClick={() => goTo("rgb")}>RGB</button> lists every lighting device Neru can drive. Pick an effect per device or per zone, or use “Apply to everything”.
            </li>
            <li>
              <strong>Optional: fan curves.</strong> <button className="link" onClick={() => goTo("fans")}>Fans</button> needs a small root helper (one click, asks for your password). Curves keep running after you close the app.
            </li>
            <li>
              <strong>Start with your session.</strong> Settings → “Start on login” adds an autostart entry that launches Neru hidden in the tray, so the screen and lights come alive at boot.
            </li>
          </ol>
          <p className="small muted">Closing the window keeps Neru running in the tray (the screen keeps updating). Quit from the tray menu, or turn that off in Settings.</p>
        </>
      ),
    },
    {
      id: "screen",
      title: "Screen & themes",
      icon: MonitorSmartphone,
      body: (
        <>
          <p>
            A theme is a layout of elements on a canvas the size of your screen (720×720 on the Thermaltake TH V3 Ultra, 320×320 on Thermalright ChiZhu panels). Themes made for another size are scaled to fit, but a theme built for your exact screen looks best — set the screen size in the right panel with nothing selected. Thermalright screens all share one USB id, so if yours is a 480×480, 640×480, 1280×480 or 2400×1080 panel, pick that variant in Settings → <em>Use this screen</em>. A Thermalright display on another controller family is reported on the Dashboard as recognised-but-not-driven; TRCC Linux covers those.
          </p>
          <h4>The editor</h4>
          <ul className="help-list">
            <li><strong>Add</strong> opens a menu: ready-made <em>widgets</em> (CPU temperature, network graphs, clock…) drop several elements at once; <em>metrics</em> are the building blocks (a value, a bar, a gauge, a graph); <em>decoration</em> holds text, images, GIFs, videos and shapes.</li>
            <li>Drag elements on the canvas; drag the corner handle to resize. Arrow keys nudge by 1 px (<Kbd>Shift</Kbd> ×10). The grid button snaps to 8 px.</li>
            <li>The left panel lists layers (top-most first): reorder with the arrows, hide or lock any of them. The right panel edits the selected element.</li>
            <li><Kbd>Ctrl+Z</Kbd> undo, <Kbd>Ctrl+Y</Kbd> redo, <Kbd>Ctrl+D</Kbd> duplicate, <Kbd>Del</Kbd> remove, <Kbd>Ctrl+S</Kbd> save, <Kbd>Esc</Kbd> deselect.</li>
            <li><strong>Save</strong> stores the theme in your themes folder; <strong>Apply</strong> saves and makes it the theme on the screen.</li>
          </ul>
          <h4>Placeholders in text</h4>
          <p>
            Anything in braces is replaced by a live value: <Code>{"{cpu.temp:0}"}°</Code>, <Code>{"{gpu.usage}"}%</Code>, <Code>{"{mem.used:size}"}</Code>, <Code>{"{net.down:speed}"}</Code>, <Code>{"{fan.cpu:0} rpm"}</Code>, <Code>{"{time:%H:%M}"}</Code>, <Code>{"{date:%A %d %B}"}</Code>, <Code>{"{uptime}"}</Code>, <Code>{"{cpu.name}"}</Code>. The number after the colon is the decimals; <Code>speed</Code>, <Code>bits</Code>, <Code>size</Code>, <Code>bytes</Code> and <Code>pct</Code> pick a unit automatically. “Insert a metric…” under the text field lists every key available on your machine, fans included.
          </p>
          <h4>Colour by value</h4>
          <p>Text, bars, gauges and graphs can take their colour from a metric through a ramp — e.g. 40° blue, 70° yellow, 85° red — so hot parts of the screen go red by themselves.</p>
          <h4>Graphs</h4>
          <p>A graph draws the last N seconds of a metric (Neru keeps an hour of history for every value). Use fixed 0–100 for percentages and <em>auto scale</em> for things like download speed. Styles: area, line, bars.</p>
          <h4>Images, GIFs and video</h4>
          <p>
            Backgrounds and image elements accept PNG/JPG/WebP, animated GIFs and video (MP4, MKV, WebM, MOV — needs <Code>ffmpeg</Code> installed). Files are copied into your media folder; “Fit” decides whether they are cropped, letterboxed or stretched. Video decoding can use your GPU: Settings → Hardware acceleration.
          </p>
          <h4>Brightness, rotation, speed</h4>
          <p>The Dashboard has brightness and rotation. Settings has the frame rate (10 fps is plenty for numbers, 20 for smooth GIFs) and the JPEG quality for Thermaltake screens.</p>
        </>
      ),
    },
    {
      id: "rgb",
      title: "RGB lighting",
      icon: Lightbulb,
      body: (
        <>
          <p>
            Neru talks to lighting hardware itself where it has a driver (marked <span className="badge green">native</span>): ASUS Aura USB motherboards with their ARGB headers, ENE / ASUS RGB RAM, ASUS graphics cards, ZOTAC RTX 50 cards, Corsair Lighting Node / Commander Pro, NZXT Hue 2 / Smart Device V2 / Kraken, ASRock Polychrome USB. Everything else can still be reached through <a href="https://openrgb.org" target="_blank" rel="noreferrer">OpenRGB</a> if it is installed — Neru starts its server quietly and lists the devices alongside its own. Both kinds work the same way in the app.
          </p>
          <h4>Zones and LED counts</h4>
          <p>
            A device has zones (a motherboard: its own LEDs plus each ARGB header). Addressable headers do not know how many LEDs hang off them, so set the count per header (a fan usually has 8–24, a strip 30–60). Too few and part of a strip stays dark; too many is harmless.
          </p>
          <h4>Effects</h4>
          <ul className="help-list">
            <li><strong>Software effects</strong> are computed by Neru and streamed to the LEDs: static, gradient, breathing, rainbow, colour cycle, wave, chase, twinkle, candle, strobe, and <em>temperature</em> (colour follows CPU/GPU heat through a ramp). They stop when Neru quits.</li>
            <li><strong>Hardware modes</strong> run on the device itself (ASUS “Spectrum Cycle”, ZOTAC “Prism”…), keep going without Neru, and <em>Save to device</em> keeps them across reboots where the hardware allows it.</li>
            <li>Set an effect for the whole device, or override single zones. “Apply to everything” pushes one effect to every device at once.</li>
            <li>Effect updates per second (Settings → RGB) can be lowered if a slow USB controller lags.</li>
          </ul>
          <h4>Device not listed?</h4>
          <p>
            Press <em>Rescan</em>. If it is still missing: for Neru's own drivers, the udev rules must be installed (Settings → Device access) and the device replugged; GPUs and RGB RAM also need the <Code>i2c-dev</Code> module plus the chipset SMBus driver (<Code>i2c-piix4</Code> on AMD, <Code>i2c-i801</Code> on Intel), which the same button loads. For OpenRGB devices, check that <Code>openrgb</Code> runs and sees the device on its own. Devices Neru has no driver for yet are welcome as issues on GitHub — include <Code>lsusb</Code> and <Code>neru rgb i2c</Code> output.
          </p>
          <h4>Colours came back different after a restart?</h4>
          <p>
            Assignments and LED counts are saved per device (name + serial), so they survive reboots even when <Code>/dev/hidrawN</Code> numbers change. Hardware modes are re-applied by Neru at start; use <em>Save to device</em> for lighting that must be right before Neru is running.
          </p>
        </>
      ),
    },
    {
      id: "fans",
      title: "Fan curves",
      icon: Fan,
      body: (
        <>
          <p>
            Fan control writes to the motherboard's PWM outputs, which needs root. Neru ships a tiny helper service (<Code>neru-fand</Code>) that does only that; install it from the Fans page (asks for your password once). The app then talks to it over a local socket, and the helper keeps applying your curves after the app is closed or at boot.
          </p>
          <ul className="help-list">
            <li>Each PWM channel can be <em>Auto</em> (BIOS keeps control), <em>Fixed</em> (a percentage) or a <em>Curve</em>: a temperature sensor (CPU, GPU, NVMe, motherboard…) mapped to fan speed through a few points.</li>
            <li>Presets (silent, balanced, performance) are a starting point; drag the points to taste. The minimum % stops a fan from stalling.</li>
            <li><em>Release</em> hands every fan back to the BIOS instantly. Uninstalling the helper does the same.</li>
            <li>No channels listed? Your Super I/O chip may need a kernel module (<Code>nct6775</Code>, <Code>it87</Code>…) — <Code>sensors-detect</Code> from lm-sensors finds it.</li>
          </ul>
        </>
      ),
    },
    {
      id: "settings",
      title: "Settings worth knowing",
      icon: Settings,
      body: (
        <ul className="help-list">
          <li><strong>Start on login</strong> — autostart entry; Neru starts hidden in the tray.</li>
          <li><strong>Keep running when the window is closed</strong> — the screen keeps updating from the tray.</li>
          <li><strong>Hardware acceleration</strong> — <em>Video decoding</em> lets ffmpeg use the GPU for video backgrounds (NVIDIA via CUDA/NVDEC, AMD and Intel via VA-API; “Auto” tries and falls back to software). <em>GPU window rendering</em> lets the app window draw through the GPU; it is off by default because NVIDIA on Wayland can show a blank window with it on — try it, and turn it back off if that happens. Both take effect after a restart of the video / app.</li>
          <li><strong>Fahrenheit</strong>, <strong>sensor refresh</strong> — how often values are read (1 s is a good default).</li>
          <li><strong>Use this screen</strong> — pin one screen when two are connected.</li>
          <li><strong>Thermalright panel calibration</strong> — only for ChiZhu-based screens whose picture is shifted or mirrored.</li>
          <li><strong>Check for updates</strong> (About) — asks GitHub for the newest release and offers the AppImage; Neru also checks once after it starts and shows a small “Neru x.y.z is out” pill in the sidebar. Nothing is downloaded on its own. AUR and .deb installs update through the package manager.</li>
        </ul>
      ),
    },
    {
      id: "cli",
      title: "Command line",
      icon: Terminal,
      body: (
        <>
          <p>
            The <Code>neru</Code> tool does everything the app does with hardware, which makes it handy for scripts and bug reports. The deb and AUR packages install it; from a source checkout build it with <Code>cargo build --release -p neru-cli</Code>.
          </p>
          <pre className="help-pre">{`neru devices                 # screens found and whether you can open them
neru probe                   # handshake: firmware, brightness, rotation
neru test                    # colour test pattern on the screen
neru show wallpaper.png      # any image; GIFs animate
neru metrics --watch         # every sensor Neru reads
neru theme render t.json -o frame.png --fake
neru rgb native              # Neru's own RGB drivers: devices and zones
neru rgb native --color ff0000 --leds 24
neru udev --install`}</pre>
          <p className="small muted">
            Set <Code>NERU_LOG=debug</Code> for verbose logs (also for the app).
          </p>
        </>
      ),
    },
    {
      id: "trouble",
      title: "Troubleshooting",
      icon: Wrench,
      body: (
        <ul className="help-list">
          <li><strong>“Screen found — needs device access”</strong>: install the udev rules (Settings) and replug the USB cable. If it persists, check <Code>ls -l /dev/hidraw*</Code> shows your user's group or <Code>uaccess</Code> tags, and that no other program (the vendor's Windows app in a VM, another LCD tool) holds the device.</li>
          <li><strong>No screen at all</strong>: <Code>lsusb</Code> must list it (Thermaltake <Code>264a:2348</Code>, Thermalright <Code>87ad:70db</Code>). Some coolers only enumerate their USB port after the pump has power. Unknown models: open an issue with the <Code>lsusb</Code> line.</li>
          <li><strong>Blank or black app window</strong> (often NVIDIA + Wayland): make sure GPU window rendering is off (Settings → Hardware acceleration), or start with <Code>WEBKIT_DISABLE_DMABUF_RENDERER=1</Code>.</li>
          <li><strong>Video background does not play</strong>: install <Code>ffmpeg</Code>. If hardware decoding is on and the picture is missing, switch it to Software or Auto.</li>
          <li><strong>RGB device missing</strong>: Rescan; for native drivers check the udev rules; for GPUs load <Code>i2c-dev</Code> (<Code>sudo modprobe i2c-dev</Code>); for everything else make sure OpenRGB sees it.</li>
          <li><strong>High CPU use</strong>: lower the screen frame rate, avoid 4K videos as backgrounds (they are decoded in full and then scaled), or enable video hardware decoding.</li>
          <li><strong>Logs</strong>: run <Code>NERU_LOG=debug neru-screen-control</Code> from a terminal and attach the output to a bug report.</li>
        </ul>
      ),
    },
  ];

  return (
    <div className="page stack" style={{ maxWidth: 960 }}>
      <div className="page-head">
        <div>
          <div className="eyebrow">// help</div>
          <h1 className="page-title">How to use Neru</h1>
          <p className="page-sub">Screens, lighting and fans — the short version. The full docs live on GitHub.</p>
        </div>
        <a className="btn" href={DOCS} target="_blank" rel="noreferrer">
          Docs &amp; source <ExternalLink size={14} />
        </a>
      </div>
      <div className="help-layout">
        <nav className="help-nav">
          {topics.map((t) => (
            <button key={t.id} className={`nav-item ${open === t.id ? "active" : ""}`} onClick={() => setOpen(t.id)}>
              <t.icon size={16} /> {t.title}
            </button>
          ))}
        </nav>
        <div className="card help-body">
          {topics
            .filter((t) => t.id === open)
            .map((t) => (
              <div key={t.id}>
                <h2 className="help-title">
                  <t.icon size={20} /> {t.title}
                </h2>
                {t.body}
              </div>
            ))}
        </div>
      </div>
    </div>
  );
}
