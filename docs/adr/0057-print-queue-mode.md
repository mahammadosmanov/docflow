# ADR 0057: Batch Print offers a fast queue mode beside the wait mode

- Date: 2026-11-22
- Status: Accepted
- Amends: DF-008 strict print order (observation scope only)

## Context

The operator reported that Batch Print is too slow on Windows and macOS. In
every route DocFlow submits one job, then polls the printer queue until the
spooler releases that job, and only then submits the next file
(`platform/windows/printing.rs` `wait_for_job`, `platform/macos/printing.rs`
`wait_for_job`). A long or slow printer therefore holds every later file
outside the queue. On 2026-11-22 the operator asked for a fast option and a
waiting option on both systems.

## Options

- Do nothing: keeps full completion evidence, keeps the delay.
- Remove the wait: faster, but loses completion evidence for every operator.
- Add an explicit choice, default wait.

## Decision

- Batch Print has a `queue_mode` choice: `wait` (default, unchanged behavior)
  and `fast`.
- `fast` returns a job as soon as the spooler holds an identified job: Windows
  after `EndDoc` succeeded for controlled PDF, image, and Excel worksheet jobs,
  or after the native application's job was identified in the queue; macOS
  after CUPS `lp` returned the accepted job ID. DocFlow then submits the next
  file.
- Jobs are still submitted one at a time in the approved order. Submission
  failures, uncertain states, and cancellation keep their current handling and
  are never retried automatically.
- In `fast` mode a `completed` job means "accepted by the spooler". DocFlow does
  not confirm that the printer finished it. The help text states this.
- The persisted job carries `queueMode`. Jobs saved before 1.52.0 have no
  field and load as `wait`. Unknown values are rejected by job validation.

## Consequences

- Fast mode cannot report a printer fault that happens after the spooler
  accepted the job; the operator checks the printer queue.
- Native Windows and macOS printer acceptance of fast mode is pending (see
  `KNOWN_ISSUES.md`).
- Rollback: select `wait`, or remove the choice; saved `fast` jobs then fail
  validation and need a new preview.
