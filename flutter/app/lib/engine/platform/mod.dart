/// Port of src-tauri/src/platform/mod.rs: platform-neutral controlled-print
/// classification helpers, the Windows job-state classifiers, and the
/// production [PrintAdapter] selector.
///
/// The Windows classifiers are pure and therefore run and are tested on every
/// host. The native adapters themselves are in `windows/printing.dart` and
/// `macos/printing.dart`.
library;

import 'dart:io' show Platform;

import '../print_queue.dart';
import 'macos/printing.dart' as macos_printing;
import 'windows/printing.dart' as windows_printing;

export 'startup.dart' show setStartWithWindows;

/// Port of `consume_controlled_page_bytes`: the bytes returned by [render]
/// are the only bytes handed to [consume]; no path is reopened.
T consumeControlledPageBytes<T>(
  List<int> Function() render,
  T Function(List<int> bytes) consume,
) {
  final rendered = render();
  return consume(rendered);
}

/// Port of `map_controlled_print_failure`: failures that already carry a
/// localization key (`error.print.*`) pass through unchanged. Raw helper
/// failures are replaced with the stable controlled-route key for their kind.
PrintFailure mapControlledPrintFailure(PrintFailure failure) {
  if (failure.message.startsWith('error.print.')) {
    return failure;
  }
  final message = switch (failure.kind) {
    PrintFailureKind.failed => 'error.print.excelPrintPreparationFailed',
    PrintFailureKind.unsupported => 'error.print.excelControlledPdfUnavailable',
    PrintFailureKind.unsupportedOptions => 'error.print.excelPrintOptionsRejected',
    PrintFailureKind.submittedUnconfirmed ||
    PrintFailureKind.uncertain => 'error.print.excelPrintCompletionUnconfirmed',
    PrintFailureKind.cancelled when failure.uncertain =>
      'error.print.excelPrintCancelledUnconfirmed',
    PrintFailureKind.cancelled => 'error.print.excelExportCancelled',
  };
  return _replaceMessage(failure, message);
}

/// Rebuilds [failure] with a new message while preserving its kind,
/// uncertainty, retry disposition, cleanup review, and diagnostic detail.
PrintFailure _replaceMessage(PrintFailure failure, String message) {
  var replaced = switch (failure.kind) {
    PrintFailureKind.failed => PrintFailure.failed(message),
    PrintFailureKind.unsupported => PrintFailure.unsupported(message),
    PrintFailureKind.unsupportedOptions => PrintFailure.unsupportedOptions(
      message,
    ),
    PrintFailureKind.submittedUnconfirmed => PrintFailure.submittedUnconfirmed(
      message,
    ),
    PrintFailureKind.uncertain => PrintFailure.uncertain(message),
    PrintFailureKind.cancelled => PrintFailure.cancelled(
      message,
      uncertain: failure.uncertain,
    ),
  };
  if (failure.cleanupReview != null) {
    replaced = replaced.withCleanupReview(failure.cleanupReview!);
  }
  replaced = replaced.withRetryDisposition(failure.retryDisposition);
  if (failure.diagnosticDetail != null) {
    replaced = replaced.withDiagnostic(failure.diagnosticDetail!);
  }
  return replaced;
}

/// Port of `WindowsSubmittedJobStatus`.
enum WindowsSubmittedJobStatus { complete, uncertain, pending }

/// Port of `WindowsPostSubmissionObservation`.
enum WindowsPostSubmissionObservation {
  printerOpen,
  jobEnumeration,
  jobRead,
  statusFault,
}

/// Port of `WindowsJobQueryError`.
enum WindowsJobQueryError { released, retryWithLargerBuffer, uncertain }

/// Port of `classify_windows_job_query_error`.
WindowsJobQueryError classifyWindowsJobQueryError(int error) => switch (error) {
  2 || 87 => WindowsJobQueryError.released,
  122 => WindowsJobQueryError.retryWithLargerBuffer,
  _ => WindowsJobQueryError.uncertain,
};

/// Port of `windows_post_submission_observation_error`.
PrintFailure windowsPostSubmissionObservationError(
  WindowsPostSubmissionObservation stage,
) {
  final message = switch (stage) {
    WindowsPostSubmissionObservation.printerOpen =>
      'Windows could not open the printer after job submission',
    WindowsPostSubmissionObservation.jobEnumeration =>
      'Windows print queue could not be enumerated after job submission',
    WindowsPostSubmissionObservation.jobRead =>
      'Windows spool job state could not be read after submission',
    WindowsPostSubmissionObservation.statusFault =>
      'Windows spooler reported a fault after the job was submitted',
  };
  if (stage == WindowsPostSubmissionObservation.statusFault) {
    return PrintFailure.uncertain(message);
  }
  return PrintFailure.submittedUnconfirmed(message);
}

/// Port of `windows_post_submission_failure`.
PrintFailure windowsPostSubmissionFailure(PrintFailure failure) {
  if (failure.kind == PrintFailureKind.cancelled) {
    return PrintFailure.cancelled(failure.message, uncertain: true);
  }
  return PrintFailure.uncertain(failure.message);
}

/// Port of `windows_post_submission_cancellation`.
PrintFailure windowsPostSubmissionCancellation({required bool requestAccepted}) {
  return PrintFailure.cancelled(
    requestAccepted
        ? 'Windows accepted cancellation of an active spool job'
        : 'Windows could not confirm cancellation of an active spool job',
    uncertain: true,
  );
}

/// Port of `classify_windows_submitted_job_status`. JOB_STATUS_* is a stable
/// Win32 bitmask. Generic error, offline, paper-out, blocked, and
/// user-intervention states wait until release, completion, cancellation, or
/// timeout. Only destructive deletion states are immediately uncertain.
WindowsSubmittedJobStatus classifyWindowsSubmittedJobStatus(int status) {
  const uncertainMask = 0x0000_0004 // DELETING
      | 0x0000_0100; // DELETED
  const completeMask = 0x0000_0080 // PRINTED
      | 0x0000_1000; // COMPLETE

  if (status & uncertainMask != 0) {
    return WindowsSubmittedJobStatus.uncertain;
  }
  if (status & completeMask != 0) {
    return WindowsSubmittedJobStatus.complete;
  }
  return WindowsSubmittedJobStatus.pending;
}

/// Port of `select_windows_submitted_job`. Accepts one fresh job whose
/// document title matches (ASCII case-insensitively, either direction), or
/// the single fresh job when no title matches. Anything ambiguous yields null.
int? selectWindowsSubmittedJob(
  Set<int> knownIds,
  List<(int, String)> candidates,
  String expectedTitle,
) {
  final fresh = [
    for (final candidate in candidates)
      if (!knownIds.contains(candidate.$1)) candidate,
  ];
  final expected = _asciiLower(expectedTitle);
  final named = [
    for (final candidate in fresh)
      if (_matchesTitle(_asciiLower(candidate.$2), expected)) candidate,
  ];
  if (named.length == 1) {
    return named[0].$1;
  }
  if (named.isEmpty && fresh.length == 1) {
    return fresh[0].$1;
  }
  return null;
}

bool _matchesTitle(String document, String expected) =>
    document.isNotEmpty &&
    (document.contains(expected) || expected.contains(document));

String _asciiLower(String value) => String.fromCharCodes(
  value.codeUnits.map((unit) => unit >= 0x41 && unit <= 0x5a ? unit + 32 : unit),
);

/// Port of `classify_windows_submitted_job_observation`. Called only after
/// EndDoc transferred the complete document. A missing job means the spooler
/// released it, which many drivers do on completion.
WindowsSubmittedJobStatus classifyWindowsSubmittedJobObservation(int? status) {
  if (status == null) {
    return WindowsSubmittedJobStatus.complete;
  }
  return classifyWindowsSubmittedJobStatus(status);
}

/// Port of `validate_windows_image_frame_count`.
void validateWindowsImageFrameCount(int frameCount) {
  if (frameCount != 1) {
    throw PrintFailure.unsupported(
      'Windows controlled image printing requires exactly one image frame',
    );
  }
}

/// Port of `production_print_adapter`. Windows and macOS get their native
/// adapters; every other host gets a fail-closed adapter.
PrintAdapter productionPrintAdapter() {
  if (Platform.isMacOS) {
    return macos_printing.MacOsPrintAdapter();
  }
  if (Platform.isWindows) {
    return windows_printing.WindowsPrintAdapter();
  }
  return const UnsupportedPrintAdapter();
}

/// Port of `UnsupportedPrintAdapter`.
class UnsupportedPrintAdapter implements PrintAdapter {
  const UnsupportedPrintAdapter();

  @override
  PrinterInventory inventory() => const PrinterInventory();

  @override
  PrintJobSupport inspect(String path, String printer, PrintSettings settings) {
    throw PrintFailure.unsupported('printing is unsupported on this platform');
  }

  @override
  void submit(PrintJob job, bool Function() cancelled) {
    throw PrintFailure.failed('printing is unsupported on this platform');
  }
}
