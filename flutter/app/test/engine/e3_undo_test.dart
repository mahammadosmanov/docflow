// Port of the inline tests in src-tauri/src/undo.rs. The directory-sync fault
// test is omitted: it needs cap_fs's fault injection hook, which is not part
// of the public port.
import 'dart:io';

import 'package:docflow/engine/cap_fs.dart';
import 'package:docflow/engine/contracts.dart';
import 'package:docflow/engine/errors.dart';
import 'package:docflow/engine/journal.dart';
import 'package:docflow/engine/paths.dart';
import 'package:docflow/engine/undo.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

class _FakeOutlookPort implements OutlookReplyDraftUndoPort {
  _FakeOutlookPort(this.answer);

  final bool answer;

  @override
  bool journalIsSafelyUndoable(JournalRecord record) => answer;
}

const _journalId = 'undo-recovery-test';
const _caller = CallerIdentity('owner', 'restart');

late Directory _fixture;
late String _root;
late String _journalDir;

void main() {
  setUp(() {
    _fixture = Directory.systemTemp.createTempSync('e3-undo-');
    final root = p.join(_fixture.path, 'documents');
    Directory(root).createSync(recursive: true);
    _root = Directory(root).resolveSymbolicLinksSync();
    _journalDir = p.join(_fixture.path, 'journal');
    UndoTestHooks.beforeQuarantineRestore = null;
    UndoTestHooks.failQuarantineDelete = false;
  });

  tearDown(() {
    UndoTestHooks.beforeQuarantineRestore = null;
    UndoTestHooks.failQuarantineDelete = false;
    _fixture.deleteSync(recursive: true);
  });

  /// Port of `committed_copy_record`.
  JournalRecord committedCopyRecord(String destination) {
    final capability = CapabilityRoot.open(_root);
    final record = JournalRecord.create(
      journalId: _journalId,
      opId: 'op.test',
      ts: '2026-08-25T00:00:00+00:00',
      ownerId: 'owner',
      sessionId: 'session',
      planDigest: 'digest',
    );
    record.steps.add(
      Step(
        kind: StepKind.copy,
        src: p.join(_root, 'source.txt'),
        dst: destination,
        noteKey: null,
        meta: <String, Object?>{
          'undoSha256': capability.authorize(destination).anchor().sha256(),
        },
      ),
    );
    record.grantRoots = [_root];
    record.grantRootIdentities = [
      GrantRootIdentity(root: _root, identity: capability.identity()),
    ];
    record.state = JournalState.committed;
    return record;
  }

  /// Port of `assert_durable_outcome_unknown`.
  Map<String, Object?> assertDurableOutcomeUnknown(JournalStore journals) {
    final record = journals.read(_journalId);
    expect(record.state, JournalState.recoveryRequired);
    expect(record.pendingUndo, 0);
    final encoded = record.toJson();
    final recovery = encoded['undoRecovery']! as Map<String, Object?>;
    expect(File(recovery['quarantine']! as String).existsSync(), isTrue);
    final recovered = journals.recoverPending();
    expect(recovered.first.disposition, RecoveryDisposition.outcomeUnknown);
    return recovery;
  }

  test(
    'hash failure and quarantine restore collision remain recoverable',
    () {
      final source = p.join(_root, 'source.txt');
      final destination = p.join(_root, 'copy.txt');
      File(source).writeAsBytesSync('APPROVED'.codeUnits);
      File(destination).writeAsBytesSync('APPROVED'.codeUnits);
      final journals = JournalStore(_journalDir);
      final record = committedCopyRecord(destination);
      record.steps.first.meta['undoSha256'] = 'not-the-approved-hash';
      journals.write(record);
      UndoTestHooks.beforeQuarantineRestore = () {
        File(destination).writeAsBytesSync('USER COLLISION'.codeUnits);
      };

      final result = undoJournal(journals, _journalId, _caller, null);

      expect(result.meta['effectState'], 'outcome_unknown');
      final recovery = assertDurableOutcomeUnknown(journals);
      expect(recovery['original'], destination);
      expect(File(destination).readAsStringSync(), 'USER COLLISION');
    },
    skip: Platform.isWindows ? 'POSIX quarantine semantics' : null,
  );

  test(
    'move target failure retains quarantine and recovery without restoring',
    () {
      final source = p.join(_root, 'source.txt');
      final destination = p.join(_root, 'moved.txt');
      File(source).writeAsBytesSync('USER TARGET COLLISION'.codeUnits);
      File(destination).writeAsBytesSync('APPROVED MOVE'.codeUnits);
      final capability = CapabilityRoot.open(_root);
      final record = JournalRecord.create(
        journalId: _journalId,
        opId: 'op.test',
        ts: '2026-08-25T00:00:00+00:00',
        ownerId: 'owner',
        sessionId: 'session',
        planDigest: 'digest',
      );
      record.steps.add(
        Step(
          kind: StepKind.move,
          src: source,
          dst: destination,
          noteKey: null,
          meta: <String, Object?>{
            'undoSha256': capability.authorize(destination).anchor().sha256(),
          },
        ),
      );
      record.grantRoots = [_root];
      record.grantRootIdentities = [
        GrantRootIdentity(root: _root, identity: capability.identity()),
      ];
      record.state = JournalState.committed;
      final journals = JournalStore(_journalDir);
      journals.write(record);

      final result = undoJournal(journals, _journalId, _caller, null);

      expect(result.meta['effectState'], 'outcome_unknown');
      final recovery = assertDurableOutcomeUnknown(journals);
      expect(recovery['target'], source);
      expect(File(source).readAsStringSync(), 'USER TARGET COLLISION');
      expect(File(destination).existsSync(), isFalse);
    },
    skip: Platform.isWindows ? 'POSIX quarantine semantics' : null,
  );

  test(
    'quarantine delete failure retains the durable locator',
    () {
      final source = p.join(_root, 'source.txt');
      final destination = p.join(_root, 'copy.txt');
      File(source).writeAsBytesSync('APPROVED'.codeUnits);
      File(destination).writeAsBytesSync('APPROVED'.codeUnits);
      final journals = JournalStore(_journalDir);
      journals.write(committedCopyRecord(destination));
      UndoTestHooks.failQuarantineDelete = true;

      final result = undoJournal(journals, _journalId, _caller, null);

      expect(result.meta['effectState'], 'outcome_unknown');
      assertDurableOutcomeUnknown(journals);
    },
    skip: Platform.isWindows ? 'POSIX quarantine semantics' : null,
  );

  test(
    'rename success followed by directory sync failure keeps durable undo recovery',
    () {
      final source = p.join(_root, 'source.txt');
      final destination = p.join(_root, 'copy.txt');
      File(source).writeAsBytesSync('APPROVED'.codeUnits);
      File(destination).writeAsBytesSync('APPROVED'.codeUnits);
      final journals = JournalStore(_journalDir);
      journals.write(committedCopyRecord(destination));
      injectDirectorySyncFailureAfter(0);

      final result = undoJournal(journals, _journalId, _caller, null);

      expect(result.meta['effectState'], 'outcome_unknown');
      final recovery = assertDurableOutcomeUnknown(journals);
      expect(File(recovery['quarantine']! as String).existsSync(), isTrue);
    },
    skip: Platform.isWindows ? 'POSIX quarantine semantics' : null,
  );

  test('journal is safely undoable only through the outlook port for outlook journals', () {
    final destination = p.join(_root, 'copy.txt');
    File(p.join(_root, 'source.txt')).writeAsBytesSync('APPROVED'.codeUnits);
    File(destination).writeAsBytesSync('APPROVED'.codeUnits);
    final record = committedCopyRecord(destination);
    expect(journalIsSafelyUndoable(record), isTrue);

    final outlook = committedCopyRecord(destination)
      ..opId = outlookReplyDraftOperationId;
    expect(journalIsSafelyUndoable(outlook), isFalse);
    expect(
      journalIsSafelyUndoable(
        outlook,
        outlookReplyDraft: _FakeOutlookPort(true),
      ),
      isTrue,
    );
    expect(
      journalIsSafelyUndoable(
        outlook,
        outlookReplyDraft: _FakeOutlookPort(false),
      ),
      isFalse,
    );
  });

  test('journals with an unresolved pending effect are refused', () {
    final destination = p.join(_root, 'copy.txt');
    File(p.join(_root, 'source.txt')).writeAsBytesSync('APPROVED'.codeUnits);
    File(destination).writeAsBytesSync('APPROVED'.codeUnits);
    final record = committedCopyRecord(destination);
    record.pendingUndo = 0;
    final journals = JournalStore(_journalDir);
    journals.write(record);

    expect(
      () => undoJournal(journals, _journalId, _caller, null),
      throwsA(
        isA<BackendError>().having((e) => e.code, 'code', 'journal_invalid'),
      ),
    );
  });
}
