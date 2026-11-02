//! `neru` — hardware test tool for Neru Screen Control.
//!
//! ```text
//! neru devices                 list screens found on the system
//! neru probe                   handshake with the first screen and print its status
//! neru show photo.png          show an image (GIFs animate) until Ctrl-C
//! neru test                    show a test pattern
//! neru brightness 60           set hardware brightness (Thermaltake)
//! neru rotate 90               rotate (hardware on Thermaltake, software on Thermalright)
//! neru metrics --watch         print CPU / GPU / RAM readings
//! neru udev [--install]        print (or install) the udev rules
//! ```

use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{bail, Context};
use clap::{Parser, Subcommand};
use image::{AnimationDecoder, RgbImage};

use neru_core::metrics::Sources;
use neru_core::screen::{self, Screen};

#[derive(Parser)]
#[command(name = "neru", version, about = "Neru Screen Control — hardware test tool")]
struct Cli {
    /// Pin a screen model id (see `neru devices`)
    #[arg(long, global = true)]
    model: Option<String>,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List supported screens found on this system
    Devices,
    /// Connect to the screen and print what it reports
    Probe,
    /// Show an image file (PNG/JPEG/WebP/BMP; GIFs animate) until Ctrl-C
    Show {
        file: PathBuf,
        /// Re-send rate for still images (Thermaltake screens need ≥ 1 fps)
        #[arg(long, default_value_t = 2.0)]
        fps: f32,
        /// Stop after this many seconds (default: until Ctrl-C)
        #[arg(long)]
        hold: Option<f32>,
        /// JPEG quality for Thermaltake screens
        #[arg(long, default_value_t = 85)]
        quality: u8,
    },
    /// Show a test pattern (corner markers + gradient) until Ctrl-C
    Test {
        #[arg(long)]
        hold: Option<f32>,
    },
    /// Set hardware brightness, 0–100
    Brightness { value: u8 },
    /// Rotate the picture: 0, 90, 180 or 270
    Rotate { degrees: u16 },
    /// Print CPU / GPU / RAM metrics
    Metrics {
        /// Keep printing every second
        #[arg(long)]
        watch: bool,
        #[arg(long)]
        json: bool,
    },
    /// Print the udev rules (or install them with --install, asks for your password)
    Udev {
        #[arg(long)]
        install: bool,
    },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("NERU_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();

    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Devices => devices(),
        Cmd::Probe => probe(cli.model.as_deref()),
        Cmd::Show { file, fps, hold, quality } => show(cli.model.as_deref(), &file, fps, hold, quality),
        Cmd::Test { hold } => test_pattern(cli.model.as_deref(), hold),
        Cmd::Brightness { value } => {
            let mut s = open(cli.model.as_deref())?;
            s.connect()?;
            s.set_brightness(value)?;
            println!("brightness set to {value}");
            Ok(())
        }
        Cmd::Rotate { degrees } => {
            let mut s = open(cli.model.as_deref())?;
            s.connect()?;
            s.set_rotation(degrees)?;
            println!("rotation set to {degrees}°");
            Ok(())
        }
        Cmd::Metrics { watch, json } => metrics(watch, json),
        Cmd::Udev { install } => udev(install),
    }
}

fn devices() -> anyhow::Result<()> {
    let found = screen::detect();
    if found.is_empty() {
        println!("No supported screen found.\n\nKnown models:");
        for m in screen::MODELS {
            let (vid, pid) = m.transport.vid_pid();
            println!("  {:<22} {} {} [{vid:04x}:{pid:04x}]", m.id, m.vendor, m.name);
        }
        println!("\nCheck `lsusb` and the udev rules (`neru udev`).");
        return Ok(());
    }
    for d in &found {
        println!("{:<22} {d}", d.model.id);
    }
    Ok(())
}

fn open(model: Option<&str>) -> anyhow::Result<Box<dyn Screen>> {
    Ok(screen::open_first(model).with_context(|| "could not open a screen (try `neru devices`)")?)
}

fn probe(model: Option<&str>) -> anyhow::Result<()> {
    let mut s = open(model)?;
    let m = s.model().clone();
    let (vid, pid) = m.transport.vid_pid();
    println!("screen:   {} {} [{vid:04x}:{pid:04x}] {}×{}", m.vendor, m.name, m.width, m.height);
    let status = s.connect()?;
    println!("firmware: {}", status.firmware.as_deref().unwrap_or("-"));
    println!("serial:   {}", status.serial.as_deref().unwrap_or("-"));
    println!("bright:   {}", status.brightness.map_or("-".into(), |b| b.to_string()));
    println!("rotation: {}", status.rotation.map_or("-".into(), |r| format!("{r}°")));
    println!("idle:     {}", status.idle_timeout_s.map_or("-".into(), |t| format!("{t} s")));
    if let Some(raw) = status.raw {
        println!("raw:      {}", serde_json::to_string(&raw)?);
    }
    Ok(())
}

/// Resize with "cover" semantics (fill the screen, crop the overflow).
fn fit_cover(img: &RgbImage, w: u32, h: u32) -> RgbImage {
    if img.width() == w && img.height() == h {
        return img.clone();
    }
    let scale = (w as f32 / img.width() as f32).max(h as f32 / img.height() as f32);
    let sw = (img.width() as f32 * scale).round().max(1.0) as u32;
    let sh = (img.height() as f32 * scale).round().max(1.0) as u32;
    let scaled = image::imageops::resize(img, sw, sh, image::imageops::FilterType::Lanczos3);
    let x = (sw - w) / 2;
    let y = (sh - h) / 2;
    image::imageops::crop_imm(&scaled, x, y, w, h).to_image()
}

fn ctrl_c_flag() -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    let f = flag.clone();
    let _ = ctrlc::set_handler(move || f.store(true, Ordering::Relaxed));
    flag
}

/// Push frames in a loop. `next` returns (frame, how long to show it).
fn run_loop(
    s: &mut dyn Screen,
    hold: Option<f32>,
    mut next: impl FnMut() -> (RgbImage, Duration),
) -> anyhow::Result<()> {
    let stop = ctrl_c_flag();
    let end = hold.map(|h| Instant::now() + Duration::from_secs_f32(h));
    let mut sent = 0u64;
    let started = Instant::now();
    while !stop.load(Ordering::Relaxed) && end.map_or(true, |e| Instant::now() < e) {
        let (frame, delay) = next();
        let t0 = Instant::now();
        s.show(&frame)?;
        sent += 1;
        if sent == 1 {
            eprintln!("first frame sent in {:.1} ms", t0.elapsed().as_secs_f64() * 1000.0);
        }
        let spent = t0.elapsed();
        if delay > spent {
            std::thread::sleep(delay - spent);
        }
    }
    eprintln!(
        "{sent} frames in {:.1} s ({:.1} fps)",
        started.elapsed().as_secs_f64(),
        sent as f64 / started.elapsed().as_secs_f64().max(0.001)
    );
    Ok(())
}

fn show(model: Option<&str>, file: &PathBuf, fps: f32, hold: Option<f32>, quality: u8) -> anyhow::Result<()> {
    let mut s = open(model)?;
    let (w, h) = (s.model().width, s.model().height);
    s.connect()?;
    s.set_jpeg_quality(quality);

    let is_gif = file.extension().map_or(false, |e| e.eq_ignore_ascii_case("gif"));
    if is_gif {
        let mut bytes = Vec::new();
        std::fs::File::open(file)?.read_to_end(&mut bytes)?;
        let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(bytes))?;
        let frames: Vec<(RgbImage, Duration)> = decoder
            .into_frames()
            .collect_frames()
            .context("decoding GIF")?
            .into_iter()
            .map(|f| {
                let (num, den) = f.delay().numer_denom_ms();
                let ms = if den == 0 { 100 } else { (num / den).max(20) };
                (fit_cover(&image::DynamicImage::ImageRgba8(f.into_buffer()).to_rgb8(), w, h), Duration::from_millis(ms as u64))
            })
            .collect();
        if frames.is_empty() {
            bail!("GIF has no frames");
        }
        eprintln!("{} GIF frames, showing on {w}×{h} (Ctrl-C to stop)", frames.len());
        let mut i = 0usize;
        return run_loop(s.as_mut(), hold, move || {
            let f = frames[i % frames.len()].clone();
            i += 1;
            f
        });
    }

    let img = image::open(file).with_context(|| format!("opening {}", file.display()))?.to_rgb8();
    let frame = fit_cover(&img, w, h);
    eprintln!("showing {} on {w}×{h} at {fps} fps (Ctrl-C to stop)", file.display());
    let delay = Duration::from_secs_f32(1.0 / fps.max(0.1));
    run_loop(s.as_mut(), hold, move || (frame.clone(), delay))
}

fn test_pattern(model: Option<&str>, hold: Option<f32>) -> anyhow::Result<()> {
    let mut s = open(model)?;
    let (w, h) = (s.model().width, s.model().height);
    s.connect()?;
    let frame = pattern(w, h);
    eprintln!("test pattern on {w}×{h}: red=top-left, green=top-right, blue=bottom-left, yellow=bottom-right");
    run_loop(s.as_mut(), hold, move || (frame.clone(), Duration::from_millis(500)))
}

/// Corner markers + horizontal gradient + white border, so orientation,
/// cropping and colour order are all visible at a glance.
fn pattern(w: u32, h: u32) -> RgbImage {
    let mut img = RgbImage::from_fn(w, h, |x, _| {
        let t = x as f32 / w.max(1) as f32;
        image::Rgb([(t * 255.0) as u8, 32, (255.0 - t * 255.0) as u8])
    });
    let m = (w.min(h) / 5).max(8);
    let corners = [
        (0, 0, [255, 0, 0]),
        (w - m, 0, [0, 255, 0]),
        (0, h - m, [0, 0, 255]),
        (w - m, h - m, [255, 255, 0]),
    ];
    for (cx, cy, c) in corners {
        for y in cy..cy + m {
            for x in cx..cx + m {
                img.put_pixel(x, y, image::Rgb(c));
            }
        }
    }
    let border = (w.min(h) / 60).max(2);
    for y in 0..h {
        for x in 0..w {
            if x < border || y < border || x >= w - border || y >= h - border {
                img.put_pixel(x, y, image::Rgb([255, 255, 255]));
            }
        }
    }
    // centre cross
    for i in 0..w.min(h) / 4 {
        let (cx, cy) = (w / 2, h / 2);
        for d in 0..2u32 {
            img.put_pixel((cx + i).min(w - 1), (cy + d).min(h - 1), image::Rgb([255, 255, 255]));
            img.put_pixel(cx.saturating_sub(i), (cy + d).min(h - 1), image::Rgb([255, 255, 255]));
            img.put_pixel((cx + d).min(w - 1), (cy + i).min(h - 1), image::Rgb([255, 255, 255]));
            img.put_pixel((cx + d).min(w - 1), cy.saturating_sub(i), image::Rgb([255, 255, 255]));
        }
    }
    img
}

fn metrics(watch: bool, json: bool) -> anyhow::Result<()> {
    let mut src = Sources::new();
    if !json {
        for line in src.describe() {
            eprintln!("{line}");
        }
    }
    let _ = src.read();
    std::thread::sleep(Duration::from_millis(500));
    let stop = ctrl_c_flag();
    loop {
        let snap = src.read();
        if json {
            println!("{}", serde_json::to_string(&snap)?);
        } else {
            let f = |v: Option<f32>, unit: &str| v.map_or("  -".to_string(), |v| format!("{v:.0}{unit}"));
            println!(
                "CPU {:>5} {:>5} {:>8} {:>5}   RAM {:>5} ({:.0}/{:.0} MB){}",
                f(snap.cpu.temp_c, "°C"),
                f(snap.cpu.usage_pct, "%"),
                f(snap.cpu.freq_mhz, " MHz"),
                f(snap.cpu.power_w, " W"),
                format!("{:.0}%", snap.mem.pct),
                snap.mem.used_mb,
                snap.mem.total_mb,
                snap.gpu.as_ref().map_or(String::new(), |g| format!(
                    "   GPU {:>5} {:>5} {:>8} {:>5}  VRAM {}/{} MB",
                    f(g.temp_c, "°C"),
                    f(g.usage_pct, "%"),
                    f(g.core_mhz, " MHz"),
                    f(g.power_w, " W"),
                    g.vram_used_mb.map_or("-".into(), |v| format!("{v:.0}")),
                    g.vram_total_mb.map_or("-".into(), |v| format!("{v:.0}")),
                ))
            );
        }
        if !watch || stop.load(Ordering::Relaxed) {
            break;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    Ok(())
}

const UDEV_FILE: &str = "/etc/udev/rules.d/70-neru-screen-control.rules";

fn udev(install: bool) -> anyhow::Result<()> {
    let rules = screen::udev_rules();
    if !install {
        print!("{rules}");
        eprintln!("\nInstall with:  neru udev --install   (or copy the text above to {UDEV_FILE})");
        return Ok(());
    }
    let script = format!(
        "set -e; cat > {UDEV_FILE} <<'EOF'\n{rules}EOF\nudevadm control --reload-rules && udevadm trigger"
    );
    let status = std::process::Command::new("pkexec")
        .args(["sh", "-c", &script])
        .status()
        .context("running pkexec (is polkit installed?)")?;
    if !status.success() {
        bail!("installation failed (status {status})");
    }
    println!("installed {UDEV_FILE}. Unplug and replug the screen's USB cable (or reboot).");
    Ok(())
}
