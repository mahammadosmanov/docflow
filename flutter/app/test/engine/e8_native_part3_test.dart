// Port of src-tauri/tests/rfi_follow_up_native.rs (part 3: MEC route-context
// fan-out, duplicate MEC contexts and inspection-location independence).
import 'package:flutter_test/flutter_test.dart';

import 'e8_full_schema_support.dart';

void main() {
  test('full sync fans out MEC RFI only across distinct exact route contexts', () {
    final source = sourcePackage(
      worksheet([fullSchemaMecHeaderRow()]),
      worksheet([
        fullSchemaMecHeaderRow(),
        fullSchemaMecRecord(10, '0120', 'MEC-1.3.5', 'PCS-06', 'S3', 'N2', 'Completed'),
        fullSchemaMecRecord(11, '0120', 'MEC-1.3.5', 'PCS-06', 'S4', 'N2', 'Completed'),
      ]),
    );
    final target = package([
      (
        'MEC',
        null,
        worksheet([
          mecTargetHeader(),
          mecTargetRow(2, 'N2', 'PCS-06', 'S3'),
          mecTargetRow(3, 'N2', 'PCS-06', 'S4'),
        ]),
      ),
    ]);

    final review = classifyBytes(source, target);
    final readyRows = whereRows(
      review,
      (row) => row['disposition'] == readyDisposition,
    ).toList();
    expect(readyRows.length, 2, reason: review.toString());
    expect(
      readyRows.every(
        (row) => row['rfiNo'] == 'MEC-0120' && row['proposedStatus'] == 'Completed',
      ),
      isTrue,
    );
    expect(
      (review['warnings'] as List).any(
        (warning) => obj(warning)['key'] == 'warn.rfi.followUp.unmappedRecords',
      ),
      isTrue,
    );
  });

  test('full sync updates prepared MEC context and reviews unprepared context', () {
    final source = sourcePackage(
      worksheet([fullSchemaMecHeaderRow()]),
      worksheet([
        fullSchemaMecHeaderRow(),
        fullSchemaMecRecord(10, '0121', 'MEC-1.3.5', 'PCS-06', 'S1,S2', 'N2', 'Completed'),
      ]),
    );
    final target = package([
      (
        'MEC',
        null,
        worksheet([
          mecTargetHeader(),
          row(2, [inline('B2', 'N2'), inline('C2', 'PCS-06'), inline('D2', 'S1'), blank('N2'), blank('O2')]),
        ]),
      ),
    ]);

    final review = classifyBytes(source, target);
    final rows = rowsOf(review);
    final readyRows = rows.where((row) => row['disposition'] == readyDisposition).toList();
    expect(readyRows.length, 1, reason: review.toString());
    expect(readyRows[0]['_row'], 2);
    expect(readyRows[0]['rfiNo'], 'MEC-0121');
    expect(readyRows[0]['proposedStatus'], 'Completed');

    final missingRows = rows
        .where(
          (row) =>
              row['_sheet'] == 'MEC' &&
              row['reasonKey'] == 'warn.rfi.followUp.reason.partialContext',
        )
        .toList();
    expect(missingRows.length, 1, reason: review.toString());
    expect(missingRows[0]['_row'], 0);
    expect(missingRows[0]['rfiNo'], 'MEC-0121');
    expect(obj(obj(missingRows[0]['target'])['worksheet'])['name'], 'MEC');
    expect(missingRows[0]['disposition'], manualDisposition);
    final details = missingRows[0]['mappingDetails'] as String;
    expect(details, contains('unmatched source context: tag=PCS-06; area=S2'));
    expect(details, contains('matched source context: tag=PCS-06; area=S1'));
    expect(details, contains('row 2: PCS-06 / S1'));
    expect(rows.every((row) => row['_row'] != 3), isTrue);
  });

  test('full sync explains unmatched MEC FINAL context for RFI Log correction', () {
    final source = sourcePackage(
      worksheet([fullSchemaMecHeaderRow()]),
      worksheet([
        fullSchemaMecHeaderRow(),
        mecRecordWithRemarks(10, '0123', 'MEC-1.3.5', 'PCS-06', 'S1', 'N2', 'Completed', 'Signed by Worley'),
        mecRecordWithRemarks(11, '0123', 'MEC-1.3.5', 'PCS-06', 'S2', 'N3', 'Completed', 'Signed by Worley'),
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
    final warning = rowsOf(review).firstWhere(
      (row) => row['_row'] == 0,
      orElse: () => fail('missing MEC FINAL context remains visible'),
    );
    final details = warning['mappingDetails'] as String;
    expect(warning['reasonKey'], 'warn.rfi.followUp.reason.partialContext');
    expect(details, contains('unmatched source context: tag=PCS-06; area=S2'));
    expect(details, contains('matched source context: tag=PCS-06; area=S1'));
    expect(details, contains('row 5: PCS-06 / S1'));
  });

  test('full sync does not use inspection location to distinguish MEC contexts', () {
    final source = sourcePackage(
      worksheet([fullSchemaMecHeaderRow()]),
      worksheet([
        fullSchemaMecHeaderRow(),
        fullSchemaMecRecord(10, '0122', 'MEC-1.3.5', 'PCS-06', 'S1', 'N2', 'Completed'),
        fullSchemaMecRecord(11, '0122', 'MEC-1.3.5', 'PCS-06', 'S1', 'N3', 'Completed'),
      ]),
    );
    final target = package([
      (
        'MEC',
        null,
        worksheet([
          mecTargetHeader(),
          row(2, [inline('B2', 'N2'), inline('C2', 'PCS-06'), inline('D2', 'S1'), blank('N2'), blank('O2')]),
          row(3, [inline('B3', 'N3'), inline('C3', 'PCS-06'), inline('D3', 'S1'), blank('N3'), blank('O3')]),
        ]),
      ),
    ]);

    final review = classifyBytes(source, target);
    final rows = rowsOf(review);
    expect(rows.length, 1, reason: review.toString());
    expect(rows[0]['disposition'], manualDisposition);
    expect(rows[0]['reasonKey'], 'warn.rfi.followUp.reason.mappingAmbiguous');
    expect(
      rows[0]['mappingDetails'] as String,
      contains('candidate target rows: row 2: PCS-06 / S1 | row 3: PCS-06 / S1'),
    );
  });

  test('full sync keeps duplicate MEC route context rows in manual review', () {
    final source = sourcePackage(
      worksheet([fullSchemaMecHeaderRow()]),
      worksheet([
        fullSchemaMecHeaderRow(),
        fullSchemaMecRecord(10, '0120', 'MEC-1.3.5', 'PCS-06', 'S3', 'N2', 'Completed'),
        fullSchemaMecRecord(11, '0120', 'MEC-1.3.5', 'PCS-06', 'S4', 'N2', 'Completed'),
      ]),
    );
    final target = package([
      (
        'MEC',
        null,
        worksheet([
          mecTargetHeader(),
          mecTargetRow(2, 'N2', 'PCS-06', 'S3'),
          mecTargetRow(3, 'N2', 'PCS-06', 'S3'),
          mecTargetRow(4, 'N2', 'PCS-06', 'S4'),
        ]),
      ),
    ]);

    final review = classifyBytes(source, target);
    final rows = rowsOf(review);
    expect(rows.length, 1, reason: review.toString());
    expect(rows[0]['reasonKey'], 'warn.rfi.followUp.reason.mappingAmbiguous');
  });
}
