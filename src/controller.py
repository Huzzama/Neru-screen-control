"""
Neru Screen Control launcher — three execution modes:

  python main.py              → --background (headless, keeps display updating)
  python main.py --ui         → full GUI window
  python main.py --background → explicit headless mode
  python main.py --ui --hidden → GUI starts but window is hidden (tray only)
"""

import argparse
import time

from config.loader import Config
from metrics.collector import MetricsCollector
from display.collector import DisplayController
from ui.main_window import launch_ui


def run_background(config_path: str) -> None:
    """
    Headless background mode — no window, just the update loop.
    This is what the systemd user service runs.
    """
    cfg = Config(config_path)

    metrics = MetricsCollector(
        interval = cfg.get("metrics_interval", 1.0),
        cpu_unit = cfg.get("cpu_temperature_unit", "celsius"),
        gpu_unit = cfg.get("gpu_temperature_unit", "celsius"),
    )
    metrics.start()

    display = DisplayController(metrics, cfg.as_dict())
    display.start()

    print("[NSC] Background service running. Press Ctrl+C to stop.")

    try:
        while True:
            time.sleep(1)
    except KeyboardInterrupt:
        print("\n[NSC] Stopping…")
    finally:
        display.stop()
        metrics.stop()


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Neru Screen Control",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  python main.py              # headless background mode (default)
  python main.py --ui         # open the GUI
  python main.py --ui --hidden  # GUI starts hidden (tray icon only)
  python main.py --background # explicit headless mode
        """
    )
    parser.add_argument(
        "--ui", action="store_true",
        help="Launch the graphical interface.")
    parser.add_argument(
        "--background", action="store_true",
        help="Run headlessly in the background (default if no flag given).")
    parser.add_argument(
        "--hidden", action="store_true",
        help="With --ui: start with the window hidden (tray icon only).")
    parser.add_argument(
        "--config", default="config.json",
        help="Path to config file (default: config.json).")

    args = parser.parse_args()

    if args.ui:
        launch_ui(args.config, start_hidden=args.hidden)
    else:
        # --background or no flag → headless
        run_background(args.config)