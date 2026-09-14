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
| Scan index | `~/.docflow/scan-index.sqlite3` | Owner/root/operation-scoped checkpoints and scan-run audit; never Apply authority |
| Credentials | Windows Credential Manager/macOS Keychain | Tokens never enter settings or React |

Cloud history contains metadata only and cannot authorize local effects or undo.
Readiness only counts bounded pending records; it does not reconcile or mutate
them. Uncertain print submission is never retried automatically. Retained backup
and recovery artifacts are never automatically deleted.

After Windows accepts the complete document through `EndDoc`, a missing job
record means the driver released the spool entry and is treated as a completed
handoff. Explicit error/deleting/deleted states remain uncertain. Offline,
paper-out, blocked, and user-intervention states remain observable and wait
until cancellation, recovery, completion, or the approved timeout; they do not
fail the queue on the first poll. A transient status-read failure is retried for
a fixed three-second observation grace, never as a second print submission.
Release between the `GetJob` size probe and buffered read is also completion,
not an observation failure; a changing buffer is reread without resubmission.

Batch Print file selections are task-local and are never stored as remembered
parameters. Safe scalar settings are captured at Preview but a new visit starts
from defaults. The operator may explicitly restore the previous settings or a
named preset; neither action restores old files or resumes a queue.

## Incremental scan index

RFI System Sync first-page analysis and PDF orientation analysis create a local
SQLite run record before scanning. A successful result is committed together
with an item audit event after each PDF completes. An abandoned active run is
marked interrupted, while already committed items remain reusable. Transient
render, parse, or detector failures are logged but deliberately not cached, so
the next run retries them.

Reuse requires the same authenticated owner, canonical root, operation,
algorithm profile, relative path, file identity, size, and modification time.
New or changed files miss the index. The task-local Force full rescan switch is
never remembered. Paths and analysis outcomes stay local; Supabase is not a
cache authority and receives no scan paths, PDF content, or cached decisions.
For an actionable cached orientation result, Apply still hashes the live source
and rejects any difference from the exact hash recorded in the approved Plan.
A cached positive Worley signature is reused only after its stored full-file
hash matches the live PDF; metadata alone cannot authorize workbook updates.

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
