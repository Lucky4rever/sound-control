#!/bin/bash
set -e

cd "$(dirname "$0")"

EXE_NAME="sound-control"
PLATFORM="macos"
CARGO_FILE="crates/gui/Cargo.toml"

VERSION=$(grep -m1 '^version' "$CARGO_FILE" | cut -d'"' -f2)

echo "==========================================="
echo " Packaging $EXE_NAME v$VERSION ($PLATFORM)"
echo "==========================================="

echo "[1/3] Building release..."
cargo build --release -p "$EXE_NAME"

TEMP_DIR="target/package-$PLATFORM-temp"
OUT_FILE="target/${EXE_NAME}-v${VERSION}-${PLATFORM}.zip"

rm -rf "$TEMP_DIR"
mkdir -p "$TEMP_DIR/assets"

echo "[2/3] Collecting files..."
cp "target/release/$EXE_NAME" "$TEMP_DIR/"
[ -d "assets" ] && cp -r assets/* "$TEMP_DIR/assets/"
[ -f "README.md" ] && cp README.md "$TEMP_DIR/"
[ -f "LICENSE" ] && cp LICENSE "$TEMP_DIR/"

echo "[3/3] Archiving..."
(cd "$TEMP_DIR" && zip -r "../../$OUT_FILE" .)

rm -rf "$TEMP_DIR"

echo "Done: $OUT_FILE"