// Port of src-tauri/tests/rfi_follow_up_native.rs (part 8: resource budgets
// for source evidence, projection and merge queries).
import 'package:flutter_test/flutter_test.dart';

import 'e8_full_schema_support.dart';
import 'e8_support.dart';

void main() {
  test('source evidence per-RFI limit fails before projection amplification', () {
    const rfi = 'AZE-SUN-INT-QMS-RFI-CIV-0006';
    final civRows = [
      for (var number = 10; number < 510; number++)
        row(number, [inline('A$number', rfi), inline('T$number', 'Completed')]),
    ];
    final mec = worksheet([row(10, [inline('A10', rfi), inline('U10', 'Completed')])]);

    expect(
      () => classifyBytes(
        sourcePackage(worksheet(civRows), mec),
        targetWithRows([row(2, [inline('A2', rfi), blank('B2')])]),
      ),
      throwsBackendMessage('error.rfi.followUp.limitExceeded'),
    );
  });

  test('projection budget rejects 500 by 500 before owned materialization', () {
    const rfi = 'AZE-SUN-INT-QMS-RFI-CIV-0006';
    final sourceRows = [
      for (var number = 10; number < 510; number++)
        row(number, [inline('A$number', rfi), inline('T$number', 'Completed')]),
    ];
    final targetRows = [
      for (var number = 2; number < 502; number++)
        row(number, [inline('A$number', rfi), blank('B$number')]),
    ];

    expect(
      () => classifyBytes(
        sourcePackage(worksheet(sourceRows), worksheet(const [])),
        targetWithRows(targetRows),
      ),
      throwsBackendMessage('error.rfi.followUp.limitExceeded'),
    );
  });

  test('merge query budget bounds duplicate target range amplification', () {
    const rfi = 'AZE-SUN-INT-QMS-RFI-CIV-0006';
    final sourceRows = [
      for (var number = 10; number < 310; number++)
        row(number, [inline('A$number', rfi), inline('T$number', 'Completed')]),
    ];
    final targetRows = [
      row(1, [inline('A1', 'RFI NO.'), inline('B1', 'STATUS')]),
      for (var number = 2; number < 502; number++)
        row(number, [inline('A$number', rfi), blank('B$number')]),
    ];
    final merges = [for (var number = 1000; number < 2414; number++) 'Z$number:Z$number'];

    expect(
      () => classifyBytes(
        sourcePackage(worksheet(sourceRows, merges), worksheet(const [])),
        targetPackage(worksheet(targetRows, merges)),
      ),
      throwsBackendMessage('error.rfi.followUp.limitExceeded'),
    );
  });

  // UNVERIFIED: the 501-RFI guarded apply test drives the apply pipeline over
  // a real workbook through the authority (E2/E3); carried as skipped.
  test(
    'follow up sync applies more than five hundred guarded cell updates',
    () {},
    skip: 'Requires FilesystemAuthority apply port (E2/E3) and real OOXML patching (E6).',
  );

  // UNVERIFIED: the undo and recovery tests need the retained-backup journal,
  // FilesystemAuthority apply/undo and restart recovery (E2/E3); carried as skipped.
  test(
    'follow up retained undo restores exact backup and preserves all evidence',
    () {},
    skip: 'Requires FilesystemAuthority undo and journal port (E2/E3).',
  );

  test(
    'follow up retained recovery completes exact applied state after restart',
    () {},
    skip: 'Requires FilesystemAuthority recovery and journal port (E2/E3).',
  );
}
