#!/bin/sh
# Builds the neru-fand root helper and drops it where the Tauri bundler expects
# external binaries (src-tauri/binaries/<name>-<target triple>).
#   sh scripts/build-fand.sh          # release (used by `npm run build`)
#   sh scripts/build-fand.sh debug    # debug   (used by `npm run dev`)
set -e
cd "$(dirname "$0")/.."
TRIPLE=$(rustc -vV | sed -n 's/^host: //p')
if [ "$1" = "debug" ]; then
  cargo build -p neru-fand
  SRC=target/debug/neru-fand
else
  cargo build --release -p neru-fand
  SRC=target/release/neru-fand
fi
mkdir -p src-tauri/binaries
cp "$SRC" "src-tauri/binaries/neru-fand-$TRIPLE"
echo "neru-fand → src-tauri/binaries/neru-fand-$TRIPLE"
