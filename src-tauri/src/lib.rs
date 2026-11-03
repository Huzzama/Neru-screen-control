//! Neru Screen Control desktop app (Tauri 2).
//!
//! One process: the render engine (neru-core) runs on its own thread, the
//! webview shows the React UI, and a tray icon keeps the app alive when the
//! window is hidden. `--background` starts with the window hidden.

mod commands;
mod state;

use std::sync::atomic::Ordering;

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tracing::{error, info, warn};

use state::AppState;

/// Command-line flags the app understands.
#[derive(Debug, Clone, Default)]
pub struct Flags {
    /// Start without showing the window (tray only).
    pub background: bool,
}

fn parse_flags(args: impl Iterator<Item = String>) -> Flags {
    let mut f = Flags::default();
    for a in args {
        match a.as_str() {
            "--background" | "--hidden" | "-b" => f.background = true,
            "--help" | "-h" => {
                println!(
                    "neru-screen-control [--background]\n\n  --background   start hidden in the tray (what the autostart entry uses)\n\nSet NERU_LOG=debug for verbose logs."
                );
                std::process::exit(0);
            }
            "--version" | "-V" => {
                println!("neru-screen-control {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            _ => {}
        }
    }
    f
}

pub fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn init_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_env("NERU_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,tauri=warn,wry=warn"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();
}

/// WebKit's GPU (DMA-BUF) rendering path is off unless the user turned it on
/// in Settings: NVIDIA + Wayland can produce a blank window with it. Must be
/// decided before the first webview exists, so the config is read here too.
fn apply_window_renderer_setting() {
    if std::env::var_os("NERU_KEEP_WEBKIT_ENV").is_some() {
        return;
    }
    let paths = neru_core::config::Paths::default_paths();
    let gpu_window = neru_core::config::AppConfig::load(&paths.config_file).map(|c| c.app.gpu_window).unwrap_or(false);
    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", if gpu_window { "0" } else { "1" });
}

pub fn run() {
    init_logging();
    let flags = parse_flags(std::env::args().skip(1));
    apply_window_renderer_setting();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            info!("second instance launched; showing window");
            show_main_window(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            let handle = app.handle().clone();

            // Where bundled fonts/themes live when installed.
            if let Ok(res) = app.path().resource_dir() {
                let assets = res.join("assets");
                if assets.join("fonts").is_dir() {
                    neru_core::render::set_assets_dir(assets);
                }
            }

            let state = AppState::new(handle.clone(), flags.background)?;
            app.manage(state);

            build_tray(&handle)?;

            if !flags.background {
                show_main_window(&handle);
            } else {
                info!("started in background mode (tray only)");
            }

            // Status ticker for the UI.
            let h = handle.clone();
            std::thread::Builder::new()
                .name("neru-status".into())
                .spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_millis(1000));
                    let Some(state) = h.try_state::<AppState>() else { continue };
                    let status = state.status();
                    if let Some(s) = status.screen.as_ref().filter(|_| status.connected) {
                        state.auto_theme_for_screen(s.width, s.height);
                    }
                    let _ = h.emit("status", &status);
                    state.update_tray(&h, &status);
                    state.maybe_start_rgb_server();
                    let _ = h.emit("rgb-status", &state.rgb_status());
                })
                .expect("spawn status thread");
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppState>();
                if state.quitting.load(Ordering::Relaxed) || !state.minimize_on_close() {
                    return;
                }
                api.prevent_close();
                let _ = window.hide();
                info!("window hidden (still running in the tray)");
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::get_metrics,
            commands::get_config,
            commands::set_config,
            commands::list_themes,
            commands::load_theme,
            commands::save_theme,
            commands::create_theme,
            commands::duplicate_theme,
            commands::delete_theme,
            commands::apply_theme,
            commands::preview_theme,
            commands::set_preview,
            commands::set_pause,
            commands::reconnect,
            commands::detect_screens,
            commands::list_fonts,
            commands::list_media,
            commands::import_media,
            commands::delete_media,
            commands::udev_status,
            commands::install_udev,
            commands::autostart_get,
            commands::autostart_set,
            commands::app_info,
            commands::metric_keys,
            commands::metric_history,
            commands::theme_thumbnail,
            commands::rgb_status,
            commands::rgb_set_assignment,
            commands::rgb_apply_all,
            commands::rgb_clear_all,
            commands::rgb_refresh,
            commands::rgb_set_zone_size,
            commands::rgb_rescan_native,
            commands::rgb_rescan,
            commands::rgb_profile,
            commands::rgb_save_to_device,
            commands::rgb_start_server,
            commands::fans_status,
            commands::fans_set_config,
            commands::fans_release,
            commands::fans_install_helper,
            commands::fans_uninstall_helper,
            commands::quit_app,
        ])
        .build(tauri::generate_context!())
        .expect("error while building the app")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                // Closing the last window must not exit while we live in the tray.
                let quitting = app.state::<AppState>().quitting.load(Ordering::Relaxed);
                if code.is_none() && !quitting {
                    api.prevent_exit();
                }
            }
        });
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Neru Screen Control", true, None::<&str>)?;
    let pause = CheckMenuItem::with_id(app, "pause", "Pause screen", true, false, None::<&str>)?;
    let status = MenuItem::with_id(app, "status", "No screen connected", false, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&open, &PredefinedMenuItem::separator(app)?, &status, &pause, &PredefinedMenuItem::separator(app)?, &quit],
    )?;

    {
        let state = app.state::<AppState>();
        *state.tray_items.lock().unwrap() = Some(state::TrayItems { pause: pause.clone(), status: status.clone() });
    }

    let icon = app.default_window_icon().cloned();
    let mut builder = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("Neru Screen Control")
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main_window(app),
            "pause" => {
                let state = app.state::<AppState>();
                let paused = !state.status().paused;
                state.set_pause(paused);
            }
            "quit" => {
                let state = app.state::<AppState>();
                state.quitting.store(true, Ordering::Relaxed);
                state.shutdown();
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = icon {
        builder = builder.icon(icon);
    } else {
        warn!("no window icon for the tray");
    }
    if let Err(e) = builder.build(app) {
        error!(error = %e, "tray icon unavailable (is libayatana-appindicator installed?)");
    }
    Ok(())
}
