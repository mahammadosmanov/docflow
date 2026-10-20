// Port of the ZIP preflight, sheet-kind and limit tests in
// src-tauri/src/ooxml.rs (mod tests).
import 'dart:convert';
import 'dart:typed_data';

import 'package:docflow/engine/ooxml.dart';
import 'package:flutter_test/flutter_test.dart';

import 'e6_ooxml_fixtures.dart';

Uint8List _b(String text) => Uint8List.fromList(utf8.encode(text));

const String _officeRel =
    'http://schemas.openxmlformats.org/officeDocument/2006/relationships';

void _assertPreflightRejects(Uint8List fixture) {
  expect(
    () => preflightArchiveForTesting(fixture),
    throwsA(ooxmlError('workbook ZIP metadata is invalid')),
  );
  expect(
    () => orderedWorkbookDiscovery(fixture),
    throwsA(ooxmlError('workbook ZIP metadata is invalid')),
  );
}

List<int> _u32Max() => [0xff, 0xff, 0xff, 0xff];

void main() {
  group('sheet kinds and limits', () {
    test('rejects non worksheet relationship', () {
      const workbook =
          '<workbook xmlns="$mainNs" xmlns:r="urn:r"><sheets><sheet name="Chart" sheetId="1" r:id="rId1"/></sheets></workbook>';
      const rels =
          '<Relationships><Relationship Id="rId1" Type="$_officeRel/chartsheet" Target="chartsheets/sheet1.xml"/></Relationships>';
      expect(
        () => orderedWorkbookDiscovery(fixtureFromMembers(workbook, rels, [
          ('xl/chartsheets/sheet1.xml', _b('chart')),
        ])),
        throwsA(ooxmlError('workbook sheet relationship is not a worksheet')),
      );
    });

    test('rejects spoofed relationship namespace', () {
      const workbook =
          '<workbook xmlns="$mainNs" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>';
      const rels =
          '<Relationships><Relationship Id="rId1" Type="http://schemas.microsoft.com/office/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>';
      expect(
        () => orderedWorkbookDiscovery(fixtureFromMembers(workbook, rels, [
          ('xl/worksheets/sheet1.xml', _b('worksheet')),
        ])),
        throwsA(ooxmlError('workbook sheet relationship is not a worksheet')),
      );
    });

    test('rejects missing worksheet member', () {
      const workbook =
          '<workbook xmlns="$mainNs" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>';
      const rels =
          '<Relationships><Relationship Id="rId1" Type="$relNs" Target="worksheets/missing.xml"/></Relationships>';
      expect(
        () => orderedWorkbookDiscovery(fixtureFromMembers(workbook, rels, [])),
        throwsA(ooxmlError('worksheet package member is missing')),
      );
    });

    test('internal relationship targets require exact package members', () {
      const workbook =
          '<workbook xmlns="$mainNs" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>';
      const missingWorkbookAuxiliary =
          '<Relationships><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/><Relationship Id="rIdStyles" Type="$_officeRel/styles" Target="styles/missing.xml"/></Relationships>';
      expect(
        () => orderedWorkbookDiscovery(fixtureFromMembers(
          workbook,
          missingWorkbookAuxiliary,
          [('xl/worksheets/sheet1.xml', _b('worksheet'))],
        )),
        throwsA(ooxmlError('workbook relationship package member is missing')),
      );

      const worksheetRelationships =
          '<Relationships xmlns="$packageRelNs"><Relationship Id="rIdDrawing" Type="$_officeRel/drawing" Target="../drawings/missing.xml"/></Relationships>';
      const rels =
          '<Relationships><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/></Relationships>';
      expect(
        () => orderedWorkbookDiscovery(fixtureFromMembers(workbook, rels, [
          ('xl/worksheets/sheet1.xml', _b('worksheet')),
          ('xl/worksheets/_rels/sheet1.xml.rels', _b(worksheetRelationships)),
        ])),
        throwsA(ooxmlError('worksheet relationship package member is missing')),
      );
    });

    test('rejects unknown visibility state', () {
      const workbook =
          '<workbook xmlns="$mainNs" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" state="mystery" r:id="rId1"/></sheets></workbook>';
      const rels =
          '<Relationships><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/></Relationships>';
      expect(
        () => orderedWorkbookDiscovery(fixtureFromMembers(workbook, rels, [
          ('xl/worksheets/sheet1.xml', _b('one')),
        ])),
        throwsA(ooxmlError('worksheet visibility state is invalid')),
      );
    });

    test('rejects zero worksheets', () {
      const workbook =
          '<workbook xmlns="$mainNs" xmlns:r="$_officeRel"><sheets/></workbook>';
      expect(
        () => orderedWorkbookDiscovery(fixtureFromMembers(
          workbook,
          '<Relationships/>',
          [],
        )),
        throwsA(ooxmlError('workbook contains no worksheets')),
      );
    });

    test('rejects 251 worksheets', () {
      final sheets = StringBuffer();
      final relationshipElements = StringBuffer();
      final members = <(String, List<int>)>[];
      for (var index = 1; index <= 251; index++) {
        sheets.write(
          '<sheet name="Sheet$index" sheetId="$index" r:id="rId$index"/>',
        );
        relationshipElements.write(
          '<Relationship Id="rId$index" Type="$relNs" Target="worksheets/sheet$index.xml"/>',
        );
        members.add(('xl/worksheets/sheet$index.xml', _b('sheet')));
      }
      final workbook =
          '<workbook xmlns="$mainNs" xmlns:r="urn:r"><sheets>$sheets</sheets></workbook>';
      final relationships =
          '<Relationships>$relationshipElements</Relationships>';
      expect(
        () => orderedWorkbookDiscovery(
          fixtureFromMembers(workbook, relationships, members),
        ),
        throwsA(ooxmlError('workbook has too many worksheets')),
      );
    });

    test('rejects oversized source before zip parse', () {
      // Zero-filled and lazily committed by the OS; only the length matters.
      final oversized = Uint8List(maxPackageBytes + 1);
      expect(
        () => orderedWorkbookDiscovery(oversized),
        throwsA(ooxmlError(
          'workbook source exceeded the size limit',
          kind: OoxmlErrorKind.limitExceeded,
        )),
      );
    });

    test('rejects declared entry count before zip parse', () {
      expect(
        () => orderedWorkbookDiscovery(
          declaredEntryCountFixture(maxEntriesForTesting + 1),
        ),
        throwsA(ooxmlError('workbook ZIP entry count is invalid')),
      );
    });

    test('rejects decoy eocd before parser fallback', () {
      expect(
        () => orderedWorkbookDiscovery(appendDecoyEocd(orderedFixture())),
        throwsA(ooxmlError('workbook ZIP metadata is invalid')),
      );
    });

    test('rejects inflated final eocd before parser fallback', () {
      expect(
        () => orderedWorkbookDiscovery(
          appendParserFallbackEocd(orderedFixture()),
        ),
        throwsA(ooxmlError('workbook ZIP metadata is invalid')),
      );
    });

    test('rejects semantically invalid final directory', () {
      expect(
        () => orderedWorkbookDiscovery(
          appendSemanticallyInvalidDirectory(orderedFixture()),
        ),
        throwsA(ooxmlError('workbook ZIP metadata is invalid')),
      );
    });
  });

  group('archive preflight', () {
    test('rejects entry nonzero disk number start', () {
      _assertPreflightRejects(
        mutateFirstCentralField(orderedFixture(), 34, [1, 0]),
      );
    });

    test('rejects entry zip64 central sentinels', () {
      for (final fieldOffset in [20, 24, 42]) {
        _assertPreflightRejects(
          mutateFirstCentralField(orderedFixture(), fieldOffset, _u32Max()),
        );
      }
      _assertPreflightRejects(
        mutateFirstCentralField(orderedFixture(), 34, [0xff, 0xff]),
      );
    });

    test('rejects entry zip64 central extra without sentinel', () {
      _assertPreflightRejects(
        insertFirstCentralExtra(orderedFixture(), [0x01, 0x00, 0x00, 0x00]),
      );
    });

    test('rejects entry malformed central extra', () {
      _assertPreflightRejects(
        insertFirstCentralExtra(orderedFixture(), [0xfe, 0xca, 0x04, 0x00]),
      );
    });

    test('rejects entry zip64 local size sentinels', () {
      for (final fieldOffset in [18, 22]) {
        _assertPreflightRejects(
          mutateFirstLocalField(orderedFixture(), fieldOffset, _u32Max()),
        );
      }
    });

    test('rejects entry zip64 local extra', () {
      _assertPreflightRejects(
        insertLastLocalExtra(orderedFixture(), [0x01, 0x00, 0x00, 0x00]),
      );
    });

    test('rejects entry malformed local extra', () {
      _assertPreflightRejects(
        insertLastLocalExtra(orderedFixture(), [0xfe, 0xca, 0x04, 0x00]),
      );
    });
  });
}
