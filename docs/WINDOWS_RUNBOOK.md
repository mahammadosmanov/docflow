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

Confirm install, normal GUI launch, version, eleven-operation registry, OCR,
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
6. For Completion Status Sync, place exact and malformed `RFI No.` PDF names at
   several nesting depths. Confirm exact PDFs map to the correct existing RFI
   Log rows, blank/Ongoing cells preview as Completed, Completed stays unchanged,
   Cancelled/Rejected/formula/merged targets remain manual review, the RFI
   System hashes do not change, and Apply/Undo passes Excel's repair check.
7. For Outlook, select one non-sensitive test message, preview, Apply, confirm an
   unsent Reply All draft, then exercise unchanged and edited-draft undo paths.
8. For printing, use at least three non-sensitive mixed fixtures in one Apply.
   Verify ordering, worksheets, ranges, A4/A3, orientation, duplex/color,
   cancellation, restart, and uncertain-job operator decisions. A job accepted
   without a final spool receipt must show sent-unconfirmed and advance without
   Review remaining or duplicate output. Exercise a native-handler title that
   differs from its filename and confirm a unique new spool job is followed.
   Simulate transient generic error/paper-out followed by completion and confirm
   the same job continues without resubmission; deleting/deleted must still
   pause. Never auto-retry uncertainty.
9. For RFI file grouping, use filename-only PDF, content-only DOCX/PDF, UTF-16
   MSG, EML with several RFIs, unmatched input, and a destination collision.
   Confirm source hashes do not change, multi-RFI mail fans out correctly,
   `_review` receives exceptions, Undo removes only created copies/folders, and
   an existing subfolder beneath the source is ignored.
10. For PDF orientation, use synthetic copies containing upright, sideways,
   upside-down, portrait, landscape, sparse-text, encrypted, and signed pages.
   Confirm only CIV/MEC/ELE/PRC are scanned, uncertain PDFs stay unapplied, source
   hashes never change, review layout is preserved, and Undo is hash-guarded.
11. For incremental scanning, run RFI System Sync and PDF orientation twice over
   the same non-sensitive tree. Confirm the second Preview reports unchanged
   files as reused without reopening them for OCR. Interrupt a larger run, add
   and modify files, then confirm only completed unchanged entries resume while
   new/changed/failed items scan again. Finally use Force full rescan and confirm
   it bypasses the cache once and is off on the next visit.
12. For signed-page staging, select the RFI System root above CIV/MEC/ELE/PRC,
    place exact test archives at several nesting depths, and use nested single-
    and multi-page signed scans. Confirm every exact archive is found, duplicates
    remain selectable/ambiguous, unreadable or reparse-linked folders fail closed,
    source hashes never change, and replacement review copies preserve the
    archive layout. During Preview, confirm discovery/OCR/archive/comparison
    stages update before completion; approved rows, comparisons, warning files,
    and warnings are reachable through the sticky navigator at the target display
    scale; before/after images are legible and enlarge correctly. Cause a no-code,
    low-confidence, missing-archive, and duplicate-page warning; verify each entry
    names the exact source PDF and page/RFI where known. Apply must copy each
    affected source PDF once beneath `_review` with its input subfolder layout,
    never overwrite an existing Review file, and reject a source changed after
    Preview.
13. For replica publication, use three isolated roots and a fourth backup root.
    Put CIV/MEC/ELE/PRC beneath each root, then exercise missing, identical,
    changed, target-only, locked, unstable, long-path, offline, and reconnect
    cases. Confirm changed targets are backed up and hash-verified before atomic
    replacement; target-only files are never deleted; failure stops later effects;
    Undo restores only unchanged published targets; backups remain after Undo.
    Enable unattended mode with one explicit Apply, restart DocFlow, and confirm
    one-minute due runs need no click. Modify a source during a due run and confirm
    the approved settle delay defers without disabling the profile. Set low file
    and MiB brakes separately and confirm each pauses before Apply; inspect the
    same change manually before approving a revised profile. Inject another
    failure and confirm it pauses. Disable and re-enable the profile in Settings.
    Enable Windows startup, sign out/in, confirm the GUI starts without a console
    window, then disable it and confirm the per-user startup entry is removed.

Never test against the only production workbook or auto-send mail. Attach only
redacted/synthetic evidence to Git; keep private documents outside the repository.
