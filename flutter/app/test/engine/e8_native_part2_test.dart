// Port of src-tauri/tests/rfi_follow_up_native.rs (part 2: scope exclusions,
// CIV multi-tag fan-out and duplicate-row refusal).
import 'package:flutter_test/flutter_test.dart';

import 'e8_full_schema_support.dart';

void main() {
  test('full sync omits confirmed scope exclusions but keeps undecided routes visible', () {
    const exclusions = <(String, String, String)>[
      ('CIV', '0002', 'CIV-2.1.4'),
      ('CIV', '0002', 'CIV-2.1.6'),
      ('CIV', '0005', 'CIV-5.1.4'),
      ('CIV', '0005', 'CIV-5.1.5'),
      ('CIV', '0005', 'CIV-5.1.9'),
      ('CIV', '0005', 'CIV-5.3.4'),
      ('CIV', '0005', 'CIV-5.3.5'),
      ('CIV', '0006', 'CIV-6.1.8'),
      ('CIV', '0006', 'CIV-6.2.1'),
      ('CIV', '0006', 'CIV-6.2.2'),
      ('CIV', '0006', 'CIV-6.2.3'),
      ('CIV', '0006', 'CIV-6.2.4'),
      ('CIV', '0006', 'CIV-6.2.5'),
      ('CIV', '0006', 'CIV-6.2.9'),
      ('CIV', '0007', 'CIV-7.1.5'),
      ('CIV', '0007', 'CIV-7.2.3'),
      ('CIV', '0007', 'CIV-7.2.4'),
      ('CIV', '0007', 'CIV-7.3.9'),
      ('MEC', '0001', 'MEC-1.1.4'),
      ('MEC', '0001', 'MEC-1.2.5'),
      ('MEC', '0004', 'MEC-4.3.1'),
    ];
    final civRecords = <RowSpec>[];
    final mecRecords = <RowSpec>[];
    for (var index = 0; index < exclusions.length; index++) {
      final (discipline, itpNumber, item) = exclusions[index];
      final rowNumber = 10 + index;
      final suffix = (2000 + index).toString().padLeft(4, '0');
      final itp = 'AZE-SUN-INT-QMS-ITP-$discipline-$itpNumber';
      if (discipline == 'CIV') {
        civRecords.add(
          fullSchemaRecordWithStatus(
            rowNumber,
            suffix,
            itp,
            item,
            'AREA-1',
            'Completed',
            '',
          ),
        );
      } else {
        mecRecords.add(
          fullSchemaMecRecordWithItp(rowNumber, suffix, (
            itp,
            item,
            'PCS-01',
            'S1',
            'N1',
            'Completed',
          )),
        );
      }
    }
    civRecords.add(
      fullSchemaRecordWithStatus(
        100,
        '1999',
        'AZE-SUN-INT-QMS-ITP-CIV-0002',
        'CIV-2.1.7',
        'AREA-1',
        'Completed',
        '',
      ),
    );
    final source = sourcePackage(
      worksheet([fullSchemaHeader(), ...civRecords]),
      worksheet([fullSchemaMecHeaderRow(), ...mecRecords]),
    );
    final target = package([
      (
        'CIV',
        null,
        worksheet([
          row(1, [inline('C1', 'RFI No.'), inline('D1', 'STATUS')]),
        ]),
      ),
      (
        'MEC',
        null,
        worksheet([
          row(1, [inline('N1', 'RFI No.'), inline('O1', 'STATUS')]),
        ]),
      ),
    ]);

    final review = classifyBytes(source, target);
    final rows = rowsOf(review);
    expect(rows.length, 1, reason: review.toString());
    expect(rows[0]['rfiNo'], 'CIV-1999');
    expect(rows[0]['reasonKey'], 'warn.rfi.followUp.reason.mappingUnsupported');
  });

  test('full sync keeps multiple matching Follow Up rows in manual review', () {
    final source = fullSchemaSource([
      fullSchemaRecordWithStatus(
        10,
        '0041',
        'AZE-SUN-INT-QMS-ITP-CIV-0002',
        'CIV-3.1.5',
        'AREA-1',
        'Completed',
        '',
      ),
    ]);
    final target = package([
      (
        'CIV',
        null,
        worksheet([
          row(1, [inline('C1', 'RFI No.'), inline('D1', 'STATUS')]),
          row(2, [inline('B2', 'AREA-1'), blank('C2'), blank('D2')]),
          row(3, [inline('B3', 'AREA-1'), blank('C3'), blank('D3')]),
        ]),
      ),
    ]);

    final review = classifyBytes(source, target);
    final rows = rowsOf(review);
    expect(rows.length, 1, reason: review.toString());
    expect(rows[0]['disposition'], manualDisposition);
    expect(rows[0]['reasonKey'], 'warn.rfi.followUp.reason.mappingAmbiguous');
    final details = rows[0]['mappingDetails'] as String?;
    expect(
      details != null &&
          details.contains('row 2: AREA-1') &&
          details.contains('row 3: AREA-1'),
      isTrue,
      reason: review.toString(),
    );
    expect(obj(review['editor'])['detailsDefaultVisible'], true);
  });

  test('full sync fans out a record only across distinct exact CIV area tags', () {
    final source = fullSchemaSource([
      fullSchemaRecordWithStatus(
        10,
        '0041',
        'AZE-SUN-INT-QMS-ITP-CIV-0002',
        'CIV-2.1.5',
        'S14,S15',
        'Completed',
        '',
      ),
    ]);
    final target = package([
      (
        'CIV',
        null,
        worksheet([
          row(1, [inline('C1', 'RFI No.'), inline('D1', 'STATUS')]),
          row(2, [inline('B2', 'S14'), blank('C2'), blank('D2')]),
          row(3, [inline('B3', 'S15'), blank('C3'), blank('D3')]),
        ]),
      ),
    ]);

    final review = classifyBytes(source, target);
    final rows = rowsOf(review);
    expect(rows.length, 2, reason: review.toString());
    expect(
      rows.every(
        (row) =>
            row['disposition'] == readyDisposition &&
            row['rfiNo'] == 'CIV-0041' &&
            row['proposedStatus'] == 'Completed',
      ),
      isTrue,
    );
    expect(review['warnings'] as List, isEmpty);
  });

  test('full sync reports unmatched CIV tag when only some tag rows are prepared', () {
    final source = fullSchemaSource([
      fullSchemaRecordWithStatus(
        10,
        '0041',
        'AZE-SUN-INT-QMS-ITP-CIV-0002',
        'CIV-2.1.5',
        'S14,S15',
        'Completed',
        '',
      ),
    ]);
    final target = package([
      (
        'CIV',
        null,
        worksheet([
          row(1, [inline('C1', 'RFI No.'), inline('D1', 'STATUS')]),
          row(2, [inline('B2', 'S14'), blank('C2'), blank('D2')]),
        ]),
      ),
    ]);

    final review = classifyBytes(source, target);
    final rows = rowsOf(review);
    expect(rows.length, 2, reason: review.toString());
    final ready = rows.firstWhere(
      (row) => row['disposition'] == readyDisposition,
      orElse: () => fail('prepared S14 context remains syncable'),
    );
    expect(ready['_row'], 2);
    expect(ready['rfiNo'], 'CIV-0041');
    expect(ready['proposedStatus'], 'Completed');

    final reviewRow = rows.firstWhere(
      (row) => row['reasonKey'] == 'warn.rfi.followUp.reason.partialContext',
      orElse: () => fail('missing S15 context remains visible'),
    );
    expect(reviewRow['_row'], 0);
    expect(reviewRow['rfiNo'], 'CIV-0041');
    expect(reviewRow['disposition'], manualDisposition);
    expect(reviewRow['selectable'], false);
    final details = reviewRow['mappingDetails'] as String;
    expect(details, contains('matched source context: tag=S14'));
    expect(details, contains('unmatched source context: tag=S15'));
  });

  test('full sync does not fan out across duplicate CIV area rows', () {
    final source = fullSchemaSource([
      fullSchemaRecordWithStatus(
        10,
        '0041',
        'AZE-SUN-INT-QMS-ITP-CIV-0002',
        'CIV-2.1.5',
        'S14,S15',
        'Completed',
        '',
      ),
    ]);
    final target = package([
      (
        'CIV',
        null,
        worksheet([
          row(1, [inline('C1', 'RFI No.'), inline('D1', 'STATUS')]),
          row(2, [inline('B2', 'S14'), blank('C2'), blank('D2')]),
          row(3, [inline('B3', 'S14'), blank('C3'), blank('D3')]),
          row(4, [inline('B4', 'S15'), blank('C4'), blank('D4')]),
        ]),
      ),
    ]);

    final review = classifyBytes(source, target);
    final rows = rowsOf(review);
    expect(rows.length, 1, reason: review.toString());
    expect(rows[0]['reasonKey'], 'warn.rfi.followUp.reason.mappingAmbiguous');
    expect(obj((review['warnings'] as List)[0])['subject'], '1');
  });
}
