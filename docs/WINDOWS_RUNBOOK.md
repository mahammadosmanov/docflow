# Windows build and acceptance runbook

## Requirements

- Windows x64 with PowerShell and WebView2
- Rust/toolchain and Node versions used by the repository workflow
- No administrator permission is required to run the per-user installer
- Desktop Excel and printers are required for their native acceptance sections

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

Confirm install, normal GUI launch, version, nine-operation registry, OCR,
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
6. For printing, use at least three non-sensitive mixed fixtures in one Apply.
   Verify ordering, worksheets, ranges, A4/A3, orientation, duplex/color,
   cancellation, restart, and uncertain-job operator decisions. A job accepted
   without a final spool receipt must show sent-unconfirmed and advance without
   Review remaining or duplicate output. Exercise a native-handler title that
   differs from its filename and confirm a unique new spool job is followed.
   Simulate transient generic error/paper-out followed by completion and confirm
   the same job continues without resubmission; deleting/deleted must still
   pause. Include safe workbooks with several worksheets and confirm every
   selected sheet exports before submission. If Excel fails, export support
   diagnostics and record the pathless COM stage/HRESULT or native PDF stage.
   Use Retry unprinted only for a saved incomplete queue; a new Preview is new
   authority and may print completed files again. Never auto-retry uncertainty.
7. For RFI file grouping, use filename-only PDF, content-only DOCX/PDF, UTF-16
   MSG, EML with several RFIs, unmatched input, and a destination collision.
   Confirm source hashes do not change, multi-RFI mail fans out correctly,
   `_review` receives exceptions, Undo removes only created copies/folders, and
   an existing subfolder beneath the source is ignored.
8. For PDF orientation, use synthetic copies containing upright, sideways,
   upside-down, portrait, landscape, sparse-text, encrypted, and signed pages.
   Confirm only CIV/MEC/ELE/PRC are scanned, uncertain PDFs stay unapplied, source
   hashes never change, review layout is preserved, and Undo is hash-guarded.
9. Run PDF orientation twice over the same test tree. Check that unchanged
    files are reused on the second Preview. Interrupt a larger run, change some
    inputs, then check that only completed unchanged results are reused. Run
    Force full rescan once and confirm it resets on the next visit.
10. For signed-page staging, select the RFI System root above CIV/MEC/ELE/PRC,
    place exact test archives at several nesting depths, and use nested single-
    and multi-page signed scans. Confirm every exact archive is found, duplicates
    remain selectable/ambiguous, unreadable or reparse-linked folders fail closed,
    source hashes never change, and replacement review copies preserve the
    archive layout. During Preview, confirm discovery/OCR/archive/comparison
    stages update before completion; approved rows, comparisons, warning files,
    and warnings are reachable through the in-flow navigator at the target display
    scale; before/after images are legible and enlarge correctly. Cause a no-code,
    low-confidence, missing-archive, and duplicate-page warning; verify each entry
    names the exact source PDF and page/RFI where known. Apply must copy each
    affected source PDF once beneath `_review` with its input subfolder layout,
    never overwrite an existing Review file, and reject a source changed after
    Preview.
Never test against the only production workbook or auto-send mail. Attach only
redacted/synthetic evidence to Git; keep private documents outside the repository.


## Windows startup

1. Enable **Start DocFlow with Windows** on an isolated profile. Close DocFlow,
   sign out of Windows, and sign back in. Check that the window opens.
2. In the test profile, remove or change the per-user DocFlow Run value while
   the preference is enabled. Open DocFlow once and verify that the quoted
   installed executable path is restored.
3. Disable the setting, sign out and back in again, and verify Windows skips
   DocFlow. Confirm the setting and registry state belong to the test user.

## OCR baseline

Use the same reference packets, Windows machine, and DocFlow versions for each
comparison. Include clean, faint, stamped, rotated, and out-of-order scans.
Run Preview with a cold cache, then repeat with the same inputs. Compare the
operator-reviewed IDs and page boundaries against the Plan. Record exact ID
coverage, wrong IDs and boundaries, manual review count, and elapsed stage time.
Use **Settings → Support** for the latest pathless timing and cache counts.
