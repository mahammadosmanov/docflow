# ADR 0040: Cover-first identity reading and OCR page cache

- Date: 2026-10-23
- Status: Accepted

## Context

Stage signed first pages ran the multi-resolution full-page OCR cascade on every signed page, while Fast RFI Scan already confirms covers with a cheaper top-band read. Previews also re-read every file each time, including files that had not changed.

## Decision

- Add an `OcrAdapter::read_identity_page` method. Its default is the full reader. The native reader first runs the fast cover read and accepts it only when it is a confirmed cover (request heading, template mark, and one exact or approved OCR-alias RFI identity) with confidence of at least 0.8. Any other outcome, including an uncertain cover, no cover signal, or a failed render, falls back to the existing full-page reader. A missing OCR engine is never masked.
- Stage signed first pages reads page identities through `read_identity_page`. Identity rules are unchanged: only Strict and approved OCR-alias matches are accepted, uppercase exact codes, no silent repair (rules 10 and 11).
- Add `CachingOcr`, a wrapper that stores successful page reads in the existing durable `ScanIndex`, scoped by owner, mode (full, cover, identity), app version, root, relative path and page, and validated by the file fingerprint (size, modified time, identity). Failures are never stored; corrupt entries, an unavailable index, or files outside the root fall back to a fresh read.
- Fast RFI Scan, Group Files, and Stage signed first pages use `CachingOcr`. The cache is an optimization only; Apply still pins and revalidates hashes and never recalculates.

## Consequences

Most signed covers resolve with one cheap read instead of the full cascade, and unchanged files cost no OCR on later previews, including those prepared by the Action Queue. Pages the fast read cannot confirm cost one extra cover read before the full read. Speed on real signed scans and Windows behavior require native measurement, tracked in [KNOWN_ISSUES.md](../KNOWN_ISSUES.md).
