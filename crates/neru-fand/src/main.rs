//! `neru-fand` — runs as root, applies fan curves, serves `/run/neru-fand.sock`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use clap::Parser;

use neru_core::fans::daemon;

#[derive(Parser)]
#[command(name = "neru-fand", version, about = "Neru Screen Control fan helper")]
struct Cli {
    #[arg(long, default_value = daemon::SOCKET_PATH)]
    socket: PathBuf,
    #[arg(long, default_value = daemon::CONFIG_PATH)]
    config: PathBuf,
    #[arg(long, default_value = neru_core::fans::DEFAULT_HWMON_ROOT)]
    hwmon: PathBuf,
    /// Print the systemd unit for this binary and exit
    #[arg(long)]
    print_unit: bool,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_env("NERU_LOG").unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")))
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();
    let cli = Cli::parse();
    if cli.print_unit {
        let exe = std::env::current_exe()?.display().to_string();
        print!("{}", daemon::unit_file(&exe));
        return Ok(());
    }
    let stop = Arc::new(AtomicBool::new(false));
    let s = stop.clone();
    ctrlc::set_handler(move || s.store(true, Ordering::Relaxed))?;
    daemon::serve(&cli.socket, &cli.config, cli.hwmon, stop)?;
    Ok(())
}
