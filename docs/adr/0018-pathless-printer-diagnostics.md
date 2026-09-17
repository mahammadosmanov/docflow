# ADR 0018: Pathless printer diagnostics

Date: 2026-09-17  
Status: Accepted

## Context

Physical printer and native Office handlers behave differently across Windows
drivers. Source tests cannot establish whether a submitted document produced
paper, and a generic failure report does not show whether DocFlow used controlled
PDF/image submission, a native application handler, or an unconfirmed handoff.
Uploading documents or raw queue files would expose private work data.

## Decision

Extend the existing local support bundle with a bounded snapshot of the latest
print queue owned by the signed-in operator. Record printer inventory
availability, selected printer, format, submission route, requested scalar
options, attempts, queue/job outcomes, counts, and bounded error detail. Do not
record document names, document paths, document contents, queue source identity,
credentials, or tokens.

Run the complete bundle through the existing recursive secret/content/path
redaction and byte/item/depth bounds. Native clipboard copy accepts only the
exact canonical Rust-generated bundle. Diagnostics remain read-only and local.

## Consequences

Operators can export comparable evidence after an EPSON or Office test queue,
which allows driver/route-specific fixes without guessing. The evidence cannot
prove physical output and never authorizes automatic retry. Real Windows and
printer acceptance remains mandatory.

Rollback removes the printer component from new diagnostic bundles. Existing
bundles and print queues remain inert data and require no migration.
