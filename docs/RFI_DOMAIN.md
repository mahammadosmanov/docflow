# RFI domain rules

## Identifiers

The only persisted full RFI identifier is:

```text
AZE-SUN-INT-QMS-RFI-<DISCIPLINE>-<FOUR DIGITS>
```

It must already match exact uppercase ASCII when persisted. Current RFI Log
disciplines are `CIV`, `MEC`, `ELE`, and `PRC`. Across processing features,
whenever a numeric value is accepted as an RFI candidate, normalize one to four
ASCII digits to a four-digit suffix only after anchoring it to a supported RFI
discipline/code context. Pad with leading zeroes; reject longer values rather
than truncating them. ITP revisions, dates, unrelated numbers, and persisted
records are not rewritten. OCR may also normalize one time to Unicode NFC and
uppercase while recognizing evidence; the `-RFI-` anchor is never fuzzy and
decorated text, `Â`, lookalike separators, or extra symbols cannot become
persisted identifiers.

ITP revisions such as `_A` are official workbook values and remain unchanged.
RFI Log can contain consecutive physical rows for one RFI; do not impose global
row uniqueness. The workbook determines the next number and approved mapping.

## Dates and compact editing

The editor may show short discipline/item/date forms for speed, but the approved
row retains the complete underlying value. The default inspection date can be
the next day; operator edits remain authoritative. Hidden common fields are
expandable and editable. Reusable remembered values must not include task/RFI
identity or secrets.

## Scans and signed pages

### Fast RFI Scan

- `rfi.process_scans_fast_covers` is the only active RFI scan-separation
  operation. It scans top-level PDFs and supports two explicit modes.
- Multi-RFI checks the upper page band in four right-angle directions at low
  resolution. A group starts only when one high-confidence, consensus-confirmed
  strict identity or approved OCR alias, `REQUEST FOR INSPECTION`, and a known
  project/client mark (`SUNRISE PROJECT`, `INTEC`, or `LIGHTSOURCEBP`) agree.
  Searchable text alone cannot confirm a cover. The independent cover-confirmed
  rescue is limited to unresolved strong-vote candidates; it does not override a
  stable text-layer identity or competing RFI. Uncertain boundaries send the
  affected pages to `_review` without attaching them to an earlier RFI.
- When safe PDF page extraction is unsupported, the complete source PDF goes to
  `_review` unchanged with an unreadable warning. Page separation preserves
  supported XMP metadata, including indirect stream lengths.
- Single-RFI reads only the first page and keeps the complete source PDF intact.
  It does not split pages or apply orientation corrections, even when that
  option is enabled. Use it only when the PDF contains one RFI.
- The approved OCR alias `CLV` is normalized to `CIV`. Filenames, searchable
  PDF text, and manual corrections never use this alias.
- Preview name correction accepts a complete supported RFI code with a
  one-to-four-digit suffix and canonicalizes that suffix to four digits. Rust
  derives the destination from the approved root and folder setting; source
  paths and page selections are immutable. Duplicate or existing targets fail
  closed before Apply.
- Page rotation correction defaults off to avoid another full-page OCR pass.
  Confirmed outputs use the matching discipline folder, create a missing RFI
  folder when needed, and never overwrite an existing destination.
- Other cover templates remain unsupported by this detector and go to review.

### Incoming file grouping

- `rfi.group_files` inspects only regular, non-hidden files immediately inside
  the selected incoming folder. Existing subfolders are ignored and untouched.
- A canonical filename match is sufficient for ordinary files. EML and MSG are
  also inspected because one mail can belong to several RFIs. Files without a
  filename match use bounded content inspection: Office package XML, raw UTF-8/
  UTF-16 content, or every PDF page through the native OCR cascade.
- Exact OCR/content recognition accepts whitespace and at most two total line
  breaks around the canonical hyphens, including `...-RFI-\nMEC-0765`. Every
  letter, digit, segment, and segment order must still be exact; decorated or
  extended identifiers remain invalid for persisted output.
- Office package inspection preserves explicit line, paragraph, and table-cell
  boundaries. Formatting runs may remain contiguous, but text from a following
  cell must never be joined directly to an otherwise valid four-digit number.
- Each detected RFI must also be identified by at least one readable EML or MSG
  file in the same incoming set. Without that mail evidence, Preview creates no
  folder and proposes no copy for that RFI; one warning lists the RFI and a
  bounded summary of affected files. Other RFIs in the same file remain
  independently eligible when their own mail exists.
- A mail or document matched to several exact RFIs is copied once into each
  selected canonical folder. Compact mail-name continuations such as
  `...-1260_1261,1262` inherit only the already explicit discipline.
- Persisted Group Files RFIs are limited to CIV, MEC, ELE, and PRC. OCR-only
  recognition may map the exact OCR alias `CLV` to `CIV`; filenames, imported
  text, and manual identifiers do not receive that alias.
- Every approved normal copy targets
  `<destination>/<discipline>/<full-code>/<source filename>`. The discipline
  and full code are derived by Rust from its validated identifier, never from a
  client-authored path. `_review` stays at the destination root.
- Source files are read-only. Preview is pure; explicit Apply executes only the
  approved Plan, journals created outputs/directories, and guarded Undo removes
  only unchanged creations. Existing targets never overwrite. Symlinks,
  overlapping roots, unsafe identifiers, and occupied targets fail closed.

- INTEC processing normally handles one complete RFI scan.
- Worley processing may receive one PDF containing many signed first pages.
- Every accepted page has a high-confidence, consensus-confirmed Strict or
  approved OcrAlias identity and an exact page index. Repaired, fuzzy,
  conflicting, and filename-only identities remain review-only.
- Missing, duplicate, low-confidence, unsupported, or ambiguous evidence creates
  a warning/review outcome, never an output.
- The archive and signed source scan remain read-only. Page replacement happens
  only in no-clobber copies under the operator-selected review root.
- The selected RFI System archive root is searched at every subfolder depth with
  a bounded multi-core directory walk. Archive lookup requires the exact
  case-insensitive `RFI No. <RFI>.pdf` basename; its numeric suffix may contain
  one to four ASCII digits and is canonicalized before matching. Sketch PDFs,
  DOCX files, and code appearing only in a folder or another filename are not
  archive identity. Short and four-digit basenames for the same RFI are duplicate
  candidates and remain ambiguous rather than selecting one.
- New signed-scan input recursion defaults on. Both recursive walks reject
  symlinks and stop the whole preview if the archive cannot be scanned completely.
- Every warning tied to a signed input records its relative PDF path, page number
  when known, exact RFI when known, and diagnostic reason. The approved Plan adds
  at most one no-clobber copy of each affected source PDF beneath
  `<review root>/_review`, preserving its signed-input relative path. Apply pins
  and revalidates the source hash; a changed source or occupied destination fails
  closed. The source is never split, moved, renamed, or modified.

Accepted OCR candidates retain the recognizer's bounded confidence value in the
approved Plan. The UI displays it as a score out of 100 for Fast RFI Scan,
signed-page rows, and Review. Orientation uses Tesseract's OSD score, normalized
to the same visible 0–100 range. These are detector scores for operator review,
not calibrated probabilities and never a substitute for preview approval.

## Workbooks

RFI Log and Inspection Status Follow Up are macro-enabled production documents.
Plans pin their hashes and the exact sheet/cell/current/replacement values. Apply
must reject drift, formulas/merged/protected unsafe targets, unknown layouts,
unsupported package structures, locks it cannot safely handle, or any unapproved
OOXML delta. Backups never overwrite and are retained for operator recovery.

### Completion evidence

- The exact case-insensitive filename wrapper is
  `RFI No. AZE-SUN-INT-QMS-RFI-<CIV|MEC|ELE|PRC>-<FOUR DIGITS>.pdf`;
  the embedded canonical RFI remains exact uppercase ASCII.
- `rfi.complete_status_sync` searches every subfolder below the selected RFI
  System root without reading PDF contents or following symlinks. The source
  tree is read-only; duplicate exact PDFs are displayed but still represent one
  RFI completion decision.
- An exact PDF means the matching RFI is complete. Preview may propose
  `Completed` only for existing literal blank or `Ongoing` `Inspection Status`
  cells. Repeated physical rows for the same RFI are updated together.
- `Completed` is already synchronized. `Cancelled`, `Rejected`, formulas,
  merged/missing cells, unknown values, malformed PDF names, and missing exact
  RFI Log rows remain manual review and authorize no cell change.
- This operation changes only RFI Log status cells. Follow Up, REMARKS, signed
  sheets, and Worley state are outside its authority.

RFI System Worley analysis accepts only the exact case-insensitive filename
`RFI No. <canonical RFI>.pdf` inside the matching canonical RFI folder. It never
falls back to another PDF. On page one, only handwriting or a signature inside
the inner name/signature area of the bottom-right `OTHERS` cell is evidence.
Printed table borders are removed from the measurement; contractor/subcontractor
signatures, dates, general notes, and every other page region are ignored.
Missing, unreadable, empty, or weak evidence is unsigned. It cannot promote a
status to `Signed`; an unsigned result may set a safe existing control status to
`Not Signed` but never downgrades a status already `Signed`. Changing this
detector requires a new scan-index algorithm profile so no earlier classification
survives the change.

A shared Follow Up status cell changes only when all RFIs represented by that
cell satisfy the required state. `Ongoing` corresponds to `In Progress` where the
workbook mapping requires it. Worley CIV/MEC control tables are discovered by
their safe headers, not assumed coordinates.

Follow Up sync changes only the mapped RFI and status cells. It never adds,
replaces, or clears content in worksheet `Remarks` cells. RFI Log `REMARKS`
remain read-only source evidence for Worley Signed/Not Signed mapping.

For RFI Log projection, blank means `Pending`, all `Completed` means
`Completed`, and any `Ongoing` or Completed/Pending mixture means `In Progress`.
Cancelled and Rejected identifiers are removed before the shared status is
recomputed. Exact historical compact forms and bounded numeric ranges are
expanded only when unambiguous; malformed or mixed-discipline values fail closed.
For supported CIV/MEC routes, including FINAL routes, records synchronize only
into uniquely matching prepared rows and exact source contexts. If none of a
record's source contexts matches any prepared row in its intended destination,
the record is outside the active template scope: omit it from Manual Review and
propose no RFI or status update, regardless of an unrelated existing placement.
Leave that existing placement untouched. If any context matches but the target
cell is unsafe, keep it in Manual Review with the specific target-safety reason.
Multiple matches remain manual review except when CIV or MEC records map to
distinct existing rows justified by different exact source contexts.

FINAL destinations are reconciled independently by exact prepared row identity.
Finding the same RFI in one Signed By row does not satisfy another source
context. If some source contexts have prepared destinations while others do
not, eligible rows may synchronize while each unmatched context remains
visible for manual review; the warning identifies matched contexts and target
rows, then lists unmatched contexts and the source-log fields relevant to that
destination. DocFlow never corrects the source log. No row or section is
synthesized.
For CIV, each target area tag must be named in the source. For MEC, repeated
source rows must agree on ITP, item, and lifecycle status. Signature evidence is
cumulative: an unsigned duplicate cannot revoke a signed finding. MEC and
`Signed By Worley MEC` map `Tag No` to target `Block` and each comma-separated
`Area` value to a distinct target `Section`. Each exact `(Block, Section)` pair
must match an existing prepared row; source and target `Inspection Location`
are ignored. Distinct section pairs may fan out from one RFI, but duplicate or
ambiguous pairs remain manual review. Any CIV RFI Log record whose `Tag No`
starts with `PCS-` is outside Follow Up scope, regardless of ITP, item, Area,
or composite suffix. It is never added and is removed from an existing Follow
Up group during the next approved sync. This rule includes values such as
`PCS-19/S1,S2,S3` and takes precedence over the older PCU routing behavior. It
never creates rows or sections.
Duplicate contexts, conflicting source values, and other multi-valued contexts
remain manual review.
RFI Log record whose `Tag No` starts with `PCS-` is outside Follow Up scope,
regardless of ITP, item, Area, or composite suffix. It is never added and is
removed from an existing Follow Up group during the next approved sync. This
rule includes values such as `PCS-19/S1,S2,S3` and takes precedence over the
older PCU routing behavior. It never creates rows or sections.
Duplicate contexts, conflicting source values, and other multi-valued contexts
remain manual review.

The preview keeps unsupported ITP/item routes visible unless their exact scope
is already approved as excluded. For a supported route, records with no source
context matching any prepared row in the intended destination are outside the
active template scope and omitted from Manual Review; existing workbook cells
remain unchanged. A matching context with an unsafe target remains reviewable
with its specific safety reason. Partial matches remain visible: Mapping
details lists matched source contexts with their destination rows, unmatched
contexts, and the RFI Log fields to verify. These warnings are unselectable; no
destination row is created or guessed and the source RFI Log is never edited by
Follow Up sync.

Scope decisions take precedence over missing-route diagnostics. The confirmed
exclusions in [ADR 0035](adr/0035-intentional-follow-up-scope-exclusions.md)
are deliberately outside Follow Up; they are not missing mappings to add.
The zero/partial-match scope is recorded in
[ADR 0036](adr/0036-follow-up-context-scope-and-diagnostics.md); MEC
Section/Block matching is defined by
[ADR 0037](adr/0037-mec-follow-up-section-block-matching.md). Treat any other
unmapped ITP/item combination as undecided until an operator explicitly
approves its inclusion or exclusion. Record that decision here and in an
accepted ADR before changing route behavior.


A context-mismatched existing placement for an otherwise in-scope record is
never moved or deleted merely to force a match; it remains Manual Review. The
zero-context template-scope rule above takes precedence and leaves any existing
placement unchanged without review or status updates. The only automatic
removal exceptions are the explicitly out-of-scope ITP/item pairs in ADR 0035
and the CIV `PCS-` rule in ADR 0033. Ambiguous contexts or conflicting source
status cannot authorize a shared-cell status write for that RFI or any other
RFI represented by the same cell.

The eligible Follow Up sheets are exactly `CIV`, `MEC`, `PCU`, `Road`,
`Signed By Worley CIV`, and `Signed By Worley MEC`. The Signed By sheets and the
current PCU FINAL `Signed by Worley` column receive only FINAL behavior and use
the exact `Signed by Worley` REMARKS line, except CIV source RFIs tagged `PCS-`
which are excluded by the rule above. Existing FINAL rows are recalculated
as `Signed` or `Not Signed`; a missing FINAL RFI may be added only to one
uniquely matched, already-prepared literal RFI/status pair. A newly inserted PCU
FINAL identifier receives its Worley value on the next preview so independently
deselecting one row cannot leave a half-applied coupled update.
A target status formula is read-only: an already-present RFI is treated as
satisfied only when the cached status already equals its projected status;
otherwise the required change remains manual review. Other formula-bearing,
merged, or unsafe matched destinations remain manual. `POT test`, `STAT`,
`Changelog`, MEC calculated remarks/manual overrides, ELE/PRC, and absent
ITP/item sections are never created or modified by this operation. A source
record with multiple areas and multiple sections is ambiguous and remains
manual rather than using a cartesian guess. Missing RFI-System signature-control
rows are not appended: the scanner does not prove the destination
category/context or a same-table row style contract. Such cases remain explicit
manual review, and unsigned scans update existing safe control statuses to
`Not Signed` without downgrading an existing `Signed` value.

## PDF page orientation

Orientation normalization searches only directory trees named `CIV`, `MEC`,
`ELE`, or `PRC` below the selected RFI system root. It renders every page and
uses 250/350-DPI Tesseract OSD to classify reading direction; agreeing passes
may combine their evidence. An unresolved page receives a four-direction
English OCR comparison, but only a sufficiently dense, high-scoring, clearly
dominant direction is accepted. Paper dimensions alone are not evidence because
a legitimate landscape form may still be upright. Preview uses a bounded Rust
worker pool while Apply remains serial and deterministic.

The selected RFI system is read-only. A PDF is actionable only when every page
has a valid right-angle result at or above the fixed confidence threshold.
Sparse technical drawings and close competing directions remain manual review.
Unreadable, low-confidence, encrypted, digitally signed, malformed, or already
existing destinations fail closed. Approved changes append only page dictionary
`/Rotate` revisions to copies beneath a separate review root and verify that page
content, geometry, annotations, links, widgets, metadata, catalog entries, and
embedded files remain unchanged.

## Mail

Incoming request prose is inconsistent. Offline/model extraction proposes data;
the operator checks and edits the full preview before any number is assigned.
RFI numbering comes from the current workbook, never from the email. Reply draft
creation uses exactly one currently selected Outlook message, Reply All, escaped
bounded content, and display-only behavior. Sending remains manual. Attached
files and unusual requests also remain operator-reviewed: mail parsing may
suggest an ITP, item, area, or requested count, but it never certifies document
correctness or makes a lifecycle transition by itself.

Selected-mail archival is a separate operation. Preview records the exact
MailItem identity, a bounded content fingerprint, and a Windows-safe filename.
Apply requires that same message to remain the sole Outlook selection, exports
Outlook's native Unicode MSG representation to private temporary storage, and
creates a no-clobber staging copy. No message is sent, moved, deleted, marked,
or otherwise changed. The existing RFI grouping operation may later copy that
MSG to every exact RFI it contains; grouping's per-RFI mail-evidence gate still
applies.
