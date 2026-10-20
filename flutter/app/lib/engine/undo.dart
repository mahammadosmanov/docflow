/// Port of src-tauri/src/undo.rs.
///
/// Reverses committed journal records through pinned capability roots. Every
/// filesystem effect goes through the [AnchoredPathPort] interface, so the
/// undo state machine here is independent of the concrete capability layer.
library;

import 'dart:io';

import 'package:path/path.dart' as p;

import 'cap_fs.dart';
import 'contracts.dart';
import 'errors.dart';
import 'journal.dart';
import 'paths.dart';

/// Opens a capability root for an absolute canonical directory path.
typedef CapabilityRootOpener = CapabilityRootPort Function(String path);

/// Test-only fault injection, mirroring the `#[cfg(test)]` thread-locals in
/// undo.rs. Production code never sets these.
class UndoTestHooks {
  UndoTestHooks._();

  /// Runs once, before a failed quarantine is moved back to its original path.
  static void Function()? beforeQuarantineRestore;

  /// Makes the next quarantine delete fail once.
  static bool failQuarantineDelete = false;
}

/// Internal failure of one undo step, split like the Rust `UndoStepFailure`.
/// [outcomeUnknown] means the filesystem may have changed and recovery
/// metadata must stay durable.
class UndoStepFailure implements Exception {
  const UndoStepFailure.noEffect(this.cause) : outcomeUnknown = false;
  const UndoStepFailure.outcomeUnknown(this.cause) : outcomeUnknown = true;

  final FsIoException cause;
  final bool outcomeUnknown;

  @override
  String toString() => cause.message;
}

OperationResult undoJournal(
  JournalStore journals,
  String journalId,
  CallerIdentity caller,
  String? destination, {
  CapabilityRootOpener openRoot = CapabilityRoot.open,
}) {
  final record = journals.read(journalId);
  final owner = record.ownerId;
  if (owner != null && owner != caller.ownerId) {
    throw BackendError.journalOwnerMismatch();
  }
  if (record.pendingStep != null ||
      record.pendingUndo != null ||
      record.undoRecovery != null) {
    throw BackendError.journalInvalid(
      'journal has an unresolved pending effect',
    );
  }
  final roots = _prepareRoots(journals, record, openRoot);
  if (destination != null) {
    _validatePath(destination, roots);
  }

  var applied = 0;
  var failed = 0;
  final errors = <Warning>[];
  final meta = JsonObject();
  for (var index = record.steps.length - 1; index >= 0; index--) {
    if (record.undoneSteps.contains(index)) {
      continue;
    }
    final step = record.steps[index];
    _validateStep(step, roots);
    final UndoRecovery? recovery;
    try {
      recovery = _planUndoRecovery(step, destination, roots, index);
    } on FsIoException catch (error) {
      throw BackendError.journalInvalid(error.message);
    }
    record.pendingUndo = index;
    record.undoRecovery = recovery;
    journals.write(record);
    _validateStep(step, roots);
    final restoreBackup = step.kind == StepKind.restoreBackup;
    final subject = step.dst ?? '';
    try {
      _undoStep(step, destination, roots, journals, record);
      record.undoneSteps.add(index);
      applied += 1;
      record.pendingUndo = null;
      record.undoRecovery = null;
      journals.write(record);
    } on UndoStepFailure catch (failure) {
      failed += 1;
      errors.add(
        Warning(key: 'error.undo_failed', subject: '$subject: ${failure.cause.message}'),
      );
      if (!failure.outcomeUnknown) {
        record.pendingUndo = null;
        record.undoRecovery = null;
        journals.write(record);
      } else {
        record.state = JournalState.recoveryRequired;
        meta['effectState'] = 'outcome_unknown';
        journals.write(record);
        return OperationResult(
          applied: applied,
          failed: failed,
          journalId: record.journalId,
          errors: errors,
          meta: meta,
        );
      }
    }
    if (restoreBackup && failed > 0) {
      break;
    }
  }
  record.state = failed == 0 ? JournalState.undone : JournalState.undoPartial;
  meta['effectState'] = failed == 0 ? 'committed' : 'partial_committed';
  journals.write(record);
  return OperationResult(
    applied: applied,
    failed: failed,
    journalId: record.journalId,
    errors: errors,
    meta: meta,
  );
}

/// `outlook_reply_draft::OPERATION_ID`.
const String outlookReplyDraftOperationId = 'rfi.outlook_reply_draft';

/// Seam for `outlook_reply_draft::journal_is_safely_undoable`, implemented by
/// the Outlook reply-draft engine (E7). Journals for that operation are
/// judged only by this port; when no port is supplied they fail closed.
abstract interface class OutlookReplyDraftUndoPort {
  bool journalIsSafelyUndoable(JournalRecord record);
}

/// Whether every remaining step of [record] can be reversed safely.
///
/// Fail-closed: any anomaly returns false.
bool journalIsSafelyUndoable(
  JournalRecord record, {
  CapabilityRootOpener openRoot = CapabilityRoot.open,
  OutlookReplyDraftUndoPort? outlookReplyDraft,
}) {
  if (record.opId == outlookReplyDraftOperationId) {
    return outlookReplyDraft?.journalIsSafelyUndoable(record) ?? false;
  }
  const blockedStates = <JournalState?>[
    JournalState.applying,
    JournalState.recoveryRequired,
    JournalState.noEffectRetryable,
    JournalState.cancelledClean,
    JournalState.undone,
  ];
  if (record.steps.isEmpty ||
      record.pendingStep != null ||
      record.pendingUndo != null ||
      record.undoRecovery != null ||
      blockedStates.contains(record.state)) {
    return false;
  }
  final List<String> rawRoots;
  if (record.grantRoots.isEmpty) {
    try {
      rawRoots = _deriveLegacyRoots(record);
    } on BackendError {
      return false;
    }
  } else {
    rawRoots = record.grantRoots;
  }
  final List<CapabilityRootPort> roots;
  try {
    roots = _openRoots(rawRoots, openRoot);
  } on BackendError {
    return false;
  }
  if (record.grantRootIdentities.isNotEmpty) {
    try {
      _validateRootIdentities(roots, record.grantRootIdentities);
    } on BackendError {
      return false;
    }
  }
  var remaining = 0;
  for (var index = 0; index < record.steps.length; index++) {
    if (record.undoneSteps.contains(index)) {
      continue;
    }
    remaining += 1;
    final step = record.steps[index];
    try {
      _validateStep(step, roots);
    } on BackendError {
      return false;
    }
    if (!_stepIsReadyForUndo(step, roots)) {
      return false;
    }
  }
  return remaining > 0;
}

bool _stepIsReadyForUndo(Step step, List<CapabilityRootPort> roots) {
  switch (step.kind) {
    case StepKind.mkdir:
      return true;
    case StepKind.copy:
    case StepKind.write:
      final current = _anchorOrNull(step.dst, roots);
      if (current == null) {
        return false;
      }
      return _tryBool(() => _verifyExpectedHash(current, step, roots));
    case StepKind.move:
    case StepKind.rename:
      final current = _anchorOrNull(step.dst, roots);
      final target = _anchorOrNull(step.src, roots);
      if (current == null || target == null) {
        return false;
      }
      return _tryBool(() => _verifyExpectedHash(current, step, roots)) &&
          !target.canOpenRead();
    case StepKind.restoreBackup:
      final backup = _anchorOrNull(step.src, roots);
      final workbook = _anchorOrNull(step.dst, roots);
      if (backup == null || workbook == null) {
        return false;
      }
      return _tryBool(
            () => _verifyHashFromMeta(
              backup,
              step,
              'originalSha256',
              'workbook backup',
            ),
          ) &&
          _tryBool(
            () => _verifyHashFromMeta(
              workbook,
              step,
              'appliedSha256',
              'applied workbook',
            ),
          );
    default:
      return false;
  }
}

UndoRecovery? _planUndoRecovery(
  Step step,
  String? destination,
  List<CapabilityRootPort> roots,
  int index,
) {
  if (!const {
    StepKind.move,
    StepKind.rename,
    StepKind.copy,
    StepKind.write,
  }.contains(step.kind)) {
    return null;
  }
  final currentPath = _requiredPath(step.dst, 'undo current path');
  final current = _anchorPathIo(currentPath, roots);
  final quarantine = current.temporarySibling('undo');
  String? target;
  if (destination != null) {
    final sourcePath = _requiredPath(step.src, 'step source');
    final name = p.basename(sourcePath);
    if (name.isEmpty || name == '.' || name == '..' || p.dirname(sourcePath) == sourcePath) {
      throw const FsIoException(FsErrorKind.invalidInput, 'source has no name');
    }
    target = p.join(destination, name);
  } else if (step.kind == StepKind.move || step.kind == StepKind.rename) {
    target = _requiredPath(step.src, 'move source');
  }
  return UndoRecovery(
    index: index,
    original: current.absolute,
    quarantine: quarantine.absolute,
    target: target,
    phase: UndoRecoveryPhase.prepared,
  );
}

void _undoStep(
  Step step,
  String? destination,
  List<CapabilityRootPort> roots,
  JournalStore journals,
  JournalRecord record,
) {
  if (step.kind == StepKind.mkdir) {
    if (destination != null) {
      return;
    }
    final directory = _anchorNoEffect(
      _requiredPathNoEffect(step.dst, 'mkdir destination'),
      roots,
    );
    try {
      directory.removeDirDurable();
    } on FsIoException catch (error) {
      if (error.kind == FsErrorKind.notFound ||
          error.kind == FsErrorKind.directoryNotEmpty ||
          error.kind == FsErrorKind.permissionDenied) {
        return;
      }
      throw UndoStepFailure.outcomeUnknown(error);
    }
    return;
  }

  if (step.kind == StepKind.restoreBackup) {
    if (destination != null) {
      return;
    }
    _restoreBackup(step, roots);
    return;
  }
  switch (step.kind) {
    case StepKind.move:
    case StepKind.rename:
    case StepKind.copy:
    case StepKind.write:
      _undoQuarantinedFile(step, roots, journals, record);
    default:
      throw UndoStepFailure.noEffect(
        const FsIoException(
          FsErrorKind.unsupported,
          'journal action does not have an inverse',
        ),
      );
  }
}

void _restoreBackup(Step step, List<CapabilityRootPort> roots) {
  final backup = _anchorNoEffect(
    _requiredPathNoEffect(step.src, 'workbook backup'),
    roots,
  );
  final workbook = _anchorNoEffect(
    _requiredPathNoEffect(step.dst, 'workbook destination'),
    roots,
  );
  _noEffect(() => _verifyHashFromMeta(backup, step, 'originalSha256', 'workbook backup'));
  _noEffect(() => _verifyHashFromMeta(workbook, step, 'appliedSha256', 'applied workbook'));
  if (Platform.isWindows) {
    final appliedHash = step.meta['appliedSha256'];
    if (appliedHash is! String) {
      throw const UndoStepFailure.noEffect(
        FsIoException(FsErrorKind.invalidData, 'applied workbook hash is missing'),
      );
    }
    try {
      workbook.overwriteVerifiedFrom(backup, appliedHash);
    } on FsIoException catch (error) {
      if (error.kind == FsErrorKind.invalidData) {
        throw UndoStepFailure.noEffect(error);
      }
      throw UndoStepFailure.outcomeUnknown(error);
    }
    return;
  }
  final temporary = _noEffect(() => workbook.temporarySibling('restore'));
  _noEffect(() => temporary.copyNoClobberFrom(backup));
  try {
    _verifyHashFromMeta(temporary, step, 'originalSha256', 'restore copy');
  } on FsIoException catch (error) {
    try {
      temporary.removeFile();
    } on FsIoException {
      // The verification failure is the error that matters.
    }
    throw UndoStepFailure.noEffect(error);
  }
  try {
    workbook.replaceWith(temporary);
  } on FsIoException catch (error) {
    throw UndoStepFailure.outcomeUnknown(error);
  }
}

void _verifyHashFromMeta(
  AnchoredPathPort path,
  Step step,
  String key,
  String label,
) {
  final expected = step.meta[key];
  if (expected is! String) {
    throw FsIoException(FsErrorKind.invalidData, '$label hash is missing');
  }
  final actual = path.sha256();
  if (actual != expected) {
    throw FsIoException(
      FsErrorKind.invalidData,
      '${path.absolute} changed since apply',
    );
  }
}

void _undoQuarantinedFile(
  Step step,
  List<CapabilityRootPort> roots,
  JournalStore journals,
  JournalRecord record,
) {
  final recovery = record.undoRecovery;
  if (recovery == null) {
    throw const UndoStepFailure.noEffect(
      FsIoException(
        FsErrorKind.invalidData,
        'undo quarantine recovery metadata is missing',
      ),
    );
  }
  final original = _anchorNoEffect(recovery.original, roots);
  final quarantine = _anchorNoEffect(recovery.quarantine, roots);
  if (Platform.isWindows) {
    final target = recovery.target;
    if (target == null) {
      final expected = _noEffect(() => _expectedUndoHash(step, roots));
      _persistRecoveryPhase(journals, record, UndoRecoveryPhase.deleting);
      try {
        original.removeVerified(expected);
      } on FsIoException catch (error) {
        if (error.kind == FsErrorKind.invalidData) {
          throw UndoStepFailure.noEffect(error);
        }
        throw UndoStepFailure.outcomeUnknown(error);
      }
      return;
    }
    _noEffect(() => _verifyExpectedHash(original, step, roots));
    final targetAnchor = _anchorNoEffect(target, roots);
    _persistRecoveryPhase(journals, record, UndoRecoveryPhase.publishing);
    try {
      original.moveNoClobberTo(targetAnchor);
    } on FsIoException catch (error) {
      throw UndoStepFailure.outcomeUnknown(error);
    }
    return;
  }

  try {
    original.moveNoClobberTo(quarantine);
  } on FsIoException catch (error) {
    if (error.kind == FsErrorKind.alreadyExists ||
        error.kind == FsErrorKind.notFound) {
      throw UndoStepFailure.noEffect(error);
    }
    throw UndoStepFailure.outcomeUnknown(error);
  }
  _persistRecoveryPhase(journals, record, UndoRecoveryPhase.quarantined);
  try {
    _verifyExpectedHash(quarantine, step, roots);
  } on FsIoException catch (error) {
    _restoreAfterFailure(error, quarantine, original, journals, record);
  }
  final targetPath = recovery.target;
  if (targetPath != null) {
    final AnchoredPathPort target;
    try {
      target = _anchorPathIo(targetPath, roots);
    } on FsIoException catch (error) {
      throw UndoStepFailure.outcomeUnknown(
        FsIoException(
          error.kind,
          'undo target is unavailable after quarantine: ${error.message}',
        ),
      );
    }
    _persistRecoveryPhase(journals, record, UndoRecoveryPhase.publishing);
    try {
      quarantine.moveNoClobberTo(target);
    } on FsIoException catch (error) {
      throw UndoStepFailure.outcomeUnknown(
        FsIoException(
          error.kind,
          'undo target publication could not be confirmed: ${error.message}',
        ),
      );
    }
  } else {
    _persistRecoveryPhase(journals, record, UndoRecoveryPhase.deleting);
    try {
      _injectedQuarantineDeleteFailure();
      quarantine.removeFileDurable();
    } on FsIoException catch (error) {
      throw UndoStepFailure.outcomeUnknown(error);
    }
  }
}

void _verifyExpectedHash(
  AnchoredPathPort path,
  Step step,
  List<CapabilityRootPort> roots,
) {
  final expected = _expectedUndoHash(step, roots);
  final actual = path.sha256();
  if (actual != expected) {
    throw FsIoException(
      FsErrorKind.invalidData,
      '${path.absolute} changed since apply',
    );
  }
}

String _expectedUndoHash(Step step, List<CapabilityRootPort> roots) {
  final recorded = step.meta['undoSha256'];
  if (recorded is String) {
    return recorded;
  }
  final source = step.src;
  if (source != null) {
    try {
      return _anchorPathIo(source, roots).sha256();
    } on FsIoException {
      // Fall through to the uniform error below.
    }
  }
  throw const FsIoException(
    FsErrorKind.invalidData,
    'legacy journal has no safe hash fallback',
  );
}

Never _restoreAfterFailure(
  FsIoException cause,
  AnchoredPathPort quarantine,
  AnchoredPathPort original,
  JournalStore journals,
  JournalRecord record,
) {
  _persistRecoveryPhase(journals, record, UndoRecoveryPhase.restoring);
  final hook = UndoTestHooks.beforeQuarantineRestore;
  UndoTestHooks.beforeQuarantineRestore = null;
  hook?.call();
  try {
    quarantine.moveNoClobberTo(original);
  } on FsIoException catch (restore) {
    throw UndoStepFailure.outcomeUnknown(
      FsIoException(
        restore.kind,
        'undo failed (${cause.message}) and quarantine restoration failed (${restore.message})',
      ),
    );
  }
  throw UndoStepFailure.noEffect(cause);
}

void _persistRecoveryPhase(
  JournalStore journals,
  JournalRecord record,
  UndoRecoveryPhase phase,
) {
  final recovery = record.undoRecovery;
  if (recovery == null) {
    throw const UndoStepFailure.outcomeUnknown(
      FsIoException(
        FsErrorKind.invalidData,
        'undo recovery metadata disappeared',
      ),
    );
  }
  record.undoRecovery = recovery.withPhase(phase);
  try {
    journals.write(record);
  } on BackendError catch (error) {
    throw UndoStepFailure.outcomeUnknown(
      FsIoException(
        FsErrorKind.other,
        'undo recovery phase could not be persisted: ${error.message}',
      ),
    );
  }
}

void _injectedQuarantineDeleteFailure() {
  if (UndoTestHooks.failQuarantineDelete) {
    UndoTestHooks.failQuarantineDelete = false;
    throw const FsIoException(
      FsErrorKind.other,
      'injected quarantine delete failure',
    );
  }
}

String _requiredPath(String? raw, String label) {
  if (raw == null || raw.isEmpty) {
    throw FsIoException(FsErrorKind.invalidInput, '$label is missing');
  }
  return raw;
}

String _requiredPathNoEffect(String? raw, String label) {
  try {
    return _requiredPath(raw, label);
  } on FsIoException catch (error) {
    throw UndoStepFailure.noEffect(error);
  }
}

List<CapabilityRootPort> _prepareRoots(
  JournalStore journals,
  JournalRecord record,
  CapabilityRootOpener openRoot,
) {
  if (record.grantRoots.isEmpty && record.grantRootIdentities.isNotEmpty) {
    throw BackendError.journalInvalid(
      'journal root identity metadata has no grant roots',
    );
  }
  if (record.grantRoots.isEmpty) {
    record.grantRoots = _deriveLegacyRoots(record);
  }
  final roots = _openRoots(record.grantRoots, openRoot);
  if (record.grantRootIdentities.isEmpty) {
    record.grantRootIdentities = _rootIdentityRecords(roots);
    journals.write(record);
    return roots;
  }
  _validateRootIdentities(roots, record.grantRootIdentities);
  return roots;
}

List<CapabilityRootPort> _openRoots(
  List<String> rawRoots,
  CapabilityRootOpener openRoot,
) {
  if (rawRoots.isEmpty) {
    throw BackendError.journalInvalid(
      'journal has no capability roots for safe undo',
    );
  }
  final roots = <CapabilityRootPort>[];
  for (final raw in rawRoots) {
    _rejectLexicalPath(raw);
    final String canonical;
    try {
      canonical = Directory(raw).resolveSymbolicLinksSync();
    } on FileSystemException {
      throw BackendError.journalInvalid('journal grant root is unavailable');
    }
    if (canonical != raw) {
      throw BackendError.journalInvalid(
        'journal grant root path is not canonical',
      );
    }
    if (roots.any((known) => known.canonical == raw)) {
      throw BackendError.journalInvalid('journal contains duplicate grant roots');
    }
    try {
      roots.add(openRoot(raw));
    } on Object {
      throw BackendError.journalInvalid('journal grant root is unavailable');
    }
  }
  return roots;
}

List<GrantRootIdentity> _rootIdentityRecords(List<CapabilityRootPort> roots) {
  return [
    for (final root in roots)
      GrantRootIdentity(
        root: root.canonical,
        identity: _identityOrInvalid(root),
      ),
  ];
}

FilesystemObjectIdentity _identityOrInvalid(CapabilityRootPort root) {
  try {
    return root.identity();
  } on Object {
    throw BackendError.journalInvalid(
      'journal grant root identity is unavailable',
    );
  }
}

void _validateRootIdentities(
  List<CapabilityRootPort> roots,
  List<GrantRootIdentity> identities,
) {
  if (roots.length != identities.length) {
    throw BackendError.journalInvalid(
      'journal grant root identity metadata is incomplete',
    );
  }
  for (final root in roots) {
    final matching = [
      for (final entry in identities)
        if (entry.root == root.canonical) entry,
    ];
    if (matching.isEmpty) {
      throw BackendError.journalInvalid(
        'journal grant root identity metadata is incomplete',
      );
    }
    if (matching.length > 1) {
      throw BackendError.journalInvalid(
        'journal contains duplicate grant root identities',
      );
    }
    final actual = _identityOrInvalid(root);
    if (actual != matching.first.identity) {
      throw BackendError.journalInvalid(
        'journal grant root object identity changed',
      );
    }
  }
}

List<String> _deriveLegacyRoots(JournalRecord record) {
  if (record.steps.isEmpty) {
    throw BackendError.journalInvalid('legacy journal has no reversible steps');
  }
  final roots = <String>[];
  for (final step in record.steps) {
    if (!const {
      StepKind.mkdir,
      StepKind.move,
      StepKind.rename,
      StepKind.copy,
      StepKind.write,
      StepKind.restoreBackup,
    }.contains(step.kind)) {
      throw BackendError.journalInvalid(
        'legacy journal contains an unmigrated executable action',
      );
    }
    for (final raw in [step.src, step.dst]) {
      if (raw == null) {
        continue;
      }
      _rejectLexicalPath(raw);
      final parent = p.dirname(raw);
      if (parent == raw) {
        throw BackendError.journalInvalid(
          'legacy journal path has no safe parent root',
        );
      }
      if (p.dirname(parent) == parent) {
        throw BackendError.journalInvalid(
          'legacy journal would require a filesystem root grant',
        );
      }
      if (!roots.contains(parent)) {
        roots.add(parent);
      }
    }
  }
  if (roots.isEmpty) {
    throw BackendError.journalInvalid('legacy journal has no safe path roots');
  }
  // Stable sort by component count, then drop roots nested under a kept root.
  final indexed = [
    for (var index = 0; index < roots.length; index++) (index, roots[index]),
  ]..sort((a, b) {
    final byDepth = p.split(a.$2).length.compareTo(p.split(b.$2).length);
    return byDepth != 0 ? byDepth : a.$1.compareTo(b.$1);
  });
  final nonRedundant = <String>[];
  for (final (_, root) in indexed) {
    if (!nonRedundant.any((known) => _componentsStartWith(root, known))) {
      nonRedundant.add(root);
    }
  }
  return nonRedundant;
}

void _validateStep(Step step, List<CapabilityRootPort> roots) {
  switch (step.kind) {
    case StepKind.mkdir:
      _validatePath(_requiredJournalPath(step.dst), roots);
    case StepKind.move:
    case StepKind.rename:
    case StepKind.copy:
    case StepKind.write:
    case StepKind.restoreBackup:
      final source = step.src;
      if (source != null) {
        _validatePath(source, roots);
      }
      _validatePath(_requiredJournalPath(step.dst), roots);
    default:
      throw BackendError.journalInvalid(
        'journal contains an unmigrated executable action',
      );
  }
}

String _requiredJournalPath(String? raw) {
  if (raw == null || raw.isEmpty) {
    throw BackendError.journalInvalid('journal step path is missing');
  }
  return raw;
}

void _validatePath(String path, List<CapabilityRootPort> roots) {
  _rejectLexicalPath(path);
  for (final root in roots) {
    if (path == root.canonical) {
      return;
    }
    try {
      root.authorize(path);
      return;
    } on Object {
      // Try the next root.
    }
  }
  throw BackendError.journalInvalid(
    'journal step escapes its original granted roots',
  );
}

AnchoredPathPort _anchorPathIo(String path, List<CapabilityRootPort> roots) {
  for (final root in roots) {
    try {
      return root.authorize(path).anchor();
    } on Object {
      // Try the next root.
    }
  }
  throw const FsIoException(
    FsErrorKind.permissionDenied,
    'journal path is unavailable through its pinned capability root',
  );
}

AnchoredPathPort _anchorNoEffect(String path, List<CapabilityRootPort> roots) =>
    _noEffect(() => _anchorPathIo(path, roots));

AnchoredPathPort? _anchorOrNull(String? path, List<CapabilityRootPort> roots) {
  if (path == null) {
    return null;
  }
  try {
    return _anchorPathIo(path, roots);
  } on FsIoException {
    return null;
  }
}

T _noEffect<T>(T Function() body) {
  try {
    return body();
  } on FsIoException catch (error) {
    throw UndoStepFailure.noEffect(error);
  }
}

bool _tryBool(void Function() body) {
  try {
    body();
    return true;
  } on FsIoException {
    return false;
  }
}

void _rejectLexicalPath(String path) {
  if (!p.isAbsolute(path) || p.split(path).contains('..')) {
    throw BackendError.journalInvalid('journal contains an invalid path');
  }
}

bool _componentsStartWith(String path, String base) {
  final parts = p.split(path);
  final prefix = p.split(base);
  if (prefix.length > parts.length) {
    return false;
  }
  for (var index = 0; index < prefix.length; index++) {
    if (parts[index] != prefix[index]) {
      return false;
    }
  }
  return true;
}
