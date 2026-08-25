#!/usr/bin/env bash
# Prepare and run the Tauri desktop application.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cd "$ROOT"

if [[ ! -d node_modules ]]; then
  npm ci
fi

node tools/version_manager.mjs check
node tools/package_audit.mjs source
if [[ ! -x src-tauri/resources/tesseract/tesseract ]]; then
  ./tools/stage_tesseract_macos.sh
fi
exec npm run tauri dev
