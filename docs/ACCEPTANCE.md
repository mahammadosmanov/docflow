# Native acceptance ledger

One row per external gate from [`KNOWN_ISSUES.md`](KNOWN_ISSUES.md). A gate
moves to **Passed** only with native Windows (or named external system)
evidence on the exact candidate version; macOS, synthetic, cross-compile, and
CI evidence never pass a row. Record the version, date, and an evidence link
(GitHub Issue, release asset, or private-evidence reference without paths or
document content).

Status values: **Pending**, **Passed**, **Failed**, **Blocked**.

| # | Gate | Area | Status | Version | Date | Evidence |
|---|------|------|--------|---------|------|----------|
| 1 | Clean x64 install, startup, package smoke, uninstall on a real OneDrive profile | Install | Pending | | | |
| 2 | Windows startup enable/disable, restart, per-user scope | Install | Pending | | | |
| 3 | `cap_fs/windows_dir.rs` listing of a real OneDrive/network folder (long names, many entries, second `GetFileInformationByHandleEx` call) | Filesystem | Pending | | | |
| 4 | Incremental scan on the real 20 GB OneDrive/network tree: interruption, resume, drift, forced rescan | Filesystem | Pending | | | |
| 5 | Signed-page recursive multi-core archive discovery on the real RFI System | Filesystem | Pending | | | |
| 6 | Action Queue inbox watcher (ADR 0039): settle delay, locks, pause, stale marking, source immutability | Filesystem | Pending | | | |
| 7 | Replica publication on real SharePoint/OneDrive/GlobalProtect roots (before any enablement) | Filesystem | Pending | | | |
| 8 | Cover-first identity reading and OCR page cache (ADR 0040) timing on real signed covers | OCR | Pending | | | |
| 9 | Orientation detection and incremental copies on CIV/MEC/ELE/PRC trees | OCR | Pending | | | |
| 10 | Worley OTHERS-only signature detection across form revisions and scan resolutions | OCR | Pending | | | |
| 11 | OCR score labels at the operator's display scaling; small-window WebView2 layout | UI | Pending | | | |
| 12 | Signed-page progress stages, comparisons, warning copies in WebView2 | UI | Pending | | | |
| 13 | Excel open/recalculate/repair-dialog check of current RFI Log and Follow Up copies | Office | Pending | | | |
| 14 | RFI Log to Follow Up 1.8.0 Signed/Not Signed recalculation in Excel | Office | Pending | | | |
| 15 | Completion Status Sync on a copy of the current RFI Log and nested RFI System | Office | Pending | | | |
| 16 | RFI file grouping with real Outlook MSG/EML, fan-out, collisions, Undo | Outlook | Pending | | | |
| 17 | Outlook Reply All draft: selection, signature/quote, auto-save, guarded undo | Outlook | Pending | | | |
| 18 | Selected-mail Unicode MSG archive on classic Outlook (after a route exists) | Outlook | Blocked | | | No operator route yet |
| 19 | Microsoft Print to PDF plus representative physical/network printers | Printing | Pending | | | |
| 20 | EPSON L15160: title substitution, transient errors, wake/offline/paper-out, ordering, cancel | Printing | Pending | | | |
| 21 | Live Supabase RLS deployment and two-user verification | Cloud | Pending | | | |
| 22 | Authenticode installer signing and macOS notarization | Release | Blocked | | | Needs paid certificates |
| 23 | File and folder drag-and-drop onto operation fields from Explorer, including OneDrive and network folders (ADR 0054) | UI | Pending | | | |
| 24 | Shared templates: run `docs/supabase_templates.sql` on the live project, then two-user sync, offline start with cached templates, and update pickup (ADR 0055) | Cloud | Pending | | | |
| 25 | Template editing: admin grants an editor, editor creates a template from a real `.docx`, saves a draft, publishes, and a second user receives it; a non-editor sees no edit controls and Supabase refuses its writes (ADR 0055) | Cloud | Pending | | | |
| 26 | Template generation on Windows: RFI cover from a published template with the live RFI Log, preset and edits in Preview, output opened in Word with layout intact, rerun refuses to overwrite, Undo removes the file (ADR 0055) | Windows / Word | Pending | | | |

## Order of work

Clear rows 1, 13, 16, 19, and 4 first: they cover the most-used operations
and the data-loss, workbook-repair, and duplicate-print risks ranked first in
[`ROADMAP.md`](ROADMAP.md#prioritization). [`ACCEPTANCE_KIT.md`](ACCEPTANCE_KIT.md)
holds the step-by-step procedure for those five rows.
