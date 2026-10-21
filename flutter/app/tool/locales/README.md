# Localization sources

DocFlow uses dotted semantic keys such as `op.rfi.process_scans.title`.
`lib/l10n/generate_catalog.dart` reads these ARB catalogs and generates the runtime catalog and returns the
key itself when a translation is missing, so missing text is never rendered as
an unexplained blank.

Every user-visible key must exist in all four catalogs: Turkish, English,
Russian, and Azerbaijani. Tests enforce identical key sets and keep Azerbaijani
and Turkish translations independent.
