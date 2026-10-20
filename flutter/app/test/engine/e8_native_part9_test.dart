// Port of src-tauri/tests/rfi_follow_up_native.rs (part 9: authority and
// layout rules, physical extent limits, unsafe-row visibility and formula or
// merge facts).
import 'package:docflow/engine/rfi_follow_up.dart';
import 'package:flutter_test/flutter_test.dart';

import 'e8_full_schema_support.dart';

void main() {
  test('Follow Up authority and layout require exact safe bounded inputs', () {
    final source = sourcePackage(
      worksheet(const []),
      worksheet(const []),
    );
    final valid = classifyBytes(source, targetWithRows(const []));
    final layout = obj(valid['layout']);
    expect(layout['headerRow'], 1);
    expect(layout['rfiColumn'], 1);
    expect(layout['statusColumn'], 2);

    final missing = targetPackage(
      worksheet([row(1, [inline('A1', 'RFI NO'), inline('B1', 'STATUS')])]),
    );
    expect(
      () => classifyBytes(source, missing),
      throwsBackendMessage('error.rfi.followUp.layoutMissing'),
    );

    final outOfBounds = targetPackage(
      worksheet([row(33, [inline('A33', 'RFI NO.'), inline('B33', 'STATUS')])]),
    );
    expect(
      () => classifyBytes(source, outOfBounds),
      throwsBackendMessage('error.rfi.followUp.layoutMissing'),
    );

    final duplicateFormulaHeader = targetPackage(
      worksheet([
        row(1, [
          formula('A1', 'LOOKUP()', 'RFI NO.'),
          inline('B1', 'STATUS'),
          inline('C1', 'RFI NO.'),
        ]),
      ]),
    );
    expect(
      () => classifyBytes(source, duplicateFormulaHeader),
      throwsBackendMessage('error.rfi.followUp.layoutAmbiguous'),
    );

    final hidden = package([
      (
        'Hidden Follow Up',
        'hidden',
        worksheet([row(1, [inline('A1', 'RFI NO.'), inline('B1', 'STATUS')])]),
      ),
    ]);
    expect(
      () => classifyBytes(source, hidden),
      throwsBackendMessage('error.rfi.followUp.protected'),
    );

    final protectedSheet = SheetSpec(
      [row(1, [inline('A1', 'RFI NO.'), inline('B1', 'STATUS')])],
      protected: true,
    );
    expect(
      () => classifyBytes(source, targetPackage(protectedSheet)),
      throwsBackendMessage('error.rfi.followUp.protected'),
    );

    const ooxml = FakeOoxml();
    final sourceParsed = ooxml.readWorkbookAndDiscovery(source);
    final signedParsed = ooxml.readWorkbookAndDiscovery(
      targetWithRows(const []),
    );
    expect(
      () => classifyWorkbooks(
        sourceParsed.discovery,
        sourceParsed.workbook,
        signedParsed.discovery,
        signedParsed.workbook,
        targetHasSignature: true,
      ),
      throwsBackendMessage('error.rfi.followUp.signedPackage'),
    );

    final ambiguous = package([
      (
        'First',
        null,
        worksheet([row(1, [inline('A1', 'RFI NO.'), inline('B1', 'STATUS')])]),
      ),
      (
        'Second',
        null,
        worksheet([row(1, [inline('D1', 'RFI NO.'), inline('F1', 'STATUS')])]),
      ),
    ]);
    expect(
      () => classifyBytes(source, ambiguous),
      throwsBackendMessage('error.rfi.followUp.layoutAmbiguous'),
    );
  });

  test('unique Follow Up table is not poisoned by a duplicate summary layout', () {
    final source = sourcePackage(worksheet(const []), worksheet(const []));
    final target = package([
      (
        'Summary',
        null,
        worksheet([
          row(1, [
            inline('A1', 'RFI NO.'),
            inline('B1', 'STATUS'),
            inline('C1', 'RFI NO.'),
            inline('D1', 'STATUS'),
          ]),
        ]),
      ),
      (
        'Signed By Worley CIV',
        null,
        worksheet([row(4, [inline('C4', 'RFI NO.'), inline('D4', 'STATUS')])]),
      ),
    ]);

    final review = classifyBytes(source, target);
    expect(obj(obj(review['layout'])['worksheet'])['name'], 'Signed By Worley CIV');
  });

  test('production template physical extent within twenty thousand rows is accepted', () {
    final source = sourcePackage(
      worksheet([row(18913, const [])]),
      worksheet(const []),
    );

    classifyBytes(source, targetWithRows(const []));
  });

  test('empty physical high row enforces source limit', () {
    final source = sourcePackage(
      worksheet([row(20010, const [])]),
      worksheet(const []),
    );

    expect(
      () => classifyBytes(source, targetWithRows(const [])),
      throwsBackendMessage('error.rfi.followUp.limitExceeded'),
    );
  });

  test('classification keeps unsafe rows visible but unselectable', () {
    const completed = 'AZE-SUN-INT-QMS-RFI-CIV-0001';
    const unknown = 'AZE-SUN-INT-QMS-RFI-CIV-0002';
    const controlled = 'AZE-SUN-INT-QMS-RFI-CIV-0003';
    final source = sourcePackage(
      worksheet([
        row(10, [inline('A10', completed), inline('T10', 'Completed')]),
        row(11, [inline('A11', unknown), inline('T11', 'Cancelled')]),
        row(12, [inline('A12', controlled), inline('T12', 'Completed')]),
      ]),
      worksheet(const []),
    );
    final review = classifyBytes(
      source,
      targetWithRows([
        row(2, [inline('A2', completed), blank('B2')]),
        row(3, [inline('A3', 'not-an-rfi'), blank('B3')]),
        row(4, [inline('A4', 'AZE-SUN-INT-QMS-RFI-CIV-9999'), blank('B4')]),
        row(5, [inline('A5', unknown), blank('B5')]),
        row(6, [inline('A6', controlled), inline('B6', 'Pending')]),
        row(7, [blank('A7'), inline('B7', 'Completed')]),
      ]),
    );
    final rows = rowsOf(review);
    expect(rows.length, 6);
    expect(rows[0]['enabled'], true);
    final reasons = rows.skip(1).map((row) => row['reasonKey']).toList();
    expect(reasons, [
      'warn.rfi.followUp.reason.invalidRfi',
      'warn.rfi.followUp.reason.sourceMissing',
      'warn.rfi.followUp.reason.readyNotSigned',
      'warn.rfi.followUp.reason.targetStatusUnknown',
      'warn.rfi.followUp.reason.blankRfi',
    ]);
    expect(rows[3]['enabled'], true);
    for (final index in [1, 2, 4, 5]) {
      expect(rows[index]['enabled'], false, reason: 'row $index');
      expect(rows[index]['selectable'], false, reason: 'row $index');
    }
  });

  test('formula and merge facts drive manual review', () {
    const sourceFormula = 'AZE-SUN-INT-QMS-RFI-CIV-0008';
    const sourceMerged = 'AZE-SUN-INT-QMS-RFI-CIV-0009';
    const targetFormula = 'AZE-SUN-INT-QMS-RFI-CIV-0010';
    const targetMerged = 'AZE-SUN-INT-QMS-RFI-CIV-0011';
    final source = sourcePackage(
      worksheet(
        [
          row(10, [inline('A10', sourceFormula), formula('T10', 'LOOKUP()', 'Completed')]),
          row(11, [inline('A11', sourceMerged), inline('T11', 'Completed')]),
          row(12, [inline('A12', targetFormula), inline('T12', 'Completed')]),
          row(13, [inline('A13', targetMerged), inline('T13', 'Completed')]),
        ],
        const ['T11:U11'],
      ),
      worksheet(const []),
    );
    final review = classifyBytes(
      source,
      targetWithRows(
        [
          row(2, [inline('A2', sourceFormula), blank('B2')]),
          row(3, [inline('A3', sourceMerged), blank('B3')]),
          row(4, [formula('A4', 'LOOKUP()', targetFormula), blank('B4')]),
          row(5, [inline('A5', targetMerged), blank('B5')]),
        ],
        const ['B5:C5'],
      ),
    );
    expect(
      rowsOf(review).map((row) => row['reasonKey']).toList(),
      [
        'warn.rfi.followUp.reason.sourceFormula',
        'warn.rfi.followUp.reason.sourceMerged',
        'warn.rfi.followUp.reason.targetRfiFormula',
        'warn.rfi.followUp.reason.targetMerged',
      ],
    );
  });
}
