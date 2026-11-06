# ADR 0054: Rust-captured file and folder drops

- Date: 2026-11-07
- Status: Accepted

## Context

Folder, file, and multi-file fields accepted HTML drag-and-drop and sent the
dropped paths from React in `paths/select` (`droppedPaths`). Two problems:

1. The desktop webviews (WebView2, WKWebView) do not expose a dropped file's
   absolute path to page script, and Tauri intercepts OS file drops before
   they reach the page, so the field sent only a file name and failed.
2. Rust trusted any absolute path the webview sent. A compromised webview
   could select a folder the operator never chose. That skipped the consent
   the native picker gives.

## Decision

Rust captures the drop. An app-level `on_window_event` handler stores the
paths from `WindowEvent::DragDrop(Drop)` in a one-slot `DropInbox` on the
`BackendRouter`. Tauri runs this handler in the same main-thread callback
that queues the `tauri://drag-drop` event to the webview, so the paths are
stored before page script can react.

The webview uses the drag-drop event only to find which field is under the
pointer and to highlight it. It then calls `paths/select` with
`fromDrop: true` and no path. Rust claims the stored drop once, only within
10 seconds, and applies the same rules as before: at most 1,000 paths, one
path unless the field takes several files, folder vs file kind, and the
field's extension filter. `droppedPaths` is removed from the protocol, and
`deny_unknown_fields` rejects it.

## Consequences

Drops work in the packaged app, and every selected path comes from the
operator's native picker or native drop. A drop that lands outside a field is
never claimed and expires. No new Tauri permission is needed: `core:default`
already allows event listening. The browser mock has no native drop, so it
uses Browse only. Native Windows acceptance of drops on OneDrive and network
folders is still pending in the ACCEPTANCE ledger.

## Rollback

Remove the `on_window_event` handler and `DropInbox`, and drop `fromDrop`;
fields then offer Browse only. Do not restore webview-supplied paths.
