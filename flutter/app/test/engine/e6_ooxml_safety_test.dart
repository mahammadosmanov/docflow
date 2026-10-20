// Port of the VBA, macro, protection, connection, query-table and worksheet
// relationship tests in src-tauri/src/ooxml.rs (mod tests).
import 'dart:convert';
import 'dart:typed_data';

import 'package:docflow/engine/ooxml.dart';
import 'package:flutter_test/flutter_test.dart';

import 'e6_ooxml_fixtures.dart';

Uint8List _b(String text) => Uint8List.fromList(utf8.encode(text));

const String _officeRel =
    'http://schemas.openxmlformats.org/officeDocument/2006/relationships';
const String _msRel = 'http://schemas.microsoft.com/office/2006/relationships';

const String _workbookSheet1 =
    '<workbook xmlns="$mainNs" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>';
const String _rootSheet1 =
    '<Relationships><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/>';

void main() {
  group('vba and macro safety facts', () {
    test('reports vba project', () {
      const rels =
          '$_rootSheet1<Relationship Id="rIdVba" Type="$_msRel/vbaProject" Target="vbaProject.bin"/></Relationships>';
      final discovery = orderedWorkbookDiscovery(fixtureFromMembers(
        '<workbook xmlns="$mainNs" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>',
        rels,
        [
          ('xl/worksheets/sheet1.xml', _b('not worksheet XML')),
          ('xl/vbaProject.bin', _b('synthetic VBA bytes')),
        ],
      ));
      expect(discovery.safety.hasVbaProject, isTrue);
    });

    test('vba relationship reports noncanonical target', () {
      const rels =
          '$_rootSheet1<Relationship Id="rIdVba" Type="$_msRel/vbaProject" Target="macros/project.bin"/></Relationships>';
      final discovery = orderedWorkbookDiscovery(fixtureFromMembers(
        _workbookSheet1,
        rels,
        [
          ('xl/worksheets/sheet1.xml', _b('worksheet')),
          ('xl/macros/project.bin', _b('synthetic VBA bytes')),
        ],
      ));
      expect(discovery.safety.hasVbaProject, isTrue);
    });

    test('vba relationship rejects missing target member', () {
      const rels =
          '$_rootSheet1<Relationship Id="rIdVba" Type="$_msRel/vbaProject" Target="macros/project.bin"/></Relationships>';
      expect(
        () => orderedWorkbookDiscovery(fixtureFromMembers(
          _workbookSheet1,
          rels,
          [('xl/worksheets/sheet1.xml', _b('worksheet'))],
        )),
        throwsA(ooxmlError('VBA project package member is missing')),
      );
    });

    test('vba relationship retains canonical orphan signal', () {
      const rels =
          '<Relationships><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/></Relationships>';
      final discovery = orderedWorkbookDiscovery(fixtureFromMembers(
        _workbookSheet1,
        rels,
        [
          ('xl/worksheets/sheet1.xml', _b('worksheet')),
          ('xl/vbaProject.bin', _b('orphan VBA bytes')),
        ],
      ));
      expect(discovery.safety.hasVbaProject, isTrue);
    });

    test('rejects non internal auxiliary workbook target modes', () {
      for (final (targetMode, expected) in <(String, String)>[
        ('External', 'external workbook relationship target is not allowed'),
        ('Bogus', 'workbook relationship target mode is invalid'),
      ]) {
        final rels =
            '$_rootSheet1<Relationship Id="rIdVba" Type="$_msRel/vbaProject" Target="vbaProject.bin" TargetMode="$targetMode"/></Relationships>';
        expect(
          () => orderedWorkbookDiscovery(fixtureFromMembers(
            _workbookSheet1,
            rels,
            [
              ('xl/worksheets/sheet1.xml', _b('worksheet')),
              ('xl/vbaProject.bin', _b('VBA')),
            ],
          )),
          throwsA(ooxmlError(expected)),
        );
      }
    });

    test('rejects encoded forbidden auxiliary workbook target', () {
      const rels =
          '$_rootSheet1<Relationship Id="rIdVba" Type="$_msRel/vbaProject" Target="vbaProject.bin%3Fignored"/></Relationships>';
      expect(
        () => orderedWorkbookDiscovery(fixtureFromMembers(
          _workbookSheet1,
          rels,
          [
            ('xl/worksheets/sheet1.xml', _b('worksheet')),
            ('xl/vbaProject.bin', _b('VBA')),
          ],
        )),
        throwsA(ooxmlError('worksheet relationship escapes the package')),
      );
    });

    test('reports workbook protection', () {
      const workbook =
          '<workbook xmlns="$mainNs" xmlns:r="urn:r"><workbookProtection lockStructure="1"/><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>';
      const rels =
          '<Relationships><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/></Relationships>';
      final discovery = orderedWorkbookDiscovery(fixtureFromMembers(
        workbook,
        rels,
        [('xl/worksheets/sheet1.xml', _b('worksheet'))],
      ));
      expect(discovery.safety.hasWorkbookProtection, isTrue);
    });

    test('reports xlm macro sheet relationship', () {
      const rels =
          '$_rootSheet1<Relationship Id="rIdMacro" Type="$_msRel/xlMacrosheet" Target="macrosheets/sheet2.xml"/></Relationships>';
      final discovery = orderedWorkbookDiscovery(fixtureFromMembers(
        _workbookSheet1,
        rels,
        [
          ('xl/worksheets/sheet1.xml', _b('worksheet cells are not read')),
          ('xl/macrosheets/sheet2.xml', _b('macro sheet cells are not read')),
        ],
      ));
      expect(discovery.safety.hasXlmMacroSheet, isTrue);
    });
  });

  group('external links, connections and query tables', () {
    test('counts external links', () {
      const rels =
          '$_rootSheet1<Relationship Id="rIdLink1" Type="$_officeRel/externalLink" Target="externalLinks/externalLink1.xml"/><Relationship Id="rIdLink2" Type="$_officeRel/externalLink" Target="externalLinks/externalLink2.xml"/></Relationships>';
      final discovery = orderedWorkbookDiscovery(fixtureFromMembers(
        _workbookSheet1,
        rels,
        [
          ('xl/worksheets/sheet1.xml', _b('worksheet')),
          ('xl/externalLinks/externalLink1.xml', _b('link one')),
          ('xl/externalLinks/externalLink2.xml', _b('link two')),
        ],
      ));
      expect(discovery.safety.externalLinkCount, 2);
    });

    test('counts explicit internal connections', () {
      const rels =
          '$_rootSheet1<Relationship Id="rIdConnections" Type="$_officeRel/connections" Target="connections.xml" TargetMode="Internal"/></Relationships>';
      final discovery = orderedWorkbookDiscovery(fixtureFromMembers(
        _workbookSheet1,
        rels,
        [
          ('xl/worksheets/sheet1.xml', _b('worksheet')),
          (
            'xl/connections.xml',
            _b('<connections xmlns="$mainNs" count="2"><connection id="1"/><connection id="2"/></connections>'),
          ),
        ],
      ));
      expect(discovery.safety.connectionCount, 2);
    });

    test('uses canonical connection expanded names', () {
      const rels =
          '$_rootSheet1<Relationship Id="rIdConnections" Type="$_officeRel/connections" Target="connections.xml"/></Relationships>';
      final connections =
          '<connections xmlns="$mainNs" xmlns:x="urn:foreign" count="1" x:count="99"><connection id="1"/><x:connection id="2"/></connections>';
      final discovery = orderedWorkbookDiscovery(fixtureFromMembers(
        _workbookSheet1,
        rels,
        [
          ('xl/worksheets/sheet1.xml', _b('worksheet')),
          ('xl/connections.xml', _b(connections)),
        ],
      ));
      expect(discovery.safety.connectionCount, 1);

      const foreign =
          '<x:connections xmlns:x="urn:foreign"><x:connection id="1"/></x:connections>';
      expect(
        () => orderedWorkbookDiscovery(fixtureFromMembers(
          _workbookSheet1,
          rels,
          [
            ('xl/worksheets/sheet1.xml', _b('worksheet')),
            ('xl/connections.xml', _b(foreign)),
          ],
        )),
        throwsA(ooxmlError('relationship count is invalid')),
      );
    });

    test('counts explicit internal query tables without reading cells', () {
      const rels =
          '$_rootSheet1</Relationships>';
      const worksheetRelationships =
          '<Relationships xmlns="$packageRelNs"><Relationship Id="rIdQuery1" Type="$_officeRel/queryTable" Target="../queryTables/queryTable1.xml" TargetMode="Internal"/><Relationship Id="rIdQuery2" Type="$_officeRel/queryTable" Target="../queryTables/queryTable2.xml" TargetMode="Internal"/></Relationships>';
      final discovery = orderedWorkbookDiscovery(fixtureFromMembers(
        _workbookSheet1,
        rels,
        [
          (
            'xl/worksheets/sheet1.xml',
            _b('deliberately malformed worksheet XML with secret cell text'),
          ),
          ('xl/worksheets/_rels/sheet1.xml.rels', _b(worksheetRelationships)),
          ('xl/queryTables/queryTable1.xml', _b('query one')),
          ('xl/queryTables/queryTable2.xml', _b('query two')),
        ],
      ));
      expect(discovery.safety.queryTableCount, 2);
    });

    test('rejects unknown worksheet relationship class', () {
      const rels =
          '$_rootSheet1</Relationships>';
      const worksheetRelationships =
          '<Relationships xmlns="$packageRelNs"><Relationship Id="rIdMystery" Type="$_officeRel/mysteryData" Target="../mystery.bin"/></Relationships>';
      expect(
        () => orderedWorkbookDiscovery(fixtureFromMembers(
          _workbookSheet1,
          rels,
          [
            ('xl/worksheets/sheet1.xml', _b('worksheet')),
            ('xl/worksheets/_rels/sheet1.xml.rels', _b(worksheetRelationships)),
            ('xl/mystery.bin', _b('unknown data')),
          ],
        )),
        throwsA(ooxmlError('unknown worksheet relationship class')),
      );
    });

    test('rejects external worksheet relationship target', () {
      const rels = '$_rootSheet1</Relationships>';
      const worksheetRelationships =
          '<Relationships xmlns="$packageRelNs"><Relationship Id="rIdQuery" Type="$_officeRel/queryTable" Target="https://example.invalid/query" TargetMode="External"/></Relationships>';
      expect(
        () => orderedWorkbookDiscovery(fixtureFromMembers(
          _workbookSheet1,
          rels,
          [
            ('xl/worksheets/sheet1.xml', _b('worksheet')),
            ('xl/worksheets/_rels/sheet1.xml.rels', _b(worksheetRelationships)),
          ],
        )),
        throwsA(ooxmlError(
          'external worksheet relationship target is not allowed',
        )),
      );
    });

    test('rejects spoofed worksheet relationship namespace', () {
      const rels = '$_rootSheet1</Relationships>';
      const worksheetRelationships =
          '<Relationships xmlns="urn:spoof"><Relationship Id="rIdQuery" Type="$_officeRel/queryTable" Target="../queryTables/queryTable1.xml"/></Relationships>';
      expect(
        () => orderedWorkbookDiscovery(fixtureFromMembers(
          _workbookSheet1,
          rels,
          [
            ('xl/worksheets/sheet1.xml', _b('worksheet')),
            ('xl/worksheets/_rels/sheet1.xml.rels', _b(worksheetRelationships)),
          ],
        )),
        throwsA(ooxmlError('worksheet relationships namespace is invalid')),
      );
    });

    test('ignores foreign worksheet relationship elements', () {
      const rels = '$_rootSheet1</Relationships>';
      const worksheetRelationships =
          '<Relationships xmlns="$packageRelNs" xmlns:ext="urn:extension"><ext:Relationship Id="rIdQuery" Type="$_officeRel/queryTable" Target="../queryTables/queryTable1.xml"/></Relationships>';
      final discovery = orderedWorkbookDiscovery(fixtureFromMembers(
        _workbookSheet1,
        rels,
        [
          ('xl/worksheets/sheet1.xml', _b('worksheet')),
          ('xl/worksheets/_rels/sheet1.xml.rels', _b(worksheetRelationships)),
        ],
      ));
      expect(discovery.safety.queryTableCount, 0);
    });

    test('rejects duplicate worksheet relationship identity', () {
      const rels = '$_rootSheet1</Relationships>';
      const worksheetRelationships =
          '<Relationships xmlns="$packageRelNs"><Relationship Id="rIdQuery" Type="$_officeRel/queryTable" Target="../queryTables/queryTable1.xml"/><Relationship Id="rIdQuery" Type="$_officeRel/queryTable" Target="../queryTables/queryTable2.xml"/></Relationships>';
      expect(
        () => orderedWorkbookDiscovery(fixtureFromMembers(
          _workbookSheet1,
          rels,
          [
            ('xl/worksheets/sheet1.xml', _b('worksheet')),
            ('xl/worksheets/_rels/sheet1.xml.rels', _b(worksheetRelationships)),
          ],
        )),
        throwsA(ooxmlError('duplicate worksheet relationship identity')),
      );
    });
  });
}
