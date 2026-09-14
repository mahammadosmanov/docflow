# Known issues and external gates

These are not silently waived by passing local tests.

## External acceptance still required

- Clean Windows x64 install, normal startup, package smoke, uninstall, and real
  OneDrive-backed path behavior for the exact candidate.
- Excel open/recalculate/repair-dialog inspection of consented current RFI Log
  and Inspection Status Follow Up copies.
- Outlook selection, Reply All, signature/quote preservation, auto-save, and
  guarded unchanged-draft undo on the operator's desktop Outlook variants.
- Microsoft Print to PDF plus representative physical/network printer output,
  cancellation, driver options, spool receipts, and uncertain recovery.
- Live Supabase RLS deployment/two-user verification, release signing, and macOS notarization.
- Orientation detection and incremental-copy output on the exact Windows x64
  package, including OneDrive-backed CIV/MEC/ELE/PRC trees and representative
  sparse-text/drawing-heavy PDFs.
- RFI Log to Follow Up 0.21.0 still requires Excel-on-Windows open/recalculate/
  save verification on consented copies. The macOS current-workbook test proves
  only targeted OOXML package fidelity, not Excel or OneDrive acceptance.

## Current local limitations

- Process Scans is intentionally top-level only.
- Signed-page preview thumbnails are capped at 20 accepted rows; open the PDFs
  manually beyond that bound.
- The frontend production bundle currently emits a non-failing size advisory;
  startup/package measurements are the acceptance evidence until safe route-level
  splitting is implemented.
- macOS DMG creation can fail on hosts where `hdiutil` reports `Device not configured`.
- The current macOS visual launch gate requires an unlocked desktop session; a
  package smoke or process sample does not replace that inspection.
- Tesseract OSD may not confidently classify sparse-text or drawing-only pages;
  the whole PDF is intentionally withheld for manual review in that case.

Resolved defects belong in `CHANGELOG.md`, not here. Never list unverified
operator data, secrets, private paths, or raw workbook contents.
