#!/bin/sh
# Neru Screen Control — installer for any Linux distro (x86_64).
#
#   curl -fsSL https://raw.githubusercontent.com/Huzzama/Neru-screen-control/main/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/Huzzama/Neru-screen-control/main/install.sh | sh -s -- --uninstall
#
# Installs the latest release AppImage for the current user (no root needed):
#   ~/.local/share/neru-screen-control/neru-screen-control.AppImage
#   ~/.local/bin/neru-screen-control                 (launcher)
#   ~/.local/share/applications/neru-screen-control.desktop
#   ~/.local/share/icons/hicolor/128x128/apps/neru-screen-control.png
# and, with your password, the udev rule that lets you open the USB / I2C devices:
#   /etc/udev/rules.d/70-neru-screen-control.rules
#   /etc/modules-load.d/neru-screen-control.conf   (loads i2c-dev for GPU RGB)
set -eu

REPO="Huzzama/Neru-screen-control"
BRANCH="${NERU_BRANCH:-main}"
APP="neru-screen-control"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
BIN="$HOME/.local/bin"
DIR="$DATA/$APP"
RULES="/etc/udev/rules.d/70-neru-screen-control.rules"
MODULES="/etc/modules-load.d/neru-screen-control.conf"
RAW="https://raw.githubusercontent.com/$REPO/$BRANCH"

say()  { printf '\033[1;33m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;31m!!\033[0m %s\n' "$*" >&2; }
die()  { warn "$*"; exit 1; }

fetch() { # url dest
    if command -v curl >/dev/null 2>&1; then curl -fL --progress-bar -o "$2" "$1"
    elif command -v wget >/dev/null 2>&1; then wget -q --show-progress -O "$2" "$1"
    else die "curl or wget is required"; fi
}
fetch_text() { # url
    if command -v curl >/dev/null 2>&1; then curl -fsSL "$1"; else wget -qO- "$1"; fi
}
as_root() {
    if [ "$(id -u)" = 0 ]; then "$@"
    elif command -v sudo >/dev/null 2>&1; then sudo "$@"
    elif command -v doas >/dev/null 2>&1; then doas "$@"
    else return 1; fi
}

uninstall() {
    say "Removing Neru Screen Control"
    pkill -f "$DIR/$APP.AppImage" 2>/dev/null || true
    rm -rf "$DIR"
    rm -f "$BIN/$APP" "$DATA/applications/$APP.desktop" "$DATA/icons/hicolor/128x128/apps/$APP.png"
    rm -f "${XDG_CONFIG_HOME:-$HOME/.config}/autostart/$APP.desktop"
    if [ -f "$RULES" ]; then
        say "Removing $RULES (asks for your password)"
        as_root rm -f "$RULES" "$MODULES" && as_root udevadm control --reload-rules || warn "could not remove $RULES"
    fi
    say "Done. Your themes and settings stay in ~/.config/$APP (delete it to reset)."
    exit 0
}

[ "${1:-}" = "--uninstall" ] && uninstall

[ "$(uname -s)" = Linux ] || die "Neru Screen Control runs on Linux only"
[ "$(uname -m)" = x86_64 ] || die "only x86_64 builds are published (you have $(uname -m))"

say "Looking up the latest release of $REPO"
JSON=$(fetch_text "https://api.github.com/repos/$REPO/releases/latest") || die "could not reach GitHub"
URL=$(printf '%s' "$JSON" | grep -o '"browser_download_url": *"[^"]*\.AppImage"' | head -n1 | sed 's/.*"\(https[^"]*\)"/\1/')
TAG=$(printf '%s' "$JSON" | grep -o '"tag_name": *"[^"]*"' | head -n1 | sed 's/.*"\([^"]*\)"$/\1/')
[ -n "$URL" ] || die "the latest release has no AppImage"

say "Downloading $TAG"
mkdir -p "$DIR" "$BIN" "$DATA/applications" "$DATA/icons/hicolor/128x128/apps"
fetch "$URL" "$DIR/$APP.AppImage.part"
mv "$DIR/$APP.AppImage.part" "$DIR/$APP.AppImage"
chmod +x "$DIR/$APP.AppImage"
printf '%s\n' "$TAG" > "$DIR/version"

# AppImages mount themselves with FUSE 2. Without libfuse2 they can still run
# by extracting to a temp dir first, so the launcher falls back to that.
cat > "$BIN/$APP" <<EOF
#!/bin/sh
APPIMAGE_FILE="$DIR/$APP.AppImage"
if ! ldconfig -p 2>/dev/null | grep -q 'libfuse.so.2' && [ ! -e /usr/lib/libfuse.so.2 ] && [ ! -e /usr/lib64/libfuse.so.2 ]; then
    export APPIMAGE_EXTRACT_AND_RUN=1
fi
# webkit2gtk + NVIDIA on Wayland can render a blank window without this.
export WEBKIT_DISABLE_DMABUF_RENDERER=\${WEBKIT_DISABLE_DMABUF_RENDERER:-1}
exec "\$APPIMAGE_FILE" "\$@"
EOF
chmod +x "$BIN/$APP"

fetch "$RAW/src-tauri/icons/128x128.png" "$DATA/icons/hicolor/128x128/apps/$APP.png" 2>/dev/null || warn "icon not downloaded"
cat > "$DATA/applications/$APP.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Neru Screen Control
GenericName=Cooler screen, RGB and fan control
Comment=Cooler LCD screens, PC RGB lighting and fans
Exec=$BIN/$APP
Icon=$APP
Terminal=false
Categories=Utility;System;HardwareSettings;
Keywords=LCD;cooler;thermaltake;thermalright;RGB;fans;
StartupWMClass=$APP
EOF
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$DATA/applications" >/dev/null 2>&1 || true

say "Installing the USB access rule (asks for your password; skip with Ctrl-C)"
TMP=$(mktemp)
if fetch_text "$RAW/packaging/shared/70-neru-screen-control.rules" > "$TMP" && \
   as_root install -m 644 "$TMP" "$RULES" && \
   as_root udevadm control --reload-rules && as_root udevadm trigger; then
    say "Rule installed — unplug and replug the cooler's USB cable (or reboot)"
    # GPU RGB controllers sit on the graphics card's I2C bus: needs i2c-dev.
    printf '# Neru Screen Control: I2C access for GPU / DRAM RGB controllers\ni2c-dev\ni2c-piix4\ni2c-i801\n' > "$TMP"
    as_root install -m 644 "$TMP" "$MODULES" 2>/dev/null && { as_root modprobe i2c-dev 2>/dev/null; as_root modprobe i2c-piix4 2>/dev/null; as_root modprobe i2c-i801 2>/dev/null; true; } || warn "i2c-dev not loaded (GPU / RAM RGB needs it): sudo modprobe i2c-dev"
else
    warn "rule not installed; do it later from the app: Settings → Device access → Install rules"
fi
rm -f "$TMP"

case ":$PATH:" in *":$BIN:"*) ;; *) warn "$BIN is not in your PATH — start Neru from the app menu, or add it to PATH";; esac
if ! ldconfig -p 2>/dev/null | grep -q 'libfuse.so.2'; then
    warn "libfuse2 not found: Neru will still run (extract mode, slower start). Install fuse2 / libfuse2 for faster starts."
fi
say "Installed $TAG. Run: $APP   (or find \"Neru Screen Control\" in your app menu)"
say "Update: run this installer again.  Remove: … | sh -s -- --uninstall"
