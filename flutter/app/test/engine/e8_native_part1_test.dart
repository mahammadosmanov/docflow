// Port of src-tauri/tests/rfi_follow_up_native.rs (part 1: mixed groups,
// MEC header variants, CIV PCS exclusion, MEC multi-area, context hiding).
import 'package:docflow/engine/rfi_follow_up.dart';
import 'package:flutter_test/flutter_test.dart';

import 'e8_full_schema_support.dart';

void main() {
  test('mixed completed groups update status without writing remarks', () {
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
        'Ongoing',
        '',
      ),
      fullSchemaRecordWithStatus(
        12,
        '1224',
        'AZE-SUN-INT-QMS-ITP-CIV-0005',
        'CIV-3.6',
        'PCS-01',
        'Completed',
        '',
      ),
      fullSchemaRecordWithStatus(
        13,
        '1225',
        'AZE-SUN-INT-QMS-ITP-CIV-0005',
        'CIV-3.6',
        'PCS-01',
        'Ongoing',
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
            inline('D2', 'Pending'),
            inline(
              'Q2',
              'operator note\nRFI Status Sync: CIV-1222 In Progress, CIV-1223 In Progress',
            ),
          ]),
        ]),
      ),
      (
        'PCU',
        null,
        worksheet([
          row(3, [
            inline('P3', 'Final acceptance of PCU foundation (3.6)'),
            inline('R3', 'Signed by Worley'),
          ]),
          row(4, [
            inline('P4', 'RFI No.'),
            inline('Q4', 'STATUS'),
            inline('U4', 'Remarks'),
          ]),
          row(5, [
            inline('B5', 'N1'),
            inline('C5', 'PCS-01'),
            inline('P5', 'CIV-1224/1225'),
            inline('Q5', 'Pending'),
            inline('R5', 'Signed'),
          ]),
        ]),
      ),
      (
        'MEC',
        null,
        worksheet([
          row(1, [
            inline('B1', 'Tag'),
            inline('C1', 'RFI No.'),
            inline('D1', 'STATUS'),
            inline('Q1', 'Remarks'),
          ]),
          row(2, [
            inline('B2', 'OTHER'),
            inline('C2', 'CIV-1222/1223'),
            inline('D2', 'Pending'),
            inline('Q2', 'operator note'),
          ]),
        ]),
      ),
    ]);

    final review = classifyBytes(source, target);
    final rows = rowsOf(review);
    final civ = rows.firstWhere(
      (row) => row['_sheet'] == 'CIV' && row['_row'] == 2,
    );
    final pcu = rows.firstWhere(
      (row) => row['_sheet'] == 'PCU' && obj(row['target'])['statusColumn'] == 17,
    );
    expect(civ['proposedStatus'], 'In Progress');
    expect(pcu['proposedStatus'], '');
    expect(pcu['proposedRfiNo'], '');
    expect(
      rows.every(
        (row) =>
            !row.containsKey('currentRemark') &&
            !row.containsKey('proposedRemark') &&
            !row.containsKey('remarkCellAddress') &&
            !obj(row['target']).containsKey('remarkColumn') &&
            !obj(row['target']).containsKey('remarkCell') &&
            !obj(row['target']).containsKey('remarkCellExists'),
      ),
      isTrue,
    );

    final plan = previewBytes(source, target);
    final updates = guardedUpdates(plan.steps.first);
    expect(updates['CIV']?.containsKey((2, 17)) ?? false, isFalse);
    expect(updates['PCU']?.containsKey((5, 16)) ?? false, isTrue);
    expect(updates['PCU']?.containsKey((5, 17)) ?? false, isTrue);
  });

  test('full sync recognizes the MEC inspection location header variant', () {
    final civ = worksheet([
      row(9, [
        inline('A9', 'RFI No.'),
        inline('D9', 'ITP No:'),
        inline('E9', 'Item No.'),
        inline('F9', 'Tag No'),
        inline('G9', 'Area'),
        inline('N9', 'Inspection Location'),
        inline('T9', 'Inspection Status'),
        inline('V9', 'Remarks'),
      ]),
    ]);
    final mec = worksheet([
      row(9, [
        inline('A9', 'RFI No.'),
        inline('D9', 'ITP No:'),
        inline('E9', 'Item No.'),
        inline('F9', 'Tag No'),
        inline('G9', 'Area'),
        inline('O9', 'INSP. LOC./AREA'),
        inline('Q9', 'Inspection Status'),
        inline('W9', 'Remarks'),
      ]),
      row(10, [
        inline('A10', 'AZE-SUN-INT-QMS-RFI-MEC-0791'),
        inline('D10', 'AZE-SUN-INT-QMS-ITP-MEC-0003'),
        inline('E10', 'MEC-3.3.1'),
        inline('F10', 'PCS-07'),
        inline('G10', 'S3'),
        inline('O10', 'N10'),
        inline('Q10', 'Ongoing'),
        blank('W10'),
      ]),
    ]);
    final source = sourcePackage(civ, mec);
    final target = package([
      (
        'MEC',
        null,
        worksheet([
          row(3, [inline('P3', 'FINAL')]),
          row(4, [inline('P4', 'RFI No.'), inline('Q4', 'STATUS')]),
          row(5, [
            inline('B5', 'N10'),
            inline('C5', 'PCS-07'),
            inline('D5', 'S3'),
            blank('P5'),
            blank('Q5'),
          ]),
        ]),
      ),
    ]);

    final review = classifyBytes(source, target);
    final row5 = whereRows(
      review,
      (row) => row['_row'] == 5 && row['disposition'] == readyDisposition,
    ).single;
    expect(row5['rfiNo'], 'MEC-0791');
    expect(row5['_sheet'], 'MEC');
    expect(row5['_row'], 5);
    expect(row5['proposedStatus'], 'In Progress');
    expect(row5['disposition'], readyDisposition);
  });

  test('full sync does not route CIV PCS final tag into PCU section', () {
    final source = fullSchemaSource([pcuFinalRecord(10, 'Signed by Worley')]);
    final target = package([('PCU', null, pcuTarget('', '', ''))]);

    final review = classifyBytes(source, target);
    expect(rowsOf(review), isEmpty, reason: review.toString());
  });

  test('full sync keeps literal PCU-tagged CIV final in scope', () {
    final source = fullSchemaSource([
      RowSpec(10, pcuFinalRecord(10, 'Signed by Worley').cells.map((cell) {
        return CellSpec(
          cell.reference,
          cell.value.replaceAll('PCS-01', 'PCU-01'),
          cell.formula,
        );
      }).toList()),
    ]);
    final target = package([('PCU', null, pcuTarget('', '', ''))]);

    final review = classifyBytes(source, target);
    final rows = rowsOf(review);
    expect(rows, isNotEmpty, reason: review.toString());
    expect(rows.any((row) => row['proposedRfiNo'] == 'CIV-1001'), isTrue);
  });

  test('full sync excludes every CIV PCS-tagged RFI from Follow Up', () {
    for (final tag in ['PCS-19', 'PCS-19/S1,S2,S3']) {
      final source = fullSchemaSource([
        fullSchemaRecordWithStatus(
          10,
          '1401',
          'AZE-SUN-INT-QMS-ITP-CIV-0002',
          'CIV-2.2.4',
          tag,
          'Ongoing',
          '',
        ),
      ]);
      final target = package([
        (
          'MEC',
          null,
          worksheet([
            row(5, [inline('A5', 'Zone 4'), inline('C5', 'PCS-19')]),
          ]),
        ),
        (
          'CIV',
          null,
          worksheet([
            row(1, [inline('I1', 'RFI No.'), inline('J1', 'STATUS')]),
            row(20, [
              inline('A20', 'Zone 4'),
              inline('B20', 'S1'),
              inline('I20', ''),
              inline('J20', ''),
            ]),
          ]),
        ),
        ('PCU', null, pcuTarget('', '', '')),
      ]);

      final review = classifyBytes(source, target);
      expect(rowsOf(review), isEmpty, reason: 'tag=$tag: $review');
    }

    final source = fullSchemaSource([
      fullSchemaRecordWithStatus(
        10,
        '1401',
        'AZE-SUN-INT-QMS-ITP-CIV-0002',
        'CIV-2.2.4',
        'PCS-19/S1,S2,S3',
        'Ongoing',
        '',
      ),
    ]);
    final target = package([
      (
        'CIV',
        null,
        worksheet([
          row(1, [inline('I1', 'RFI No.'), inline('J1', 'STATUS')]),
          row(20, [
            inline('A20', 'Zone 4'),
            inline('B20', 'S1'),
            inline('I20', 'CIV-1401'),
            inline('J20', 'In Progress'),
          ]),
        ]),
      ),
    ]);
    final review = classifyBytes(source, target);
    final rows = rowsOf(review);
    expect(rows.length, 1, reason: review.toString());
    expect(rows[0]['proposedRfiNo'], '');
    expect(rows[0]['proposedStatus'], '');
  });

  test('full sync maps MEC multi-area RFI to distinct rows without inspection location', () {
    final source = sourcePackage(
      worksheet([fullSchemaMecHeaderRow()]),
      worksheet([
        fullSchemaMecHeaderRow(),
        fullSchemaMecRecordWithItp(10, '0790', (
          'AZE-SUN-INT-QMS-ITP-MEC-0003',
          'MEC-3.3.1',
          'PCS-21',
          'S1,S2,S4',
          'N99',
          'Ongoing',
        )),
        fullSchemaMecRecordWithItp(11, '0786', (
          'AZE-SUN-INT-QMS-ITP-MEC-0004',
          'MEC-4.3.2',
          'PCS-08',
          'S4',
          'N11',
          'Ongoing',
        )),
      ]),
    );
    final target = package([
      (
        'MEC',
        null,
        worksheet([
          row(4, [
            inline('P4', 'RFI No.'),
            inline('Q4', 'STATUS'),
            inline('R4', 'RFI No.'),
            inline('S4', 'STATUS'),
          ]),
          row(5, [
            inline('B5', 'S2'),
            inline('C5', 'PCS-21'),
            inline('D5', 'S1'),
            blank('P5'),
            blank('Q5'),
          ]),
          row(6, [
            inline('B6', 'S2'),
            inline('C6', 'PCS-21'),
            inline('D6', 'S2'),
            blank('P6'),
            blank('Q6'),
          ]),
          row(7, [
            inline('B7', 'S2'),
            inline('C7', 'PCS-21'),
            inline('D7', 'S4'),
            blank('P7'),
            blank('Q7'),
          ]),
          row(8, [
            inline('B8', 'N10/N11'),
            inline('C8', 'PCS-08'),
            inline('D8', 'S4'),
            blank('R8'),
            blank('S8'),
          ]),
        ]),
      ),
    ]);

    final review = classifyBytes(source, target);
    final rows = rowsOf(review);
    final mec0790 = whereRows(
      review,
      (row) =>
          row['rfiNo'] == 'MEC-0790' && row['disposition'] == readyDisposition,
    ).toList();
    expect(mec0790.length, 3, reason: review.toString());
    expect(mec0790.map((row) => row['_row']).toList(), [5, 6, 7]);
    final mec0786 = whereRows(
      review,
      (row) =>
          row['rfiNo'] == 'MEC-0786' && row['disposition'] == readyDisposition,
    ).single;
    expect(mec0786['_row'], 8);
    expect(obj(mec0786['target'])['rfiColumn'], 18);
    expect(
      rows
          .where((row) => row['disposition'] == readyDisposition)
          .every((row) => row['proposedStatus'] == 'In Progress'),
      isTrue,
    );
  });

  test('full sync hides a supported route without a prepared Follow Up context', () {
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
        ]),
      ),
    ]);

    final review = classifyBytes(source, target);
    expect(rowsOf(review), isEmpty, reason: review.toString());
  });
}
