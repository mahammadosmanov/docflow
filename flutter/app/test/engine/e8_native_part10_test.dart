// Port of src-tauri/tests/rfi_follow_up_native.rs (part 10: source package
// acceptance rules and multi-sheet Follow Up synchronisation).
import 'package:docflow/engine/rfi_follow_up.dart';
import 'package:flutter_test/flutter_test.dart';

import 'e8_full_schema_support.dart';

void main() {
  test('partial discipline source package without all four sheets is accepted', () {
    const rfi = 'AZE-SUN-INT-QMS-RFI-CIV-0001';
    final source = package([
      (
        'CIV',
        null,
        worksheet([row(10, [inline('A10', rfi), inline('T10', 'Signed by Worley')])]),
      ),
    ]);
    final review = classifyBytes(
      source,
      targetWithRows([row(2, [inline('A2', rfi), blank('B2')])]),
    );
    expect(rowsOf(review).length, 1);
    expect(rowsOf(review)[0]['disposition'], readyDisposition);
    expect(rowsOf(review)[0]['proposedStatus'], 'Signed');
  });

  test('source package with sheet name whitespace and case is accepted', () {
    const rfi = 'AZE-SUN-INT-QMS-RFI-CIV-0001';
    final source = package([
      (
        'Civ ',
        null,
        worksheet([row(10, [inline('A10', rfi), inline('T10', 'Completed')])]),
      ),
    ]);
    final review = classifyBytes(
      source,
      targetWithRows([row(2, [inline('A2', rfi), blank('B2')])]),
    );
    expect(rowsOf(review).length, 1);
    expect(rowsOf(review)[0]['disposition'], readyDisposition);
  });

  test('source package with short RFI code is matched', () {
    final source = package([
      (
        'CIV',
        null,
        worksheet([
          row(10, [inline('A10', 'CIV-0001'), inline('T10', 'Signed by Worley')]),
        ]),
      ),
    ]);
    final review = classifyBytes(
      source,
      targetWithRows([row(2, [inline('A2', 'CIV-0001'), blank('B2')])]),
    );
    expect(rowsOf(review).length, 1);
    expect(rowsOf(review)[0]['disposition'], readyDisposition);
    expect(rowsOf(review)[0]['proposedStatus'], 'Signed');
  });

  test('preview from open files accepts uppercase extension', () {
    const rfi = 'AZE-SUN-INT-QMS-RFI-CIV-0001';
    final source = sourcePackage(
      worksheet([row(10, [inline('A10', rfi), inline('T10', 'Completed')])]),
      worksheet(const []),
    );
    final target = targetWithRows([row(2, [inline('A2', rfi), blank('B2')])]);
    final plan = previewFromOpenFiles(
      rfiLog: FollowUpWorkbookFile(
        path: 'RFI_LOG.XLSX',
        bytes: source,
        identity: const FollowUpFileIdentity.unix(device: 1, inode: 10),
        mtimeNs: 1000,
      ),
      followUp: FollowUpWorkbookFile(
        path: 'FOLLOW_UP.XLSX',
        bytes: target,
        identity: const FollowUpFileIdentity.unix(device: 1, inode: 20),
        mtimeNs: 2000,
      ),
      ooxml: const FakeOoxml(),
      newToken: () => 'a1b2c3d4e5f60718293a4b5c6d7e8f90',
    );
    expect(plan.opId, 'rfi.follow_up_sync');
    expect(plan.steps, isNotEmpty);
  });

  test('source package with single RFI Log sheet is accepted', () {
    const rfi = 'AZE-SUN-INT-QMS-RFI-CIV-0001';
    final source = package([
      (
        'RFI Log',
        null,
        worksheet([
          row(1, [
            inline('A1', 'No'),
            inline('B1', 'RFI No'),
            inline('C1', 'Description'),
            inline('D1', 'Remarks'),
          ]),
          row(2, [
            inline('A2', '1'),
            inline('B2', rfi),
            inline('C2', 'Inspection'),
            inline('D2', 'Signed by Worley'),
          ]),
        ]),
      ),
    ]);
    final review = classifyBytes(
      source,
      targetWithRows([row(2, [inline('A2', rfi), blank('B2')])]),
    );
    expect(rowsOf(review).length, 1);
    expect(rowsOf(review)[0]['disposition'], readyDisposition);
    expect(rowsOf(review)[0]['proposedStatus'], 'Signed');
  });

  test('source package with shifted RFI column in discipline sheet is accepted', () {
    const rfi = 'CIV-0002';
    final source = package([
      (
        'CIVIL WORKS',
        null,
        worksheet([
          row(1, [
            inline('A1', 'Item'),
            inline('B1', 'RFI Code'),
            inline('T1', 'Remarks'),
          ]),
          row(2, [
            inline('A2', '1'),
            inline('B2', rfi),
            inline('T2', 'Signed by Worley'),
          ]),
        ]),
      ),
    ]);
    final review = classifyBytes(
      source,
      targetWithRows([row(2, [inline('A2', rfi), blank('B2')])]),
    );
    expect(rowsOf(review).length, 1);
    expect(rowsOf(review)[0]['disposition'], readyDisposition);
    expect(rowsOf(review)[0]['proposedStatus'], 'Signed');
  });

  test('multi-sheet Follow Up with CIV and MEC is synchronized', () {
    const civRfi = 'CIV-0001';
    const mecRfi = 'MEC-0005';
    final source = package([
      (
        'CIV',
        null,
        worksheet([
          row(1, [inline('A1', 'RFI NO'), inline('T1', 'Remarks')]),
          row(2, [inline('A2', civRfi), inline('T2', 'Signed by Worley')]),
        ]),
      ),
      (
        'MEC',
        null,
        worksheet([
          row(1, [inline('A1', 'RFI NO'), inline('U1', 'Remarks')]),
          row(2, [inline('A2', mecRfi), inline('U2', 'Under review')]),
        ]),
      ),
    ]);
    final target = package([
      (
        'Signed by CIV',
        null,
        worksheet([
          row(1, [inline('A1', 'RFI NO.'), inline('B1', 'STATUS')]),
          row(2, [inline('A2', civRfi), blank('B2')]),
        ]),
      ),
      (
        'Signed by MEC',
        null,
        worksheet([
          row(1, [inline('C1', 'RFI NO.'), inline('D1', 'STATUS')]),
          row(2, [inline('C2', mecRfi), blank('D2')]),
        ]),
      ),
    ]);

    final rows = rowsOf(classifyBytes(source, target));
    expect(rows.length, 2);
    expect(rows[0]['_sheet'], 'Signed by CIV');
    expect(rows[0]['rfiNo'], civRfi);
    expect(rows[0]['proposedStatus'], 'Signed');
    expect(rows[0]['disposition'], readyDisposition);
    expect(rows[1]['_sheet'], 'Signed by MEC');
    expect(rows[1]['rfiNo'], mecRfi);
    expect(rows[1]['proposedStatus'], 'Not Signed');
    expect(rows[1]['disposition'], readyDisposition);

    final plan = previewBytes(source, target);
    expect(plan.steps.length, 1);
    final updates = guardedUpdates(plan.steps[0]);
    expect(updates.containsKey('Signed by CIV'), isTrue);
    expect(updates.containsKey('Signed by MEC'), isTrue);
    expect(updates['Signed by CIV']!.length, 1);
    expect(updates['Signed by MEC']!.length, 1);
  });
}
