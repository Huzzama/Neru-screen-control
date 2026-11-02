#!/bin/sh
# Builds the helper binaries that ship next to the app (the `neru` CLI and the
# `neru-fand` root helper) and drops them where the Tauri bundler expects
# external binaries: src-tauri/binaries/<name>-<target triple>.
#   sh scripts/build-bins.sh          # release (used by `npm run build`)
#   sh scripts/build-bins.sh debug    # debug   (used by `npm run dev`)
set -e
cd "$(dirname "$0")/.."
TRIPLE=$(rustc -vV | sed -n 's/^host: //p')
if [ "$1" = "debug" ]; then
  cargo build -p neru-fand -p neru-cli
  DIR=target/debug
else
  cargo build --release -p neru-fand -p neru-cli
  DIR=target/release
fi
mkdir -p src-tauri/binaries
cp "$DIR/neru-fand" "src-tauri/binaries/neru-fand-$TRIPLE"
cp "$DIR/neru" "src-tauri/binaries/neru-$TRIPLE"
echo "neru, neru-fand → src-tauri/binaries/ ($TRIPLE)"
