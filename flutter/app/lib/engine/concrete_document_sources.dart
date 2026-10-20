/// Port of src-tauri/src/concrete_document_sources.rs.
///
/// Reads the RFI Log (Civil sheet) and the Concrete Specimen Test Follow-Up
/// workbook, and resolves CIV RFI numbers. Workbooks are passed as bytes.
library;

import 'dart:collection';
import 'dart:math' as math;
import 'dart:typed_data';

import 'ooxml.dart';
import 'rfi_code.dart' as rfi_code;

/// Error from concrete-document source reading or generation. Mirrors the
/// `Err(String)` results of the Rust module.
class ConcreteDocumentError implements Exception {
  const ConcreteDocumentError(this.message);
  final String message;

  @override
  String toString() => message;
}

final RegExp _civSerial = RegExp(
  r'CIV[^0-9]{0,4}([0-9]{1,4})',
  caseSensitive: false,
);
final RegExp _conCiv010Form = RegExp(
  r'(?:^|[^A-Z0-9])CON[\s_-]*CIV[\s_-]*010(?:$|[^0-9])',
  caseSensitive: false,
);

class CivilRecord {
  const CivilRecord({this.tag = '', this.area = '', this.forms = ''});

  final String tag;
  final String area;
  final String forms;
}

/// Port of requested_rfis: parses a free-text list of CIV RFI numbers into
/// canonical codes, de-duplicated in first-seen order.
List<String> requestedRfis(String raw) {
  final values = <String>[];
  for (final part in raw.split(RegExp(r'[,;\n\r\t]'))) {
    final item = part.trim();
    if (item.isEmpty) continue;
    final digits = item.startsWith('CIV-')
        ? item.substring(4)
        : item.startsWith('CIV')
        ? item.substring(3)
        : item;
    if (digits.codeUnits.every(_isAsciiDigit) &&
        digits.isNotEmpty &&
        digits.length <= 4) {
      values.add('AZE-SUN-INT-QMS-RFI-CIV-${digits.padLeft(4, '0')}');
      continue;
    }
    final matched = rfi_code.find(item);
    if (matched.ok && matched.discipline == 'CIV') {
      values.add(matched.code);
    } else {
      throw ConcreteDocumentError('Invalid CIV RFI number: $item');
    }
  }
  final seen = <String>{};
  values.retainWhere(seen.add);
  if (values.isEmpty) {
    throw const ConcreteDocumentError(
      'Enter at least one RFI number for a document type',
    );
  }
  return values;
}

/// Port of read_civil_records.
Map<String, CivilRecord> readCivilRecords(
  Uint8List workbookBytes,
  List<String> requested,
) {
  final WorkbookData workbook;
  try {
    workbook = readWorkbook(workbookBytes);
  } on OoxmlError catch (error) {
    throw ConcreteDocumentError('Cannot read RFI Log workbook: $error');
  }
  final sheet = _findSheet(workbook, (name) => _normalize(name) == 'civ');
  if (sheet == null) {
    throw const ConcreteDocumentError(
      'RFI Log does not contain a Civil (CIV) sheet',
    );
  }
  final header = _findHeader(sheet, const ['rfino'], const ['area'], const [
    ('tag', ['tagno', 'tag']),
    ('area', ['area']),
    ('location', ['inspectionlocation', 'location']),
    ('forms', ['forms', 'form']),
  ]);
  final columns = header.columns;
  final rfiColumn = columns['rfino']!;
  final records = <String, CivilRecord>{};
  for (var row = header.row + 1; row <= sheet.maxRow; row++) {
    if (sheet.cell(row, rfiColumn)?.formula != null) {
      throw ConcreteDocumentError(
        'RFI Log identifier at row $row is a formula',
      );
    }
    final rawRfi = _cellValue(sheet, row, rfiColumn);
    if (rawRfi.trim().isEmpty) continue;
    final rfi = _canonicalLogRfi(rawRfi);
    if (rfi == null) continue;
    if (!requested.contains(rfi)) continue;
    final tag = _valueFor(sheet, row, columns, 'tag');
    var area = _valueFor(sheet, row, columns, 'area');
    if (area.isEmpty) {
      area = _valueFor(sheet, row, columns, 'location');
    }
    final record = CivilRecord(
      tag: tag,
      forms: _valueFor(sheet, row, columns, 'forms'),
      area: area,
    );
    if (records.containsKey(rfi)) {
      throw ConcreteDocumentError(
        'RFI Log has more than one Civil row for $rfi',
      );
    }
    records[rfi] = record;
  }
  for (final rfi in requested) {
    if (!records.containsKey(rfi)) {
      throw ConcreteDocumentError(
        'RFI Log Civil sheet has no exact row for $rfi',
      );
    }
  }
  return records;
}

/// Port of has_con_civ_010_form.
bool hasConCiv010Form(String forms) => _conCiv010Form.hasMatch(forms);

/// Port of read_follow_up_records. Each RFI maps to its ordered batch rows;
/// continuation rows attach to the RFIs of the nearest preceding RFI row.
Map<String, List<Map<String, String>>> readFollowUpRecords(
  Uint8List workbookBytes,
  List<String> requested,
) {
  final WorkbookData workbook;
  try {
    workbook = readWorkbook(workbookBytes);
  } on OoxmlError catch (error) {
    throw ConcreteDocumentError(
      'Cannot read Concrete Specimen Test Follow-Up workbook: $error',
    );
  }
  final sheet = _findSheet(
    workbook,
    (name) => _normalize(name).contains('followup'),
  );
  if (sheet == null) {
    throw const ConcreteDocumentError(
      'Follow Up workbook does not contain a Follow Up sheet',
    );
  }
  final header = _findHeader(
    sheet,
    const ['rfino'],
    const ['concreteclass'],
    const [
      ('rfi', ['rfino', 'rfi']),
      ('drawing', ['drawingno', 'drawingnumber']),
      ('pour_date', ['pouringdate', 'pourdate']),
      ('concrete_class', ['concreteclass', 'class']),
      ('location', ['unitlocation', 'location']),
      ('structure', ['structure', 'structue']),
      ('supplier', ['supplier']),
      ('batch_ticket', ['batchticketno', 'batchticketnumber']),
      ('truck', ['truckno', 'trucknumber']),
      ('batch_time', ['timeofloading', 'loadingtime', 'batchtime']),
      ('start_time', ['starttime', 'pouringstarttime']),
      ('end_time', ['completiontime', 'endtime', 'pouringendtime']),
      ('quantity', ['quantity', 'quantitym', 'quantitypoured']),
      ('total_quantity', ['totalquantity', 'totalquantitypoured']),
      (
        'plant_slump',
        [
          'slumpingplantmm',
          'slumpinplantmm',
          'slumpplant',
          'slumptestplant',
          'inplantslump',
        ],
      ),
      (
        'in_situ_slump',
        [
          'slumpinsitumm',
          'slumpinginsitumm',
          'insituslump',
          'slumpinsitu',
          'slumptestinsitu',
        ],
      ),
      (
        'plant_temp',
        [
          'tempratureinplantc',
          'temperatureinplantc',
          'planttemperature',
          'planttemp',
          'inplanttemperature',
        ],
      ),
      (
        'in_situ_temp',
        [
          'tempratureinsituc',
          'temperatureinsituc',
          'insitutemperature',
          'insitutemp',
        ],
      ),
      (
        'samples',
        [
          'specimenno',
          'specimennumber',
          'concretesamplesno',
          'samplesno',
          'samplenumbers',
        ],
      ),
      ('water_cement_ratio', ['watercementratio', 'wcratio']),
    ],
  );
  final columns = header.columns;
  final rfiColumn = columns['rfi']!;
  final records = <String, List<Map<String, String>>>{};
  var precedingRfis = <String>[];
  for (var row = header.row + 1; row <= sheet.maxRow; row++) {
    if (sheet.cell(row, rfiColumn)?.formula != null) {
      throw ConcreteDocumentError(
        'Follow Up identifier at row $row is a formula',
      );
    }
    final rawRfis = _valueFor(sheet, row, columns, 'rfi');
    final fields = SplayTreeMap<String, String>();
    for (final name in columns.keys) {
      fields[name] = _valueFor(sheet, row, columns, name);
    }
    if (rawRfis.trim().isEmpty) {
      final hasFollowUpData = fields.entries.any(
        (entry) => entry.key != 'rfi' && entry.value.trim().isNotEmpty,
      );
      if (hasFollowUpData) {
        for (final rfi in precedingRfis) {
          records.putIfAbsent(rfi, () => []).add(SplayTreeMap.of(fields));
        }
      }
      continue;
    }

    precedingRfis = [
      for (final rfi in requested)
        if (_rfiMatches(rawRfis, rfi)) rfi,
    ];
    for (final rfi in precedingRfis) {
      records.putIfAbsent(rfi, () => []).add(SplayTreeMap.of(fields));
    }
  }
  return records;
}

class _Header {
  const _Header(this.row, this.columns);
  final int row;
  final Map<String, int> columns;
}

WorksheetData? _findSheet(
  WorkbookData workbook,
  bool Function(String name) matches,
) {
  for (final entry in workbook.sheets.entries) {
    if (matches(entry.key)) return entry.value;
  }
  return null;
}

_Header _findHeader(
  WorksheetData sheet,
  List<String> required,
  List<String> oneOf,
  List<(String, List<String>)> mapped,
) {
  final lastRow = math.min(sheet.maxRow, 32);
  for (var row = 1; row <= lastRow; row++) {
    final headers = <String, int>{};
    for (var column = 1; column <= sheet.maxColumn; column++) {
      final name = _normalize(sheet.value(row, column));
      if (name.isNotEmpty) {
        headers[name] = column;
      }
    }
    if (required.every(headers.containsKey) && oneOf.any(headers.containsKey)) {
      final columns = <String, int>{};
      for (final (target, candidates) in mapped) {
        for (final candidate in candidates) {
          final column = headers[candidate];
          if (column != null) {
            columns[target] = column;
            break;
          }
        }
      }
      for (final key in required) {
        final column = headers[key];
        if (column != null) {
          columns[key] = column;
        }
      }
      return _Header(row, columns);
    }
  }
  throw const ConcreteDocumentError(
    'Could not identify an unambiguous workbook header row',
  );
}

String _valueFor(
  WorksheetData sheet,
  int row,
  Map<String, int> columns,
  String name,
) {
  final column = columns[name];
  if (column == null) return '';
  return _cellValue(sheet, row, column);
}

String _cellValue(WorksheetData sheet, int row, int column) {
  final cell = sheet.cell(row, column);
  if (cell == null) return '';
  return cell.value.trim();
}

/// ASCII alphanumerics only, uppercased then lowercased per the Rust
/// `normalize` helpers (the header matcher lowercases).
String _normalize(String value) =>
    String.fromCharCodes(value.codeUnits.where(_isAsciiAlphanumeric))
        .toLowerCase();

bool _isAsciiDigit(int unit) => unit >= 0x30 && unit <= 0x39;

bool _isAsciiAlphanumeric(int unit) =>
    _isAsciiDigit(unit) ||
    (unit >= 0x41 && unit <= 0x5a) ||
    (unit >= 0x61 && unit <= 0x7a);

String? _canonicalLogRfi(String value) {
  for (final code in rfi_code.exactCodes(value)) {
    if (code.contains('-CIV-')) return code;
  }
  final captures = _civSerial.firstMatch(value);
  if (captures == null) return null;
  final end = captures.end;
  if (end < value.length && _isAsciiDigit(value.codeUnitAt(end))) {
    return null;
  }
  final serial = captures.group(1)!;
  return 'AZE-SUN-INT-QMS-RFI-CIV-${serial.padLeft(4, '0')}';
}

bool _rfiMatches(String value, String expected) {
  if (rfi_code.exactCodes(value).contains(expected) ||
      _canonicalLogRfi(value) == expected) {
    return true;
  }
  final upper = _asciiUppercase(value);
  final position = upper.indexOf('CIV');
  if (position < 0) return false;
  final expectedSerial = expected.split('-').last;
  return value
      .substring(position + 3)
      .split(RegExp(r'[^0-9]'))
      .any((digits) => digits == expectedSerial);
}

String _asciiUppercase(String value) => String.fromCharCodes(
  value.codeUnits.map(
    (unit) => unit >= 0x61 && unit <= 0x7a ? unit - 32 : unit,
  ),
);
