// Port of the inline tests of src-tauri/src/operations/replica_publish.rs.
import 'dart:io';

import 'package:docflow/engine/contracts.dart';
import 'package:docflow/engine/errors.dart';
import 'package:docflow/engine/operations/replica_publish.dart';
import 'package:docflow/engine/scan_index.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

const _disciplines = ['CIV', 'MEC', 'ELE', 'PRC'];

late Directory _tmp;

String _join(String a, [String? b, String? c]) => p.joinAll([a, ?b, ?c]);

ReplicaPublishParams _params(String root) {
  for (final name in ['a', 'b', 'c', 'backup']) {
    Directory(_join(root, name)).createSync(recursive: true);
  }
  for (final system in ['a', 'b', 'c']) {
    for (final discipline in _disciplines) {
      Directory(_join(root, system, discipline)).createSync(recursive: true);
    }
  }
  return ReplicaPublishParams(
    rootA: _join(root, 'a'),
    rootB: _join(root, 'b'),
    rootC: _join(root, 'c'),
    sourceRoot: 'a',
    backupRoot: _join(root, 'backup'),
    rootFiles: 'RFI Log.xlsm',
    unattended: true,
  );
}

String _resolved(String path) => Directory(path).resolveSymbolicLinksSync();

ReplicaPublishParams _resolvedParams(ReplicaPublishParams params) =>
    ReplicaPublishParams(
      rootA: _resolved(params.rootA),
      rootB: _resolved(params.rootB),
      rootC: _resolved(params.rootC),
      sourceRoot: params.sourceRoot,
      backupRoot: _resolved(params.backupRoot),
      rootFiles: params.rootFiles,
      unattended: params.unattended,
      intervalMinutes: params.intervalMinutes,
      settleSeconds: params.settleSeconds,
      maxChangedFiles: params.maxChangedFiles,
      maxChangedMib: params.maxChangedMib,
    );

void _write(String path, String content) {
  File(path)
    ..createSync(recursive: true)
    ..writeAsStringSync(content);
}

void main() {
  setUp(() {
    _tmp = Directory.systemTemp.createTempSync('e13_replica_');
  });

  tearDown(() {
    _tmp.deleteSync(recursive: true);
  });

  test(
    'preview copies missing, replaces changed and never deletes extra',
    () async {
      final params = _resolvedParams(_params(_tmp.path));
      _write(_join(params.rootA, 'CIV', 'source.pdf'), 'source');
      _write(_join(params.rootB, 'CIV', 'source.pdf'), 'old');
      _write(_join(params.rootC, 'CIV', 'extra.pdf'), 'extra');
      _write(_join(params.rootA, 'RFI Log.xlsm'), 'log');

      final plan = await previewReplicaPublish(params, (_) {});

      expect(
        plan.steps.where((step) => step.kind == StepKind.replicaPublish),
        hasLength(1),
      );
      expect(
        plan.steps.where((step) => step.kind == StepKind.copy),
        hasLength(3),
      );
      expect(
        plan.warnings.any(
          (w) =>
              w.key == 'warn.replica.targetExtra' &&
              w.subject.contains('extra.pdf'),
        ),
        isTrue,
      );
      expect(plan.meta.containsKey('automationProfile'), isTrue);
    },
  );

  test('preview rejects overlapping roots', () async {
    final params = _resolvedParams(_params(_tmp.path));
    final backup = _join(params.rootA, 'backup');
    Directory(backup).createSync(recursive: true);
    final overlapping = ReplicaPublishParams(
      rootA: params.rootA,
      rootB: params.rootB,
      rootC: params.rootC,
      sourceRoot: 'a',
      backupRoot: backup,
      rootFiles: params.rootFiles,
      unattended: true,
    );
    await expectLater(
      previewReplicaPublish(overlapping, (_) {}),
      throwsA(
        isA<BackendError>().having(
          (e) => e.message,
          'message',
          'error.replica.backupOverlap',
        ),
      ),
    );
  });

  test('preview rejects a missing selected source root file', () async {
    final params = _resolvedParams(_params(_tmp.path));
    await expectLater(
      previewReplicaPublish(params, (_) {}),
      throwsA(
        isA<BackendError>().having(
          (e) => e.message,
          'message',
          'error.replica.sourceRootFileMissing',
        ),
      ),
    );
  });

  test('indexed preview reuses unchanged hashes', () async {
    final params = _resolvedParams(_params(_tmp.path));
    _write(_join(params.rootA, 'MEC', 'source.pdf'), 'source');
    _write(_join(params.rootA, 'RFI Log.xlsm'), 'log');
    final index = ScanIndex.open(_join(_tmp.path, 'scan-index.sqlite3'));
    addTearDown(index.close);

    final first = await previewReplicaPublishWithIndex(
      params,
      ownerId: 'owner',
      index: index,
      progress: (_) {},
    );
    final second = await previewReplicaPublishWithIndex(
      params,
      ownerId: 'owner',
      index: index,
      progress: (_) {},
    );

    final firstStats = ScanIndexStats.fromJson(
      Map<String, Object?>.from(first.meta['scanIndex']! as Map),
    );
    final secondStats = ScanIndexStats.fromJson(
      Map<String, Object?>.from(second.meta['scanIndex']! as Map),
    );
    expect(firstStats.discovered, 2);
    expect(firstStats.analyzed, 2);
    expect(firstStats.reused, 0);
    expect(secondStats.discovered, 2);
    expect(secondStats.analyzed, 0);
    expect(secondStats.reused, 2);
    expect(second.steps, first.steps);
  });

  test('automatic preview defers a source that is still settling', () async {
    final params = _resolvedParams(_params(_tmp.path));
    _write(_join(params.rootA, 'RFI Log.xlsm'), 'log');
    final index = ScanIndex.open(_join(_tmp.path, 'scan-index.sqlite3'));
    addTearDown(index.close);

    await expectLater(
      previewAutomaticReplicaPublishWithIndex(
        params,
        ownerId: 'owner',
        index: index,
        progress: (_) {},
      ),
      throwsA(
        isA<BackendError>().having(
          (e) => e.message,
          'message',
          'error.replica.sourceSettling',
        ),
      ),
    );
  });

  test('automatic preview stops before a mass change', () {
    final params = ReplicaPublishParams(
      rootA: _tmp.path,
      rootB: _tmp.path,
      rootC: _tmp.path,
      sourceRoot: 'a',
      backupRoot: _tmp.path,
      maxChangedFiles: 1,
    );
    expect(
      () => enforceAutomaticChangeLimit(2, 1, params),
      throwsA(
        isA<BackendError>().having(
          (e) => e.message,
          'message',
          'error.replica.changeLimitExceeded',
        ),
      ),
    );
  });

  test('decodeReplicaStepMeta rejects a step without replica metadata', () {
    expect(
      () => decodeReplicaStepMeta(
        const Step(
          kind: StepKind.replicaPublish,
          src: null,
          dst: null,
          noteKey: null,
          meta: {},
        ),
      ),
      throwsA(
        isA<BackendError>().having(
          (e) => e.message,
          'message',
          'error.replica.approvedStepInvalid',
        ),
      ),
    );
  });
}
