# Native acceptance kit

Step-by-step procedures for the first five native gates in
[`ACCEPTANCE.md`](ACCEPTANCE.md), in the order that ledger asks for them: row 1
(install), row 13 (Excel), row 16 (RFI file grouping), row 19 (printing) and
row 4 (incremental scan). Run it on the Windows machine, top to bottom, and finish
each gate with one filled evidence record.

The kit adds procedure only. Rules, expected behaviour and the reasons behind
them stay in the documents it links to: [`KNOWN_ISSUES.md`](KNOWN_ISSUES.md),
[`WINDOWS_RUNBOOK.md`](WINDOWS_RUNBOOK.md), [`FEATURES.md`](FEATURES.md),
[`SAFETY_AND_STATE.md`](SAFETY_AND_STATE.md), [`RFI_DOMAIN.md`](RFI_DOMAIN.md)
and [`RELEASE_SAFETY.md`](RELEASE_SAFETY.md). A result seen on macOS, in CI, in
a package smoke run by CI, or in a cross-compile never passes a gate.

The expected results below were written from the documents and the source, not
from a run on Windows. If the application behaves safely but differently from a
step, record what you saw and correct this kit; do not force the result to fit.

## Preparation for all gates

### Candidate and installer

1. Decide the candidate version. It is the contents of `VERSION` at the release
   tag `v<version>`. Use one candidate for all five gates; a Passed row is only
   valid for the version written in it.
2. Download the signed installer `.exe` and its `.sha256` file from the GitHub
   Release (or the `DocFlow-windows-x64` Actions artifact). The `.sha256` file
   holds `<hash>  <installer name>`.
3. Compute the hash of the downloaded installer and compare it with the
   `.sha256` file:

       Get-FileHash -LiteralPath <installer path> -Algorithm SHA256

4. Stop if the hashes differ. Otherwise keep the lowercase hash: it goes into
   `installerSha256` of every evidence record.
5. Do not install an update during acceptance. If DocFlow offers one, choose
   Dismiss; "Install update" would change the candidate under test.

### Machine and settings

1. Use a native Windows x64 machine. Gate 13 needs desktop Excel, gate 16 needs
   Outlook to create the test messages, gate 19 needs the printers named there.
2. In DocFlow, open Settings and set Language to English so the labels in this
   kit match the screen.
3. Note the Windows edition and build, and the Excel and Outlook versions you
   will use. They go into the evidence record.

### Isolated test folder

1. Create one test folder outside any Git checkout, for example
   `<test>`, with these subfolders: `fixtures`, `work`, `out`, `evidence`.
2. Everything DocFlow reads or writes during these gates lives under `<test>`.
   Before every Apply, read each path field on the screen and confirm it points
   into `<test>`. If a path points anywhere else, stop and press Cancel.
3. Never select the live RFI Log, the live Follow Up workbook, the live RFI
   System, a live mailbox export or a production print queue. Gate 4 reads a
   copy of the RFI System, not the live tree.
4. The copies stay on this machine. They are not committed, not attached to an
   issue and not pasted anywhere. Delete them when the gates are finished.

### Fixtures

1. Workbook and scan copies come from the current documents, because the gates
   test real structure. Treat them as private.
2. Everything you create yourself (PDFs, Word files, Outlook messages, print
   files) must contain only invented content. Use invented RFI numbers that
   cannot exist in the project, for example the `9101`-`9104` range with the
   exact form `AZE-SUN-INT-QMS-RFI-<CIV|MEC|ELE|PRC>-<four digits>`
   ([`RFI_DOMAIN.md`](RFI_DOMAIN.md#identifiers)).

### Hash snapshot helper

Several steps compare a folder before and after. Paste this once into the
PowerShell session; it prints one line per file, and an empty comparison means
"identical". The output contains file names, so keep it on screen and out of the
evidence record.

    function Get-Snapshot([string]$Root) {
      Get-ChildItem -LiteralPath $Root -Recurse -File | Sort-Object FullName |
        ForEach-Object {
          '{0}  {1}' -f (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash,
            $_.FullName.Substring($Root.Length)
        }
    }
    # $before = Get-Snapshot '<folder>'
    # ... run the operation ...
    # Compare-Object $before (Get-Snapshot '<folder>')

`<folder>` must not end in a backslash.

### Operating DocFlow

- The left navigation offers Workspace, Review folder, Action queue, Templates,
  History, Recovery center, Settings and, for admins, Admin panel.
- Open an operation from Workspace by its title; the Operations list has a
  "Search operations…" box. An operation runs as Configure, Preview, Apply.
- Every change goes Preview, then your explicit Apply, then the result screen.
  The result screen and History both offer Undo. In History, Undo opens a dialog
  with "Undo to original location"; use that choice in this kit.
- During Preview the stop button is Cancel. During Apply it is "Stop after
  current item" (for printing, "Pause queue").
- Never press Apply on a preview you have not read.

### Recording a result

1. Copy [`acceptance-evidence.template.json`](acceptance-evidence.template.json)
   to `<test>\evidence\row-<number>.json`. The copy is not saved in the
   repository.
2. Fill the fixed fields at the start of the gate: `ledgerRow`, `gate`,
   `candidateVersion`, `installerSha256`, `date` (YYYY-MM-DD), `windowsVersion`
   and the `applications` that gate uses.
3. After each step add one entry to `stepResults`. `step` is `<row>.<step
   number>`, for example `13.7`. `result` is `pass`, `fail`, `skipped` or
   `not-run`. `observed` is one short sentence.
4. Add each failure to `failures` with its step and a one-sentence summary.
5. Set `status` last (Passed, Failed or Blocked) using the rules below.
6. The record has no field for file paths, folder names, document names, mail
   subjects, addresses or contents, on purpose: evidence must stay free of
   private data. Do not put them in `observed`, `summary` or `note` either. Use
   counts, durations, DocFlow's own status words and error messages (after
   checking the message shows no path), and application versions.

Status rules, for every gate:

- **Passed**: every step passed or was skipped for a reason written in `note`
  that this kit marks as allowed, and no fail condition was met.
- **Failed**: any fail condition was met, or a step produced a result the pass
  conditions do not allow. Keep going through the remaining steps if it is safe,
  so the record is complete.
- **Blocked**: the gate could not be run (for example no Ready rows to apply,
  a missing application, or a defect in the source copy that exists before
  DocFlow touches it). Say why in `note`.

---

## Gate 1: clean install, startup, package smoke, uninstall (ledger row 1)

**What it proves.** The exact installer installs silently for one user, the
installed application starts and stays alive, its own package smoke runs, the
installed tree passes the package audit, and the uninstaller removes it. The
package smoke (`--package-smoke`) exercises `rfi.process_scans_fast_covers`
through Preview, Apply, journal and Undo on a built-in fixture. Steps 10 to 14
add normal GUI startup and a OneDrive-backed folder, which the script does not
cover.

**Prepare.**

- A Windows user profile with OneDrive signed in and no DocFlow installed. The
  script refuses to run if `docflow.exe` already exists under
  `%LOCALAPPDATA%\DocFlow`, `%LOCALAPPDATA%\Programs\DocFlow` or
  `%ProgramFiles%\DocFlow`.
- PowerShell and Node.js on `PATH`. The script runs `node
  tools/package_audit.mjs`, so it must start from a checkout of the tag
  `v<version>`.
- The verified installer from the preparation section.
- Three small invented files in a folder inside the OneDrive-synced area and an
  empty second folder there, for step 13: any two documents plus one file whose
  name contains an invented code.

**Steps.**

1. Open Installed apps and confirm DocFlow is not listed.
2. Open PowerShell in the root of the checkout.
3. The script builds its report path from `RUNNER_TEMP`, a GitHub Actions
   variable that a workstation normally lacks. Set it for this session:

       $env:RUNNER_TEMP = $env:TEMP

4. Run the smoke script. `-EvidencePath` writes a small JSON with hashes and
   timings and no document data:

       ./tools/smoke_windows_installer.ps1 -Installer <installer path> -EvidencePath <test>\evidence\smoke.json

5. Wait for it to end without using the machine. It installs with `/S`, starts
   the GUI, waits about five seconds, stops it, runs `--package-smoke`, audits
   the installed tree and runs the uninstaller with `/S`.
6. Confirm the last output line is "Automated Windows installer
   install/startup/OCR/apply/undo/uninstall smoke: OK", followed by a warning
   that Office/Excel COM and printer certification remain manual. That warning
   is expected.
7. In the JSON report the script printed, check `ok` is true and `version`
   equals the candidate version. Record `operationCount`. The smoke fails unless
   the registry exposes twelve operations; if `WINDOWS_RUNBOOK.md` or
   `RELEASE_SAFETY.md` says thirteen, the code wins and the discrepancy goes in
   `note`.
8. Open `smoke.json`. Check `result` is `pass`, `installerSha256` equals the
   hash from preparation, `packageAuditPassed` and `uninstallVerified` are true.
   Put `installMilliseconds`, `coldStartMilliseconds` and `peakWorkingSetBytes`
   in the `observed` text of this step.
9. Open Installed apps again and confirm DocFlow is gone.
10. Run the installer normally, without `/S`. Note whether Windows asked for
    administrator permission; the runbook says none is required.
11. Start DocFlow the way you will start it every day. Confirm a window opens, no
    separate console window appears, and the "DocFlow couldn't start" screen
    does not appear. If a sign-in screen appears, sign in with your own account.
12. Open Settings, then Advanced support, then "Preview support bundle". Confirm
    the version it lists equals the candidate version.
13. From Workspace open "Group Files by RFI". Set "Incoming files folder" to the
    invented-files folder in OneDrive and "Grouped output folder" to the empty
    folder next to it (use Browse). Press Preview. Wait for "Preview ready".
    Do not press Apply.
14. Close DocFlow. Leave it installed for the other gates.

**Pass.** Every step passes: the script ends with OK, the version and checksum
match the candidate, the manual launch shows no console window and no start
error, and step 13 reaches "Preview ready" with no error on the OneDrive paths
(rows or a "No top-level files were found" style warning are both fine).

**Fail.**

- The script throws at any point: install exit code not zero, GUI not input-idle
  within 30 seconds or exited during bootstrap, smoke exit code not zero, package
  audit failure, no uninstaller found, or the executable still present after
  uninstall.
- `result` is not `pass`, the checksum differs from the installer you verified,
  or the reported version differs from the candidate.
- DocFlow asks for administrator permission, opens a console window, shows the
  start-failure screen, or step 13 ends in an error on a OneDrive path.

**Fill in.** `ledgerRow` 1, `windowsVersion`, no application versions needed.
Put the timings from `smoke.json` in `observed` of step 1.8. Passed or Failed
for the whole row; Blocked if no clean profile or no OneDrive is available.

---

## Gate 13: Excel open, recalculate and repair-dialog check (ledger row 13)

**What it proves.** Output from DocFlow's two Excel-writing operations on copies
of the current workbooks opens in desktop Excel with no repair dialog, keeps
formulas, validations and styles, changes only the cells shown in Preview, keeps
a backup, and is restored byte-for-byte by Undo. Operations:
`rfi.log_add` ("Add multi-sheet RFI Log entries") on an RFI Log copy and
`rfi.follow_up_sync` ("Synchronize RFI Log to Follow Up") on a Follow Up copy.
The Follow Up Apply also sets full recalculation on open
([ADR 0045](adr/0045-follow-up-recalculate-on-open.md)), which is what the
recalculate check below depends on.

**Prepare.**

- Copy the current RFI Log and the current Follow Up workbook into
  `<test>\work`. Keep a third, untouched copy of each in `<test>\fixtures`.
- Make sure no copy is open in Excel and that Excel is closed.
- Record the SHA-256 of both working copies:

       Get-FileHash -LiteralPath <workbook copy> -Algorithm SHA256

  Call them `RFI Log hash` and `Follow Up hash`.
- An empty folder `<test>\out\backups` for the Follow Up backup.

**Steps.**

1. Open each working copy in desktop Excel once, before DocFlow touches it. If a
   repair or recover dialog appears now, stop: the gate is Blocked, because the
   fault is in the source copy. Close Excel without saving.
2. In DocFlow open "Add multi-sheet RFI Log entries". Set "RFI Log workbook" to
   the RFI Log copy. Keep "Email extraction method" on "Offline — private
   (recommended)". Under "Sheets and RFI counts" select only CIV with one RFI.
   Do not paste a real email; leave "Paste the request email" empty or paste an
   invented one.
3. Press Preview and wait for "Preview ready".
4. In "Review and edit RFI entries", read every row. Where the preview lists
   "Exact workbook cell changes", expand it and read each Sheet and cell, the
   Current value and the After Apply value. Do not use any copy-to-Excel action
   in this gate.
5. Press Apply. Confirm the result reports the rows applied and none failed.
6. Compare the RFI Log copy with the hash you recorded. It must now differ.
   Open the `.docflow-backups` folder beside the RFI Log copy and confirm the
   backup there has a SHA-256 equal to `RFI Log hash`.
7. Open the RFI Log copy in Excel. Check, and put the answers in `observed`:
   no repair or recover dialog; the file opens as the same macro-enabled
   workbook type; formulas next to the new rows return values (no `#REF!` or
   `#NAME?` that were not in step 1); drop-down validation still works on a new
   row; the new rows are formatted like their neighbours; any "update links"
   prompt matches what step 1 showed.
8. Close Excel and choose not to save. Saving would change the file and Undo
   refuses to overwrite later edits.
9. In History choose Undo on this entry and then "Undo to original location".
   Compare the SHA-256 of the RFI Log copy with `RFI Log hash`; it must be equal.
10. Open "Synchronize RFI Log to Follow Up". Set "Source RFI Log workbook" to the
    RFI Log copy, "Target Follow Up workbook" to the Follow Up copy and "Backup
    folder (optional)" to `<test>\out\backups`.
11. Press Preview and wait for "Preview ready". In "Review RFI Follow Up status
    candidates", note how many rows have the Decision "Ready to update", "Already
    synchronized" and "Manual review only". If no row is "Ready to update", the
    Follow Up half of this gate has nothing to test: use a Follow Up copy that
    differs from the RFI Log, or mark the gate Blocked.
12. Expand "Exact workbook cell changes" and read every row. Keep the selected
    "Apply" boxes as proposed.
13. Press Apply. Confirm the result reports rows applied and none failed.
14. Confirm the Follow Up copy hash now differs from `Follow Up hash` and the
    backup in `<test>\out\backups` has a SHA-256 equal to `Follow Up hash`.
15. Open the Follow Up copy in Excel. Check: no repair or recover dialog; the
    cells listed in step 12 hold the "After Apply" values and spot checks of cells
    around them are unchanged; percentage indicators and totals that depend on
    the changed statuses have recalculated; no `#REF!` or `#NAME?` that was not
    in step 1. Excel will offer to save on close because of the recalculation;
    choose not to save.
16. In History choose Undo on this entry and then "Undo to original location".
    Compare the SHA-256 of the Follow Up copy with `Follow Up hash`; it must be
    equal.
17. Lock check. Open the Follow Up copy in Excel and leave it open. In DocFlow
    repeat steps 10 and 11, then press Apply if the preview lets you. Expect the
    refusal "Close the selected workbook in Excel or another application, then
    try again." (at Preview or at Apply) and no change. Close Excel without
    saving and compare the SHA-256 with `Follow Up hash`; it must be equal.
18. Repeat steps 10 to 16 once with the working copies placed inside the
    OneDrive-synced area. You may mark this step `skipped` and say so in `note`
    only if OneDrive is not used for these workbooks in daily work.

**Pass.** Steps 1 to 17 pass: no repair dialog after Apply or after Undo; the
expected cells and only those changed; formulas and recalculation behave; both
backups equal the original hashes; both Undo operations restore the original
hashes; the lock refusal leaves the file untouched.

**Fail.**

- Excel shows any repair, recover or "unreadable content" dialog on output that
  opened cleanly in step 1, after Apply or after Undo.
- A formula, validation, style, macro or link that worked in step 1 is lost or
  broken, or a cell outside the previewed list changed.
- A backup is missing or its hash differs from the original.
- Undo leaves a hash different from the original (when no one saved in Excel).
- Apply overwrote a file it should have refused, or the lock case changed the
  file.
- Apply is refused with "DocFlow could not verify that only approved status
  cells changed" or "The workbook contains a digital signature and cannot be
  changed safely by automation" on a normal current workbook. The refusal is
  safe, but it means the gate cannot pass on that workbook; record it as a
  failure and keep the message text.

**Fill in.** `ledgerRow` 13, `windowsVersion`, `applications.excel` (product and
build). Counts of Ready / Already synchronized / Manual review rows and the
number of changed cells go in `observed`.

---

## Gate 16: RFI file grouping with Outlook MSG and EML (ledger row 16)

**What it proves.** `rfi.group_files` ("Group Files by RFI") finds exact RFIs in
file names and in content, requires a readable EML or MSG for each RFI, copies
a multi-RFI mail into every related RFI folder, never overwrites, leaves the
source untouched, and Undo removes only what Apply created. Rules:
[`RFI_DOMAIN.md`](RFI_DOMAIN.md#incoming-file-grouping).

**Prepare.** Build the incoming folder `<test>\fixtures\incoming` with only
invented content. Use four invented RFIs: A = `CIV-9101`, B = `MEC-9102`,
C = `MEC-9103`, D = `ELE-9104`, each in the full
`AZE-SUN-INT-QMS-RFI-...` form.

| Fixture | Content |
|---|---|
| PDF named with RFI A's full code | filename-only match |
| MSG mentioning RFI A | a real Outlook MSG: save an invented message from desktop Outlook as an `.msg` file |
| EML mentioning both B and C | one mail, several RFIs |
| DOCX with B's code inside the text, neutral file name | content-only Office match |
| PDF with C's code inside the page, neutral file name | content-only PDF; an image-only scan also works |
| PDF named with RFI D's full code, and no mail mentioning D | missing-mail case |
| PDF or text file with no RFI code anywhere | unmatched |
| A subfolder holding a file named with a full code | must be ignored |

Create the destination `<test>\out\grouped`. To test the collision, pre-create
`<test>\out\grouped\CIV\AZE-SUN-INT-QMS-RFI-CIV-9101\<name of the filename-only
PDF>` as a small file with different content.

Take snapshots:

    $incomingBefore = Get-Snapshot '<test>\fixtures\incoming'
    $outBefore      = Get-Snapshot '<test>\out\grouped'

**Steps.**

1. Open "Group Files by RFI". Set "Incoming files folder" to `incoming` and
   "Grouped output folder" to `grouped`.
2. Press Preview and wait for "Preview ready".
3. In "Detected RFI matches", read each row: the Copy box, "RFI number",
   "Source file" and "Detection method" (File name, PDF / OCR, Mail content,
   Office content or File content). Confirm:
   - the filename-only PDF matches RFI A by File name;
   - the MSG matches RFI A by Mail content;
   - the EML appears once for B and once for C;
   - the DOCX matches B and the content PDF matches C;
   - RFI D has no copy row and the warning "No EML or MSG email file was found
     for this RFI; related files will not be placed" appears;
   - the unmatched file is routed to review (warning "No RFI number was found;
     the file will be placed in review");
   - the filename-only PDF for A shows "The destination file already exists and
     will not be overwritten";
   - the file inside the subfolder is not listed.
4. Expand "Open to inspect exact source and destination paths". Each normal
   destination must be `<discipline>/<full-code>/<source filename>` under the
   grouped folder, and `_review` must be at the root of the grouped folder.
5. Press Apply. Read the result counts.
6. Compare `$incomingBefore` with a new snapshot of `incoming`; the comparison
   must be empty.
7. Check the grouped folder: the EML exists once under B's folder and once under
   C's folder; the pre-created file for A still has its original content (compare
   its hash with the one before Apply); no folder exists for RFI D; the
   unmatched file is under `_review`.
8. In History choose Undo on this entry, then "Undo to original location".
9. Compare `$outBefore` with a new snapshot of `grouped`: the folders and files
   DocFlow created are gone, and the pre-created collision file is still there
   unchanged. Compare the incoming snapshot once more; it must still be empty.
10. Repeat steps 2 and 3 once more on the same folders to confirm Preview is
    repeatable and again changes nothing (re-take the incoming snapshot).

Not covered here: interruption during Apply, wrapped OCR identifiers on
real scans, and a OneDrive-synced source/destination (named in
[`KNOWN_ISSUES.md`](KNOWN_ISSUES.md)). If you test them separately, say so in
`note`; do not mark them passed here.

**Pass.** Step 3 matches every item above; the incoming snapshot never differs;
the collision file is never replaced; Undo returns the grouped folder to its
pre-Apply snapshot.

**Fail.**

- A file is copied for RFI D, or any RFI without mail evidence.
- An RFI or folder is created that the Preview did not list, or a file lands
  outside `<discipline>/<full-code>/` and `_review`.
- The multi-RFI EML is copied to only one of its RFIs.
- The pre-created destination file is overwritten, renamed, or removed by Apply
  or by Undo.
- Any incoming file changes (hash, name or presence).
- Undo leaves a created file or created folder behind, or removes something
  DocFlow did not create.
- A file from the ignored subfolder is processed.
- Preview shows a path outside `<test>`.

**Fill in.** `ledgerRow` 16, `windowsVersion`, `applications.outlook` (the Outlook
that created the MSG). `observed` records the number of copy rows, warnings and
created files, never names.

---

## Gate 19: Microsoft Print to PDF and a physical or network printer (ledger row 19)

**What it proves.** `files.batch_print` ("Print multiple files") builds one ordered
queue from mixed files and prints them strictly one at a time on Microsoft Print
to PDF and on one physical or network printer, honours the chosen options, reports
each job's status honestly, pauses on request, and resumes only unprinted jobs
([`SAFETY_AND_STATE.md`](SAFETY_AND_STATE.md#persistent-local-state)). Printing is
not undoable; the result screen's Undo is disabled for it. EPSON-specific cases
(wake, offline, paper-out) belong to row 20, not here.

**Prepare.**

- Fixtures with invented content in `<test>\fixtures\print`: a PDF of five or
  more pages, a Word document of two pages, an Excel workbook with three
  worksheets, each holding different data, and one image.
- An empty folder `<test>\out\pdf` to save Microsoft Print to PDF output into.
- Note the printer model and the driver name shown for each printer in Windows'
  printer properties. Use a printer where a few sheets of paper are acceptable.

**Steps.**

1. Open "Print multiple files". Choose "Add files" and add the four fixtures.
   Order them with "Move up" and "Move down" so the PDF is first.
2. Set "Printer" to Microsoft Print to PDF, "Paper size" A4, "Copies" 1,
   "Automatic retries" 0, "If a job still fails" to "Pause queue".
3. Press Preview. In "Review and customize the print queue", confirm every file
   is a row with Status "Ready" and Print checked, the displayed order matches
   what you set, and the workbook appears as one row per worksheet in the
   "Worksheet" column with its Visibility.
4. Read the warning "Printing cannot be undone". Press Apply.
5. Watch the progress line advance one file at a time. When the queue ends,
   check the per-file status list on the result screen: each file shows
   "Completed", or "Sent; receipt unavailable" when Windows keeps no final
   receipt. If Windows asks where to save the PDF, save into `<test>\out\pdf`; the
   queue waits for you before starting the next file. Note whether such a prompt
   appeared.
6. When the queue ends, open the produced PDFs. Check: one output per row and no
   duplicates; creation times follow the queue order; the PDF has the expected
   page count; the workbook produced one output for each selected worksheet; the
   Word and image outputs show their content.
7. Repeat steps 1 to 6 with the physical or network printer. In Preview, set a
   different "Paper" or "Duplex" for one row and "Copies" 2 for another, using
   options the printer supports.
8. After the queue ends, check the paper: order, copy count, paper size,
   orientation and two-sided setting match the Preview. If the printer or the
   native application does not support an option, DocFlow's warning "The
   selected printer or native application does not support these print options"
   is the expected, honest outcome.
9. Pause test, physical or network printer. Queue the five-page PDF followed by
   two other files. Press Apply, and while the first job is printing press
   "Pause queue".
10. Observe the row statuses and the message "The print queue was paused by the
    user". The remaining rows must not be submitted. If a row shows "Check
    printer", inspect the printer and its Windows queue before doing anything
    else and do not select that file again until you know what printed.
11. Close DocFlow and start it again. Open "Print multiple files" and confirm the
    card "Unfinished print queue found" appears. Choose "Retry unprinted only".
12. Confirm only the unprinted jobs printed and no completed file printed a second
    time. Check Windows' own print queue for stuck jobs.
13. Open Settings, then Advanced support, then "Preview support bundle". Confirm
    it reports the latest print queue without any file name or path. Read it
    before keeping a copy.

**Pass.** Each queue prints each file exactly once, in order, with the chosen
options or an honest unsupported-option warning; statuses match what actually
happened; Pause stops submission; "Retry unprinted only" prints only unprinted
jobs; the support bundle is pathless.

**Fail.**

- A duplicate job or output, an out-of-order output, or a missing output for a
  row that says "Completed".
- A selected worksheet is missing from the output, or a hidden worksheet printed.
- An option is silently ignored with no warning.
- After Pause, the queue submits another file, or resubmits the current one.
- A row in "Check printer" or "Sent; receipt unavailable" is retried
  automatically.
- A completed file prints again after "Retry unprinted only".
- The support bundle contains a path or a document name.

**Fill in.** `ledgerRow` 19, `windowsVersion`, one `applications.printers` entry
per printer used (Microsoft Print to PDF and the physical or network printer, each
with model and driver). Record how many rows ended as "Completed", "Sent; receipt
unavailable" and "Check printer" in `observed`.

---

## Gate 4: incremental scan on the real large tree (ledger row 4)

**What it proves.** The durable scan index reuses unchanged files, survives
interruption, rescans new or changed files, and can be bypassed once with a forced
rescan, on a tree the size of the real RFI System. Operations:
`rfi.normalize_pdf_orientation` ("Rotate RFI PDF pages") for the main sequence and
`rfi.system_sync` ("Analyze RFI System & Sync Worley Signatures") for the second
part. Index rules: [`SAFETY_AND_STATE.md`](SAFETY_AND_STATE.md#incremental-scan-index).
This gate only previews. Do not press Apply anywhere in it.

**Prepare.**

- A full copy of the RFI System tree in `<test>\tree`, on the same kind of
  storage as the live tree (OneDrive-synced or a network share). The tree must
  contain CIV, MEC, ELE and PRC folders and be comparable in size to the real
  20 GB tree. If you can only copy part of it, write the copied size in `note`
  and do not mark the row Passed.
- An empty folder `<test>\out\orientation-review`.
- For part B, working copies of the RFI Log and Follow Up workbooks in
  `<test>\work`.
- Record the size of the copy:

      Get-ChildItem -LiteralPath <test>\tree -Recurse -File -Filter *.pdf |
        Measure-Object Length -Sum

  Keep the PDF count, the total bytes and the newest `LastWriteTime` for
  step 10.

**Steps.**

Part A: "Rotate RFI PDF pages".

1. Open "Rotate RFI PDF pages". Set "RFI system root" to `<test>\tree` and
   "Orientation review folder" to `<test>\out\orientation-review`. Leave "Force a
   full rescan this time" off.
2. Press Preview. Note the start time. When the counters show some progress,
   press Cancel. Confirm the operation stops and reports it stopped safely.
3. Press Preview again. While this run is still in progress, end the DocFlow
   process from Task Manager. (The index counts appear only when a Preview
   finishes, so nothing can be read at this point.)
4. Start DocFlow and press Preview a third time. Let it finish. In the
   "Incremental scan index" section confirm "Earlier interrupted runs" is at least
   1 and "Reused" is greater than 0 (finished work from steps 2 and 3 was kept).
   Write down the duration and the Found, Reused, Scanned now and Failed counts.
5. Press Preview once more with nothing changed. Confirm "Scanned now" is near 0,
   "Reused" is near "Found" minus "Failed", and the run is much shorter than step
   4. Write down the duration and counts.
6. Leave the operation and close DocFlow. Start it again, repeat the Preview and
   confirm the counts match step 5 (reuse survives a restart).
7. Drift. In `<test>\tree` only: add one new PDF under one of the CIV, MEC, ELE or
   PRC folders, and replace one existing PDF in those folders with a different
   PDF of the same name. Press Preview. Confirm "Scanned now" is 2 (plus any
   items counted as Failed in step 4, which are retried) and "Reused" is the
   rest.
8. Forced rescan. Open the operation, switch on "Force a full rescan this time"
   and press Preview. Confirm the section says "Full rescan was requested for this
   run." and "Reused" is 0. This repeats the full scan time.
9. Leave the operation and open it again. Confirm the force switch is off. Press
   Preview and confirm "Reused" is greater than 0 again.

Part B: "Analyze RFI System & Sync Worley Signatures".

10. Check the tree is unchanged apart from step 7: repeat the Get-ChildItem
    command from Prepare and compare count, bytes and newest `LastWriteTime`
    (the new and replaced files account for the differences).
11. Open "Analyze RFI System & Sync Worley Signatures". Set "RFI System Root
    Folder" to `<test>\tree`, "RFI Log Workbook" and "Follow Up Workbook" to the
    working copies. Press Preview, record the duration and counts, then Preview
    again and record them. The second run must report reuse. Do not press Apply.
12. Turn on "Force a full rescan this time", Preview, and confirm "Full rescan was
    requested for this run." Leave the operation.

**Pass.** After interruption the completed work is reused and the interrupted
count is shown; an unchanged second run reuses nearly everything, is faster, and
still does after restart; the new and replaced files are the only ones scanned in
step 7; a forced rescan bypasses the index once and is off the next time; the
tree copy and the review folder end unchanged.

**Fail.**

- Interruption loses finished work, or the interrupted count never appears.
- An unchanged second run reuses nothing, or reuse disappears after restart.
- The replaced file is reused instead of scanned (step 7 shows "Scanned now" 0 or
  1).
- Force does not bypass the index, or stays on after leaving the operation.
- A tree file changes, or DocFlow writes anything into the review folder.
- The run cannot finish (crash, hang or an unrecoverable error) on a tree of this
  size.

**Fill in.** `ledgerRow` 4, `windowsVersion`. Put the tested size (PDF count and
GB), the durations and the counts for steps 4, 5, 6, 7 and 11 in `observed`.
There is no row-level performance threshold in the repository, so record the
numbers and let the Passed decision rest on the reuse behaviour above.

---

## Transferring a result into the ledger

1. Check the record: `status` is Passed, Failed or Blocked; `candidateVersion` and
   `installerSha256` are those of the verified installer; `date` is the day of the
   run; nothing in `observed`, `failures` or `note` identifies a path, a file, a
   document or a mail.
2. Edit the matching row of the table in [`ACCEPTANCE.md`](ACCEPTANCE.md). Row
   numbers in the kit are the ledger's own.
   - **Status**: Passed, Failed or Blocked. Leave Pending for a gate you did not
     run. A Passed row needs every step passed on the exact candidate version.
   - **Version**: `candidateVersion`.
   - **Date**: `date`.
   - **Evidence**: a GitHub Issue link, a release asset, or a short private
     reference such as "evidence record row-13, held locally". Add the one-line
     reason for Failed or Blocked. Never add a path or document content.
3. The evidence record itself contains no private data, so it may be attached to
   a GitHub Issue. The `smoke.json` from gate 1 contains an installer file name
   and hashes only. Do not attach the PowerShell output of `Get-Snapshot`, the
   support bundle without reading it, or any screenshot of a workbook, a scan or
   a mail.
4. Editing `ACCEPTANCE.md` is a repository change. Follow the version bump,
   release notes and commit rules in [`AGENTS.md`](../AGENTS.md); do not edit the
   ledger and leave the version unchanged.
5. A Failed or Blocked row stays visible in the ledger until a later run on a
   newer candidate passes it. Open or update an Issue for each failure so it
   leaves the kit as a tracked defect.
