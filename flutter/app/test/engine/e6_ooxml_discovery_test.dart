// Port of the discovery, relationship-authority and workbook-read tests in
// src-tauri/src/ooxml.rs (mod tests), first group.
import 'dart:convert';
import 'dart:typed_data';

import 'package:docflow/engine/ooxml.dart';
import 'package:flutter_test/flutter_test.dart';

import 'e6_ooxml_fixtures.dart';

Uint8List _b(String text) => Uint8List.fromList(utf8.encode(text));

void main() {
  group('merge comparison budget', () {
    test('guarded follow-up apply merge budget exact boundary and one over', () {
      final exact = MergeComparisonBudget.forApply(maxApplyMergeComparisons - 1);
      exact.consume();
      expect(
        () => exact.consume(),
        throwsA(ooxmlError(
          'guarded merge comparison count exceeded the limit',
          kind: OoxmlErrorKind.limitExceeded,
        )),
      );
      expect(
        () => MergeComparisonBudget.forApply(maxApplyMergeComparisons + 1),
        throwsA(ooxmlError(
          'guarded merge comparison count exceeded the limit',
          kind: OoxmlErrorKind.limitExceeded,
        )),
      );
    });
  });

  group('xml parser limits', () {
    test('rejects excessive element depth', () {
      expect(
        () => parseDocumentWithLimitsForTesting(
          _b('<a><b><c/></b></a>'),
          maxDepth: 2,
          maxNodes: 100,
        ),
        throwsA(ooxmlError(
          'XML element depth exceeded the limit',
          kind: OoxmlErrorKind.limitExceeded,
        )),
      );
    });

    test('rejects excessive retained nodes', () {
      expect(
        () => parseDocumentWithLimitsForTesting(
          _b('<a><b/><c/></a>'),
          maxDepth: 10,
          maxNodes: 2,
        ),
        throwsA(ooxmlError(
          'XML node count exceeded the limit',
          kind: OoxmlErrorKind.limitExceeded,
        )),
      );
    });
  });

  group('ordered discovery', () {
    test('preserves order identity and visibility', () {
      final discovery = orderedWorkbookDiscovery(orderedFixture());
      expect(discovery.worksheets, [
        const WorksheetDescriptor(
          workbookIndex: 1,
          sheetId: 17,
          relationshipId: 'rId7',
          relationshipType: relNs,
          partPath: 'xl/worksheets/sheet7.xml',
          name: 'MEC',
          visibility: WorksheetVisibility.visible,
        ),
        const WorksheetDescriptor(
          workbookIndex: 2,
          sheetId: 3,
          relationshipId: 'rId2',
          relationshipType: relNs,
          partPath: 'xl/worksheets/sheet2.xml',
          name: 'CIV',
          visibility: WorksheetVisibility.visible,
        ),
        const WorksheetDescriptor(
          workbookIndex: 3,
          sheetId: 44,
          relationshipId: 'rId9',
          relationshipType: relNs,
          partPath: 'xl/worksheets/sheet9.xml',
          name: 'Hidden',
          visibility: WorksheetVisibility.hidden,
        ),
        const WorksheetDescriptor(
          workbookIndex: 4,
          sheetId: 45,
          relationshipId: 'rId10',
          relationshipType: relNs,
          partPath: 'xl/worksheets/sheet10.xml',
          name: 'VeryHidden',
          visibility: WorksheetVisibility.veryHidden,
        ),
      ]);
      expect(discovery.safety, const WorkbookSafetyFacts());
    });

    test('ignores sheet elements outside sheets container', () {
      const workbook =
          '<workbook xmlns="$mainNs" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:ext="urn:extension"><ext:data><ext:sheet name="Decoy" sheetId="99" r:id="rId99"/></ext:data><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>';
      const relationships =
          '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId99" Type="$relNs" Target="worksheets/decoy.xml"/><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/></Relationships>';
      final discovery = orderedWorkbookDiscovery(fixtureFromMembers(
        workbook,
        relationships,
        [
          ('xl/worksheets/decoy.xml', _b('decoy')),
          ('xl/worksheets/sheet1.xml', _b('worksheet')),
        ],
      ));
      expect(discovery.worksheets.length, 1);
      expect(discovery.worksheets[0].name, 'MEC');
      expect(discovery.worksheets[0].workbookIndex, 1);
    });

    test('ignores foreign sheet elements in sheets container', () {
      const workbook =
          '<workbook xmlns="$mainNs" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:ext="urn:extension"><sheets><ext:sheet name="Decoy" sheetId="99" r:id="rId99"/><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>';
      const relationships =
          '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId99" Type="$relNs" Target="worksheets/decoy.xml"/><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/></Relationships>';
      final discovery = orderedWorkbookDiscovery(fixtureFromMembers(
        workbook,
        relationships,
        [
          ('xl/worksheets/decoy.xml', _b('decoy')),
          ('xl/worksheets/sheet1.xml', _b('worksheet')),
        ],
      ));
      expect(discovery.worksheets.length, 1);
      expect(discovery.worksheets[0].name, 'MEC');
    });
  });

  group('sheet projection and reads', () {
    test('legacy sheet info projection ignores foreign sheet elements', () {
      const workbook =
          '<workbook xmlns="$mainNs" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:ext="urn:extension"><sheets><ext:sheet name="Decoy" sheetId="99" r:id="rId99"/><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>';
      const relationships =
          '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId99" Type="$relNs" Target="worksheets/decoy.xml"/><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/></Relationships>';
      final infos = sheetInfosFromBytes(fixtureFromMembers(
        workbook,
        relationships,
        [
          ('xl/worksheets/decoy.xml', _b('decoy')),
          ('xl/worksheets/sheet1.xml', _b('worksheet')),
        ],
      ));
      expect(infos.length, 1);
      expect(infos['MEC']!.sheetId, 1);
      expect(infos['MEC']!.path, 'xl/worksheets/sheet1.xml');
    });

    test('read workbook uses canonical sheet projection', () {
      const workbook =
          '<workbook xmlns="$mainNs" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:ext="urn:extension"><sheets><ext:sheet name="Decoy" sheetId="99" r:id="rId99"/><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>';
      const relationships =
          '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId99" Type="$relNs" Target="worksheets/decoy.xml"/><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/></Relationships>';
      const worksheet = '<worksheet xmlns="$mainNs"><sheetData/></worksheet>';
      final data = readWorkbook(fixtureFromMembers(
        workbook,
        relationships,
        [
          ('xl/worksheets/decoy.xml', _b('decoy')),
          ('xl/worksheets/sheet1.xml', _b(worksheet)),
        ],
      ));
      expect(data.sheet('MEC'), isNotNull);
      expect(data.sheet('Decoy'), isNull);
    });

    test('read workbook uses authenticated shared strings target', () {
      const workbook =
          '<workbook xmlns="$mainNs" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>';
      const relationships =
          '<Relationships><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/><Relationship Id="rIdStrings" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="/custom/strings.xml"/></Relationships>';
      const worksheet =
          '<worksheet xmlns="$mainNs"><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c></row></sheetData></worksheet>';
      const trusted = '<sst xmlns="$mainNs" count="1" uniqueCount="1"><si><t>Trusted</t></si></sst>';
      const decoy = '<sst xmlns="$mainNs" count="1" uniqueCount="1"><si><t>Decoy</t></si></sst>';
      final data = readWorkbook(fixtureFromMembers(
        workbook,
        relationships,
        [
          ('xl/worksheets/sheet1.xml', _b(worksheet)),
          ('custom/strings.xml', _b(trusted)),
          ('xl/sharedStrings.xml', _b(decoy)),
        ],
      ));
      expect(data.sheet('MEC')!.value(1, 1), 'Trusted');
    });

    test('patch workbook sheets uses canonical sheet projection', () {
      const workbook =
          '<workbook xmlns="$mainNs" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:ext="urn:extension"><sheets><ext:sheet name="MEC" sheetId="99" r:id="rId99"/><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>';
      const relationships =
          '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId99" Type="$relNs" Target="worksheets/decoy.xml"/><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/></Relationships>';
      const worksheet =
          '<worksheet xmlns="$mainNs"><sheetData><row r="1"><c r="A1"><v>old</v></c></row></sheetData></worksheet>';
      final source = fixtureFromMembers(
        workbook,
        relationships,
        [
          ('xl/worksheets/decoy.xml', _b('decoy')),
          ('xl/worksheets/sheet1.xml', _b(worksheet)),
        ],
      );
      final destination = patchWorkbookSheets(source, {
        'MEC': {(1, 1): const CellUpdateText('new')},
      });
      expect(readWorkbook(destination).sheet('MEC')!.value(1, 1), 'new');
    });

    test('guarded text patch can clear a literal cell', () {
      const workbook =
          '<workbook xmlns="$mainNs" xmlns:r="urn:r"><sheets><sheet name="Follow Up" sheetId="1" r:id="rId1"/></sheets></workbook>';
      const relationships =
          '<Relationships><Relationship Id="rId1" Type="$relNs" Target="worksheets/sheet1.xml"/></Relationships>';
      const worksheet =
          '<worksheet xmlns="$mainNs"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Pending</t></is></c></row></sheetData></worksheet>';
      final source = fixtureFromMembers(
        workbook,
        relationships,
        [('xl/worksheets/sheet1.xml', _b(worksheet))],
      );
      final updates = {
        'Follow Up': {
          (1, 1): const GuardedCellUpdate(
            expected: CellData(value: 'Pending'),
            replacement: CellUpdateText(''),
            createIfAbsent: false,
          ),
        },
      };
      final destination = patchWorkbookSheetsGuardedText(source, updates);
      verifyGuardedWorkbookDeltaText(source, destination, updates);
    });
  });
}
