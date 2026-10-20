// Port of the `mod tests` block in src-tauri/src/concrete_documents.rs.
import 'dart:convert';
import 'dart:typed_data';

import 'package:docflow/engine/concrete_document_sources.dart'
    show ConcreteDocumentError;
import 'package:docflow/engine/concrete_documents.dart';
import 'package:docflow/engine/ooxml.dart';
import 'package:flutter_test/flutter_test.dart';

import 'e6_ooxml_fixtures.dart';

SourceRecord _record(String rfi, String location) =>
    SourceRecord(rfi: rfi, location: location);

Uint8List _package(List<(String, String)> entries) =>
    zipOf([for (final (name, body) in entries) (name, utf8.encode(body))]);

String _member(Uint8List bytes, String name) =>
    utf8.decode(readMember(bytes, name));

/// Matches `<c r="REF" ...>(inner)</c>` and returns the inner XML.
String? _cellInner(String sheet, String reference) {
  final match = RegExp(
    '<c\\b[^>]*\\br="$reference"[^>]*>(.*?)</c>',
    dotAll: true,
  ).firstMatch(sheet);
  return match?.group(1);
}

void main() {
  test('extends civ010 template rows without overwriting following rows', () {
    const worksheet =
        '<worksheet><dimension ref="A1:CS22"/><sheetData><row r="19"><c r="AI19"><v>1</v></c></row><row r="20"></row><row r="21"><c r="AI21"><v>2</v></c></row><row r="22"><c r="AI22"><v>3</v></c></row></sheetData></worksheet>';
    final expanded = expandCivBatchRows(worksheet, 4);
    expect(expanded, contains('<dimension ref="A1:CS26"/>'));
    expect(expanded, contains('<row r="23"><c r="AI23"><v>2</v></c></row>'));
    expect(expanded, contains('<row r="25"><c r="AI25"><v>2</v></c></row>'));
    expect(expanded, contains('<row r="26"><c r="AI26"><v>3</v></c></row>'));
  });

  test('generates docx per record and preserves unrelated parts', () {
    final template = _package([
      (
        'word/document.xml',
        '<w:document><w:p><w:t>AZE-SUN-INT-QMS-</w:t><w:t>RFI-CIV-1498</w:t></w:p><w:p><w:t>LOCATION</w:t><w:t>SOUTH SITE PCU-18</w:t></w:p><w:p><w:t>AMBIENT TEMP.</w:t><w:t>27 °C</w:t></w:p></w:document>',
      ),
      ('word/styles.xml', '<w:styles><w:style w:val="unchanged"/></w:styles>'),
    ]);
    final first = _record('AZE-SUN-INT-QMS-RFI-CIV-2001', 'PCU-1')
      ..ambientRange = (31, 31);
    final second = _record('AZE-SUN-INT-QMS-RFI-CIV-2002', 'PCU-2')
      ..ambientRange = (42, 42);
    final result = generateConcreteDocuments(
      documentType: DocumentType.civ011,
      template: template,
      records: [first, second],
      ambientRange: (20, 30),
      rngSeed: 4,
    );
    expect(result.length, 2);
    final doc0 = _member(result[0].bytes, 'word/document.xml');
    expect(doc0, contains('31 °C'));
    expect(doc0, contains('PCU-1'));
    final doc1 = _member(result[1].bytes, 'word/document.xml');
    expect(doc1, contains('42 °C'));
    expect(doc1, contains('PCU-2'));
    expect(
      _member(result[0].bytes, 'word/styles.xml'),
      '<w:styles><w:style w:val="unchanged"/></w:styles>',
    );
    expect(result[0].filename, 'CON-CIV-011_CIV-2001.docx');
  });

  test('generates xlsx and rejects missing anchor', () {
    final template = _package([
      (
        'xl/worksheets/sheet1.xml',
        '<worksheet><row><c><is><t>AZE-SUN-INT-QMS-RFI-CIV-1500</t></is></c><c><is><t>LOCATION/STRUCTURE</t></is></c><c><is><t>PCU-18 (South Site)</t></is></c></row></worksheet>',
      ),
      ('xl/styles.xml', '<styleSheet/>'),
    ]);
    final result = generateConcreteDocuments(
      documentType: DocumentType.civ013,
      template: template,
      records: [_record('AZE-SUN-INT-QMS-RFI-CIV-2003', 'PCU-9')],
      ambientRange: (0, 0),
      rngSeed: 1,
    );
    expect(
      _member(result[0].bytes, 'xl/worksheets/sheet1.xml'),
      contains('PCU-9'),
    );
    final invalid = _package([('xl/worksheets/sheet1.xml', '<worksheet/>')]);
    expect(
      () => generateConcreteDocuments(
        documentType: DocumentType.civ013,
        template: invalid,
        records: [_record('CIV-1', 'x')],
        ambientRange: (0, 0),
        rngSeed: 0,
      ),
      throwsA(isA<ConcreteDocumentError>()),
    );
  });

  test('generates civ010 data and civ014 identity', () {
    final source = _record('AZE-SUN-INT-QMS-RFI-CIV-2010', 'N-5')
      ..tag = 'N-5'
      ..concreteClass = 'C-30/37 XA2'
      ..supplier = 'HOLCIM'
      ..batchTicket = '66817'
      ..truck = 'TRK-1'
      ..quantity = '5'
      ..totalQuantity = '15'
      ..batchTime = '09:30'
      ..pourDate = '06.10.2026'
      ..startTime = '10:00'
      ..endTime = '11:00'
      ..plantSlump = '175'
      ..inSituSlump = '170'
      ..plantTemp = '21.3'
      ..inSituTemp = '21.4'
      ..samples = '841-846';
    // (reference, value, isText)
    final cells = <(String, String, bool)>[
      ('A11', 'DRAWING NO: DRG-1', true),
      ('A12', 'REPORT (RFI) NUMBER: AZE-SUN-INT-QMS-RFI-CIV-1460', true),
      ('A13', 'LOCATION: North site area / N-5', true),
      ('BB14', 'CONCRETE POURING DATE:06.10.2026', true),
      ('A19', 'N-5', true),
      ('H19', 'C-30/37 XA2', true),
      ('O19', 'HOLCIM', true),
      ('V19', '66817', true),
      ('AC19', 'TRK-1', true),
      ('CS19', '841-846', true),
    ];
    for (final reference in [
      'AI',
      'AP',
      'AW',
      'BD',
      'BK',
      'BR',
      'BY',
      'CC',
      'CF',
      'CJ',
      'CM',
    ]) {
      cells.add(('${reference}19', '1', false));
      cells.add(('${reference}21', '2', false));
    }
    for (final reference in ['A', 'H', 'O', 'V', 'AC', 'CS']) {
      cells.add(('${reference}21', '', true));
    }
    String renderRow(int row) {
      final out = StringBuffer();
      for (final (reference, value, text) in cells) {
        final index = reference.lastIndexOf(RegExp(r'[^0-9]'));
        if (int.parse(reference.substring(index + 1)) != row) continue;
        out.write(
          text
              ? '<c r="$reference" t="inlineStr"><is><t>$value</t></is></c>'
              : '<c r="$reference"><v>$value</v></c>',
        );
      }
      return out.toString();
    }

    final worksheet =
        '<worksheet><dimension ref="A1:CS22"/><sheetData><row r="11">${renderRow(11)}</row><row r="12">${renderRow(12)}</row><row r="13">${renderRow(13)}</row><row r="14">${renderRow(14)}</row><row r="19">${renderRow(19)}</row><row r="20"></row><row r="21">${renderRow(21)}</row><row r="22"><c r="AI22"><v>3</v></c></row></sheetData></worksheet>';
    final xlsx = _package([('xl/worksheets/sheet1.xml', worksheet)]);
    final output = generateConcreteDocuments(
      documentType: DocumentType.civ010,
      template: xlsx,
      records: [source],
      ambientRange: (0, 0),
      rngSeed: 2,
    );
    final sheet = _member(output[0].bytes, 'xl/worksheets/sheet1.xml');
    expect(sheet, contains('CIV-2010'));
    expect(sheet, contains('N-5'));
    expect(sheet, contains('06.10.2026'));
    String cell(String reference) => _cellInner(sheet, reference)!;
    expect(cell('A12'), contains('CIV-2010'));
    expect(cell('H19'), contains('C-30/37 XA2'));
    expect(cell('AI19'), contains('<v>5</v>'));
    expect(cell('AP19'), contains('<v>15</v>'));
    expect(cell('CF19'), contains('<v>21.3</v>'));
    final ambientValue = double.parse(
      RegExp(r'<v>([^<]+)</v>').firstMatch(cell('BD19'))![1]!,
    );
    expect(ambientValue, inInclusiveRange(20.6, 20.8));
    expect(cell('AI21'), isEmpty);

    final multi = _record('AZE-SUN-INT-QMS-RFI-CIV-2010', 'N-5')
      ..plantTemp = '21.3';
    multi.additionalBatches.add(
      const ConcreteBatch(
        tag: 'N-5',
        concreteClass: 'C-30/37 XA2',
        supplier: 'HOLCIM',
        batchTicket: '66818',
        truck: 'TRK-2',
        quantity: '6',
        totalQuantity: '15',
        batchTime: '10:00',
        startTime: '10:00',
        endTime: '11:00',
        plantSlump: '175',
        inSituSlump: '170',
        plantTemp: '21.3',
        inSituTemp: '21.4',
        samples: '847-852',
      ),
    );
    final third = ConcreteBatch(
      tag: 'N-5',
      concreteClass: 'C-30/37 XA2',
      supplier: 'HOLCIM',
      batchTicket: '66819',
      truck: 'TRK-2',
      quantity: '7',
      totalQuantity: '15',
      batchTime: '10:00',
      startTime: '10:00',
      endTime: '11:00',
      plantSlump: '175',
      inSituSlump: '170',
      plantTemp: '21.3',
      inSituTemp: '21.4',
      samples: '853-858',
    );
    multi.additionalBatches.add(third);
    final multiOutput = generateConcreteDocuments(
      documentType: DocumentType.civ010,
      template: xlsx,
      records: [multi],
      ambientRange: (0, 0),
      rngSeed: 2,
    );
    final multiSheet = _member(
      multiOutput[0].bytes,
      'xl/worksheets/sheet1.xml',
    );
    String secondCell(String reference) => _cellInner(multiSheet, reference)!;
    expect(secondCell('AI21'), contains('<v>6</v>'));
    expect(secondCell('V21'), contains('66818'));
    expect(secondCell('CS21'), contains('847-852'));
    expect(secondCell('AI23'), contains('<v>7</v>'));
    expect(secondCell('V23'), contains('66819'));
    expect(secondCell('CS23'), contains('853-858'));
    expect(secondCell('AI24'), contains('<v>3</v>'));
    expect(multiSheet, contains('<dimension ref="A1:CS24"/>'));

    final docx = _package([
      (
        'word/document.xml',
        '<w:document><w:p><w:t>REPORT NO.</w:t><w:t>AZE-SUN-INT-QMS-RFI-CIV-1404</w:t></w:p><w:p><w:t>LOCATION</w:t><w:t>(North Site) N5</w:t></w:p></w:document>',
      ),
    ]);
    final civ14 = generateConcreteDocuments(
      documentType: DocumentType.civ014,
      template: docx,
      records: [_record('AZE-SUN-INT-QMS-RFI-CIV-2014', 'South Site')],
      ambientRange: (0, 0),
      rngSeed: 2,
    );
    final document = _member(civ14[0].bytes, 'word/document.xml');
    expect(document, contains('CIV-2014'));
    expect(document, contains('South Site'));
  });
}
