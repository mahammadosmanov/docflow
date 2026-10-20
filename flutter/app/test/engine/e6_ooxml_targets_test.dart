// Port of the final relationship-target, class and count tests in
// src-tauri/src/ooxml.rs (mod tests).
import 'dart:convert';
import 'dart:typed_data';

import 'package:docflow/engine/ooxml.dart';
import 'package:flutter_test/flutter_test.dart';

import 'e6_ooxml_fixtures.dart';

Uint8List _b(String text) => Uint8List.fromList(utf8.encode(text));

const String _officeRel =
    'http://schemas.openxmlformats.org/officeDocument/2006/relationships';

const String _workbookSheet1 =
    '<workbook xmlns="$mainNs" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>';
const String _rootSheet1 =
    '<Relationships><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/>';

void main() {
  test('rejects duplicate worksheet relationship target', () {
    const worksheetRelationships =
        '<Relationships xmlns="$packageRelNs"><Relationship Id="rIdQuery1" Type="$_officeRel/queryTable" Target="../queryTables/queryTable1.xml"/><Relationship Id="rIdQuery2" Type="$_officeRel/queryTable" Target="../queryTables/queryTable1.xml"/></Relationships>';
    expect(
      () => orderedWorkbookDiscovery(fixtureFromMembers(
        _workbookSheet1,
        '$_rootSheet1</Relationships>',
        [
          ('xl/worksheets/sheet1.xml', _b('worksheet')),
          ('xl/worksheets/_rels/sheet1.xml.rels', _b(worksheetRelationships)),
        ],
      )),
      throwsA(ooxmlError('duplicate worksheet relationship target')),
    );
  });

  test('rejects missing worksheet relationship target', () {
    const worksheetRelationships =
        '<Relationships xmlns="$packageRelNs"><Relationship Id="rIdQuery" Type="$_officeRel/queryTable"/></Relationships>';
    expect(
      () => orderedWorkbookDiscovery(fixtureFromMembers(
        _workbookSheet1,
        '$_rootSheet1</Relationships>',
        [
          ('xl/worksheets/sheet1.xml', _b('worksheet')),
          ('xl/worksheets/_rels/sheet1.xml.rels', _b(worksheetRelationships)),
        ],
      )),
      throwsA(ooxmlError('worksheet relationship target is missing')),
    );
  });

  test('rejects encoded forbidden worksheet relationship target', () {
    const worksheetRelationships =
        '<Relationships xmlns="$packageRelNs"><Relationship Id="rIdQuery" Type="$_officeRel/queryTable" Target="../queryTables/queryTable1.xml%3Fignored"/></Relationships>';
    expect(
      () => orderedWorkbookDiscovery(fixtureFromMembers(
        _workbookSheet1,
        '$_rootSheet1</Relationships>',
        [
          ('xl/worksheets/sheet1.xml', _b('worksheet')),
          ('xl/worksheets/_rels/sheet1.xml.rels', _b(worksheetRelationships)),
        ],
      )),
      throwsA(ooxmlError('worksheet relationship escapes the package')),
    );
  });

  test('rejects unknown workbook relationship class', () {
    const rels =
        '$_rootSheet1<Relationship Id="rIdMystery" Type="$_officeRel/mysteryData" Target="mystery.bin"/></Relationships>';
    expect(
      () => orderedWorkbookDiscovery(fixtureFromMembers(
        _workbookSheet1,
        rels,
        [
          ('xl/worksheets/sheet1.xml', _b('worksheet')),
          ('xl/mystery.bin', _b('unknown data')),
        ],
      )),
      throwsA(ooxmlError('unknown workbook relationship class')),
    );
  });

  test('accepts volatile dependencies and modern office relationships', () {
    const rels = '''<Relationships>
                <Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/>
                <Relationship Id="rIdVolatile" Type="$_officeRel/volatileDependencies" Target="volatileDependencies.xml"/>
                <Relationship Id="rIdCalc" Type="http://schemas.microsoft.com/office/2020/07/relationships/calcFeatures" Target="calcFeatures.xml"/>
                <Relationship Id="rIdCustomUI" Type="http://schemas.microsoft.com/office/2006/relationships/ui/customUI" Target="customUI/customUI.xml"/>
            </Relationships>''';
    final discovery = orderedWorkbookDiscovery(fixtureFromMembers(
      _workbookSheet1,
      rels,
      [
        ('xl/worksheets/sheet1.xml', _b('worksheet')),
        ('xl/volatileDependencies.xml', _b('<volTypes/>')),
        ('xl/calcFeatures.xml', _b('<calcFeatures/>')),
        ('xl/customUI/customUI.xml', _b('<customUI/>')),
      ],
    ));
    expect(discovery.worksheets.length, 1);
    expect(discovery.worksheets[0].name, 'MEC');
  });

  test('rejects spoofed workbook relationship document namespace', () {
    const rels =
        '<Relationships xmlns="urn:spoof"><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/></Relationships>';
    expect(
      () => orderedWorkbookDiscovery(fixtureFromMembers(
        _workbookSheet1,
        rels,
        [('xl/worksheets/sheet1.xml', _b('worksheet'))],
      )),
      throwsA(ooxmlError('workbook relationships namespace is invalid')),
    );
  });

  test('ignores foreign workbook relationship elements', () {
    const rels =
        '<Relationships xmlns="$packageRelNs" xmlns:ext="urn:extension"><ext:Relationship Id="rId1" Type="http://schemas.microsoft.com/office/2006/relationships/xlMacrosheet" Target="worksheets/decoy.xml" TargetMode="External"/><ext:Relationship Id="rIdConnections" Type="$_officeRel/connections" Target="missing.xml"/><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/></Relationships>';
    final discovery = orderedWorkbookDiscovery(fixtureFromMembers(
      _workbookSheet1,
      rels,
      [
        ('xl/worksheets/decoy.xml', _b('decoy')),
        ('xl/worksheets/sheet1.xml', _b('worksheet')),
      ],
    ));
    expect(discovery.worksheets.length, 1);
    expect(discovery.worksheets[0].partPath, 'xl/worksheets/sheet1.xml');
    expect(discovery.safety.hasXlmMacroSheet, isFalse);
  });

  test('rejects malformed connection count', () {
    const rels =
        '$_rootSheet1<Relationship Id="rIdConnections" Type="$_officeRel/connections" Target="connections.xml"/></Relationships>';
    expect(
      () => orderedWorkbookDiscovery(fixtureFromMembers(
        _workbookSheet1,
        rels,
        [
          ('xl/worksheets/sheet1.xml', _b('worksheet')),
          (
            'xl/connections.xml',
            _b('<connections count="many"><connection id="1"/></connections>'),
          ),
        ],
      )),
      throwsA(ooxmlError('relationship count is invalid')),
    );
  });

  test('rejects oversized sheet attribute', () {
    final name = 'N' * (1024 + 1);
    final workbook =
        '<workbook xmlns="$mainNs" xmlns:r="urn:r"><sheets><sheet name="$name" sheetId="1" r:id="rId1"/></sheets></workbook>';
    expect(
      () => orderedWorkbookDiscovery(fixtureFromMembers(
        workbook,
        '$_rootSheet1</Relationships>',
        [('xl/worksheets/sheet1.xml', _b('worksheet'))],
      )),
      throwsA(ooxmlError('workbook sheet name exceeded the size limit')),
    );
  });
}
