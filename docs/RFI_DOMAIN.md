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

- INTEC processing normally handles one complete RFI scan.
- Worley processing may receive one PDF containing many signed first pages.
- Every accepted page is linked to exactly one canonical RFI and exact page index.
- Missing, duplicate, low-confidence, unsupported, or ambiguous evidence creates
  a warning/review outcome, never an output.
- The archive and signed source scan remain read-only. Page replacement happens
  only in no-clobber copies under the operator-selected review root.

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

## Mail

Incoming request prose is inconsistent. Offline/model extraction proposes data;
the operator checks and edits the full preview before any number is assigned.
RFI numbering comes from the current workbook, never from the email. Reply draft
creation uses exactly one currently selected Outlook message, Reply All, escaped
bounded content, and display-only behavior. Sending remains manual.
