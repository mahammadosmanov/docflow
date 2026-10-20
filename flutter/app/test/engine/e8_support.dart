/// Shared fixtures for the E8 RFI Follow Up parity tests.
///
/// The Rust tests build XLSX packages from XML strings. Here the same sheets
/// are built as [SheetSpec] values and encoded into a compact JSON package.
/// [FakeOoxml] decodes that package into the workbook model the engine reads
/// and implements the guarded patch / delta verification over it. The real
/// zip/XML layer is the E6 ooxml port and is tested there.
library;

import 'dart:convert';
import 'dart:typed_data';

import 'package:docflow/engine/contracts.dart';
import 'package:docflow/engine/errors.dart';
import 'package:docflow/engine/ooxml.dart';
import 'package:docflow/engine/rfi_follow_up.dart';
import 'package:flutter_test/flutter_test.dart';

export 'package:docflow/engine/errors.dart' show BackendError;
export 'package:docflow/engine/rfi_follow_up.dart'
    show FollowUpFileIdentity, FollowUpWorkbookFile;

/// One cell in a sheet, positioned by its A1 reference.
class CellSpec {
  const CellSpec(this.reference, this.value, this.formula);
  final String reference;
  final String value;
  final String? formula;
}

/// One `<row>` element. Empty rows still count toward `max_row`.
class RowSpec {
  const RowSpec(this.number, this.cells);
  final int number;
  final List<CellSpec> cells;
}

/// One worksheet with its visibility and merged ranges.
class SheetSpec {
  const SheetSpec(
    this.rows, {
    this.merges = const [],
    this.protected = false,
    this.state,
  });
  final List<RowSpec> rows;
  final List<String> merges;
  final bool protected;
  final String? state;
}

/// Port of the Rust `inline(reference, value)` helper.
CellSpec inline(String reference, String value) =>
    CellSpec(reference, value, null);

/// Port of the Rust `blank(reference)` helper (an empty inline cell).
CellSpec blank(String reference) => CellSpec(reference, '', null);

/// Port of the Rust `formula(reference, expression, cached)` helper.
CellSpec formula(String reference, String expression, String cached) =>
    CellSpec(reference, cached, expression);

/// Port of the Rust `row(number, cells)` helper.
RowSpec row(int number, List<CellSpec> cells) => RowSpec(number, cells);

/// Port of the Rust `worksheet(rows, merges)` helper.
SheetSpec worksheet(List<RowSpec> rows, [List<String> merges = const []]) =>
    SheetSpec(rows, merges: merges);

/// Port of the Rust `package(&[(name, state, sheet)])` helper.
Uint8List package(List<(String, String?, SheetSpec)> sheets, {bool signed = false, bool workbookProtected = false}) {
  final encoded = <Map<String, Object?>>[];
  for (final (name, state, sheet) in sheets) {
    encoded.add(_encodeSheet(name, state, sheet));
  }
  return Uint8List.fromList(
    utf8.encode(
      jsonEncode({
        'signed': signed,
        'workbookProtected': workbookProtected,
        'sheets': encoded,
      }),
    ),
  );
}

/// Port of the Rust `source_package(civ, mec)` helper: CIV, MEC, ELE, PRC.
Uint8List sourcePackage(SheetSpec civ, SheetSpec mec) => package([
  ('CIV', null, civ),
  ('MEC', null, mec),
  ('ELE', null, worksheet(const [])),
  ('PRC', null, worksheet(const [])),
]);

/// Port of the Rust `target_package(sheet)` helper.
Uint8List targetPackage(SheetSpec sheet) => package([('Follow Up', null, sheet)]);

/// Port of the Rust `target_with_rows(rows, merges)` helper.
Uint8List targetWithRows(List<RowSpec> rows, [List<String> merges = const []]) =>
    targetPackage(
      worksheet([
        row(1, [inline('A1', 'RFI NO.'), inline('B1', 'STATUS')]),
        ...rows,
      ], merges),
    );

Map<String, Object?> _encodeSheet(String name, String? state, SheetSpec sheet) {
  var maxRow = 0;
  var maxColumn = 0;
  final cells = <List<Object?>>[];
  for (final rowSpec in sheet.rows) {
    if (rowSpec.number > maxRow) maxRow = rowSpec.number;
    for (final cell in rowSpec.cells) {
      final (r, c) = parseReference(cell.reference);
      if (r > maxRow) maxRow = r;
      if (c > maxColumn) maxColumn = c;
      cells.add([r, c, cell.value, cell.formula]);
    }
  }
  final merges = [
    for (final merge in sheet.merges) _parseRange(merge),
  ];
  return {
    'name': name,
    'state': state,
    'protected': sheet.protected,
    'maxRow': maxRow,
    'maxColumn': maxColumn,
    'cells': cells,
    'merges': merges,
  };
}

List<int> _parseRange(String range) {
  final parts = range.split(':');
  final (r1, c1) = parseReference(parts[0]);
  final (r2, c2) = parseReference(parts.length > 1 ? parts[1] : parts[0]);
  return [r1, c1, r2, c2];
}

/// Parses an A1 reference such as `AB12` into (row, column).
(int, int) parseReference(String reference) {
  var column = 0;
  var index = 0;
  while (index < reference.length) {
    final unit = reference.codeUnitAt(index);
    if (unit >= 0x41 && unit <= 0x5A) {
      column = column * 26 + (unit - 0x40);
    } else if (unit >= 0x61 && unit <= 0x7A) {
      column = column * 26 + (unit - 0x60);
    } else {
      break;
    }
    index++;
  }
  final row = int.parse(reference.substring(index));
  return (row, column);
}

/// Test double for the OOXML adapter over the JSON package encoding.
class FakeOoxml implements FollowUpOoxml {
  const FakeOoxml();

  @override
  FollowUpParsedWorkbook readWorkbookAndDiscovery(Uint8List bytes) {
    final json = _decode(bytes);
    final worksheets = <WorksheetDescriptor>[];
    final sheets = <String, WorksheetData>{};
    final list = (json['sheets']! as List).cast<Map<String, Object?>>();
    for (var index = 0; index < list.length; index++) {
      final sheet = list[index];
      final ordinal = index + 1;
      final name = sheet['name']! as String;
      final state = sheet['state'] as String?;
      worksheets.add(
        WorksheetDescriptor(
          workbookIndex: ordinal,
          sheetId: ordinal,
          relationshipId: 'rId$ordinal',
          relationshipType: 'worksheet',
          partPath: 'xl/worksheets/sheet$ordinal.xml',
          name: name,
          visibility: switch (state) {
            null || 'visible' => WorksheetVisibility.visible,
            'hidden' => WorksheetVisibility.hidden,
            _ => WorksheetVisibility.veryHidden,
          },
        ),
      );
      final cells = <(int, int), CellData>{};
      for (final entry in (sheet['cells']! as List).cast<List<Object?>>()) {
        cells[((entry[0]! as num).toInt(), (entry[1]! as num).toInt())] =
            CellData(
              value: entry[2]! as String,
              formula: entry[3] as String?,
            );
      }
      final merges = [
        for (final range in (sheet['merges']! as List).cast<List<Object?>>())
          CellRange(
            firstRow: (range[0]! as num).toInt(),
            firstColumn: (range[1]! as num).toInt(),
            lastRow: (range[2]! as num).toInt(),
            lastColumn: (range[3]! as num).toInt(),
          ),
      ];
      sheets[name] = WorksheetData(
        maxRow: (sheet['maxRow']! as num).toInt(),
        maxColumn: (sheet['maxColumn']! as num).toInt(),
        cells: cells,
        hasSheetProtection: sheet['protected']! as bool,
        mergedRanges: merges,
      );
    }
    return FollowUpParsedWorkbook(
      OrderedWorkbookDiscovery(
        worksheets: worksheets,
        safety: WorkbookSafetyFacts(
          hasVbaProject: false,
          hasXlmMacroSheet: false,
          hasWorkbookProtection: json['workbookProtected']! as bool,
          externalLinkCount: 0,
          connectionCount: 0,
          queryTableCount: 0,
        ),
      ),
      WorkbookData(sheets: sheets),
    );
  }

  @override
  bool packageHasSignature(Uint8List bytes) => _decode(bytes)['signed']! as bool;

  @override
  Uint8List patchWorkbookSheetsGuardedText(
    Uint8List source,
    Map<String, Map<(int, int), GuardedCellUpdate>> updates,
  ) {
    final json = _decode(source);
    final list = (json['sheets']! as List).cast<Map<String, Object?>>();
    for (final sheetJson in list) {
      final sheetUpdates = updates[sheetJson['name']! as String];
      if (sheetUpdates == null) continue;
      final cells = (sheetJson['cells']! as List).cast<List<Object?>>();
      for (final entry in sheetUpdates.entries) {
        final (row, column) = entry.key;
        final guarded = entry.value;
        final index = cells.indexWhere(
          (c) => c[0] == row && c[1] == column,
        );
        if (index < 0) {
          if (!guarded.createIfAbsent) {
            throw const FollowUpOoxmlError(false, 'guarded cell missing');
          }
          continue;
        }
        final current = cells[index];
        final expected = guarded.expected;
        if (current[2] != expected.value || current[3] != expected.formula) {
          throw const FollowUpOoxmlError(false, 'guarded cell changed');
        }
        final replacement = guarded.replacement;
        if (replacement is! CellUpdateText) {
          throw const FollowUpOoxmlError(false, 'unsupported replacement');
        }
        cells[index] = [row, column, replacement.value, null];
      }
    }
    return Uint8List.fromList(utf8.encode(jsonEncode(json)));
  }

  @override
  void verifyGuardedWorkbookDeltaText(
    Uint8List original,
    Uint8List output,
    Map<String, Map<(int, int), GuardedCellUpdate>> updates,
  ) {
    final before = _cellMap(_decode(original));
    final after = _cellMap(_decode(output));
    if (before.keys.toSet().difference(after.keys.toSet()).isNotEmpty) {
      throw const FollowUpOoxmlError(false, 'cell removed');
    }
    for (final sheet in after.keys) {
      final sheetUpdates = updates[sheet] ?? const {};
      for (final coordinate in after[sheet]!.keys) {
        final beforeCell = before[sheet]![coordinate];
        final afterCell = after[sheet]![coordinate];
        final update = sheetUpdates[coordinate];
        if (update == null) {
          if (beforeCell != afterCell) {
            throw const FollowUpOoxmlError(false, 'unapproved delta');
          }
        } else {
          final replacement = (update.replacement as CellUpdateText).value;
          if (afterCell != replacement) {
            throw const FollowUpOoxmlError(false, 'approved delta mismatch');
          }
        }
      }
    }
  }

  Map<String, Map<(int, int), String>> _cellMap(Map<String, Object?> json) {
    final out = <String, Map<(int, int), String>>{};
    for (final sheet in (json['sheets']! as List).cast<Map<String, Object?>>()) {
      final cells = <(int, int), String>{};
      for (final entry in (sheet['cells']! as List).cast<List<Object?>>()) {
        cells[((entry[0]! as num).toInt(), (entry[1]! as num).toInt())] =
            entry[2]! as String;
      }
      out[sheet['name']! as String] = cells;
    }
    return out;
  }
}

Map<String, Object?> _decode(Uint8List bytes) =>
    jsonDecode(utf8.decode(bytes)) as Map<String, Object?>;

/// Port of the Rust `classify(source, target)` helper.
JsonObject classifyBytes(Uint8List source, Uint8List target) {
  const ooxml = FakeOoxml();
  final sourceParsed = ooxml.readWorkbookAndDiscovery(source);
  final targetParsed = ooxml.readWorkbookAndDiscovery(target);
  return classifyWorkbooks(
    sourceParsed.discovery,
    sourceParsed.workbook,
    targetParsed.discovery,
    targetParsed.workbook,
    targetHasSignature: false,
  );
}

/// Port of the Rust `compatibility_plan` helper: preview from two files.
Plan previewBytes(Uint8List source, Uint8List target, {String token = 'a1b2c3d4e5f60718293a4b5c6d7e8f90'}) {
  return previewFromOpenFiles(
    rfiLog: FollowUpWorkbookFile(
      path: 'RFI Log.xlsx',
      bytes: source,
      identity: const FollowUpFileIdentity.unix(device: 1, inode: 10),
      mtimeNs: 1000,
    ),
    followUp: FollowUpWorkbookFile(
      path: 'Follow Up.xlsx',
      bytes: target,
      identity: const FollowUpFileIdentity.unix(device: 1, inode: 20),
      mtimeNs: 2000,
    ),
    ooxml: const FakeOoxml(),
    newToken: () => token,
  );
}

/// Expects a [BackendError] whose message equals [message].
Matcher throwsBackendMessage(String message) => throwsA(
  isA<BackendError>().having((error) => error.message, 'message', message),
);

/// Port of the Rust `public_oracle_projection` helper.
JsonObject publicOracleProjection(Plan plan) {
  final editor = plan.meta['editor']! as Map<String, Object?>;
  final rows = [
    for (final row in (editor['rows']! as List).cast<Map<String, Object?>>())
      {
        '_rowId': row['_rowId'],
        '_sheet': row['_sheet'],
        '_row': row['_row'],
        'rfiNo': row['rfiNo'],
        'sourceStatuses': row['sourceStatuses'],
        'currentStatus': row['currentStatus'],
        'proposedStatus': row['proposedStatus'],
        'disposition': row['disposition'],
        'reasonKey': row['reasonKey'],
        'selectable': row['selectable'],
        'enabled': row['enabled'],
        'sourceProvenance': [
          for (final evidence
              in (row['sourceEvidence']! as List).cast<Map<String, Object?>>())
            {'sheet': evidence['sheet'], 'row': evidence['row']},
        ],
      },
  ];
  final updates = [
    for (final update in (plan.steps.isEmpty
            ? const <Object?>[]
            : (plan.steps.first.meta['updates']! as List)))
      {
        'rowId': (update! as Map)['rowId'],
        'canonicalRfi': update['canonicalRfi'],
        'proposedStatus': update['proposedStatus'],
        'target': {
          'sheet': (update['target'] as Map)['worksheet'] is Map
              ? ((update['target'] as Map)['worksheet'] as Map)['name']
              : null,
          'row': (update['target'] as Map)['row'],
          'rfiColumn': (update['target'] as Map)['rfiColumn'],
          'statusColumn': (update['target'] as Map)['statusColumn'],
        },
      },
  ];
  return {
    'opId': plan.opId,
    'columns': editor['columns'],
    'rows': rows,
    'updates': updates,
    'warnings': [for (final warning in plan.warnings) warning.toJson()],
  };
}

/// Port of the Rust `oracle_projection` helper over an editor projection.
JsonObject oracleProjection(JsonObject review) {
  final rows = [
    for (final row in (review['rows']! as List).cast<Map<String, Object?>>())
      {
        '_rowId': row['_rowId'],
        '_sheet': row['_sheet'],
        '_row': row['_row'],
        'rfiNo': row['rfiNo'],
        'sourceStatuses': row['sourceStatuses'],
        'currentStatus': row['currentStatus'],
        'proposedStatus': row['proposedStatus'],
        'disposition': row['disposition'],
        'reasonKey': row['reasonKey'],
        'selectable': row['selectable'],
        'enabled': row['enabled'],
        'sourceProvenance': [
          for (final evidence
              in (row['sourceEvidence']! as List).cast<Map<String, Object?>>())
            {'sheet': evidence['sheet'], 'row': evidence['row']},
        ],
      },
  ];
  return {'rows': rows, 'warnings': review['warnings']};
}
