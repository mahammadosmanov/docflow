// Port of src-tauri/src/fs_ops.rs: the Rust filesystem authority.
//
// Every mutation goes preview -> approval -> apply -> journal -> undo. Plans
// are bound to owner/session, paths are resolved only through pinned grant-root
// capabilities (cap_fs.dart), destinations are never overwritten, and each
// effect is journaled before and after it runs.

import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:uuid/uuid.dart';

import 'cap_fs.dart';
import 'contracts.dart';
import 'errors.dart' show BackendError;
import 'journal.dart';
import 'leaf_transaction.dart';
import 'ooxml.dart' show CellUpdate, patchWorkbookSheets;
import 'paths.dart';
import 'pdf_publish.dart' show temporaryPdfNameMatches;
import 'plan_store.dart';
import 'progress.dart';
import 'rfi_follow_up.dart' as rfi_follow_up;

// ---------------------------------------------------------------------------
// Operation identifiers and ports for modules not yet ported to Dart.
//
// rfi_log.dart, rfi_system_sync.dart, operations/replica_publish.dart, pdf.dart
// and undo.dart are owned by other workers. Until they land, fs_ops receives
// their behavior through the interfaces below, which mirror the Rust free
// functions one-for-one.
// ---------------------------------------------------------------------------

/// `rfi_system_sync::OPERATION_ID`.
const String rfiSystemSyncOperationId = 'rfi.system_sync';

/// `rfi_system_sync::COMPLETE_STATUS_OPERATION_ID`.
const String rfiCompleteStatusSyncOperationId = 'rfi.complete_status_sync';

/// `replica_publish::OPERATION_ID`.
const String replicaPublishOperationId = 'files.replica_publish';

const String _concreteDocumentOperation = 'rfi.generate_concrete_documents';
const String _concreteDocumentNote = 'step.rfi.generatedConcreteDocument';
const int _maxOutlookMsgBytes = 100 * 1024 * 1024;
const int _maxPackageBytesDart = 256 * 1024 * 1024;

/// Decoded `rfi.system_sync` step metadata (`rfi_system_sync::StepMeta`).
class RfiSystemSyncStepMeta {
  const RfiSystemSyncStepMeta({
    required this.targetPath,
    required this.backupPath,
    required this.sourceFingerprint,
  });

  final String targetPath;
  final String backupPath;
  final rfi_follow_up.FollowUpFingerprint sourceFingerprint;
}

/// Port of `rfi_system_sync`: step metadata decoding and the staged sync step.
abstract interface class RfiSystemSyncPort {
  RfiSystemSyncStepMeta decodeStepMeta(Step step);

  /// `execute_sync_step`: reads [input] bytes and returns the synced workbook.
  Uint8List executeSyncStep(Step step, Uint8List input);
}

/// Decoded `files.replica_publish` step metadata (`replica_publish::StepMeta`).
class ReplicaPublishStepMeta {
  const ReplicaPublishStepMeta({
    required this.backupPath,
    required this.sourceSha256,
    required this.expectedTargetSha256,
  });

  final String backupPath;
  final String sourceSha256;
  final String expectedTargetSha256;
}

/// Port of `operations::replica_publish`.
abstract interface class ReplicaPublishPort {
  ReplicaPublishStepMeta decodeStepMeta(Step step);
}

/// Port of `rfi_log` workbook helpers used by the workbook update step.
abstract interface class RfiLogWorkbookPort {
  /// `workbook_step_updates`.
  Map<String, Map<(int, int), CellUpdate>> workbookStepUpdates(Step step);

  /// `workbook_record_count`.
  int workbookRecordCount(Step step);

  /// `verify_workbook_step`: throws [BackendError] when [output] does not
  /// match the approved record updates.
  void verifyWorkbookStep(Step step, Uint8List output);
}

/// Port of the `pdf` module: incremental page operations on PDF bytes.
abstract interface class PdfEngine {
  /// `replace_first_page_incremental_from_page`.
  Uint8List replaceFirstPageIncrementalFromPage(
    Uint8List archive,
    Uint8List signedPdf,
    int signedPageIndex,
  );

  /// `rotate_pages_incremental`.
  Uint8List rotatePagesIncremental(Uint8List source, List<(int, int)> rotations);

  /// `extract_pages`.
  Uint8List extractPages(Uint8List source, List<int> pageIndexes);
}

/// Port of `crate::undo::undo_journal`.
typedef UndoJournalFn =
    OperationResult Function(
      JournalStore journals,
      String journalId,
      CallerIdentity caller,
      String? destination,
    );

// ---------------------------------------------------------------------------
// Results of step execution (`StepFailure`, `FollowUpApplyFailure`).
// ---------------------------------------------------------------------------

/// `StepFailure`: how one journaled step failed.
sealed class StepFailure implements Exception {
  const StepFailure();
}

final class StepNoEffect extends StepFailure {
  const StepNoEffect(this.subject);

  final String subject;

  @override
  String toString() => subject;
}

final class StepWarningNoEffect extends StepFailure {
  const StepWarningNoEffect(this.warning);

  final Warning warning;
}

final class StepOutcomeUnknown extends StepFailure {
  const StepOutcomeUnknown(this.subject);

  final String subject;

  @override
  String toString() => subject;
}

final class StepCancelled extends StepFailure {
  const StepCancelled();
}

/// `FollowUpApplyFailure`: how an S5 follow-up apply failed.
sealed class FollowUpApplyFailure implements Exception {
  const FollowUpApplyFailure();
}

final class FollowUpNoEffect extends FollowUpApplyFailure {
  const FollowUpNoEffect(this.warning);

  final Warning warning;
}

final class FollowUpPartial extends FollowUpApplyFailure {
  const FollowUpPartial(this.warning);

  final Warning warning;
}

final class FollowUpOutcomeUnknown extends FollowUpApplyFailure {
  const FollowUpOutcomeUnknown(this.warning);

  final Warning warning;
}

final class FollowUpCancelled extends FollowUpApplyFailure {
  const FollowUpCancelled();
}

/// `GeneratedWriteFailure`.
sealed class GeneratedWriteFailure {
  const GeneratedWriteFailure(this.key);

  final String key;
}

final class GeneratedNoEffect extends GeneratedWriteFailure {
  const GeneratedNoEffect(super.key);
}

final class GeneratedOutcomeUnknown extends GeneratedWriteFailure {
  const GeneratedOutcomeUnknown(super.key);
}

// ---------------------------------------------------------------------------
// Free functions: no-clobber copy, write and move, plus digests.
// ---------------------------------------------------------------------------

String _errorText(Object error) =>
    error is BackendError ? error.message : error.toString();

/// `copy_no_clobber`: copies [source] to [destination] through a temporary
/// sibling, then publishes it with a no-replace link.
void copyNoClobber(String source, String destination) {
  final parent = pathParent(destination);
  if (parent == null) {
    throw const FsIoException(
      FsErrorKind.invalidInput,
      'destination has no parent',
    );
  }
  final created = _createTemporary(parent, destination);
  final temporary = created.$1;
  final output = created.$2;
  try {
    final input = openSourceNoFollow(source);
    try {
      copyStream(input, output);
      output.setPermissionBits(input.metadata().permissionBits);
      output.syncAll();
    } finally {
      input.close();
    }
    output.close();
    _publishNoClobber(temporary, destination);
    syncDirectoryPath(parent);
  } catch (_) {
    try {
      removePathFile(temporary);
    } on Object {
      // Best effort cleanup; the original error is rethrown below.
    }
    rethrow;
  }
}

/// `write_no_clobber` for a plain path.
void writeNoClobberPath(String destination, List<int> contents) {
  final parent = pathParent(destination);
  if (parent == null) {
    throw const FsIoException(
      FsErrorKind.invalidInput,
      'destination has no parent',
    );
  }
  final created = _createTemporary(parent, destination);
  final temporary = created.$1;
  final output = created.$2;
  try {
    output.writeAll(Uint8List.fromList(contents));
    output.syncAll();
    output.close();
    _publishNoClobber(temporary, destination);
    syncDirectoryPath(parent);
  } catch (_) {
    try {
      removePathFile(temporary);
    } on Object {
      // Best effort cleanup; the original error is rethrown below.
    }
    rethrow;
  }
}

/// `move_no_clobber`: renames without replacing; falls back to copy plus
/// remove when the rename crosses devices (EXDEV).
void moveNoClobber(String source, String destination) {
  try {
    capabilityPlatform.renamePathNoReplace(source, destination);
    final destinationParent = pathParent(destination);
    if (destinationParent != null) {
      syncDirectoryPath(destinationParent);
    }
    final sourceParent = pathParent(source);
    if (sourceParent != null && destinationParent != sourceParent) {
      syncDirectoryPath(sourceParent);
    }
  } on IoError catch (error) {
    if (error.osError != _exdevCode) rethrow;
    copyNoClobber(source, destination);
    removePathFile(source);
    final sourceParent = pathParent(source);
    if (sourceParent != null) {
      syncDirectoryPath(sourceParent);
    }
  }
}

const int _exdevCode = 18;

/// `file_sha256`: SHA-256 of the file at [path], opened without following links.
String fileSha256(String path) {
  final file = openSourceNoFollow(path);
  try {
    return sha256OpenFile(file);
  } finally {
    file.close();
  }
}

// ---------------------------------------------------------------------------
// Internal helpers shared by the authority methods.
// ---------------------------------------------------------------------------

Uint8List _readAll(OsHandle handle) {
  handle.seek(0);
  final builder = BytesBuilder(copy: false);
  final buffer = Uint8List(64 * 1024);
  while (true) {
    final count = handle.read(buffer);
    if (count == 0) break;
    builder.add(Uint8List.sublistView(buffer, 0, count));
  }
  handle.seek(0);
  return builder.takeBytes();
}

/// Wraps an open workbook handle as the follow-up layer's workbook file.
rfi_follow_up.FollowUpWorkbookFile _followUpFile(OsHandle file, String path) {
  final identity = fileIdentity(file);
  final stat = file.metadata();
  final followUpIdentity = switch (identity) {
    UnixObjectIdentity(:final device, :final inode) =>
      rfi_follow_up.FollowUpFileIdentity.unix(device: device, inode: inode),
    WindowsObjectIdentity(:final volumeSerial, :final fileIndex) =>
      rfi_follow_up.FollowUpFileIdentity.windows(
        volumeSerial: volumeSerial,
        fileIndex: fileIndex,
      ),
  };
  return rfi_follow_up.FollowUpWorkbookFile(
    path: path,
    bytes: _readAll(file),
    identity: followUpIdentity,
    mtimeNs: stat.mtimeNs,
  );
}

/// `sha256_open_file` on a handle: rewinds, hashes, rewinds.
String _sha256Open(OsHandle file) => sha256OpenFile(file);

/// `fingerprint_open_file`.
rfi_follow_up.FollowUpFingerprint _fingerprintOpenFile(
  OsHandle file,
  String path,
) => rfi_follow_up.fingerprintWorkbookFile(_followUpFile(file, path));

/// Runs [body] and maps any thrown error to a no-effect step failure.
T _stepNoEffect<T>(T Function() body) {
  try {
    return body();
  } on StepFailure {
    rethrow;
  } on Object catch (error) {
    throw StepNoEffect(_errorText(error));
  }
}

/// Runs [body] and maps any thrown error to an outcome-unknown step failure.
T _stepUnknown<T>(T Function() body) {
  try {
    return body();
  } on StepFailure {
    rethrow;
  } on Object catch (error) {
    throw StepOutcomeUnknown(_errorText(error));
  }
}

/// Mirrors `required_path`.
String _requiredPath(String? raw, String label) {
  if (raw == null || raw.isEmpty) {
    throw StepNoEffect('$label is missing');
  }
  return raw;
}

/// Mirrors `insert_effect_state`.
void _insertEffectState(JsonObject meta, String state) {
  meta['effectState'] = state;
}

JsonObject _newMeta() => <String, Object?>{};

/// Mutable accumulator for an [OperationResult] (`result` in the Rust code is
/// updated in place while steps run).
class _Result {
  _Result({
    required this.applied,
    required this.failed,
    required this.journalId,
    required this.errors,
    required this.meta,
  });

  int applied;
  int failed;
  final String journalId;
  final List<Warning> errors;
  final JsonObject meta;

  OperationResult build() => OperationResult(
    applied: applied,
    failed: failed,
    journalId: journalId,
    errors: errors,
    meta: meta,
    cloudSynced: null,
  );
}

_Result _resultWith({
  required int applied,
  required int failed,
  required String journalId,
  List<Warning>? errors,
  JsonObject? meta,
}) => _Result(
  applied: applied,
  failed: failed,
  journalId: journalId,
  errors: errors ?? <Warning>[],
  meta: meta ?? _newMeta(),
);

/// `mark_cancelled`.
void _markCancelled(JournalRecord record, _Result result, int pending) {
  final partial = record.steps.isNotEmpty;
  record.state = partial ? JournalState.cancelledPartial : JournalState.cancelledClean;
  result.errors.add(const Warning(key: 'warn.operation.cancelled', subject: ''));
  result.meta['cancelled'] = true;
  result.meta['pending'] = pending;
  _insertEffectState(
    result.meta,
    partial ? 'cancelled_partial' : 'cancelled_clean',
  );
}

/// `operation_result_for_generated_failure`.
_Result _generatedFailureResult(
  JournalRecord record,
  String key,
  bool outcomeUnknown,
) {
  final result = _resultWith(
    applied: 0,
    failed: 1,
    journalId: record.journalId,
    errors: [Warning(key: key, subject: '')],
  );
  _insertEffectState(
    result.meta,
    outcomeUnknown ? 'outcome_unknown' : 'no_effect_retryable',
  );
  return result;
}

/// `operation_result_for_generated_cancelled`.
_Result _generatedCancelledResult(JournalRecord record) {
  final result = _resultWith(
    applied: 0,
    failed: 0,
    journalId: record.journalId,
    errors: [const Warning(key: 'warn.operation.cancelled', subject: '')],
  );
  result.meta['cancelled'] = true;
  _insertEffectState(result.meta, 'cancelled_clean');
  return result;
}

/// `decode_concrete_document_base64`.
Uint8List _decodeConcreteDocumentBase64(String value) {
  const maxBytes = _maxPackageBytesDart;
  final maxEncoded = ((maxBytes + 2) ~/ 3) * 4;
  if (value.length > maxEncoded) {
    throw const GeneratedNoEffect('error.rfi.documents.invalidOutput');
  }
  final List<int> bytes;
  try {
    bytes = base64.decode(value);
  } on FormatException {
    throw const GeneratedNoEffect('error.rfi.documents.invalidOutput');
  }
  if (bytes.length > maxBytes) {
    throw const GeneratedNoEffect('error.rfi.documents.invalidOutput');
  }
  return Uint8List.fromList(bytes);
}

bool _startsWithOutlookHeader(Uint8List bytes) {
  const header = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
  if (bytes.length < header.length) return false;
  for (var index = 0; index < header.length; index++) {
    if (bytes[index] != header[index]) return false;
  }
  return true;
}

/// `create_temporary`: exclusive sibling `.{name}.docflow-{uuid}.tmp`.
(String, OsHandle) _createTemporary(String parent, String destination) {
  if (!FileSystemEntity.isDirectorySync(parent)) {
    throw const FsIoException(
      FsErrorKind.notFound,
      'destination parent does not exist',
    );
  }
  final name = pathFileName(destination) ?? 'file';
  const uuid = Uuid();
  for (var attempt = 0; attempt < 16; attempt++) {
    final temporary = pathJoin(parent, '.$name.docflow-${uuid.v4()}.tmp');
    try {
      return (temporary, createExclusivePath(temporary));
    } on IoError catch (error) {
      if (error.kind == FsErrorKind.alreadyExists) continue;
      rethrow;
    }
  }
  throw const FsIoException(
    FsErrorKind.alreadyExists,
    'could not allocate a unique temporary file',
  );
}

/// `publish_no_clobber`: hard link into place, then remove the temporary.
void _publishNoClobber(String temporary, String destination) {
  capabilityPlatform.linkPath(temporary, destination);
  removePathFile(temporary);
}

bool _isWorkbookAccessError(Object error) {
  if (error is FsIoException && error.kind == FsErrorKind.permissionDenied) {
    return true;
  }
  final code = error is IoError ? error.osError : null;
  return code == 5 || code == 32 || code == 33;
}

/// `atomic_replace_capability_with_retry`: 20 attempts, 250 ms apart.
void _atomicReplaceCapabilityWithRetry(
  AnchoredPath source,
  AnchoredPath destination,
) {
  for (var attempt = 0; attempt < 20; attempt++) {
    try {
      destination.replaceWith(source);
      return;
    } on Object catch (error) {
      if (_isWorkbookAccessError(error) && attempt < 19) {
        sleep(const Duration(milliseconds: 250));
        continue;
      }
      rethrow;
    }
  }
  throw StateError('replacement retry loop always returns');
}

/// `remove_anchored_file_with_access_retry`: 40 attempts, 150 ms apart.
void _removeAnchoredFileWithAccessRetry(AnchoredPath path, int attempts) {
  final total = attempts < 1 ? 1 : attempts;
  for (var attempt = 0; attempt < total; attempt++) {
    try {
      path.removeFile();
      return;
    } on IoError catch (error) {
      if (error.kind == FsErrorKind.notFound) return;
      if (_isWorkbookAccessError(error) && attempt + 1 < total) {
        sleep(const Duration(milliseconds: 150));
        continue;
      }
      rethrow;
    }
  }
  throw const FsIoException(
    FsErrorKind.other,
    'capability-relative file could not be removed',
  );
}

/// `rollback_from_backup_capability`.
void _rollbackFromBackupCapability(
  AnchoredPath backup,
  AnchoredPath workbook,
  String expectedHash,
) {
  final temporary = workbook.temporarySibling('restore');
  temporary.copyNoClobberFrom(backup);
  try {
    if (temporary.sha256() != expectedHash) {
      throw const FsIoException(
        FsErrorKind.invalidData,
        'workbook rollback copy did not match the approved backup',
      );
    }
    _atomicReplaceCapabilityWithRetry(temporary, workbook);
    if (workbook.sha256() != expectedHash) {
      throw const FsIoException(
        FsErrorKind.invalidData,
        'workbook rollback verification failed',
      );
    }
  } catch (_) {
    try {
      temporary.removeFile();
    } on Object {
      // Best effort: the rollback error is the one reported.
    }
    rethrow;
  }
}

/// `replacement_failure`.
StepFailure _replacementFailure(AnchoredPath destination, Object error) =>
    StepOutcomeUnknown('${destination.absolute}: $error');

/// Opens the handle for [path] under a grant-root capability for read or write.
// (See FilesystemAuthority._anchor for the callers.)

/// Port of `FilesystemAuthority`: the single mutation authority.
class FilesystemAuthority {
  FilesystemAuthority({
    required String journalDir,
    required Duration ttl,
    required this.ooxml,
    required this.rfiLog,
    required this.rfiSystemSync,
    required this.pdf,
    required this.replicaPublish,
    required this.undoJournal,
    GlobalMutationLock? mutationLock,
  }) : plans = PlanStore(ttl),
       grants = PathGrantStore(ttl: ttl),
       journals = JournalStore(journalDir),
       mutationLock = mutationLock ?? GlobalMutationLock(),
       cancellations = CancellationRegistry();

  final PlanStore plans;
  final PathGrantStore grants;
  final JournalStore journals;
  final GlobalMutationLock mutationLock;
  final CancellationRegistry cancellations;

  /// OOXML adapter used by the follow-up S5 path (`FollowUpOoxml`).
  final rfi_follow_up.FollowUpOoxml ooxml;
  final RfiLogWorkbookPort rfiLog;
  final RfiSystemSyncPort rfiSystemSync;
  final PdfEngine pdf;
  final ReplicaPublishPort replicaPublish;
  final UndoJournalFn undoJournal;

  // Test hooks (`#[cfg(test)]` fields in Rust).
  bool _failAfterEffect = false;
  void Function()? _beforeCapabilityEffect;

  /// Test hook: fail the next file effect after it ran (`inject_post_effect_failure`).
  void injectPostEffectFailure() => _failAfterEffect = true;

  /// Test hook: run [hook] once before the next capability effect.
  void injectBeforeCapabilityEffect(void Function() hook) =>
      _beforeCapabilityEffect = hook;

  void _runBeforeCapabilityEffect() {
    final hook = _beforeCapabilityEffect;
    _beforeCapabilityEffect = null;
    hook?.call();
  }

  /// `issue_grant`.
  PathGrant issueGrant(
    GrantScope scope,
    String selectedRoot,
    GrantAccess access,
  ) => grants.issue(scope, selectedRoot, access);

  /// `issue_provisional_grants`.
  ProvisionalPathGrants issueProvisionalGrants(
    GrantScope scope,
    List<(String, GrantAccess)> requests,
  ) => grants.issueProvisional(scope, requests);

  /// `anchor_granted_path`.
  AnchoredPath anchorGrantedPath(
    PathGrant grant,
    String candidate,
    PathIntent intent,
  ) {
    try {
      return grants
          .authorizeCapabilityForGrant(grant, candidate, intent)
          .anchor();
    } on Object {
      throw BackendError.pathInvalid('selected workbook cannot be anchored');
    }
  }

  /// `reserve_plan_id`.
  ReservedPlanId reservePlanId() => plans.reservePlanId();

  /// `cache_reserved_plan`.
  Plan cacheReservedPlan(
    BackendOrigin origin,
    PlanBinding binding,
    ReservedPlanId reservation,
    Plan plan,
  ) {
    final caller = CallerIdentity(binding.ownerId, binding.sessionId);
    grants.rootsForPlan(binding.grantIds, caller, binding.operationId);
    return plans.insertBoundReserved(origin, binding, reservation, plan);
  }

  /// `cache_plan`.
  Plan cachePlan(BackendOrigin origin, PlanBinding binding, Plan plan) {
    final caller = CallerIdentity(binding.ownerId, binding.sessionId);
    grants.rootsForPlan(binding.grantIds, caller, binding.operationId);
    return plans.insertBound(origin, binding, plan);
  }

  /// `journals()`.
  JournalStore get journalStore => journals;

  /// `plan_origin`.
  BackendOrigin? planOrigin(String planId) => plans.snapshot(planId)?.origin;

  /// `validate_plan_operation`.
  String validatePlanOperation(
    String planId,
    CallerIdentity caller, {
    String? operationId,
  }) {
    final plan = plans.resolveBound(planId, caller);
    if (plan.origin != BackendOrigin.rust) {
      throw BackendError.validation(
        'plan is not owned by the Rust filesystem backend',
      );
    }
    if (operationId != null && operationId != plan.plan.opId) {
      throw BackendError.validation(
        'apply operation does not match the approved plan',
      );
    }
    return plan.plan.opId;
  }

  /// `resolve_native_plan`.
  ClaimedPlan resolveNativePlan(String planId, CallerIdentity caller) {
    final plan = plans.resolveBound(planId, caller);
    if (plan.origin != BackendOrigin.rust) {
      throw BackendError.validation(
        'plan is not owned by the Rust filesystem backend',
      );
    }
    return plan;
  }

  /// `anchor_bound_path`.
  AnchoredPath anchorBoundPath(
    PlanBinding binding,
    CallerIdentity caller,
    String candidate,
    PathIntent intent,
  ) {
    try {
      return grants
          .authorizeCapabilityForPlan(
            binding.grantIds,
            caller,
            binding.operationId,
            candidate,
            intent,
          )
          .anchor();
    } on Object {
      throw BackendError.pathInvalid('bound workbook cannot be anchored');
    }
  }

  /// `replace_cached_plan`.
  Plan replaceCachedPlan(
    PlanBinding binding,
    CallerIdentity caller,
    String planId,
    String expectedDigest,
    Plan replacement,
  ) {
    grants.rootsForPlan(binding.grantIds, caller, binding.operationId);
    return plans.replaceBoundAvailable(
      planId,
      caller,
      expectedDigest,
      replacement,
    );
  }

  /// `cancel`.
  bool cancel(String requestId) => cancellations.cancel(requestId);

  /// `register_cancellation`: the token plus a registration that finishes on
  /// [CancellationRegistration.close].
  (CancellationToken, CancellationRegistration) registerCancellation(
    String requestId,
  ) {
    final token = cancellations.register(requestId);
    return (token, CancellationRegistration._(cancellations, requestId));
  }

  /// `revoke`.
  void revoke(CallerIdentity caller) {
    plans.revoke(caller);
    grants.revoke(caller);
    cancellations.revokeAll();
  }

  /// `apply`.
  OperationResult apply(String planId, CallerIdentity caller) =>
      applyWithProgress(planId, caller, (_) {});

  /// `apply_generated_binary_write`: the Outlook `.msg` write path.
  OperationResult applyGeneratedBinaryWrite(
    String planId,
    CallerIdentity caller,
    String expectedOperation,
    void Function(ProgressEvent) progress,
    Uint8List Function() producer,
  ) {
    final inspected = plans.resolveBound(planId, caller);
    final steps = inspected.plan.steps;
    final step = steps.isEmpty ? null : steps.first;
    final isOutlook = expectedOperation == 'rfi.outlook_archive_selected';
    final validGeneratedStep = step != null &&
        step.kind == StepKind.write &&
        !step.meta.containsKey('content') &&
        !step.meta.containsKey('contentBase64') &&
        step.meta['generatedBy'] == 'outlook_msg';
    if (!isOutlook ||
        inspected.plan.opId != expectedOperation ||
        steps.length != 1 ||
        !validGeneratedStep) {
      throw BackendError.validation('generated write plan is invalid');
    }
    final claim = plans.claimForApplyBound(planId, caller, mutationLock);
    try {
      return _applyGeneratedBinaryWriteClaimed(
        planId,
        caller,
        expectedOperation,
        claim,
        progress,
        producer,
      ).build();
    } finally {
      claim.release();
    }
  }

  _Result _applyGeneratedBinaryWriteClaimed(
    String planId,
    CallerIdentity caller,
    String expectedOperation,
    ApplyClaim claim,
    void Function(ProgressEvent) progress,
    Uint8List Function() producer,
  ) {
    final binding = claim.claimed.binding;
    if (binding == null) {
      throw BackendError.planUnavailable();
    }
    final roots = grants.rootsForPlan(
      binding.grantIds,
      caller,
      binding.operationId,
    );
    final rootIdentities = grants.rootIdentitiesForPlan(
      binding.grantIds,
      caller,
      binding.operationId,
    );
    final token = cancellations.register(planId);
    final registration = CancellationRegistration._(cancellations, planId);
    try {
      final approved = claim.claimed.plan.steps[0];
      final record = JournalRecord.fresh(
        opId: expectedOperation,
        ownerId: binding.ownerId,
        sessionId: binding.sessionId,
        planDigest: claim.claimed.digest,
      );
      record.grantRoots = roots;
      record.grantRootIdentities = rootIdentities;
      record.state = JournalState.applying;
      journals.write(record);
      progress(
        ProgressEvent(
          phase: 'apply',
          done: 0,
          total: 1,
          subject: approved.src ?? '',
        ),
      );
      if (_checkpointFails(token)) {
        record.state = JournalState.cancelledClean;
        journals.write(record);
        return _generatedCancelledResult(record);
      }
      final Uint8List bytes;
      try {
        bytes = producer();
      } on GeneratedWriteFailure catch (failure) {
        record.state = failure is GeneratedOutcomeUnknown
            ? JournalState.recoveryRequired
            : JournalState.noEffectRetryable;
        journals.write(record);
        final result = _resultWith(
          applied: 0,
          failed: 1,
          journalId: record.journalId,
          errors: [Warning(key: failure.key, subject: '')],
        );
        _insertEffectState(
          result.meta,
          failure is GeneratedOutcomeUnknown
              ? 'outcome_unknown'
              : 'no_effect_retryable',
        );
        return result;
      }
      if (bytes.length > _maxOutlookMsgBytes ||
          !_startsWithOutlookHeader(bytes)) {
        record.state = JournalState.noEffectRetryable;
        journals.write(record);
        return _generatedFailureResult(
          record,
          'error.rfi.outlookArchive.invalidMsg',
          false,
        );
      }
      if (_checkpointFails(token)) {
        record.state = JournalState.cancelledClean;
        journals.write(record);
        return _generatedCancelledResult(record);
      }
      final destinationPath = approved.dst;
      if (destinationPath == null || destinationPath.isEmpty) {
        throw BackendError.validation('generated destination is missing');
      }
      final AnchoredPath destination;
      try {
        destination = grants
            .authorizeCapabilityForPlan(
              binding.grantIds,
              caller,
              binding.operationId,
              destinationPath,
              PathIntent.write,
            )
            .anchor();
      } on Object {
        throw BackendError.pathInvalid(
          'archive destination cannot be anchored',
        );
      }
      final actualMeta = Map<String, Object?>.of(approved.meta)
        ..remove('generatedBy')
        ..remove('contentBase64');
      journals.write(record);
      try {
        destination.writeNoClobber(bytes);
      } on IoError catch (error) {
        final exists = error.kind == FsErrorKind.alreadyExists;
        if (!exists && _openReadOk(destination)) {
          record.state = JournalState.recoveryRequired;
          journals.write(record);
          return _generatedFailureResult(
            record,
            'error.rfi.outlookArchive.writeUnknown',
            true,
          );
        }
        record.pendingStep = null;
        record.state = JournalState.noEffectRetryable;
        journals.write(record);
        return _generatedFailureResult(
          record,
          exists
              ? 'error.rfi.outlookArchive.destinationExists'
              : 'error.rfi.outlookArchive.writeFailed',
          false,
        );
      }
      final String hash;
      try {
        hash = destination.sha256();
      } on Object {
        record.state = JournalState.recoveryRequired;
        journals.write(record);
        return _generatedFailureResult(
          record,
          'error.rfi.outlookArchive.writeUnknown',
          true,
        );
      }
      actualMeta['undoSha256'] = hash;
      final actual = Step(
        kind: approved.kind,
        src: null,
        dst: approved.dst,
        noteKey: approved.noteKey,
        meta: actualMeta,
      );
      record.pendingStep = null;
      record.steps.add(actual);
      record.state = JournalState.committed;
      journals.write(record);
      progress(
        ProgressEvent(phase: 'apply', done: 1, total: 1, subject: ''),
      );
      final result = _resultWith(
        applied: 1,
        failed: 0,
        journalId: record.journalId,
      );
      _insertEffectState(result.meta, 'committed');
      return result;
    } finally {
      registration.close();
    }
  }

  bool _checkpointFails(CancellationToken token) {
    try {
      token.checkpoint();
      return false;
    } on BackendError {
      return true;
    }
  }

  bool _openReadOk(AnchoredPath path) {
    try {
      path.openRead().close();
      return true;
    } on Object {
      return false;
    }
  }

  /// `apply_with_progress`: dispatches to the S5, RFI sync and step paths.
  OperationResult applyWithProgress(
    String planId,
    CallerIdentity caller,
    void Function(ProgressEvent) progress,
  ) {
    final inspected = plans.resolveBound(planId, caller);
    final hasEncodedPayload = inspected.plan.steps.any(
      (step) => step.meta.containsKey('contentBase64'),
    );
    if (hasEncodedPayload) {
      final binding = inspected.binding;
      final invalid =
          binding?.operationId != _concreteDocumentOperation ||
          inspected.plan.opId != _concreteDocumentOperation ||
          inspected.plan.steps.any((step) {
            if (!step.meta.containsKey('contentBase64')) return false;
            return step.kind != StepKind.write ||
                step.noteKey != _concreteDocumentNote ||
                step.meta.containsKey('content') ||
                step.meta['contentBase64'] is! String;
          });
      if (invalid) {
        throw BackendError.validation(
          'encoded generated payload is not authorized for this operation',
        );
      }
    }
    if (inspected.plan.opId == rfi_follow_up.followUpOperationId) {
      if (inspected.plan.steps.isEmpty) {
        throw BackendError.validation('error.rfi.followUp.noRowsSelected');
      }
      return _applyFollowUp(planId, caller, progress).build();
    }
    if (inspected.plan.opId == rfiSystemSyncOperationId ||
        inspected.plan.opId == rfiCompleteStatusSyncOperationId) {
      if (inspected.plan.steps.isEmpty) {
        throw BackendError.validation(
          inspected.plan.opId == rfiCompleteStatusSyncOperationId
              ? 'warn.rfi.completeStatus.noChangesNeeded'
              : 'warn.rfi.systemSync.noChangesNeeded',
        );
      }
      return _applyRfiSystemSync(planId, caller, progress);
    }
    final claim = plans.claimForApplyBound(planId, caller, mutationLock);
    try {
      if (claim.claimed.origin != BackendOrigin.rust) {
        throw BackendError.validation(
          'plan is not owned by the Rust filesystem backend',
        );
      }
      final binding = claim.claimed.binding;
      if (binding == null) {
        throw BackendError.planUnavailable();
      }
      final roots = grants.rootsForPlan(
        binding.grantIds,
        caller,
        binding.operationId,
      );
      final rootIdentities = grants.rootIdentitiesForPlan(
        binding.grantIds,
        caller,
        binding.operationId,
      );
      final token = cancellations.register(planId);
      final registration = CancellationRegistration._(cancellations, planId);
      try {
        final record = JournalRecord.fresh(
          opId: claim.claimed.plan.opId,
          ownerId: binding.ownerId,
          sessionId: binding.sessionId,
          planDigest: claim.claimed.digest,
        );
        record.grantRoots = roots;
        record.grantRootIdentities = rootIdentities;
        record.state = JournalState.applying;
        final result = _resultWith(
          applied: 0,
          failed: 0,
          journalId: record.journalId,
        );
        final steps = claim.claimed.plan.steps;
        final failFast = claim.claimed.plan.opId == replicaPublishOperationId ||
            ((claim.claimed.plan.opId == 'rfi.process_scans' ||
                    claim.claimed.plan.opId == 'rfi.process_scans_fast_covers') &&
                steps.any((step) => step.kind == StepKind.pdfPagesExtract));
        if (failFast) {
          _preflightReplicaSteps(steps, binding, caller);
        }
        final total = steps.length;
        progress(ProgressEvent(phase: 'apply', done: 0, total: total, subject: ''));
        for (var index = 0; index < steps.length; index++) {
          final step = steps[index];
          if (_checkpointFails(token)) {
            _markCancelled(record, result, total - index);
            _cleanupSignedPageDirectories(binding, caller, record, result);
            journals.write(record);
            return result.build();
          }
          try {
            result.applied += _executeStep(step, binding, caller, token, record);
          } on StepNoEffect catch (failure) {
            result.failed += 1;
            result.errors.add(
              Warning(key: 'error.step_failed', subject: failure.subject),
            );
          } on StepWarningNoEffect catch (failure) {
            result.failed += 1;
            result.errors.add(failure.warning);
          } on StepOutcomeUnknown catch (failure) {
            result.failed += 1;
            result.errors.add(
              Warning(key: 'error.step_failed', subject: failure.subject),
            );
            record.state = JournalState.recoveryRequired;
            _insertEffectState(result.meta, 'outcome_unknown');
            journals.write(record);
            return result.build();
          } on StepCancelled {
            _markCancelled(record, result, total - index);
            _cleanupSignedPageDirectories(binding, caller, record, result);
            journals.write(record);
            return result.build();
          }
          if (failFast && result.failed > 0) {
            break;
          }
          progress(
            ProgressEvent(
              phase: 'apply',
              done: index + 1,
              total: total,
              subject: step.src ?? step.dst ?? '',
            ),
          );
        }
        if (result.failed > 0) {
          _cleanupSignedPageDirectories(binding, caller, record, result);
        }
        final noFailure = result.failed == 0;
        final hadProgress = result.applied > 0 || record.steps.isNotEmpty;
        record.state = noFailure
            ? JournalState.committed
            : hadProgress
            ? JournalState.partialCommitted
            : JournalState.noEffectRetryable;
        _insertEffectState(
          result.meta,
          noFailure
              ? 'committed'
              : hadProgress
              ? 'partial_committed'
              : 'no_effect_retryable',
        );
        journals.write(record);
        return result.build();
      } finally {
        registration.close();
      }
    } finally {
      claim.release();
    }
  }

  // -------------------------------------------------------------------------
  // S5 follow-up apply (`apply_follow_up`).
  // -------------------------------------------------------------------------

  _Result _applyFollowUp(
    String planId,
    CallerIdentity caller,
    void Function(ProgressEvent) progress,
  ) {
    final inspected = plans.resolveBound(planId, caller);
    final binding = inspected.binding;
    if (binding == null) {
      throw BackendError.planUnavailable();
    }
    final process = mutationLock.tryAcquire();
    final InterprocessMutationPermit interprocess;
    try {
      interprocess = journals.tryAcquireMutationLock();
    } on Object {
      process.release();
      rethrow;
    }
    final permits = journals.bindS5Permits(process, interprocess);
    final ClaimedS5Apply claim;
    try {
      final roots = grants.rootAccessesForPlan(
        binding.grantIds,
        caller,
        binding.operationId,
      );
      claim = plans.claimS5ForApplyBoundUnderPermit(
        planId,
        caller,
        permits,
        _GrantRosterAdapter(roots),
      );
    } on Object {
      permits.release();
      rethrow;
    }
    final ValidatedS5Record record;
    try {
      record = journals.beginS5(claim, caller);
    } on Object {
      claim.release();
      rethrow;
    }
    final token = cancellations.register(planId);
    final registration = CancellationRegistration._(cancellations, planId);
    try {
      progress(ProgressEvent(phase: 'apply', done: 0, total: 1, subject: ''));
      _Result result;
      try {
        final changed = _followUpWorkbookUpdate(caller, token, record);
        result = _resultWith(
          applied: changed,
          failed: 0,
          journalId: record.journalId,
        );
        _insertEffectState(result.meta, 'committed');
        _insertRetainedEvidence(result.meta, record.retainedEvidence);
      } on FollowUpNoEffect catch (failure) {
        final expected = record.revision;
        record.prepareNoEffect();
        journals.writeS5Cas(record, expected);
        result = _resultWith(
          applied: 0,
          failed: 1,
          journalId: record.journalId,
          errors: [failure.warning],
        );
        _insertEffectState(result.meta, 'no_effect_retryable');
      } on FollowUpPartial catch (failure) {
        final locators = record.retainedLocators();
        final notice = RecoveryNotice(
          warningKey: failure.warning.key,
          locator: locators.first,
        );
        final expected = record.revision;
        record.prepareFailedPartial(notice);
        journals.writeS5Cas(record, expected);
        result = _resultWith(
          applied: 0,
          failed: 1,
          journalId: record.journalId,
          errors: [failure.warning],
        );
        _insertEffectState(result.meta, 'partial');
        _insertRetainedLocators(result.meta, locators.asList);
        _insertRetainedEvidence(result.meta, record.retainedEvidence);
      } on FollowUpOutcomeUnknown catch (failure) {
        final notice = RecoveryNotice(
          warningKey: failure.warning.key,
          locator: failure.warning.subject,
        );
        final expected = record.revision;
        record.prepareOutcomeUnknown(notice);
        journals.writeS5Cas(record, expected);
        result = _resultWith(
          applied: 0,
          failed: 1,
          journalId: record.journalId,
          errors: [failure.warning],
        );
        _insertEffectState(result.meta, 'outcome_unknown');
        _insertRetainedEvidence(result.meta, record.retainedEvidence);
      } on FollowUpCancelled {
        final partial = record.retainedEvidence.isNotEmpty;
        final expected = record.revision;
        record.prepareCancelled();
        journals.writeS5Cas(record, expected);
        result = _resultWith(
          applied: 0,
          failed: 0,
          journalId: record.journalId,
          errors: [const Warning(key: 'warn.operation.cancelled', subject: '')],
        );
        result.meta['cancelled'] = true;
        _insertEffectState(
          result.meta,
          partial ? 'cancelled_partial' : 'cancelled_clean',
        );
        _insertRetainedEvidence(result.meta, record.retainedEvidence);
      }
      progress(ProgressEvent(phase: 'apply', done: 1, total: 1, subject: ''));
      return result;
    } finally {
      registration.close();
      claim.release();
    }
  }

  /// `s5_transition`: applies one journaled transition under revision CAS.
  /// Any failure is an outcome-unknown follow-up failure.
  void _s5Transition(ValidatedS5Record record, S5Transition transition) {
    final expected = record.revision;
    try {
      record.applyTransition(transition);
      journals.writeS5Cas(record, expected);
    } on Object {
      throw _followUpOutcome(record);
    }
  }

  /// `transition_s5_backend`: like [_s5Transition] but keeps the backend error.
  void _transitionS5Backend(ValidatedS5Record record, S5Transition transition) {
    final expected = record.revision;
    record.applyTransition(transition);
    journals.writeS5Cas(record, expected);
  }

  /// `follow_up_outcome`.
  FollowUpOutcomeUnknown _followUpOutcome(ValidatedS5Record record) {
    String locator;
    try {
      locator = record.retainedLocators().first;
    } on Object {
      locator = 'journal:${record.journalId};root:0;leaf:record';
    }
    return FollowUpOutcomeUnknown(
      Warning(key: 'error.rfi.followUp.recoveryRequired', subject: locator),
    );
  }

  /// `s5_locator`.
  String _s5Locator(ValidatedS5Record record, String path) {
    final roots = record.roots;
    if (roots == null) {
      throw const FollowUpNoEffect(
        Warning(key: 'error.rfi.followUp.workbookInvalid', subject: ''),
      );
    }
    final index = roots.roots.indexWhere(
      (root) => pathStartsWith(path, root.canonical),
    );
    if (index < 0) {
      throw const FollowUpNoEffect(
        Warning(key: 'error.rfi.followUp.workbookInvalid', subject: ''),
      );
    }
    final leaf = pathFileName(path);
    if (leaf == null) {
      throw const FollowUpNoEffect(
        Warning(key: 'error.rfi.followUp.workbookInvalid', subject: ''),
      );
    }
    return 'journal:${record.journalId};root:$index;leaf:$leaf';
  }

  /// `follow_up_no_effect`.
  static FollowUpNoEffect _fuNoEffect(String key) =>
      FollowUpNoEffect(Warning(key: key, subject: ''));

  /// `follow_up_partial`.
  static FollowUpPartial _fuPartial(String key) =>
      FollowUpPartial(Warning(key: key, subject: ''));

  /// Maps a backend error to no-effect, keyed by its message.
  static FollowUpNoEffect _fuBackendNoEffect(Object error) =>
      FollowUpNoEffect(Warning(key: _errorText(error), subject: ''));

  /// Maps a backend error to a partial failure, keyed by its message.
  static FollowUpPartial _fuBackendPartial(Object error) =>
      FollowUpPartial(Warning(key: _errorText(error), subject: ''));

  /// `execute_follow_up_workbook_update`: the S5 retained-publication state
  /// machine for one `rfi.follow_up_sync` step. Returns the changed-cell count.
  int _followUpWorkbookUpdate(
    CallerIdentity caller,
    CancellationToken token,
    ValidatedS5Record record,
  ) {
    final plan = record.plan;
    if (plan == null || plan.opId != rfi_follow_up.followUpOperationId) {
      throw _fuNoEffect('error.rfi.followUp.workbookInvalid');
    }
    final step = record.approvedStep;
    final binding = record.binding;
    if (binding == null) {
      throw _fuNoEffect('error.rfi.followUp.workbookInvalid');
    }
    final (sourcePathRaw, targetPathRaw, backupPathRaw) = _guardFu(
      () => rfi_follow_up.stepWorkbookPaths(step),
    );
    final sourcePath = sourcePathRaw;
    final targetPath = targetPathRaw;
    final backupPath = backupPathRaw;

    final AnchoredPath source;
    final AnchoredPath canonical;
    final AnchoredPath backup;
    try {
      source = grants
          .authorizeCapabilityForPlan(
            binding.grantIds,
            caller,
            binding.operationId,
            sourcePath,
            PathIntent.read,
          )
          .anchor();
    } on Object {
      throw _fuBackendNoEffect(
        BackendError.pathInvalid('RFI Log workbook is unavailable'),
      );
    }
    try {
      canonical = grants
          .authorizeCapabilityForPlan(
            binding.grantIds,
            caller,
            binding.operationId,
            targetPath,
            PathIntent.write,
          )
          .anchor();
    } on Object {
      throw _fuBackendNoEffect(
        BackendError.pathInvalid('Follow Up workbook is unavailable'),
      );
    }
    try {
      backup = grants
          .authorizeCapabilityForPlan(
            binding.grantIds,
            caller,
            binding.operationId,
            backupPath,
            PathIntent.write,
          )
          .anchor();
    } on Object {
      throw _fuBackendNoEffect(
        BackendError.pathInvalid('Follow Up backup cannot be anchored'),
      );
    }

    final OsHandle sourceFile;
    try {
      sourceFile = source.openRead();
    } on Object {
      throw _fuNoEffect('error.rfi.followUp.workbookInvalid');
    }
    final OsHandle originalFile;
    try {
      originalFile = canonical.openMutable();
    } on Object {
      sourceFile.close();
      throw _fuNoEffect('error.rfi.followUp.workbookLocked');
    }
    try {
      _guardFu(
        () => rfi_follow_up.revalidateOpenWorkbooks(
          step,
          _followUpFile(sourceFile, sourcePath),
          _followUpFile(originalFile, targetPath),
        ),
      );
      if (_checkpointFails(token)) {
        throw const FollowUpCancelled();
      }
      final FilesystemObjectIdentity originalIdentity;
      final int originalLen;
      final FileSecuritySnapshot originalSnapshot;
      final ParentSecuritySnapshot parentSnapshot;
      final CreationSecurityDescriptor creation;
      final String originalHash;
      try {
        originalIdentity = fileIdentity(originalFile);
      } on Object {
        throw _fuNoEffect('error.rfi.followUp.workbookChanged');
      }
      try {
        originalLen = originalFile.metadata().size;
      } on Object {
        throw _fuNoEffect('error.rfi.followUp.workbookChanged');
      }
      try {
        originalSnapshot = FileSecuritySnapshot.capture(originalFile);
      } on Object {
        throw _fuNoEffect('error.rfi.followUp.workbookInvalid');
      }
      try {
        parentSnapshot = canonical.captureParentSecurity();
      } on Object {
        throw _fuNoEffect('error.rfi.followUp.workbookChanged');
      }
      try {
        creation = originalSnapshot.creationDescriptor(parentSnapshot);
      } on Object {
        throw _fuNoEffect('error.rfi.followUp.workbookInvalid');
      }
      try {
        originalHash = canonical.sha256();
      } on Object {
        throw _fuNoEffect('error.rfi.followUp.workbookChanged');
      }
      final canonicalParentPath = pathParent(targetPath);
      if (canonicalParentPath == null) {
        throw _fuNoEffect('error.rfi.followUp.workbookInvalid');
      }
      final privateParentPath = pathJoin(canonicalParentPath, '.docflow-backups');
      final usage = _guardFu(
        () => _s5RecordedRetainedUsage(record, canonicalParentPath),
        _fuBackendNoEffect,
      );
      final reservation = _guardFuWith(
        () => RetainedStorageReservation.forApply(
          originalLen,
          originalSnapshot.auxiliaryChargeBytes(),
        ),
        (_) => _fuNoEffect('error.rfi.followUp.retainedStorageFull'),
      );
      if (usage.$1 + reservation.fileCount > maxS5RetainedFiles ||
          usage.$2 + reservation.logicalChargeBytes > maxS5RetainedBytes) {
        throw _fuNoEffect('error.rfi.followUp.retainedStorageFull');
      }

      final parentLocator = _s5Locator(record, privateParentPath);
      _s5Transition(
        record,
        SetPendingArtifactParent(
          PendingArtifactParent(
            path: privateParentPath,
            locator: parentLocator,
          ),
        ),
      );
      final AuthorizedPath privateParent;
      try {
        privateParent = grants.authorizeCapabilityForPlan(
          binding.grantIds,
          caller,
          binding.operationId,
          privateParentPath,
          PathIntent.write,
        );
      } on Object catch (error) {
        throw _fuBackendPartial(error);
      }
      final ArtifactParentCreateOutcome parentOutcome;
      try {
        parentOutcome = privateParent.createArtifactParentNoClobber();
      } on Object {
        throw _followUpOutcome(record);
      }
      if (parentOutcome is ArtifactParentCreated) {
        try {
          privateParent.anchor().syncParent();
        } on Object {
          throw _fuPartial('error.rfi.followUp.recoveryRequired');
        }
      }
      _s5Transition(record, ResolveArtifactParent(parentOutcome));
      if (record.pendingArtifactParent != null &&
          record.phase == FollowUpJournalPhase.artifactParentReady) {
        throw _followUpOutcome(record);
      }

      final extension = pathExtension(targetPath);
      if (extension == null) {
        throw _fuPartial('error.rfi.followUp.workbookInvalid');
      }
      final privatePath = pathJoin(
        privateParentPath,
        '.docflow-s5-private-output-${record.journalId}-${_simpleUuid()}.$extension',
      );
      final privateLocator = _s5Locator(record, privatePath);
      _s5Transition(
        record,
        SetPendingRetainedArtifact(
          PendingRetainedArtifact(
            role: RetainedArtifactRole.privateOutput,
            path: privatePath,
            locator: privateLocator,
            creationState: const PendingArtifactCreatedIntent(),
          ),
        ),
      );
      final AnchoredPath privateAnchor;
      try {
        privateAnchor = grants
            .authorizeCapabilityForPlan(
              binding.grantIds,
              caller,
              binding.operationId,
              privatePath,
              PathIntent.write,
            )
            .anchor();
      } on Object {
        throw _fuBackendPartial(
          BackendError.pathInvalid('Follow Up private output cannot be anchored'),
        );
      }
      final CreationSecurityDescriptor privateCreation;
      try {
        privateCreation = CreationSecurityDescriptor.currentOwnerOnly();
      } on Object {
        throw _fuPartial('error.rfi.followUp.workbookInvalid');
      }
      final SecuredCreatedFile privateFile;
      final FilesystemObjectIdentity privateIdentity;
      final SecuredCreateOutcome privateOutcome;
      try {
        privateOutcome = privateAnchor.createNewSecuredIdentified(
          privateCreation,
        );
      } on Object {
        throw _followUpOutcome(record);
      }
      switch (privateOutcome) {
        case SecuredCreated(:final file, :final identity):
          privateFile = file;
          privateIdentity = identity;
        case SecuredCreatedIdentityUnknown():
          _s5Transition(record, const MarkPendingRetainedIdentityUnknown());
          throw _followUpOutcome(record);
      }
      _s5Transition(
        record,
        MarkPendingRetainedCreated(privateIdentity),
      );
      final String privateHash;
      final int privateLen;
      final FileSecuritySnapshot privateSnapshot;
      final Uint8List patched;
      try {
        final originalBytes = _readAll(originalFile);
        patched = rfi_follow_up.writeAndVerifyPrivateOutput(
          step,
          ooxml,
          originalBytes,
        );
        privateFile.handle.writeAll(patched);
        privateFile.syncAll();
        privateSnapshot = privateFile.securitySnapshot();
        privateHash = privateFile.sha256();
        privateLen = privateFile.len();
      } on FollowUpApplyFailure {
        rethrow;
      } on Object catch (error) {
        throw _fuBackendPartial(error);
      }
      final privateEvidence = _retainedEvidenceOf(
        RetainedArtifactRole.privateOutput,
        privatePath,
        privateIdentity,
        privateHash,
        privateSnapshot,
        privateLen,
        privateLocator,
      );
      _s5Transition(
        record,
        FinalizeRetainedArtifact(
          evidence: privateEvidence,
          phase: FollowUpJournalPhase.privateOutputReady,
        ),
      );
      if (_checkpointFails(token)) {
        throw const FollowUpCancelled();
      }

      _guardFuWith(
        () => rfi_follow_up.revalidateOpenWorkbooks(
          step,
          _followUpFile(sourceFile, sourcePath),
          _followUpFile(originalFile, targetPath),
        ),
        _fuBackendPartial,
      );
      _verifyParentOrPartial(canonical, parentSnapshot, 'error.rfi.followUp.workbookChanged');

      final backupLocator = _s5Locator(record, backupPath);
      _s5Transition(
        record,
        SetPendingRetainedArtifact(
          PendingRetainedArtifact(
            role: RetainedArtifactRole.backup,
            path: backupPath,
            locator: backupLocator,
            creationState: const PendingArtifactCreatedIntent(),
          ),
        ),
      );
      final SecuredCreatedFile backupFile;
      final FilesystemObjectIdentity backupIdentity;
      final SecuredCreateOutcome backupOutcome;
      try {
        backupOutcome = backup.createNewSecuredIdentified(creation);
      } on Object {
        throw _fuPartial('error.rfi.followUp.backupExists');
      }
      switch (backupOutcome) {
        case SecuredCreated(:final file, :final identity):
          backupFile = file;
          backupIdentity = identity;
        case SecuredCreatedIdentityUnknown():
          _s5Transition(record, const MarkPendingRetainedIdentityUnknown());
          throw _followUpOutcome(record);
      }
      _s5Transition(record, MarkPendingRetainedCreated(backupIdentity));
      final String backupHash;
      try {
        originalSnapshot.verifyCreatedAccess(backupFile);
        originalSnapshot.replayAndVerifyAuxiliary(backupFile);
        final copyInput = originalFile;
        copyInput.seek(0);
        copyStream(copyInput, backupFile.handle);
        backupFile.syncAll();
        originalSnapshot.verifyComplete(backupFile);
        backupHash = backupFile.sha256();
      } on Object {
        throw _fuPartial('error.rfi.followUp.backupVerify');
      }
      if (backupHash != originalHash) {
        throw _fuPartial('error.rfi.followUp.backupVerify');
      }
      final backupEvidence = _retainedEvidenceOf(
        RetainedArtifactRole.backup,
        backupPath,
        backupIdentity,
        backupHash,
        originalSnapshot,
        originalLen,
        backupLocator,
      );
      _s5Transition(
        record,
        FinalizeRetainedArtifact(
          evidence: backupEvidence,
          phase: FollowUpJournalPhase.backupReady,
        ),
      );
      if (_checkpointFails(token)) {
        throw const FollowUpCancelled();
      }

      _guardFuWith(
        () => rfi_follow_up.revalidateOpenWorkbooks(
          step,
          _followUpFile(sourceFile, sourcePath),
          _followUpFile(originalFile, targetPath),
        ),
        _fuBackendPartial,
      );
      _verifyParentOrPartial(canonical, parentSnapshot, 'error.rfi.followUp.workbookChanged');

      final retainedPath = pathJoin(
        canonicalParentPath,
        '.docflow-s5-retained-original-${record.journalId}-${_simpleUuid()}.$extension',
      );
      final retainedLocator = _s5Locator(record, retainedPath);
      final AnchoredPath retained;
      try {
        retained = grants
            .authorizeCapabilityForPlan(
              binding.grantIds,
              caller,
              binding.operationId,
              retainedPath,
              PathIntent.write,
            )
            .anchor();
      } on Object {
        throw _fuBackendPartial(
          BackendError.pathInvalid(
            'Follow Up retained original cannot be anchored',
          ),
        );
      }
      _s5Transition(
        record,
        SetPendingRetainedArtifact(
          PendingRetainedArtifact(
            role: RetainedArtifactRole.displacedOriginal,
            path: retainedPath,
            locator: retainedLocator,
            creationState: const PendingArtifactCreatedIntent(),
          ),
        ),
      );
      final int chargeBytes;
      final int logicalBytes;
      try {
        chargeBytes = originalSnapshot.auxiliaryChargeBytes();
        logicalBytes = _checkedAddOrThrow(originalLen, chargeBytes);
      } on Object {
        throw _fuPartial('error.rfi.followUp.retainedStorageFull');
      }
      _s5Transition(
        record,
        InitializePublication(
          RetainedPublication(
            purpose: RetainedPublicationPurpose.apply,
            canonical: targetPath,
            retained: retainedPath,
            backup: backupPath,
            publicationSource: privatePath,
            expectedCanonicalIdentity: originalIdentity,
            expectedCanonicalSha256: originalHash,
            expectedBackupIdentity: backupIdentity,
            expectedBackupSha256: backupHash,
            publicationSourceIdentity: privateIdentity,
            publicationSha256: privateHash,
            expectedCanonicalSecuritySha256: originalSnapshot.sha256(),
            expectedBackupSecuritySha256: originalSnapshot.sha256(),
            publicationSecuritySha256: privateSnapshot.sha256(),
            expectedCanonicalDefaultStreamBytes: originalLen,
            expectedCanonicalAuxiliaryChargeBytes: chargeBytes,
            expectedCanonicalLogicalChargeBytes: logicalBytes,
            destinationAccessSha256: originalSnapshot.accessSha256(),
            destinationAuxiliarySha256: originalSnapshot.auxiliarySha256(),
            destinationAuxiliaryChargeBytes: chargeBytes,
            retainedIdentity: null,
            retainedSha256: null,
            retainedSecuritySha256: null,
            createdCanonicalIdentity: null,
            phase: RetainedPublicationPhase.prepared,
          ),
        ),
      );

      final RetainedPublicationOutcome outcome;
      try {
        outcome = executeRetainedPublication(
          journals: journals,
          record: record,
          canonical: canonical,
          retained: retained,
          publicationSource: privateFile.handle,
          destinationCreation: creation,
          destinationSnapshot: originalSnapshot,
          cancellation: ApplyPublicationCancellation(token),
          beforeCanonicalCreate: () {
            _guardFuWith(
              () => rfi_follow_up.revalidateOpenWorkbooks(
                step,
                _followUpFile(sourceFile, sourcePath),
                _followUpFile(originalFile, targetPath),
              ),
              _fuBackendNoEffect,
            );
          },
          verifyDestination: (created) {
            _guardFuWith(
              () => rfi_follow_up.verifyPatchedOutput(
                step,
                ooxml,
                _readAll(originalFile),
                _readAll(created.handle),
              ),
              _fuBackendPartial,
            );
          },
        );
      } on BackendError {
        throw _followUpOutcome(record);
      }
      switch (outcome) {
        case RetainedPublicationCancelledBeforeDisplacement():
          throw const FollowUpCancelled();
        case RetainedPublicationRecoveryRequired(:final notice):
          throw FollowUpOutcomeUnknown(
            Warning(key: notice.warningKey, subject: notice.locator),
          );
        case RetainedPublicationPublished(:final createdIdentity):
          try {
            rfi_follow_up.revalidateOpenWorkbooks(
              step,
              _followUpFile(sourceFile, sourcePath),
              _followUpFile(originalFile, targetPath),
            );
          } on Object {
            throw _followUpOutcome(record);
          }
          final completed = Step(
            kind: StepKind.restoreBackup,
            src: backupPath,
            dst: targetPath,
            noteKey: 'note.rfi.followUp.workbookUpdate',
            meta: <String, Object?>{
              'originalSha256': originalHash,
              'appliedSha256': privateHash,
              'createdCanonicalIdentity': createdIdentity.toJson(),
            },
          );
          _s5Transition(
            record,
            FinalizeApplySuccess(completedStep: completed, evidence: const []),
          );
          return _guardFu(() => rfi_follow_up.changedCellCount(step));
      }
    } finally {
      _closeQuietly(sourceFile);
      _closeQuietly(originalFile);
    }
  }

  void _verifyParentOrPartial(
    AnchoredPath canonical,
    ParentSecuritySnapshot parentSnapshot,
    String key,
  ) {
    try {
      canonical.verifyParentSecurity(parentSnapshot);
    } on Object {
      throw _fuPartial(key);
    }
  }

  /// Retained evidence record with the auxiliary charge recomputed (`retained_evidence`).
  RetainedArtifactEvidence _retainedEvidenceOf(
    RetainedArtifactRole role,
    String path,
    FilesystemObjectIdentity identity,
    String sha256,
    FileSecuritySnapshot security,
    int defaultStreamBytes,
    String locator,
  ) {
    final auxiliaryChargeBytes = security.auxiliaryChargeBytes();
    final logical = _checkedAddOrNull(defaultStreamBytes, auxiliaryChargeBytes);
    if (logical == null) {
      throw _fuPartial('error.rfi.followUp.retainedStorageFull');
    }
    return RetainedArtifactEvidence(
      role: role,
      path: path,
      identity: identity,
      sha256: sha256,
      securitySha256: security.sha256(),
      defaultStreamBytes: defaultStreamBytes,
      auxiliaryChargeBytes: auxiliaryChargeBytes,
      logicalChargeBytes: logical,
      locator: locator,
    );
  }

  /// Runs [body], mapping errors through [map] (default: no-effect by message).
  T _guardFu<T>(
    T Function() body, [
    FollowUpApplyFailure Function(Object error)? map,
  ]) {
    try {
      return body();
    } on FollowUpApplyFailure {
      rethrow;
    } on Object catch (error) {
      throw (map ?? _fuBackendNoEffect)(error);
    }
  }

  /// Runs [body], mapping any error through [map].
  T _guardFuWith<T>(
    T Function() body,
    FollowUpApplyFailure Function(Object error) map,
  ) => _guardFu(body, map);

  /// `s5_recorded_retained_usage`: (file count, logical bytes) of retained
  /// artifacts in the canonical directory and its `.docflow-backups` child.
  (int, int) _s5RecordedRetainedUsage(
    ValidatedS5Record record,
    String canonicalParent,
  ) {
    final canonical = _authorizeS5RecordedDirectory(record, canonicalParent);
    final RetainedStorageUsage canonicalUsage;
    try {
      canonicalUsage = canonical.s5RetainedStorageUsage(maxS5RetainedFiles);
    } on Object {
      throw BackendError.validation('error.rfi.followUp.retainedStorageFull');
    }
    final privateParent = pathJoin(canonicalParent, '.docflow-backups');
    final RetainedStorageUsage privateUsage;
    try {
      privateUsage = _authorizeS5RecordedDirectory(record, privateParent)
          .s5RetainedStorageUsage(maxS5RetainedFiles);
    } on IoError catch (error) {
      if (error.kind != FsErrorKind.notFound) {
        throw BackendError.validation('error.rfi.followUp.retainedStorageFull');
      }
      return _checkedPairOrThrow(
        canonicalUsage.fileCount,
        canonicalUsage.logicalChargeBytes,
        0,
        0,
      );
    } on Object {
      throw BackendError.validation('error.rfi.followUp.retainedStorageFull');
    }
    return _checkedPairOrThrow(
      canonicalUsage.fileCount,
      canonicalUsage.logicalChargeBytes,
      privateUsage.fileCount,
      privateUsage.logicalChargeBytes,
    );
  }

  (int, int) _checkedPairOrThrow(int files, int bytes, int moreFiles, int moreBytes) {
    final count = files + moreFiles;
    final total = _checkedAddOrNull(bytes, moreBytes);
    if (total == null) {
      throw BackendError.validation('error.rfi.followUp.retainedStorageFull');
    }
    return (count, total);
  }

  /// `authorize_s5_recorded_directory`.
  AuthorizedPath _authorizeS5RecordedDirectory(
    ValidatedS5Record record,
    String path,
  ) {
    final roots = record.roots;
    if (roots == null) {
      throw BackendError.journalInvalid('S5 root authority is unavailable');
    }
    for (final root in roots.roots) {
      try {
        return (root as CapabilityRoot).authorizeDirectory(path);
      } on Object {
        continue;
      }
    }
    throw BackendError.pathEscape();
  }

  // -------------------------------------------------------------------------
  // RFI system sync (`apply_rfi_system_sync`).
  // -------------------------------------------------------------------------

  OperationResult _applyRfiSystemSync(
    String planId,
    CallerIdentity caller,
    void Function(ProgressEvent) progress,
  ) {
    final claim = plans.claimForApplyBound(planId, caller, mutationLock);
    try {
      if (claim.claimed.origin != BackendOrigin.rust) {
        throw BackendError.validation(
          'plan is not owned by the Rust filesystem backend',
        );
      }
      final binding = claim.claimed.binding;
      if (binding == null) {
        throw BackendError.planUnavailable();
      }
      final roots = grants.rootsForPlan(
        binding.grantIds,
        caller,
        binding.operationId,
      );
      final rootIdentities = grants.rootIdentitiesForPlan(
        binding.grantIds,
        caller,
        binding.operationId,
      );
      final token = cancellations.register(planId);
      final registration = CancellationRegistration._(cancellations, planId);
      try {
        final record = JournalRecord.fresh(
          opId: claim.claimed.plan.opId,
          ownerId: binding.ownerId,
          sessionId: binding.sessionId,
          planDigest: claim.claimed.digest,
        );
        record.grantRoots = roots;
        record.grantRootIdentities = rootIdentities;
        record.state = JournalState.applying;
        journals.write(record);

        final result = _resultWith(
          applied: 0,
          failed: 0,
          journalId: record.journalId,
        );
        final steps = claim.claimed.plan.steps;

        // Validate every approved workbook before creating a backup or changing
        // either canonical file, so a stale multi-workbook plan cannot apply
        // partially.
        for (final step in steps) {
          final stepMeta = rfiSystemSync.decodeStepMeta(step);
          final workbookPath = step.src;
          final backupPath = step.dst;
          if (workbookPath == null) {
            throw BackendError.validation('Step is missing source path');
          }
          if (backupPath == null) {
            throw BackendError.validation('Step is missing backup path');
          }
          if (workbookPath != stepMeta.targetPath ||
              backupPath != stepMeta.backupPath) {
            throw BackendError.validation(
              'error.rfi.systemSync.approvedUpdateInvalid',
            );
          }
          final capability = grants.authorizeCapabilityForPlan(
            binding.grantIds,
            caller,
            binding.operationId,
            workbookPath,
            PathIntent.write,
          );
          final anchor = _backendAnchor(() => capability.anchor());
          final OsHandle workbook;
          try {
            workbook = anchor.openMutable();
          } on Object {
            throw BackendError.validation('error.rfi.systemSync.workbookLocked');
          }
          try {
            if (_fingerprintOpenFile(workbook, workbookPath) !=
                stepMeta.sourceFingerprint) {
              throw BackendError.validation(
                'error.rfi.systemSync.workbookChanged',
              );
            }
          } finally {
            workbook.close();
          }
        }

        final totalSteps = steps.length;
        for (var stepIndex = 0; stepIndex < steps.length; stepIndex++) {
          final step = steps[stepIndex];
          if (_checkpointFails(token)) {
            record.state = JournalState.cancelledPartial;
            try {
              journals.write(record);
            } on Object {
              // Mirrors `let _ = self.journals.write(&record)`.
            }
            throw BackendError.stopped();
          }
          final srcPath = step.src;
          final dstPath = step.dst;
          if (srcPath == null) {
            throw BackendError.validation('Step is missing source path');
          }
          if (dstPath == null) {
            throw BackendError.validation('Step is missing backup path');
          }
          final workbookCap = grants.authorizeCapabilityForPlan(
            binding.grantIds,
            caller,
            binding.operationId,
            srcPath,
            PathIntent.write,
          );
          final workbookAnchor = _backendAnchor(() => workbookCap.anchor());

          final stepMeta = rfiSystemSync.decodeStepMeta(step);
          final currentWorkbook = _openMutableOrValidation(workbookAnchor);
          try {
            if (_fingerprintOpenFile(currentWorkbook, srcPath) !=
                stepMeta.sourceFingerprint) {
              throw BackendError.validation(
                'error.rfi.systemSync.workbookChanged',
              );
            }
          } finally {
            currentWorkbook.close();
          }

          final backupCap = grants.authorizeCapabilityForPlan(
            binding.grantIds,
            caller,
            binding.operationId,
            dstPath,
            PathIntent.write,
          );
          final backupAnchor = _backendAnchor(() => backupCap.anchor());

          final actualHash = _backendValue(() => workbookAnchor.sha256());

          final backupMeta = <String, Object?>{'sourceSha256': actualHash};
          var backupStep = Step(
            kind: StepKind.copy,
            src: srcPath,
            dst: dstPath,
            noteKey: 'note.rfi.systemSync.backup',
            meta: backupMeta,
          );
          record.pendingStep = backupStep;
          journals.write(record);

          try {
            backupAnchor.copyNoClobberFrom(workbookAnchor);
          } on Object catch (error) {
            record.pendingStep = null;
            _tryWrite(record);
            throw BackendError.validation(
              'Workbook backup failed for $dstPath: $error',
            );
          }

          final backupHash = _backendValue(() => backupAnchor.sha256());
          if (backupHash != actualHash) {
            record.pendingStep = null;
            _tryWrite(record);
            throw BackendError.validation('Backup verification hash mismatch');
          }
          backupStep = Step(
            kind: backupStep.kind,
            src: backupStep.src,
            dst: backupStep.dst,
            noteKey: backupStep.noteKey,
            meta: <String, Object?>{
              ...backupStep.meta,
              'undoSha256': backupHash,
            },
          );
          record.steps.add(backupStep);
          record.pendingStep = null;
          journals.write(record);

          // Stage the workbook in memory, sync it through the port, then write
          // the staged output back to the canonical path.
          final stagedInput = _readHandleBytes(workbookAnchor);
          final stagedOutput = rfiSystemSync.executeSyncStep(step, stagedInput);
          _writeHandleBytes(workbookAnchor, stagedOutput);

          final appliedHash = _backendValue(() => workbookAnchor.sha256());

          final restoreMeta = <String, Object?>{
            'originalSha256': backupHash,
            'appliedSha256': appliedHash,
          };
          final restoreStep = Step(
            kind: StepKind.restoreBackup,
            src: dstPath,
            dst: srcPath,
            noteKey: 'note.rfi.systemSync.restore',
            meta: restoreMeta,
          );
          record.steps.add(restoreStep);
          journals.write(record);

          result.applied += 1;
          progress(
            ProgressEvent(
              phase: 'apply',
              done: stepIndex + 1,
              total: totalSteps,
              subject: srcPath,
            ),
          );
        }

        record.state = JournalState.committed;
        journals.write(record);
        return OperationResult(
          applied: result.applied,
          failed: result.failed,
          journalId: record.journalId,
          errors: result.errors,
          meta: result.meta,
          cloudSynced: null,
        );
      } finally {
        registration.close();
      }
    } finally {
      claim.release();
    }
  }

  /// Converts a capability lookup failure into a validation error.
  AnchoredPath _backendAnchor(AnchoredPath Function() body) {
    try {
      return body();
    } on BackendError {
      rethrow;
    } on Object catch (error) {
      throw BackendError.validation(_errorText(error));
    }
  }

  /// Converts an I/O failure into a validation error.
  T _backendValue<T>(T Function() body) {
    try {
      return body();
    } on BackendError {
      rethrow;
    } on Object catch (error) {
      throw BackendError.validation(_errorText(error));
    }
  }

  OsHandle _openMutableOrValidation(AnchoredPath anchor) {
    try {
      return anchor.openMutable();
    } on Object {
      throw BackendError.validation('error.rfi.systemSync.workbookLocked');
    }
  }

  void _tryWrite(JournalRecord record) {
    try {
      journals.write(record);
    } on Object {
      // Mirrors `let _ = self.journals.write(&record)`.
    }
  }

  /// Reads every byte of the anchored file, rewinding before and after.
  Uint8List _readHandleBytes(AnchoredPath anchor) {
    final handle = anchor.openRead();
    try {
      return _readAll(handle);
    } finally {
      handle.close();
    }
  }

  /// Replaces the anchored file's bytes in place with [bytes].
  void _writeHandleBytes(AnchoredPath anchor, Uint8List bytes) {
    final handle = anchor.openMutable();
    try {
      handle.setLen(0);
      handle.seek(0);
      handle.writeAll(bytes);
      handle.syncAll();
    } finally {
      handle.close();
    }
  }

  // -------------------------------------------------------------------------
  // Cleanup of signed-page directories and undo / recovery.
  // -------------------------------------------------------------------------

  void _cleanupSignedPageDirectories(
    PlanBinding binding,
    CallerIdentity caller,
    JournalRecord record,
    _Result result,
  ) {
    if (record.opId != 'rfi.stage_signed_first_pages' &&
        record.opId != 'rfi.normalize_pdf_orientation') {
      return;
    }
    final directories = <(int, Step)>[];
    for (var index = 0; index < record.steps.length; index++) {
      final step = record.steps[index];
      if (step.kind == StepKind.mkdir) {
        directories.add((index, step));
      }
    }
    for (final (index, step) in directories.reversed) {
      final directory = step.dst;
      if (directory == null) continue;
      record.pendingStep = step;
      journals.write(record);
      var removed = false;
      try {
        final anchor = grants
            .authorizeCapabilityForPlan(
              binding.grantIds,
              caller,
              binding.operationId,
              directory,
              PathIntent.write,
            )
            .anchor();
        anchor.removeDir();
        removed = true;
      } on Object {
        removed = false;
      }
      if (removed) {
        record.steps.removeAt(index);
        result.applied = result.applied > 0 ? result.applied - 1 : 0;
      }
      record.pendingStep = null;
      journals.write(record);
    }
  }

  /// `undo`: undoes a journaled apply. RFI Follow Up journals take the S5 path.
  OperationResult undo(
    String journalId,
    CallerIdentity caller, {
    String? destination,
  }) {
    if (journals.inspectOperation(journalId) ==
        rfi_follow_up.followUpOperationId) {
      if (destination != null) {
        throw BackendError.validation(
          'RFI Follow Up undo does not accept a destination',
        );
      }
      return _undoFollowUp(journalId, caller).build();
    }
    final permit = mutationLock.tryAcquire();
    try {
      return undoJournal(journals, journalId, caller, destination);
    } finally {
      permit.release();
    }
  }

  /// `recover_pending`: replays every pending S5 record under the mutation locks.
  List<RecoveryEntry> recoverPending(CallerIdentity caller) {
    final process = mutationLock.tryAcquire();
    final InterprocessMutationPermit interprocess;
    try {
      interprocess = journals.tryAcquireMutationLock();
    } on Object {
      process.release();
      rethrow;
    }
    final permits = journals.bindS5Permits(process, interprocess);
    final session = journals.beginS5Recovery(
      permits,
      reconcile: (store, record) => reconcileRetainedPublication(
        journals: store,
        record: record,
      ),
    );
    try {
      final recovered = <RecoveryEntry>[];
      while (session.hasNext) {
        final accesses = session.nextRootAccesses;
        final GrantRootRoster? roots = accesses == null
            ? null
            : _GrantRosterAdapter(
                grants.reopenRecordedRoots(
                  accesses,
                  caller,
                  rfi_follow_up.followUpOperationId,
                ),
              );
        final entry = session.recoverNext(roots, caller);
        if (entry != null) {
          recovered.add(entry);
        }
      }
      recovered.sort((left, right) => left.journalId.compareTo(right.journalId));
      return recovered;
    } finally {
      session.release();
    }
  }

  /// `undo_follow_up`.
  _Result _undoFollowUp(String journalId, CallerIdentity caller) {
    final process = mutationLock.tryAcquire();
    final InterprocessMutationPermit interprocess;
    try {
      interprocess = journals.tryAcquireMutationLock();
    } on Object {
      process.release();
      rethrow;
    }
    final permits = journals.bindS5Permits(process, interprocess);
    final pending = journals.inspectS5ForUndo(journalId, caller, permits);
    final undoMode = pending.mode;
    final roots = _GrantRosterAdapter(
      grants.reopenRecordedRoots(
        pending.recordedRootAccesses,
        caller,
        rfi_follow_up.followUpOperationId,
      ),
    );
    final record = journals.admitS5Undo(pending, roots);
    try {
      if (undoMode == S5UndoMode.restoreFailedApply) {
        return _restoreFailedFollowUpApply(record);
      }
      final steps = record.completedSteps;
      if (steps.isEmpty) {
        throw BackendError.journalInvalid('S5 undo step is missing');
      }
      final step = steps.first;
      final backupPathRaw = step.src;
      if (backupPathRaw == null) {
        throw BackendError.journalInvalid('S5 backup path is missing');
      }
      final canonicalPathRaw = step.dst;
      if (canonicalPathRaw == null) {
        throw BackendError.journalInvalid('S5 canonical path is missing');
      }
      final backupEvidence = record.retainedEvidenceFor(
        RetainedArtifactRole.backup,
      );
      if (backupEvidence == null) {
        throw BackendError.journalInvalid('S5 backup evidence is missing');
      }
      if (backupEvidence.path != backupPathRaw) {
        throw BackendError.journalInvalid('S5 backup evidence path is invalid');
      }
      final expectedAppliedHash = step.meta['appliedSha256'];
      if (expectedAppliedHash is! String) {
        throw BackendError.journalInvalid('S5 applied hash is missing');
      }
      final createdIdentityJson = step.meta['createdCanonicalIdentity'];
      if (createdIdentityJson == null) {
        throw BackendError.journalInvalid('S5 applied identity is missing');
      }
      final FilesystemObjectIdentity expectedAppliedIdentity;
      try {
        expectedAppliedIdentity = FilesystemObjectIdentity.fromJson(
          createdIdentityJson,
        );
      } on Object {
        throw BackendError.journalInvalid('S5 applied identity is invalid');
      }
      final canonical = _backendAnchorOf(
        () => _authorizeS5Recorded(record, canonicalPathRaw).anchor(),
        'S5 canonical workbook cannot be anchored',
      );
      final backup = _backendAnchorOf(
        () => _authorizeS5Recorded(record, backupPathRaw).anchor(),
        'S5 backup cannot be anchored',
      );
      final currentFile = _openOrValidation(
        () => canonical.openMutable(),
        'error.rfi.followUp.workbookLocked',
      );
      final backupFile = _openOrValidation(
        () => backup.openRead(),
        'error.rfi.followUp.backupVerify',
      );
      try {
        final currentIdentity = _validationOf(
          () => fileIdentity(currentFile),
          'error.rfi.followUp.workbookChanged',
        );
        final backupIdentity = _validationOf(
          () => fileIdentity(backupFile),
          'error.rfi.followUp.backupVerify',
        );
        final currentHash = _validationOf(
          () => _sha256Open(currentFile),
          'error.rfi.followUp.workbookChanged',
        );
        final backupHash = _validationOf(
          () => _sha256Open(backupFile),
          'error.rfi.followUp.backupVerify',
        );
        final currentSnapshot = _validationOf(
          () => FileSecuritySnapshot.capture(currentFile),
          'error.rfi.followUp.workbookChanged',
        );
        final backupSnapshot = _validationOf(
          () => FileSecuritySnapshot.capture(backupFile),
          'error.rfi.followUp.backupVerify',
        );
        if (currentIdentity != expectedAppliedIdentity ||
            currentHash != expectedAppliedHash ||
            currentSnapshot.sha256() != backupEvidence.securitySha256 ||
            backupIdentity != backupEvidence.identity ||
            backupHash != backupEvidence.sha256 ||
            backupSnapshot.sha256() != backupEvidence.securitySha256) {
          throw BackendError.validation('error.rfi.followUp.workbookChanged');
        }
        final currentLen = _validationOf(
          () => currentFile.metadata().size,
          'error.rfi.followUp.workbookChanged',
        );
        final reservation = _validationOf(
          () => RetainedStorageReservation.forUndo(
            currentLen,
            currentSnapshot.auxiliaryChargeBytes(),
          ),
          'error.rfi.followUp.retainedStorageFull',
        );
        final canonicalParentPath = pathParent(canonicalPathRaw);
        if (canonicalParentPath == null) {
          throw BackendError.journalInvalid('S5 canonical parent is unavailable');
        }
        final usage = _s5RecordedRetainedUsage(record, canonicalParentPath);
        if (usage.$1 + reservation.fileCount > maxS5RetainedFiles ||
            usage.$2 + reservation.logicalChargeBytes > maxS5RetainedBytes) {
          throw BackendError.validation('error.rfi.followUp.retainedStorageFull');
        }
        final parentSnapshot = _validationOf(
          () => canonical.captureParentSecurity(),
          'error.rfi.followUp.workbookChanged',
        );
        final creation = _validationOf(
          () => backupSnapshot.creationDescriptor(parentSnapshot),
          'error.rfi.followUp.backupVerify',
        );
        final extension = pathExtension(canonicalPathRaw);
        if (extension == null) {
          throw BackendError.journalInvalid('S5 canonical extension is invalid');
        }
        final retainedPath = pathJoin(
          canonicalParentPath,
          '.docflow-s5-retained-applied-${record.journalId}-${_simpleUuid()}.$extension',
        );
        final retained = _backendAnchorOf(
          () => _authorizeS5Recorded(record, retainedPath).anchor(),
          'S5 retained applied leaf cannot be anchored',
        );
        final String retainedLocator;
        try {
          retainedLocator = _s5LocatorBackend(record, retainedPath);
        } on Object {
          throw BackendError.journalInvalid('S5 retained locator is invalid');
        }
        _transitionS5Backend(
          record,
          SetPendingRetainedArtifact(
            PendingRetainedArtifact(
              role: RetainedArtifactRole.displacedApplied,
              path: retainedPath,
              locator: retainedLocator,
              creationState: const PendingArtifactCreatedIntent(),
            ),
          ),
        );
        final chargeBytes = _validationOf(
          () => currentSnapshot.auxiliaryChargeBytes(),
          'error.rfi.followUp.retainedStorageFull',
        );
        final logicalBytes = _checkedAddOrThrowBackend(currentLen, chargeBytes);
        _transitionS5Backend(
          record,
          InitializePublication(
            RetainedPublication(
              purpose: RetainedPublicationPurpose.undo,
              canonical: canonicalPathRaw,
              retained: retainedPath,
              backup: backupPathRaw,
              publicationSource: backupPathRaw,
              expectedCanonicalIdentity: currentIdentity,
              expectedCanonicalSha256: currentHash,
              expectedBackupIdentity: backupIdentity,
              expectedBackupSha256: backupHash,
              publicationSourceIdentity: backupIdentity,
              publicationSha256: backupHash,
              expectedCanonicalSecuritySha256: currentSnapshot.sha256(),
              expectedBackupSecuritySha256: backupSnapshot.sha256(),
              publicationSecuritySha256: backupSnapshot.sha256(),
              expectedCanonicalDefaultStreamBytes: currentLen,
              expectedCanonicalAuxiliaryChargeBytes: chargeBytes,
              expectedCanonicalLogicalChargeBytes: logicalBytes,
              destinationAccessSha256: backupSnapshot.accessSha256(),
              destinationAuxiliarySha256: backupSnapshot.auxiliarySha256(),
              destinationAuxiliaryChargeBytes: backupSnapshot.auxiliaryChargeBytes(),
              retainedIdentity: null,
              retainedSha256: null,
              retainedSecuritySha256: null,
              createdCanonicalIdentity: null,
              phase: RetainedPublicationPhase.prepared,
            ),
          ),
        );
        final RetainedPublicationOutcome outcome;
        try {
          outcome = executeRetainedPublication(
            journals: journals,
            record: record,
            canonical: canonical,
            retained: retained,
            publicationSource: backupFile,
            destinationCreation: creation,
            destinationSnapshot: backupSnapshot,
            cancellation: const NotCancellablePublication(),
            beforeCanonicalCreate: () {},
            verifyDestination: (_) {},
          );
        } on Object {
          final locator = record.retainedLocators().first;
          final notice = RecoveryNotice(
            warningKey: 'error.rfi.followUp.recoveryRequired',
            locator: locator,
          );
          final expected = record.revision;
          record.prepareOutcomeUnknown(notice);
          journals.writeS5Cas(record, expected);
          return _s5RecoveryResult(record, notice);
        }
        switch (outcome) {
          case RetainedPublicationPublished():
            _transitionS5Backend(
              record,
              FinalizeUndoSuccess(completedStep: step, evidence: const []),
            );
            final result = _resultWith(
              applied: 1,
              failed: 0,
              journalId: record.journalId,
            );
            _insertEffectState(result.meta, 'committed');
            _insertRetainedEvidence(result.meta, record.retainedEvidence);
            return result;
          case RetainedPublicationCancelledBeforeDisplacement():
            throw BackendError.journalInvalid('S5 undo cannot be cancelled');
          case RetainedPublicationRecoveryRequired(:final notice):
            final expected = record.revision;
            record.prepareOutcomeUnknown(notice);
            journals.writeS5Cas(record, expected);
            return _s5RecoveryResult(record, notice);
        }
      } finally {
        _closeQuietly(currentFile);
        _closeQuietly(backupFile);
      }
    } finally {
      // Dart has no Drop: release the S5 lease on every exit (Rust RAII).
      record.release();
    }
  }

  /// `restore_failed_follow_up_apply`.
  _Result _restoreFailedFollowUpApply(ValidatedS5Record record) {
    if (record.undoMode != S5UndoMode.restoreFailedApply) {
      throw BackendError.journalInvalid(
        'S5 failed apply restore lease is missing',
      );
    }
    final publication = record.pendingPublication;
    if (publication == null ||
        publication.purpose != RetainedPublicationPurpose.apply) {
      throw BackendError.journalInvalid('S5 failed apply is not pending');
    }
    final pendingRetained = record.pendingRetainedArtifact;
    if (pendingRetained == null ||
        pendingRetained.role != RetainedArtifactRole.displacedOriginal ||
        pendingRetained.path != publication.retained) {
      throw BackendError.journalInvalid('S5 retained original is not pending');
    }
    final evidence = record.retainedEvidenceFor(
      RetainedArtifactRole.displacedOriginal,
    );
    if (evidence != null &&
        (evidence.path != pendingRetained.path ||
            evidence.identity != publication.expectedCanonicalIdentity ||
            evidence.sha256 != publication.expectedCanonicalSha256 ||
            evidence.securitySha256 !=
                publication.expectedCanonicalSecuritySha256)) {
      throw BackendError.journalInvalid(
        'S5 retained original evidence does not match publication authority',
      );
    }
    final canonicalPath = publication.canonical;
    final retainedPath = publication.retained;
    final canonical = _backendAnchorOf(
      () => _authorizeS5Recorded(record, canonicalPath).anchor(),
      'S5 Follow Up path cannot be anchored',
    );
    final retained = _backendAnchorOf(
      () => _authorizeS5Recorded(record, retainedPath).anchor(),
      'S5 retained original cannot be anchored',
    );

    _transitionS5Backend(record, const BeginFailedApplyRestore());

    final OsHandle current;
    try {
      current = canonical.openRead();
    } on IoError catch (error) {
      if (error.kind != FsErrorKind.notFound) {
        throw BackendError.validation('error.rfi.followUp.workbookChanged');
      }
      return _restoreFromRetained(record, publication, canonical, retained);
    } on Object {
      throw BackendError.validation('error.rfi.followUp.workbookChanged');
    }
    try {
      final exactCurrent = _tryOr(
            () => fileIdentity(current) == publication.expectedCanonicalIdentity,
          ) &&
          _tryOr(
            () => _sha256Open(current) == publication.expectedCanonicalSha256,
          ) &&
          _tryOr(
            () =>
                FileSecuritySnapshot.capture(current).sha256() ==
                publication.expectedCanonicalSecuritySha256,
          );
      final retainedAbsent = _isNotFound(() => retained.openRead());
      if (exactCurrent && retainedAbsent && _trySyncParent(canonical)) {
        return _finalizeFailedFollowUpRestore(record);
      }
      throw BackendError.validation('error.rfi.followUp.workbookChanged');
    } finally {
      current.close();
    }
  }

  /// The branch of `restore_failed_follow_up_apply` where the canonical name is
  /// absent and the retained original is put back.
  _Result _restoreFromRetained(
    ValidatedS5Record record,
    RetainedPublication publication,
    AnchoredPath canonical,
    AnchoredPath retained,
  ) {
    final OsHandle retainedFile;
    try {
      retainedFile = retained.openRead();
    } on Object {
      throw BackendError.validation('error.rfi.followUp.recoveryRequired');
    }
    final bool retainedExact;
    try {
      retainedExact =
          fileIdentity(retainedFile) == publication.expectedCanonicalIdentity &&
          _sha256Open(retainedFile) == publication.expectedCanonicalSha256 &&
          FileSecuritySnapshot.capture(retainedFile).sha256() ==
              publication.expectedCanonicalSecuritySha256;
    } on Object {
      retainedFile.close();
      rethrow;
    }
    retainedFile.close();
    if (!retainedExact) {
      throw BackendError.validation('error.rfi.followUp.workbookChanged');
    }

    try {
      retained.restoreOriginalNameNoClobber(canonical);
    } on Object {
      throw BackendError.validation('error.rfi.followUp.recoveryRequired');
    }
    final OsHandle restored;
    try {
      restored = canonical.openRead();
    } on Object {
      throw BackendError.validation('error.rfi.followUp.recoveryRequired');
    }
    final bool restoredExact;
    try {
      restoredExact =
          fileIdentity(restored) == publication.expectedCanonicalIdentity &&
          _sha256Open(restored) == publication.expectedCanonicalSha256 &&
          FileSecuritySnapshot.capture(restored).sha256() ==
              publication.expectedCanonicalSecuritySha256 &&
          _isNotFound(() => retained.openRead());
    } finally {
      restored.close();
    }
    if (!restoredExact) {
      throw BackendError.validation('error.rfi.followUp.recoveryRequired');
    }
    return _finalizeFailedFollowUpRestore(record);
  }

  /// `finalize_failed_follow_up_restore`.
  _Result _finalizeFailedFollowUpRestore(ValidatedS5Record record) {
    _transitionS5Backend(record, const FinalizeFailedApplyRestore());
    final result = _resultWith(
      applied: 1,
      failed: 0,
      journalId: record.journalId,
    );
    _insertEffectState(result.meta, 'committed');
    _insertRetainedEvidence(result.meta, record.retainedEvidence);
    return result;
  }

  // -------------------------------------------------------------------------
  // Step dispatch and per-kind step execution.
  // -------------------------------------------------------------------------

  /// `execute_step`: returns the number of applied units for the step.
  int _executeStep(
    Step step,
    PlanBinding binding,
    CallerIdentity caller,
    CancellationToken token,
    JournalRecord record,
  ) {
    switch (step.kind) {
      case StepKind.mkdir:
        _executeMkdir(step, binding, caller, token, record);
        return 1;
      case StepKind.copy:
      case StepKind.move:
      case StepKind.rename:
      case StepKind.write:
        _executeFileEffect(step, binding, caller, token, record);
        return 1;
      case StepKind.workbookUpdate:
        return _executeWorkbookUpdate(step, binding, caller, token, record);
      case StepKind.pdfFirstPageReplace:
        return _executePdfFirstPageReplace(step, binding, caller, token, record);
      case StepKind.pdfPageRotate:
        return _executePdfPageRotate(step, binding, caller, token, record);
      case StepKind.pdfPagesExtract:
        return _executePdfPagesExtract(step, binding, caller, token, record);
      case StepKind.replicaPublish:
        return _executeReplicaPublish(step, binding, caller, token, record);
      case StepKind.cleanupTemp:
        return _executeCleanupTemp(step, binding, caller, token, record);
      default:
        throw StepNoEffect(
          'unsupported Rust filesystem action: ${step.kind.name}',
        );
    }
  }

  /// `preflight_replica_steps`.
  void _preflightReplicaSteps(
    List<Step> steps,
    PlanBinding binding,
    CallerIdentity caller,
  ) {
    for (final step in steps) {
      if (step.kind != StepKind.replicaPublish) continue;
      final meta = replicaPublish.decodeStepMeta(step);
      final sourcePath = step.src;
      final targetPath = step.dst;
      if (sourcePath == null || targetPath == null) {
        throw BackendError.validation('error.replica.approvedStepInvalid');
      }
      final source = _backendAnchorOf(
        () => grants
            .authorizeCapabilityForPlan(
              binding.grantIds,
              caller,
              binding.operationId,
              sourcePath,
              PathIntent.read,
            )
            .anchor(),
        'error.replica.sourceChanged',
      );
      final target = _backendAnchorOf(
        () => grants
            .authorizeCapabilityForPlan(
              binding.grantIds,
              caller,
              binding.operationId,
              targetPath,
              PathIntent.write,
            )
            .anchor(),
        'error.replica.targetChanged',
      );
      if (_validationOf(() => source.sha256(), 'error.replica.sourceChanged') !=
          meta.sourceSha256) {
        throw BackendError.validation('error.replica.sourceChanged');
      }
      if (_validationOf(() => target.sha256(), 'error.replica.targetChanged') !=
          meta.expectedTargetSha256) {
        throw BackendError.validation('error.replica.targetChanged');
      }
    }
  }

  /// `execute_replica_publish`.
  int _executeReplicaPublish(
    Step step,
    PlanBinding binding,
    CallerIdentity caller,
    CancellationToken token,
    JournalRecord record,
  ) {
    final meta = _stepNoEffect(() => replicaPublish.decodeStepMeta(step));
    final sourcePath = _stepNoEffect(
      () => _requiredPath(step.src, 'replica source'),
    );
    final targetPath = _stepNoEffect(
      () => _requiredPath(step.dst, 'replica target'),
    );
    final backupPath = meta.backupPath;
    final source = _stepNoEffect(
      () => grants
          .authorizeCapabilityForPlan(
            binding.grantIds,
            caller,
            binding.operationId,
            sourcePath,
            PathIntent.read,
          )
          .anchor(),
    );
    final target = _stepNoEffect(
      () => grants
          .authorizeCapabilityForPlan(
            binding.grantIds,
            caller,
            binding.operationId,
            targetPath,
            PathIntent.write,
          )
          .anchor(),
    );
    final backup = _stepNoEffect(
      () => grants
          .authorizeCapabilityForPlan(
            binding.grantIds,
            caller,
            binding.operationId,
            backupPath,
            PathIntent.write,
          )
          .anchor(),
    );
    if (_stepNoEffect(() => source.sha256()) != meta.sourceSha256) {
      throw const StepNoEffect('error.replica.sourceChanged');
    }
    if (_stepNoEffect(() => target.sha256()) != meta.expectedTargetSha256) {
      throw const StepNoEffect('error.replica.targetChanged');
    }
    if (_backupExists(backup)) {
      throw const StepNoEffect('error.replica.backupExists');
    }
    _checkpointOrCancel(token);
    _stepNoEffect(() => backup.copyNoClobberFrom(target));
    if (_stepNoEffect(() => backup.sha256()) != meta.expectedTargetSha256) {
      throw const StepOutcomeUnknown('error.replica.backupVerificationFailed');
    }

    final temporary = _stepNoEffect(() => target.temporarySibling('replica'));
    _stepNoEffect(() => temporary.copyNoClobberFrom(source));
    if (_stepNoEffect(() => temporary.sha256()) != meta.sourceSha256) {
      _tryRemove(temporary);
      throw const StepNoEffect('error.replica.copyVerificationFailed');
    }
    if (_checkpointFails(token)) {
      _tryRemove(temporary);
      throw const StepCancelled();
    }
    final restoreMeta = <String, Object?>{
      'originalSha256': meta.expectedTargetSha256,
      'appliedSha256': meta.sourceSha256,
    };
    final restore = Step(
      kind: StepKind.restoreBackup,
      src: backupPath,
      dst: targetPath,
      noteKey: 'note.replica.restore',
      meta: restoreMeta,
    );
    record.pendingStep = restore;
    _stepUnknown(() => journals.write(record));
    try {
      _atomicReplaceCapabilityWithRetry(temporary, target);
    } on Object catch (error) {
      throw _replacementFailure(target, error);
    }
    final String applied;
    try {
      applied = target.sha256();
    } on Object {
      throw const StepOutcomeUnknown('error.replica.publishVerificationFailed');
    }
    if (applied != meta.sourceSha256) {
      var rollbackOk = true;
      try {
        _rollbackFromBackupCapability(
          backup,
          target,
          meta.expectedTargetSha256,
        );
      } on Object {
        rollbackOk = false;
      }
      if (rollbackOk) {
        record.pendingStep = null;
        _stepUnknown(() => journals.write(record));
        throw const StepNoEffect('error.replica.publishVerificationFailed');
      }
      throw const StepOutcomeUnknown('error.replica.publishVerificationFailed');
    }
    record.steps.add(restore);
    record.pendingStep = null;
    _stepUnknown(() => journals.write(record));
    return 1;
  }

  /// `execute_mkdir`.
  void _executeMkdir(
    Step step,
    PlanBinding binding,
    CallerIdentity caller,
    CancellationToken token,
    JournalRecord record,
  ) {
    final destination = _stepNoEffect(
      () => _requiredPath(step.dst, 'mkdir destination'),
    );
    _stepNoEffect(
      () => grants.authorizeForPlan(
        binding.grantIds,
        caller,
        binding.operationId,
        destination,
        PathIntent.write,
      ),
    );
    _createMissingDirectoriesForPath(destination, step, binding, caller, token, record);
  }

  /// `create_missing_directories`: creates each missing component under a
  /// pinned parent, journaling every creation before and after it runs.
  void _createMissingDirectories(
    AuthorizedPath authorized,
    Step template,
    CancellationToken token,
    JournalRecord record,
  ) {
    final MissingDirectoryCursor missing;
    try {
      missing = authorized.missingDirectories();
    } on Object catch (error) {
      throw StepNoEffect(_errorText(error));
    }
    while (missing.nextPath != null) {
      final directory = missing.nextPath!;
      _checkpointOrCancel(token);
      final actual = Step(
        kind: StepKind.mkdir,
        src: null,
        dst: directory,
        noteKey: template.noteKey,
        meta: Map<String, Object?>.of(template.meta),
      );
      record.pendingStep = actual;
      record.state = JournalState.applying;
      _stepUnknown(() => journals.write(record));
      _runBeforeCapabilityEffect();
      try {
        missing.createNext();
      } on CapabilityEffectError catch (error) {
        if (error.isOutcomeUnknown) {
          throw StepOutcomeUnknown('$directory: ${error.innerError}');
        }
        record.pendingStep = null;
        _stepUnknown(() => journals.write(record));
        throw StepNoEffect('could not create $directory: ${error.innerError}');
      }
      record.steps.add(actual);
      record.pendingStep = null;
      _stepUnknown(() => journals.write(record));
    }
  }

  /// `execute_file_effect`: copy, move, rename or write one file.
  void _executeFileEffect(
    Step step,
    PlanBinding binding,
    CallerIdentity caller,
    CancellationToken token,
    JournalRecord record,
  ) {
    if (step.meta.containsKey('contentBase64') &&
        (binding.operationId != _concreteDocumentOperation ||
            step.kind != StepKind.write ||
            step.noteKey != _concreteDocumentNote ||
            step.meta.containsKey('content') ||
            step.meta['contentBase64'] is! String)) {
      throw const StepNoEffect('encoded generated payload is not authorized');
    }
    final destination = _stepNoEffect(
      () => _requiredPath(step.dst, 'step destination'),
    );
    _stepNoEffect(
      () => grants.authorizeForPlan(
        binding.grantIds,
        caller,
        binding.operationId,
        destination,
        PathIntent.write,
      ),
    );
    String? source;
    if (step.kind != StepKind.write) {
      final sourceRaw = _stepNoEffect(() => _requiredPath(step.src, 'step source'));
      final intent = step.kind == StepKind.copy ? PathIntent.read : PathIntent.write;
      source = _stepNoEffect(
        () => grants.authorizeForPlan(
          binding.grantIds,
          caller,
          binding.operationId,
          sourceRaw,
          intent,
        ),
      );
    }
    if (source == destination) {
      throw const StepNoEffect('source and destination are the same path');
    }
    final parent = pathParent(destination);
    if (parent == null) {
      throw const StepNoEffect('destination has no parent directory');
    }
    _createMissingDirectoriesForPath(parent, step, binding, caller, token, record);
    _checkpointOrCancel(token);
    final actualMeta = Map<String, Object?>.of(step.meta)
      ..remove('content')
      ..remove('contentBase64');
    AnchoredPath? sourceAnchor;
    if (source != null) {
      sourceAnchor = _stepNoEffect(
        () => grants
            .authorizeCapabilityForPlan(
              binding.grantIds,
              caller,
              binding.operationId,
              source!,
              step.kind == StepKind.copy ? PathIntent.read : PathIntent.write,
            )
            .anchor(),
      );
    }
    final destinationAnchor = _stepNoEffect(
      () => grants
          .authorizeCapabilityForPlan(
            binding.grantIds,
            caller,
            binding.operationId,
            destination,
            PathIntent.write,
          )
          .anchor(),
    );
    if (sourceAnchor != null) {
      final expected = step.meta['expectedSourceSha256'];
      if (expected is String) {
        final hash = _stepNoEffect(() => sourceAnchor!.sha256());
        if (hash != expected) {
          throw const StepNoEffect(
            'copy source changed after preview; preview again',
          );
        }
        actualMeta['sourceSha256'] = hash;
      } else {
        try {
          actualMeta['sourceSha256'] = sourceAnchor.sha256();
        } on Object {
          // Matches `else if let Ok(hash)`: the hash is omitted on failure.
        }
      }
    }
    final actual = Step(
      kind: step.kind,
      src: source,
      dst: destination,
      noteKey: step.noteKey,
      meta: actualMeta,
    );
    record.pendingStep = actual;
    record.state = JournalState.applying;
    _stepUnknown(() => journals.write(record));
    _runBeforeCapabilityEffect();
    final void Function() effect;
    switch (step.kind) {
      case StepKind.copy:
        effect = () => destinationAnchor.copyNoClobberFrom(sourceAnchor!);
      case StepKind.move:
      case StepKind.rename:
        effect = () => sourceAnchor!.moveNoClobberTo(destinationAnchor);
      case StepKind.write:
        final encoded = step.meta['contentBase64'];
        if (encoded is String) {
          final Uint8List bytes;
          try {
            bytes = _decodeConcreteDocumentBase64(encoded);
          } on GeneratedWriteFailure catch (failure) {
            record.pendingStep = null;
            _stepUnknown(() => journals.write(record));
            throw StepNoEffect(failure.key);
          }
          effect = () => destinationAnchor.writeNoClobber(bytes);
        } else if (step.meta['content'] is String) {
          final bytes = utf8.encode(step.meta['content'] as String);
          effect = () => destinationAnchor.writeNoClobber(bytes);
        } else {
          record.pendingStep = null;
          _stepUnknown(() => journals.write(record));
          throw const StepNoEffect('write action has no string content');
        }
      default:
        throw StateError('file effect kind checked by caller');
    }
    try {
      effect();
    } on Object catch (error) {
      final alreadyExists =
          error is FsIoException && error.kind == FsErrorKind.alreadyExists;
      final outcomeUnknown = !alreadyExists && _openReadOk(destinationAnchor);
      if (outcomeUnknown) {
        throw StepOutcomeUnknown('$destination: $error');
      }
      record.pendingStep = null;
      _stepUnknown(() => journals.write(record));
      throw StepNoEffect(_errorText(error));
    }
    if (_consumeFailAfterEffect()) {
      throw const StepOutcomeUnknown('injected failure after filesystem effect');
    }
    final String hash;
    try {
      hash = destinationAnchor.sha256();
    } on Object catch (error) {
      throw StepOutcomeUnknown('$destination: $error');
    }
    actualMeta['undoSha256'] = hash;
    record.steps.add(actual);
    record.pendingStep = null;
    _stepUnknown(() => journals.write(record));
  }

  bool _consumeFailAfterEffect() {
    final fail = _failAfterEffect;
    _failAfterEffect = false;
    return fail;
  }

  /// `create_missing_directories` for a plain parent path (step parent).
  void _createMissingDirectoriesForPath(
    String parent,
    Step template,
    PlanBinding binding,
    CallerIdentity caller,
    CancellationToken token,
    JournalRecord record,
  ) {
    final authorized = _stepNoEffect(
      () => grants.authorizeDirectoryCapabilityForPlan(
        binding.grantIds,
        caller,
        binding.operationId,
        parent,
        PathIntent.write,
      ),
    );
    _createMissingDirectories(authorized, template, token, record);
  }

  /// `execute_workbook_update`: backup, staged patch, verify, atomic replace,
  /// and rollback when verification fails.
  int _executeWorkbookUpdate(
    Step step,
    PlanBinding binding,
    CallerIdentity caller,
    CancellationToken token,
    JournalRecord record,
  ) {
    final workbook = _stepNoEffect(
      () => grants.authorizeForPlan(
        binding.grantIds,
        caller,
        binding.operationId,
        _requiredPath(step.src, 'workbook source'),
        PathIntent.write,
      ),
    );
    final backup = _stepNoEffect(
      () => grants.authorizeForPlan(
        binding.grantIds,
        caller,
        binding.operationId,
        _requiredPath(step.dst, 'workbook backup'),
        PathIntent.write,
      ),
    );
    if (workbook == backup) {
      throw const StepNoEffect('workbook and backup are the same path');
    }
    final signature = step.meta['sourceSignature'];
    if (signature is! Map) {
      throw const StepNoEffect('workbook signature is missing');
    }
    final expectedSize = signature['size'];
    if (expectedSize is! int || expectedSize < 0) {
      throw const StepNoEffect('workbook size is missing');
    }
    final expectedHash = signature['sha256'];
    if (expectedHash is! String) {
      throw const StepNoEffect('workbook hash is missing');
    }
    final workbookAnchor = _stepNoEffect(
      () => grants
          .authorizeCapabilityForPlan(
            binding.grantIds,
            caller,
            binding.operationId,
            workbook,
            PathIntent.write,
          )
          .anchor(),
    );
    final actualSize = _stepNoEffect(() => workbookAnchor.len());
    final actualHash = _stepNoEffect(() => workbookAnchor.sha256());
    if (actualSize != expectedSize || actualHash != expectedHash) {
      throw const StepNoEffect('workbook changed after preview');
    }
    try {
      workbookAnchor.openMutable().close();
    } on Object {
      throw StepWarningNoEffect(
        Warning(
          key: 'error.rfi.workbookLocked',
          subject: pathFileName(workbook) ?? '',
        ),
      );
    }

    final backupParent = pathParent(backup);
    if (backupParent == null) {
      throw const StepNoEffect('workbook backup has no parent');
    }
    _createMissingDirectoriesForPath(backupParent, step, binding, caller, token, record);
    _checkpointOrCancel(token);

    final backupMetaSource = <String, Object?>{'sourceSha256': actualHash};
    final backupStep = Step(
      kind: StepKind.copy,
      src: workbook,
      dst: backup,
      noteKey: 'note.rfi.workbookUpdate',
      meta: backupMetaSource,
    );
    record.pendingStep = backupStep;
    record.state = JournalState.applying;
    _stepUnknown(() => journals.write(record));
    final backupAnchor = _stepNoEffect(
      () => grants
          .authorizeCapabilityForPlan(
            binding.grantIds,
            caller,
            binding.operationId,
            backup,
            PathIntent.write,
          )
          .anchor(),
    );
    try {
      backupAnchor.copyNoClobberFrom(workbookAnchor);
    } on Object catch (error) {
      final alreadyExists =
          error is FsIoException && error.kind == FsErrorKind.alreadyExists;
      if (!alreadyExists && _openReadOk(backupAnchor)) {
        throw StepOutcomeUnknown('$backup: $error');
      }
      record.pendingStep = null;
      _stepUnknown(() => journals.write(record));
      throw StepNoEffect('workbook backup $backup: $error');
    }
    final String backupHash;
    try {
      backupHash = backupAnchor.sha256();
    } on Object catch (error) {
      throw StepOutcomeUnknown('$backup: $error');
    }
    if (backupHash != actualHash) {
      throw const StepOutcomeUnknown('workbook backup verification failed');
    }
    final undoMeta = Map<String, Object?>.of(backupStep.meta)
      ..['undoSha256'] = backupHash;
    record.steps.add(
      Step(
        kind: backupStep.kind,
        src: backupStep.src,
        dst: backupStep.dst,
        noteKey: backupStep.noteKey,
        meta: undoMeta,
      ),
    );
    record.pendingStep = null;
    _stepUnknown(() => journals.write(record));

    _checkpointOrCancel(token);
    // The approved input is read out of the workbook capability, patched with
    // the approved cell updates, and verified before anything is replaced.
    final stagedInput = _readHandleBytes(workbookAnchor);
    final Uint8List patched;
    try {
      final updates = rfiLog.workbookStepUpdates(step);
      patched = patchWorkbookSheets(
        stagedInput,
        updates,
        allowConsecutiveDuplicatesColumn: 1,
      );
    } on Object catch (error) {
      throw StepNoEffect('workbook patch $workbook: ${_errorText(error)}');
    }
    try {
      rfiLog.verifyWorkbookStep(step, patched);
    } on Object catch (error) {
      throw StepNoEffect(
        'workbook verification $workbook: ${_errorText(error)}',
      );
    }
    final outputHash = sha256Hex(patched);

    _checkpointOrCancel(token);

    final restoreMeta = <String, Object?>{'originalSha256': actualHash};
    final restoreStep = Step(
      kind: StepKind.restoreBackup,
      src: backup,
      dst: workbook,
      noteKey: 'note.rfi.restoreBackup',
      meta: restoreMeta,
    );
    record.pendingStep = restoreStep;
    _stepUnknown(() => journals.write(record));
    final AnchoredPath output;
    try {
      output = workbookAnchor.temporarySibling('completed');
      output.writeNoClobber(patched);
      output.syncParent();
    } on Object catch (error) {
      record.pendingStep = null;
      _stepUnknown(() => journals.write(record));
      throw StepNoEffect('workbook sync $workbook: ${_errorText(error)}');
    }
    try {
      _atomicReplaceCapabilityWithRetry(output, workbookAnchor);
    } on Object catch (error) {
      throw _replacementFailure(workbookAnchor, error);
    }
    final String appliedHash;
    try {
      appliedHash = workbookAnchor.sha256();
    } on Object catch (error) {
      if (_tryRollback(backupAnchor, workbookAnchor, expectedHash)) {
        record.pendingStep = null;
        _stepUnknown(() => journals.write(record));
        throw StepNoEffect(_errorText(error));
      }
      throw StepOutcomeUnknown('$workbook: $error');
    }
    if (appliedHash != outputHash) {
      if (_tryRollback(backupAnchor, workbookAnchor, expectedHash)) {
        record.pendingStep = null;
        _stepUnknown(() => journals.write(record));
        throw const StepNoEffect('workbook replacement verification failed');
      }
      throw const StepOutcomeUnknown('workbook replacement verification failed');
    }
    final completedRestore = Step(
      kind: restoreStep.kind,
      src: restoreStep.src,
      dst: restoreStep.dst,
      noteKey: restoreStep.noteKey,
      meta: <String, Object?>{
        ...restoreStep.meta,
        'appliedSha256': appliedHash,
      },
    );
    record.steps.add(completedRestore);
    record.pendingStep = null;
    _stepUnknown(() => journals.write(record));
    return _stepNoEffect(() => rfiLog.workbookRecordCount(step));
  }

  int _executePdfFirstPageReplace(
    Step step,
    PlanBinding binding,
    CallerIdentity caller,
    CancellationToken token,
    JournalRecord record,
  ) {
    final archive = _stepNoEffect(
      () => grants.authorizeForPlan(
        binding.grantIds,
        caller,
        binding.operationId,
        _requiredPath(step.src, 'archive PDF'),
        PathIntent.read,
      ),
    );
    final signedRaw = step.meta['signedPdf'];
    if (signedRaw is! String || signedRaw.isEmpty) {
      throw const StepNoEffect('signed PDF path is missing');
    }
    final signed = _stepNoEffect(
      () => grants.authorizeForPlan(
        binding.grantIds,
        caller,
        binding.operationId,
        signedRaw,
        PathIntent.read,
      ),
    );
    final destination = _stepNoEffect(
      () => grants.authorizeForPlan(
        binding.grantIds,
        caller,
        binding.operationId,
        _requiredPath(step.dst, 'review PDF destination'),
        PathIntent.write,
      ),
    );
    final expectedArchiveHash = _nonEmptyString(
      step.meta['archiveSha256'],
      'archive PDF hash is missing',
    );
    final expectedSignedHash = _nonEmptyString(
      step.meta['signedSha256'],
      'signed PDF hash is missing',
    );
    var signedPageIndex = 0;
    if (step.meta.containsKey('signedPageIndex')) {
      final value = step.meta['signedPageIndex'];
      if (value is! int || value < 0) {
        throw const StepNoEffect('signed PDF page index is invalid');
      }
      signedPageIndex = value;
    }
    final archiveAnchor = _stepNoEffect(
      () => grants
          .authorizeCapabilityForPlan(
            binding.grantIds,
            caller,
            binding.operationId,
            archive,
            PathIntent.read,
          )
          .anchor(),
    );
    final signedAnchor = _stepNoEffect(
      () => grants
          .authorizeCapabilityForPlan(
            binding.grantIds,
            caller,
            binding.operationId,
            signed,
            PathIntent.read,
          )
          .anchor(),
    );
    final destinationAnchor = _stepNoEffect(
      () => grants
          .authorizeCapabilityForPlan(
            binding.grantIds,
            caller,
            binding.operationId,
            destination,
            PathIntent.write,
          )
          .anchor(),
    );
    if (_stepNoEffect(() => archiveAnchor.sha256()) != expectedArchiveHash) {
      throw const StepNoEffect('archive PDF changed after preview');
    }
    if (_stepNoEffect(() => signedAnchor.sha256()) != expectedSignedHash) {
      throw const StepNoEffect('signed PDF changed after preview');
    }
    _checkpointOrCancel(token);
    record.pendingStep = step;
    record.state = JournalState.applying;
    _stepUnknown(() => journals.write(record));

    final archiveBytes = _stepNoEffect(() => _readHandleBytes(archiveAnchor));
    final signedBytes = _stepNoEffect(() => _readHandleBytes(signedAnchor));
    final Uint8List completed;
    try {
      completed = pdf.replaceFirstPageIncrementalFromPage(
        archiveBytes,
        signedBytes,
        signedPageIndex,
      );
    } on Object catch (error) {
      record.pendingStep = null;
      _stepUnknown(() => journals.write(record));
      throw StepNoEffect(_errorText(error));
    }
    final outputHash = sha256Hex(completed);
    if (_checkpointFails(token)) {
      record.pendingStep = null;
      _stepUnknown(() => journals.write(record));
      throw const StepCancelled();
    }
    if (_stepNoEffect(() => archiveAnchor.sha256()) != expectedArchiveHash ||
        _stepNoEffect(() => signedAnchor.sha256()) != expectedSignedHash) {
      record.pendingStep = null;
      _stepUnknown(() => journals.write(record));
      throw const StepNoEffect('approved PDF input changed during apply');
    }
    _publishCompletedPdf(
      destinationAnchor,
      completed,
      destination,
      record,
      outputHash,
      'published review PDF failed hash validation',
    );
    record.steps.add(
      Step(
        kind: StepKind.copy,
        src: archive,
        dst: destination,
        noteKey: step.noteKey,
        meta: <String, Object?>{...step.meta, 'undoSha256': outputHash},
      ),
    );
    record.pendingStep = null;
    _stepUnknown(() => journals.write(record));
    return 1;
  }

  /// Copies [completed] to [destination] with no-clobber semantics and checks the
  /// published hash. Shared by the first-page replace, rotation and extraction.
  void _publishCompletedPdf(
    AnchoredPath destinationAnchor,
    Uint8List completed,
    String destination,
    JournalRecord record,
    String outputHash,
    String hashFailureKey,
  ) {
    try {
      destinationAnchor.writeNoClobber(completed);
    } on Object catch (error) {
      final alreadyExists =
          error is FsIoException && error.kind == FsErrorKind.alreadyExists;
      if (!alreadyExists && _openReadOk(destinationAnchor)) {
        throw StepOutcomeUnknown('$destination: $error');
      }
      record.pendingStep = null;
      _stepUnknown(() => journals.write(record));
      throw StepNoEffect(_errorText(error));
    }
    final String published;
    try {
      published = destinationAnchor.sha256();
    } on Object catch (error) {
      throw StepOutcomeUnknown('$destination: $error');
    }
    if (published != outputHash) {
      throw StepOutcomeUnknown(hashFailureKey);
    }
  }

  /// Parses `rotations` metadata into `(pageIndex, targetRotation)` pairs.
  List<(int, int)> _parseRotations(
    Object? raw, {
    required String missingMessage,
    required String pageMessage,
    required String rotationMessage,
  }) {
    if (raw is! List) {
      throw StepNoEffect(missingMessage);
    }
    final rotations = <(int, int)>[];
    for (final rotation in raw) {
      final map = rotation is Map ? rotation : const <Object?, Object?>{};
      final pageIndex = map['pageIndex'];
      if (pageIndex is! int || pageIndex < 0) {
        throw StepNoEffect(pageMessage);
      }
      final target = map['targetRotation'];
      if (target is! int || !const {0, 90, 180, 270}.contains(target)) {
        throw StepNoEffect(rotationMessage);
      }
      rotations.add((pageIndex, target));
    }
    return rotations;
  }

  int _executePdfPageRotate(
    Step step,
    PlanBinding binding,
    CallerIdentity caller,
    CancellationToken token,
    JournalRecord record,
  ) {
    if (binding.operationId != 'rfi.normalize_pdf_orientation') {
      throw const StepNoEffect(
        'PDF rotation is not authorized for this operation',
      );
    }
    final source = _stepNoEffect(
      () => grants.authorizeForPlan(
        binding.grantIds,
        caller,
        binding.operationId,
        _requiredPath(step.src, 'source PDF'),
        PathIntent.read,
      ),
    );
    final destination = _stepNoEffect(
      () => grants.authorizeForPlan(
        binding.grantIds,
        caller,
        binding.operationId,
        _requiredPath(step.dst, 'orientation review PDF'),
        PathIntent.write,
      ),
    );
    final expectedHash = _nonEmptyString(
      step.meta['sourceSha256'],
      'source PDF hash is missing',
    );
    final rotations = _parseRotations(
      step.meta['rotations'],
      missingMessage: 'approved PDF rotations are missing',
      pageMessage: 'approved page index is invalid',
      rotationMessage: 'approved page rotation is invalid',
    );
    if (rotations.isEmpty) {
      throw const StepNoEffect('approved PDF rotations are empty');
    }
    final sourceAnchor = _stepNoEffect(
      () => grants
          .authorizeCapabilityForPlan(
            binding.grantIds,
            caller,
            binding.operationId,
            source,
            PathIntent.read,
          )
          .anchor(),
    );
    if (_stepNoEffect(() => sourceAnchor.sha256()) != expectedHash) {
      throw const StepNoEffect('source PDF changed after preview');
    }
    final destinationAnchor = _stepNoEffect(
      () => grants
          .authorizeCapabilityForPlan(
            binding.grantIds,
            caller,
            binding.operationId,
            destination,
            PathIntent.write,
          )
          .anchor(),
    );
    _checkpointOrCancel(token);
    record.pendingStep = step;
    record.state = JournalState.applying;
    _stepUnknown(() => journals.write(record));

    final sourceBytes = _stepNoEffect(() => _readHandleBytes(sourceAnchor));
    final Uint8List completed;
    try {
      completed = pdf.rotatePagesIncremental(sourceBytes, rotations);
    } on Object catch (error) {
      record.pendingStep = null;
      _stepUnknown(() => journals.write(record));
      throw StepNoEffect(_errorText(error));
    }
    final outputHash = sha256Hex(completed);
    if (_checkpointFails(token)) {
      record.pendingStep = null;
      _stepUnknown(() => journals.write(record));
      throw const StepCancelled();
    }
    if (_stepNoEffect(() => sourceAnchor.sha256()) != expectedHash) {
      record.pendingStep = null;
      _stepUnknown(() => journals.write(record));
      throw const StepNoEffect('approved PDF changed during apply');
    }
    _publishCompletedPdf(
      destinationAnchor,
      completed,
      destination,
      record,
      outputHash,
      'published orientation PDF failed hash validation',
    );
    record.steps.add(
      Step(
        kind: StepKind.copy,
        src: source,
        dst: destination,
        noteKey: step.noteKey,
        meta: <String, Object?>{...step.meta, 'undoSha256': outputHash},
      ),
    );
    record.pendingStep = null;
    _stepUnknown(() => journals.write(record));
    return 1;
  }

  int _executePdfPagesExtract(
    Step step,
    PlanBinding binding,
    CallerIdentity caller,
    CancellationToken token,
    JournalRecord record,
  ) {
    if (binding.operationId != 'rfi.process_scans' &&
        binding.operationId != 'rfi.process_scans_fast_covers') {
      throw const StepNoEffect(
        'PDF page extraction is not authorized for this operation',
      );
    }
    final source = _stepNoEffect(
      () => grants.authorizeForPlan(
        binding.grantIds,
        caller,
        binding.operationId,
        _requiredPath(step.src, 'batch scan PDF'),
        PathIntent.read,
      ),
    );
    final destination = _stepNoEffect(
      () => grants.authorizeForPlan(
        binding.grantIds,
        caller,
        binding.operationId,
        _requiredPath(step.dst, 'extracted RFI PDF'),
        PathIntent.write,
      ),
    );
    final expectedHash = _nonEmptyString(
      step.meta['sourceSha256'],
      'batch scan hash is missing',
    );
    final rawIndexes = step.meta['pageIndexes'];
    if (rawIndexes is! List || rawIndexes.isEmpty) {
      throw const StepNoEffect('approved PDF pages are missing');
    }
    final pageIndexes = <int>[];
    for (final index in rawIndexes) {
      if (index is! int || index < 0) {
        throw const StepNoEffect('approved PDF page is invalid');
      }
      pageIndexes.add(index);
    }
    for (var index = 1; index < pageIndexes.length; index++) {
      if (pageIndexes[index - 1] >= pageIndexes[index]) {
        throw const StepNoEffect('approved PDF page order is invalid');
      }
    }
    final sourceAnchor = _stepNoEffect(
      () => grants
          .authorizeCapabilityForPlan(
            binding.grantIds,
            caller,
            binding.operationId,
            source,
            PathIntent.read,
          )
          .anchor(),
    );
    if (_stepNoEffect(() => sourceAnchor.sha256()) != expectedHash) {
      throw const StepNoEffect('batch scan changed after Preview');
    }
    _checkpointOrCancel(token);
    final sourceBytes = _stepNoEffect(() => _readHandleBytes(sourceAnchor));
    final Uint8List extracted;
    try {
      extracted = pdf.extractPages(sourceBytes, pageIndexes);
    } on Object catch (error) {
      throw StepNoEffect(_errorText(error));
    }
    final rawRotations = step.meta['rotations'];
    final rotations = rawRotations is List
        ? _parseRotations(
            rawRotations,
            missingMessage: '',
            pageMessage: 'approved output page is invalid',
            rotationMessage: 'approved output rotation is invalid',
          )
        : <(int, int)>[];
    final Uint8List completed;
    if (rotations.isEmpty) {
      completed = extracted;
    } else {
      try {
        completed = pdf.rotatePagesIncremental(extracted, rotations);
      } on Object catch (error) {
        throw StepNoEffect(_errorText(error));
      }
    }
    final outputHash = sha256Hex(completed);
    if (_stepNoEffect(() => sourceAnchor.sha256()) != expectedHash) {
      throw const StepNoEffect('batch scan changed during Apply');
    }
    _checkpointOrCancel(token);
    final destinationParent = pathParent(destination);
    if (destinationParent == null) {
      throw const StepNoEffect('extracted PDF destination has no parent');
    }
    _createMissingDirectoriesForPath(destinationParent, step, binding, caller, token, record);
    final destinationAnchor = _stepNoEffect(
      () => grants
          .authorizeCapabilityForPlan(
            binding.grantIds,
            caller,
            binding.operationId,
            destination,
            PathIntent.write,
          )
          .anchor(),
    );
    record.pendingStep = step;
    record.state = JournalState.applying;
    _stepUnknown(() => journals.write(record));
    _publishCompletedPdf(
      destinationAnchor,
      completed,
      destination,
      record,
      outputHash,
      'extracted PDF failed hash validation',
    );
    record.steps.add(
      Step(
        kind: StepKind.copy,
        src: source,
        dst: destination,
        noteKey: step.noteKey,
        meta: <String, Object?>{...step.meta, 'undoSha256': outputHash},
      ),
    );
    record.pendingStep = null;
    _stepUnknown(() => journals.write(record));
    return 1;
  }

  /// `execute_cleanup_temp`: removes a stale temporary PDF hard link.
  int _executeCleanupTemp(
    Step step,
    PlanBinding binding,
    CallerIdentity caller,
    CancellationToken token,
    JournalRecord record,
  ) {
    if (binding.operationId != 'rfi.stage_signed_first_pages') {
      throw const StepNoEffect(
        'temporary PDF cleanup is not authorized for this operation',
      );
    }
    final temporary = _stepNoEffect(
      () => grants.authorizeForPlan(
        binding.grantIds,
        caller,
        binding.operationId,
        _requiredPath(step.src, 'stale PDF temporary path'),
        PathIntent.write,
      ),
    );
    final destination = _stepNoEffect(
      () => grants.authorizeForPlan(
        binding.grantIds,
        caller,
        binding.operationId,
        _requiredPath(step.dst, 'completed review PDF'),
        PathIntent.write,
      ),
    );
    final reviewRootRaw = step.meta['reviewRoot'];
    if (reviewRootRaw is! String || reviewRootRaw.isEmpty) {
      throw const StepNoEffect('review root is missing');
    }
    final reviewRoot = _stepNoEffect(
      () => grants.authorizeForPlan(
        binding.grantIds,
        caller,
        binding.operationId,
        reviewRootRaw,
        PathIntent.write,
      ),
    );
    if (!pathStartsWith(temporary, reviewRoot) ||
        !pathStartsWith(destination, reviewRoot) ||
        pathParent(temporary) != pathParent(destination)) {
      throw const StepNoEffect(
        'temporary PDF cleanup escaped the approved review path',
      );
    }
    final destinationName = pathFileName(destination);
    if (destinationName == null) {
      throw const StepNoEffect('review PDF name is invalid');
    }
    final temporaryName = pathFileName(temporary);
    if (temporaryName == null) {
      throw const StepNoEffect('temporary PDF name is invalid');
    }
    if (!temporaryPdfNameMatches(temporaryName, destinationName)) {
      throw const StepNoEffect(
        'temporary PDF name is outside the cleanup contract',
      );
    }
    final temporaryAnchor = _stepNoEffect(
      () => grants
          .authorizeCapabilityForPlan(
            binding.grantIds,
            caller,
            binding.operationId,
            temporary,
            PathIntent.write,
          )
          .anchor(),
    );
    final destinationAnchor = _stepNoEffect(
      () => grants
          .authorizeCapabilityForPlan(
            binding.grantIds,
            caller,
            binding.operationId,
            destination,
            PathIntent.write,
          )
          .anchor(),
    );
    if (!_stepNoEffectValue(() => temporaryAnchor.sameFileAs(destinationAnchor))) {
      throw const StepNoEffect(
        'temporary PDF is not the completed review PDF hard link',
      );
    }
    final expectedHash = _nonEmptyString(
      step.meta['destinationSha256'],
      'review PDF hash is missing',
    );
    if (_stepNoEffect(() => destinationAnchor.sha256()) != expectedHash) {
      throw const StepNoEffect('completed review PDF changed after preview');
    }
    _checkpointOrCancel(token);
    record.pendingStep = step;
    record.state = JournalState.applying;
    _stepUnknown(() => journals.write(record));
    try {
      _removeAnchoredFileWithAccessRetry(temporaryAnchor, 40);
    } on Object catch (error) {
      if (_openReadOk(temporaryAnchor) &&
          _tryOr(() => destinationAnchor.sha256() == expectedHash)) {
        record.pendingStep = null;
        _stepUnknown(() => journals.write(record));
        throw StepNoEffect(_errorText(error));
      }
      throw StepOutcomeUnknown('$temporary: $error');
    }
    final String finalHash;
    try {
      finalHash = destinationAnchor.sha256();
    } on Object catch (error) {
      throw StepOutcomeUnknown('$destination: $error');
    }
    if (finalHash != expectedHash) {
      throw const StepOutcomeUnknown(
        'completed review PDF changed during temporary cleanup',
      );
    }
    record.pendingStep = null;
    _stepUnknown(() => journals.write(record));
    return 1;
  }

  // -------------------------------------------------------------------------
  // Small helpers used by the step bodies.
  // -------------------------------------------------------------------------

  String _nonEmptyString(Object? value, String message) {
    if (value is! String || value.isEmpty) {
      throw StepNoEffect(message);
    }
    return value;
  }

  void _checkpointOrCancel(CancellationToken token) {
    if (_checkpointFails(token)) {
      throw const StepCancelled();
    }
  }

  bool _backupExists(AnchoredPath backup) {
    try {
      backup.openRead().close();
      return true;
    } on IoError catch (error) {
      if (error.kind == FsErrorKind.notFound) return false;
      throw StepNoEffect(_errorText(error));
    } on Object catch (error) {
      throw StepNoEffect(_errorText(error));
    }
  }

  void _tryRemove(AnchoredPath path) {
    try {
      path.removeFile();
    } on Object {
      // `let _ = temporary.remove_file()`.
    }
  }

  bool _tryRollback(
    AnchoredPath backup,
    AnchoredPath workbook,
    String expectedHash,
  ) {
    try {
      _rollbackFromBackupCapability(backup, workbook, expectedHash);
      return true;
    } on Object {
      return false;
    }
  }

  /// Runs [body] and reports whether it returned true without throwing.
  bool _tryOr(bool Function() body) {
    try {
      return body();
    } on Object {
      return false;
    }
  }

  bool _isNotFound(OsHandle Function() open) {
    try {
      open().close();
      return false;
    } on IoError catch (error) {
      return error.kind == FsErrorKind.notFound;
    } on Object {
      return false;
    }
  }

  bool _trySyncParent(AnchoredPath anchor) {
    try {
      anchor.syncParent();
      return true;
    } on Object {
      return false;
    }
  }

  T _stepNoEffectValue<T>(T Function() body) => _stepNoEffect(body);

  /// Converts a body error into a backend validation error with [message].
  T _validationOf<T>(T Function() body, String message) {
    try {
      return body();
    } on BackendError {
      rethrow;
    } on Object {
      throw BackendError.validation(message);
    }
  }

  /// Opens a handle via [open], mapping failure to a validation error.
  OsHandle _openOrValidation(OsHandle Function() open, String message) {
    try {
      return open();
    } on Object {
      throw BackendError.validation(message);
    }
  }

  /// Resolves an anchor, mapping a failure to a path-invalid error.
  AnchoredPath _backendAnchorOf(AnchoredPath Function() body, String message) {
    try {
      return body();
    } on BackendError {
      rethrow;
    } on Object {
      throw BackendError.pathInvalid(message);
    }
  }

  /// Retained storage charge checks: `checked_add` that reports overflow.
  int _checkedAddOrThrow(int left, int right) {
    final total = _checkedAddOrNull(left, right);
    if (total == null) {
      throw const FollowUpPartial(
        Warning(key: 'error.rfi.followUp.retainedStorageFull', subject: ''),
      );
    }
    return total;
  }

  int _checkedAddOrThrowBackend(int left, int right) {
    final total = _checkedAddOrNull(left, right);
    if (total == null) {
      throw BackendError.validation('error.rfi.followUp.retainedStorageFull');
    }
    return total;
  }

  /// Opens the workbook or throws. See [_openOrValidation].
  void _closeQuietly(OsHandle? handle) {
    try {
      handle?.close();
    } on Object {
      // Closing an already-closed handle is a no-op.
    }
  }

  /// Insert `retainedEvidence` projection (`insert_retained_evidence`).
  void _insertRetainedEvidence(
    JsonObject meta,
    List<RetainedArtifactEvidence> evidence,
  ) {
    const order = [
      RetainedArtifactRole.backup,
      RetainedArtifactRole.privateOutput,
      RetainedArtifactRole.displacedOriginal,
      RetainedArtifactRole.displacedApplied,
    ];
    final projection = <Object?>[];
    for (final role in order) {
      for (final item in evidence) {
        if (item.role == role) {
          projection.add(<String, Object?>{
            'role': item.role.projectionKey,
            'locator': item.locator,
          });
          break;
        }
      }
    }
    meta['retainedEvidence'] = projection;
  }

  /// Insert `retainedLocators` (`insert_retained_locators`).
  void _insertRetainedLocators(JsonObject meta, List<String> locators) {
    meta['retainedLocators'] = List<String>.of(locators);
  }

  /// `s5_recovery_result`.
  _Result _s5RecoveryResult(
    ValidatedS5Record record,
    RecoveryNotice notice,
  ) {
    final result = _resultWith(
      applied: 0,
      failed: 1,
      journalId: record.journalId,
      errors: [Warning(key: notice.warningKey, subject: notice.locator)],
    );
    _insertEffectState(result.meta, 'outcome_unknown');
    _insertRetainedEvidence(result.meta, record.retainedEvidence);
    return result;
  }

  /// `authorize_s5_recorded`.
  AuthorizedPath _authorizeS5Recorded(ValidatedS5Record record, String path) {
    final roots = record.roots;
    if (roots == null) {
      throw BackendError.journalInvalid('S5 root authority is unavailable');
    }
    for (final root in roots.roots) {
      try {
        return (root as CapabilityRoot).authorize(path);
      } on Object {
        continue;
      }
    }
    throw BackendError.pathEscape();
  }

  /// `s5_locator` for undo paths, returning a backend error.
  String _s5LocatorBackend(ValidatedS5Record record, String path) {
    try {
      return _s5Locator(record, path);
    } on Object {
      throw BackendError.journalInvalid('S5 retained locator is invalid');
    }
  }
}

/// Finishes a cancellation request when dropped (`CancellationRegistration`).
class CancellationRegistration {
  CancellationRegistration._(this._registry, this._requestId);

  final CancellationRegistry _registry;
  final String _requestId;
  bool _closed = false;

  /// Equivalent of the Rust `Drop` impl: unregisters the request once.
  void close() {
    if (_closed) return;
    _closed = true;
    _registry.finish(_requestId);
  }
}

String _simpleUuid() => const Uuid().v4().replaceAll('-', '');

int? _checkedAddOrNull(int left, int right) {
  if (left < 0 || right < 0) return null;
  if (left > 0x7fffffffffffffff - right) return null;
  return left + right;
}

/// Adapts a validated grant-root roster to the journal's roster port.
class _GrantRosterAdapter implements GrantRootRoster {
  _GrantRosterAdapter(this._roster);

  final ValidatedGrantRootRoster _roster;

  @override
  List<GrantRootAccessRecord> get records => _roster.records;

  @override
  List<CapabilityRootPort> get roots => _roster.roots;

  @override
  bool matchesGrants(List<String> grantIds) =>
      sameStrings(_roster.grantIds, grantIds);
}
