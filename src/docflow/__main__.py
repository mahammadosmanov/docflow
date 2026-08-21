"""Compatibility entry point for the packaged DocFlow engine sidecar."""

from __future__ import annotations

from docflow.sidecar import main


if __name__ == "__main__":
    raise SystemExit(main())
