# PyInstaller configuration for Tauri's framework-independent Python sidecar.

from pathlib import Path

from PyInstaller.compat import is_win

root = Path(SPECPATH)

a = Analysis(
    [str(root / "src" / "docflow" / "sidecar.py")],
    pathex=[str(root / "src")],
    binaries=[],
    datas=[],
    hiddenimports=[
        "docflow.ops.rfi_process_scans",
        "docflow.ops.rfi_log_add",
        "docflow.ops.batch_print",
        "docflow.ops.rfi_stage_signed_pages",
    ],
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=["PySide6"],
    noarchive=False,
)
pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    a.binaries,
    a.datas,
    [],
    name="docflow-sidecar",
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    console=True,
    icon=str(root / "src-tauri" / "icons" / ("icon.ico" if is_win else "icon.png")),
)
