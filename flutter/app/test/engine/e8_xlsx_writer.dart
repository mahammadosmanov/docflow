// Writes real .xlsx packages from the e8 sheet specs so the production OOXML
// reader (not the JSON FakeOoxml) parses them. Test-only.
import 'dart:typed_data';

import 'package:archive/archive.dart';

import 'e8_support.dart';

const _mainNs = 'http://schemas.openxmlformats.org/spreadsheetml/2006/main';
const _relNs =
    'http://schemas.openxmlformats.org/officeDocument/2006/relationships';
const _pkgRelNs =
    'http://schemas.openxmlformats.org/package/2006/relationships';

String _esc(String value) => value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');

String _sheetXml(SheetSpec sheet) {
  final buffer = StringBuffer(
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    '<worksheet xmlns="$_mainNs" xmlns:r="$_relNs"><sheetData>',
  );
  for (final row in sheet.rows) {
    buffer.write('<row r="${row.number}">');
    for (final cell in row.cells) {
      if (cell.value.isEmpty && cell.formula == null) {
        buffer.write('<c r="${cell.reference}"/>');
      } else {
        buffer.write(
          '<c r="${cell.reference}" t="inlineStr"><is><t>${_esc(cell.value)}</t></is></c>',
        );
      }
    }
    buffer.write('</row>');
  }
  buffer.write('</sheetData>');
  if (sheet.merges.isNotEmpty) {
    buffer.write('<mergeCells count="${sheet.merges.length}">');
    for (final merge in sheet.merges) {
      buffer.write('<mergeCell ref="$merge"/>');
    }
    buffer.write('</mergeCells>');
  }
  buffer.write('</worksheet>');
  return buffer.toString();
}

/// Packs [sheets] (name, spec) into a standard .xlsx zip.
Uint8List xlsxPackage(List<(String, SheetSpec)> sheets) {
  final archive = Archive();
  void add(String name, String text) {
    final bytes = Uint8List.fromList(text.codeUnits);
    archive.addFile(ArchiveFile(name, bytes.length, bytes));
  }

  final overrides = StringBuffer();
  final sheetEntries = StringBuffer();
  final relEntries = StringBuffer();
  for (var index = 0; index < sheets.length; index++) {
    final number = index + 1;
    add('xl/worksheets/sheet$number.xml', _sheetXml(sheets[index].$2));
    overrides.write(
      '<Override PartName="/xl/worksheets/sheet$number.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>',
    );
    sheetEntries.write(
      '<sheet name="${_esc(sheets[index].$1)}" sheetId="$number"'
      '${sheets[index].$2.state == null ? '' : ' state="${_esc(sheets[index].$2.state!)}"'}'
      ' r:id="rId$number"/>',
    );
    relEntries.write(
      '<Relationship Id="rId$number" Type="$_relNs/worksheet" Target="worksheets/sheet$number.xml"/>',
    );
  }
  add(
    '[Content_Types].xml',
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        '<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>'
        '$overrides</Types>',
  );
  add(
    '_rels/.rels',
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Relationships xmlns="$_pkgRelNs"><Relationship Id="rId1" Type="$_relNs/officeDocument" Target="xl/workbook.xml"/></Relationships>',
  );
  add(
    'xl/workbook.xml',
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<workbook xmlns="$_mainNs" xmlns:r="$_relNs"><sheets>$sheetEntries</sheets></workbook>',
  );
  add(
    'xl/_rels/workbook.xml.rels',
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Relationships xmlns="$_pkgRelNs">$relEntries</Relationships>',
  );
  return Uint8List.fromList(ZipEncoder().encode(archive)!);
}

/// Real source package: CIV, MEC, ELE, PRC (matches `sourcePackage`).
Uint8List xlsxSourcePackage(SheetSpec civ, SheetSpec mec) => xlsxPackage([
  ('CIV', civ),
  ('MEC', mec),
  ('ELE', worksheet(const [])),
  ('PRC', worksheet(const [])),
]);

/// Real Follow Up workbook (matches `targetWithRows`, with its header row).
Uint8List xlsxTargetWithRows(
  List<RowSpec> rows, [
  List<String> merges = const [],
]) => xlsxPackage([
  (
    'Follow Up',
    worksheet([
      row(1, [inline('A1', 'RFI NO.'), inline('B1', 'STATUS')]),
      ...rows,
    ], merges),
  ),
]);
