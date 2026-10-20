// Port of the wire-format and store-level tests from src-tauri/src/journal.rs.
// The S5 CAS and retained-publication suites depend on the E8 follow-up
// fixtures and are not ported here.
import 'dart:convert';
import 'dart:io';

import 'package:docflow/engine/contracts.dart';
import 'package:docflow/engine/errors.dart';
import 'package:docflow/engine/journal.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

void main() {
  late Directory fixture;

  setUp(() {
    fixture = Directory.systemTemp.createTempSync('e3-journal-');
  });

  tearDown(() => fixture.deleteSync(recursive: true));

  JournalRecord sampleRecord() {
    final record = JournalRecord.create(
      journalId: '11111111-1111-4111-8111-111111111111',
      opId: 'rfi.process_scans',
      ts: '2026-08-25T00:00:00+00:00',
      ownerId: 'owner',
      sessionId: 'session',
      planDigest: 'digest',
    );
    record.steps.add(
      Step(
        kind: StepKind.move,
        src: '/in/a.pdf',
        dst: '/out/a.pdf',
        noteKey: 'note.renamed',
        meta: <String, Object?>{'undoSha256': 'abc'},
      ),
    );
    record.grantRoots = ['/out'];
    record.grantRootIdentities = [
      const GrantRootIdentity(
        root: '/out',
        identity: UnixObjectIdentity(device: 2049, inode: 77),
      ),
    ];
    record.state = JournalState.partialCommitted;
    record.undoneSteps.add(0);
    record.undoRecovery = const UndoRecovery(
      index: 0,
      original: '/out/a.pdf',
      quarantine: '/out/.docflow-undo.tmp',
      target: '/in/a.pdf',
      phase: UndoRecoveryPhase.quarantined,
    );
    return record;
  }

  test('journal records round-trip through the camelCase wire shape', () {
    final record = sampleRecord();
    final encoded = jsonDecode(jsonEncode(record.toJson())) as Map<String, Object?>;

    expect(encoded['opId'], 'rfi.process_scans');
    expect(encoded['state'], 'partial_committed');
    expect(encoded['undoneSteps'], [0]);
    expect(encoded['grantRootIdentities'], [
      {
        'root': '/out',
        'identity': {'kind': 'unix', 'device': 2049, 'inode': 77},
      },
    ]);
    final undoRecovery = encoded['undoRecovery']! as Map<String, Object?>;
    expect(undoRecovery['phase'], 'quarantined');
    expect(undoRecovery['target'], '/in/a.pdf');

    final decoded = JournalRecord.fromJson(encoded);
    expect(decoded.toJson(), record.toJson());
    expect(decoded.undoRecovery, record.undoRecovery);
  });

  test('journal wire enums reject unknown values', () {
    expect(
      () => JournalState.fromWire('not_a_state'),
      throwsA(isA<Object>()),
    );
    expect(
      () => UndoRecoveryPhase.fromWire('bogus'),
      throwsA(isA<Object>()),
    );
  });

  test('journal store persists and reads back records atomically', () {
    final journals = JournalStore(p.join(fixture.path, 'journal'));
    final record = sampleRecord()
      ..state = JournalState.committed
      ..undoRecovery = null;
    journals.write(record);

    final read = journals.read(record.journalId);
    expect(read.toJson(), record.toJson());
    expect(journals.recoverPending(), isEmpty);
  });

  test('a missing journal is reported as an error, not a fabricated record', () {
    final journals = JournalStore(p.join(fixture.path, 'journal'));
    expect(
      () => journals.read('22222222-2222-4222-8222-222222222222'),
      throwsA(isA<BackendError>()),
    );
  });

  test('interprocess mutation lock contends across store instances', () {
    final root = p.join(fixture.path, 'journal');
    final first = JournalStore(root);
    final second = JournalStore(root);

    final permit = first.tryAcquireMutationLock();
    expect(
      () => second.tryAcquireMutationLock(),
      throwsA(
        isA<BackendError>().having((e) => e.code, 'code', 'operation_in_progress'),
      ),
    );
    permit.release();
    second.tryAcquireMutationLock().release();
  }, skip: Platform.isWindows ? 'flock is POSIX-only; Windows lock is unverified' : null);
}
