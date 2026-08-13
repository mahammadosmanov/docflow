#!/usr/bin/env bash
# Prepare and run the Tauri desktop application.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PYTHON="${DOCFLOW_PYTHON:-python3.12}"

cd "$ROOT"

if [[ ! -x .venv/bin/python ]]; then
  "$PYTHON" -m venv .venv
  .venv/bin/python -m pip install -r requirements-dev.txt
  .venv/bin/python -m pip install -e .
fi

if [[ ! -d node_modules ]]; then
  npm ci
fi

.venv/bin/python tools/build_sidecar.py
exec npm run tauri dev
