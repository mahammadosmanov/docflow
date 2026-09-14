# Safety and state

## Mutation lifecycle

```text
select scoped paths -> preview immutable Plan -> explicit Apply
-> write-ahead journal -> exact effects -> result -> optional guarded Undo
```

Preview is pure. Apply accepts an opaque plan ID, revalidates pinned identities
and hashes, then executes only approved steps. Destination collisions, drift,
locks, symlinks/reparse points, malformed input, exhausted bounds, or ambiguous
evidence fail closed. Cancellation is checked before effects; partial and unknown
outcomes stay recoverable rather than being presented as success.

Undo runs successful journaled steps in reverse. It refuses to overwrite later
user edits, verifies identities/hashes, and removes only empty directories that
the operation created. Review delete is a recoverable move to app trash.

## Persistent local state

| State | Default location | Contract |
|---|---|---|
| Settings | `~/.docflow/settings.json` | Atomic, schema-filtered, secrets removed |
| Journals | `~/.docflow/journal/` | Local undo authority and recovery evidence |
| Trash | `~/.docflow/trash/` | Recoverable review removals |
| Print queue | `~/.docflow/print_queue.json` | Resume state, not an undo journal |
| Work events | `~/.docflow/work-items/events/` | Owner-scoped append-only lifecycle data |
| Credentials | Windows Credential Manager/macOS Keychain | Tokens never enter settings or React |

Cloud history contains metadata only and cannot authorize local effects or undo.
Readiness only counts bounded pending records; it does not reconcile or mutate
them. Uncertain print submission is never retried automatically. Retained backup
and recovery artifacts are never automatically deleted.

## Workbook replacement

Workbook preview pins original content and approved literal-cell changes. Apply
creates a non-overwriting backup, writes a private candidate, verifies permitted
OOXML deltas, then uses capability-relative guarded replacement. A crash or
post-rename durability uncertainty remains in the journal for explicit recovery.

## PDF orientation copies

Orientation preview records the exact source hash, page indexes, current and
target rotations, confidence, method, relative destination, and directory steps.
Apply revalidates that approved hash, performs no new OCR, writes and validates a
private incremental PDF, then publishes it no-clobber beneath the separate review
root. The journal records the final copy hash so Undo removes it only while it is
still the exact file DocFlow created.
