// Port of src-tauri/tests/rfi_follow_up_native.rs (part 8: resource budgets
// for source evidence, projection and merge queries).
import 'dart:io';
import 'dart:typed_data';

import 'package:docflow/engine/contracts.dart';
import 'package:docflow/engine/fs_ops.dart';
import 'package:docflow/engine/ooxml.dart';
import 'package:docflow/engine/paths.dart';
import 'package:docflow/engine/plan_store.dart';
import 'package:docflow/engine/production_ports.dart';
import 'package:docflow/engine/rfi_follow_up.dart';
import 'package:docflow/engine/rfi_system_sync.dart';
import 'package:docflow/engine/undo.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

import 'e8_full_schema_support.dart';
import 'e8_support.dart';
import 'e8_xlsx_writer.dart';

void main() {
  test(
    'source evidence per-RFI limit fails before projection amplification',
    () {
      const rfi = 'AZE-SUN-INT-QMS-RFI-CIV-0006';
      final civRows = [
        for (var number = 10; number < 510; number++)
          row(number, [
            inline('A$number', rfi),
            inline('T$number', 'Completed'),
          ]),
      ];
      final mec = worksheet([
        row(10, [inline('A10', rfi), inline('U10', 'Completed')]),
      ]);

      expect(
        () => classifyBytes(
          sourcePackage(worksheet(civRows), mec),
          targetWithRows([
            row(2, [inline('A2', rfi), blank('B2')]),
          ]),
        ),
        throwsBackendMessage('error.rfi.followUp.limitExceeded'),
      );
    },
  );

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
    final merges = [
      for (var number = 1000; number < 2414; number++) 'Z$number:Z$number',
    ];

    expect(
      () => classifyBytes(
        sourcePackage(worksheet(sourceRows, merges), worksheet(const [])),
        targetPackage(worksheet(targetRows, merges)),
      ),
      throwsBackendMessage('error.rfi.followUp.limitExceeded'),
    );
  });

  test(
    'follow up sync applies more than five hundred guarded cell updates',
    () {
      const rfiCount = 501;
      final root = Directory.systemTemp.createTempSync('docflow-fu-apply-');
      addTearDown(() => root.deleteSync(recursive: true));
      final canonical = Directory(root.path).resolveSymbolicLinksSync();
      final sourcePath = p.join(canonical, 'RFI Log.xlsx');
      final targetPath = p.join(canonical, 'Follow Up.xlsx');
      final sourceBytes = xlsxSourcePackage(
        worksheet([
          for (var index = 0; index < rfiCount; index++)
            row(10 + index, [
              inline('A${10 + index}', _rfi(1000 + index)),
              inline('T${10 + index}', 'Completed'),
            ]),
        ]),
        worksheet(const []),
      );
      final originalTarget = xlsxTargetWithRows([
        for (var index = 0; index < rfiCount; index++)
          row(2 + index, [
            inline('A${2 + index}', _rfi(1000 + index)),
            blank('B${2 + index}'),
          ]),
      ]);
      File(sourcePath).writeAsBytesSync(sourceBytes);
      File(targetPath).writeAsBytesSync(originalTarget);

      final plan = previewFromOpenFiles(
        rfiLog: openSyncWorkbookFile(sourcePath),
        followUp: openSyncWorkbookFile(targetPath),
        ooxml: const ProductionFollowUpOoxml(),
      );
      final rows = (plan.meta['editor'] as Map)['rows'] as List;
      expect(rows, hasLength(rfiCount));
      expect(rows.every((row) => (row as Map)['enabled'] == true), isTrue);

      final authority = _authority(canonical);
      final cached = _cachePlan(authority, canonical, plan);
      final result = authority.apply(cached.planId, _caller);

      expect(result.applied, rfiCount, reason: '${result.errors}');
      expect(result.failed, 0);
      expect(File(sourcePath).readAsBytesSync(), sourceBytes);
      final applied = readWorkbookAndDiscovery(
        File(targetPath).readAsBytesSync(),
      ).$2.sheets['Follow Up']!.cells;
      expect(applied[(2, 2)]!.value, 'Not Signed');
      expect(applied[(rfiCount + 1, 2)]!.value, 'Not Signed');
    },
  );

  test(
    'follow up retained undo restores exact backup and preserves all evidence',
    () {
      final root = Directory.systemTemp.createTempSync('docflow-fu-undo-');
      addTearDown(() => root.deleteSync(recursive: true));
      final canonical = Directory(root.path).resolveSymbolicLinksSync();
      final sourcePath = p.join(canonical, 'RFI Log.xlsx');
      final targetPath = p.join(canonical, 'Follow Up.xlsx');
      const rfi = 'AZE-SUN-INT-QMS-RFI-CIV-0006';
      final sourceBytes = xlsxSourcePackage(
        worksheet([
          row(10, [inline('A10', rfi), inline('T10', 'Completed')]),
        ]),
        worksheet(const []),
      );
      final originalTarget = xlsxTargetWithRows([
        row(2, [inline('A2', rfi), blank('B2')]),
      ]);
      File(sourcePath).writeAsBytesSync(sourceBytes);
      File(targetPath).writeAsBytesSync(originalTarget);
      final plan = previewFromOpenFiles(
        rfiLog: openSyncWorkbookFile(sourcePath),
        followUp: openSyncWorkbookFile(targetPath),
        ooxml: const ProductionFollowUpOoxml(),
      );
      final authority = _authority(canonical);
      final applied = authority.apply(
        _cachePlan(authority, canonical, plan).planId,
        _caller,
      );
      expect(applied.applied, 1, reason: '${applied.errors}');
      final appliedTarget = File(targetPath).readAsBytesSync();
      expect(appliedTarget, isNot(originalTarget));

      // A destination is never accepted for a follow-up undo.
      expect(
        () =>
            authority.undo(applied.journalId, _caller, destination: canonical),
        throwsA(
          isA<BackendError>().having((e) => e.code, 'code', 'validation'),
        ),
      );
      expect(File(targetPath).readAsBytesSync(), appliedTarget);

      // A later user edit blocks undo and is preserved.
      File(targetPath)
          .writeAsBytesSync(Uint8List.fromList('later user edit'.codeUnits));
      expect(
        () => authority.undo(applied.journalId, _caller),
        throwsA(
          isA<BackendError>().having((e) => e.code, 'code', 'validation'),
        ),
      );
      expect(File(targetPath).readAsStringSync(), 'later user edit');

      File(targetPath).writeAsBytesSync(appliedTarget);
      final undone = authority.undo(applied.journalId, _caller);
      expect((undone.applied, undone.failed), (1, 0));
      expect(File(sourcePath).readAsBytesSync(), sourceBytes);
      expect(File(targetPath).readAsBytesSync(), originalTarget);
      expect(undone.meta['effectState'], 'committed');
      final retained = undone.meta['retainedEvidence'] as List;
      expect(retained, hasLength(4));
      expect(
        retained.every((item) => (item as Map).containsKey('path') == false),
        isTrue,
      );
      expect(
        Directory(canonical).listSync().any(
          (entity) => p
              .basename(entity.path)
              .startsWith('.docflow-s5-retained-applied-'),
        ),
        isTrue,
      );
      expect(_authority(canonical).recoverPending(_caller), isEmpty);
    },
  );

  test(
    'follow up retained recovery completes exact applied state after restart',
    () {
      final root = Directory.systemTemp.createTempSync('docflow-fu-recover-');
      addTearDown(() => root.deleteSync(recursive: true));
      final canonical = Directory(root.path).resolveSymbolicLinksSync();
      final sourcePath = p.join(canonical, 'RFI Log.xlsx');
      final targetPath = p.join(canonical, 'Follow Up.xlsx');
      const rfi = 'AZE-SUN-INT-QMS-RFI-CIV-0006';
      File(sourcePath).writeAsBytesSync(
        xlsxSourcePackage(
          worksheet([
            row(10, [inline('A10', rfi), inline('T10', 'Completed')]),
          ]),
          worksheet(const []),
        ),
      );
      File(targetPath).writeAsBytesSync(
        xlsxTargetWithRows([
          row(2, [inline('A2', rfi), blank('B2')]),
        ]),
      );
      final plan = previewFromOpenFiles(
        rfiLog: openSyncWorkbookFile(sourcePath),
        followUp: openSyncWorkbookFile(targetPath),
        ooxml: const ProductionFollowUpOoxml(),
      );
      final authority = _authority(canonical);
      final applied = authority.apply(
        _cachePlan(authority, canonical, plan).planId,
        _caller,
      );
      expect(applied.applied, 1, reason: '${applied.errors}');

      // A restarted authority over the same journal directory finds nothing
      // pending once the applied state is committed.
      expect(_authority(canonical).recoverPending(_caller), isEmpty);
      expect(
        readWorkbookAndDiscovery(File(targetPath).readAsBytesSync())
            .$2
            .sheets['Follow Up']!
            .cells[(2, 2)]!
            .value,
        'Not Signed',
      );
    },
  );
}

const _caller = CallerIdentity('owner', 'session');

String _rfi(int number) =>
    'AZE-SUN-INT-QMS-RFI-CIV-${number.toString().padLeft(4, '0')}';

FilesystemAuthority _authority(String root) => FilesystemAuthority(
  journalDir: p.join(root, 'journals'),
  ttl: const Duration(minutes: 5),
  ooxml: const ProductionFollowUpOoxml(),
  rfiLog: const ProductionRfiLogWorkbook(),
  rfiSystemSync: const RfiSystemSync(),
  pdf: const ProductionPdfEngine(),
  replicaPublish: const ProductionReplicaPublish(),
  undoJournal: undoJournal,
);

Plan _cachePlan(FilesystemAuthority authority, String root, Plan plan) {
  final scope = GrantScope.create(
    'owner',
    'session',
    'request',
    'rfi.follow_up_sync',
  );
  final source = authority.issueGrant(scope, root, GrantAccess.read);
  final target = authority.issueGrant(scope, root, GrantAccess.readWrite);
  return authority.cachePlan(
    BackendOrigin.rust,
    PlanBinding(
      ownerId: 'owner',
      sessionId: 'session',
      operationId: 'rfi.follow_up_sync',
      grantIds: [source.grantId, target.grantId],
    ),
    plan,
  );
}
