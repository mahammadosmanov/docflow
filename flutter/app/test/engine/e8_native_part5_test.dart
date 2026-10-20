// Port of src-tauri/tests/rfi_follow_up_native.rs (part 5: CIV/MEC FINAL
// fan-out, unsigned duplicates and MEC FINAL placement review).
import 'package:flutter_test/flutter_test.dart';

import 'e8_full_schema_support.dart';

void main() {
  test('full sync fans out signed CIV FINAL to each distinct exact area', () {
    final source = fullSchemaSource([
      fullSchemaRecordWithStatus(
        10,
        '0001',
        'AZE-SUN-INT-QMS-ITP-CIV-0002',
        'CIV-2.3.1',
        'AREA-1',
        'Completed',
        'Signed by Worley',
      ),
      fullSchemaRecordWithStatus(
        11,
        '0001',
        'AZE-SUN-INT-QMS-ITP-CIV-0002',
        'CIV-2.3.1',
        'AREA-2',
        'Completed',
        'Signed by Worley',
      ),
    ]);
    final target = package([
      (
        'Signed By Worley CIV',
        null,
        worksheet([
          row(4, [inline('C4', 'RFI No.'), inline('D4', 'Status')]),
          row(5, [inline('B5', 'AREA-1'), inline('C5', 'CIV-0001'), inline('D5', 'Signed')]),
          row(6, [inline('B6', 'AREA-2'), blank('C6'), blank('D6')]),
        ]),
      ),
    ]);

    final review = classifyBytes(source, target);
    final rows = rowsOf(review);
    expect(rows.length, 1, reason: review.toString());
    expect(rows[0]['_row'], 6);
    expect(rows[0]['proposedRfiNo'], 'CIV-0001');
    expect(rows[0]['sourceStatuses'], ['Signed']);
    expect(rows[0]['proposedStatus'], 'Signed');
  });

  test('unsigned duplicate does not downgrade a signed source RFI', () {
    final source = fullSchemaSource([
      fullSchemaRecordWithStatus(
        10,
        '0001',
        'AZE-SUN-INT-QMS-ITP-CIV-0002',
        'CIV-1.3.1',
        'AREA-1',
        'Completed',
        'Signed by Worley',
      ),
      fullSchemaRecordWithStatus(
        11,
        '0001',
        'AZE-SUN-INT-QMS-ITP-CIV-0002',
        'CIV-1.3.1',
        'AREA-1',
        'Completed',
        '',
      ),
    ]);
    final target = package([
      (
        'Signed By Worley CIV',
        null,
        worksheet([
          row(4, [inline('C4', 'RFI No.'), inline('D4', 'Status')]),
          row(5, [inline('B5', 'AREA-1'), blank('C5'), blank('D5')]),
        ]),
      ),
    ]);

    final review = classifyBytes(source, target);
    final found = rowsOf(review).firstWhere(
      (row) => row['proposedRfiNo'] == 'CIV-0001',
      orElse: () => fail('CIV-0001 row missing: $review'),
    );
    expect(found['sourceStatuses'], ['Signed']);
    expect(found['proposedStatus'], 'Signed');
    expect(found['disposition'], readyDisposition);
  });

  test('full sync adds unsigned MEC FINAL RFI to a matching prepared row', () {
    final source = sourcePackage(
      worksheet([fullSchemaMecHeaderRow()]),
      worksheet([
        fullSchemaMecHeaderRow(),
        fullSchemaMecRecord(10, '0001', 'MEC-1.3.5', 'PCS-01', 'S1', 'N1', 'Completed'),
      ]),
    );
    final target = package([
      (
        'Signed By Worley MEC',
        null,
        worksheet([
          row(3, [inline('H3', 'Final acceptance of pile installation (3.5)')]),
          row(4, [inline('H4', 'RFI No.'), inline('I4', 'STATUS')]),
          row(5, [
            inline('A5', 'Zone 1'),
            inline('B5', 'N1'),
            inline('C5', 'PCS-01'),
            inline('D5', 'S1'),
            blank('H5'),
            blank('I5'),
          ]),
        ]),
      ),
    ]);

    final review = classifyBytes(source, target);
    final found = rowsOf(review).firstWhere(
      (row) => row['proposedRfiNo'] == 'MEC-0001',
      orElse: () => fail('MEC-0001 row missing: $review'),
    );
    expect(found['rfiNo'], 'MEC-0001');
    expect(found['sourceStatuses'], ['Not Signed']);
    expect(found['proposedRfiNo'], 'MEC-0001');
    expect(found['proposedStatus'], 'Not Signed');
    expect(found['disposition'], readyDisposition);
  });

  test('full sync adds missing signed MEC destination even when same RFI exists elsewhere', () {
    for (final areas in [
      ['S1', 'S2'],
      ['S2', 'S1'],
    ]) {
      final source = sourcePackage(
        worksheet([fullSchemaMecHeaderRow()]),
        worksheet([
          fullSchemaMecHeaderRow(),
          mecRecordWithRemarks(10, '0767', 'MEC-1.3.5', 'PCS-06', areas[0], 'N2', 'Completed', 'Signed by Worley'),
          mecRecordWithRemarks(11, '0767', 'MEC-1.3.5', 'PCS-06', areas[1], 'N2', 'Completed', 'Signed by Worley'),
        ]),
      );
      final target = package([
        (
          'Signed By Worley MEC',
          null,
          worksheet([
            row(3, [inline('H3', 'Final acceptance of pile installation (3.5)')]),
            row(4, [inline('H4', 'RFI No.'), inline('I4', 'STATUS')]),
            row(5, [
              inline('B5', 'N2'),
              inline('C5', 'PCS-06'),
              inline('D5', 'S1'),
              inline('H5', 'MEC-0767'),
              inline('I5', 'Signed'),
            ]),
            row(6, [
              inline('B6', 'N2'),
              inline('C6', 'PCS-06'),
              inline('D6', 'S2'),
              blank('H6'),
              blank('I6'),
            ]),
          ]),
        ),
      ]);

      final review = classifyBytes(source, target);
      final rows = rowsOf(review);
      expect(rows.length, 1, reason: review.toString());
      expect(rows[0]['_sheet'], 'Signed By Worley MEC');
      expect(rows[0]['_row'], 6);
      expect(rows[0]['proposedRfiNo'], 'MEC-0767');
      expect(rows[0]['proposedStatus'], 'Signed');
      expect(rows[0]['disposition'], readyDisposition);
    }
  });

  test('full sync updates prepared MEC context and reviews missing context', () {
    final source = sourcePackage(
      worksheet([fullSchemaMecHeaderRow()]),
      worksheet([
        fullSchemaMecHeaderRow(),
        mecRecordWithRemarks(10, '0123', 'MEC-1.3.5', 'PCS-06', 'S1', 'N2', 'Completed', 'Signed by Worley'),
        mecRecordWithRemarks(11, '0123', 'MEC-1.3.5', 'PCS-06', 'S2', 'N2', 'Completed', 'Signed by Worley'),
      ]),
    );
    final target = package([
      (
        'Signed By Worley MEC',
        null,
        worksheet([
          row(3, [inline('H3', 'Final acceptance of pile installation (3.5)')]),
          row(4, [inline('H4', 'RFI No.'), inline('I4', 'STATUS')]),
          row(5, [inline('B5', 'N2'), inline('C5', 'PCS-06'), inline('D5', 'S1'), blank('H5'), blank('I5')]),
        ]),
      ),
    ]);

    final review = classifyBytes(source, target);
    final rows = rowsOf(review);
    expect(
      rows.any(
        (row) =>
            row['_row'] == 5 &&
            row['proposedRfiNo'] == 'MEC-0123' &&
            row['proposedStatus'] == 'Signed' &&
            row['disposition'] == readyDisposition,
      ),
      isTrue,
      reason: review.toString(),
    );
    expect(
      rows.any(
        (row) =>
            row['_row'] == 0 &&
            obj(obj(row['target'])['worksheet'])['name'] == 'Signed By Worley MEC' &&
            row['disposition'] == manualDisposition &&
            row['reasonKey'] == 'warn.rfi.followUp.reason.partialContext',
      ),
      isTrue,
      reason: review.toString(),
    );
  });

  test('full sync does not duplicate MEC FINAL entries when all exact rows are present', () {
    final source = sourcePackage(
      worksheet([fullSchemaMecHeaderRow()]),
      worksheet([
        fullSchemaMecHeaderRow(),
        fullSchemaMecRecord(10, '0767', 'MEC-1.3.5', 'PCS-06', 'S1', 'N2', 'Completed'),
        fullSchemaMecRecord(11, '0767', 'MEC-1.3.5', 'PCS-06', 'S2', 'N2', 'Completed'),
      ]),
    );
    final target = package([
      (
        'Signed By Worley MEC',
        null,
        worksheet([
          row(3, [inline('H3', 'Final acceptance of pile installation (3.5)')]),
          row(4, [inline('H4', 'RFI No.'), inline('I4', 'STATUS')]),
          row(5, [
            inline('B5', 'N2'),
            inline('C5', 'PCS-06'),
            inline('D5', 'S1'),
            inline('H5', 'MEC-0767'),
            inline('I5', 'Not Signed'),
          ]),
          row(6, [
            inline('B6', 'N2'),
            inline('C6', 'PCS-06'),
            inline('D6', 'S2'),
            inline('H6', 'MEC-0767'),
            inline('I6', 'Not Signed'),
          ]),
        ]),
      ),
    ]);

    final review = classifyBytes(source, target);
    expect(rowsOf(review), isEmpty, reason: review.toString());
  });

  test('full sync reports unrelated MEC FINAL placement in manual review', () {
    final source = sourcePackage(
      worksheet([fullSchemaMecHeaderRow()]),
      worksheet([
        fullSchemaMecHeaderRow(),
        fullSchemaMecRecord(10, '0767', 'MEC-1.3.5', 'PCS-06', 'S2', 'N2', 'Completed'),
      ]),
    );
    final target = package([
      (
        'Signed By Worley MEC',
        null,
        worksheet([
          row(3, [inline('H3', 'Final acceptance of pile installation (3.5)')]),
          row(4, [inline('H4', 'RFI No.'), inline('I4', 'STATUS')]),
          row(5, [
            inline('B5', 'N2'),
            inline('C5', 'PCS-06'),
            inline('D5', 'S1'),
            inline('H5', 'MEC-0767'),
            inline('I5', 'Not Signed'),
          ]),
          row(6, [inline('B6', 'N2'), inline('C6', 'PCS-06'), inline('D6', 'S2'), blank('H6'), blank('I6')]),
        ]),
      ),
    ]);

    final review = classifyBytes(source, target);
    final rows = rowsOf(review);
    expect(rows.length, 1, reason: review.toString());
    expect(rows[0]['disposition'], manualDisposition);
    expect(rows[0]['reasonKey'], 'warn.rfi.followUp.reason.mappingAmbiguous');
  });

  test('full sync keeps ambiguous MEC FINAL contexts in manual review when one exists', () {
    final source = sourcePackage(
      worksheet([fullSchemaMecHeaderRow()]),
      worksheet([
        fullSchemaMecHeaderRow(),
        mecRecordWithRemarks(10, '0767', 'MEC-1.3.5', 'PCS-06', 'S1', 'N2', 'Completed', 'Signed by Worley'),
        mecRecordWithRemarks(11, '0767', 'MEC-1.3.5', 'PCS-06', 'S1', 'N2', 'Completed', 'Signed by Worley'),
      ]),
    );
    final target = package([
      (
        'Signed By Worley MEC',
        null,
        worksheet([
          row(3, [inline('H3', 'Final acceptance of pile installation (3.5)')]),
          row(4, [inline('H4', 'RFI No.'), inline('I4', 'STATUS')]),
          row(5, [
            inline('B5', 'N2'),
            inline('C5', 'PCS-06'),
            inline('D5', 'S1'),
            inline('H5', 'MEC-0767'),
            inline('I5', 'Not Signed'),
          ]),
          row(6, [inline('B6', 'N2'), inline('C6', 'PCS-06'), inline('D6', 'S1'), blank('H6'), blank('I6')]),
        ]),
      ),
    ]);

    final review = classifyBytes(source, target);
    final rows = rowsOf(review);
    expect(
      rows.any(
        (row) =>
            row['rfiNo'] == 'MEC-0767' &&
            row['disposition'] == manualDisposition &&
            row['reasonKey'] == 'warn.rfi.followUp.reason.mappingAmbiguous',
      ),
      isTrue,
      reason: review.toString(),
    );
    expect(
      rows.any(
        (row) =>
            row['rfiNo'] == 'MEC-0767' &&
            (row['disposition'] == readyDisposition ||
                row['enabled'] == true ||
                row['proposedRfiNo'] is String ||
                row['proposedStatus'] is String),
      ),
      isFalse,
      reason: review.toString(),
    );
  });
}
