// CompactDateInput and the editor date helpers from frontend/src/App.tsx. A
// date is stored as `dd-Mon-yy` text. The control edits a day, a month and,
// on demand, a year, and reports a value only once day and month are both set.

import 'dart:math' as math;

import 'package:flutter/material.dart' hide IconButton, Switch;
import 'package:intl/date_symbol_data_local.dart';
import 'package:intl/intl.dart';

import '../../l10n/strings.dart';
import '../../theme/app_icons.dart';
import '../../theme/tokens.dart';
import '../widgets/metrics.dart';
import 'form_controls.dart';

/// `EDITOR_DATE_MONTHS`: the English month abbreviations used in stored dates.
const List<String> editorDateMonths = [
  'Jan',
  'Feb',
  'Mar',
  'Apr',
  'May',
  'Jun',
  'Jul',
  'Aug',
  'Sep',
  'Oct',
  'Nov',
  'Dec',
];

/// A parsed `dd-Mon-yy` date. [month] is 1-based.
final class EditorDateParts {
  const EditorDateParts({
    required this.day,
    required this.month,
    required this.year,
  });

  final int day;
  final int month;
  final int year;
}

final RegExp _editorDatePattern = RegExp(
  r'^(\d{1,2})-([A-Za-z]{3})-(\d{2}|\d{4})$',
);

/// Days in [month] (1-based) of [year].
int editorDaysInMonth(int year, int month) =>
    DateTime.utc(year, month + 1, 0).day;

/// `parseEditorDate`: reads `d-Mon-yy` or `dd-Mon-yyyy` for years 2000 to
/// 2068 and rejects days the month does not have. Null when [value] is not a
/// valid date.
EditorDateParts? parseEditorDate(String value) {
  final match = _editorDatePattern.firstMatch(value.trim());
  if (match == null) return null;
  final day = int.parse(match.group(1)!);
  final monthName = match.group(2)!.toLowerCase();
  final monthIndex = editorDateMonths.indexWhere(
    (name) => name.toLowerCase() == monthName,
  );
  final month = monthIndex + 1;
  final rawYear = int.parse(match.group(3)!);
  final year = rawYear < 100 ? 2000 + rawYear : rawYear;
  if (month == 0 || year < 2000 || year > 2068) return null;
  return day >= 1 && day <= editorDaysInMonth(year, month)
      ? EditorDateParts(day: day, month: month, year: year)
      : null;
}

/// `formatEditorDate`: `dd-Mon-yy`.
String formatEditorDate(EditorDateParts parts) {
  final day = parts.day.toString().padLeft(2, '0');
  final yy = (parts.year % 100).toString().padLeft(2, '0');
  return '$day-${editorDateMonths[parts.month - 1]}-$yy';
}

/// Short month names for [localeName], as `Intl.DateTimeFormat` gives them
/// with `{ month: "short" }` and a trailing dot removed.
Future<List<String>> editorMonthShortNames(String localeName) async {
  await initializeDateFormatting(localeName);
  final format = DateFormat.MMM(localeName);
  return [
    for (var month = 1; month <= 12; month++)
      format.format(DateTime.utc(2024, month, 1)).replaceFirst('.', ''),
  ];
}

/// The draft of the control: day and month may be unset, the year always is.
final class _DateDraft {
  const _DateDraft({this.day, this.month, required this.year});

  final int? day;
  final int? month;
  final int year;

  _DateDraft copyWith({int? day, int? month, int? year}) => _DateDraft(
    day: day ?? this.day,
    month: month ?? this.month,
    year: year ?? this.year,
  );
}

/// `CompactDateInput`: a day select, a month select, and a year button that
/// reveals a year select (`.compact-date-cell`).
class CompactDateInput extends StatefulWidget {
  const CompactDateInput({
    super.key,
    required this.value,
    required this.label,
    required this.s,
    required this.onChange,
  });

  final String value;
  final String label;
  final Strings s;
  final ValueChanged<String> onChange;

  @override
  State<CompactDateInput> createState() => _CompactDateInputState();
}

class _CompactDateInputState extends State<CompactDateInput> {
  static final Map<String, List<String>> _monthCache = {};
  static final Set<String> _monthLoading = {};

  late _DateDraft _draft;
  bool _yearVisible = false;
  bool _yearMenuOpen = false;
  final FocusNode _yearFocus = FocusNode();

  @override
  void initState() {
    super.initState();
    _draft = _draftFor(widget.value, DateTime.now().year);
    _yearFocus.addListener(_onYearFocus);
  }

  @override
  void didUpdateWidget(CompactDateInput oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.value != widget.value) {
      setState(() => _draft = _draftFor(widget.value, DateTime.now().year));
    }
  }

  @override
  void dispose() {
    _yearFocus.removeListener(_onYearFocus);
    _yearFocus.dispose();
    super.dispose();
  }

  _DateDraft _draftFor(String value, int currentYear) {
    final parsed = parseEditorDate(value);
    if (parsed == null) return _DateDraft(year: currentYear);
    return _DateDraft(day: parsed.day, month: parsed.month, year: parsed.year);
  }

  void _onYearFocus() {
    // The year select closes when focus leaves it, unless its menu is open.
    if (!_yearFocus.hasFocus && _yearVisible && !_yearMenuOpen) {
      setState(() => _yearVisible = false);
    }
  }

  void _commit(_DateDraft next) {
    var normalized = next;
    final day = next.day;
    final month = next.month;
    if (day != null && month != null) {
      final maxDay = editorDaysInMonth(next.year, month);
      normalized = next.copyWith(day: math.min(day, maxDay));
      widget.onChange(
        formatEditorDate(
          EditorDateParts(
            day: normalized.day!,
            month: month,
            year: normalized.year,
          ),
        ),
      );
    }
    setState(() => _draft = normalized);
  }

  void _ensureMonthNames(String locale) {
    if (_monthCache.containsKey(locale) || _monthLoading.contains(locale)) {
      return;
    }
    _monthLoading.add(locale);
    editorMonthShortNames(locale).then((names) {
      _monthCache[locale] = names;
      _monthLoading.remove(locale);
      if (mounted) setState(() {});
    });
  }

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final s = widget.s;
    final locale = s.locale.name;
    _ensureMonthNames(locale);
    final monthNames = _monthCache[locale];
    final draft = _draft;
    final maxDay = draft.month != null
        ? editorDaysInMonth(draft.year, draft.month!)
        : 31;
    final dayValue = draft.day != null && draft.day! <= maxDay
        ? draft.day
        : null;
    final monthValue = monthNames == null ? null : draft.month;
    final yearValue = draft.year >= 2000 && draft.year <= 2068
        ? draft.year
        : null;

    return Tooltip(
      message: widget.value,
      child: Semantics(
        container: true,
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            _compactSelect<int?>(
              context: context,
              width: 58,
              semanticsLabel: '${widget.label} — ${s.t('rfi.date.day')}',
              value: dayValue,
              items: [
                const DropdownMenuItem<int?>(value: null, child: Text('--')),
                for (var day = 1; day <= maxDay; day++)
                  DropdownMenuItem<int?>(
                    value: day,
                    child: Text(day.toString().padLeft(2, '0')),
                  ),
              ],
              onChanged: (value) => _commit(draft.copyWith(day: value)),
            ),
            const SizedBox(width: 4),
            _compactSelect<int?>(
              context: context,
              width: 70,
              semanticsLabel: '${widget.label} — ${s.t('rfi.date.month')}',
              value: monthValue,
              items: [
                const DropdownMenuItem<int?>(value: null, child: Text('---')),
                if (monthNames != null)
                  for (var index = 0; index < 12; index++)
                    DropdownMenuItem<int?>(
                      value: index + 1,
                      child: Text(monthNames[index]),
                    ),
              ],
              onChanged: (value) => _commit(draft.copyWith(month: value)),
            ),
            const SizedBox(width: 4),
            Tooltip(
              message: s.t('rfi.date.changeYear'),
              child: Semantics(
                button: true,
                label: '${widget.label} — ${s.t('rfi.date.changeYear')}',
                child: InkWell(
                  borderRadius: BorderRadius.circular(7),
                  onTap: () => setState(() {
                    _yearVisible = !_yearVisible;
                    _yearMenuOpen = false;
                    if (_yearVisible) {
                      WidgetsBinding.instance.addPostFrameCallback((_) {
                        if (mounted && _yearVisible) _yearFocus.requestFocus();
                      });
                    }
                  }),
                  child: Container(
                    width: 30,
                    height: 30,
                    alignment: Alignment.center,
                    decoration: BoxDecoration(
                      color: _yearVisible ? t.surface3 : Colors.transparent,
                      borderRadius: BorderRadius.circular(7),
                    ),
                    child: Icon(
                      AppIcons.calendarDays,
                      size: remPx(context, 0.95),
                      color: t.text,
                    ),
                  ),
                ),
              ),
            ),
            if (_yearVisible) ...[
              const SizedBox(width: 4),
              _compactSelect<int?>(
                context: context,
                width: 74,
                semanticsLabel: '${widget.label} — ${s.t('rfi.date.year')}',
                value: yearValue,
                focusNode: _yearFocus,
                onTap: () => _yearMenuOpen = true,
                items: [
                  for (var year = 2000; year <= 2068; year++)
                    DropdownMenuItem<int?>(value: year, child: Text('$year')),
                ],
                onChanged: (value) {
                  _yearMenuOpen = false;
                  _yearVisible = false;
                  if (value != null) {
                    _commit(draft.copyWith(year: value));
                  } else {
                    setState(() {});
                  }
                },
              ),
            ],
          ],
        ),
      ),
    );
  }

  Widget _compactSelect<T>({
    required BuildContext context,
    required double width,
    required String semanticsLabel,
    required T? value,
    required List<DropdownMenuItem<T>> items,
    required ValueChanged<T?> onChanged,
    FocusNode? focusNode,
    VoidCallback? onTap,
  }) {
    final t = DocFlowTokens.of(context);
    return SizedBox(
      width: width,
      child: Semantics(
        label: semanticsLabel,
        child: ControlBox(
          minHeight: 30,
          child: DropdownButtonHideUnderline(
            child: DropdownButton<T>(
              value: value,
              items: items,
              onChanged: onChanged,
              onTap: onTap,
              focusNode: focusNode,
              isExpanded: true,
              isDense: true,
              dropdownColor: t.surface,
              iconSize: remPx(context, 0.85),
              style: TextStyle(fontSize: remPx(context, 0.8), color: t.text),
            ),
          ),
        ),
      ),
    );
  }
}
