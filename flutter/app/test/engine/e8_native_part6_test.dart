// Port of src-tauri/tests/rfi_follow_up_native.rs (part 6: missing FINAL
// contexts, conflicting MEC routes, formula-backed targets and plan shape).
import 'package:docflow/engine/rfi_follow_up.dart';
import 'package:flutter_test/flutter_test.dart';

import 'e8_full_schema_support.dart';

void main() {
  test('full sync does not mark a shared MEC Worley group signed from one RFI', () {
    final source = sourcePackage(
      worksheet([fullSchemaMecHeaderRow()]),
      worksheet([
        fullSchemaMecHeaderRow(),
        mecRecordWithRemarks(10, '0020', 'MEC-1.3.5', 'PCS-06', 'S1', 'N2', 'Completed', 'Signed by Worley'),
        fullSchemaMecRecord(11, '0021', 'MEC-1.3.5', 'PCS-06', 'S1', 'N2', 'Completed'),
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
            inline('H5', 'MEC-0020/0021'),
            inline('I5', 'Signed'),
          ]),
        ]),
      ),
    ]);

    final review = classifyBytes(source, target);
    expect(rowsOf(review).length, 1, reason: review.toString());
    expect(rowsOf(review)[0]['proposedStatus'], 'Not Signed');
  });

  test('full sync omits a required FINAL RFI without a prepared context', () {
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
            inline('B5', 'OTHER'),
            inline('C5', 'PCS-OTHER'),
            inline('D5', 'S9'),
            blank('H5'),
            blank('I5'),
          ]),
        ]),
      ),
    ]);

    final review = classifyBytes(source, target);
    expect(
      rowsOf(review).any((row) => row['rfiNo'] == 'MEC-0001'),
      isFalse,
      reason: review.toString(),
    );
    expect(
      (review['warnings'] as List).any(
        (warning) => obj(warning)['key'] == 'warn.rfi.followUp.unmappedRecords',
      ),
      isFalse,
      reason: review.toString(),
    );
  });

  test('conflicting MEC FINAL routes do not authorize shared cell signature writes', () {
    final source = sourcePackage(
      worksheet([fullSchemaMecHeaderRow()]),
      worksheet([
        fullSchemaMecHeaderRow(),
        row(10, [
          inline('A10', 'AZE-SUN-INT-QMS-RFI-MEC-0790'),
          inline('D10', 'AZE-SUN-INT-QMS-ITP-MEC-0003'),
          inline('E10', 'MEC-1.3.1'),
          inline('F10', 'PCS-02'),
          inline('G10', 'S1'),
          inline('N10', 'N1'),
          inline('T10', 'Completed'),
          inline('V10', 'Signed by Worley'),
        ]),
        fullSchemaMecRecordWithItp(11, '0790', (
          'AZE-SUN-INT-QMS-ITP-MEC-0004',
          'MEC-1.3.2',
          'PCS-02',
          'S1',
          'N1',
          'Completed',
        )),
        row(12, [
          inline('A12', 'AZE-SUN-INT-QMS-RFI-MEC-0791'),
          inline('D12', 'AZE-SUN-INT-QMS-ITP-MEC-0003'),
          inline('E12', 'MEC-1.3.1'),
          inline('F12', 'PCS-02'),
          inline('G12', 'S1'),
          inline('N12', 'N1'),
          inline('T12', 'Completed'),
          inline('V12', 'Signed by Worley'),
        ]),
      ]),
    );
    final target = package([
      (
        'Signed By Worley MEC',
        null,
        worksheet([
          row(3, [inline('H3', 'Final acceptance of mechanical installation (3.1)')]),
          row(4, [inline('H4', 'RFI No.'), inline('I4', 'STATUS')]),
          row(5, [
            inline('B5', 'N1'),
            inline('C5', 'PCS-02'),
            inline('D5', 'S1'),
            inline('H5', 'MEC-0790/0791'),
            inline('I5', 'Not Signed'),
          ]),
        ]),
      ),
    ]);

    final review = classifyBytes(source, target);
    final rows = rowsOf(review);
    expect(
      rows.any(
        (row) =>
            row['rfiNo'] == 'MEC-0790' &&
            row['disposition'] == manualDisposition &&
            row['reasonKey'] == 'warn.rfi.followUp.reason.sourceStatusUnknown',
      ),
      isTrue,
      reason: review.toString(),
    );
    expect(
      rows.any(
        (row) =>
            row['rfiNo'] == 'MEC-0790' &&
            (row['disposition'] == readyDisposition ||
                row['enabled'] == true ||
                row['proposedRfiNo'] is String ||
                row['proposedStatus'] is String),
      ),
      isFalse,
      reason: review.toString(),
    );
    expect(
      (review['warnings'] as List).any(
        (warning) => obj(warning)['key'] == 'warn.rfi.followUp.unmappedRecords',
      ),
      isTrue,
    );
  });

  test('full sync keeps formula-backed missing FINAL RFI in manual review', () {
    final source = fullSchemaSource([
      fullSchemaRecord(10, '0001', 'CIV-2.3.1', 'AREA-1', 'Signed by Worley'),
    ]);
    final target = package([
      (
        'Signed By Worley CIV',
        null,
        worksheet([
          row(4, [inline('C4', 'RFI No.'), inline('D4', 'Status')]),
          row(5, [
            inline('B5', 'AREA-1'),
            blank('C5'),
            formula('D5', 'LOOKUP(C5)', 'Not Signed'),
          ]),
        ]),
      ),
    ]);

    final review = classifyBytes(source, target);
    final rows = rowsOf(review);
    expect(rows.length, 1);
    expect(rows[0]['disposition'], manualDisposition);
    expect(rows[0]['reasonKey'], 'warn.rfi.followUp.reason.targetFormula');
    expect(rows[0]['enabled'], false);
  });

  test('full sync ignores formula summary in lifecycle comparison and mapping', () {
    final header = fullSchemaMecHeaderRow();
    final source = sourcePackage(
      worksheet([header]),
      worksheet([
        header,
        mecRecordWithRemarks(10, '0001', 'MEC-1.3.5', 'PCS-01', 'S1', 'N1', 'Completed', 'Signed by Worley'),
        mecRecordWithRemarks(11, '0001', 'MEC-1.3.5', 'PCS-01', 'S2', 'N1', 'Completed', 'Signed by Worley'),
      ]),
    );
    final target = package([
      (
        'MEC',
        null,
        worksheet([
          row(4, [inline('N4', 'RFI No.'), inline('O4', 'STATUS')]),
          row(5, [
            inline('A5', 'Zone 1'),
            inline('B5', 'N1'),
            inline('C5', 'PCS-01'),
            inline('D5', 'S1'),
            inline('N5', 'MEC-0001'),
            inline('O5', 'Completed'),
          ]),
          row(6, [
            inline('A6', 'Zone 1'),
            inline('B6', 'N1'),
            inline('C6', 'PCS-01'),
            inline('D6', 'S2'),
            inline('N6', 'MEC-0001'),
            inline('O6', 'Completed'),
          ]),
        ]),
      ),
      (
        'Signed By Worley MEC',
        null,
        worksheet([
          row(3, [inline('S3', 'RFI No.'), inline('T3', 'Worley Status')]),
          row(4, [
            inline('H4', 'RFI No.'),
            inline('I4', 'STATUS'),
            inline('S4', 'MEC-0001'),
            inline('T4', 'Signed'),
          ]),
          row(5, [
            inline('A5', 'Zone 1'),
            inline('B5', 'N1'),
            inline('C5', 'PCS-01'),
            inline('D5', 'S1'),
            inline('H5', 'MEC-0001'),
            formula('I5', 'T4', 'Signed'),
          ]),
          row(6, [
            inline('A6', 'Zone 1'),
            inline('B6', 'N1'),
            inline('C6', 'PCS-01'),
            inline('D6', 'S2'),
            inline('H6', 'MEC-0001'),
            formula('I6', 'T4', 'Signed'),
          ]),
        ]),
      ),
    ]);

    final review = classifyBytes(source, target);
    expect(rowsOf(review), isEmpty, reason: review.toString());
    expect(
      (review['warnings'] as List).every(
        (warning) => obj(warning)['key'] != 'warn.rfi.followUp.unmappedRecords',
      ),
      isTrue,
    );
  });

  test('full sync omits a FINAL RFI without a prepared context', () {
    final source = fullSchemaSource([
      fullSchemaRecord(10, '0001', 'CIV-2.3.1', 'AREA-1', 'Signed by Worley'),
    ]);
    final target = package([
      (
        'Signed By Worley CIV',
        null,
        worksheet([
          row(4, [inline('C4', 'RFI No.'), inline('D4', 'Status')]),
          row(5, [inline('B5', 'OTHER-AREA'), blank('C5'), blank('D5')]),
        ]),
      ),
    ]);

    final review = classifyBytes(source, target);
    expect(
      rowsOf(review).any((row) => row['rfiNo'] == 'CIV-0001'),
      isFalse,
      reason: review.toString(),
    );
    expect(
      (review['warnings'] as List).any(
        (warning) => obj(warning)['key'] == 'warn.rfi.followUp.unmappedRecords',
      ),
      isFalse,
      reason: review.toString(),
    );
  });

  test('preview builds an inert exact workbook plan', () {
    const rfi = 'AZE-SUN-INT-QMS-RFI-CIV-0006';
    final source = sourcePackage(
      worksheet([row(10, [inline('A10', rfi), inline('T10', 'Completed')])]),
      worksheet(const []),
    );
    final target = targetWithRows([row(2, [inline('A2', rfi), blank('B2')])]);

    final plan = previewBytes(source, target);
    guardedUpdates(plan.steps[0]);

    expect(plan.planId, isEmpty);
    expect(plan.opId, 'rfi.follow_up_sync');
    expect(plan.steps.length, 1);
    expect(plan.steps[0].src, 'Follow Up.xlsx');
    expect(plan.steps[0].dst, contains('.docflow-s5-backup-'));
  });

  // UNVERIFIED: the retained-apply, undo and recovery tests in the Rust file
  // drive FilesystemAuthority (journal, grants, plan cache, apply and undo)
  // on real files. Those modules (E2/E3) are not ported to lib/engine yet, so
  // these tests are carried as skipped entries until they land.
  test(
    'follow up retained apply revalidates open workbooks at every boundary',
    () {},
    skip: 'Requires FilesystemAuthority journal/grant/apply port (E2/E3).',
  );
}
