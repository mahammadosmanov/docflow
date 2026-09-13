# ADR 0002: Local RFI work-item lifecycle

- Date: 2026-09-13
- Status: Accepted

## Context

DocFlow exposes safe individual operations, but the operator's real work spans
days and crosses Outlook, RFI Log, Inspection Status Follow Up, scanned PDFs,
Worley signatures, and three filesystem replicas. Remembering which operation
comes next creates duplicate effort and makes an interrupted task hard to resume.

The Excel workbooks and controlled folders remain the official records. DocFlow
must not silently replace them with an application database or infer that an
email, file, print job, or signature has completed merely because an earlier
step succeeded.

## Decision

Introduce an offline-first **RFI work item** as a local coordination record. A
work item groups one request and one or more canonical RFI identities through
these explicit stages:

1. request captured;
2. checked by operator;
3. RFI numbers assigned;
4. prepared files received;
5. published as Ongoing;
6. publication email archived;
7. INTEC scan filed;
8. Worley signature staged and synchronized;
9. replicas reviewed and synchronized;
10. complete.

Transitions are append-only events. The current stage is a rebuildable
projection, not mutation authority. Each transition stores only bounded
metadata and references to local DocFlow journal IDs; document contents,
credentials, Outlook bodies, and executable filesystem steps are forbidden.

Automatic transitions are allowed only when a successful local journal proves
the exact effect. Email sending, workbook writes, scan placement, printing, and
replica transfer retain their own preview and explicit approval. Manual stage
changes require a reason and are visibly labelled as operator assertions.

The initial implementation will use a versioned local store beneath the existing
DocFlow application-data directory. Cloud history may mirror inert metadata,
but the feature must work fully offline and cloud data can never authorize a
local effect.

## Consequences

- The home screen can show Today, Resume, and Needs Review without treating
  memory or email patterns as evidence.
- One RFI request can safely span multiple days and machines while official
  workbooks remain authoritative.
- Existing operations remain independently usable and backward compatible.
- Store migrations, event validation, retention, export, and rebuild tests are
  required before the first persistent work-item release.

## Initial implementation

Version 0.9.0 implements the owner-scoped append-only event store, strict event
validation, Today/Resume/Needs Review projection, explicit manual transition
reasons, no-clobber journaled writes, and undo. Automatic journal-linked
transitions, export, and longer-term retention controls remain later increments.

Version 0.10.0 adds direct queue-card transitions through the same generic
schema editor. Work-item fields are explicitly transient schema values, so a
previous task identity or stage is never restored as a preference for a new
item.

Version 0.11.0 implements journal-linked assisted transitions. Eligible work
cards open the operation that can produce the required evidence. Only a fully
committed, same-owner local journal with exact RFI coverage can prepare a second
work-item transition Plan, which still requires explicit preview and approval.
RFI Log advances to `numbers_assigned`, successfully filed Process Scans advance
to `intec_scan_filed`, and a two-workbook System Sync advances to
`worley_signed_synced`. Outlook drafts are deliberately excluded. Missing or
later-undone evidence projects the work item into Needs Review.

## Rollback

The work-item store is coordination metadata. Disabling or rebuilding it does
not alter official documents. Document rollback continues to use the existing
validated local journals only.
