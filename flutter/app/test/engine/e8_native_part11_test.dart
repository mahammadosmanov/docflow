// Port of src-tauri/tests/rfi_follow_up_native.rs (part 11: sheet priority,
// placeholder RFIs, road tags, MEC section/block matching and isolation).
import 'package:docflow/engine/rfi_follow_up.dart';
import 'package:flutter_test/flutter_test.dart';

import 'e8_full_schema_support.dart';

void main() {
  test('multi-sheet Follow Up prioritizes Signed By sheets over arbitrary sheets', () {
    const civRfi = 'CIV-0001';
    final source = package([
      (
        'CIV',
        null,
        worksheet([
          row(1, [inline('A1', 'RFI NO'), inline('T1', 'Remarks')]),
          row(2, [inline('A2', civRfi), inline('T2', 'Signed by Worley')]),
        ]),
      ),
    ]);
    final target = package([
      (
        'Arbitrary Notes',
        null,
        worksheet([row(1, [inline('A1', 'RFI NO.'), inline('B1', 'STATUS')])]),
      ),
      (
        'Signed by CIV',
        null,
        worksheet([
          row(4, [inline('C4', 'RFI NO.'), inline('D4', 'STATUS')]),
          row(5, [inline('C5', civRfi), blank('D5')]),
        ]),
      ),
    ]);

    final rows = rowsOf(classifyBytes(source, target));
    expect(rows.length, 1);
    expect(rows[0]['_sheet'], 'Signed by CIV');
    expect(rows[0]['rfiNo'], civRfi);
    expect(rows[0]['proposedStatus'], 'Signed');
  });

  test('placeholder 2000-series RFIs are preserved without sourceMissing warnings', () {
    expect(isPlaceholderRfi('CIV-2007'), isTrue);
    expect(isPlaceholderRfi('MEC-2183'), isTrue);
    expect(isPlaceholderRfi('CIV-0170'), isFalse);
    expect(isPlaceholderRfi('MEC-0330'), isFalse);
  });

  test('road name matches by exact tag without requiring zone match', () {
    final source = fullSchemaSource([
      fullSchemaRecordWithStatus(
        10,
        '0001',
        'AZE-SUN-INT-QMS-ITP-CIV-0004',
        'CIV-4.2.4',
        'Road 14 (Part 1)',
        'Ongoing',
        '',
      ),
    ]);
    final target = package([
      (
        'CIV',
        null,
        worksheet([row(4, [inline('C4', 'RFI No.'), inline('D4', 'STATUS')])]),
      ),
      (
        'MEC',
        null,
        worksheet([row(4, [inline('H4', 'RFI No.'), inline('I4', 'STATUS')])]),
      ),
      (
        'Road',
        null,
        worksheet([
          row(4, [
            inline('B4', 'ROAD No.'),
            inline('K4', 'RFI No.'),
            inline('L4', 'STATUS'),
          ]),
          row(5, [
            inline('A5', 'South'),
            inline('B5', 'Road 14 (Part 1)'),
            inline('K5', ''),
            inline('L5', ''),
          ]),
        ]),
      ),
    ]);
    final rows = rowsOf(classifyBytes(source, target));
    expect(rows.length, 1);
    expect(rows[0]['_sheet'], 'Road');
    expect(rows[0]['_row'], 5);
    expect(rows[0]['proposedRfiNo'], 'CIV-0001');
    expect(rows[0]['proposedStatus'], 'In Progress');
  });

  test('multi-section MEC signed RFI uses section and block without location', () {
    final source = sourcePackage(
      worksheet([fullSchemaMecHeaderRow()]),
      worksheet([
        fullSchemaMecHeaderRow(),
        row(10, [
          inline('A10', 'AZE-SUN-INT-QMS-RFI-MEC-0001'),
          inline('D10', 'AZE-SUN-INT-QMS-ITP-MEC-0003'),
          inline('E10', 'MEC-3.3.1'),
          inline('F10', 'PCS-10'),
          inline('G10', 'S1, S3'),
          inline('N10', 'N99'),
          inline('T10', 'Completed'),
          inline('V10', 'Signed by Worley'),
        ]),
      ]),
    );
    final target = package([
      (
        'CIV',
        null,
        worksheet([row(4, [inline('C4', 'RFI No.'), inline('D4', 'STATUS')])]),
      ),
      (
        'MEC',
        null,
        worksheet([row(4, [inline('H4', 'RFI No.'), inline('I4', 'STATUS')])]),
      ),
      (
        'Signed By Worley MEC',
        null,
        worksheet([
          row(4, [
            inline('C4', 'Block #'),
            inline('D4', 'Section'),
            inline('J4', 'RFI No.'),
            inline('K4', 'STATUS'),
          ]),
          row(5, [
            inline('B5', 'N1'),
            inline('C5', 'PCS-10'),
            inline('D5', 'S1'),
            inline('J5', ''),
            inline('K5', ''),
          ]),
          row(6, [
            inline('C6', 'PCS-10'),
            inline('D6', 'S2'),
            inline('J6', 'MEC-0558'),
            inline('K6', 'Signed'),
          ]),
          row(7, [
            inline('B7', 'N1'),
            inline('C7', 'PCS-10'),
            inline('D7', 'S3'),
            inline('J7', ''),
            inline('K7', ''),
          ]),
        ]),
      ),
    ]);
    final review = classifyBytes(source, target);
    expect(review['warnings'] as List, isEmpty);
    final rows = rowsOf(review);
    expect(rows.length, 2);
    expect(rows[0]['_sheet'], 'Signed By Worley MEC');
    expect(rows[0]['_row'], 5);
    expect(rows[0]['proposedRfiNo'], 'MEC-0001');
    expect(rows[0]['proposedStatus'], 'Signed');
    expect(rows[1]['_sheet'], 'Signed By Worley MEC');
    expect(rows[1]['_row'], 7);
    expect(rows[1]['proposedRfiNo'], 'MEC-0001');
    expect(rows[1]['proposedStatus'], 'Signed');
  });

  test('full sync maps signed MEC without inspection location columns', () {
    final source = sourcePackage(
      worksheet([fullSchemaMecHeaderRow()]),
      worksheet([
        fullSchemaMecHeaderRow(),
        row(10, [
          inline('A10', 'AZE-SUN-INT-QMS-RFI-MEC-0001'),
          inline('D10', 'AZE-SUN-INT-QMS-ITP-MEC-0003'),
          inline('E10', 'MEC-3.3.1'),
          inline('F10', 'PCS-10'),
          inline('G10', 'S1, S3'),
          inline('T10', 'Completed'),
          inline('V10', 'Signed by Worley'),
        ]),
      ]),
    );
    final target = package([
      (
        'MEC',
        null,
        worksheet([row(4, [inline('H4', 'RFI No.'), inline('I4', 'STATUS')])]),
      ),
      (
        'Signed By Worley MEC',
        null,
        worksheet([
          row(4, [
            inline('C4', 'Block #'),
            inline('D4', 'Section'),
            inline('J4', 'RFI No.'),
            inline('K4', 'STATUS'),
          ]),
          row(5, [
            inline('C5', 'PCS-10'),
            inline('D5', 'S1'),
            inline('J5', ''),
            inline('K5', ''),
          ]),
          row(6, [
            inline('C6', 'PCS-10'),
            inline('D6', 'S3'),
            inline('J6', ''),
            inline('K6', ''),
          ]),
        ]),
      ),
    ]);
    final review = classifyBytes(source, target);
    final rows = rowsOf(review);
    expect(rows.length, 2, reason: review.toString());
    expect(
      rows.any(
        (row) =>
            row['_sheet'] == 'Signed By Worley MEC' &&
            row['_row'] == 5 &&
            row['rfiNo'] == 'MEC-0001' &&
            row['proposedStatus'] == 'Signed',
      ),
      isTrue,
    );
    expect(
      rows.any(
        (row) =>
            row['_sheet'] == 'Signed By Worley MEC' &&
            row['_row'] == 6 &&
            row['rfiNo'] == 'MEC-0001' &&
            row['proposedStatus'] == 'Signed',
      ),
      isTrue,
    );
  });

  test('RFIs of different items or ITPs are strictly isolated and never merge into the same slot', () {
    const trackerRfi = 'AZE-SUN-INT-QMS-RFI-MEC-0370';
    const moduleRfi = 'AZE-SUN-INT-QMS-RFI-MEC-0530';
    final source = sourcePackage(
      worksheet([fullSchemaMecHeaderRow()]),
      worksheet([
        fullSchemaMecHeaderRow(),
        row(10, [
          inline('A10', trackerRfi),
          inline('D10', 'AZE-SUN-INT-QMS-ITP-MEC-0003'),
          inline('E10', 'MEC-3.3.1'),
          inline('F10', 'PCS-02'),
          inline('G10', 'S1'),
          inline('T10', 'Completed'),
          inline('V10', 'Signed by Worley'),
        ]),
        row(11, [
          inline('A11', moduleRfi),
          inline('D11', 'AZE-SUN-INT-QMS-ITP-MEC-0004'),
          inline('E11', 'MEC-4.3.2'),
          inline('F11', 'PCS-02'),
          inline('G11', 'S1'),
          inline('T11', 'Completed'),
          inline('V11', 'Signed by Worley'),
        ]),
      ]),
    );
    final target = package([
      (
        'CIV',
        null,
        worksheet([row(4, [inline('C4', 'RFI No.'), inline('D4', 'STATUS')])]),
      ),
      (
        'MEC',
        null,
        worksheet([
          row(4, [
            inline('C4', 'Block #'),
            inline('D4', 'Section'),
            inline('P4', 'RFI No.'),
            inline('Q4', 'STATUS'),
            inline('R4', 'RFI No.'),
            inline('S4', 'STATUS'),
          ]),
          row(5, [
            inline('C5', 'PCS-02'),
            inline('D5', 'S1'),
            inline('P5', 'MEC-0370'),
            inline('Q5', 'Completed'),
            inline('R5', 'MEC-0530'),
            inline('S5', 'Completed'),
          ]),
        ]),
      ),
    ]);
    final review = classifyBytes(source, target);
    final rows = rowsOf(review);
    expect(
      rows.every(
        (row) =>
            row['disposition'] != readyDisposition &&
            row['proposedRfiNo'] == null &&
            row['proposedStatus'] == null,
      ),
      isTrue,
      reason: review.toString(),
    );
    expect(
      (review['warnings'] as List).any(
        (warning) => obj(warning)['key'] == 'warn.rfi.followUp.unmappedRecords',
      ),
      isTrue,
    );
  });
}
