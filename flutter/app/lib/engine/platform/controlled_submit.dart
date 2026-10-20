/// Port of src-tauri/src/platform/controlled_submit.rs: the ordered,
/// signal-checked StartDoc / RenderPages / EndDoc / observe sequence for
/// controlled (spool-owned) print submission.
///
/// Safety rules preserved from Rust:
///  * Cancellation or deadline expiry before StartDoc performs zero effects.
///  * Once StartDoc has succeeded, any later failure is uncertain: the
///    document may already have reached the spooler, so it is never retried.
///  * Abort is attempted at most once and its outcome never replaces the
///    classified failure.
library;

import '../print_queue.dart';

/// Port of `SubmitSnapshot`.
class SubmitSnapshot {
  const SubmitSnapshot({required this.cancelled, required this.deadlineExpired});

  final bool cancelled;
  final bool deadlineExpired;

  @override
  bool operator ==(Object other) =>
      other is SubmitSnapshot &&
      other.cancelled == cancelled &&
      other.deadlineExpired == deadlineExpired;

  @override
  int get hashCode => Object.hash(cancelled, deadlineExpired);

  @override
  String toString() =>
      'SubmitSnapshot(cancelled: $cancelled, deadlineExpired: $deadlineExpired)';
}

/// Port of the `SubmitSignals` trait.
abstract interface class SubmitSignals {
  SubmitSnapshot sample();
}

/// Port of the `ControlledSubmitEffects` trait. Effects report failure by
/// throwing [PrintFailure]; [abortDoc] reports success as a bool.
abstract interface class ControlledSubmitEffects<JobId extends Object> {
  JobId startDoc();
  bool abortDoc();
  void renderPages();
  void endDoc();
  void observeJob(JobId jobId);
}

/// Port of `post_effect_failure`: any failure after StartDoc becomes an
/// uncertain outcome. A cancellation keeps its cancelled kind.
PrintFailure _postEffectFailure(PrintFailure failure) {
  if (failure.kind == PrintFailureKind.cancelled) {
    return PrintFailure.cancelled(
      'error.print.excelPrintCancelledUnconfirmed',
      uncertain: true,
    );
  }
  return PrintFailure.uncertain('error.print.excelPrintCompletionUnconfirmed');
}

/// Port of `execute_controlled_submit`. Throws [PrintFailure] on any
/// non-success outcome; returns normally when the whole sequence succeeded.
void executeControlledSubmit<JobId extends Object>(
  SubmitSignals signals,
  ControlledSubmitEffects<JobId> effects,
) {
  final beforeStart = signals.sample();
  if (beforeStart.cancelled) {
    throw PrintFailure.cancelled(
      'error.print.excelExportCancelled',
      uncertain: false,
    );
  }
  if (beforeStart.deadlineExpired) {
    throw PrintFailure.failed('error.print.excelExportTimedOut');
  }

  // A rejected StartDoc is certain: nothing reached the spooler.
  final jobId = effects.startDoc();

  final afterStart = signals.sample();
  if (afterStart.cancelled) {
    effects.abortDoc();
    throw PrintFailure.cancelled(
      'error.print.excelPrintCancelledUnconfirmed',
      uncertain: true,
    );
  }
  if (afterStart.deadlineExpired) {
    effects.abortDoc();
    throw PrintFailure.uncertain('error.print.excelPrintCompletionUnconfirmed');
  }

  try {
    effects.renderPages();
  } on PrintFailure catch (failure) {
    effects.abortDoc();
    throw _postEffectFailure(failure);
  }

  try {
    effects.endDoc();
  } on PrintFailure catch (failure) {
    throw _postEffectFailure(failure);
  }

  try {
    effects.observeJob(jobId);
  } on PrintFailure catch (failure) {
    throw _postEffectFailure(failure);
  }
}
