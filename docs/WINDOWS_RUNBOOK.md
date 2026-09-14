# Windows build and acceptance runbook

## Requirements

- Windows x64 with PowerShell and WebView2
- Rust/toolchain and Node versions used by the repository workflow
- No administrator permission is required to run the per-user installer
- Excel/Outlook/printers are required only for their native acceptance sections

## Canonical build

From a clean reviewed commit:

```powershell
./tools/build_windows.ps1
```

Do not copy only the executable out of the bundle. Keep the generated installer,
bundled OCR resources, manifests, checksums, and evidence together. The Tauri
Windows subsystem prevents a companion console window in packaged builds.

## Clean install smoke

On a clean test account or runner:

```powershell
./tools/smoke_windows_installer.ps1 -Installer <exact-installer-path>
```

Confirm install, normal GUI launch, version, ten-operation registry, OCR,
preview/apply/journal/undo package smoke, installed-tree audit, and uninstall.
Record the exact commit, installer SHA-256, report, and failures. Do not infer a
pass from a macOS build or Windows cross-compile.

## Operator acceptance with copies only

1. Copy the current RFI Log and Follow Up workbooks into an isolated test folder.
2. Preview each workbook operation and inspect every approved cell comparison.
3. Apply, then open both outputs in desktop Excel. Confirm no repair dialog,
   macros/formulas/styles/links remain, expected cells alone changed, and backup exists.
4. Undo and compare hashes/behavior with the test copy.
5. Repeat with OneDrive paths, open-file locks, destination collisions, and cancellation.
6. For Outlook, select one non-sensitive test message, preview, Apply, confirm an
   unsent Reply All draft, then exercise unchanged and edited-draft undo paths.
7. For printing, use at least three non-sensitive mixed fixtures in one Apply.
   Verify ordering, worksheets, ranges, A4/A3, orientation, duplex/color,
   cancellation, restart, and uncertain-job operator decisions. A job accepted
   without a final spool receipt must show sent-unconfirmed and advance without
   Review remaining or duplicate output; explicit faults must still pause. Never
   auto-retry uncertainty.
8. For PDF orientation, use synthetic copies containing upright, sideways,
   upside-down, portrait, landscape, sparse-text, encrypted, and signed pages.
   Confirm only CIV/MEC/ELE/PRC are scanned, uncertain PDFs stay unapplied, source
   hashes never change, review layout is preserved, and Undo is hash-guarded.
9. For incremental scanning, run RFI System Sync and PDF orientation twice over
   the same non-sensitive tree. Confirm the second Preview reports unchanged
   files as reused without reopening them for OCR. Interrupt a larger run, add
   and modify files, then confirm only completed unchanged entries resume while
   new/changed/failed items scan again. Finally use Force full rescan and confirm
   it bypasses the cache once and is off on the next visit.

Never test against the only production workbook or auto-send mail. Attach only
redacted/synthetic evidence to Git; keep private documents outside the repository.
