from PIL import Image, ImageDraw, ImageFont
import os

# Colors (RGB tuples — encode_frame handles GRB conversion)
BG_COLOR       = (10, 10, 25)
CPU_COLOR      = (0, 180, 255)    # cyan-blue
GPU_COLOR      = (255, 100, 0)    # orange
TEXT_COLOR     = (220, 220, 220)
LABEL_COLOR    = (120, 120, 120)
BAR_BG_COLOR   = (40, 40, 60)
WARN_COLOR     = (255, 200, 0)
HOT_COLOR      = (255, 50, 50)

W, H = 320, 320


def _get_font(size):
    """Try to load a monospace font, fall back to default."""
    candidates = [
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationMono-Bold.ttf",
        "/usr/share/fonts/TTF/DejaVuSansMono-Bold.ttf",
        "/usr/share/fonts/dejavu/DejaVuSansMono-Bold.ttf",
    ]
    for path in candidates:
        if os.path.exists(path):
            try:
                return ImageFont.truetype(path, size)
            except Exception:
                continue
    return ImageFont.load_default()


# Pre-load fonts
FONT_LARGE  = _get_font(64)
FONT_MEDIUM = _get_font(32)
FONT_SMALL  = _get_font(20)
FONT_TINY   = _get_font(14)


def _temp_color(temp, max_temp=90):
    """Return color based on temperature: cool=cyan → warm=yellow → hot=red."""
    factor = min(1.0, max(0.0, (temp - 30) / (max_temp - 30)))
    if factor < 0.5:
        # cyan → yellow
        f = factor * 2
        return (int(255 * f), int(180 + 75 * (1 - f)), int(255 * (1 - f)))
    else:
        # yellow → red
        f = (factor - 0.5) * 2
        return (255, int(200 * (1 - f)), 0)


def _draw_bar(d, x, y, w, h, value, max_val=100, color=CPU_COLOR):
    """Draw a filled progress bar."""
    d.rectangle([x, y, x + w, y + h], fill=BAR_BG_COLOR)
    fill_w = int(w * min(1.0, value / max_val))
    if fill_w > 0:
        d.rectangle([x, y, x + fill_w, y + h], fill=color)


def build_metrics_frame(metrics: dict, config: dict = None) -> Image.Image:
    """
    Build a 320x320 metrics display frame.

    Args:
        metrics: dict with keys:
            cpu_temp, gpu_temp, cpu_usage, gpu_usage,
            cpu_frequency, gpu_frequency, cpu_power, gpu_power
        config: optional dict with cpu_max_temp, gpu_max_temp etc.

    Returns:
        PIL Image 320x320 RGB
    """
    if config is None:
        config = {}

    cpu_max_temp = config.get('cpu_max_temp', 90)
    gpu_max_temp = config.get('gpu_max_temp', 90)

    img = Image.new('RGB', (W, H), BG_COLOR)
    d = ImageDraw.Draw(img)

    # ── Header bar ──────────────────────────────────────────
    d.rectangle([0, 0, W, 36], fill=(20, 20, 45))
    d.text((10, 8), "CPU", font=FONT_SMALL, fill=CPU_COLOR)
    d.text((W // 2 + 10, 8), "GPU", font=FONT_SMALL, fill=GPU_COLOR)
    d.line([(0, 36), (W, 36)], fill=(50, 50, 80), width=1)
    d.line([(W // 2, 0), (W // 2, 36)], fill=(50, 50, 80), width=1)

    # ── CPU side (left half) ─────────────────────────────────
    cpu_temp    = metrics.get('cpu_temp', 0)
    cpu_usage   = metrics.get('cpu_usage', 0)
    cpu_freq    = metrics.get('cpu_frequency', 0)
    cpu_power   = metrics.get('cpu_power', 0)

    cpu_temp_col = _temp_color(cpu_temp, cpu_max_temp)

    # Temperature (big number)
    d.text((10, 48), f"{cpu_temp}°", font=FONT_LARGE, fill=cpu_temp_col)

    # Usage bar + label
    d.text((10, 122), "LOAD", font=FONT_TINY, fill=LABEL_COLOR)
    _draw_bar(d, 10, 138, 140, 14, cpu_usage, color=CPU_COLOR)
    d.text((10, 154), f"{cpu_usage}%", font=FONT_SMALL, fill=TEXT_COLOR)

    # Frequency
    d.text((10, 190), "FREQ", font=FONT_TINY, fill=LABEL_COLOR)
    freq_str = f"{cpu_freq/1000:.1f}G" if cpu_freq >= 1000 else f"{cpu_freq}M"
    d.text((10, 206), freq_str, font=FONT_SMALL, fill=CPU_COLOR)

    # Power
    d.text((10, 242), "PWR", font=FONT_TINY, fill=LABEL_COLOR)
    d.text((10, 258), f"{cpu_power}W", font=FONT_SMALL, fill=TEXT_COLOR)

    # ── Divider ──────────────────────────────────────────────
    d.line([(W // 2, 36), (W // 2, H)], fill=(50, 50, 80), width=1)

    # ── GPU side (right half) ────────────────────────────────
    gpu_temp    = metrics.get('gpu_temp', 0)
    gpu_usage   = metrics.get('gpu_usage', 0)
    gpu_freq    = metrics.get('gpu_frequency', 0)
    gpu_power   = metrics.get('gpu_power', 0)

    gpu_temp_col = _temp_color(gpu_temp, gpu_max_temp)

    ox = W // 2 + 10  # x offset for GPU side

    d.text((ox, 48), f"{gpu_temp}°", font=FONT_LARGE, fill=gpu_temp_col)

    d.text((ox, 122), "LOAD", font=FONT_TINY, fill=LABEL_COLOR)
    _draw_bar(d, ox, 138, 140, 14, gpu_usage, color=GPU_COLOR)
    d.text((ox, 154), f"{gpu_usage}%", font=FONT_SMALL, fill=TEXT_COLOR)

    d.text((ox, 190), "FREQ", font=FONT_TINY, fill=LABEL_COLOR)
    freq_str = f"{gpu_freq/1000:.1f}G" if gpu_freq >= 1000 else f"{gpu_freq}M"
    d.text((ox, 206), freq_str, font=FONT_SMALL, fill=GPU_COLOR)

    d.text((ox, 242), "PWR", font=FONT_TINY, fill=LABEL_COLOR)
    d.text((ox, 258), f"{gpu_power}W", font=FONT_SMALL, fill=TEXT_COLOR)

    # ── Bottom border ─────────────────────────────────────────
    d.line([(0, H - 2), (W, H - 2)], fill=(50, 50, 80), width=2)

    return img


def build_clock_frame(now=None) -> Image.Image:
    """Build a 320x320 clock display."""
    import datetime
    if now is None:
        now = datetime.datetime.now()

    img = Image.new('RGB', (W, H), BG_COLOR)
    d = ImageDraw.Draw(img)

    time_str = now.strftime("%H:%M")
    secs_str = now.strftime("%S")
    date_str = now.strftime("%Y-%m-%d")

    # Big time
    d.text((30, 100), time_str, font=FONT_LARGE, fill=(0, 200, 255))
    d.text((220, 130), secs_str, font=FONT_MEDIUM, fill=(100, 150, 200))
    d.text((60, 220), date_str, font=FONT_MEDIUM, fill=LABEL_COLOR)

    return img