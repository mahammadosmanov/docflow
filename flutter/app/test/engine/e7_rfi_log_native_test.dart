// Port of src-tauri/tests/rfi_log_native.rs (preview, revise, table and
// clipboard payloads, overflow/cancel/malformed contracts) plus the in-module
// identity test of src-tauri/src/rfi_log.rs.
//
// Apply/undo tests from the Rust file need FilesystemAuthority (fs_ops.rs),
// which is not ported yet; they are skipped with that reason below.
// UNVERIFIED: the pinned package SHA-256 values in the Rust test depend on
// Rust zip bytes and are not reproduced here.
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:archive/archive.dart';
import 'package:docflow/engine/contracts.dart';
import 'package:docflow/engine/errors.dart';
import 'package:docflow/engine/rfi_log.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

const String _main =
    'http://schemas.openxmlformats.org/spreadsheetml/2006/main';
const String _oracleRelative =
    '../../src-tauri/tests/fixtures/slice6_rfi_compatibility_oracle.json';
final DateTime _today = DateTime.utc(2026, 8, 25);

String _inline(String ref, String value, int style) =>
    '<c r="$ref" s="$style" t="inlineStr"><is><t>$value</t></is></c>';

String _number(String ref, String value, int style) =>
    '<c r="$ref" s="$style"><v>$value</v></c>';

String _columnName(int number) {
  var n = number;
  var out = '';
  while (n > 0) {
    out = String.fromCharCode(0x41 + (n - 1) % 26) + out;
    n = (n - 1) ~/ 26;
  }
  return out;
}

Uint8List _workbookBytes({bool macros = false}) {
  final archive = Archive();
  void add(String name, String content) {
    final bytes = utf8.encode(content);
    archive.addFile(ArchiveFile(name, bytes.length, bytes));
  }

  add(
    '[Content_Types].xml',
    '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        '<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>'
        '</Types>',
  );
  add(
    '_rels/.rels',
    '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rIdWorkbook" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>'
        '</Relationships>',
  );
  add(
    'xl/workbook.xml',
    '<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="$_main" '
        'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">'
        '<sheets><sheet name="CIV" sheetId="1" r:id="rId1"/><sheet name="MEC" sheetId="2" r:id="rId2"/>'
        '<sheet name="Sheet1" sheetId="3" r:id="rId3"/><sheet name="Unrelated" sheetId="4" r:id="rId4"/></sheets>'
        '<calcPr calcId="191029" calcMode="auto" fullCalcOnLoad="1"/></workbook>',
  );
  const worksheetType =
      'http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet';
  add(
    'xl/_rels/workbook.xml.rels',
    '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="$worksheetType" Target="worksheets/sheet1.xml"/>'
        '<Relationship Id="rId2" Type="$worksheetType" Target="worksheets/sheet2.xml"/>'
        '<Relationship Id="rId3" Type="$worksheetType" Target="worksheets/sheet3.xml"/>'
        '<Relationship Id="rId4" Type="$worksheetType" Target="worksheets/sheet4.xml"/>'
        '</Relationships>',
  );

  const civHeaders = [
    'RFI No.',
    'ITP No:',
    'Item No.',
    'INSPECTION DESCRIPTION',
    'FORMS',
    'TAG NO',
    'AREA',
    'DATE OF ISSUE',
    'INSPECTION DATE',
    'EXPIRY DATE',
    'INSPECTION START TIME',
    'INSP. TYPE AZFEN',
    'INSP. TYPE INTEC',
    'Inspection Location',
    'SUBCONTRACTOR',
    'Inspection Status',
    'RFI VALIDITY',
  ];
  final civHeader = [
    for (var index = 0; index < civHeaders.length; index++)
      _inline('${_columnName(index + 1)}9', civHeaders[index], 1),
  ].join();
  final civRow = [
    _inline('A10', 'AZE-SUN-INT-QMS-RFI-CIV-0007', 4),
    _inline('B10', 'AZE˗SUN˗INT˗QMS˗ITP˗CIV˗0005', 5),
    _inline('C10', 'CIV-5.2.1', 6),
    _inline('D10', 'Civil description', 9),
    _inline('E10', 'CON-CIV-001', 10),
    _inline('F10', 'N12', 7),
    _inline('G10', 'North Site', 7),
    _number('H10', '46258', 2),
    _number('I10', '46260', 2),
    _number('K10', '0.729166666666667', 3),
    _inline('L10', 'H', 8),
    _inline('M10', 'W', 8),
    _inline('N10', 'N3', 7),
    _inline('O10', 'AZFEN', 7),
  ].join();
  add(
    'xl/worksheets/sheet1.xml',
    '<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="$_main" '
        'xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" '
        'xmlns:xr2="http://schemas.microsoft.com/office/spreadsheetml/2015/revision2" mc:Ignorable="xr2">'
        '<dimension ref="A1:Q30"/><cols><col min="1" max="1" width="25" customWidth="1"/>'
        '<col min="4" max="4" width="31" customWidth="1"/></cols><sheetData>'
        '<row r="9">$civHeader</row><row r="10">$civRow</row>'
        '<row r="11"><c r="D11" s="9" t="str"><f>IFERROR(VLOOKUP(C11,Sheet1!K:L,2,0),"")</f><v/></c>'
        '<c r="E11" s="10"/><c r="J11" s="2" t="inlineStr"><is><t>KEEP EXPIRY</t></is></c>'
        '<c r="P11" t="inlineStr"><is><t>KEEP STATUS</t></is></c>'
        '<c r="Q11" t="inlineStr"><is><t>KEEP VALIDITY</t></is></c></row>'
        '<row r="30"><c r="Q30"/></row></sheetData></worksheet>',
  );
  const mecHeaders = [
    'RFI No.',
    'ITP No:',
    'Item No.',
    'INSPECTION DESCRIPTION',
    'FORMS',
    'TAG NO',
    'AREA',
    'ROW No.',
    'DATE OF ISSUE',
    'INSPECTION DATE',
    'EXPIRY DATE',
    'INSPECTION START TIME',
    'INSP. TYPE AZFEN',
    'INSP. TYPE INTEC',
    'Inspection Location',
    'SUBCONTRACTOR',
    'Inspection Status',
    'RFI VALIDITY',
  ];
  final mecHeader = [
    for (var index = 0; index < mecHeaders.length; index++)
      _inline('${_columnName(index + 1)}9', mecHeaders[index], 1),
  ].join();
  add(
    'xl/worksheets/sheet2.xml',
    '<worksheet xmlns="$_main"><dimension ref="A1:R30"/><sheetData>'
        '<row r="9">$mecHeader</row>'
        '<row r="10">${_inline('A10', 'AZE-SUN-INT-QMS-RFI-MEC-0020', 4)}${_inline('C10', 'MEC-3.2.1', 6)}</row>'
        '<row r="30"><c r="R30"/></row></sheetData></worksheet>',
  );
  final lookupHeader = [
    _inline('B1', 'AZE˗SUN˗INT˗QMS˗ITP˗CIV˗0005', 1),
    _inline('C1', 'AZE˗SUN˗INT˗QMS˗ITP˗MEC˗0003', 1),
    _inline('K1', 'Item', 1),
    _inline('L1', 'Description', 1),
    _inline('M1', 'Forms', 1),
    _inline('N1', 'Item', 1),
    _inline('O1', 'AZFEN', 1),
    _inline('P1', 'Item', 1),
    _inline('Q1', 'INTEC', 1),
  ].join();
  final lookupCiv = [
    _inline('K2', 'CIV-5.2.1', 1),
    _inline('L2', 'Civil description', 1),
    _inline('M2', 'CON-CIV-001', 1),
    _inline('N2', 'CIV-5.2.1', 1),
    _inline('O2', 'H', 1),
    _inline('P2', 'CIV-5.2.1', 1),
    _inline('Q2', 'W', 1),
  ].join();
  final lookupMec = [
    _inline('K3', 'MEC-3.2.1', 1),
    _inline('L3', 'Mechanical description', 1),
    _inline('M3', 'CON-MEC-002', 1),
    _inline('N3', 'MEC-3.2.1', 1),
    _inline('O3', 'H', 1),
    _inline('P3', 'MEC-3.2.1', 1),
    _inline('Q3', 'S', 1),
  ].join();
  add(
    'xl/worksheets/sheet3.xml',
    '<worksheet xmlns="$_main"><dimension ref="A1:Q3"/><sheetData>'
        '<row r="1">$lookupHeader</row><row r="2">$lookupCiv</row><row r="3">$lookupMec</row>'
        '</sheetData></worksheet>',
  );
  add(
    'xl/worksheets/sheet4.xml',
    '<worksheet xmlns="$_main"><sheetData><row r="1">${_inline('A1', 'UNRELATED', 1)}</row>'
        '</sheetData></worksheet>',
  );
  add(
    'xl/calcChain.xml',
    '<calcChain xmlns="$_main"><c r="D11" i="1"/><c r="D12"/><c r="A1" i="4"/></calcChain>',
  );
  for (final entry in const {
    'xl/styles.xml': 'styles-sentinel',
    'xl/theme/theme1.xml': 'theme-sentinel',
    'xl/drawings/drawing1.xml': 'drawing-sentinel',
    'xl/comments1.xml': 'comments-sentinel',
    'docProps/core.xml': 'metadata-sentinel',
  }.entries) {
    add(entry.key, entry.value);
  }
  if (macros) {
    final bytes = Uint8List.fromList([
      ...utf8.encode('VBA\u0000RFI'),
      0xff,
      ...utf8.encode('SENTINEL'),
    ]);
    archive.addFile(ArchiveFile('xl/vbaProject.bin', bytes.length, bytes));
  }
  final encoded = ZipEncoder().encode(archive)!;
  return Uint8List.fromList(encoded);
}

String _writeWorkbook(Directory dir, {bool macros = false}) {
  final path = p.join(dir.path, macros ? 'RFI Log.xlsm' : 'RFI Log.xlsx');
  File(path).writeAsBytesSync(_workbookBytes(macros: macros));
  return path;
}

Map<String, Object?> _values(String workbook) => {
  'workbook': workbook,
  'email_text': 'Create 1 CIV RFI number\nITP No: 5\nItem: CIV-5.2.1\nTag No: N12, S14\nInspection Location: Foundation A',
  'email_provider': 'offline',
  'gemini_api_key': '',
  'sheet_counts': {'CIV': 1, 'MEC': 0, 'ELE': 0, 'PRC': 0},
  'overflow_next_day': true,
};

Object? _deepCopy(Object? value) => jsonDecode(jsonEncode(value));

Map<String, Object?> _editor(Plan plan) =>
    plan.meta['editor']! as Map<String, Object?>;

void main() {
  late Directory tmp;

  setUp(() {
    tmp = Directory.systemTemp.createTempSync('e7_rfi_log_');
  });

  tearDown(() {
    tmp.deleteSync(recursive: true);
  });

  test(
    'preview, revise and payloads match the frozen compatibility oracle',
    () async {
      final workbook = _writeWorkbook(tmp, macros: true);
      final values = _values(workbook)
        ..['gemini_api_key'] = 'super-secret-token';
      final progress = <ProgressEvent>[];
      final plan = await preview(
        values,
        _today,
        progress: progress.add,
        cancelled: () => false,
      );

      expect(plan.opId, 'rfi.log_add');
      expect(jsonEncode(plan.toJson()), isNot(contains('super-secret-token')));
      expect(plan.steps.length, 1);
      expect(plan.steps.first.kind, StepKind.workbookUpdate);
      expect(plan.steps.first.dst!, endsWith('.xlsm'));
      final rows = _editor(plan)['rows']! as List<Object?>;
      expect(rows.length, 2);
      final first = rows[0]! as Map<String, Object?>;
      final second = rows[1]! as Map<String, Object?>;
      expect(first['rfiNo'], 'AZE-SUN-INT-QMS-RFI-CIV-0008');
      expect(second['rfiNo'], first['rfiNo']);
      expect(first['inspectionDate'], '27-Aug-26');
      expect(first['startTime'], '09:00');
      expect(first['area'], 'North Site');
      expect(second['area'], 'South Site');
      expect(progress.first.done, 0);
      expect(progress.last.phase, 'preview_ready');
      expect(progress.last.done, 1);
      expect(
        progress.any(
          (event) =>
              event.phase == 'rfi_plan_preparation' &&
              event.done == event.total &&
              event.total > 0,
        ),
        isTrue,
      );

      final batch =
          (plan.steps.first.meta['batches']! as List).first
              as Map<String, Object?>;
      final copySource = _editor(plan)['copySource']! as Map<String, Object?>;
      final oracle = jsonDecode(
        File(_oracleRelative).readAsStringSync(),
      ) as Map<String, Object?>;
      final actual = <String, Object?>{
        'opId': plan.opId,
        'warnings': [
          for (final warning in plan.warnings)
            {'key': warning.key, 'subject': warning.subject},
        ],
        'rows': _deepCopy(rows),
        'copyFields': _editor(plan)['copyFields'],
        'copyHeaders': _editor(plan)['copyHeaders'],
        'copyColumns': copySource['columns'],
        'batch': {
          'discipline': batch['discipline'],
          'startRow': batch['startRow'],
          'baseRfis': batch['baseRfis'],
          'lookupColumns': batch['lookupColumns'],
        },
      };
      expect(actual, equals(oracle));

      final before = previewTable(plan, sheet: 'CIV');
      expect(before['count'], 2);
      expect(before['fromWorkbook'], false);
      expect((before['columnWidths']! as List).first, 22.0);
      final payload = copyPayload(plan, sheet: 'CIV');
      expect(payload.count, 2);
      expect(payload.plain, contains('\t'));
      expect(payload.plain, isNot(contains('KEEP EXPIRY')));
      expect(payload.html, isNot(contains('KEEP STATUS')));
      final reply = replyPayload(plan);
      expect(reply.count, payload.count);
      expect(reply.plain, payload.plain);
      expect(reply.html, startsWith('<table'));
      expect(reply.html, endsWith('<br><br>'));

      final edited = _deepCopy(rows)! as List<Object?>;
      final editedSecond = edited[1]! as Map<String, Object?>;
      editedSecond['location'] = 'South foundation';
      editedSecond['_sheet'] = 'civ';
      editedSecond['itpNo'] = 'legacy-separator-damage';
      editedSecond.remove('rowNo');
      final revised = revise(plan, {'rows': edited});
      final revisedRows = _editor(revised)['rows']! as List<Object?>;
      expect(
        (revisedRows[1]! as Map<String, Object?>)['location'],
        'South foundation',
      );
      expect(
        (revisedRows[1]! as Map<String, Object?>)['itpNo'],
        'AZE˗SUN˗INT˗QMS˗ITP˗CIV˗0005',
      );
      final invalid = _deepCopy(revisedRows)! as List<Object?>;
      (invalid[0]! as Map<String, Object?>)['itemNo'] = 'CIV-DOES-NOT-EXIST';
      expect(
        () => revise(plan, {'rows': invalid}),
        throwsA(isA<BackendError>()),
      );
    },
  );

  test('missing workbook returns the localized message key', () async {
    final error = await preview(
      {},
      _today,
    ).then<Object?>((_) => null, onError: (Object error) => error);
    expect(error, isA<BackendError>());
    expect((error! as BackendError).message, 'error.rfi.workbookInvalid');
  });

  test('multi-sheet, overflow, cancel and malformed count contracts', () async {
    final workbook = _writeWorkbook(tmp);
    final multi = _values(workbook)
      ..['email_text'] = ''
      ..['sheet_counts'] = {'CIV': 1, 'MEC': 2};
    final plan = await preview(multi, _today);
    final rows = _editor(plan)['rows']! as List<Object?>;
    expect(rows.length, 3);
    int countSheet(String sheet) => rows
        .where((row) => (row! as Map<String, Object?>)['_sheet'] == sheet)
        .length;
    expect(countSheet('CIV'), 1);
    expect(countSheet('MEC'), 2);
    expect(() => previewTable(plan), throwsA(isA<BackendError>()));
    expect(previewTable(plan, sheet: 'MEC')['count'], 2);

    final overflow = _values(workbook)
      ..['email_text'] = ''
      ..['sheet_counts'] = {'CIV': 1}
      ..['overflow_next_day'] = false;
    final overflowPlan = await preview(overflow, _today);
    final overflowRows = _editor(overflowPlan)['rows']! as List<Object?>;
    final overflowRow = overflowRows[0]! as Map<String, Object?>;
    expect(overflowRow['inspectionDate'], '26-Aug-26');
    expect(overflowRow['startTime'], '18:00');
    expect(
      overflowPlan.warnings.any(
        (warning) =>
            warning.key == 'warn.rfi.overflowKept' && warning.subject == '1',
      ),
      isTrue,
    );

    final cancelled = await preview(
      multi,
      _today,
      cancelled: () => true,
    ).then<Object?>((_) => null, onError: (Object error) => error);
    expect((cancelled! as BackendError).code, 'operation_cancelled');

    final malformed = Map<String, Object?>.of(multi)
      ..['sheet_counts'] = {'CIV': 'many'};
    final malformedError = await preview(
      malformed,
      _today,
    ).then<Object?>((_) => null, onError: (Object error) => error);
    expect((malformedError! as BackendError).message, 'CIV RFI adedi gecersiz');

    final legacy = Map<String, Object?>.of(multi)
      ..remove('sheet_counts')
      ..['discipline'] = 'CIV'
      ..['count'] = 1.9;
    final legacyPlan = await preview(legacy, _today);
    expect((_editor(legacyPlan)['rows']! as List).length, 1);
  });

  test('rfi identity pads only RFI-anchored serials', () {
    expect(rfiPartsForTesting('AZE-SUN-INT-QMS-RFI-CIV-9'), ('CIV', 9));
    expect(rfiPartsForTesting('AZE-SUN-INT-QMS-RFI-CIV-901'), ('CIV', 901));
    expect(rfiPartsForTesting('ITP-CIV-901'), isNull);
    expect(rfiPartsForTesting('2026-09-01'), isNull);
    expect(rfiPartsForTesting('AZE-SUN-INT-QMS-RFI-CIV-09010'), isNull);
  });

  test(
    'approved apply and undo of the RFI Log workbook',
    () {},
    skip: 'Needs FilesystemAuthority (fs_ops.rs), not ported yet; owned by E2/E3.',
  );
}
