//! Renders the bundled themes with made-up metrics and checks that something
//! sensible came out (fonts found, text drawn, bars filled).

use std::path::PathBuf;

use neru_core::metrics::{CpuMetrics, GpuMetrics, MemMetrics, Snapshot};
use neru_core::render::{load_theme, FontBook, Renderer};

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

fn snapshot() -> Snapshot {
    Snapshot {
        cpu: CpuMetrics { temp_c: Some(70.0), usage_pct: Some(50.0), freq_mhz: Some(4000.0), power_w: Some(60.0), ..Default::default() },
        gpu: Some(GpuMetrics { temp_c: Some(60.0), usage_pct: Some(80.0), vram_used_mb: Some(4000.0), vram_total_mb: Some(8000.0), ..Default::default() }),
        mem: MemMetrics { used_mb: 8000.0, total_mb: 16000.0, pct: 50.0 },
        ..Default::default()
    }
}

#[test]
fn bundled_themes_render() {
    let fonts = FontBook::load(Some(&assets().join("fonts")));
    assert!(fonts.families().iter().any(|f| f == "Inter"), "bundled fonts not loaded");

    for name in ["neru-720.json", "neru-320.json"] {
        let theme = load_theme(&assets().join("themes").join(name)).unwrap();
        let (w, h) = (theme.width, theme.height);
        let mut r = Renderer::new(theme, fonts.db.clone(), None).unwrap();
        let frame = r.render(&snapshot()).unwrap();
        assert_eq!((frame.width(), frame.height()), (w, h));
        // Some bright (text) pixels and some accent pixels must exist.
        let bright = frame.pixels().filter(|p| p[0] > 150 || p[1] > 150 || p[2] > 150).count();
        let yellow = frame.pixels().filter(|p| p[0] > 200 && p[1] > 150 && p[2] < 80).count();
        assert!(bright > 500, "{name}: no text drawn ({bright} bright px)");
        assert!(yellow > 100, "{name}: no accent drawn ({yellow} yellow px)");
        assert!(r.media_errors().is_empty());
    }
}

#[test]
fn theme_scales_to_other_sizes() {
    let theme = load_theme(&assets().join("themes/neru-720.json")).unwrap();
    let small = Renderer::scale_theme(&theme, 320, 320);
    assert_eq!((small.width, small.height), (320, 320));
    let fonts = FontBook::load(Some(&assets().join("fonts")));
    let mut r = Renderer::new(small, fonts.db.clone(), None).unwrap();
    let frame = r.render(&snapshot()).unwrap();
    assert_eq!(frame.width(), 320);
}
