# Known issues and external gates

These are not silently waived by passing local tests.

## External acceptance still required

- Replica publication and Windows startup require native Windows acceptance on
  isolated non-sensitive copies of the actual SharePoint/OneDrive,
  GlobalProtect/network, and local roots. Verify long paths, locks, offline and
  reconnect states, interruption, startup enable/disable, retained backups,
  target-only warnings, one-minute cadence, timestamp/settle behavior,
  file/size circuit breakers, profile pause/resume, Apply, restart, and Undo.
  macOS tests and Windows cross-compile do not certify these external systems.

- Completion Status Sync requires native Windows/OneDrive acceptance on a copy
  of the current RFI Log and the real nested RFI System. Verify traversal speed,
  exact/malformed/duplicate filenames, locks and drift, Excel repair-free open,
  source-tree immutability, Apply, and Undo. The local current-workbook test used
  an available prior copy because the latest removable-volume source was not
  readable during this build session.

- RFI file grouping still requires native Windows/OneDrive acceptance with real
  Outlook MSG/EML, content-only Office/PDF fixtures, wrapped OCR identifiers,
  per-RFI missing-mail refusal, multi-RFI mail fan-out, destination collisions,
  interruption, Apply, and Undo. Source/cross-build evidence cannot certify
  Outlook storage variants or live sync locks.

- Clean Windows x64 install, normal startup, package smoke, uninstall, and real
  OneDrive-backed path behavior for the exact candidate.
- Excel open/recalculate/repair-dialog inspection of consented current RFI Log
  and Inspection Status Follow Up copies.
- Outlook selection, Reply All, signature/quote preservation, auto-save, and
  guarded unchanged-draft undo on the operator's desktop Outlook variants.
- Selected-mail Unicode MSG archive requires classic Outlook COM acceptance on
  Windows: exact-selection drift, readable saved MSG, destination collision,
  OneDrive locks, mailbox immutability, and hash-guarded Undo. New Outlook does
  not expose the required classic Outlook COM object model.
- Microsoft Print to PDF plus representative physical/network printer output,
  cancellation, driver options, spool receipts, and uncertain recovery.
- DocFlow 1.0.2 requires physical Windows acceptance on the EPSON L15160 Series:
  native-handler title substitution/receipt loss, transient generic error,
  immediate spool-record release, wake/offline/paper-out recovery, multi-job
  ordering, cancellation, and the selected timeout must be exercised with
  non-sensitive fixtures. Source and cross-build tests cannot certify paper output.
- Live Supabase RLS deployment/two-user verification, release signing, and macOS notarization.
- Orientation detection and incremental-copy output on the exact Windows x64
  package, including OneDrive-backed CIV/MEC/ELE/PRC trees and representative
  sparse-text/drawing-heavy PDFs.
- Incremental scan performance and fingerprint behavior still require native
  Windows acceptance on the operator's actual OneDrive/network-backed 20 GB RFI
  tree, including interruption, resume, changed files, and forced full rescan.
- Worley OTHERS-only detection still requires native Windows acceptance across
  representative CIV/MEC form revisions, scan resolutions, faint signatures,
  OneDrive files, an exact missing-PDF case, and signatures placed only outside
  OTHERS. Source regression evidence is fail-closed but cannot certify every
  physical form or scanner variation.
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
