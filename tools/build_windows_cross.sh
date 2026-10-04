#!/usr/bin/env bash
# Automated Windows x64 cross-compilation and packaging script for DocFlow.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
VERSION=$(node -p "JSON.parse(require('fs').readFileSync('package.json', 'utf8')).version")
TARGET_DIR="$PROJECT_ROOT/artifacts/windows-$VERSION"
WINDOWS_TESSERACT_SOURCE="$PROJECT_ROOT/src-tauri/resources/windows-tesseract"
RESOURCES_TESSERACT="$PROJECT_ROOT/src-tauri/resources/tesseract"

echo "==> DocFlow Windows x64 Cross-Compilation for v$VERSION"

if [ ! -d "$WINDOWS_TESSERACT_SOURCE" ]; then
    echo "ERROR: Windows Tesseract source not found at $WINDOWS_TESSERACT_SOURCE" >&2
    exit 1
fi

TEMP_BACKUP=$(mktemp -d "${TMPDIR:-/tmp}/docflow-tess-backup.XXXXXX")
cleanup() {
    echo "==> Restoring host Tesseract resources..."
    if [ -d "$TEMP_BACKUP" ]; then
        rm -rf "$RESOURCES_TESSERACT"/*
        cp -R "$TEMP_BACKUP"/* "$RESOURCES_TESSERACT"/ 2>/dev/null || true
        rm -rf "$TEMP_BACKUP"
    fi
}
trap cleanup EXIT INT TERM

echo "==> Backing up host Tesseract resources..."
cp -R "$RESOURCES_TESSERACT"/* "$TEMP_BACKUP"/

echo "==> Staging Windows Tesseract resources..."
rm -rf "$RESOURCES_TESSERACT"/*
cp -R "$WINDOWS_TESSERACT_SOURCE"/* "$RESOURCES_TESSERACT"/
(
    cd "$RESOURCES_TESSERACT"
    find . -type f ! -name SHA256SUMS -print0 | LC_ALL=C sort -z | xargs -0 shasum -a 256 > SHA256SUMS
)

echo "==> Building Windows binary and NSIS bundle..."
cd "$PROJECT_ROOT"
npm run build
npx tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc

mkdir -p "$TARGET_DIR"

SETUP_EXE="$PROJECT_ROOT/src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/DocFlow_${VERSION}_x64-setup.exe"
PORTABLE_EXE="$PROJECT_ROOT/src-tauri/target/x86_64-pc-windows-msvc/release/docflow.exe"

if [ ! -f "$SETUP_EXE" ]; then
    SETUP_EXE=$(find "$PROJECT_ROOT/src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis" -name "*.exe" | head -n 1)
fi

echo "==> Copying artifacts to $TARGET_DIR..."
cp "$SETUP_EXE" "$TARGET_DIR/DocFlow_${VERSION}_x64-setup.exe"
cp "$PORTABLE_EXE" "$TARGET_DIR/DocFlow_${VERSION}_x64-portable.exe"

echo "==> Creating portable package with bundled OCR..."
PORTABLE_STAGE=$(mktemp -d "${TMPDIR:-/tmp}/docflow-portable-stage.XXXXXX")
mkdir -p "$PORTABLE_STAGE/resources"
cp "$PORTABLE_EXE" "$PORTABLE_STAGE/docflow.exe"
cp -R "$RESOURCES_TESSERACT" "$PORTABLE_STAGE/resources/"
(
    cd "$PORTABLE_STAGE"
    zip -qr "$TARGET_DIR/DocFlow_${VERSION}_x64-portable-with-ocr.zip" docflow.exe resources
)
rm -rf "$PORTABLE_STAGE"

echo "==> Generating SHA-256 checksums..."
(
    cd "$TARGET_DIR"
    shasum -a 256 DocFlow_${VERSION}_x64-* > SHA256SUMS
    cp SHA256SUMS SHA256SUMS.txt
    cat SHA256SUMS.txt
)

echo "==> Windows v$VERSION release build complete!"
