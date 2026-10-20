// Port of src-tauri/src/outlook_reply_draft.rs: reply-all draft preview, apply,
// exact undo of the durable draft, and the owner-bound `.msg` archive export.
//
// The adapter contract (OutlookAdapter, OutlookDraft, MailItemIdentity,
// SelectedMailItem, ReplyContent, DraftIdentity, OutlookFailure) is reused from
// platform/windows/outlook.dart, which mirrors this Rust module's types. The
// Windows COM implementation there is UNVERIFIED on this Linux host; see that
// file's header. Everything in this file is platform-neutral and runs against
// any OutlookAdapter.

import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:path/path.dart' as p;

import 'contracts.dart';
import 'errors.dart';
import 'journal.dart';
import 'paths.dart';
import 'platform/windows/outlook.dart'
    show
        DraftIdentity,
        MailItemIdentity,
        OutlookAdapter,
        OutlookFailure,
        OutlookFailureKind,
        ReplyContent,
        SelectedMailItem;
import 'plan_store.dart';
import 'progress.dart';
import 'rfi_follow_up.dart' show isRustWhitespaceRune, rustTrim;

/// `OPERATION_ID`.
const String outlookReplyOperationId = 'rfi.outlook_reply_draft';

/// `ARCHIVE_OPERATION_ID`.
const String outlookArchiveOperationId = 'rfi.outlook_archive_selected';

/// `MAX_REPLY_TEXT_CHARS`.
const int maxReplyTextChars = 10000;

const int _maxReplyHtmlBytes = 1024 * 1024;
const int _maxGeneratedReplyBytes = 1024 * 1024;
const int _maxOutlookIdChars = 4096;
const int _maxSubjectChars = 512;

/// Result of a successful ReplyAll display, `ReplyApplyOutcome`.
final class ReplyApplyOutcome {
  const ReplyApplyOutcome({required this.durableIdentity});

  final DraftIdentity? durableIdentity;
}

/// `PreparedArchive`: the archive plan plus the authority that exports it.
final class PreparedArchive {
  const PreparedArchive({required this.plan, required this.authority});

  final Plan plan;
  final OutlookArchiveAuthority authority;
}

/// Owner-, session-, and mail-bound authority for one archive export.
final class OutlookArchiveAuthority {
  const OutlookArchiveAuthority({
    required this.ownerId,
    required this.sessionId,
    required this.source,
    required this.fingerprint,
  });

  final String ownerId;
  final String sessionId;
  final MailItemIdentity source;
  final String fingerprint;
}

final class _PlanAuthority {
  const _PlanAuthority({required this.source, required this.content});

  final MailItemIdentity source;
  final ReplyContent content;
}

bool _isControlRune(int rune) =>
    rune <= 0x1f || (rune >= 0x7f && rune <= 0x9f);

/// Port of `build_reply_content`.
ReplyContent buildReplyContent(String replyText) {
  if (rustTrim(replyText).isEmpty) {
    throw const OutlookFailure.noEffect('error.rfi.outlookDraft.replyRequired');
  }
  final runes = replyText.runes.toList();
  if (runes.length > maxReplyTextChars) {
    throw const OutlookFailure.noEffect('error.rfi.outlookDraft.replyTooLong');
  }
  for (final rune in runes) {
    if (_isControlRune(rune) &&
        rune != 0x0d &&
        rune != 0x0a &&
        rune != 0x09) {
      throw const OutlookFailure.noEffect('error.rfi.outlookDraft.replyInvalid');
    }
  }

  final normalized = replyText.replaceAll('\r\n', '\n').replaceAll('\r', '\n');
  final plainText = normalized.replaceAll('\n', '\r\n');
  final escaped = StringBuffer();
  for (final rune in plainText.runes) {
    switch (rune) {
      case 0x26:
        escaped.write('&amp;');
      case 0x3c:
        escaped.write('&lt;');
      case 0x3e:
        escaped.write('&gt;');
      case 0x22:
        escaped.write('&quot;');
      case 0x27:
        escaped.write('&#39;');
      case 0x0d:
        // Carriage returns are dropped from HTML; line breaks come from LF.
        break;
      case 0x0a:
        escaped.write('<br>');
      default:
        escaped.writeCharCode(rune);
    }
  }
  return ReplyContent(
    plainText: plainText,
    html: '<div>$escaped</div><br><br>',
  );
}

/// Port of `apply_reply_all`. The draft is disposed on every path, matching
/// the Rust `Drop` of the boxed draft.
ReplyApplyOutcome applyReplyAll(
  OutlookAdapter adapter,
  MailItemIdentity expected,
  ReplyContent content,
) {
  final draft = adapter.replyAll(expected);
  try {
    draft.prependBody(content);
    draft.display();
    return ReplyApplyOutcome(durableIdentity: draft.durableIdentity());
  } finally {
    draft.dispose();
  }
}

/// `OutlookDraftService`: the preview/apply/undo state machine for the
/// reply-all draft and the archive export.
final class OutlookDraftService {
  OutlookDraftService({
    required String journalDir,
    required Duration ttl,
    required GlobalMutationLock mutationLock,
    required OutlookAdapter adapter,
  }) : _plans = PlanStore(ttl),
       _journals = JournalStore(journalDir),
       _mutationLock = mutationLock,
       _adapter = adapter;

  final PlanStore _plans;
  final Map<String, _PlanAuthority> _authorities = {};
  final Map<String, OutlookArchiveAuthority> _archiveAuthorities = {};
  final JournalStore _journals;
  final GlobalMutationLock _mutationLock;
  final CancellationRegistry _cancellations = CancellationRegistry();
  final OutlookAdapter _adapter;

  JournalStore get journals => _journals;

  /// Port of `prepare_archive`.
  PreparedArchive prepareArchive(CallerIdentity caller, String destinationRoot) {
    final selected = _selected();
    _validateSelectedMailItem(selected);
    final destination = p.join(destinationRoot, archiveFilename(selected.subject));
    if (_pathExists(destination)) {
      throw BackendError.validation(
        'error.rfi.outlookArchive.destinationExists',
      );
    }
    final step = Step(
      kind: StepKind.write,
      src: selected.subject,
      dst: destination,
      noteKey: 'step.rfi.outlookArchive',
      meta: <String, Object?>{
        'generatedBy': 'outlook_msg',
        'format': 'msg',
        'mailboxMutationAllowed': false,
        'sendAllowed': false,
      },
    );
    return PreparedArchive(
      plan: Plan(
        planId: '',
        opId: outlookArchiveOperationId,
        steps: [step],
        warnings: [
          Warning(
            key: 'warn.rfi.outlookArchive.localCopy',
            subject: selected.subject,
          ),
        ],
        meta: <String, Object?>{},
      ),
      authority: OutlookArchiveAuthority(
        ownerId: caller.ownerId,
        sessionId: caller.sessionId,
        source: selected.identity,
        fingerprint: selected.fingerprint,
      ),
    );
  }

  /// Port of `bind_archive`.
  void bindArchive(String planId, PreparedArchive prepared) {
    _archiveAuthorities[planId] = prepared.authority;
  }

  /// Port of `export_archive`. The authority is consumed on first use.
  Uint8List exportArchive(String planId, CallerIdentity caller) {
    final authority = _archiveAuthorities.remove(planId);
    if (authority == null ||
        authority.ownerId != caller.ownerId ||
        authority.sessionId != caller.sessionId) {
      throw const OutlookFailure.noEffect(
        'error.rfi.outlookArchive.planUnavailable',
      );
    }
    return _adapter.exportSelectedMsg(authority.source, authority.fingerprint);
  }

  /// Port of `preview`.
  Plan preview(CallerIdentity caller, String replyText) {
    final ReplyContent content;
    try {
      content = buildReplyContent(replyText);
    } on OutlookFailure catch (failure) {
      throw _outlookValidation(failure);
    }
    return previewContent(caller, content);
  }

  /// Port of `preview_content`.
  Plan previewContent(CallerIdentity caller, ReplyContent content) {
    if (rustTrim(content.plainText).isEmpty ||
        _utf8Length(content.plainText) > _maxGeneratedReplyBytes ||
        content.html.isEmpty ||
        _utf8Length(content.html) > _maxReplyHtmlBytes) {
      throw BackendError.validation('error.rfi.outlookDraft.replyTooLong');
    }
    final selected = _selected();
    _validateSelectedMailItem(selected);
    final plan = Plan(
      planId: '',
      opId: outlookReplyOperationId,
      steps: [
        Step(
          kind: StepKind.outlookReplyDraft,
          src: selected.subject,
          dst: null,
          noteKey: 'step.rfi.outlookReplyDraft',
          meta: <String, Object?>{'displayOnly': true, 'sendAllowed': false},
        ),
      ],
      warnings: [
        Warning(key: 'warn.rfi.outlookDraft.unsent', subject: selected.subject),
        const Warning(key: 'warn.rfi.outlookDraft.undoConditional', subject: ''),
      ],
      meta: <String, Object?>{
        'replyPreview': <String, Object?>{
          'subject': selected.subject,
          'plainText': content.plainText,
          'html': content.html,
        },
      },
    );
    final cached = _plans.insertBound(
      BackendOrigin.rust,
      PlanBinding(
        ownerId: caller.ownerId,
        sessionId: caller.sessionId,
        operationId: outlookReplyOperationId,
        grantIds: const [],
      ),
      plan,
    );
    _authorities[cached.planId] = _PlanAuthority(
      source: selected.identity,
      content: content,
    );
    return cached;
  }

  bool containsPlan(String planId) => _plans.snapshot(planId) != null;

  /// Port of `apply`. The cached plan is executed as approved; nothing is
  /// recalculated. Journal writes bracket the ReplyAll side effect.
  OperationResult apply(
    String planId, {
    String? operationId,
    required CallerIdentity caller,
    void Function(ProgressEvent event)? progress,
  }) {
    final approved = _plans.inspectAvailableBoundOperation(planId, caller);
    if (approved != outlookReplyOperationId ||
        (operationId != null && operationId != approved)) {
      throw BackendError.validation(
        'apply operation does not match the approved plan',
      );
    }
    final claim = _plans.claimForApplyBound(planId, caller, _mutationLock);
    try {
      final authority = _authorities.remove(planId);
      if (authority == null) {
        throw BackendError.planUnavailable();
      }
      final token = _cancellations.register(planId);
      try {
        return _applyClaimed(
          planId,
          claim,
          authority,
          token,
          progress ?? (_) {},
        );
      } finally {
        _cancellations.finish(planId);
      }
    } finally {
      claim.release();
    }
  }

  OperationResult _applyClaimed(
    String planId,
    ApplyClaim claim,
    _PlanAuthority authority,
    CancellationToken token,
    void Function(ProgressEvent event) progress,
  ) {
    token.checkpoint();
    final claimed = claim.claimed;
    final binding = claimed.binding;
    if (binding == null) {
      throw BackendError.planUnavailable();
    }
    if (claimed.plan.steps.length != 1) {
      throw BackendError.planUnavailable();
    }
    final step = claimed.plan.steps.first;
    if (step.kind != StepKind.outlookReplyDraft) {
      throw BackendError.planUnavailable();
    }
    final record = JournalRecord.fresh(
      opId: outlookReplyOperationId,
      ownerId: binding.ownerId,
      sessionId: binding.sessionId,
      planDigest: claimed.digest,
    )..state = JournalState.applying;
    record.pendingStep = step;
    _journals.write(record);
    progress(
      ProgressEvent(
        phase: 'apply',
        done: 0,
        total: 1,
        subject: step.src ?? '',
      ),
    );

    OperationResult result;
    try {
      final outcome = applyReplyAll(_adapter, authority.source, authority.content);
      final durable = outcome.durableIdentity;
      final meta = Map<String, Object?>.of(step.meta);
      meta['draftDurable'] = durable != null;
      if (durable != null) {
        meta['draftEntryId'] = durable.entryId;
        meta['draftStoreId'] = durable.storeId;
        meta['draftFingerprint'] = durable.fingerprint;
      }
      final committedStep = Step(
        kind: step.kind,
        src: step.src,
        dst: step.dst,
        noteKey: step.noteKey,
        meta: meta,
      );
      record.pendingStep = null;
      record.steps.add(committedStep);
      record.state = JournalState.committed;
      _journals.write(record);
      final errors = <Warning>[];
      final resultMeta = _resultMeta(_effectCommitted);
      resultMeta['draftDisplayed'] = true;
      resultMeta['undoAvailable'] = durable != null;
      if (durable == null) {
        resultMeta['nonUndoable'] = true;
        errors.add(
          const Warning(key: 'warn.rfi.outlookDraft.undoUnavailable', subject: ''),
        );
      }
      result = _operationResult(
        record.journalId,
        applied: 1,
        failed: 0,
        errors: errors,
        meta: resultMeta,
      );
    } on OutlookFailure catch (failure) {
      final outcomeUnknown = failure.kind == OutlookFailureKind.outcomeUnknown;
      if (!outcomeUnknown) {
        record.pendingStep = null;
      }
      record.state = outcomeUnknown
          ? JournalState.recoveryRequired
          : JournalState.noEffectRetryable;
      _journals.write(record);
      result = _operationResult(
        record.journalId,
        applied: 0,
        failed: 1,
        errors: [Warning(key: failure.key, subject: '')],
        meta: _resultMeta(
          outcomeUnknown ? _effectOutcomeUnknown : _effectNoEffectRetryable,
        ),
      );
    }
    progress(const ProgressEvent(phase: 'apply', done: 1, total: 1, subject: ''));
    return result;
  }

  /// Port of `undo`: deletes only the exact durable draft.
  OperationResult undo(String journalId, CallerIdentity caller) {
    final record = _journals.read(journalId);
    _validateUndoRecord(record, caller);
    final draft = _durableIdentityFromStep(record.steps[0]);
    final permit = _mutationLock.tryAcquire();
    try {
      record.pendingUndo = 0;
      _journals.write(record);
      try {
        _adapter.deleteUnchangedDraft(draft);
        record.pendingUndo = null;
        record.undoneSteps.add(0);
        record.state = JournalState.undone;
        _journals.write(record);
        return _operationResult(
          journalId,
          applied: 1,
          failed: 0,
          errors: const [],
          meta: _resultMeta(_effectCommitted),
        );
      } on OutlookFailure catch (failure) {
        final outcomeUnknown = failure.kind == OutlookFailureKind.outcomeUnknown;
        if (!outcomeUnknown) {
          record.pendingUndo = null;
        }
        record.state = outcomeUnknown
            ? JournalState.recoveryRequired
            : JournalState.committed;
        _journals.write(record);
        return _operationResult(
          journalId,
          applied: 0,
          failed: 1,
          errors: [Warning(key: failure.key, subject: '')],
          meta: _resultMeta(
            outcomeUnknown ? _effectOutcomeUnknown : _effectNoEffectRetryable,
          ),
        );
      }
    } finally {
      permit.release();
    }
  }

  bool cancel(String planId) => _cancellations.cancel(planId);

  /// Port of `revoke`: drops everything bound to [caller].
  void revoke(CallerIdentity caller) {
    _plans.revoke(caller);
    _authorities.removeWhere((planId, _) => _plans.snapshot(planId) == null);
    _archiveAuthorities.removeWhere(
      (_, authority) =>
          authority.ownerId != caller.ownerId ||
          authority.sessionId != caller.sessionId,
    );
    _cancellations.revokeAll();
  }

  SelectedMailItem _selected() {
    try {
      return _adapter.selectedMailItem();
    } on OutlookFailure catch (failure) {
      throw _outlookValidation(failure);
    }
  }
}

const String _effectCommitted = 'committed';
const String _effectNoEffectRetryable = 'no_effect_retryable';
const String _effectOutcomeUnknown = 'outcome_unknown';

Map<String, Object?> _resultMeta(String effectState) => <String, Object?>{
  'effectState': effectState,
};

OperationResult _operationResult(
  String journalId, {
  required int applied,
  required int failed,
  required List<Warning> errors,
  required Map<String, Object?> meta,
}) => OperationResult(
  applied: applied,
  failed: failed,
  journalId: journalId,
  errors: errors,
  meta: meta,
);

/// Port of `archive_filename`: a Windows-safe `.msg` name derived from the
/// subject.
String archiveFilename(String subject) {
  final stem = StringBuffer();
  var stemChars = 0;
  var previousSpace = false;
  for (final rune in rustTrim(subject).runes) {
    var character = rune;
    if (_isControlRune(character) || _forbiddenFilenameRunes.contains(character)) {
      character = 0x20;
    }
    if (isRustWhitespaceRune(character)) {
      if (!previousSpace && stemChars > 0) {
        stem.write(' ');
        stemChars++;
      }
      previousSpace = true;
    } else {
      if (stemChars >= 100) {
        break;
      }
      stem.writeCharCode(character);
      stemChars++;
      previousSpace = false;
    }
  }
  var trimmed = stem.toString();
  while (trimmed.isNotEmpty &&
      (trimmed.endsWith(' ') || trimmed.endsWith('.'))) {
    trimmed = trimmed.substring(0, trimmed.length - 1);
  }
  var name = trimmed.isEmpty ? 'Outlook message' : trimmed;
  final upper = _asciiUpper(name);
  const reserved = {'CON', 'PRN', 'AUX', 'NUL'};
  final isDeviceDigit =
      upper.length == 4 &&
      (upper.startsWith('COM') || upper.startsWith('LPT')) &&
      upper.codeUnitAt(3) >= 0x31 &&
      upper.codeUnitAt(3) <= 0x39;
  if (reserved.contains(upper) || isDeviceDigit) {
    name = '_$name';
  }
  return '$name.msg';
}

const Set<int> _forbiddenFilenameRunes = {
  0x3c, // <
  0x3e, // >
  0x3a, // :
  0x22, // "
  0x2f, // /
  0x5c, // backslash
  0x7c, // |
  0x3f, // ?
  0x2a, // *
};

String _asciiUpper(String value) => String.fromCharCodes(
  value.codeUnits.map((unit) => unit >= 0x61 && unit <= 0x7a ? unit - 32 : unit),
);

/// Port of `journal_is_safely_undoable`.
bool journalIsSafelyUndoable(JournalRecord record) {
  final caller = CallerIdentity(record.ownerId ?? '', record.sessionId ?? '');
  try {
    _validateUndoRecord(record, caller);
    _durableIdentityFromStep(record.steps[0]);
    return true;
  } on BackendError {
    return false;
  }
}

void _validateSelectedMailItem(SelectedMailItem selected) {
  final identity = selected.identity;
  final fingerprintOk =
      selected.fingerprint.length == 64 &&
      RegExp(r'^[0-9a-fA-F]{64}$').hasMatch(selected.fingerprint);
  if (!_boundedIdentifier(identity.entryId) ||
      !_boundedIdentifier(identity.storeId) ||
      !fingerprintOk) {
    throw BackendError.validation('error.rfi.outlookDraft.selectionInvalid');
  }
  if (selected.subject.runes.length > _maxSubjectChars ||
      selected.subject.runes.any(_isControlRune)) {
    throw BackendError.validation('error.rfi.outlookDraft.subjectTooLong');
  }
}

void _validateUndoRecord(JournalRecord record, CallerIdentity caller) {
  if (record.opId != outlookReplyOperationId ||
      record.state != JournalState.committed ||
      record.pendingStep != null ||
      record.pendingUndo != null ||
      record.undoRecovery != null ||
      record.steps.length != 1 ||
      record.steps[0].kind != StepKind.outlookReplyDraft ||
      record.undoneSteps.isNotEmpty) {
    throw BackendError.journalInvalid(
      'Outlook draft journal is not eligible for exact undo',
    );
  }
  if (record.ownerId != caller.ownerId) {
    throw BackendError.journalOwnerMismatch();
  }
}

DraftIdentity _durableIdentityFromStep(Step step) {
  if (step.meta['draftDurable'] != true) {
    throw BackendError.validation('error.rfi.outlookDraft.undoUnavailable');
  }
  String field(String name) {
    final value = step.meta[name];
    if (value is String && _boundedIdentifier(value)) {
      return value;
    }
    throw BackendError.journalInvalid('Outlook draft identity is invalid');
  }

  return DraftIdentity(
    entryId: field('draftEntryId'),
    storeId: field('draftStoreId'),
    fingerprint: field('draftFingerprint'),
  );
}

bool _boundedIdentifier(String value) =>
    value.isNotEmpty &&
    value.runes.length <= _maxOutlookIdChars &&
    !value.runes.any(_isControlRune);

BackendError _outlookValidation(OutlookFailure failure) =>
    BackendError.validation(failure.key);

int _utf8Length(String value) => utf8.encode(value).length;

bool _pathExists(String path) {
  return FileSystemEntity.typeSync(path) != FileSystemEntityType.notFound;
}
