# ADR 0055: Shared document templates with offline cache

- Date: 2026-11-07
- Status: Accepted
- Supersedes over time: the hard-coded CON-CIV-010/011/013/014 cell maps in
  `concrete_documents/` (they keep working until each form is migrated)

## Context

The operator generates RFI Forms and the ITP quality forms (CON-CIV, CON-MEC,
CON-ELE, CON-MT, QM-FRM, and others) for every ITP item. About 55 forms
exist as `.xlsx` and `.docx` files, and they change with ITP revisions. Today
only four concrete forms are generated, from cell maps compiled into Rust, so
every form change needs a DocFlow release.

Requirements from the operator (2026-11-07):

- Template changes made once must reach every DocFlow user.
- Admins and chosen editors change templates. Everyone else uses them.
- Word forms mark fields with `{{placeholders}}`. Excel forms are mapped by
  clicking cells in a preview, without editing the original form.
- The internet on site is unreliable. Generation must work offline with the
  last synced templates, and DocFlow must pull new templates when online.
- Each template has its own data sources, defined per template.

## Decision

### Shared store (Supabase)

- `doc_templates`: one row per form (`code` such as `QM-FRM-001` or
  `CON-CIV-010`, title, discipline, file kind, `current_version`, archived).
- `doc_template_versions`: immutable revisions (`version`, `status` draft,
  published or retired, `file_sha256`, `file_size`, `storage_path`,
  `field_map` JSON, note, author, timestamps). A published version's file and
  field map can never change. A correction is a new version.
- Files live in the private Storage bucket `doc-templates` at
  `<template_id>/<version>/<sha256>.<ext>`. Objects are insert-only.
- `app_users.can_edit_templates` grants editing. `is_template_editor()` is
  true for admins and granted editors. Approved users can read; only editors
  can insert drafts, upload objects, and call `publish_template_version`.
- `doc_template_manifest` lists every published template in one request, so
  checking for updates is a single small read.

SQL: [`docs/supabase_templates.sql`](../supabase_templates.sql).

### Local cache (Rust only)

- Rust keeps the last good manifest and every downloaded file under
  `<app data>/templates/`. React never sees tokens, URLs, or file bytes.
- Sync runs at startup, every 30 minutes, and on demand. It downloads only
  missing or changed versions. Each file must match the manifest's SHA-256
  and size and open as a valid OOXML package. It is then installed with an
  exclusive create and rename, so a cached file is never overwritten.
- Offline, or when sync fails, the cached manifest stays in use and the UI
  shows the last successful sync time. A failed download never replaces a
  working cached version.

### Field map

Each version carries a `field_map` (schema 1): fields with a key, labels in
the four locales, a type (text, date, time, number, choice, check), a data
source (manual, constant, today, RFI Log column, ITP item), formatting, and
targets. Targets are worksheet cells for `.xlsx` and placeholder names for
`.docx`. A choice field can write a mark (for example `X`) into the cell for
the selected option. Rust validates every map before publish and before use.

### Generation

Generation is a normal operation: Preview shows each output file with its
resolved values, the operator edits or approves, and Apply writes new files
only (never overwriting), journaled with Undo. The template version used is
recorded in the journal, so a later template change does not alter
already generated documents.

## Consequences

Form changes no longer need a DocFlow release. The Supabase SQL must be run
once by an admin in the project's SQL editor before the shared store works.
Until then, DocFlow shows the template library as empty and offline.
Release order: 1.44.0 store, sync, cache, and read-only library; 1.45.0
editor and publishing; 1.46.0 RFI Form generation; later releases add the
CON forms per discipline and move the concrete forms onto templates.

## Rollback

Hide the Templates screen and stop the sync task. Cached files and Supabase
rows stay intact and can be re-enabled. Already generated documents are
ordinary files with journal entries and are unaffected.
