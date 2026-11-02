//! Start on login via an XDG autostart entry
//! (`~/.config/autostart/neru-screen-control.desktop`), which every desktop
//! honours and which runs inside the graphical session (needed for the tray
//! icon and the webview).

use std::fs;
use std::path::PathBuf;

use crate::Result;

pub const DESKTOP_FILE: &str = "neru-screen-control.desktop";

pub fn autostart_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("autostart")
}

pub fn entry_path() -> PathBuf {
    autostart_dir().join(DESKTOP_FILE)
}

pub fn is_enabled() -> bool {
    let p = entry_path();
    match fs::read_to_string(&p) {
        Ok(s) => !s.lines().any(|l| l.trim() == "Hidden=true"),
        Err(_) => false,
    }
}

/// `exec` is the command to run (normally the current executable's path).
pub fn set_enabled(enabled: bool, exec: &str) -> Result<()> {
    let p = entry_path();
    if !enabled {
        if p.exists() {
            fs::remove_file(&p)?;
        }
        return Ok(());
    }
    fs::create_dir_all(autostart_dir())?;
    let contents = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Neru Screen Control\n\
         Comment=Cooler LCD screens, PC RGB lighting and fans\n\
         Exec={exec} --background\n\
         Icon=neru-screen-control\n\
         Terminal=false\n\
         X-GNOME-Autostart-enabled=true\n\
         X-KDE-autostart-after=panel\n"
    );
    fs::write(&p, contents)?;
    Ok(())
}
