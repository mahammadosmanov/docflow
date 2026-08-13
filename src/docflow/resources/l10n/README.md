# Runtime localization

DocFlow uses dotted semantic keys such as `op.rfi.process_scans.title`.
`docflow.i18n.Strings` loads the selected ARB catalog at runtime and returns the
key itself when a translation is missing, so missing text is never rendered as
an unexplained blank.

Every user-visible key must exist in all four catalogs: Turkish, English,
Russian, and Azerbaijani. Tests enforce identical key sets and keep Azerbaijani
and Turkish translations independent.
