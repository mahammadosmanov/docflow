// Port of the discovery rejection / acceptance tests in
// src-tauri/src/ooxml.rs (mod tests), second group.
import 'dart:convert';
import 'dart:typed_data';

import 'package:docflow/engine/ooxml.dart';
import 'package:flutter_test/flutter_test.dart';

import 'e6_ooxml_fixtures.dart';

Uint8List _b(String text) => Uint8List.fromList(utf8.encode(text));

const String _officeRel =
    'http://schemas.openxmlformats.org/officeDocument/2006/relationships';

Matcher _msg(String message) => ooxmlError(message);

void main() {
  group('namespace and attribute authority', () {
    test('rejects sheet id from foreign namespace', () {
      const workbook =
          '<workbook xmlns="$mainNs" xmlns:r="$_officeRel" xmlns:evil="urn:evil"><sheets><sheet name="MEC" sheetId="1" evil:id="rId1"/></sheets></workbook>';
      const relationships =
          '<Relationships xmlns="$packageRelNs"><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/></Relationships>';
      expect(
        () => orderedWorkbookDiscovery(fixtureFromMembers(
          workbook,
          relationships,
          [('xl/worksheets/sheet1.xml', _b('worksheet'))],
        )),
        throwsA(_msg('workbook sheet relationship is missing')),
      );
    });

    test('rejects spoofed sheet attribute namespace', () {
      const workbook =
          '<workbook xmlns="$mainNs" xmlns:r="urn:spoof"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>';
      const relationships =
          '<Relationships xmlns="$packageRelNs"><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/></Relationships>';
      expect(
        () => orderedWorkbookDiscovery(fixtureFromMembers(
          workbook,
          relationships,
          [('xl/worksheets/sheet1.xml', _b('worksheet'))],
        )),
        throwsA(_msg('workbook relationship namespace is invalid')),
      );
    });

    test('rejects spoofed workbook namespace', () {
      const workbook =
          '<workbook xmlns="urn:spoof" xmlns:r="$_officeRel"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>';
      const relationships =
          '<Relationships xmlns="$packageRelNs"><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/></Relationships>';
      expect(
        () => orderedWorkbookDiscovery(fixtureFromMembers(
          workbook,
          relationships,
          [('xl/worksheets/sheet1.xml', _b('worksheet'))],
        )),
        throwsA(_msg('workbook namespace is invalid')),
      );
    });

    test('expanded names rejects evil sheets namespace', () {
      const workbook =
          '<workbook xmlns="$mainNs" xmlns:r="urn:r"><sheets xmlns="urn:evil"><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>';
      const relationships =
          '<Relationships><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/></Relationships>';
      expect(
        () => orderedWorkbookDiscovery(fixtureFromMembers(
          workbook,
          relationships,
          [('xl/worksheets/sheet1.xml', _b('worksheet'))],
        )),
        throwsA(_msg('workbook sheets container is invalid')),
      );
    });

    test('expanded names rejects shadowed relationship child', () {
      const workbook =
          '<workbook xmlns="$mainNs" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>';
      const relationships =
          '<Relationships xmlns="$packageRelNs"><Relationship xmlns="urn:evil" Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/></Relationships>';
      expect(
        () => orderedWorkbookDiscovery(fixtureFromMembers(
          workbook,
          relationships,
          [('xl/worksheets/sheet1.xml', _b('worksheet'))],
        )),
        throwsA(_msg('workbook sheet target is missing')),
      );
    });

    test('rejects duplicate raw default namespace declarations', () {
      const canonicalWorkbook =
          '<workbook xmlns="$mainNs" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>';
      const canonicalRelationships =
          '<Relationships xmlns="$packageRelNs"><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/></Relationships>';
      final cases = <(String, String)>[
        (
          '<workbook xmlns="urn:foreign" xmlns="$mainNs" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>',
          canonicalRelationships,
        ),
        (
          '<workbook xmlns="$mainNs" xmlns:r="urn:r"><sheets xmlns="urn:foreign" xmlns="$mainNs"><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>',
          canonicalRelationships,
        ),
        (
          '<workbook xmlns="$mainNs" xmlns:r="urn:r"><sheets><sheet xmlns="urn:foreign" xmlns="$mainNs" name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>',
          canonicalRelationships,
        ),
        (
          canonicalWorkbook,
          '<Relationships xmlns="urn:foreign" xmlns="$packageRelNs"><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/></Relationships>',
        ),
        (
          canonicalWorkbook,
          '<Relationships xmlns="$packageRelNs"><Relationship xmlns="urn:foreign" xmlns="$packageRelNs" Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/></Relationships>',
        ),
      ];
      for (final (workbook, relationships) in cases) {
        expect(
          () => orderedWorkbookDiscovery(fixtureFromMembers(
            workbook,
            relationships,
            [('xl/worksheets/sheet1.xml', _b('worksheet'))],
          )),
          throwsA(_msg('XML attribute is invalid')),
        );
      }
    });

    test('rejects duplicate raw prefixed namespace declarations', () {
      const officeNs =
          'http://schemas.openxmlformats.org/officeDocument/2006/relationships';
      const canonicalWorkbook =
          '<workbook xmlns="$mainNs" xmlns:r="$officeNs"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>';
      const canonicalRelationships =
          '<Relationships xmlns="$packageRelNs"><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/></Relationships>';
      final cases = <(String, String)>[
        (
          '<workbook xmlns="$mainNs" xmlns:r="urn:foreign" xmlns:r="$officeNs"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>',
          canonicalRelationships,
        ),
        (
          '<workbook xmlns="$mainNs"><sheets xmlns:r="urn:foreign" xmlns:r="$officeNs"><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>',
          canonicalRelationships,
        ),
        (
          '<workbook xmlns="$mainNs"><sheets><sheet xmlns:r="urn:foreign" xmlns:r="$officeNs" name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>',
          canonicalRelationships,
        ),
        (
          canonicalWorkbook,
          '<Relationships xmlns="$packageRelNs" xmlns:r="urn:foreign" xmlns:r="$officeNs"><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/></Relationships>',
        ),
        (
          canonicalWorkbook,
          '<Relationships xmlns="$packageRelNs"><Relationship xmlns:r="urn:foreign" xmlns:r="$officeNs" Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/></Relationships>',
        ),
      ];
      for (final (workbook, relationships) in cases) {
        expect(
          () => orderedWorkbookDiscovery(fixtureFromMembers(
            workbook,
            relationships,
            [('xl/worksheets/sheet1.xml', _b('worksheet'))],
          )),
          throwsA(_msg('XML attribute is invalid')),
        );
      }
    });

    test('expanded names require unqualified attributes', () {
      const relationships =
          '<Relationships><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/></Relationships>';
      for (final (attribute, expected) in <(String, String)>[
        ('evil:name="MEC" sheetId="1"', 'workbook sheet name is missing'),
        ('name="MEC" evil:sheetId="1"', 'workbook sheet identity is invalid'),
      ]) {
        final workbook =
            '<workbook xmlns="$mainNs" xmlns:r="urn:r" xmlns:evil="urn:evil"><sheets><sheet $attribute r:id="rId1"/></sheets></workbook>';
        expect(
          () => orderedWorkbookDiscovery(fixtureFromMembers(
            workbook,
            relationships,
            [('xl/worksheets/sheet1.xml', _b('worksheet'))],
          )),
          throwsA(_msg(expected)),
        );
      }

      for (final (attribute, expected) in <(String, String)>[
        (
          'evil:Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"',
          'workbook relationship identity is missing',
        ),
        (
          'Id="rId1" evil:Type="urn:evil" Target="worksheets/sheet1.xml"',
          'workbook sheet relationship type is missing',
        ),
        (
          'Id="rId1" Type="$relNs" evil:Target="worksheets/sheet1.xml"',
          'workbook relationship target is missing',
        ),
      ]) {
        const workbook =
            '<workbook xmlns="$mainNs" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>';
        final rels =
            '<Relationships xmlns:evil="urn:evil"><Relationship $attribute/></Relationships>';
        expect(
          () => orderedWorkbookDiscovery(fixtureFromMembers(
            workbook,
            rels,
            [('xl/worksheets/sheet1.xml', _b('worksheet'))],
          )),
          throwsA(_msg(expected)),
        );
      }

      const workbook =
          '<workbook xmlns="$mainNs" xmlns:r="urn:r" xmlns:evil="urn:evil"><sheets><sheet name="MEC" sheetId="1" evil:state="hidden" r:id="rId1"/></sheets></workbook>';
      const rels =
          '<Relationships xmlns:evil="urn:evil"><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml" evil:TargetMode="External"/></Relationships>';
      final discovery = orderedWorkbookDiscovery(fixtureFromMembers(
        workbook,
        rels,
        [('xl/worksheets/sheet1.xml', _b('worksheet'))],
      ));
      expect(discovery.worksheets[0].visibility, WorksheetVisibility.visible);
    });

    test('expanded names accepts alternate valid prefixes', () {
      final cases = <(String, String, String, String)>[
        (
          mainNs,
          'http://schemas.openxmlformats.org/officeDocument/2006/relationships',
          'http://schemas.openxmlformats.org/package/2006/relationships',
          relNs,
        ),
        (
          'http://purl.oclc.org/ooxml/spreadsheetml/main',
          'http://purl.oclc.org/ooxml/officeDocument/relationships',
          'http://purl.oclc.org/ooxml/package/relationships',
          'http://purl.oclc.org/ooxml/officeDocument/relationships/worksheet',
        ),
      ];
      for (final (workbookNs, officeNs, packageNs, relType) in cases) {
        final workbook =
            '<ss:workbook xmlns:ss="$workbookNs" xmlns:od="$officeNs"><ss:sheets><ss:sheet name="MEC" sheetId="1" od:id="rId1"/></ss:sheets></ss:workbook>';
        final rels =
            '<pkg:Relationships xmlns:pkg="$packageNs"><pkg:Relationship Id="rId1" Type="$relType" Target="worksheets/sheet1.xml"/></pkg:Relationships>';
        final discovery = orderedWorkbookDiscovery(fixtureFromMembers(
          workbook,
          rels,
          [('xl/worksheets/sheet1.xml', _b('worksheet'))],
        ));
        expect(discovery.worksheets[0].relationshipId, 'rId1');
        expect(discovery.worksheets[0].relationshipType, relType);
      }
    });

    test('expanded names reports qualified workbook protection', () {
      const workbook =
          '<workbook xmlns="$mainNs" xmlns:r="urn:r" xmlns:ss="$mainNs"><ss:workbookProtection lockStructure="1"/><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>';
      const rels =
          '<Relationships><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/></Relationships>';
      final discovery = orderedWorkbookDiscovery(fixtureFromMembers(
        workbook,
        rels,
        [('xl/worksheets/sheet1.xml', _b('worksheet'))],
      ));
      expect(discovery.safety.hasWorkbookProtection, isTrue);
    });
  });

  group('duplicate and target rules', () {
    test('rejects duplicate name', () {
      const workbook =
          '<workbook xmlns="$mainNs" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/><sheet name="MEC" sheetId="2" r:id="rId2"/></sheets></workbook>';
      const rels =
          '<Relationships><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="$relNs" Target="worksheets/sheet2.xml"/></Relationships>';
      expect(
        () => orderedWorkbookDiscovery(fixtureFromMembers(workbook, rels, [
          ('xl/worksheets/sheet1.xml', _b('one')),
          ('xl/worksheets/sheet2.xml', _b('two')),
        ])),
        throwsA(_msg('duplicate workbook sheet name')),
      );
    });

    test('rejects duplicate sheet id', () {
      const workbook =
          '<workbook xmlns="$mainNs" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="7" r:id="rId1"/><sheet name="CIV" sheetId="7" r:id="rId2"/></sheets></workbook>';
      const rels =
          '<Relationships><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="$relNs" Target="worksheets/sheet2.xml"/></Relationships>';
      expect(
        () => orderedWorkbookDiscovery(fixtureFromMembers(workbook, rels, [
          ('xl/worksheets/sheet1.xml', _b('one')),
          ('xl/worksheets/sheet2.xml', _b('two')),
        ])),
        throwsA(_msg('duplicate workbook sheet identity')),
      );
    });

    test('rejects duplicate relationship id', () {
      const workbook =
          '<workbook xmlns="$mainNs" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/><sheet name="CIV" sheetId="2" r:id="rId1"/></sheets></workbook>';
      const rels =
          '<Relationships><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/></Relationships>';
      expect(
        () => orderedWorkbookDiscovery(fixtureFromMembers(workbook, rels, [
          ('xl/worksheets/sheet1.xml', _b('one')),
        ])),
        throwsA(_msg('duplicate workbook sheet relationship identity')),
      );
    });

    test('rejects duplicate relationship declaration', () {
      const workbook =
          '<workbook xmlns="$mainNs" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>';
      const rels =
          '<Relationships><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet2.xml"/></Relationships>';
      expect(
        () => orderedWorkbookDiscovery(fixtureFromMembers(workbook, rels, [
          ('xl/worksheets/sheet1.xml', _b('one')),
          ('xl/worksheets/sheet2.xml', _b('two')),
        ])),
        throwsA(_msg('duplicate workbook relationship identity')),
      );
    });

    test('rejects duplicate resolved part', () {
      const workbook =
          '<workbook xmlns="$mainNs" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/><sheet name="CIV" sheetId="2" r:id="rId2"/></sheets></workbook>';
      const rels =
          '<Relationships><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="$relNs" Target="/xl/worksheets/sheet1.xml"/></Relationships>';
      expect(
        () => orderedWorkbookDiscovery(fixtureFromMembers(workbook, rels, [
          ('xl/worksheets/sheet1.xml', _b('one')),
        ])),
        throwsA(_msg('duplicate worksheet part path')),
      );
    });

    test('rejects duplicate normalized workbook relationship target', () {
      const workbook =
          '<workbook xmlns="$mainNs" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>';
      for (final alias in [
        './worksheets/sheet1.xml',
        '/xl/worksheets/%73heet1.xml',
        r'worksheets\sheet1.xml',
      ]) {
        final rels =
            '<Relationships><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/><Relationship Id="rIdStyles" Type="$_officeRel/styles" Target="$alias"/></Relationships>';
        expect(
          () => orderedWorkbookDiscovery(fixtureFromMembers(workbook, rels, [
            ('xl/worksheets/sheet1.xml', _b('worksheet')),
          ])),
          throwsA(_msg('duplicate workbook relationship target')),
        );
      }
    });

    test('rejects external relationship target', () {
      const workbook =
          '<workbook xmlns="$mainNs" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>';
      const rels =
          '<Relationships><Relationship Id="rId1" Type="$relNs" Target="https://example.invalid/sheet.xml" TargetMode="External"/></Relationships>';
      expect(
        () => orderedWorkbookDiscovery(fixtureFromMembers(workbook, rels, [])),
        throwsA(_msg('external workbook relationship target is not allowed')),
      );
    });

    test('accepts package internal parent relative custom xml', () {
      const workbook =
          '<workbook xmlns="$mainNs" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>';
      const rels =
          '<Relationships><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="$_officeRel/customXml" Target="../customXml/item1.xml"/><Relationship Id="rId3" Type="$_officeRel/sheetMetadata" Target="metadata.xml"/></Relationships>';
      final discovery = orderedWorkbookDiscovery(fixtureFromMembers(
        workbook,
        rels,
        [
          ('xl/worksheets/sheet1.xml', _b('worksheet')),
          ('customXml/item1.xml', _b('custom xml')),
          ('xl/metadata.xml', _b('sheet metadata')),
        ],
      ));
      expect(discovery.worksheets.length, 1);
      expect(discovery.worksheets[0].partPath, 'xl/worksheets/sheet1.xml');
    });

    test('accepts explicit internal relationship target', () {
      const workbook =
          '<workbook xmlns="$mainNs" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>';
      const rels =
          '<Relationships><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml" TargetMode="Internal"/></Relationships>';
      final discovery = orderedWorkbookDiscovery(fixtureFromMembers(
        workbook,
        rels,
        [('xl/worksheets/sheet1.xml', _b('worksheet'))],
      ));
      expect(discovery.worksheets.length, 1);
      expect(discovery.worksheets[0].name, 'MEC');
    });

    test('rejects encoded traversal target', () {
      const workbook =
          '<workbook xmlns="$mainNs" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>';
      const rels =
          '<Relationships><Relationship Id="rId1" Type="$relNs" Target="worksheets/%2E%2E/evil.xml"/></Relationships>';
      expect(
        () => orderedWorkbookDiscovery(fixtureFromMembers(workbook, rels, [])),
        throwsA(_msg('worksheet relationship escapes the package')),
      );
    });

    test('rejects encoded forbidden target character', () {
      const workbook =
          '<workbook xmlns="$mainNs" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>';
      const rels =
          '<Relationships><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml%3Fignored"/></Relationships>';
      expect(
        () => orderedWorkbookDiscovery(fixtureFromMembers(workbook, rels, [
          ('xl/worksheets/sheet1.xml?ignored', _b('worksheet')),
        ])),
        throwsA(_msg('worksheet relationship escapes the package')),
      );
    });
  });
}
