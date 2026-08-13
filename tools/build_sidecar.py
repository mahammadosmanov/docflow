"""Freeze and stage the Python engine using Tauri's target-triple convention."""

from __future__ import annotations

import os
import shutil
import stat
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def _rustc() -> str:
    candidates = [
        shutil.which("rustc"),
        str(
            Path.home()
            / ".rustup"
            / "toolchains"
            / "stable-aarch64-apple-darwin"
            / "bin"
            / "rustc"
        ),
        str(
            Path.home()
            / ".rustup"
            / "toolchains"
            / "stable-x86_64-apple-darwin"
            / "bin"
            / "rustc"
        ),
    ]
    for candidate in candidates:
        if candidate and Path(candidate).is_file():
            return candidate
    raise RuntimeError("rustc was not found; install the stable Rust toolchain")


def target_triple() -> str:
    output = subprocess.check_output([_rustc(), "-vV"], text=True)
    for line in output.splitlines():
        if line.startswith("host: "):
            return line.removeprefix("host: ").strip()
    raise RuntimeError("rustc did not report a host target")


def main() -> int:
    environment = os.environ.copy()
    environment["PYINSTALLER_CONFIG_DIR"] = str(ROOT / "build" / ".pyinstaller")
    subprocess.run(
        [
            sys.executable,
            "-m",
            "PyInstaller",
            "--noconfirm",
            "--clean",
            str(ROOT / "docflow-sidecar.spec"),
        ],
        cwd=ROOT,
        env=environment,
        check=True,
    )
    suffix = ".exe" if os.name == "nt" else ""
    source = ROOT / "dist" / f"docflow-sidecar{suffix}"
    if not source.is_file():
        raise RuntimeError(f"PyInstaller output is missing: {source}")
    destination = (
        ROOT
        / "src-tauri"
        / "binaries"
        / f"docflow-sidecar-{target_triple()}{suffix}"
    )
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, destination)
    destination.chmod(destination.stat().st_mode | stat.S_IXUSR)
    print(destination)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
