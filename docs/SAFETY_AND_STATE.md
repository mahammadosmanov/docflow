# Safety and state

## Mutation lifecycle

```text
select scoped paths -> preview immutable Plan -> explicit Apply
-> write-ahead journal -> exact effects -> result -> optional guarded Undo
```

Replica publication has one narrow extension: an explicit first Apply may store
a revocable standing authorization for the exact three roots, selected source,
backup root, file scope, interval, settle window, and mass-change limits. Later
automatic runs create fresh internal Plans but need no click. They never reuse an
old Plan. A source file inside the settle window defers without revoking approval.
Exceeding the approved file-effect or byte limit, profile drift, path loss,
incomplete scan, symlink, lock, hash mismatch, unstable file, backup error, or
publication error pauses automation before further mutation.

Preview is pure. Apply accepts an opaque plan ID, revalidates pinned identities
and hashes, then executes only approved steps. Destination collisions, drift,
locks, symlinks/reparse points, malformed input, exhausted bounds, or ambiguous
evidence fail closed. Cancellation is checked before effects; partial and unknown
outcomes stay recoverable rather than being presented as success.

RFI Follow Up sync keeps its local journal, exact backup, and private output.
If Apply fails after displacing the Follow Up workbook, Result and Recovery
Center offer an explicit **Restore original Follow Up file** action. Undo checks
the displaced file's journaled identity, byte hash, and complete security/stream
hash and requires the canonical name to be empty before a no-clobber restore.
Recovery can finalize that journaled restore only after observing the exact
original at the canonical name; it never retries the rename. Any collision or
mismatch remains untouched for manual review.

Signed-page archive discovery is always recursive and read-only. It processes
independent directory batches through the bounded Rust analysis pool, merges
results in deterministic path order, and records worker/directory/entry counts
in the Plan. The 250,000-entry bound, exact filename requirement, symlink refusal,
complete-scan requirement, source hashes, no-clobber review output, serial Apply,
journal order, and Undo rules are unchanged.

Warnings that belong to signed input PDFs are actionable Plan effects, not hidden
side effects. Preview records one hash-pinned copy per affected source beneath
the review root's `_review` subtree and preserves the relative input path. Apply
does not rediscover these files, refuses drift and collisions, journals successful
copies, and leaves all signed/archive sources unchanged. Informational completed
temporary-file cleanup does not create a warning copy.

Undo runs successful journaled steps in reverse. It refuses to overwrite later
user edits, verifies identities/hashes, and removes only empty directories that
the operation created. Review delete is a recoverable move to app trash.

Outlook MSG archive Preview stores no body, recipient, attachment, or Outlook
identifier in the executable Plan. Private native authority binds the plan to
the selected MailItem identity and fingerprint. Apply consumes that authority
once, revalidates selection before `SaveAs`, validates the private temporary MSG
envelope and size, then performs one capability-scoped no-clobber write. Export
failure creates no approved destination; uncertain destination writes remain
recovery-required. Undo hash-checks and removes only the unchanged created MSG.

## Persistent local state

Incoming RFI grouping never uses the incoming folder as output. Preview scans
only its top-level regular files and pins exact copy targets beneath a separate
destination. Apply reuses the cached Plan, copies serially without overwrite,
and journals every created file and directory for guarded Undo. Unmatched or
unreadable sources are preserved and copied to destination `_review`.

Completion Status Sync recursively discovers exact completed-RFI PDF filenames
without opening or changing them. The approved Plan pins only existing safe RFI
Log cells and their expected values. Apply does not rescan the RFI System; it
revalidates the workbook fingerprint, creates a no-clobber backup, verifies a
closed OOXML delta, journals replacement, and supports guarded Undo. Follow Up
and Worley workbooks are never selected or modified by this operation.

| State | Default location | Contract |
|---|---|---|
| Settings | `~/.docflow/settings.json` | Atomic, schema-filtered, secrets removed |
| Journals | `~/.docflow/journal/` | Local undo authority and recovery evidence |
| Trash | `~/.docflow/trash/` | Recoverable review removals |
| Print queue | `~/.docflow/print_queue.json` | Resume state, not an undo journal |
| Retired work events | `~/.docflow/work-items/events/` | Preserved legacy data; no active UI or operation mutates it |
| Scan index | `~/.docflow/scan-index.sqlite3` | Owner/root/operation-scoped checkpoints and scan-run audit; never Apply authority |
| Credentials | Windows Credential Manager/macOS Keychain | Tokens never enter settings or React |
| Replica automation | `~/.docflow/settings.json` | Exact approved profile, due time, last journal/status; errors pause it |

Replica publication scans only CIV/MEC/ELE/PRC plus explicitly named root files.
One chosen root is read-only source authority. Target-only files remain in place
as warnings. Missing target files use no-clobber copy. Changed targets are copied
to a unique retained backup, backup hash is verified, source bytes are staged in
the target parent, and only then atomically replace the target. Apply is serial
and fail-fast. Undo removes exact created copies or restores exact retained
backups only when the published target hash is unchanged. Backup files and
non-empty backup directories are never automatically removed.

Cloud history contains metadata only and cannot authorize local effects or undo.
Readiness only counts bounded pending records; it does not reconcile or mutate
them. Uncertain print submission is never retried automatically. Retained backup
and recovery artifacts are never automatically deleted.

After Windows accepts the complete document through `EndDoc`, a missing job
record means the driver released the spool entry and is treated as a completed
handoff. When `ShellExecute` accepts a native Office/PDF/image print request but
the driver never exposes an identifiable job record, DocFlow records the item as
`completed_unconfirmed`, journals the accepted submission, does not retry it,
and continues the queue. Deleting/deleted states and uncertain cancellation
remain uncertain and pause. Generic error, offline, paper-out, blocked, and
user-intervention states remain observable and wait
until cancellation, recovery, completion, or the approved timeout; they do not
fail the queue on the first poll. A simultaneous completion bit wins over a
stale generic error bit. A transient status-read failure is retried for
a fixed three-second observation grace, never as a second print submission.
Release between the `GetJob` size probe and buffered read is also completion,
not an observation failure; a changing buffer is reread without resubmission.
No software-only state certifies that paper physically exited the printer; the
completed-unconfirmed warning is retained for the final operator check.
Native handlers are matched first by document title and then, only when unique,
by the single new job ID created after submission. Multiple unmatched new jobs
remain ambiguous rather than being attached to the wrong file.

Batch Print file selections are task-local and are never stored as remembered
parameters. Safe scalar settings are captured at Preview but a new visit starts
from defaults. The operator may explicitly restore the previous settings or a
named preset; neither action restores old files or resumes a queue.

Support diagnostics may read the latest queue owned by the signed-in operator.
The bundle contains no document name or path and never opens document content.
It records only bounded printer inventory state, selected printer, format,
submission route, requested scalar options, attempt count, queue/job outcome,
counts, and recursively redacted error detail. Copy accepts only the exact
canonical bundle generated by Rust. This log helps separate unsupported format,
native-handler handoff, controlled submission, queue pause, and unconfirmed
receipt cases; it is not physical-output proof and cannot authorize a retry.

Excel automation does not modify application-wide calculation mode before a
workbook is open. Its read-only Open call still disables links, events, alerts,
and macros. A private export must have a bounded PDF envelope; Windows' native
decoder then loads it and confirms a non-zero page count before any printer job
starts. The mutation-oriented parser is not used as a print-compatibility gate.
Pathless failure evidence records only an allowlisted COM or native PDF stage
and HRESULT when available.

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
The current Worley detector profile accepts only the matching canonical RFI PDF
and only ink inside the page-one bottom-right `OTHERS` name/signature cell. A
profile change invalidates both positive and negative results from every older
detector; missing exact PDFs are retained as non-actionable unsigned evidence.

Replica publication uses the same index for SHA-256 comparison work, with a
separate algorithm profile and root scope for each of the three systems. It
records each fresh hash before the run completes, so interruption does not lose
finished work. A cache hit requires the exact file identity, size, modification
time, owner, root, relative path, operation, and algorithm profile. Missing,
changed, malformed, or unreadable cache evidence is hashed again. Publication
still reopens and hashes the live approved source and target during Apply; the
index never grants mutation authority.

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

Process Scans uses the same detector for incoming files. Its Plan pins the
source hash, selected page indexes, and target rotations. Apply extracts the
approved pages, applies those rotations in private storage, verifies the PDF,
then publishes through the existing no-clobber journal path. An uncertain page
sends its entire source PDF unchanged to `_review`.
