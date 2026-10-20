// Port of src-tauri/src/rfi_log.rs: RFI Log add preview, revise, table and
// clipboard payloads, workbook step updates and post-write verification.
//
// The Rust module is the authority. Ordering-sensitive maps use SplayTreeMap
// with rsCompare (Rust String order) and sorts are stable via rsStableSortBy.
//
// UNVERIFIED (not checkable on Linux without a Windows/macOS run):
//   * chrono `%y` pivot for two-digit years (assumed 00-69 -> 20xx, 70-99 -> 19xx).
//   * chrono leniency for one-digit day/month/hour fields (assumed accepted).

import 'dart:collection';
import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;
import 'dart:math' show Random;
import 'dart:typed_data';

import 'package:path/path.dart' as p;
import 'package:unorm_dart/unorm_dart.dart' as unorm;

import 'contracts.dart';
import 'email_extract.dart';
import 'errors.dart';
import 'fs_ops.dart' show fileSha256;
import 'gemini.dart';
import 'ooxml.dart';
import 'rfi_code.dart' show normalizeCandidateNumber;

const List<String> disciplines = ['CIV', 'MEC', 'ELE', 'PRC'];
const List<String> _requiredSheets = ['CIV', 'MEC', 'ELE', 'PRC', 'Sheet1'];
const int _dayStartMinutes = 9 * 60;
const int _dayEndMinutes = 17 * 60 + 30;
const int _slotMinutes = 30;
const int _maxPhysicalRows = 500;
const int _maxRowTextChars = 20000;
const int _maxWarningChars = 500;
const int _maxTableBytes = 8 * 1024 * 1024;
const String _workbookSelectionError = 'error.rfi.workbookInvalid';
const String _operationId = 'rfi.log_add';

const List<String> copyFields = [
  'rfiNo',
  'itpNo',
  'itemNo',
  'description',
  'forms',
  'tagNo',
  'area',
  'dateOfIssue',
  'inspectionDate',
  'startTime',
  'azfenType',
  'intecType',
  'location',
  'subcontractor',
];

const List<String> copyHeaders = [
  'RFI No.',
  'ITP No.',
  'Item No.',
  'INSPECTION DESCRIPTION',
  'FORMS',
  'TAG NO',
  'AREA',
  'DATE OF ISSUE',
  'INSPECTION DATE',
  'INSPECTION START TIME',
  'INSP. TYPE AZFEN',
  'INSP. TYPE INTEC',
  'Inspection Location',
  'SUBCONTRACTOR',
];

const List<String> _monthNames = [
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

typedef WorkbookCell = (int, int);

/// Workbook cell updates keyed by sheet name, then (row, column).
typedef WorkbookUpdates = Map<String, Map<WorkbookCell, CellUpdate>>;

/// Port of the Rust `Layout` (per-discipline column map).
final class Layout {
  const Layout({
    required this.sheet,
    required this.issueCol,
    required this.inspectionCol,
    required this.expiryCol,
    required this.timeCol,
    required this.azfenCol,
    required this.intecCol,
    required this.locationCol,
    required this.subcontractorCol,
    required this.statusCol,
    required this.remarksCol,
    required this.validityCol,
    this.rowCol,
  });

  final String sheet;
  final int issueCol;
  final int inspectionCol;
  final int expiryCol;
  final int timeCol;
  final int azfenCol;
  final int intecCol;
  final int locationCol;
  final int subcontractorCol;
  final int statusCol;
  final int remarksCol;
  final int validityCol;
  final int? rowCol;
}

final class RfiStatusColumns {
  const RfiStatusColumns({
    required this.sheet,
    required this.rfiColumn,
    required this.statusColumn,
  });

  final String sheet;
  final int rfiColumn;
  final int statusColumn;

  @override
  bool operator ==(Object other) =>
      other is RfiStatusColumns &&
      other.sheet == sheet &&
      other.rfiColumn == rfiColumn &&
      other.statusColumn == statusColumn;

  @override
  int get hashCode => Object.hash(sheet, rfiColumn, statusColumn);
}

List<RfiStatusColumns> statusColumns() => [
  for (final discipline in disciplines)
    RfiStatusColumns(
      sheet: layout(discipline)!.sheet,
      rfiColumn: 1,
      statusColumn: layout(discipline)!.remarksCol,
    ),
];

Layout? layout(String discipline) {
  switch (discipline) {
    case 'CIV':
      return const Layout(
        sheet: 'CIV',
        issueCol: 8,
        inspectionCol: 9,
        expiryCol: 10,
        timeCol: 11,
        azfenCol: 12,
        intecCol: 13,
        locationCol: 14,
        subcontractorCol: 15,
        statusCol: 16,
        remarksCol: 20,
        validityCol: 17,
      );
    case 'MEC':
      return const Layout(
        sheet: 'MEC',
        issueCol: 9,
        inspectionCol: 10,
        expiryCol: 11,
        timeCol: 12,
        azfenCol: 13,
        intecCol: 14,
        locationCol: 15,
        subcontractorCol: 16,
        statusCol: 17,
        remarksCol: 21,
        validityCol: 18,
        rowCol: 8,
      );
    case 'ELE':
      return const Layout(
        sheet: 'ELE',
        issueCol: 8,
        inspectionCol: 9,
        expiryCol: 10,
        timeCol: 11,
        azfenCol: 12,
        intecCol: 13,
        locationCol: 14,
        subcontractorCol: 15,
        statusCol: 16,
        remarksCol: 20,
        validityCol: 17,
      );
    case 'PRC':
      return const Layout(
        sheet: 'PRC',
        issueCol: 8,
        inspectionCol: 9,
        expiryCol: 10,
        timeCol: 11,
        azfenCol: 12,
        intecCol: 13,
        locationCol: 14,
        subcontractorCol: 15,
        statusCol: 16,
        remarksCol: 20,
        validityCol: 17,
      );
    default:
      return null;
  }
}

/// Port of the Rust `Preset` (serde camelCase).
final class Preset {
  const Preset({
    required this.description,
    required this.forms,
    required this.azfenType,
    required this.intecType,
  });

  factory Preset.fromJson(Object? json) {
    final map = _asObject(json);
    return Preset(
      description: _reqString(map, 'description'),
      forms: _reqString(map, 'forms'),
      azfenType: _reqString(map, 'azfenType'),
      intecType: _reqString(map, 'intecType'),
    );
  }

  final String description;
  final String forms;
  final String azfenType;
  final String intecType;

  Map<String, Object?> toJson() => {
    'description': description,
    'forms': forms,
    'azfenType': azfenType,
    'intecType': intecType,
  };

  @override
  bool operator ==(Object other) =>
      other is Preset &&
      other.description == description &&
      other.forms == forms &&
      other.azfenType == azfenType &&
      other.intecType == intecType;

  @override
  int get hashCode => Object.hash(description, forms, azfenType, intecType);
}

/// Port of the Rust `SourceData`.
final class SourceData {
  SourceData({
    required this.presets,
    required this.itpToItems,
    required this.lookupColumns,
    required this.tags,
    required this.areas,
    required this.locations,
    required this.subcontractors,
    required this.maxRow,
  });

  factory SourceData.fromJson(Object? json) {
    final map = _asObject(json);
    return SourceData(
      presets: _presetMap(map, 'presets'),
      itpToItems: _itpMap(map, 'itpToItems'),
      lookupColumns: _columnMap(map, 'lookupColumns'),
      tags: _reqStringList(map, 'tags'),
      areas: _reqStringList(map, 'areas'),
      locations: _reqStringList(map, 'locations'),
      subcontractors: _reqStringList(map, 'subcontractors'),
      maxRow: _reqU32(map, 'maxRow'),
    );
  }

  final SplayTreeMap<String, Preset> presets;
  final SplayTreeMap<String, List<String>> itpToItems;
  final SplayTreeMap<String, int> lookupColumns;
  final List<String> tags;
  final List<String> areas;
  final List<String> locations;
  final List<String> subcontractors;
  final int maxRow;

  Map<String, Object?> toJson() => {
    'presets': _presetsJson(presets),
    'itpToItems': _itpJson(itpToItems),
    'lookupColumns': Map<String, Object?>.of(lookupColumns),
    'tags': tags,
    'areas': areas,
    'locations': locations,
    'subcontractors': subcontractors,
    'maxRow': maxRow,
  };
}

/// Port of the Rust `Record` (serde `default`, camelCase, `_rowId`/`_sheet`).
final class Record {
  Record({
    this.rowId = '',
    this.sheet = '',
    this.rfiNo = '',
    this.itpNo = '',
    this.itemNo = '',
    this.description = '',
    this.forms = '',
    this.tagNo = '',
    this.area = '',
    this.rowNo = '',
    this.dateOfIssue = '',
    this.inspectionDate = '',
    this.startTime = '',
    this.azfenType = '',
    this.intecType = '',
    this.location = '',
    this.subcontractor = '',
  });

  factory Record.fromJson(Object? json) {
    final map = _asObject(json);
    return Record(
      rowId: _optString(map, '_rowId'),
      sheet: _optString(map, '_sheet'),
      rfiNo: _optString(map, 'rfiNo'),
      itpNo: _optString(map, 'itpNo'),
      itemNo: _optString(map, 'itemNo'),
      description: _optString(map, 'description'),
      forms: _optString(map, 'forms'),
      tagNo: _optString(map, 'tagNo'),
      area: _optString(map, 'area'),
      rowNo: _optString(map, 'rowNo'),
      dateOfIssue: _optString(map, 'dateOfIssue'),
      inspectionDate: _optString(map, 'inspectionDate'),
      startTime: _optString(map, 'startTime'),
      azfenType: _optString(map, 'azfenType'),
      intecType: _optString(map, 'intecType'),
      location: _optString(map, 'location'),
      subcontractor: _optString(map, 'subcontractor'),
    );
  }

  String rowId;
  String sheet;
  String rfiNo;
  String itpNo;
  String itemNo;
  String description;
  String forms;
  String tagNo;
  String area;
  String rowNo;
  String dateOfIssue;
  String inspectionDate;
  String startTime;
  String azfenType;
  String intecType;
  String location;
  String subcontractor;

  Record copy() => Record.fromJson(toJson());

  Map<String, Object?> toJson() => {
    '_rowId': rowId,
    '_sheet': sheet,
    'rfiNo': rfiNo,
    'itpNo': itpNo,
    'itemNo': itemNo,
    'description': description,
    'forms': forms,
    'tagNo': tagNo,
    'area': area,
    'rowNo': rowNo,
    'dateOfIssue': dateOfIssue,
    'inspectionDate': inspectionDate,
    'startTime': startTime,
    'azfenType': azfenType,
    'intecType': intecType,
    'location': location,
    'subcontractor': subcontractor,
  };
}

/// Port of the Rust `Batch` (one sheet's planned rows).
final class Batch {
  Batch({
    required this.discipline,
    required this.startRow,
    required this.records,
    required this.baseRfis,
    required this.presets,
    required this.lookupColumns,
    required this.maxRow,
    required this.sourceData,
  });

  factory Batch.fromJson(Object? json) {
    final map = _asObject(json);
    return Batch(
      discipline: _reqString(map, 'discipline'),
      startRow: _reqU32(map, 'startRow'),
      records: _reqList(map, 'records', Record.fromJson),
      baseRfis: _reqStringList(map, 'baseRfis'),
      presets: _presetMap(map, 'presets'),
      lookupColumns: _columnMap(map, 'lookupColumns'),
      maxRow: _reqU32(map, 'maxRow'),
      sourceData: SourceData.fromJson(map['sourceData']),
    );
  }

  final String discipline;
  final int startRow;
  final List<Record> records;
  final List<String> baseRfis;
  final SplayTreeMap<String, Preset> presets;
  final SplayTreeMap<String, int> lookupColumns;
  final int maxRow;
  final SourceData sourceData;

  Map<String, Object?> toJson() => {
    'discipline': discipline,
    'startRow': startRow,
    'records': [for (final record in records) record.toJson()],
    'baseRfis': baseRfis,
    'presets': _presetsJson(presets),
    'lookupColumns': Map<String, Object?>.of(lookupColumns),
    'maxRow': maxRow,
    'sourceData': sourceData.toJson(),
  };
}

/// Port of the Rust `CopySource` (clipboard source descriptor).
final class CopySource {
  const CopySource({
    required this.workbook,
    required this.sheet,
    required this.headerRow,
    required this.startRow,
    required this.rfiNumbers,
    required this.fields,
    required this.headers,
    required this.columns,
  });

  factory CopySource.fromJson(Object? json) {
    final map = _asObject(json);
    return CopySource(
      workbook: _reqString(map, 'workbook'),
      sheet: _reqString(map, 'sheet'),
      headerRow: _reqU32(map, 'headerRow'),
      startRow: _reqU32(map, 'startRow'),
      rfiNumbers: _reqStringList(map, 'rfiNumbers'),
      fields: _reqStringList(map, 'fields'),
      headers: _reqStringList(map, 'headers'),
      columns: _reqU32List(map, 'columns'),
    );
  }

  final String workbook;
  final String sheet;
  final int headerRow;
  final int startRow;
  final List<String> rfiNumbers;
  final List<String> fields;
  final List<String> headers;
  final List<int> columns;

  Map<String, Object?> toJson() => {
    'workbook': workbook,
    'sheet': sheet,
    'headerRow': headerRow,
    'startRow': startRow,
    'rfiNumbers': rfiNumbers,
    'fields': fields,
    'headers': headers,
    'columns': columns,
  };
}

/// Clipboard payload: HTML table, tab-separated plain text and row count.
final class ClipboardPayload {
  const ClipboardPayload({
    required this.html,
    required this.plain,
    required this.count,
  });

  final String html;
  final String plain;
  final int count;

  @override
  bool operator ==(Object other) =>
      other is ClipboardPayload &&
      other.html == html &&
      other.plain == plain &&
      other.count == count;

  @override
  int get hashCode => Object.hash(html, plain, count);
}

/// Internal per-discipline source read from the workbook (Rust `Source`).
final class _Source {
  _Source({
    required this.data,
    required this.defaultItem,
    required this.defaultItp,
    required this.maxNumber,
    required this.lastRow,
    required this.occupied,
  });

  final SourceData data;
  final String defaultItem;
  final String defaultItp;
  final int maxNumber;
  final int lastRow;

  /// Day -> (rfi -> minute-of-day). Minute-of-day equals Rust `NaiveTime`
  /// ordering because every stored time has zero seconds.
  final Map<DateTime, Map<String, int>> occupied;
}

/// Preview: builds a single workbook-update plan for the requested sheets.
Future<Plan> preview(
  Map<String, Object?> values,
  DateTime today, {
  GeminiClient? gemini,
  void Function(ProgressEvent event)? progress,
  bool Function()? cancelled,
}) async {
  void checkpoint() {
    if (cancelled?.call() ?? false) {
      throw BackendError.cancelled();
    }
  }

  void report(String phase, int done, int total) {
    progress?.call(
      ProgressEvent(phase: phase, done: done, total: total, subject: ''),
    );
  }

  checkpoint();
  final workbookPath = _workbookPath(values);
  final emailText = rsTrim(_valueString(values, 'email_text'));
  final wrap = values['overflow_next_day'] is bool
      ? values['overflow_next_day']! as bool
      : true;
  var counts = _requestedSheetCounts(values);
  final detected = emailText.isNotEmpty ? detectDiscipline(emailText) : null;
  final hasNewSelection = values['sheet_counts'] is Map;
  if (detected != null && _isDiscipline(detected)) {
    if (!hasNewSelection || (counts.length == 1 && counts['CIV'] == 3)) {
      final count = counts['CIV'] ?? 3;
      counts = {detected: count};
    } else if (!counts.containsKey(detected)) {
      counts[detected] = 1;
    }
  }
  final requested = [
    for (final discipline in disciplines)
      if (counts.containsKey(discipline)) discipline,
  ];
  report('workbook_reading', 0, 1);
  final workbook = _readWorkbook(workbookPath);
  checkpoint();
  final sources = <String, _Source>{};
  for (final discipline in requested) {
    sources[discipline] = _readSource(workbook, discipline);
    checkpoint();
  }
  report('workbook_reading', 1, 1);
  report('rfi_request_analysis', 0, 1);

  EmailExtraction? extraction;
  String? extractionDiscipline;
  final extractionNotes = <String>[];
  final warnings = <Warning>[];
  if (emailText.isNotEmpty) {
    final discipline = (detected != null && sources.containsKey(detected))
        ? detected
        : requested.first;
    final source = sources[discipline]!;
    final context = EmailContext(
      discipline: discipline,
      itpToItems: source.data.itpToItems,
      knownTags: source.data.tags,
      knownSubcontractors: source.data.subcontractors,
    );
    final offline = offlineExtract(emailText, context);
    final providerRequested = rsAsciiLower(
      _valueString(values, 'email_provider'),
    );
    final provider = providerRequested == 'gemini' ? 'gemini' : 'offline';
    var selected = offline;
    String? fallbackError;
    if (provider == 'gemini') {
      checkpoint();
      try {
        selected = await _extractWithGemini(
          values,
          emailText,
          discipline,
          source.data,
          gemini,
        );
      } on BackendError catch (error) {
        selected = offline;
        fallbackError = _safeError(error);
      }
    }
    if (selected.count >= 1 && selected.count <= maxRfiCount) {
      counts[discipline] = selected.count;
    }
    extractionNotes.addAll(
      selected.notes.where((note) => rsTrim(note).isNotEmpty),
    );
    warnings.add(
      Warning(
        key: 'warn.rfi.emailExtracted',
        subject: '${selected.provider} | $discipline ${counts[discipline]} RFI',
      ),
    );
    if (provider == 'gemini') {
      warnings.add(const Warning(key: 'warn.rfi.geminiPrivacy', subject: ''));
    }
    if (fallbackError != null) {
      warnings.add(
        Warning(
          key: 'warn.rfi.aiFallback',
          subject: rsTruncateChars(fallbackError, 300),
        ),
      );
    }
    extractionDiscipline = discipline;
    extraction = selected;
  }
  checkpoint();
  report('rfi_request_analysis', 1, 1);
  report('rfi_plan_preparation', 0, requested.length);

  final issueDay = DateTime.utc(today.year, today.month, today.day);
  final startDay = issueDay.add(const Duration(days: 1));
  final rows = <Record>[];
  final batches = <Batch>[];
  var overflowed = 0;
  for (
    var disciplineIndex = 0;
    disciplineIndex < requested.length;
    disciplineIndex++
  ) {
    final discipline = requested[disciplineIndex];
    final source = sources[discipline]!;
    final allocated = _allocate(
      source.occupied,
      startDay,
      counts[discipline]!,
      wrap,
    );
    overflowed += allocated.$2;
    var sheetRows = <Record>[];
    final baseRfis = <String>[];
    for (var index = 0; index < allocated.$1.length; index++) {
      final slot = allocated.$1[index];
      final number = source.maxNumber + index + 1;
      final rfi =
          'AZE-SUN-INT-QMS-RFI-$discipline-${number.toString().padLeft(4, '0')}';
      baseRfis.add(rfi);
      sheetRows.add(_record(index, discipline, rfi, source, issueDay, slot));
    }
    if (extractionDiscipline == discipline && extraction != null) {
      final (extractedRows, notes) = _rowsFromEmail(
        sheetRows,
        extraction,
        source,
      );
      sheetRows = extractedRows;
      extractionNotes.addAll(notes);
    }
    for (final row in sheetRows) {
      _cleanRecord(row);
    }
    final startRow = source.lastRow + 1;
    final endRow = startRow + math.max(0, sheetRows.length - 1);
    if (endRow > source.data.maxRow) {
      throw BackendError.validation(
        "$discipline sheet'inde yeni satirlar icin ayrilmis kapasite kalmadi",
      );
    }
    batches.add(
      Batch(
        discipline: discipline,
        startRow: startRow,
        records: [for (final row in sheetRows) row.copy()],
        baseRfis: baseRfis,
        presets: source.data.presets,
        lookupColumns: source.data.lookupColumns,
        maxRow: source.data.maxRow,
        sourceData: source.data,
      ),
    );
    rows.addAll(sheetRows);
    report('rfi_plan_preparation', disciplineIndex + 1, requested.length);
    checkpoint();
  }
  if (extractionNotes.isNotEmpty) {
    rsDeduplicate(extractionNotes);
    warnings.add(
      Warning(
        key: 'warn.rfi.emailNeedsReview',
        subject: rsTruncateChars(extractionNotes.join(' | '), _maxWarningChars),
      ),
    );
  }
  warnings.addAll(_warningsForRows(rows, overflowed, wrap));
  final signature = _signature(workbookPath);
  final extension = p.extension(workbookPath).replaceFirst('.', '');
  final stem = p.basenameWithoutExtension(workbookPath);
  final backupName =
      '${stem.isEmpty ? 'RFI Log' : stem}_${_localStamp(DateTime.now())}_'
      '${_randomHex(8)}.${extension.isEmpty ? 'xlsx' : extension}';
  final backupPath = p.join(
    p.dirname(workbookPath),
    '.docflow-backups',
    backupName,
  );
  final stepMeta = <String, Object?>{
    'sourceSignature': signature,
    'batches': [for (final batch in batches) batch.toJson()],
  };
  if (batches.length == 1) {
    final batch = batches.first;
    stepMeta['discipline'] = batch.discipline;
    stepMeta['startRow'] = batch.startRow;
    stepMeta['records'] = [for (final record in batch.records) record.toJson()];
    stepMeta['baseRfis'] = batch.baseRfis;
    stepMeta['presets'] = _presetsJson(batch.presets);
    stepMeta['lookupColumns'] = Map<String, Object?>.of(batch.lookupColumns);
    stepMeta['sourceData'] = batch.sourceData.toJson();
  }
  final step = Step(
    kind: StepKind.workbookUpdate,
    src: workbookPath,
    dst: backupPath,
    noteKey: 'note.rfi.workbookUpdate',
    meta: stepMeta,
  );
  final meta = _planMeta({
    for (final entry in sources.entries) entry.key: entry.value.data,
  }, rows);
  final copySources = <String, Object?>{
    for (final batch in batches)
      batch.discipline: _copySource(workbookPath, batch).toJson(),
  };
  final editor = meta['editor']! as Map<String, Object?>;
  editor['copySources'] = copySources;
  if (copySources.length == 1) {
    editor['copySource'] = copySources.values.first;
  }
  if (extraction != null && extractionDiscipline != null) {
    editor['emailExtraction'] = <String, Object?>{
      'provider': extraction.provider,
      'confidence': extraction.confidence,
      'count': counts[extractionDiscipline] ?? 0,
      'sheet': extractionDiscipline,
    };
  }
  report('preview_ready', 1, 1);
  return Plan(
    planId: '',
    opId: _operationId,
    steps: [step],
    warnings: warnings,
    meta: meta,
  );
}

/// Revise: re-validates edited rows against the existing plan's batches.
Plan revise(Plan plan, Object? payload) {
  final step = _onlyWorkbookStep(plan);
  final batches = _decodeBatches(step);
  final rawRows = payload is Map ? payload['rows'] : null;
  if (rawRows is! List) {
    throw BackendError.validation('gecersiz RFI satiri');
  }
  if (rawRows.length > _maxPhysicalRows) {
    throw BackendError.validation(
      'tek planda en fazla 500 fiziksel satir olabilir',
    );
  }
  final allowed = {for (final batch in batches) batch.discipline};
  final supplied = <String>{
    for (final row in rawRows)
      if (row is Map && row['_sheet'] is String)
        rsAsciiUpper(row['_sheet']! as String),
  };
  if (!_setEquals(supplied, allowed)) {
    throw BackendError.validation(
      'RFI sheet gruplari eklenemez veya silinemez',
    );
  }
  final revisedBatches = <Batch>[];
  final revisedRows = <Record>[];
  final sources = <String, SourceData>{};
  for (final batch in batches) {
    final sheetValues = [
      for (final row in rawRows)
        if (row is Map &&
            row['_sheet'] is String &&
            rsEqualsIgnoreAsciiCase(row['_sheet']! as String, batch.discipline))
          row,
    ];
    final records = <Record>[];
    for (var index = 0; index < sheetValues.length; index++) {
      final Record record;
      try {
        record = Record.fromJson(sheetValues[index]);
      } on Object {
        throw BackendError.validation('gecersiz RFI satiri');
      }
      _cleanRecord(record);
      _validateRecord(record, index, batch);
      records.add(record);
    }
    if (records.isEmpty) {
      throw BackendError.validation('en az bir RFI satiri gerekli');
    }
    final present = {for (final record in records) record.rfiNo};
    final expected = batch.baseRfis.toSet();
    if (!_setEquals(present, expected)) {
      final missing = [
        for (final rfi in expected)
          if (!present.contains(rfi)) rfi,
      ];
      throw BackendError.validation(
        'RFI grubu silinemez: ${missing.join(', ')}',
      );
    }
    final end = batch.startRow + math.max(0, records.length - 1);
    if (end > batch.maxRow) {
      throw BackendError.validation(
        "${batch.discipline} sheet'inde kopyalar icin kapasite kalmadi",
      );
    }
    revisedBatches.add(
      Batch(
        discipline: batch.discipline,
        startRow: batch.startRow,
        records: [for (final record in records) record.copy()],
        baseRfis: batch.baseRfis,
        presets: batch.presets,
        lookupColumns: batch.lookupColumns,
        maxRow: batch.maxRow,
        sourceData: batch.sourceData,
      ),
    );
    sources[batch.discipline] = batch.sourceData;
    revisedRows.addAll(records);
  }
  final revisedStepMeta = Map<String, Object?>.of(step.meta);
  revisedStepMeta['batches'] = [
    for (final batch in revisedBatches) batch.toJson(),
  ];
  if (revisedBatches.length == 1) {
    revisedStepMeta['records'] = [
      for (final record in revisedBatches.first.records) record.toJson(),
    ];
  }
  final revisedStep = Step(
    kind: step.kind,
    src: step.src,
    dst: step.dst,
    noteKey: step.noteKey,
    meta: revisedStepMeta,
  );
  final meta = _planMeta(sources, revisedRows);
  final workbook = step.src ?? '';
  final copySources = <String, Object?>{
    for (final batch in revisedBatches)
      batch.discipline: _copySource(workbook, batch).toJson(),
  };
  final editor = meta['editor']! as Map<String, Object?>;
  editor['copySources'] = copySources;
  if (copySources.length == 1) {
    editor['copySource'] = copySources.values.first;
  }
  final oldEditor = plan.meta['editor'];
  if (oldEditor is Map && oldEditor['emailExtraction'] != null) {
    editor['emailExtraction'] = oldEditor['emailExtraction'];
  }
  return Plan(
    planId: '',
    opId: _operationId,
    steps: [revisedStep],
    warnings: _warningsForRows(revisedRows, 0, true),
    meta: meta,
  );
}

/// Table preview (headers, fields, rows, widths) for one sheet.
Map<String, Object?> previewTable(Plan plan, {String? sheet}) {
  final data = _copyData(plan, sheet);
  final table = <String, Object?>{
    'headers': data.headers,
    'fields': data.fields,
    'rows': [
      for (final row in data.rows)
        [for (final field in data.fields) row[field] ?? ''],
    ],
    'columnWidths': [
      for (final width in data.widths) width.clamp(7.0, 42.0).toDouble(),
    ],
    'fromWorkbook': data.fromWorkbook,
    'count': data.rows.length,
  };
  if (utf8.encode(jsonEncode(table)).length <= _maxTableBytes) {
    return table;
  }
  throw BackendError.validation('table preview exceeded the size limit');
}

/// Clipboard payload for one sheet of the plan.
ClipboardPayload copyPayload(Plan plan, {String? sheet}) {
  final data = _copyData(plan, sheet);
  final plainRows = [
    for (final row in data.rows)
      [for (final field in data.fields) row[field] ?? ''].join('\t'),
  ];
  final plain = [data.headers.join('\t'), ...plainRows].join('\r\n');
  final headings = data.headers
      .map(
        (header) =>
            '<th style="border:1px solid #8b8b8b;padding:6px 8px;font-weight:700;text-align:center;vertical-align:middle;">${_htmlEscape(header)}</th>',
      )
      .join();
  final body = data.rows.map((row) {
    final cells = data.fields.map((field) {
      final align = const {'description', 'forms', 'location'}.contains(field)
          ? 'left'
          : 'center';
      final value = _htmlEscape(row[field] ?? '').replaceAll('\n', '<br>');
      return '<td style="border:1px solid #9a9a9a;padding:6px 8px;text-align:$align;vertical-align:middle;white-space:normal;">$value</td>';
    }).join();
    return '<tr>$cells</tr>';
  }).join();
  final html =
      '<table cellpadding="0" cellspacing="0" style="border-collapse:collapse;font-family:Arial,sans-serif;font-size:10pt;color:#202020;"><thead><tr style="background:#dedede;">$headings</tr></thead><tbody>$body</tbody></table>';
  if (rsUtf8Len(plain) > _maxTableBytes || rsUtf8Len(html) > _maxTableBytes) {
    throw BackendError.validation('clipboard table exceeded the size limit');
  }
  return ClipboardPayload(html: html, plain: plain, count: data.rows.length);
}

/// Clipboard payload covering every sheet in the plan.
ClipboardPayload replyPayload(Plan plan) {
  final editor = plan.meta['editor'];
  if (editor is! Map) {
    throw BackendError.validation('bu planda kopyalanabilir tablo yok');
  }
  final sources = editor['copySources'];
  if (sources is! Map) {
    throw BackendError.validation('bu planda kopyalanabilir tablo yok');
  }
  final sheets = [
    for (final sheet in disciplines)
      if (sources.containsKey(sheet)) sheet,
  ];
  if (sheets.isEmpty) {
    throw BackendError.validation('bu planda kopyalanabilir tablo yok');
  }
  final multiple = sheets.length > 1;
  final plainParts = <String>[];
  final htmlParts = <String>[];
  var count = 0;
  for (final sheet in sheets) {
    final payload = copyPayload(plan, sheet: sheet);
    count += payload.count;
    plainParts.add(multiple ? '$sheet\r\n${payload.plain}' : payload.plain);
    htmlParts.add(
      multiple
          ? '<div style="font-family:Arial,sans-serif;font-size:10pt;font-weight:700;margin:0 0 6px 0;">$sheet</div>${payload.html}'
          : payload.html,
    );
  }
  final plain = plainParts.join('\r\n\r\n');
  final html = '${htmlParts.join('<br>')}<br><br>';
  if (rsUtf8Len(plain) > _maxTableBytes || rsUtf8Len(html) > _maxTableBytes) {
    throw BackendError.validation('clipboard table exceeded the size limit');
  }
  return ClipboardPayload(html: html, plain: plain, count: count);
}

/// Cell updates for every batch of a workbook step, keyed by sheet name.
WorkbookUpdates workbookStepUpdates(Step step) {
  final batches = _decodeBatches(step);
  final sheets = <String, Map<WorkbookCell, CellUpdate>>{};
  for (final batch in batches) {
    final layoutOf = layout(batch.discipline);
    if (layoutOf == null) {
      throw BackendError.validation('gecersiz sheet plani');
    }
    final updates = <WorkbookCell, CellUpdate>{};
    for (var index = 0; index < batch.records.length; index++) {
      final record = batch.records[index];
      final row = batch.startRow + index;
      final values = <(int, CellUpdate)>[
        (1, CellUpdateText(record.rfiNo)),
        (2, CellUpdateText(record.itpNo)),
        (3, CellUpdateText(record.itemNo)),
        (6, CellUpdateText(record.tagNo)),
        (7, CellUpdateText(record.area)),
        (
          layoutOf.issueCol,
          CellUpdateNumber(
            _excelDate(_parseDate(record.dateOfIssue)).toString(),
          ),
        ),
        (
          layoutOf.inspectionCol,
          CellUpdateNumber(
            _excelDate(_parseDate(record.inspectionDate)).toString(),
          ),
        ),
        (
          layoutOf.timeCol,
          CellUpdateNumber(_excelTime(_parseTime(record.startTime))),
        ),
        (layoutOf.locationCol, CellUpdateText(record.location)),
        (layoutOf.subcontractorCol, CellUpdateText(record.subcontractor)),
      ];
      for (final (column, update) in values) {
        updates[(row, column)] = update;
      }
      if (layoutOf.rowCol != null) {
        updates[(row, layoutOf.rowCol!)] = CellUpdateText(record.rowNo);
      }
      final preset = batch.presets[record.itemNo];
      if (preset == null) {
        throw BackendError.validation('bilinmeyen Item No.: ${record.itemNo}');
      }
      final lookup = batch.lookupColumns;
      updates[(row, 4)] = _formulaOrValue(
        record.description,
        preset.description,
        'IFERROR(VLOOKUP(C$row,Sheet1!${_columnName(lookup['item']!)}:${_columnName(lookup['description']!)},2,0),"")',
      );
      updates[(row, 5)] = _formulaOrValue(
        record.forms,
        preset.forms,
        'IFERROR(VLOOKUP(D$row,Sheet1!${_columnName(lookup['description']!)}:${_columnName(lookup['forms']!)},2,0),"")',
      );
      updates[(row, layoutOf.azfenCol)] = _formulaOrValue(
        record.azfenType,
        preset.azfenType,
        'IFERROR(VLOOKUP(C$row,Sheet1!${_columnName(lookup['azfenItem']!)}:${_columnName(lookup['azfenType']!)},2,0),"")',
      );
      updates[(row, layoutOf.intecCol)] = _formulaOrValue(
        record.intecType,
        preset.intecType,
        'IFERROR(VLOOKUP(C$row,Sheet1!${_columnName(lookup['intecItem']!)}:${_columnName(lookup['intecType']!)},2,0),"")',
      );
    }
    sheets[layoutOf.sheet] = _sortedUpdates(updates);
  }
  return sheets;
}

/// Total number of planned workbook records.
int workbookRecordCount(Step step) {
  return _decodeBatches(step)
      .fold<int>(0, (sum, batch) => sum + batch.records.length);
}

/// Re-reads the written workbook and checks every planned cell.
void verifyWorkbookStep(Step step, String path) {
  final workbook = _readWorkbook(path);
  for (final batch in _decodeBatches(step)) {
    final layoutOf = layout(batch.discipline);
    if (layoutOf == null) {
      throw BackendError.validation('gecersiz sheet plani');
    }
    final sheet = workbook.sheet(layoutOf.sheet);
    if (sheet == null) {
      throw BackendError.validation('yazma dogrulamasi basarisiz');
    }
    for (var index = 0; index < batch.records.length; index++) {
      final record = batch.records[index];
      final row = batch.startRow + index;
      final checks = <(int, String)>[
        (1, record.rfiNo),
        (2, record.itpNo),
        (3, record.itemNo),
        (6, record.tagNo),
        (7, record.area),
        (layoutOf.locationCol, record.location),
        (layoutOf.subcontractorCol, record.subcontractor),
      ];
      for (final (column, expected) in checks) {
        if (sheet.value(row, column) != expected) {
          throw BackendError.requestFailed(
            'yazma dogrulamasi basarisiz: ${layoutOf.sheet}!${_columnName(column)}$row',
          );
        }
      }
      if (layoutOf.expiryCol == layoutOf.issueCol ||
          layoutOf.statusCol == layoutOf.inspectionCol ||
          layoutOf.validityCol == layoutOf.timeCol) {
        throw BackendError.protocol('gecersiz kolon eslemesi');
      }
    }
  }
}

// ---------------------------------------------------------------------------
// Input and workbook helpers
// ---------------------------------------------------------------------------

String _workbookPath(Map<String, Object?> values) {
  final raw = values['workbook'];
  if (raw is! String) {
    throw BackendError.validation(_workbookSelectionError);
  }
  final extension = p.extension(raw).replaceFirst('.', '').toLowerCase();
  if (!(extension == 'xlsx' || extension == 'xlsm') ||
      !File(raw).existsSync()) {
    throw BackendError.validation(_workbookSelectionError);
  }
  try {
    return File(raw).resolveSymbolicLinksSync();
  } on FileSystemException {
    throw BackendError.pathInvalid('workbook path cannot be canonicalized');
  }
}

Map<String, int> _requestedSheetCounts(Map<String, Object?> values) {
  final counts = <String, int>{};
  final raw = values['sheet_counts'];
  if (raw is Map) {
    for (final discipline in disciplines) {
      final value = raw[discipline];
      final count = value == null
          ? 0
          : _protocolInteger(value) ??
                (throw BackendError.validation(
                  '$discipline RFI adedi gecersiz',
                ));
      if (count < 0 || count > maxRfiCount) {
        throw BackendError.validation(
          '$discipline RFI adedi 0-$maxRfiCount arasinda olmali',
        );
      }
      if (count > 0) {
        counts[discipline] = count;
      }
    }
  } else {
    var discipline = rsAsciiUpper(_valueString(values, 'discipline'));
    if (discipline.isEmpty) {
      discipline = 'CIV';
    }
    final value = values['count'];
    final count = value == null
        ? 0
        : _protocolInteger(value) ??
              (throw BackendError.validation('$discipline RFI adedi gecersiz'));
    if (count < 0 || count > maxRfiCount) {
      throw BackendError.validation(
        '$discipline RFI adedi 0-$maxRfiCount arasinda olmali',
      );
    }
    if (_isDiscipline(discipline) && count > 0) {
      counts[discipline] = count;
    }
  }
  if (counts.isEmpty) {
    throw BackendError.validation('en az bir sheet icin RFI adedi secin');
  }
  return counts;
}

int? _protocolInteger(Object? value) {
  if (value is bool) {
    return value ? 1 : 0;
  }
  if (value is int) {
    return value;
  }
  if (value is double) {
    if (value.isFinite &&
        value >= -9.2233720368547758e18 &&
        value <= 9.2233720368547758e18) {
      return value.truncate();
    }
    return null;
  }
  if (value is String) {
    return int.tryParse(rsTrim(value));
  }
  return null;
}

_Source _readSource(WorkbookData workbook, String discipline) {
  final layoutOf = layout(discipline);
  if (layoutOf == null) {
    throw BackendError.validation("gecersiz RFI sheet'i");
  }
  final sheet = workbook.sheet(layoutOf.sheet);
  final lookup = workbook.sheet('Sheet1');
  if (sheet == null || lookup == null) {
    throw BackendError.validation(
      'RFI Log icin ${layoutOf.sheet}/Sheet1 sayfalari bulunamadi',
    );
  }
  final lookupColumns = _lookupColumns(lookup);
  final presets = SplayTreeMap<String, Preset>(rsCompare);
  for (var row = 1; row <= lookup.maxRow; row++) {
    final item = _cleanText(lookup.value(row, lookupColumns['item']!));
    final parts = _itemParts(item);
    if (parts == null || parts.$1 != discipline) {
      continue;
    }
    final azfenItem = _cleanText(
      lookup.value(row, lookupColumns['azfenItem']!),
    );
    final intecItem = _cleanText(
      lookup.value(row, lookupColumns['intecItem']!),
    );
    if (azfenItem != item || intecItem != item) {
      throw BackendError.validation(
        '$discipline Sheet1 Item eslemesi tutarsiz: $item',
      );
    }
    final preset = Preset(
      description: _cleanText(lookup.value(row, lookupColumns['description']!)),
      forms: _cleanText(lookup.value(row, lookupColumns['forms']!)),
      azfenType: _cleanText(lookup.value(row, lookupColumns['azfenType']!)),
      intecType: _cleanText(lookup.value(row, lookupColumns['intecType']!)),
    );
    final missing = [
      if (preset.description.isEmpty) 'description',
      if (preset.forms.isEmpty) 'forms',
      if (preset.azfenType.isEmpty) 'azfenType',
      if (preset.intecType.isEmpty) 'intecType',
    ];
    if (missing.isNotEmpty) {
      throw BackendError.validation(
        '$discipline Sheet1 esleme verisi eksik: $item / ${missing.join(', ')}',
      );
    }
    final existing = presets[item];
    if (existing != null && existing != preset) {
      throw BackendError.validation(
        '$discipline Sheet1 cakisan Item eslemesi: $item',
      );
    }
    presets[item] = preset;
  }
  if (presets.isEmpty) {
    throw BackendError.validation(
      '$discipline icin Sheet1 esleme verisi bulunamadi',
    );
  }
  var maxNumber = 0;
  var lastRow = 9;
  var latestItem = '';
  final tags = <String>[];
  final areas = <String>[];
  final locations = <String>[];
  final subcontractors = <String>[];
  final occupied = <DateTime, Map<String, int>>{};
  for (var row = 10; row <= sheet.maxRow; row++) {
    final rfi = rsTrim(sheet.value(row, 1));
    final parts = _rfiParts(rfi);
    if (parts == null || parts.$1 != discipline) {
      continue;
    }
    maxNumber = math.max(maxNumber, parts.$2);
    lastRow = row;
    final item = _cleanText(sheet.value(row, 3));
    if (item.isNotEmpty) {
      latestItem = item;
    }
    tags.add(_cleanText(sheet.value(row, 6)));
    areas.add(_cleanText(sheet.value(row, 7)));
    locations.add(_cleanText(sheet.value(row, layoutOf.locationCol)));
    subcontractors.add(_cleanText(sheet.value(row, layoutOf.subcontractorCol)));
    final day = _cellDate(sheet.value(row, layoutOf.inspectionCol));
    final time = _cellTime(sheet.value(row, layoutOf.timeCol));
    if (day != null && time != null) {
      (occupied[day] ??= {})[rfi] = time;
    }
  }
  final workbookItps = <int, String>{};
  for (var column = 1; column <= lookup.maxColumn; column++) {
    final label = _cleanText(lookup.value(1, column));
    final upper = rsAsciiUpper(label);
    if (upper.contains('ITP') && upper.contains(discipline)) {
      final number = _trailingFourDigits(label);
      if (number != null) {
        workbookItps[number] = label;
      }
    }
  }
  final itpToItems = SplayTreeMap<String, List<String>>(rsCompare);
  for (final item in presets.keys) {
    final number = _itemParts(item)!.$2;
    final itp = workbookItps[number] ?? _canonicalItp(discipline, number);
    (itpToItems[itp] ??= []).add(item);
  }
  for (final items in itpToItems.values) {
    rsStableSortBy(items, _naturalKey, rsCompareLists);
  }
  final defaultItem = presets.containsKey(latestItem)
      ? latestItem
      : presets.keys.first;
  final defaultNumber = _itemParts(defaultItem)!.$2;
  final defaultItp =
      workbookItps[defaultNumber] ?? _canonicalItp(discipline, defaultNumber);
  return _Source(
    data: SourceData(
      presets: presets,
      itpToItems: itpToItems,
      lookupColumns: SplayTreeMap<String, int>(rsCompare)
        ..addAll(lookupColumns),
      tags: _recentUnique(tags),
      areas: _recentUnique(areas),
      locations: _recentUnique(locations),
      subcontractors: _recentUnique(subcontractors),
      maxRow: sheet.maxRow,
    ),
    defaultItem: defaultItem,
    defaultItp: defaultItp,
    maxNumber: maxNumber,
    lastRow: lastRow,
    occupied: occupied,
  );
}

Map<String, int> _lookupColumns(WorksheetData sheet) {
  for (var start = 1; start <= math.max(0, sheet.maxColumn - 6); start++) {
    if (!_isItemHeader(sheet.value(1, start))) {
      continue;
    }
    if (!_isItemHeader(sheet.value(1, start + 3)) ||
        !_isItemHeader(sheet.value(1, start + 5))) {
      continue;
    }
    return {
      'item': start,
      'description': start + 1,
      'forms': start + 2,
      'azfenItem': start + 3,
      'azfenType': start + 4,
      'intecItem': start + 5,
      'intecType': start + 6,
    };
  }
  throw BackendError.validation(
    'Sheet1 Item/description/forms/type esleme kolonlari bulunamadi',
  );
}

bool _isItemHeader(String value) {
  final key = _headerKey(value);
  return key == 'item' || key == 'itemno' || key == 'itemnumber';
}

(List<(DateTime, int)>, int) _allocate(
  Map<DateTime, Map<String, int>> occupied,
  DateTime startDay,
  int count,
  bool wrap,
) {
  final working = <DateTime, Map<String, int>>{
    for (final entry in occupied.entries)
      entry.key: Map<String, int>.of(entry.value),
  };
  final slots = <(DateTime, int)>[];
  var overflowed = 0;
  var day = startDay;
  for (var index = 0; index < count; index++) {
    var minutes = _nextSlotMinutes(working, day);
    if (minutes > _dayEndMinutes) {
      overflowed++;
      if (wrap) {
        while (minutes > _dayEndMinutes) {
          day = day.add(const Duration(days: 1));
          minutes = _nextSlotMinutes(working, day);
        }
      }
    }
    final time = minutes % (24 * 60);
    (working[day] ??= {})['__planned_$index'] = time;
    slots.add((day, time));
  }
  return (slots, overflowed);
}

int _nextSlotMinutes(Map<DateTime, Map<String, int>> occupied, DateTime day) {
  final values = occupied[day];
  if (values == null || values.isEmpty) {
    return _dayStartMinutes;
  }
  var latest = values.values.first;
  for (final value in values.values) {
    if (value > latest) {
      latest = value;
    }
  }
  return latest + _slotMinutes;
}

Record _record(
  int index,
  String discipline,
  String rfi,
  _Source source,
  DateTime issueDay,
  (DateTime, int) slot,
) {
  final preset = source.data.presets[source.defaultItem]!;
  return Record(
    rowId: '$discipline-g${index + 1}-r1',
    sheet: discipline,
    rfiNo: rfi,
    itpNo: source.defaultItp,
    itemNo: source.defaultItem,
    description: preset.description,
    forms: preset.forms,
    tagNo: '',
    area: '',
    rowNo: '',
    dateOfIssue: _dateText(issueDay),
    inspectionDate: _dateText(slot.$1),
    startTime: _timeText(slot.$2),
    azfenType: preset.azfenType,
    intecType: preset.intecType,
    location: '',
    subcontractor: 'AZFEN',
  );
}

(List<Record>, List<String>) _rowsFromEmail(
  List<Record> baseRows,
  EmailExtraction extraction,
  _Source source,
) {
  if (extraction.rows.isEmpty) {
    return (baseRows, <String>[]);
  }
  final grouped = <int, List<EmailRow>>{};
  for (final row in extraction.rows.take(maxRfiCount)) {
    if (row.rfiIndex >= 1 && row.rfiIndex <= baseRows.length) {
      (grouped[row.rfiIndex - 1] ??= []).add(row);
    }
  }
  final rows = <Record>[];
  final notes = <String>[];
  for (var index = 0; index < baseRows.length; index++) {
    final base = baseRows[index];
    final candidates = grouped.remove(index) ?? const <EmailRow>[];
    if (candidates.isEmpty) {
      rows.add(base.copy());
      continue;
    }
    for (var duplicate = 0; duplicate < candidates.length; duplicate++) {
      final row = base.copy();
      if (duplicate > 0) {
        row.rowId = '${row.sheet}-g${index + 1}-r${duplicate + 1}';
      }
      _applyEmailValues(row, candidates[duplicate], source, notes);
      rows.add(row);
    }
  }
  return (rows, notes);
}

void _applyEmailValues(
  Record row,
  EmailRow extracted,
  _Source source,
  List<String> notes,
) {
  final item = rsTrim(extracted.itemNo);
  final itp = rsTrim(extracted.itpNo);
  if (item.isNotEmpty) {
    final preset = source.data.presets[item];
    if (preset != null) {
      row.itemNo = item;
      final resolved =
          _resolveItp(itp, item, source.data.itpToItems) ??
          _uniqueItpForItem(item, source.data.itpToItems);
      if (resolved != null) {
        row.itpNo = resolved;
      }
      row.description = preset.description;
      row.forms = preset.forms;
      row.azfenType = preset.azfenType;
      row.intecType = preset.intecType;
    } else {
      notes.add('Unknown Item No. ignored: $item');
    }
  } else if (itp.isNotEmpty) {
    final resolved = _resolveItp(itp, row.itemNo, source.data.itpToItems);
    if (resolved != null) {
      row.itpNo = resolved;
      final allowed = source.data.itpToItems[resolved]!;
      if (!allowed.contains(row.itemNo) && allowed.length == 1) {
        row.itemNo = allowed.first;
        final preset = source.data.presets[row.itemNo]!;
        row.description = preset.description;
        row.forms = preset.forms;
        row.azfenType = preset.azfenType;
        row.intecType = preset.intecType;
      }
    } else {
      notes.add('Unknown ITP No. ignored: $itp');
    }
  }
  final tag = rsTrim(extracted.tagNo);
  final area = rsTrim(extracted.area);
  final location = rsTrim(extracted.location);
  final subcontractor = rsTrim(extracted.subcontractor);
  if (tag.isNotEmpty) row.tagNo = tag;
  if (area.isNotEmpty) row.area = area;
  if (location.isNotEmpty) row.location = location;
  if (subcontractor.isNotEmpty) row.subcontractor = subcontractor;
  if (row.area.isEmpty) {
    row.area = _inferArea(row.tagNo);
  }
}

Map<String, Object?> _planMeta(
  Map<String, SourceData> sources,
  List<Record> rows,
) {
  final itpToItems = SplayTreeMap<String, List<String>>(rsCompare);
  final presets = SplayTreeMap<String, Preset>(rsCompare);
  final autocomplete = <String, List<String>>{
    'tagNo': [],
    'area': [],
    'location': [],
    'subcontractor': ['AZFEN', 'PDL'],
  };
  for (final discipline in disciplines) {
    final source = sources[discipline];
    if (source == null) {
      continue;
    }
    itpToItems.addAll(source.itpToItems);
    presets.addAll(source.presets);
    final fields = <String, List<String>>{
      'tagNo': source.tags,
      'area': source.areas,
      'location': source.locations,
      'subcontractor': source.subcontractors,
    };
    for (final entry in fields.entries) {
      final target = autocomplete[entry.key]!;
      target.addAll(entry.value);
      rsDeduplicate(target);
    }
  }
  final sheets = [
    for (final discipline in disciplines)
      if (sources.containsKey(discipline)) discipline,
  ];
  final sheetCounts = <String, Object?>{
    for (final sheet in sheets)
      sheet: rows.where((row) => row.sheet == sheet).length,
  };
  return {
    'editor': <String, Object?>{
      'kind': 'recordTable',
      'layout': 'compact',
      'titleKey': 'rfi.editor.title',
      'groupField': 'rfiNo',
      'selectionGroupFields': ['_sheet', 'rfiNo'],
      'columns': _columns(),
      'rows': [for (final row in rows) row.toJson()],
      'choices': {'itpNo': itpToItems.keys.toList()},
      'choicesByValue': {
        'itemNo': {'dependsOn': 'itpNo', 'values': _itpJson(itpToItems)},
      },
      'autoFill': {'field': 'itemNo', 'values': _presetsJson(presets)},
      'autocomplete': autocomplete,
      'regexFill': [
        {
          'source': 'tagNo',
          'target': 'area',
          'pattern': r'(?:^|[^A-Z0-9])N\d{1,3}(?:[^A-Z0-9]|$)',
          'value': 'North Site',
        },
        {
          'source': 'tagNo',
          'target': 'area',
          'pattern': r'(?:^|[^A-Z0-9])S\d{1,3}(?:[^A-Z0-9]|$)',
          'value': 'South Site',
        },
      ],
      'duplicateEnabled': true,
      'selectionEnabled': true,
      'bulkEnabled': true,
      'detailsEnabled': true,
      'detailsDefaultVisible': false,
      'copyEnabled': true,
      'copyFields': copyFields,
      'copyHeaders': copyHeaders,
      'sheets': sheets,
      'sheetCounts': sheetCounts,
    },
  };
}

List<Map<String, Object?>> _columns() => const [
  {
    'key': '_sheet',
    'labelKey': 'rfi.col.sheet',
    'kind': 'readonly',
    'width': 42,
    'compact': true,
  },
  {
    'key': 'rfiNo',
    'labelKey': 'rfi.col.rfiNo',
    'kind': 'readonly',
    'width': 54,
    'compact': true,
    'display': 'suffix',
  },
  {
    'key': 'itpNo',
    'labelKey': 'rfi.col.itpNo',
    'kind': 'choice',
    'width': 50,
    'compact': true,
    'display': 'shortNumber',
  },
  {
    'key': 'itemNo',
    'labelKey': 'rfi.col.itemNo',
    'kind': 'choice',
    'width': 62,
    'compact': true,
    'display': 'itemSuffix',
  },
  {
    'key': 'description',
    'labelKey': 'rfi.col.description',
    'kind': 'multiline',
    'width': 360,
    'detail': true,
  },
  {
    'key': 'forms',
    'labelKey': 'rfi.col.forms',
    'kind': 'multiline',
    'width': 280,
    'detail': true,
  },
  {
    'key': 'tagNo',
    'labelKey': 'rfi.col.tagNo',
    'kind': 'autocomplete',
    'width': 92,
    'compact': true,
  },
  {
    'key': 'area',
    'labelKey': 'rfi.col.area',
    'kind': 'autocomplete',
    'width': 94,
    'compact': true,
  },
  {
    'key': 'rowNo',
    'labelKey': 'rfi.col.rowNo',
    'kind': 'text',
    'width': 70,
    'compact': true,
    'sheets': ['MEC'],
  },
  {
    'key': 'dateOfIssue',
    'labelKey': 'rfi.col.issueDate',
    'kind': 'compactDate',
    'width': 116,
    'compact': true,
  },
  {
    'key': 'inspectionDate',
    'labelKey': 'rfi.col.inspectionDate',
    'kind': 'compactDate',
    'width': 116,
    'compact': true,
  },
  {
    'key': 'startTime',
    'labelKey': 'rfi.col.startTime',
    'kind': 'text',
    'width': 58,
    'compact': true,
  },
  {
    'key': 'azfenType',
    'labelKey': 'rfi.col.azfenType',
    'kind': 'text',
    'width': 130,
    'detail': true,
  },
  {
    'key': 'intecType',
    'labelKey': 'rfi.col.intecType',
    'kind': 'text',
    'width': 130,
    'detail': true,
  },
  {
    'key': 'location',
    'labelKey': 'rfi.col.location',
    'kind': 'autocomplete',
    'width': 104,
    'compact': true,
  },
  {
    'key': 'subcontractor',
    'labelKey': 'rfi.col.subcontractor',
    'kind': 'autocomplete',
    'width': 88,
    'compact': true,
  },
];

CopySource _copySource(String path, Batch batch) {
  final layoutOf = layout(batch.discipline);
  if (layoutOf == null) {
    throw BackendError.validation('gecersiz sheet plani');
  }
  final fields = [...copyFields];
  final headers = [...copyHeaders];
  final columns = [1, 2, 3, 4, 5, 6, 7];
  if (layoutOf.rowCol != null) {
    fields.insert(7, 'rowNo');
    headers.insert(7, 'ROW No.');
    columns.add(layoutOf.rowCol!);
  }
  columns.addAll([
    layoutOf.issueCol,
    layoutOf.inspectionCol,
    layoutOf.timeCol,
    layoutOf.azfenCol,
    layoutOf.intecCol,
    layoutOf.locationCol,
    layoutOf.subcontractorCol,
  ]);
  return CopySource(
    workbook: path,
    sheet: layoutOf.sheet,
    headerRow: 9,
    startRow: batch.startRow,
    rfiNumbers: [for (final record in batch.records) record.rfiNo],
    fields: fields,
    headers: headers,
    columns: columns,
  );
}

final class _CopyData {
  const _CopyData({
    required this.fields,
    required this.headers,
    required this.rows,
    required this.fromWorkbook,
    required this.widths,
  });

  final List<String> fields;
  final List<String> headers;
  final List<Map<String, String>> rows;
  final bool fromWorkbook;
  final List<double> widths;
}

_CopyData _copyData(Plan plan, String? selectedSheet) {
  final editor = plan.meta['editor'];
  if (editor is! Map ||
      editor['kind'] != 'recordTable' ||
      editor['copyEnabled'] != true) {
    throw BackendError.validation('bu planda kopyalanabilir tablo yok');
  }
  final sourceValue = _selectCopySource(
    Map<String, Object?>.from(editor),
    selectedSheet,
  );
  final CopySource source;
  try {
    source = CopySource.fromJson(sourceValue);
  } on Object {
    throw BackendError.validation('mail tablosu kaynagi gecersiz');
  }
  if (source.fields.length != source.headers.length ||
      source.fields.length != source.columns.length) {
    throw BackendError.validation('mail tablosu kolonlari gecersiz');
  }
  final plannedRows = <Map<String, String>>[];
  final editorRows = editor['rows'];
  if (editorRows is List) {
    for (final row in editorRows) {
      if (row is! Map || row['_sheet'] != source.sheet) {
        continue;
      }
      final Record record;
      try {
        record = Record.fromJson(row);
      } on Object {
        continue;
      }
      plannedRows.add(_recordMap(record));
    }
  }
  final fallbackWidths = [
    for (final field in source.fields) _fieldWidth(field),
  ];
  final _Workbook? workbook = _tryReadWorkbook(source.workbook);
  final sheet = workbook?.sheet(source.sheet);
  if (sheet == null) {
    return _CopyData(
      fields: source.fields,
      headers: source.headers,
      rows: plannedRows,
      fromWorkbook: false,
      widths: fallbackWidths,
    );
  }
  final workbookHeaders = [
    for (final column in source.columns) sheet.value(source.headerRow, column),
  ];
  final headers = workbookHeaders.every((header) => header.isNotEmpty)
      ? workbookHeaders
      : source.headers;
  final remaining = <String, int>{};
  for (final rfi in source.rfiNumbers) {
    remaining[rfi] = (remaining[rfi] ?? 0) + 1;
  }
  final plannedByRfi = <String, Queue<Map<String, String>>>{};
  for (final row in plannedRows) {
    (plannedByRfi[row['rfiNo'] ?? ''] ??= Queue()).add(row);
  }
  final rows = <Map<String, String>>[];
  for (
    var rowNumber = source.headerRow + 1;
    rowNumber <= sheet.maxRow;
    rowNumber++
  ) {
    final rfi = sheet.value(rowNumber, source.columns[0]);
    final count = remaining[rfi];
    if (count == null || count == 0) {
      continue;
    }
    final queue = plannedByRfi[rfi];
    final plannedRow = (queue != null && queue.isNotEmpty)
        ? queue.removeFirst()
        : <String, String>{};
    final actual = <String, String>{};
    for (var index = 0; index < source.fields.length; index++) {
      final field = source.fields[index];
      final value = _copyCellText(
        sheet,
        rowNumber,
        source.columns[index],
        field,
      );
      actual[field] = value.isEmpty ? (plannedRow[field] ?? '') : value;
    }
    rows.add(actual);
    remaining[rfi] = count - 1;
  }
  if (remaining.values.any((count) => count > 0) ||
      rows.length != source.rfiNumbers.length) {
    return _CopyData(
      fields: source.fields,
      headers: source.headers,
      rows: plannedRows,
      fromWorkbook: false,
      widths: fallbackWidths,
    );
  }
  final widths = [
    for (var index = 0; index < source.columns.length; index++)
      sheet.columnWidths[source.columns[index]] ?? fallbackWidths[index],
  ];
  return _CopyData(
    fields: source.fields,
    headers: headers,
    rows: rows,
    fromWorkbook: true,
    widths: widths,
  );
}

Object? _selectCopySource(Map<String, Object?> editor, String? selected) {
  final sources = editor['copySources'];
  if (sources is Map) {
    var chosen = rsAsciiUpper(selected ?? '');
    if (chosen.isEmpty) {
      if (sources.length != 1) {
        throw BackendError.validation('kopyalanacak sheet secilmelidir');
      }
      chosen = sources.keys.first.toString();
    }
    for (final entry in sources.entries) {
      if (rsEqualsIgnoreAsciiCase(entry.key.toString(), chosen)) {
        return entry.value;
      }
    }
    throw BackendError.validation('bu planda $chosen kopyalama kaynagi yok');
  }
  final single = editor['copySource'];
  if (single == null) {
    throw BackendError.validation('mail tablosu kaynagi gecersiz');
  }
  return single;
}

void _validateRecord(Record record, int index, Batch batch) {
  if (record.sheet != batch.discipline) {
    throw BackendError.validation(
      "RFI satiri yanlis sheet'e ait: ${batch.discipline}",
    );
  }
  if (!batch.baseRfis.contains(record.rfiNo)) {
    throw BackendError.validation('plan disi RFI numarasi: ${record.rfiNo}');
  }
  _parseDate(record.dateOfIssue);
  _parseDate(record.inspectionDate);
  _parseTime(record.startTime);
  if (!batch.sourceData.presets.containsKey(record.itemNo)) {
    throw BackendError.validation('bilinmeyen Item No.: ${record.itemNo}');
  }
  final resolved = _resolveItp(
    record.itpNo,
    record.itemNo,
    batch.sourceData.itpToItems,
  );
  if (resolved == null) {
    throw BackendError.validation('bilinmeyen ITP No.: ${record.itpNo}');
  }
  record.itpNo = resolved;
  if (!batch.sourceData.itpToItems[resolved]!.contains(record.itemNo)) {
    throw BackendError.validation(
      "Item No. secilen ITP'ye ait degil: $resolved / ${record.itemNo}",
    );
  }
  final required = [
    ('description', record.description),
    ('forms', record.forms),
    ('azfenType', record.azfenType),
    ('intecType', record.intecType),
    ('subcontractor', record.subcontractor),
  ];
  for (final (name, value) in required) {
    if (value.isEmpty) {
      throw BackendError.validation(
        'zorunlu alan bos: $name (satir ${index + 1})',
      );
    }
  }
}

void _cleanRecord(Record record) {
  void clean(String value, void Function(String) assign) {
    assign(rsTruncateChars(_cleanText(value), _maxRowTextChars));
  }

  clean(record.rowId, (v) => record.rowId = v);
  clean(record.sheet, (v) => record.sheet = v);
  clean(record.rfiNo, (v) => record.rfiNo = v);
  clean(record.itpNo, (v) => record.itpNo = v);
  clean(record.itemNo, (v) => record.itemNo = v);
  clean(record.description, (v) => record.description = v);
  clean(record.forms, (v) => record.forms = v);
  clean(record.tagNo, (v) => record.tagNo = v);
  clean(record.area, (v) => record.area = v);
  clean(record.rowNo, (v) => record.rowNo = v);
  clean(record.dateOfIssue, (v) => record.dateOfIssue = v);
  clean(record.inspectionDate, (v) => record.inspectionDate = v);
  clean(record.startTime, (v) => record.startTime = v);
  clean(record.azfenType, (v) => record.azfenType = v);
  clean(record.intecType, (v) => record.intecType = v);
  clean(record.location, (v) => record.location = v);
  clean(record.subcontractor, (v) => record.subcontractor = v);
  if (record.rowId.isEmpty) {
    record.rowId = 'row-${_randomHex(10)}';
  }
  record.sheet = rsAsciiUpper(record.sheet);
  final layoutOf = layout(record.sheet);
  if (layoutOf != null && layoutOf.rowCol == null) {
    record.rowNo = '';
  }
  if (record.area.isEmpty) {
    record.area = _inferArea(record.tagNo);
  }
}

List<Warning> _warningsForRows(List<Record> rows, int overflowed, bool wrap) {
  final warnings = <Warning>[];
  if (overflowed > 0) {
    warnings.add(
      Warning(
        key: wrap ? 'warn.rfi.overflowMoved' : 'warn.rfi.overflowKept',
        subject: '$overflowed',
      ),
    );
  }
  final grouped = SplayTreeMap<String, Set<(String, String)>>(rsCompare);
  for (final row in rows) {
    (grouped[row.rfiNo] ??= <(String, String)>{}).add((
      row.inspectionDate,
      row.startTime,
    ));
  }
  for (final entry in grouped.entries) {
    if (entry.value.length > 1) {
      warnings.add(
        Warning(key: 'warn.rfi.groupScheduleMismatch', subject: entry.key),
      );
    }
  }
  return warnings;
}

Future<EmailExtraction> _extractWithGemini(
  Map<String, Object?> values,
  String email,
  String discipline,
  SourceData source,
  GeminiClient? gemini,
) async {
  var key = _valueString(values, 'gemini_api_key');
  if (rsTrim(key).isEmpty) {
    key = Platform.environment['GEMINI_API_KEY'] ?? '';
  }
  if (gemini == null) {
    throw BackendError.requestFailed('Gemini is unavailable');
  }
  final value = await gemini.extract(
    GeminiRequest(email, key, discipline, source.itpToItems),
  );
  return validateGeminiExtraction(value);
}

String _safeError(BackendError error) {
  final safe = error.code == 'validation'
      ? 'ValueError: ${error.message}'
      : 'RuntimeError: ${error.message}';
  return rsTruncateChars(safe.replaceAll(RegExp(r'[\r\n]'), ' '), 300);
}

Map<String, Object?> _signature(String path) {
  final int size;
  final String sha256;
  try {
    size = File(path).lengthSync();
    sha256 = fileSha256(path);
  } on Object {
    throw BackendError.pathInvalid('workbook hash is unavailable');
  }
  return {'size': size, 'sha256': sha256};
}

Step _onlyWorkbookStep(Plan plan) {
  if (plan.opId != _operationId ||
      plan.steps.length != 1 ||
      plan.steps.first.kind != StepKind.workbookUpdate) {
    throw BackendError.validation('gecersiz RFI Log plani');
  }
  return plan.steps.first;
}

List<Batch> _decodeBatches(Step step) {
  final raw = step.meta['batches'];
  if (raw == null) {
    throw BackendError.validation('gecersiz coklu-sheet RFI plani');
  }
  try {
    if (raw is! List) {
      throw const FormatException('batches');
    }
    return [for (final item in raw) Batch.fromJson(item)];
  } on Object {
    throw BackendError.validation('gecersiz coklu-sheet RFI plani');
  }
}

/// Formula-or-text rule: keep the formula when the text equals the preset.
CellUpdate _formulaOrValue(String value, String defaultValue, String formula) {
  if (_normalizeLines(value) == _normalizeLines(defaultValue)) {
    return CellUpdateFormula(formula: formula, cached: defaultValue);
  }
  return CellUpdateText(value);
}

String _normalizeLines(String value) =>
    unorm.nfc(value.replaceAll('\r\n', '\n').replaceAll('\r', '\n'));

String? _resolveItp(
  String value,
  String item,
  Map<String, List<String>> mapping,
) {
  if (mapping.containsKey(value)) {
    return value;
  }
  final signature = _itpSignature(value);
  final matches = [
    for (final label in mapping.keys)
      if (signature.isNotEmpty && _listEq(_itpSignature(label), signature))
        label,
  ]..sort(rsCompare);
  if (matches.length == 1) {
    return matches.first;
  }
  return _uniqueItpForItem(item, mapping);
}

String? _uniqueItpForItem(String item, Map<String, List<String>> mapping) {
  final matches = [
    for (final entry in mapping.entries)
      if (entry.value.contains(item)) entry.key,
  ];
  return matches.length == 1 ? matches.first : null;
}

List<String> _itpSignature(String value) {
  final upper = unorm.nfkc(value).toUpperCase();
  return [for (final match in _tokenPattern.allMatches(upper)) match.group(0)!];
}

String _inferArea(String tag) {
  final upper = rsAsciiUpper(tag);
  final leading = _leadingSitePattern.firstMatch(upper);
  if (leading != null) {
    return leading.group(1) == 'N' ? 'North Site' : 'South Site';
  }
  final north = _northPattern.hasMatch(upper);
  final south = _southPattern.hasMatch(upper);
  if (north && !south) {
    return 'North Site';
  }
  if (!north && south) {
    return 'South Site';
  }
  return '';
}

/// Port of `clean_text`: trim, NFC, and up to two cp1252 mojibake repairs.
String _cleanText(String value) {
  var text = unorm.nfc(rsTrim(value));
  for (var attempt = 0; attempt < 2; attempt++) {
    if (_mojibakeScore(text) == 0) {
      break;
    }
    final bytes = <int>[];
    var convertible = true;
    for (final rune in text.runes) {
      final byte = _cp1252Byte(rune);
      if (byte == null) {
        convertible = false;
        break;
      }
      bytes.add(byte);
    }
    if (!convertible) {
      break;
    }
    final String repaired;
    try {
      repaired = utf8.decode(bytes);
    } on FormatException {
      break;
    }
    if (_mojibakeScore(repaired) >= _mojibakeScore(text)) {
      break;
    }
    text = unorm.nfc(repaired);
  }
  return text;
}

int? _cp1252Byte(int rune) {
  if (rune <= 0x7f || (rune >= 0xa0 && rune <= 0xff)) {
    return rune;
  }
  return _cp1252Specials[rune];
}

const Map<int, int> _cp1252Specials = {
  0x20ac: 0x80,
  0x201a: 0x82,
  0x0192: 0x83,
  0x201e: 0x84,
  0x2026: 0x85,
  0x2020: 0x86,
  0x2021: 0x87,
  0x02c6: 0x88,
  0x2030: 0x89,
  0x0160: 0x8a,
  0x2039: 0x8b,
  0x0152: 0x8c,
  0x017d: 0x8e,
  0x2018: 0x91,
  0x2019: 0x92,
  0x201c: 0x93,
  0x201d: 0x94,
  0x2022: 0x95,
  0x2013: 0x96,
  0x2014: 0x97,
  0x02dc: 0x98,
  0x2122: 0x99,
  0x0161: 0x9a,
  0x203a: 0x9b,
  0x0153: 0x9c,
  0x017e: 0x9e,
  0x0178: 0x9f,
};

int _mojibakeScore(String value) {
  var score = 0;
  for (final marker in const ['Ã', 'Â', 'â', 'ð', 'Ë']) {
    score += marker.allMatches(value).length;
  }
  return score;
}

String _headerKey(String value) {
  final buffer = StringBuffer();
  for (final character
      in unorm.nfkd(value).toLowerCase().runes.map(String.fromCharCode)) {
    if (_alphanumeric.hasMatch(character)) {
      buffer.write(character);
    }
  }
  return buffer.toString();
}

(String, int)? _itemParts(String value) {
  final match = _itemPattern.firstMatch(value);
  if (match == null) {
    return null;
  }
  final number = _u32Parse(match.group(2)!);
  if (number == null) {
    return null;
  }
  return (match.group(1)!.toUpperCase(), number);
}

(String, int)? _rfiParts(String value) {
  final match = _rfiPattern.firstMatch(value);
  if (match == null) {
    return null;
  }
  final normalized = normalizeCandidateNumber(match.group(2)!);
  if (normalized == null) {
    return null;
  }
  final number = _u32Parse(normalized);
  if (number == null) {
    return null;
  }
  return (match.group(1)!.toUpperCase(), number);
}

int? _trailingFourDigits(String value) {
  final match = _trailingDigitsPattern.firstMatch(value);
  return match == null ? null : int.tryParse(match.group(1)!);
}

int? _u32Parse(String value) {
  final number = BigInt.tryParse(value);
  if (number == null ||
      number < BigInt.zero ||
      number > BigInt.from(0xFFFFFFFF)) {
    return null;
  }
  return number.toInt();
}

String _canonicalItp(String discipline, int number) =>
    'AZE-SUN-INT-QMS-ITP-$discipline-${number.toString().padLeft(4, '0')}';

/// Port of `natural_key`: digit runs as zero-padded u64, other runs prefixed
/// with "1" so they sort after numbers.
List<String> _naturalKey(String value) {
  return [
    for (final match in _naturalPattern.allMatches(value))
      () {
        final part = match.group(0)!;
        final number = _isAsciiDigits(part) ? BigInt.tryParse(part) : null;
        if (number == null || number > _maxU64) {
          return '1$part';
        }
        return '0${number.toString().padLeft(20, '0')}';
      }(),
  ];
}

final BigInt _maxU64 = (BigInt.one << 64) - BigInt.one;

bool _isAsciiDigits(String value) =>
    value.isNotEmpty &&
    value.codeUnits.every((unit) => unit >= 0x30 && unit <= 0x39);

List<String> _recentUnique(List<String> values) {
  final seen = <String>{};
  final out = <String>[];
  for (var index = values.length - 1; index >= 0 && out.length < 500; index--) {
    final value = values[index];
    if (value.isNotEmpty && seen.add(value)) {
      out.add(value);
    }
  }
  return out;
}

DateTime? _cellDate(String value) {
  final serial = double.tryParse(value);
  if (serial != null) {
    final raw = serial.isNaN ? 0.0 : serial.floorToDouble();
    final days = raw < 0 ? 0 : raw;
    if (days > 3000000) {
      return null;
    }
    return DateTime.utc(1899, 12, 30).add(Duration(days: days.toInt()));
  }
  try {
    return _parseDate(value);
  } on BackendError {
    return null;
  }
}

/// Minute-of-day for a cell time value, or null when not parseable.
int? _cellTime(String value) {
  final serial = double.tryParse(value);
  if (serial != null) {
    final raw = serial * 24 * 60 * 60;
    final seconds = raw.isNaN ? 0 : raw.clamp(0.0, 4294967295.0).round();
    return (seconds % (24 * 60 * 60)) ~/ 60;
  }
  try {
    return _parseTime(value);
  } on BackendError {
    return null;
  }
}

final RegExp _dayMonthYear = RegExp(r'^([0-9]{1,2})-([A-Za-z]{3})-([0-9]{2})$');
final RegExp _isoDate = RegExp(r'^([0-9]{4})-([0-9]{1,2})-([0-9]{1,2})$');
final RegExp _dottedDate = RegExp(r'^([0-9]{1,2})\.([0-9]{1,2})\.([0-9]{4})$');
final RegExp _clockTime = RegExp(r'^([0-9]{1,2}):([0-9]{1,2})$');

DateTime _parseDate(String value) {
  final text = rsTrim(value);
  DateTime? date;
  var match = _dayMonthYear.firstMatch(text);
  if (match != null) {
    final month = _monthNames.indexWhere(
      (name) => name.toLowerCase() == match!.group(2)!.toLowerCase(),
    );
    final year = int.parse(match.group(3)!);
    // UNVERIFIED: chrono %y pivot assumed to be 00-69 -> 20xx, 70-99 -> 19xx.
    date = month < 0
        ? null
        : _validDate(
            year < 70 ? 2000 + year : 1900 + year,
            month + 1,
            int.parse(match.group(1)!),
          );
  } else if ((match = _isoDate.firstMatch(text)) != null) {
    date = _validDate(
      int.parse(match!.group(1)!),
      int.parse(match.group(2)!),
      int.parse(match.group(3)!),
    );
  } else if ((match = _dottedDate.firstMatch(text)) != null) {
    date = _validDate(
      int.parse(match!.group(3)!),
      int.parse(match.group(2)!),
      int.parse(match.group(1)!),
    );
  }
  if (date == null) {
    throw BackendError.validation('gecersiz tarih: $value');
  }
  return date;
}

DateTime? _validDate(int year, int month, int day) {
  if (month < 1 || month > 12 || day < 1) {
    return null;
  }
  final date = DateTime.utc(year, month, day);
  if (date.year != year || date.month != month || date.day != day) {
    return null;
  }
  return date;
}

/// Parses `%H:%M` and returns the minute of day.
int _parseTime(String value) {
  final match = _clockTime.firstMatch(rsTrim(value));
  if (match != null) {
    final hour = int.parse(match.group(1)!);
    final minute = int.parse(match.group(2)!);
    if (hour < 24 && minute < 60) {
      return hour * 60 + minute;
    }
  }
  throw BackendError.validation('gecersiz saat: $value');
}

String _dateText(DateTime value) {
  final yy = (value.year % 100).toString().padLeft(2, '0');
  return '${value.day.toString().padLeft(2, '0')}-${_monthNames[value.month - 1]}-$yy';
}

String _timeText(int minutes) =>
    '${(minutes ~/ 60).toString().padLeft(2, '0')}:${(minutes % 60).toString().padLeft(2, '0')}';

int _excelDate(DateTime value) =>
    value.difference(DateTime.utc(1899, 12, 30)).inDays;

String _excelTime(int minutes) {
  final fraction = (minutes * 60) / (24 * 60 * 60);
  var text = fraction.toStringAsFixed(15);
  text = text.replaceFirst(RegExp(r'0+$'), '');
  return text.replaceFirst(RegExp(r'\.$'), '');
}

Map<String, String> _recordMap(Record record) => {
  for (final entry in record.toJson().entries)
    entry.key: entry.value is String ? entry.value! as String : '',
};

String _copyCellText(WorksheetData sheet, int row, int column, String field) {
  final raw = sheet.value(row, column);
  switch (field) {
    case 'dateOfIssue':
    case 'inspectionDate':
      final day = _cellDate(raw);
      return day == null ? raw : _dateText(day);
    case 'startTime':
      final time = _cellTime(raw);
      return time == null ? raw : _timeText(time);
    default:
      return raw;
  }
}

double _fieldWidth(String field) {
  switch (field) {
    case 'rfiNo':
      return 22;
    case 'itpNo':
      return 24;
    case 'itemNo':
    case 'rowNo':
    case 'azfenType':
    case 'intecType':
      return 10;
    case 'description':
      return 28;
    case 'forms':
      return 20;
    case 'tagNo':
      return 15;
    case 'area':
    case 'dateOfIssue':
    case 'inspectionDate':
    case 'startTime':
      return 12;
    case 'location':
      return 18;
    case 'subcontractor':
      return 14;
    default:
      return 14;
  }
}

String _htmlEscape(String value) => value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#x27;');

String _columnName(int number) {
  var remaining = number;
  var output = '';
  while (remaining > 0) {
    final remainder = (remaining - 1) % 26;
    output = String.fromCharCode(0x41 + remainder) + output;
    remaining = (remaining - 1) ~/ 26;
  }
  return output;
}

String _valueString(Map<String, Object?> values, String key) {
  final value = values[key];
  return value is String ? value : '';
}

bool _isDiscipline(String value) => disciplines.contains(value);

String _localStamp(DateTime value) {
  String two(int number) => number.toString().padLeft(2, '0');
  return '${value.year.toString().padLeft(4, '0')}${two(value.month)}${two(value.day)}_'
      '${two(value.hour)}${two(value.minute)}${two(value.second)}';
}

String _randomHex(int length) {
  final random = Random.secure();
  final buffer = StringBuffer();
  for (var index = 0; index < length; index++) {
    buffer.write(random.nextInt(16).toRadixString(16));
  }
  return buffer.toString();
}

Map<String, Object?> _asObject(Object? value) {
  if (value is Map) {
    return Map<String, Object?>.from(value);
  }
  throw const FormatException('object expected');
}

String _reqString(Map<String, Object?> map, String key) {
  final value = map[key];
  if (value is String) {
    return value;
  }
  throw FormatException('$key must be a string');
}

String _optString(Map<String, Object?> map, String key) {
  if (!map.containsKey(key)) {
    return '';
  }
  return _reqString(map, key);
}

int _reqU32(Map<String, Object?> map, String key) {
  final value = map[key];
  if (value is int && value >= 0 && value <= 0xFFFFFFFF) {
    return value;
  }
  throw FormatException('$key must be a u32');
}

List<String> _reqStringList(Map<String, Object?> map, String key) {
  final value = map[key];
  if (value is! List) {
    throw FormatException('$key must be a list');
  }
  return [
    for (final item in value)
      if (item is String) item else throw FormatException('$key item'),
  ];
}

List<int> _reqU32List(Map<String, Object?> map, String key) {
  final value = map[key];
  if (value is! List) {
    throw FormatException('$key must be a list');
  }
  return [
    for (final item in value)
      if (item is int && item >= 0 && item <= 0xFFFFFFFF)
        item
      else
        throw FormatException('$key item'),
  ];
}

List<T> _reqList<T>(
  Map<String, Object?> map,
  String key,
  T Function(Object?) decode,
) {
  final value = map[key];
  if (value is! List) {
    throw FormatException('$key must be a list');
  }
  return [for (final item in value) decode(item)];
}

SplayTreeMap<String, Preset> _presetMap(Map<String, Object?> map, String key) {
  final value = map[key];
  if (value is! Map) {
    throw FormatException('$key must be an object');
  }
  final result = SplayTreeMap<String, Preset>(rsCompare);
  for (final entry in value.entries) {
    result[entry.key.toString()] = Preset.fromJson(entry.value);
  }
  return result;
}

SplayTreeMap<String, List<String>> _itpMap(
  Map<String, Object?> map,
  String key,
) {
  final value = map[key];
  if (value is! Map) {
    throw FormatException('$key must be an object');
  }
  final result = SplayTreeMap<String, List<String>>(rsCompare);
  for (final entry in value.entries) {
    final items = entry.value;
    if (items is! List || items.any((item) => item is! String)) {
      throw FormatException('$key item');
    }
    result[entry.key.toString()] = [for (final item in items) item! as String];
  }
  return result;
}

SplayTreeMap<String, int> _columnMap(Map<String, Object?> map, String key) {
  final value = map[key];
  if (value is! Map) {
    throw FormatException('$key must be an object');
  }
  final result = SplayTreeMap<String, int>(rsCompare);
  for (final entry in value.entries) {
    final column = entry.value;
    if (column is! int || column < 0 || column > 0xFFFFFFFF) {
      throw FormatException('$key column');
    }
    result[entry.key.toString()] = column;
  }
  return result;
}

Map<String, Object?> _presetsJson(Map<String, Preset> presets) => {
  for (final entry in presets.entries) entry.key: entry.value.toJson(),
};

Map<String, Object?> _itpJson(Map<String, List<String>> itpToItems) => {
  for (final entry in itpToItems.entries) entry.key: entry.value,
};

Map<WorkbookCell, CellUpdate> _sortedUpdates(
  Map<WorkbookCell, CellUpdate> updates,
) {
  final keys = updates.keys.toList()
    ..sort(
      (left, right) => left.$1 != right.$1
          ? left.$1.compareTo(right.$1)
          : left.$2.compareTo(right.$2),
    );
  return {for (final key in keys) key: updates[key]!};
}

bool _setEquals(Set<String> left, Set<String> right) =>
    left.length == right.length && left.containsAll(right);

bool _listEq(List<String> left, List<String> right) {
  if (left.length != right.length) {
    return false;
  }
  for (var index = 0; index < left.length; index++) {
    if (left[index] != right[index]) {
      return false;
    }
  }
  return true;
}

/// A workbook read through [readWorkbookSelective]; kept as a typedef so the
/// copy-data path can fall back without throwing.
typedef _Workbook = WorkbookData;

_Workbook _readWorkbook(String path) {
  try {
    final bytes = Uint8List.fromList(File(path).readAsBytesSync());
    return readWorkbookSelective(bytes, _requiredSheets);
  } on OoxmlError catch (error) {
    throw BackendError.validation(error.message);
  } on FileSystemException catch (error) {
    throw BackendError.validation(error.message);
  }
}

_Workbook? _tryReadWorkbook(String path) {
  try {
    return _readWorkbook(path);
  } on BackendError {
    return null;
  }
}

final RegExp _rfiPattern = RegExp(
  r'-RFI-(CIV|MEC|ELE|PRC)-([0-9]{1,4})$',
  caseSensitive: false,
);
final RegExp _itemPattern = RegExp(
  r'^(CIV|MEC|ELE|PRC)-([0-9]+)\.',
  caseSensitive: false,
);
final RegExp _trailingDigitsPattern = RegExp(r'([0-9]{4})$');
final RegExp _tokenPattern = RegExp(r'[A-Z]+|\p{Nd}+', unicode: true);
final RegExp _naturalPattern = RegExp(r'\p{Nd}+|\P{Nd}+', unicode: true);
final RegExp _alphanumeric = RegExp(
  r'^[\p{Alphabetic}\p{Nd}\p{Nl}\p{No}]$',
  unicode: true,
);
final RegExp _leadingSitePattern = RegExp(
  r'^\p{White_Space}*([NS])\p{Nd}{1,3}(?:[^A-Z0-9]|$)',
  unicode: true,
  caseSensitive: false,
);
final RegExp _northPattern = RegExp(
  r'(?:^|[^A-Z0-9])N\p{Nd}{1,3}(?:[^A-Z0-9]|$)',
  unicode: true,
  caseSensitive: false,
);
final RegExp _southPattern = RegExp(
  r'(?:^|[^A-Z0-9])S\p{Nd}{1,3}(?:[^A-Z0-9]|$)',
  unicode: true,
  caseSensitive: false,
);

/// Test hook for the Rust in-module identity test (`rfi_parts`).
(String, int)? rfiPartsForTesting(String value) => _rfiParts(value);
