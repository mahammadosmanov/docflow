// Dart port of src-tauri/src/ooxml.rs: OOXML (xlsx) zip/xml read-write with
// byte-fidelity for untouched package members and XML content.
//
// Library layout (parts share private scope):
//   ooxml/xml.dart       raw-preserving XML tree (package:xml event spans)
//   ooxml/package.dart   ZIP preflight, member reads, raw-copy package writer
//   ooxml/workbook.dart  workbook discovery, relationships, safety facts
//   ooxml/sheet.dart     worksheet reading, shared strings, cell patching
//   ooxml/patch.dart     public patch / guarded verify entry points
library;

import 'dart:collection';
import 'dart:convert';
import 'dart:math' as math;
import 'dart:typed_data';

import 'package:archive/archive.dart';
import 'package:xml/xml_events.dart';

part 'ooxml/xml.dart';
part 'ooxml/package.dart';
part 'ooxml/workbook.dart';
part 'ooxml/sheet.dart';
part 'ooxml/patch.dart';

const int maxPackageBytes = 256 * 1024 * 1024;
const int maxMergeValidationComparisonsPerWorkbook = 1000000;
const int maxApplyMergeComparisons = 20000000;
const int _maxUncompressedBytes = 512 * 1024 * 1024;
const int _maxEntryBytes = 128 * 1024 * 1024;
const int _maxEntries = 4096;

/// Exposed for tests: the ZIP entry cap.
const int maxEntriesForTesting = _maxEntries;
const int _maxWorksheets = 250;
const int _maxSheetNameBytes = 1024;
const int _maxRelationshipIdBytes = 1024;
const int _maxRelationshipTypeBytes = 2048;
const int _maxRelationshipTargetBytes = 4096;
const int _maxStateBytes = 16;
const int _maxU32AttributeBytes = 10;
const int _maxMergedRanges = 10000;
const int _maxWorksheetRows = 1048576;
const int _maxWorksheetColumns = 16384;

const String _transitionalWorkbookNamespace =
    'http://schemas.openxmlformats.org/spreadsheetml/2006/main';
const String _strictWorkbookNamespace =
    'http://purl.oclc.org/ooxml/spreadsheetml/main';
const String _transitionalRelationshipNamespace =
    'http://schemas.openxmlformats.org/officeDocument/2006/relationships';
const String _strictRelationshipNamespace =
    'http://purl.oclc.org/ooxml/officeDocument/relationships';
const String _transitionalOfficeDocumentRelationship =
    'http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument';
const String _strictOfficeDocumentRelationship =
    'http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument';
const String _transitionalPackageRelationshipNamespace =
    'http://schemas.openxmlformats.org/package/2006/relationships';
const String _strictPackageRelationshipNamespace =
    'http://purl.oclc.org/ooxml/package/relationships';
const List<String> _workbookNamespaces = [
  _transitionalWorkbookNamespace,
  _strictWorkbookNamespace,
];
const List<String> _packageRelationshipNamespaces = [
  _transitionalPackageRelationshipNamespace,
  _strictPackageRelationshipNamespace,
];
const String _transitionalRelationshipPrefix =
    'http://schemas.openxmlformats.org/officeDocument/2006/relationships/';
const String _strictRelationshipPrefix =
    'http://purl.oclc.org/ooxml/officeDocument/relationships/';

enum OoxmlErrorKind { invalid, limitExceeded }

class OoxmlError implements Exception {
  const OoxmlError(this.kind, this.message);

  const OoxmlError.invalid(String message)
    : this(OoxmlErrorKind.invalid, message);

  const OoxmlError.limitExceeded(String message)
    : this(OoxmlErrorKind.limitExceeded, message);

  final OoxmlErrorKind kind;
  final String message;

  @override
  String toString() => message;
}

sealed class CellUpdate {
  const CellUpdate();
}

final class CellUpdateText extends CellUpdate {
  const CellUpdateText(this.value);
  final String value;
  @override
  bool operator ==(Object other) =>
      other is CellUpdateText && other.value == value;
  @override
  int get hashCode => value.hashCode;
}

final class CellUpdateNumber extends CellUpdate {
  const CellUpdateNumber(this.value);
  final String value;
  @override
  bool operator ==(Object other) =>
      other is CellUpdateNumber && other.value == value;
  @override
  int get hashCode => value.hashCode;
}

final class CellUpdateBool extends CellUpdate {
  const CellUpdateBool(this.value);
  final bool value;
  @override
  bool operator ==(Object other) =>
      other is CellUpdateBool && other.value == value;
  @override
  int get hashCode => value.hashCode;
}

final class CellUpdateFormula extends CellUpdate {
  const CellUpdateFormula({required this.formula, this.cached});
  final String formula;
  final String? cached;
  @override
  bool operator ==(Object other) =>
      other is CellUpdateFormula &&
      other.formula == formula &&
      other.cached == cached;
  @override
  int get hashCode => Object.hash(formula, cached);
}

class GuardedCellUpdate {
  const GuardedCellUpdate({
    required this.expected,
    required this.replacement,
    required this.createIfAbsent,
  });
  final CellData expected;
  final CellUpdate replacement;
  final bool createIfAbsent;

  @override
  bool operator ==(Object other) =>
      other is GuardedCellUpdate &&
      other.expected == expected &&
      other.replacement == replacement &&
      other.createIfAbsent == createIfAbsent;

  @override
  int get hashCode => Object.hash(expected, replacement, createIfAbsent);
}

class SheetInfo {
  const SheetInfo({required this.path, required this.sheetId});
  final String path;
  final int sheetId;

  @override
  bool operator ==(Object other) =>
      other is SheetInfo && other.path == path && other.sheetId == sheetId;
  @override
  int get hashCode => Object.hash(path, sheetId);
}

enum WorksheetVisibility { visible, hidden, veryHidden }

class WorksheetDescriptor {
  const WorksheetDescriptor({
    required this.workbookIndex,
    required this.sheetId,
    required this.relationshipId,
    required this.relationshipType,
    required this.partPath,
    required this.name,
    required this.visibility,
  });

  final int workbookIndex;
  final int sheetId;
  final String relationshipId;
  final String relationshipType;
  final String partPath;
  final String name;
  final WorksheetVisibility visibility;

  @override
  bool operator ==(Object other) =>
      other is WorksheetDescriptor &&
      other.workbookIndex == workbookIndex &&
      other.sheetId == sheetId &&
      other.relationshipId == relationshipId &&
      other.relationshipType == relationshipType &&
      other.partPath == partPath &&
      other.name == name &&
      other.visibility == visibility;

  @override
  int get hashCode => Object.hash(
    workbookIndex,
    sheetId,
    relationshipId,
    relationshipType,
    partPath,
    name,
    visibility,
  );
}

class WorkbookSafetyFacts {
  const WorkbookSafetyFacts({
    this.hasVbaProject = false,
    this.hasXlmMacroSheet = false,
    this.hasWorkbookProtection = false,
    this.externalLinkCount = 0,
    this.connectionCount = 0,
    this.queryTableCount = 0,
  });

  final bool hasVbaProject;
  final bool hasXlmMacroSheet;
  final bool hasWorkbookProtection;
  final int externalLinkCount;
  final int connectionCount;
  final int queryTableCount;

  WorkbookSafetyFacts copyWith({
    bool? hasVbaProject,
    bool? hasXlmMacroSheet,
    bool? hasWorkbookProtection,
    int? externalLinkCount,
    int? connectionCount,
    int? queryTableCount,
  }) => WorkbookSafetyFacts(
    hasVbaProject: hasVbaProject ?? this.hasVbaProject,
    hasXlmMacroSheet: hasXlmMacroSheet ?? this.hasXlmMacroSheet,
    hasWorkbookProtection: hasWorkbookProtection ?? this.hasWorkbookProtection,
    externalLinkCount: externalLinkCount ?? this.externalLinkCount,
    connectionCount: connectionCount ?? this.connectionCount,
    queryTableCount: queryTableCount ?? this.queryTableCount,
  );

  @override
  bool operator ==(Object other) =>
      other is WorkbookSafetyFacts &&
      other.hasVbaProject == hasVbaProject &&
      other.hasXlmMacroSheet == hasXlmMacroSheet &&
      other.hasWorkbookProtection == hasWorkbookProtection &&
      other.externalLinkCount == externalLinkCount &&
      other.connectionCount == connectionCount &&
      other.queryTableCount == queryTableCount;

  @override
  int get hashCode => Object.hash(
    hasVbaProject,
    hasXlmMacroSheet,
    hasWorkbookProtection,
    externalLinkCount,
    connectionCount,
    queryTableCount,
  );
}

class OrderedWorkbookDiscovery {
  const OrderedWorkbookDiscovery({
    required this.worksheets,
    required this.safety,
  });
  final List<WorksheetDescriptor> worksheets;
  final WorkbookSafetyFacts safety;

  @override
  bool operator ==(Object other) =>
      other is OrderedWorkbookDiscovery &&
      _listEquals(other.worksheets, worksheets) &&
      other.safety == safety;

  @override
  int get hashCode => Object.hash(Object.hashAll(worksheets), safety);
}

bool _listEquals<T>(List<T> a, List<T> b) {
  if (a.length != b.length) return false;
  for (var i = 0; i < a.length; i++) {
    if (a[i] != b[i]) return false;
  }
  return true;
}

class CellData {
  const CellData({this.value = '', this.formula, this.style});
  final String value;
  final String? formula;
  final String? style;

  @override
  bool operator ==(Object other) =>
      other is CellData &&
      other.value == value &&
      other.formula == formula &&
      other.style == style;

  @override
  int get hashCode => Object.hash(value, formula, style);

  @override
  String toString() => 'CellData(value: $value, formula: $formula, style: $style)';
}

class CellRange {
  const CellRange({
    required this.firstRow,
    required this.firstColumn,
    required this.lastRow,
    required this.lastColumn,
  });
  final int firstRow;
  final int firstColumn;
  final int lastRow;
  final int lastColumn;

  @override
  bool operator ==(Object other) =>
      other is CellRange &&
      other.firstRow == firstRow &&
      other.firstColumn == firstColumn &&
      other.lastRow == lastRow &&
      other.lastColumn == lastColumn;

  @override
  int get hashCode => Object.hash(firstRow, firstColumn, lastRow, lastColumn);
}

/// Parsed worksheet. Port of `WorksheetData`. Cells are keyed by (row, column).
class WorksheetData {
  WorksheetData({
    this.maxRow = 0,
    this.maxColumn = 0,
    Map<(int, int), CellData>? cells,
    Map<int, double>? columnWidths,
    this.hasSheetProtection = false,
    List<CellRange>? mergedRanges,
  }) : cells = cells ?? <(int, int), CellData>{},
       columnWidths = columnWidths ?? <int, double>{},
       mergedRanges = mergedRanges ?? <CellRange>[];

  int maxRow;
  int maxColumn;
  final Map<(int, int), CellData> cells;
  final Map<int, double> columnWidths;
  bool hasSheetProtection;
  final List<CellRange> mergedRanges;

  String value(int row, int column) => cells[(row, column)]?.value ?? '';

  CellData? cell(int row, int column) => cells[(row, column)];

  CellRange? mergedRangeContaining(int row, int column) {
    for (final range in mergedRanges) {
      if (range.firstRow <= row &&
          row <= range.lastRow &&
          range.firstColumn <= column &&
          column <= range.lastColumn) {
        return range;
      }
    }
    return null;
  }
}

/// Parsed workbook. Port of `WorkbookData`.
class WorkbookData {
  WorkbookData({
    Map<String, WorksheetData>? sheets,
    this.mergeValidationComparisons = 0,
    this.mergeValidationComplete = false,
  }) : sheets = sheets ?? <String, WorksheetData>{};

  /// Sheets keyed by name, in discovery order.
  final Map<String, WorksheetData> sheets;
  final int mergeValidationComparisons;
  final bool mergeValidationComplete;

  WorksheetData? sheet(String name) => sheets[name];
}

/// Port of `MergeComparisonBudget`.
class MergeComparisonBudget {
  MergeComparisonBudget._(this._remaining, this.consumed);

  factory MergeComparisonBudget.forValidation() => MergeComparisonBudget._(
    maxMergeValidationComparisonsPerWorkbook,
    0,
  );

  factory MergeComparisonBudget.forApply(int parsedComparisons) {
    final remaining = maxApplyMergeComparisons - parsedComparisons;
    if (remaining < 0) {
      throw const OoxmlError.limitExceeded(
        'guarded merge comparison count exceeded the limit',
      );
    }
    return MergeComparisonBudget._(remaining, parsedComparisons);
  }

  int _remaining;
  int consumed;

  void consume() {
    if (_remaining <= 0) {
      throw const OoxmlError.limitExceeded(
        'guarded merge comparison count exceeded the limit',
      );
    }
    _remaining--;
    consumed++;
  }
}

/// Test hook (no production callers): runs the archive preflight (port of `preflight_archive`).
void preflightArchiveForTesting(Uint8List source) => _preflightArchive(source);

/// Test hook: `parse_document_with_limits` with explicit depth and node caps.
XmlDocument parseDocumentWithLimitsForTesting(
  Uint8List raw, {
  required int maxDepth,
  required int maxNodes,
}) => _parseDocumentWithLimits(raw, maxDepth, maxNodes);

