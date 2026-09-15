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
- DocFlow 0.24.2 requires physical Windows acceptance on the EPSON L15160 Series:
  native-handler receipt loss, immediate spool-record release, wake/offline/
  paper-out recovery, multi-job ordering, cancellation, and the selected timeout
  must be exercised with non-sensitive fixtures. Source and cross-build tests
  cannot certify paper output.
- Live Supabase RLS deployment/two-user verification, release signing, and macOS notarization.
- Orientation detection and incremental-copy output on the exact Windows x64
  package, including OneDrive-backed CIV/MEC/ELE/PRC trees and representative
  sparse-text/drawing-heavy PDFs.
- Incremental scan performance and fingerprint behavior still require native
  Windows acceptance on the operator's actual OneDrive/network-backed 20 GB RFI
  tree, including interruption, resume, changed files, and forced full rescan.
- OCR score labels and responsive plan scrolling still require native Windows
  acceptance at the operator's actual display scaling and representative small
  window sizes. Browser viewport evidence cannot certify WebView2 behavior.
- Signed-page recursive multi-core archive discovery still requires native
  Windows acceptance on the operator's actual OneDrive/network-backed RFI System,
  including deep nesting, an unreadable folder, cancellation, duplicate exact
  filenames, and a representative large entry count.
- Signed-page 0.26.0 progress stages, sticky preview navigation, enlarged
  comparisons, and hash-pinned warning copies still require native Windows/
  WebView2 acceptance with representative OneDrive input and Review roots.
- Version 0.27.0 bounded RFI System/signature analysis, parallel recursive
  discovery, and operation-wide progress stages require native Windows/WebView2
  acceptance on the operator's actual OneDrive-backed tree. Verify throughput,
  readable current-file subjects, monotonic counters, cancellation boundaries,
  deterministic plans, and checkpoint reuse under interruption.
- RFI Log to Follow Up 0.21.0 still requires Excel-on-Windows open/recalculate/
  save verification on consented copies. The macOS current-workbook test proves
  only targeted OOXML package fidelity, not Excel or OneDrive acceptance.

## Current local limitations

- Process Scans is intentionally top-level only.
- Signed-page preview thumbnails are capped at 20 accepted rows and 12 MiB of
  encoded preview data; open the PDFs manually beyond either bound.
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
