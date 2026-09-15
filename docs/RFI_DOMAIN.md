# RFI domain rules

## Identifiers

The only persisted full RFI identifier is:

```text
AZE-SUN-INT-QMS-RFI-<DISCIPLINE>-<FOUR DIGITS>
```

It must already match exact uppercase ASCII. Current RFI Log disciplines are
`CIV`, `MEC`, `ELE`, and `PRC`. OCR may normalize one time to Unicode NFC and
uppercase while recognizing evidence, but the `-RFI-` anchor is never fuzzy and
decorated text, `Â`, lookalike separators, extra symbols, or lowercase values
cannot become persisted identifiers silently.

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
- Source files are read-only. Preview proposes only no-clobber copies beneath a
  separate destination. Unmatched or unreadable files are copied to `_review`;
  symlinks, overlapping roots, unsafe identifiers, and occupied targets fail closed.

- INTEC processing normally handles one complete RFI scan.
- Worley processing may receive one PDF containing many signed first pages.
- Every accepted page is linked to exactly one canonical RFI and exact page index.
- Missing, duplicate, low-confidence, unsupported, or ambiguous evidence creates
  a warning/review outcome, never an output.
- The archive and signed source scan remain read-only. Page replacement happens
  only in no-clobber copies under the operator-selected review root.
- The selected RFI System archive root is searched at every subfolder depth with
  a bounded multi-core directory walk. Only the exact canonical RFI PDF filename
  may match; a code appearing only in a folder or unrelated filename is not authority.
- New signed-scan input recursion defaults on. Both recursive walks reject
  symlinks and stop the whole preview if the archive cannot be scanned completely.
- Every warning tied to a signed input records its relative PDF path, page number
  when known, exact RFI when known, and diagnostic reason. The approved Plan adds
  at most one no-clobber copy of each affected source PDF beneath
  `<review root>/_review`, preserving its signed-input relative path. Apply pins
  and revalidates the source hash; a changed source or occupied destination fails
  closed. The source is never split, moved, renamed, or modified.

Accepted OCR candidates retain the recognizer's bounded confidence value in the
approved Plan. The UI displays it as a score out of 100 for Process Scans,
signed-page rows, and Review. Orientation uses Tesseract's OSD score, normalized
to the same visible 0–100 range. These are detector scores for operator review,
not calibrated probabilities and never a substitute for preview approval.

## Workbooks

RFI Log and Inspection Status Follow Up are macro-enabled production documents.
Plans pin their hashes and the exact sheet/cell/current/replacement values. Apply
must reject drift, formulas/merged/protected unsafe targets, unknown layouts,
unsupported package structures, locks it cannot safely handle, or any unapproved
OOXML delta. Backups never overwrite and are retained for operator recovery.

A shared Follow Up status cell changes only when all RFIs represented by that
cell satisfy the required state. `Ongoing` corresponds to `In Progress` where the
workbook mapping requires it. Worley CIV/MEC control tables are discovered by
their safe headers, not assumed coordinates.

For RFI Log projection, blank means `Pending`, all `Completed` means
`Completed`, and any `Ongoing` or Completed/Pending mixture means `In Progress`.
Cancelled and Rejected identifiers are removed before the shared status is
recomputed. Exact historical compact forms and bounded numeric ranges are
expanded only when unambiguous; malformed or mixed-discipline values fail closed.

The eligible Follow Up sheets are exactly `CIV`, `MEC`, `PCU`, `Road`,
`Signed By Worley CIV`, and `Signed By Worley MEC`. The Signed By sheets receive
only FINAL behavior and use the exact `Signed by Worley` REMARKS line. `POT test`,
`STAT`, `Changelog`, ELE/PRC, and absent ITP/item sections are never created or
modified by this operation. A source record with multiple areas and multiple
sections is ambiguous and remains manual rather than using a cartesian guess.

## PDF page orientation

Orientation normalization searches only directory trees named `CIV`, `MEC`,
`ELE`, or `PRC` below the selected RFI system root. It renders every page and
uses Tesseract OSD to classify reading direction; paper dimensions alone are not
evidence because a legitimate landscape form may still be upright. Preview uses
a bounded Rust worker pool while Apply remains serial and deterministic.

The selected RFI system is read-only. A PDF is actionable only when every page
has a valid right-angle result at or above the fixed confidence threshold.
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
bounded content, and display-only behavior. Sending remains manual.
