// Port of src-tauri/tests/rfi_follow_up_native.rs (part 4: unknown source
// status, PCU/FINAL exclusions, and Worley final RFI population).
import 'package:docflow/engine/rfi_follow_up.dart';
import 'package:flutter_test/flutter_test.dart';

import 'e8_full_schema_support.dart';
import 'e8_support.dart';

/// PCU sheet with a multi-line Worley header (`Signed \nby \nWorley`).
SheetSpec pcuTargetMultilineHeader() => worksheet([
  row(3, [
    inline('P3', 'Final acceptance of PCU foundation (3.6)'),
    inline('R3', 'Signed \nby \nWorley'),
  ]),
  row(4, [inline('P4', 'RFI No.'), inline('Q4', 'STATUS')]),
  row(5, [
    inline('B5', 'N1'),
    inline('C5', 'PCS-01'),
    inline('P5', 'CIV-1001'),
    inline('Q5', 'Completed'),
    inline('R5', 'Not Signed'),
  ]),
]);

void main() {
  test('mixed group with unknown source status stays in manual review', () {
    final source = fullSchemaSource([
      fullSchemaRecordWithStatus(
        10,
        '1222',
        'AZE-SUN-INT-QMS-ITP-CIV-0002',
        'CIV-1.5',
        'AREA-1',
        'Completed',
        '',
      ),
      fullSchemaRecordWithStatus(
        11,
        '1223',
        'AZE-SUN-INT-QMS-ITP-CIV-0002',
        'CIV-1.5',
        'AREA-1',
        'Unrecognized status',
        '',
      ),
    ]);
    final target = package([
      (
        'CIV',
        null,
        worksheet([
          row(1, [
            inline('B1', 'Tag'),
            inline('C1', 'RFI No.'),
            inline('D1', 'STATUS'),
            inline('Q1', 'Remarks'),
          ]),
          row(2, [
            inline('B2', 'AREA-1'),
            inline('C2', 'CIV-1222/1223'),
            inline('D2', 'In Progress'),
            inline('Q2', 'RFI Status Sync: CIV-1222 Completed, CIV-1223 In Progress'),
          ]),
        ]),
      ),
    ]);

    final review = classifyBytes(source, target);
    final found = rowsOf(review).firstWhere(
      (row) => row['reasonKey'] == 'warn.rfi.followUp.reason.sourceStatusUnknown',
      orElse: () => fail('sourceStatusUnknown row missing: $review'),
    );
    expect(found['disposition'], manualDisposition);
    expect(found['reasonKey'], 'warn.rfi.followUp.reason.sourceStatusUnknown');
    expect(found.containsKey('proposedRemark'), isFalse);
  });

  test('full sync removes PCS-tagged PCU FINAL from existing Follow Up rows', () {
    final source = fullSchemaSource([pcuFinalRecord(10, 'Signed by Worley')]);
    final target = package([
      ('PCU', null, pcuTarget('CIV-1001', 'Completed', 'Not Signed')),
      (
        'POT test',
        null,
        worksheet([
          row(3, [inline('D3', 'RFI No.'), inline('E3', 'STATUS')]),
          row(4, [inline('D4', 'MEC-2009'), inline('E4', 'Signed')]),
        ]),
      ),
    ]);

    final review = classifyBytes(source, target);
    final rows = rowsOf(review);
    expect(rows, isNotEmpty, reason: review.toString());
    expect(
      rows.every(
        (row) =>
            row['_sheet'] == 'PCU' &&
            row['proposedRfiNo'] == '' &&
            row['proposedStatus'] == '',
      ),
      isTrue,
      reason: review.toString(),
    );
    expect(
      (review['layouts'] as List).every(
        (layout) => obj(obj(layout)['worksheet'])['name'] != 'POT test',
      ),
      isTrue,
    );
  });

  test('full sync removes PCS-tagged PCU FINAL with multiline Worley header', () {
    final source = fullSchemaSource([pcuFinalRecord(10, 'Signed by Worley')]);
    final target = package([('PCU', null, pcuTargetMultilineHeader())]);

    final review = classifyBytes(source, target);
    final rows = rowsOf(review);
    expect(rows, isNotEmpty, reason: review.toString());
    expect(
      rows.every(
        (row) => row['proposedRfiNo'] == '' && row['proposedStatus'] == '',
      ),
      isTrue,
    );
  });

  test('full sync does not add PCS-tagged PCU FINAL to empty Follow Up', () {
    final source = fullSchemaSource([pcuFinalRecord(10, 'Signed by Worley')]);
    final target = package([('PCU', null, pcuTarget('', '', ''))]);

    final review = classifyBytes(source, target);
    expect(rowsOf(review), isEmpty, reason: review.toString());
    expect(
      (review['warnings'] as List).every(
        (warning) => obj(warning)['key'] != 'warn.rfi.followUp.unmappedRecords',
      ),
      isTrue,
    );
  });

  test('PCU Worley plan clears RFI and status for excluded PCS FINAL', () {
    final source = fullSchemaSource([pcuFinalRecord(10, 'Signed by Worley')]);
    final target = package([
      ('PCU', null, pcuTarget('CIV-1001', 'Completed', 'Not Signed')),
    ]);

    final plan = previewBytes(source, target);
    final updates = guardedUpdates(plan.steps[0]);
    final pcu = updates['PCU']!;

    expect(pcu.length, greaterThanOrEqualTo(2));
    expect(pcu.containsKey((5, 16)), isTrue);
    expect(pcu.containsKey((5, 17)), isTrue);
    expect(pcu.containsKey((5, 18)), isTrue);
  });

  test('full sync adds missing FINAL RFIs and exact Worley statuses to prepared rows', () {
    final source = fullSchemaSource([
      fullSchemaRecord(10, '0001', 'CIV-2.3.1', 'AREA-1', 'Signed by Worley'),
      fullSchemaRecord(11, '0002', 'CIV-2.3.1', 'AREA-2', ''),
      fullSchemaRecord(12, '0003', 'CIV-2.1.5', 'AREA-3', 'Signed by Worley'),
    ]);
    final target = package([
      (
        'Signed By Worley CIV',
        null,
        worksheet([
          row(4, [inline('C4', 'RFI No.'), inline('D4', 'Status')]),
          row(5, [inline('B5', 'AREA-1'), blank('C5'), blank('D5')]),
          row(6, [inline('B6', 'AREA-2'), blank('C6'), blank('D6')]),
          row(7, [inline('B7', 'AREA-3'), blank('C7'), blank('D7')]),
        ]),
      ),
    ]);

    final review = classifyBytes(source, target);
    final rows = rowsOf(review);

    expect(rows.length, 2);
    expect(rows[0]['proposedRfiNo'], 'CIV-0001');
    expect(rows[0]['sourceStatuses'], ['Signed']);
    expect(rows[0]['proposedStatus'], 'Signed');
    expect(rows[1]['proposedRfiNo'], 'CIV-0002');
    expect(rows[1]['sourceStatuses'], ['Not Signed']);
    expect(rows[1]['proposedStatus'], 'Not Signed');
    expect(
      rows.every((row) => row['reasonKey'] == 'warn.rfi.followUp.reason.readyWorleySync'),
      isTrue,
    );
  });
}
