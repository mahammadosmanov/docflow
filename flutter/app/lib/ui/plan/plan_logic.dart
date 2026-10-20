// Pure helpers behind the Plan preview, ported from frontend/src/App.tsx:
// `summarizePlanImpact`, `summarizeWorkbookCellChanges`, `canonicalRfiInText`,
// `isPlanEditorReady`, `safePngThumbnail`, `outlookReplyPreview`,
// `displayEditorValue` and the editor column sizing rules. The engine metadata
// they read is loosely typed JSON, so each reader checks types the way the
// TypeScript did and treats anything else as absent.

import '../../models/models.dart';
import '../screens/screen_common.dart' show formatConfidencePercent;

/// A JSON object, or null for anything else (`recordValue`).
Map<String, Object?>? recordValue(Object? value) {
  if (value is Map<String, Object?>) return value;
  if (value is Map) {
    return value.map((key, entry) => MapEntry(key.toString(), entry));
  }
  return null;
}

/// JavaScript `String(value)` for JSON values. Integral numbers print without
/// a decimal part, as they do in JavaScript.
String jsString(Object? value) {
  if (value == null) return 'null';
  if (value is double && value.isFinite && value == value.truncate()) {
    return value.toInt().toString();
  }
  if (value is List) {
    return value.map((e) => e == null ? '' : jsString(e)).join(',');
  }
  if (value is Map) return '[object Object]';
  return value.toString();
}

/// JavaScript truthiness for JSON values.
bool jsTruthy(Object? value) => switch (value) {
  null => false,
  final bool b => b,
  final num n => n != 0 && !n.isNaN,
  final String text => text.isNotEmpty,
  _ => true,
};

/// JavaScript `Number(value)` for JSON values; null when it would be NaN.
double? jsNumber(Object? value) => switch (value) {
  final num n => n.toDouble(),
  final bool b => b ? 1 : 0,
  final String text when text.trim().isEmpty => 0,
  final String text => double.tryParse(text.trim()),
  _ => null,
};

/// `Number.isInteger(Number(value))`, returning the integer.
int? jsInteger(Object? value) {
  final numeric = jsNumber(value);
  if (numeric == null || !numeric.isFinite) return null;
  return numeric == numeric.truncate() ? numeric.toInt() : null;
}

bool _isArrayIndex(String key) {
  final parsed = int.tryParse(key);
  return parsed != null &&
      parsed >= 0 &&
      parsed < 4294967295 &&
      parsed.toString() == key;
}

/// The keys of a JSON object in JavaScript enumeration order: integer-like
/// keys ascending, then the others in insertion order.
List<String> jsObjectKeys(Map<String, Object?> map) {
  final indexes = map.keys.where(_isArrayIndex).toList()
    ..sort((a, b) => int.parse(a).compareTo(int.parse(b)));
  return [...indexes, ...map.keys.where((key) => !_isArrayIndex(key))];
}

/// `toLocaleLowerCase(s.locale)`: Turkish and Azerbaijani lowercase the dotted
/// and dotless capital I by their own rules.
String localeLowerCase(String value, AppLocale locale) {
  if (locale == AppLocale.tr || locale == AppLocale.az) {
    value = value.replaceAll('I', 'ı').replaceAll('İ', 'i');
  }
  return value.toLowerCase();
}

final RegExp _trailingDigits = RegExp(r'(\d+)\s*$');

/// The trailing number of [value], or null (`value.match(/(\d+)\s*$/)?.[1]`).
String? trailingDigits(String value) =>
    _trailingDigits.firstMatch(value)?.group(1);

// ---------------------------------------------------------------------------
// Editor columns

/// One column of the Plan editor (`EditorColumn`).
final class EditorColumn {
  const EditorColumn({
    required this.key,
    required this.labelKey,
    this.kind,
    this.width,
    this.sheets,
    this.detail = false,
    this.bulk = true,
    this.display,
    this.disabledWhenField,
    this.disabledWhenEquals,
  });

  /// Reads a column; returns null for an entry without a string `key`.
  static EditorColumn? fromJson(Object? raw) {
    final json = recordValue(raw);
    if (json == null) return null;
    final key = json['key'];
    if (key is! String) return null;
    final sheets = json['sheets'];
    final disabledWhen = recordValue(json['disabledWhen']);
    return EditorColumn(
      key: key,
      labelKey: json['labelKey'] is String ? json['labelKey']! as String : key,
      kind: json['kind'] is String ? json['kind']! as String : null,
      width: json['width'] is num ? (json['width']! as num).toDouble() : null,
      sheets: sheets is List ? [for (final s in sheets) jsString(s)] : null,
      detail: jsTruthy(json['detail']),
      bulk: json['bulk'] != false,
      display: json['display'] is String ? json['display']! as String : null,
      disabledWhenField: disabledWhen == null
          ? null
          : jsString(disabledWhen['field']),
      disabledWhenEquals: disabledWhen?['equals'],
    );
  }

  final String key;
  final String labelKey;
  final String? kind;
  final double? width;
  final List<String>? sheets;
  final bool detail;
  final bool bulk;
  final String? display;
  final String? disabledWhenField;
  final Object? disabledWhenEquals;
}

/// The columns of an editor (`editor.columns`), skipping malformed entries.
List<EditorColumn> editorColumns(Map<String, Object?> editor) {
  final raw = editor['columns'];
  if (raw is! List) return const [];
  return [for (final entry in raw) ?EditorColumn.fromJson(entry)];
}

/// `editorColumnDefaultWidth`.
double editorColumnDefaultWidth(EditorColumn column) {
  if (column.width != null) return column.width!;
  if (column.kind == 'bool') return 50;
  return switch (column.key) {
    'sourceStatuses' || 'disposition' => 220,
    'reasonKey' => 180,
    'currentStatus' || 'proposedStatus' => 130,
    'rfiNo' => 120,
    '_sheet' => 60,
    '_row' => 50,
    _ => 180,
  };
}

/// `editorColumnMinWidth`.
double editorColumnMinWidth(EditorColumn column) =>
    column.kind == 'compactDate' ? 136 : 48;

final RegExp _itemPrefix = RegExp(r'^[A-Z]{2,5}[-˗‐-―]+', caseSensitive: false);

/// `displayEditorValue`: the column's display transform of a cell value.
String displayEditorValue(EditorColumn column, String value) {
  switch (column.display) {
    case 'suffix':
      return trailingDigits(value) ?? value;
    case 'shortNumber':
      final suffix = trailingDigits(value);
      return suffix == null ? value : int.parse(suffix).toString();
    case 'itemSuffix':
      return value.replaceFirst(_itemPrefix, '');
    case 'percent':
      return formatConfidencePercent(value) ?? '—';
  }
  return value;
}

// ---------------------------------------------------------------------------
// RFI codes and editor readiness

final RegExp _canonicalRfi = RegExp(
  r'^AZE-SUN-INT-QMS-RFI-(?:CIV|MEC|ELE|PRC)-\d{4}$',
);
final RegExp _partialRfi = RegExp(
  r'^AZE-SUN-INT-QMS-RFI-(?:CIV|MEC|ELE|PRC)-[0-9]{1,3}(?![\s\S])',
);
final RegExp _rfiInText = RegExp(
  r'(?:^|[^A-Z0-9-])(AZE-SUN-INT-QMS-RFI-[A-Z]{3}-[0-9]{4})(?=$|[^A-Z0-9-])',
);

/// `isRfiCodeCandidate`: a canonical code or one still being typed.
bool isRfiCodeCandidate(Object? value) =>
    value is String &&
    (_canonicalRfi.hasMatch(value) || _partialRfi.hasMatch(value));

/// `canonicalRfiInText`: the first canonical RFI code inside [value].
String? canonicalRfiInText(String value) =>
    _rfiInText.firstMatch(value)?.group(1);

/// `isPlanEditorReady`: every enabled row has its required fields, and a fast
/// scan editor has valid, unique codes.
bool isPlanEditorReady(
  Map<String, Object?> editor,
  List<Map<String, Object?>> rows,
) {
  final rawRequired = editor['requiredFields'];
  final requiredFields = rawRequired is List
      ? rawRequired.whereType<String>().toList()
      : const <String>[];
  final requiredValuesReady = rows.every(
    (row) =>
        row['enabled'] == false ||
        requiredFields.every((field) {
          final value = row[field];
          return value is String ? value.trim().isNotEmpty : value != null;
        }),
  );
  if (!requiredValuesReady) return false;
  if (editor['kind'] != 'rfiScan') return true;
  final codes = [for (final row in rows) row['code']];
  return codes.every(isRfiCodeCandidate) &&
      codes.toSet().length == codes.length;
}

// ---------------------------------------------------------------------------
// Impact summary

/// Counts of planned effects by kind (`PlanImpact`).
final class PlanImpact {
  int outputs = 0;
  int updates = 0;
  int moves = 0;
  int printJobs = 0;
  int drafts = 0;
  int cleanup = 0;
  int warnings = 0;
}

/// `summarizePlanImpact`.
PlanImpact summarizePlanImpact(Plan plan) {
  final impact = PlanImpact()..warnings = plan.warnings.length;
  for (final step in plan.steps) {
    switch (step.kind) {
      case StepKind.copy:
      case StepKind.write:
      case StepKind.pdfFirstPageReplace:
      case StepKind.pdfPageRotate:
      case StepKind.pdfPagesExtract:
        impact.outputs += 1;
      case StepKind.workbookUpdate:
      case StepKind.restoreBackup:
      case StepKind.replicaPublish:
        impact.updates += 1;
      case StepKind.move:
      case StepKind.rename:
        impact.moves += 1;
      case StepKind.print:
        impact.printJobs += 1;
      case StepKind.outlookReplyDraft:
        impact.drafts += 1;
      case StepKind.cleanupTemp:
        impact.cleanup += 1;
      case StepKind.mkdir:
        break;
    }
  }
  return impact;
}

// ---------------------------------------------------------------------------
// Workbook cell changes

/// One planned workbook cell edit (`WorkbookCellChange`).
final class WorkbookCellChange {
  const WorkbookCellChange({
    required this.workbook,
    required this.sheet,
    required this.cell,
    required this.before,
    required this.after,
    required this.rfiCode,
  });

  final String workbook;
  final String sheet;
  final String cell;
  final String before;
  final String after;
  final String rfiCode;
}

/// `MAX_WORKBOOK_CELL_PREVIEWS`.
const int maxWorkbookCellPreviews = 1000;

String? _previewText(Object? value) =>
    value is String && value.length <= 500 ? value : null;

/// `excelCell`: an A1 address for 1-based [row] and [column].
String? excelCell(Object? row, Object? column) {
  if (row is! num || column is! num) return null;
  if (row != row.truncate() || column != column.truncate()) return null;
  final r = row.toInt();
  var value = column.toInt();
  if (r < 1 || r > 1048576 || value < 1 || value > 16384) return null;
  var letters = '';
  while (value > 0) {
    value -= 1;
    letters = String.fromCharCode(65 + value % 26) + letters;
    value ~/= 26;
  }
  return '$letters$r';
}

/// `summarizeWorkbookCellChanges`: the cell edits the Plan's workbook steps
/// make, capped at [maxWorkbookCellPreviews].
List<WorkbookCellChange> summarizeWorkbookCellChanges(Plan plan) {
  final result = <WorkbookCellChange>[];
  final rowCodes = <String, String>{};
  final editor = recordValue(plan.meta['editor']);
  final rawRows = editor?['rows'];
  final editorRows = rawRows is List ? rawRows : const <Object?>[];
  for (final rawRow in editorRows.take(2000)) {
    final row = recordValue(rawRow);
    final rowId = _previewText(row?['_rowId']);
    final code =
        _previewText(row?['canonicalRfi']) ?? _previewText(row?['rfiNo']);
    if (rowId != null &&
        rowId.isNotEmpty &&
        code != null &&
        code.isNotEmpty &&
        canonicalRfiInText(code) == code) {
      rowCodes[rowId] = code;
    }
  }
  bool full() => result.length >= maxWorkbookCellPreviews;
  void add(WorkbookCellChange change) {
    if (!full()) result.add(change);
  }

  for (final step in plan.steps) {
    if (full()) return result;
    if (step.kind != StepKind.workbookUpdate) continue;
    final sync = recordValue(step.meta['syncStep']);
    final syncUpdates = recordValue(sync?['updates']);
    if (sync != null && syncUpdates != null) {
      final kind = sync['workbookKind'];
      final workbook = kind == 'rfiLog' || kind == 'followUp'
          ? kind! as String
          : 'workbook';
      for (final sheetName in jsObjectKeys(syncUpdates)) {
        final sheet = _previewText(sheetName);
        final sheetUpdates = recordValue(syncUpdates[sheetName]);
        if (sheet == null || sheet.isEmpty || sheetUpdates == null) continue;
        for (final key in jsObjectKeys(sheetUpdates)) {
          final update = recordValue(sheetUpdates[key]);
          final cell = excelCell(update?['row'], update?['col']);
          final before = _previewText(update?['expectedValue']);
          final after = _previewText(update?['replacementValue']);
          final rowId = _previewText(update?['rowId']);
          if (cell == null ||
              before == null ||
              after == null ||
              rowId == null ||
              rowId.isEmpty) {
            continue;
          }
          add(
            WorkbookCellChange(
              workbook: workbook,
              sheet: sheet,
              cell: cell,
              before: before,
              after: after,
              rfiCode: rowCodes[rowId] ?? '',
            ),
          );
          if (full()) return result;
        }
      }
      continue;
    }
    final rawUpdates = step.meta['updates'];
    final updates = rawUpdates is List ? rawUpdates : const <Object?>[];
    for (final rawUpdate in updates) {
      final update = recordValue(rawUpdate);
      final target = recordValue(update?['target']);
      final worksheet = recordValue(target?['worksheet']);
      final statusCell = recordValue(target?['statusCell']);
      final rfiCell = recordValue(target?['rfiCell']);
      final sheet = _previewText(worksheet?['name']);
      final cell = excelCell(target?['row'], target?['statusColumn']);
      final before = _previewText(statusCell?['value']);
      final after = _previewText(update?['proposedStatus']);
      final candidateCode = _previewText(update?['canonicalRfi']) ?? '';
      final rfiCode = canonicalRfiInText(candidateCode) == candidateCode
          ? candidateCode
          : '';
      final rfiAddress = excelCell(target?['row'], target?['rfiColumn']);
      final rfiBefore = _previewText(rfiCell?['value']);
      final rfiAfter = _previewText(update?['proposedRfi']);
      if (sheet != null &&
          sheet.isNotEmpty &&
          rfiAddress != null &&
          rfiBefore != null &&
          rfiAfter != null &&
          rfiBefore != rfiAfter) {
        add(
          WorkbookCellChange(
            workbook: 'followUp',
            sheet: sheet,
            cell: rfiAddress,
            before: rfiBefore,
            after: rfiAfter,
            rfiCode: rfiCode,
          ),
        );
        if (full()) return result;
      }
      if (sheet == null ||
          sheet.isEmpty ||
          cell == null ||
          before == null ||
          after == null) {
        continue;
      }
      add(
        WorkbookCellChange(
          workbook: 'followUp',
          sheet: sheet,
          cell: cell,
          before: before,
          after: after,
          rfiCode: rfiCode,
        ),
      );
      if (full()) return result;
    }
  }
  return result;
}

// ---------------------------------------------------------------------------
// Signed first-page staging

/// One reason a signed file needs review (`SignedWarningReason`).
final class SignedWarningReason {
  const SignedWarningReason({
    required this.key,
    required this.pageNumber,
    required this.code,
    required this.detail,
  });

  final String key;
  final int? pageNumber;
  final String code;
  final String detail;
}

/// A signed file that needs review (`SignedWarningReviewItem`).
final class SignedWarningReviewItem {
  const SignedWarningReviewItem({
    required this.sourceFile,
    required this.destination,
    required this.status,
    required this.reasons,
  });

  final String sourceFile;
  final String destination;

  /// `planned`, `exists` or `blocked`.
  final String status;
  final List<SignedWarningReason> reasons;
}

/// `signedWarningReviewItems`: `plan.meta.warningReviewItems`, skipping
/// malformed entries.
List<SignedWarningReviewItem> signedWarningReviewItems(Plan plan) {
  final raw = plan.meta['warningReviewItems'];
  if (raw is! List) return const [];
  final items = <SignedWarningReviewItem>[];
  for (final value in raw) {
    final item = recordValue(value);
    if (item == null) continue;
    final sourceFile = item['sourceFile'];
    final destination = item['destination'];
    final status = item['status'];
    final rawReasons = item['reasons'];
    if (sourceFile is! String ||
        destination is! String ||
        !const ['planned', 'exists', 'blocked'].contains(jsString(status)) ||
        rawReasons is! List) {
      continue;
    }
    final reasons = <SignedWarningReason>[];
    for (final rawReason in rawReasons) {
      final reason = recordValue(rawReason);
      if (reason == null) continue;
      final key = reason['key'];
      final detail = reason['detail'];
      if (key is! String || detail is! String) continue;
      final page = reason['pageNumber'];
      reasons.add(
        SignedWarningReason(
          key: key,
          pageNumber:
              page is num &&
                  page == page.truncate() &&
                  page.abs() <= 9007199254740991 &&
                  page > 0
              ? page.toInt()
              : null,
          code: reason['code'] is String ? reason['code']! as String : '',
          detail: detail,
        ),
      );
    }
    items.add(
      SignedWarningReviewItem(
        sourceFile: sourceFile,
        destination: destination,
        status: status! as String,
        reasons: reasons,
      ),
    );
  }
  return items;
}

final RegExp _base64Payload = RegExp(r'^[A-Za-z0-9+/]*={0,2}$');

/// `safePngThumbnail`: [value] when it is a bounded PNG data URI, else null.
String? safePngThumbnail(Object? value) {
  const prefix = 'data:image/png;base64,';
  const pngSignature = 'iVBORw0KGgo';
  if (value is! String || !value.startsWith(prefix) || value.length > 1100000) {
    return null;
  }
  final payload = value.substring(prefix.length);
  if (!payload.startsWith(pngSignature) ||
      payload.length % 4 != 0 ||
      !_base64Payload.hasMatch(payload)) {
    return null;
  }
  return value;
}

/// The Outlook reply draft a Plan previews (`outlookReplyPreview`).
typedef OutlookReplyPreview = ({String subject, String plainText, String html});

OutlookReplyPreview? outlookReplyPreview(Object? value) {
  final preview = recordValue(value);
  if (preview == null) return null;
  final subject = preview['subject'];
  final plainText = preview['plainText'];
  final html = preview['html'];
  if (subject is! String || plainText is! String || html is! String) {
    return null;
  }
  return (subject: subject, plainText: plainText, html: html);
}
