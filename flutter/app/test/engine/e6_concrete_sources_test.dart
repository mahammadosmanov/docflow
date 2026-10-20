// Port of the `mod tests` block in src-tauri/src/concrete_document_sources.rs.
import 'dart:convert';
import 'dart:typed_data';

import 'package:docflow/engine/concrete_document_sources.dart';
import 'package:flutter_test/flutter_test.dart';

import 'e6_ooxml_fixtures.dart';

Uint8List _followUpWorkbook(List<(String, String, String)> rows) {
  final worksheet = StringBuffer();
  for (var index = 0; index < rows.length; index++) {
    final row = index + 1;
    final (rfi, klass, ticket) = rows[index];
    worksheet.write(
      '<row r="$row"><c r="A$row" t="inlineStr"><is><t>$rfi</t></is></c>'
      '<c r="B$row" t="inlineStr"><is><t>$klass</t></is></c>'
      '<c r="C$row" t="inlineStr"><is><t>$ticket</t></is></c></row>',
    );
  }
  final sheet =
      '<worksheet xmlns="$mainNs"><sheetData>$worksheet</sheetData></worksheet>';
  const workbook =
      '<workbook xmlns="$mainNs" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">'
      '<sheets><sheet name="Follow Up" sheetId="1" r:id="rId1"/></sheets></workbook>';
  const relationships =
      '<Relationships xmlns="$packageRelNs"><Relationship Id="rId1" Type="$relNs" '
      'Target="worksheets/sheet1.xml"/></Relationships>';
  const packageRelationships =
      '<Relationships xmlns="$packageRelNs"><Relationship Id="rIdWorkbook" '
      'Type="$officeDocumentRel" Target="xl/workbook.xml"/></Relationships>';
  List<int> b(String text) => utf8.encode(text);
  return zipOf([
    ('[Content_Types].xml', b('content-types')),
    ('_rels/.rels', b(packageRelationships)),
    ('xl/workbook.xml', b(workbook)),
    ('xl/_rels/workbook.xml.rels', b(relationships)),
    ('xl/worksheets/sheet1.xml', b(sheet)),
  ]);
}

Map<String, List<Map<String, String>>> _read(
  List<(String, String, String)> rows,
  List<String> requested,
) => readFollowUpRecords(_followUpWorkbook(rows), requested);

void main() {
  test('concrete specimen follow-up is only expected for exact form code', () {
    expect(hasConCiv010Form('CON-CIV-010'), isTrue);
    expect(hasConCiv010Form('CON-CIV-013 / con civ 010'), isTrue);
    expect(hasConCiv010Form('CON-CIV-011'), isFalse);
    expect(hasConCiv010Form('CON-CIV-0105'), isFalse);
    expect(hasConCiv010Form('CON-CIV-1010'), isFalse);
  });

  test('requested RFIs accept bare serials and reject non-CIV codes', () {
    expect(requestedRfis('12, CIV-0013; 12'), [
      'AZE-SUN-INT-QMS-RFI-CIV-0012',
      'AZE-SUN-INT-QMS-RFI-CIV-0013',
    ]);
    expect(
      () => requestedRfis('CIV-XYZ'),
      throwsA(isA<ConcreteDocumentError>()),
    );
    expect(() => requestedRfis(' ,; '), throwsA(isA<ConcreteDocumentError>()));
  });

  test('retains multiple explicit batches for each rfi', () {
    final records = _read(
      [
        ('RFI No', 'Concrete Class', 'Batch Ticket No'),
        ('CIV-0123', 'C40', 'A-1'),
        ('CIV-0123', 'C45', 'A-2'),
      ],
      ['AZE-SUN-INT-QMS-RFI-CIV-0123'],
    );
    final batches = records['AZE-SUN-INT-QMS-RFI-CIV-0123']!;
    expect(batches.length, 2);
    expect(batches[0]['batch_ticket'], 'A-1');
    expect(batches[1]['batch_ticket'], 'A-2');
  });

  test('ignores blank spacers and attaches continuation rows', () {
    final records = _read(
      [
        ('RFI No', 'Concrete Class', 'Batch Ticket No'),
        ('CIV-0123', 'C40', 'A-1'),
        ('', '', ''),
        ('', 'C40', 'A-2'),
      ],
      ['AZE-SUN-INT-QMS-RFI-CIV-0123'],
    );
    final batches = records['AZE-SUN-INT-QMS-RFI-CIV-0123']!;
    expect(batches.length, 2);
    expect(batches[1]['batch_ticket'], 'A-2');
  });

  test(
    'composite rfi rows and continuations are kept for each requested rfi',
    () {
      final records = _read(
        [
          ('RFI No', 'Concrete Class', 'Batch Ticket No'),
          ('CIV-0123, CIV-0124', 'C40', 'A-1'),
          ('', 'C40', 'A-2'),
        ],
        ['AZE-SUN-INT-QMS-RFI-CIV-0123', 'AZE-SUN-INT-QMS-RFI-CIV-0124'],
      );
      for (final rfi in [
        'AZE-SUN-INT-QMS-RFI-CIV-0123',
        'AZE-SUN-INT-QMS-RFI-CIV-0124',
      ]) {
        expect(records[rfi]!.length, 2);
        expect(records[rfi]![1]['batch_ticket'], 'A-2');
      }
    },
  );
}
