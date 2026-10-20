// Port of the S5 / retained-publication tests from src-tauri/src/journal.rs.
// Store-level tests are ported directly. Three tests that drive the
// FilesystemAuthority undo/recovery facade are skipped: that facade's undo and
// recover_pending entry points are not yet ported to Dart (see fs_ops.dart).
import 'dart:convert';
import 'dart:io';

import 'package:docflow/engine/contracts.dart';
import 'package:docflow/engine/errors.dart';
import 'package:docflow/engine/journal.dart';
import 'package:docflow/engine/paths.dart';
import 'package:docflow/engine/progress.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

const _authorityBlocked =
    'needs FilesystemAuthority.undo/recoverPending, not yet ported to Dart';

FilesystemObjectIdentity _id(int value) =>
    UnixObjectIdentity(device: 1, inode: value);

String _hex(String char) => char * 64;

S5PermitBundle _permits(JournalStore store) => store.bindS5Permits(
  GlobalMutationLock().tryAcquire(),
  store.tryAcquireMutationLock(),
);

Step _workbookStep(String root) => Step(
  kind: StepKind.workbookUpdate,
  src: p.join(root, 'target.xlsx'),
  dst: p.join(root, '.docflow-s5-backup-test.xlsx'),
  noteKey: 'note.rfi.followUp.workbookUpdate',
  meta: <String, Object?>{},
);

/// Port of `retained_publication_record`.
JournalRecord _retainedPublicationRecord(String root) {
  final record = JournalRecord.create(
    journalId: 'journal-s5',
    opId: 'rfi.follow_up_sync',
    ts: '2026-08-29T00-00-00Z',
    ownerId: 'owner',
    sessionId: 'session',
    planDigest: _hex('a'),
  );
  record.state = JournalState.applying;
  record.grantRoots = [root];
  record.grantRootIdentities = [GrantRootIdentity(root: root, identity: _id(1))];
  record.grantRootAccesses = [
    GrantRootAccessRecord(
      root: root,
      identity: _id(1),
      access: PersistedRootAccess.readWrite,
    ),
  ];
  record.s5Authority = S5AuthorityRecord(
    initialStep: _workbookStep(root),
    sourcePath: p.join(root, 'source.xlsx'),
    sourceIdentity: _id(2),
    targetPath: p.join(root, 'target.xlsx'),
    targetIdentity: _id(3),
    approvedUpdatesSha256: _hex('b'),
  );
  return record;
}

/// Port of `failed_apply_restore_record`.
JournalRecord _failedApplyRestoreRecord(String root) {
  const locator =
      'journal:journal-failed-apply;root:0;leaf:.docflow-s5-retained-original-journal-failed-apply.xlsx';
  final record = JournalRecord.create(
    journalId: 'journal-failed-apply',
    opId: 'rfi.follow_up_sync',
    ts: '2026-08-29T00-00-00Z',
    ownerId: 'owner',
    sessionId: 'session',
    planDigest: _hex('a'),
  );
  record.state = JournalState.recoveryRequired;
  record.grantRoots = [root];
  record.grantRootIdentities = [GrantRootIdentity(root: root, identity: _id(1))];
  record.grantRootAccesses = [
    GrantRootAccessRecord(
      root: root,
      identity: _id(1),
      access: PersistedRootAccess.readWrite,
    ),
  ];
  record.s5Authority = S5AuthorityRecord(
    initialStep: _workbookStep(root),
    sourcePath: p.join(root, 'source.xlsx'),
    sourceIdentity: _id(2),
    targetPath: p.join(root, 'target.xlsx'),
    targetIdentity: _id(3),
    approvedUpdatesSha256: _hex('b'),
  );
  record.pendingRetainedArtifact = PendingRetainedArtifact(
    role: RetainedArtifactRole.displacedOriginal,
    path: p.join(root, '.docflow-s5-retained-original-journal-failed-apply.xlsx'),
    locator: locator,
    creationState: PendingArtifactCreated(_id(3)),
  );
  record.pendingPublication = RetainedPublication(
    purpose: RetainedPublicationPurpose.apply,
    canonical: p.join(root, 'target.xlsx'),
    retained: p.join(root, '.docflow-s5-retained-original-journal-failed-apply.xlsx'),
    backup: p.join(root, '.docflow-s5-backup-journal-failed-apply.xlsx'),
    publicationSource: p.join(root, '.docflow-backups', '.docflow-s5-private-output.xlsx'),
    expectedCanonicalIdentity: _id(3),
    expectedCanonicalSha256: _hex('c'),
    expectedBackupIdentity: _id(4),
    expectedBackupSha256: _hex('c'),
    publicationSourceIdentity: _id(5),
    publicationSha256: _hex('e'),
    expectedCanonicalSecuritySha256: _hex('d'),
    expectedBackupSecuritySha256: _hex('d'),
    publicationSecuritySha256: _hex('f'),
    expectedCanonicalDefaultStreamBytes: 1,
    expectedCanonicalAuxiliaryChargeBytes: 0,
    expectedCanonicalLogicalChargeBytes: 1,
    destinationAccessSha256: _hex('1'),
    destinationAuxiliarySha256: _hex('2'),
    destinationAuxiliaryChargeBytes: 0,
    retainedIdentity: _id(3),
    retainedSha256: _hex('c'),
    retainedSecuritySha256: _hex('d'),
    phase: RetainedPublicationPhase.recoveryRequired,
  );
  record.recoveryNotice = RecoveryNotice(
    warningKey: 'error.rfi.followUp.recoveryRequired',
    locator: locator,
  );
  record.followUpPhase = FollowUpJournalPhase.publicationActive;
  return record;
}

RetainedArtifactEvidence _backupEvidence(String root, {int logical = 14, String sha = 'c'}) =>
    RetainedArtifactEvidence(
      role: RetainedArtifactRole.backup,
      path: p.join(root, '.docflow-s5-backup-evidence.xlsx'),
      identity: _id(4),
      sha256: _hex(sha),
      securitySha256: _hex('d'),
      defaultStreamBytes: 10,
      auxiliaryChargeBytes: 5,
      logicalChargeBytes: logical,
      locator: 'journal:journal-s5;root:0;leaf:.docflow-s5-backup-evidence.xlsx',
    );

void main() {
  late Directory fixture;
  late String root;
  late JournalStore store;

  setUp(() {
    fixture = Directory.systemTemp.createTempSync('e3-s5-');
    root = Directory(fixture.path).resolveSymbolicLinksSync();
    store = JournalStore(p.join(root, 'journals'));
  });

  tearDown(() => fixture.deleteSync(recursive: true));

  test('legacy JSON defaults additive S5 fields and omits them again', () {
    final legacy = <String, Object?>{
      'journalId': 'legacy-journal',
      'opId': 'rfi.process_scans',
      'ts': '2026-08-29T00-00-00Z',
      'steps': <Object?>[],
    };
    final record = JournalRecord.fromJson(legacy);
    expect(record.revision, 0);
    expect(record.s5Authority, isNull);
    expect(record.pendingPublication, isNull);
    expect(record.pendingArtifactParent, isNull);
    expect(record.createdArtifactParent, isNull);
    expect(record.pendingRetainedArtifact, isNull);
    expect(record.retainedEvidence, isEmpty);
    expect(record.recoveryNotice, isNull);
    expect(record.followUpPhase, isNull);
    expect(record.grantRootAccesses, isEmpty);

    final encoded = jsonDecode(jsonEncode(record.toJson())) as Map<String, Object?>;
    for (final key in [
      's5Authority',
      'pendingPublication',
      'pendingArtifactParent',
      'createdArtifactParent',
      'pendingRetainedArtifact',
      'retainedEvidence',
      'recoveryNotice',
      'followUpPhase',
      'grantRootAccesses',
      'revision',
    ]) {
      expect(encoded.containsKey(key), isFalse, reason: 'legacy output exposed $key');
    }
  });

  test('recovery projection is additive and wire exact', () {
    const entry = RecoveryEntry(
      journalId: 'journal-s5',
      disposition: RecoveryDisposition.outcomeUnknown,
      warningKey: 'error.rfi.followUp.recoveryRequired',
      locator: 'journal:journal-s5;root:0;leaf:evidence.xlsx',
    );
    expect(entry.toJson(), {
      'journalId': 'journal-s5',
      'disposition': 'outcome_unknown',
      'warningKey': 'error.rfi.followUp.recoveryRequired',
      'locator': 'journal:journal-s5;root:0;leaf:evidence.xlsx',
      'restoreOriginalAvailable': false,
    });
  });

  test('retained publication validation rejects roster, charge and locator drift', () {
    ValidatedS5Record validate(JournalRecord record) {
      final permits = _permits(store);
      try {
        final token = ValidatedS5Record.forTest(record, permits);
        token.release();
        return token;
      } on Object {
        permits.release();
        rethrow;
      }
    }

    final record = _retainedPublicationRecord(root);
    validate(record);

    record.grantRootAccesses.add(record.grantRootAccesses.first);
    expect(() => validate(record), throwsA(isA<BackendError>()));
    record.grantRootAccesses.removeLast();

    record.retainedEvidence.add(_backupEvidence(root, logical: 14));
    expect(() => validate(record), throwsA(isA<BackendError>()));
    record.retainedEvidence[0] = _backupEvidence(root, logical: 15);
    validate(record);

    record.retainedEvidence[0] = RetainedArtifactEvidence(
      role: RetainedArtifactRole.backup,
      path: p.join(root, '.docflow-s5-backup-evidence.xlsx'),
      identity: _id(4),
      sha256: 'UPPER',
      securitySha256: _hex('d'),
      defaultStreamBytes: 10,
      auxiliaryChargeBytes: 5,
      logicalChargeBytes: 15,
      locator: 'journal:journal-s5;root:0;leaf:.docflow-s5-backup-evidence.xlsx',
    );
    expect(() => validate(record), throwsA(isA<BackendError>()));
    record.retainedEvidence[0] = RetainedArtifactEvidence(
      role: RetainedArtifactRole.backup,
      path: p.join(root, '.docflow-s5-backup-evidence.xlsx'),
      identity: _id(4),
      sha256: _hex('c'),
      securitySha256: _hex('d'),
      defaultStreamBytes: 10,
      auxiliaryChargeBytes: 5,
      logicalChargeBytes: 15,
      locator: 'x' * (maxRecoveryLocatorScalars + 1),
    );
    expect(() => validate(record), throwsA(isA<BackendError>()));
  });

  test('a final-notice S5 record recovers without reopening roots or rewriting', () {
    final record = _retainedPublicationRecord(root);
    final permits = _permits(store);
    final token = ValidatedS5Record.forTest(record, permits);
    token.prepareFailedPartial(
      RecoveryNotice(
        warningKey: 'error.rfi.followUp.recoveryRequired',
        locator: 'journal:journal-s5;root:0;leaf:target.xlsx',
      ),
    );
    store.writeS5Cas(token, 0);
    token.release();

    final journalFile = Directory(store.root)
        .listSync()
        .whereType<File>()
        .singleWhere((file) => file.path.endsWith('.json'));
    final before = journalFile.readAsBytesSync();

    final session = store.beginS5Recovery(
      _permits(store),
      reconcile: (_, _) => throw StateError('reconciler must not run'),
    );
    expect(session.nextRootAccesses, isNull);
    final recovered = session.recoverNext(null, const CallerIdentity('owner', 'session'));
    expect(recovered?.journalId, 'journal-s5');
    expect(journalFile.readAsBytesSync(), before);
    session.release();
  });

  test('validated S5 record CAS is monotonic and legacy routes reject S5', () {
    final raw = _retainedPublicationRecord(root);
    final token = ValidatedS5Record.forTest(raw, _permits(store));

    store.writeS5Cas(token, 0);
    expect(token.revision, 1);
    expect(store.readS5ForTest('journal-s5').revision, 1);

    token.applyTransition(
      SetPendingArtifactParent(
        PendingArtifactParent(
          path: p.join(root, '.docflow-backups'),
          locator: 'journal:journal-s5;root:0;leaf:.docflow-backups',
        ),
      ),
    );
    expect(() => store.writeS5Cas(token, 0), throwsA(isA<BackendError>()));
    expect(token.revision, 1);
    expect(store.readS5ForTest('journal-s5').pendingArtifactParent, isNull);
    store.writeS5Cas(token, 1);
    expect(token.revision, 2);

    expect(() => store.read('journal-s5'), throwsA(isA<BackendError>()));
    raw.revision = 2;
    expect(() => store.write(raw), throwsA(isA<BackendError>()));
    token.release();
  });

  test('a failed follow-up apply with exact displaced original is restore-undo eligible', () {
    final record = _failedApplyRestoreRecord(root);
    final token = ValidatedS5Record.forTest(record, _permits(store));
    store.writeS5Cas(token, 0);
    token.release();

    final pending = store.inspectS5ForUndo(
      'journal-failed-apply',
      const CallerIdentity('owner', 'session'),
      _permits(store),
    );
    expect(pending.mode, S5UndoMode.restoreFailedApply);
  });

  test(
    'failed follow-up apply undo restores the exact original and keeps backup',
    () {},
    skip: _authorityBlocked,
  );
  test(
    'recovery finalizes an exact user-initiated restore without moving files',
    () {},
    skip: _authorityBlocked,
  );
  test(
    'failed follow-up apply restore never clobbers a new canonical file',
    () {},
    skip: _authorityBlocked,
  );

  test(
    'journal replace retries transient Windows access denied',
    () {},
    skip: Platform.isWindows ? 'Windows retry path is UNVERIFIED' : 'Windows-only test',
  );
  test(
    'journal store survives a transient Windows file lock',
    () {},
    skip: Platform.isWindows ? 'Windows lock is UNVERIFIED' : 'Windows-only test',
  );
}
