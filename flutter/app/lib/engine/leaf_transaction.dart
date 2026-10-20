// Port of src-tauri/src/leaf_transaction.rs.
//
// Retained-publication leaf transaction for S5 (follow-up) publications: the
// canonical leaf is displaced into a retained name without clobbering, a new
// canonical file is created exclusively, its bytes are copied from the
// publication source and verified, and every step is journaled through a
// compare-and-swap revision write before the next step runs.


import 'cap_fs.dart'
    show
        AnchoredPath,
        CreationSecurityDescriptor,
        FileSecuritySnapshot,
        IoError,
        OsHandle,
        SecuredCreatedFile,
        copyStream,
        fileIdentity,
        sha256OpenFile;
import 'errors.dart' show BackendError;
import 'journal.dart'
    show
        AnchoredPathPort,
        AuthorizedPathPort,
        CapabilityRootPort,
        FilesystemObjectIdentity,
        FinalizeRecovery,
        FsErrorKind,
        JournalStore,
        MarkCanonicalBytesStarted,
        MarkCanonicalVerifying,
        RecordCanonicalAuxiliaryReady,
        RecordCanonicalSecured,
        RecordRetainedPublicationEvidence,
        RecoveryDisposition,
        RecoveryEntry,
        RecoveryNotice,
        RetainedArtifactEvidence,
        RetainedArtifactRole,
        RetainedPublicationPhase,
        RetainedPublicationPurpose,
        S5RecoveryTerminal,
        S5Transition,
        ValidatedS5Record;
import 'progress.dart' show CancellationToken;

const _recoveryRequiredKey = 'error.rfi.followUp.recoveryRequired';

/// `PublicationCancellation`: either a cancellable apply token or none (undo).
sealed class PublicationCancellation {
  const PublicationCancellation();
}

final class ApplyPublicationCancellation extends PublicationCancellation {
  const ApplyPublicationCancellation(this.token);

  final CancellationToken token;
}

final class NotCancellablePublication extends PublicationCancellation {
  const NotCancellablePublication();
}

/// `RetainedPublicationOutcome`.
sealed class RetainedPublicationOutcome {
  const RetainedPublicationOutcome();
}

final class RetainedPublicationPublished extends RetainedPublicationOutcome {
  const RetainedPublicationPublished(this.createdIdentity);

  final FilesystemObjectIdentity createdIdentity;
}

final class RetainedPublicationCancelledBeforeDisplacement
    extends RetainedPublicationOutcome {
  const RetainedPublicationCancelledBeforeDisplacement();
}

final class RetainedPublicationRecoveryRequired
    extends RetainedPublicationOutcome {
  const RetainedPublicationRecoveryRequired(this.notice);

  final RecoveryNotice notice;
}

/// `execute_retained_publication`.
///
/// [publicationSource] is the already-open publication payload (`R: Read +
/// Seek`); [beforeCanonicalCreate] and [verifyDestination] are the caller's
/// hooks (`B` and `V`). Errors that Rust returns as `BackendError` are thrown
/// as [BackendError]; leaf-level failures that Rust maps to a recovery notice
/// are returned as [RetainedPublicationRecoveryRequired].
RetainedPublicationOutcome executeRetainedPublication({
  required JournalStore journals,
  required ValidatedS5Record record,
  required AnchoredPath canonical,
  required AnchoredPath retained,
  required OsHandle publicationSource,
  required CreationSecurityDescriptor destinationCreation,
  required FileSecuritySnapshot destinationSnapshot,
  required PublicationCancellation cancellation,
  required void Function() beforeCanonicalCreate,
  required void Function(SecuredCreatedFile created) verifyDestination,
}) {
  if (cancellation is ApplyPublicationCancellation) {
    try {
      cancellation.token.checkpoint();
    } on BackendError {
      return const RetainedPublicationCancelledBeforeDisplacement();
    }
  }
  final publication = record.pendingPublication;
  if (publication == null) {
    throw BackendError.journalInvalid('S5 publication is not prepared');
  }
  final pendingRetained = record.pendingRetainedArtifact;
  if (pendingRetained == null) {
    throw BackendError.journalInvalid('S5 retained intent is missing');
  }
  final expectedRole = switch (publication.purpose) {
    RetainedPublicationPurpose.apply => RetainedArtifactRole.displacedOriginal,
    RetainedPublicationPurpose.undo => RetainedArtifactRole.displacedApplied,
  };
  if (publication.phase != RetainedPublicationPhase.prepared ||
      pendingRetained.role != expectedRole ||
      pendingRetained.path != publication.retained) {
    throw BackendError.journalInvalid(
      'S5 retained publication authority is inconsistent',
    );
  }
  _transitionPhase(
    journals,
    record,
    RetainedPublicationPhase.prepared,
    RetainedPublicationPhase.displacing,
  );

  final locator = pendingRetained.locator;
  RetainedPublicationRecoveryRequired recoveryRequired() =>
      RetainedPublicationRecoveryRequired(
        RecoveryNotice(warningKey: _recoveryRequiredKey, locator: locator),
      );

  try {
    canonical.retainCurrentNameNoClobber(retained);
  } on Exception {
    return recoveryRequired();
  }

  final OsHandle retainedFile;
  try {
    retainedFile = retained.openRead();
  } on Exception {
    return recoveryRequired();
  }
  final FilesystemObjectIdentity retainedIdentity;
  final int retainedBytes;
  final FileSecuritySnapshot retainedSecurity;
  final String retainedHash;
  try {
    retainedIdentity = fileIdentity(retainedFile);
    retainedBytes = retainedFile.metadata().size;
    retainedSecurity = FileSecuritySnapshot.capture(retainedFile);
    retainedHash = _sha256Reader(retainedFile);
  } on Exception {
    retainedFile.close();
    return recoveryRequired();
  }
  retainedFile.close();

  if (retainedIdentity != publication.expectedCanonicalIdentity ||
      retainedHash != publication.expectedCanonicalSha256 ||
      retainedSecurity.sha256() != publication.expectedCanonicalSecuritySha256) {
    return recoveryRequired();
  }
  final auxiliaryChargeBytes = retainedSecurity.auxiliaryChargeBytes();
  final logicalChargeBytes = _checkedAddInt(retainedBytes, auxiliaryChargeBytes);
  if (logicalChargeBytes == null) {
    return recoveryRequired();
  }
  final evidence = RetainedArtifactEvidence(
    role: expectedRole,
    path: publication.retained,
    identity: retainedIdentity,
    sha256: retainedHash,
    securitySha256: retainedSecurity.sha256(),
    defaultStreamBytes: retainedBytes,
    auxiliaryChargeBytes: auxiliaryChargeBytes,
    logicalChargeBytes: logicalChargeBytes,
    locator: pendingRetained.locator,
  );
  _transition(
    journals,
    record,
    RecordRetainedPublicationEvidence(evidence),
  );
  beforeCanonicalCreate();
  _transitionPhase(
    journals,
    record,
    RetainedPublicationPhase.retained,
    RetainedPublicationPhase.creatingCanonical,
  );

  final SecuredCreatedFile created;
  try {
    created = canonical.createNewSecured(destinationCreation);
  } on Exception {
    return recoveryRequired();
  }
  final FilesystemObjectIdentity createdIdentity;
  try {
    createdIdentity = created.identity();
  } on Exception {
    return recoveryRequired();
  }
  try {
    destinationSnapshot.verifyCreatedAccess(created);
  } on Exception {
    return recoveryRequired();
  }
  _transition(
    journals,
    record,
    RecordCanonicalSecured(
      identity: createdIdentity,
      descriptorSha256: destinationSnapshot.accessSha256(),
    ),
  );
  try {
    canonical.syncParent();
  } on Exception {
    return recoveryRequired();
  }
  try {
    destinationSnapshot.replayAndVerifyAuxiliary(created);
  } on Exception {
    return recoveryRequired();
  }
  _transition(
    journals,
    record,
    RecordCanonicalAuxiliaryReady(
      auxiliarySha256: destinationSnapshot.auxiliarySha256(),
      auxiliaryChargeBytes: destinationSnapshot.auxiliaryChargeBytes(),
    ),
  );
  _transition(journals, record, const MarkCanonicalBytesStarted());

  try {
    publicationSource.seek(0);
    copyStream(publicationSource, created.handle);
    created.syncAll();
  } on Exception {
    throw BackendError.validation(_recoveryRequiredKey);
  }
  _transition(journals, record, const MarkCanonicalVerifying());
  verifyDestination(created);

  try {
    destinationSnapshot.verifyComplete(created);
  } on Exception {
    return recoveryRequired();
  }
  if (_identityOrNull(created) != createdIdentity) {
    return recoveryRequired();
  }
  final String createdHash;
  try {
    createdHash = sha256OpenFile(created.handle);
  } on Exception {
    throw BackendError.validation(_recoveryRequiredKey);
  }
  if (createdHash != publication.publicationSha256) {
    return recoveryRequired();
  }
  try {
    if (canonical.identity() != createdIdentity) {
      return recoveryRequired();
    }
  } on Exception {
    return recoveryRequired();
  }
  return RetainedPublicationPublished(createdIdentity);
}

/// `reconcile_retained_publication`. Returns the recovery entry to report, or
/// null when the publication was completed or finalized cleanly.
RecoveryEntry? reconcileRetainedPublication({
  required JournalStore journals,
  required ValidatedS5Record record,
}) {
  final publication = record.pendingPublication;
  if (publication == null) {
    throw BackendError.journalInvalid('S5 publication is not pending');
  }
  final pendingLocator = record.pendingRetainedArtifact?.locator;
  final String? locator = pendingLocator ??
      _firstEvidenceLocator(record, publication.retained);
  if (locator == null) {
    throw BackendError.journalInvalid('S5 recovery locator is missing');
  }
  final canonical = _observeRecordedPath(record, publication.canonical);
  final retained = _observeRecordedPath(record, publication.retained);
  final backup = _observeRecordedPath(record, publication.backup);
  final source = _observeRecordedPath(record, publication.publicationSource);

  if (publication.phase == RetainedPublicationPhase.restoringOriginal) {
    final notice = RecoveryNotice(
      warningKey: _recoveryRequiredKey,
      locator: locator,
    );
    final restored = canonical.matches(
          publication.expectedCanonicalIdentity,
          publication.expectedCanonicalSha256,
          publication.expectedCanonicalSecuritySha256,
        ) &&
        retained is _PathAbsent;
    if (restored && _syncRecordedParent(record, publication.canonical)) {
      _transition(
        journals,
        record,
        const FinalizeRecovery(
          terminal: S5RecoveryTerminal.completedRestore,
          notice: null,
          evidence: [],
        ),
      );
      return null;
    }
    return RecoveryEntry(
      journalId: record.journalId,
      disposition: RecoveryDisposition.outcomeUnknown,
      warningKey: notice.warningKey,
      locator: notice.locator,
      restoreOriginalAvailable: true,
    );
  }

  final backupExact = backup.matches(
    publication.expectedBackupIdentity,
    publication.expectedBackupSha256,
    publication.expectedBackupSecuritySha256,
  );
  final sourceExact = source.matches(
    publication.publicationSourceIdentity,
    publication.publicationSha256,
    publication.publicationSecuritySha256,
  );
  final retainedIdentity = publication.retainedIdentity;
  final retainedHash = publication.retainedSha256;
  final retainedSecurity = publication.retainedSecuritySha256;
  final retainedExact = retainedIdentity != null &&
      retainedHash != null &&
      retainedSecurity != null &&
      retained.matches(retainedIdentity, retainedHash, retainedSecurity) &&
      retainedIdentity == publication.expectedCanonicalIdentity &&
      retainedHash == publication.expectedCanonicalSha256 &&
      retainedSecurity == publication.expectedCanonicalSecuritySha256;
  final createdIdentity = publication.createdCanonicalIdentity;
  final exactDestination = createdIdentity != null &&
      canonical.matches(
        createdIdentity,
        publication.publicationSha256,
        publication.expectedBackupSecuritySha256,
      );
  if (backupExact && sourceExact && retainedExact && exactDestination) {
    final terminal = switch (publication.purpose) {
      RetainedPublicationPurpose.apply => S5RecoveryTerminal.completedApply,
      RetainedPublicationPurpose.undo => S5RecoveryTerminal.completedUndo,
    };
    _transition(
      journals,
      record,
      FinalizeRecovery(
        terminal: terminal,
        notice: null,
        evidence: const [],
      ),
    );
    return null;
  }
  final noCanonicalEffect = createdIdentity == null &&
      canonical.matches(
        publication.expectedCanonicalIdentity,
        publication.expectedCanonicalSha256,
        publication.expectedCanonicalSecuritySha256,
      ) &&
      retained is _PathAbsent &&
      backupExact &&
      sourceExact;
  final notice = RecoveryNotice(warningKey: _recoveryRequiredKey, locator: locator);
  _transition(
    journals,
    record,
    FinalizeRecovery(
      terminal: noCanonicalEffect
          ? S5RecoveryTerminal.failedPartial
          : S5RecoveryTerminal.outcomeUnknown,
      notice: notice,
      evidence: const [],
    ),
  );
  return RecoveryEntry(
    journalId: record.journalId,
    disposition: RecoveryDisposition.outcomeUnknown,
    warningKey: notice.warningKey,
    locator: notice.locator,
    restoreOriginalAvailable: false,
  );
}

String? _firstEvidenceLocator(ValidatedS5Record record, String retainedPath) {
  for (final evidence in record.retainedEvidence) {
    if (evidence.path == retainedPath) return evidence.locator;
  }
  return null;
}

/// `sync_recorded_parent`: syncs the parent of the first root that authorizes
/// [rawPath]. Returns false when no root can do it.
bool _syncRecordedParent(ValidatedS5Record record, String rawPath) {
  final roots = record.roots;
  if (roots == null) return false;
  for (final root in roots.roots) {
    final anchor = _anchorFor(root, rawPath);
    if (anchor == null) continue;
    try {
      anchor.syncParent();
      return true;
    } on Exception {
      continue;
    }
  }
  return false;
}

/// Observation of one recorded path (`PathObservation`).
sealed class _PathObservation {
  const _PathObservation();

  /// `PathObservation::matches`.
  bool matches(FilesystemObjectIdentity identity, String hash, String security) =>
      false;
}

final class _PathAbsent extends _PathObservation {
  const _PathAbsent();
}

final class _PathPresent extends _PathObservation {
  const _PathPresent({
    required this.identity,
    required this.sha256,
    required this.securitySha256,
  });

  final FilesystemObjectIdentity identity;
  final String sha256;
  final String securitySha256;

  @override
  bool matches(FilesystemObjectIdentity identity, String hash, String security) =>
      this.identity == identity &&
      sha256 == hash &&
      securitySha256 == security;
}

final class _PathUnknown extends _PathObservation {
  const _PathUnknown();
}

/// `observe_recorded_path`.
_PathObservation _observeRecordedPath(ValidatedS5Record record, String rawPath) {
  final roots = record.roots;
  if (roots == null) return const _PathUnknown();
  AnchoredPath? anchored;
  for (final root in roots.roots) {
    anchored = _anchorFor(root, rawPath);
    if (anchored != null) break;
  }
  if (anchored == null) return const _PathUnknown();
  final OsHandle file;
  try {
    file = anchored.openRead();
  } on Exception catch (error) {
    if (error is IoError && error.kind == FsErrorKind.notFound) {
      return const _PathAbsent();
    }
    return const _PathUnknown();
  }
  try {
    final identity = fileIdentity(file);
    final security = FileSecuritySnapshot.capture(file);
    final hash = _sha256Reader(file);
    return _PathPresent(
      identity: identity,
      sha256: hash,
      securitySha256: security.sha256(),
    );
  } on Exception {
    return const _PathUnknown();
  } finally {
    file.close();
  }
}

/// Authorizes [rawPath] under [root] and anchors it. Returns null when the root
/// does not authorize the path (`root.authorize(path).ok().and_then(anchor)`).
AnchoredPath? _anchorFor(CapabilityRootPort root, String rawPath) {
  try {
    final AuthorizedPathPort authorized = root.authorize(rawPath);
    final AnchoredPathPort anchor = authorized.anchor();
    // Every root in this engine is a cap_fs CapabilityRoot, so the anchor is a
    // concrete AnchoredPath that exposes the pinned-parent operations.
    return anchor as AnchoredPath;
  } on Exception {
    return null;
  }
}

/// `sha256_reader`: rewinds, hashes to EOF, rewinds again.
String _sha256Reader(OsHandle handle) => sha256OpenFile(handle);

int? _checkedAddInt(int left, int right) {
  if (left < 0 || right < 0) return null;
  if (left > 0x7fffffffffffffff - right) return null;
  return left + right;
}

FilesystemObjectIdentity? _identityOrNull(SecuredCreatedFile file) {
  try {
    return file.identity();
  } on Exception {
    return null;
  }
}

void _transition(
  JournalStore journals,
  ValidatedS5Record record,
  S5Transition transition,
) {
  final expected = record.revision;
  record.applyTransition(transition);
  journals.writeS5Cas(record, expected);
}

void _transitionPhase(
  JournalStore journals,
  ValidatedS5Record record,
  RetainedPublicationPhase expectedPhase,
  RetainedPublicationPhase nextPhase,
) {
  final expectedRevision = record.revision;
  record.markPublicationPhase(expectedPhase, nextPhase);
  journals.writeS5Cas(record, expectedRevision);
}
