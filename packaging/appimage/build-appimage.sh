#!/bin/bash
# Neru Screen Control — AppImage build script
# Run from the project root: bash packaging/appimage/build-appimage.sh
set -euo pipefail

APP="neru-screen-control"
VERSION="${NSC_VERSION:-$(python3 -c "import sys; sys.path.insert(0,'src'); exec(open('src/version.py').read()) if __import__('os').path.exists('src/version.py') else None; print(getattr(__import__('builtins'), '__version__', '0.1.0'))" 2>/dev/null || echo "0.1.0")}"
ARCH="$(uname -m)"
OUTPUT="${APP}-${VERSION}-${ARCH}.AppImage"

echo "==> Building $OUTPUT"

# 1. Clean and create AppDir
rm -rf AppDir
mkdir -p \
  AppDir/usr/bin \
  AppDir/usr/share/applications \
  AppDir/usr/share/icons/hicolor/256x256/apps \
  AppDir/usr/share/"$APP"

# 2. Copy application files
cp -r src                 AppDir/usr/share/"$APP"/
cp    main.py             AppDir/usr/share/"$APP"/
cp    requirements.txt    AppDir/usr/share/"$APP"/
cp    icon.png            AppDir/usr/share/icons/hicolor/256x256/apps/"$APP".png
cp    packaging/shared/"$APP".desktop  AppDir/usr/share/applications/
cp    packaging/shared/"$APP".desktop  AppDir/
cp    packaging/appimage/AppRun        AppDir/
chmod +x AppDir/AppRun

# 3. Bundle Python virtualenv (portable Python dependencies)
echo "==> Bundling Python environment..."
python3 -m venv AppDir/usr/share/"$APP"/venv
AppDir/usr/share/"$APP"/venv/bin/pip install \
  -r requirements.txt \
  --quiet \
  --no-cache-dir

# 4. Create /usr/bin wrapper
cat > AppDir/usr/bin/"$APP" << WRAPPER
#!/bin/bash
DIR="\$(dirname \$(readlink -f "\$0"))"
exec "\$DIR/../share/${APP}/venv/bin/python" \
     "\$DIR/../share/${APP}/main.py" "\$@"
WRAPPER
chmod +x AppDir/usr/bin/"$APP"

# 5. Root-level symlinks required by AppImage spec
ln -sf usr/share/icons/hicolor/256x256/apps/"$APP".png AppDir/"$APP".png

# 6. Download appimagetool if not present
if [ ! -f appimagetool ]; then
  echo "==> Downloading appimagetool..."
  wget -q \
    "https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-x86_64.AppImage" \
    -O appimagetool
  chmod +x appimagetool
fi

# 7. Build AppImage
echo "==> Packaging AppImage..."
ARCH="$ARCH" ./appimagetool AppDir "$OUTPUT"

echo ""
echo "✓ Built: $OUTPUT"
echo "  Size: $(du -sh "$OUTPUT" | cut -f1)"
echo "  SHA256: $(sha256sum "$OUTPUT")"
