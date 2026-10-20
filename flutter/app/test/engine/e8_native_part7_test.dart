// Port of src-tauri/tests/rfi_follow_up_native.rs (part 7: revise, duplicate
// guarded coordinates, RFI canonicalisation and source duplicate status).
import 'dart:typed_data';

import 'package:docflow/engine/contracts.dart';
import 'package:docflow/engine/rfi_follow_up.dart';
import 'package:flutter_test/flutter_test.dart';

import 'e8_full_schema_support.dart';

/// Source (RFI Log) and target (Follow Up) for the open-file preview tests.
Uint8List _revisionSource(String rfi) => sourcePackage(
  worksheet([row(10, [inline('A10', rfi), inline('T10', 'Completed')])]),
  worksheet(const []),
);

FollowUpWorkbookFile _logFile(Uint8List bytes) => FollowUpWorkbookFile(
  path: 'RFI Log.xlsx',
  bytes: bytes,
  identity: const FollowUpFileIdentity.unix(device: 1, inode: 10),
  mtimeNs: 1000,
);

FollowUpWorkbookFile _followUpFile(Uint8List bytes) => FollowUpWorkbookFile(
  path: 'Follow Up.xlsx',
  bytes: bytes,
  identity: const FollowUpFileIdentity.unix(device: 1, inode: 20),
  mtimeNs: 2000,
);

List<Map<String, Object?>> _editorRows(Plan plan) =>
    (obj(plan.meta['editor'])['rows']! as List)
        .map((row) => Map<String, Object?>.from(row as Map<Object?, Object?>))
        .toList();

void main() {
  test('revise from open files changes only enabled and preserves plan identity', () {
    const rfi = 'AZE-SUN-INT-QMS-RFI-CIV-0006';
    final source = _revisionSource(rfi);
    final target = targetWithRows([row(2, [inline('A2', rfi), blank('B2')])]);
    final original = previewBytes(source, target);
    final plan = Plan(
      planId: 'preserved-plan',
      opId: original.opId,
      steps: original.steps,
      warnings: original.warnings,
      meta: original.meta,
    );
    final rows = _editorRows(plan);
    rows[0]['enabled'] = false;

    final revised = reviseFromOpenFiles(
      plan: plan,
      submittedRows: rows,
      rfiLog: _logFile(source),
      followUp: _followUpFile(target),
      ooxml: const FakeOoxml(),
    );

    expect(revised.planId, 'preserved-plan');
    expect(revised.steps, isEmpty);
    expect(obj(revised.meta['editor'])['rows'], isA<List>());
    expect(
      (obj(revised.meta['editor'])['rows']! as List).first['enabled'],
      false,
    );
    expect(revised.warnings.last.key, 'warn.rfi.followUp.noRowsSelected');
  });

  test('duplicate approved coordinates are rejected before the guarded map', () {
    const rfi = 'AZE-SUN-INT-QMS-RFI-CIV-0006';
    final plan = previewBytes(
      _revisionSource(rfi),
      targetWithRows([row(2, [inline('A2', rfi), blank('B2')])]),
    );
    final step = plan.steps[0];
    final updates = (step.meta['updates']! as List).cast<Object?>();
    final duplicated = Step(
      kind: step.kind,
      src: step.src,
      dst: step.dst,
      noteKey: step.noteKey,
      meta: {
        ...step.meta,
        'updates': [...updates, updates[0]],
      },
    );

    expect(
      () => guardedUpdates(duplicated),
      throwsA(
        isA<BackendError>()
            .having((error) => error.code, 'code', 'validation')
            .having(
              (error) => error.message,
              'message',
              'error.rfi.followUp.workbookInvalid',
            ),
      ),
    );
  });

  test('canonical RFI requires an exact supported anchor and normalizes numeric suffixes', () {
    expect(
      canonicalRfi('  aze-sun-int-qms-rfi-civ-0001  '),
      'AZE-SUN-INT-QMS-RFI-CIV-0001',
    );
    expect(
      canonicalRfi('AZE-SUN-INT-QMS-RFI-CIV-1'),
      'AZE-SUN-INT-QMS-RFI-CIV-0001',
    );
    expect(canonicalRfi('AZE-SUN-INT-QMS-RFI-CIV́-0001'), isNull);
    for (final rejected in [
      'prefix AZE-SUN-INT-QMS-RFI-CIV-0001',
      'AZE SUN INT QMS RFI CIV 0001',
      'AZE-SUN-INT-QMS-RFI-CIV -0001',
      'AZE-SUN-INT-QMS-RFI-CIV-00010',
      'AZE-SUN-INT-QMS-RFI-CIV-0001 suffix',
      'ITP-CIV-1',
      '2026-09-01',
    ]) {
      expect(canonicalRfi(rejected), isNull, reason: rejected);
    }
  });

  test('short Follow Up RFI uses Worley remark and preserves short display', () {
    const full = 'AZE-SUN-INT-QMS-RFI-CIV-0170';
    final review = classifyBytes(
      sourcePackage(
        worksheet([row(10, [inline('A10', full), inline('T10', 'Signed by Worley')])]),
        worksheet(const []),
      ),
      targetWithRows([row(2, [inline('A2', 'CIV-170'), inline('B2', 'Not Signed')])]),
    );

    final first = rowsOf(review)[0];
    expect(first['rfiNo'], 'CIV-0170');
    expect(first['sourceStatuses'], ['Signed']);
    expect(first['currentStatus'], 'Not Signed');
    expect(first['proposedStatus'], 'Signed');
    expect(first['selectable'], true);
    expect(first['reasonKey'], 'warn.rfi.followUp.reason.readySigned');
  });

  test('Follow Up uses short RFI identity and Worley signature remark only', () {
    expect(
      canonicalFollowUpRfi('CIV-0170'),
      ('AZE-SUN-INT-QMS-RFI-CIV-0170', 'CIV-0170'),
    );
    expect(
      canonicalFollowUpRfi('AZE-SUN-INT-QMS-RFI-CIV-0170'),
      ('AZE-SUN-INT-QMS-RFI-CIV-0170', 'CIV-0170'),
    );
    expect(
      canonicalFollowUpRfi('CIV-170'),
      ('AZE-SUN-INT-QMS-RFI-CIV-0170', 'CIV-0170'),
    );
    for (final rejected in [
      'CIV-01700',
      'prefix CIV-0170',
      'CIV-0170 suffix',
      'ITP-CIV-170',
      '2026-09-01',
    ]) {
      expect(canonicalFollowUpRfi(rejected), isNull, reason: rejected);
    }

    expect(mappedFollowUpStatus('Signed by Worley'), 'Signed');
    expect(
      mappedFollowUpStatus('survey report completed\r\n\r\nSigned by Worley'),
      'Signed',
    );
    for (final notSigned in ['', 'Completed', 'Not Signed by Worley', 'signed by worley']) {
      expect(mappedFollowUpStatus(notSigned), 'Not Signed', reason: notSigned);
    }
  });

  test('source duplicates require same safe status', () {
    const rfi = 'AZE-SUN-INT-QMS-RFI-CIV-0006';
    final civ = worksheet([row(10, [inline('A10', rfi), inline('T10', 'Signed by Worley')])]);
    final same = worksheet([row(10, [inline('A10', rfi), inline('U10', 'Signed by Worley')])]);
    final ready = classifyBytes(
      sourcePackage(civ, same),
      targetWithRows([row(2, [inline('A2', rfi), blank('B2')])]),
    );
    expect(rowsOf(ready)[0]['disposition'], readyDisposition);
    expect(rowsOf(ready)[0]['sourceStatuses'], ['Signed']);
    expect(
      (rowsOf(ready)[0]['sourceEvidence'] as List)
          .map((evidence) => obj(evidence)['sheet'])
          .toList(),
      ['CIV', 'MEC'],
    );

    final conflict = worksheet([row(10, [inline('A10', rfi), inline('U10', 'Ongoing')])]);
    final manual = classifyBytes(
      sourcePackage(civ, conflict),
      targetWithRows([row(2, [inline('A2', rfi), blank('B2')])]),
    );
    expect(rowsOf(manual)[0]['reasonKey'], 'warn.rfi.followUp.reason.sourceConflict');
    expect(rowsOf(manual)[0]['selectable'], false);
  });
}
