// Port of src-tauri/src/platform/excel.rs (production export supervisor).
//
// The supervisor stages a copy of an approved workbook into a private
// workspace, runs a worksheet PDF export in a worker, validates the produced
// PDF and submits it through the controlled print path. Every failure path
// preserves the Rust cleanup contract: staged files are removed before a
// failure is reported, and a cleanup that cannot be proven quarantines the
// workspace and trips the process-wide breaker.
//
// Concurrency model: Rust used OS threads with blocking polls. Dart is
// single-threaded per isolate, so the same waits are expressed as Futures
// with the same deadlines, poll cadence and teardown grace. The exporter runs
// on whatever runtime the caller supplies (see [FutureExcelWorkerRuntime]).
library;

import 'dart:async';
import 'dart:collection';
import 'dart:io';
import 'dart:typed_data';

import 'package:uuid/uuid.dart';

import '../cap_fs.dart';
import '../ooxml.dart' show WorkbookSafetyFacts, WorksheetDescriptor;
import '../print_queue.dart' show ExcelExportPipelinePort;
import '../progress.dart' show CancellationToken;
import 'excel_print_ports.dart';

/// `WORKER_POLL_INTERVAL`.
const Duration workerPollInterval = Duration(milliseconds: 10);

/// `crate::pdf::MAX_PDF_BYTES` (512 MiB).
const int maxPdfBytes = 512 * 1024 * 1024;

const String _exportCleanupWarningKey = 'error.print.exportCleanupRequiresReview';
const String _fallbackCleanupLocator = 'docflow-excel-cleanup';

/// A process-shared boolean (`Arc<AtomicBool>`).
final class ExcelFlag {
  ExcelFlag([this.value = false]);

  bool value;
}

/// The Excel operation lock (`Arc<Mutex<()>>`) with try-lock semantics.
/// [poisoned] models a `std::sync::Mutex` whose holder panicked.
final class ExcelOperationLock {
  bool _held = false;

  /// Set when a previous holder failed while owning the lock.
  bool poisoned = false;

  bool get isHeld => _held;

  /// `try_lock`: returns true when the lock was acquired.
  bool tryAcquire() {
    if (_held) {
      return false;
    }
    _held = true;
    return true;
  }

  void release() {
    _held = false;
  }
}

/// `ExcelProcessGuards`: breaker, operation lock and quarantine shared by
/// every pipeline in the process.
final class ExcelProcessGuards {
  ExcelProcessGuards({
    ExcelFlag? breaker,
    ExcelOperationLock? operation,
    SplayTreeSet<String>? quarantine,
  }) : breaker = breaker ?? ExcelFlag(),
       operation = operation ?? ExcelOperationLock(),
       quarantine = quarantine ?? SplayTreeSet<String>();

  final ExcelFlag breaker;
  final ExcelOperationLock operation;

  /// Sorted workspace paths whose cleanup could not be proven.
  final SplayTreeSet<String> quarantine;
}

ExcelProcessGuards? _processExcelGuards;

/// `process_excel_guards`: every production caller shares one set of guards
/// for the life of the process.
ExcelProcessGuards processExcelGuards() =>
    _processExcelGuards ??= ExcelProcessGuards();

/// `MonotonicClock`. Instants are durations since an arbitrary origin.
abstract interface class MonotonicClock {
  Duration now();
}

/// `SystemMonotonicClock`.
final class SystemMonotonicClock implements MonotonicClock {
  SystemMonotonicClock() : _watch = Stopwatch()..start();

  final Stopwatch _watch;

  @override
  Duration now() => _watch.elapsed;
}

/// `ExcelExportRequest`.
final class ExcelExportRequest {
  ExcelExportRequest({
    required this.requestId,
    required this.generation,
    required this.stagedWorkbook,
    required this.safety,
    required this.worksheet,
    required this.deadline,
    required this.cancellation,
  });

  final String requestId;
  final int generation;
  final StagedWorkbook stagedWorkbook;
  final WorkbookSafetyFacts safety;
  final WorksheetDescriptor worksheet;
  final Duration deadline;
  final CancellationToken cancellation;

  CleanupReview cleanupReview() => CleanupReview(
    warningKey: _exportCleanupWarningKey,
    locator: stagedWorkbook.cleanupLocator(),
  );
}

/// `WorksheetPdfExporter`. Exporters take ownership of the request and must
/// either return an [ExportedWorksheetPdf] that owns its staged workbook, or
/// throw; an unconsumed staged workbook is then cleaned up by the runtime.
abstract interface class WorksheetPdfExporter {
  Future<ExportedWorksheetPdf> exportPdf(ExcelExportRequest request);
}

/// `WorkbookSourcePolicy`: pre-stage validation of the source workbook.
abstract interface class WorkbookSourcePolicy {
  void validateSource(AnchoredPath source, WorkbookSafetyFacts safety);
}

/// `ControlledPdfSubmitter`: hands a validated PDF to the print path.
abstract interface class ControlledPdfSubmitter {
  Future<void> submitPdf(
    PrintJob job,
    ExportedWorksheetPdf pdf,
    CancellationToken cancellation,
    Duration deadline,
  );
}

/// `WorkerResponse`: exactly one of [artifact] or [failure] is set.
final class WorkerResponse {
  const WorkerResponse({
    required this.requestId,
    required this.generation,
    this.artifact,
    this.failure,
  }) : assert((artifact == null) != (failure == null));

  final String requestId;
  final int generation;
  final ExportedWorksheetPdf? artifact;
  final PrintFailure? failure;
}

/// `WorkerPoll`.
sealed class WorkerPoll {
  const WorkerPoll();
}

final class WorkerPollResponse extends WorkerPoll {
  const WorkerPollResponse(this.response);
  final WorkerResponse response;
}

final class WorkerPollPending extends WorkerPoll {
  const WorkerPollPending();
}

final class WorkerPollDisconnected extends WorkerPoll {
  const WorkerPollDisconnected();
}

/// `WorkerJoin`.
enum WorkerJoin { pending, finished, failed }

/// `ExcelWorkerRuntime`: starts one export worker.
abstract interface class ExcelWorkerRuntime {
  ExcelWorkerHandle launch(
    WorksheetPdfExporter exporter,
    ExcelExportRequest request,
  );
}

/// `ExcelWorkerHandle`.
abstract interface class ExcelWorkerHandle {
  /// Waits up to [wait] for a response.
  Future<WorkerPoll> poll(Duration wait);

  /// Non-blocking join status.
  WorkerJoin joinStatus();
}

/// Default runtime: runs the exporter as an asynchronous task in this
/// isolate. Exporters that block on native work must offload it themselves
/// (the Windows exporter runs COM on a dedicated isolate).
final class FutureExcelWorkerRuntime implements ExcelWorkerRuntime {
  const FutureExcelWorkerRuntime();

  @override
  ExcelWorkerHandle launch(
    WorksheetPdfExporter exporter,
    ExcelExportRequest request,
  ) {
    final handle = _FutureWorkerHandle();
    unawaited(handle._run(exporter, request));
    return handle;
  }
}

final class _FutureWorkerHandle implements ExcelWorkerHandle {
  final Completer<void> _done = Completer<void>();
  WorkerResponse? _response;
  bool _finished = false;
  bool _failed = false;

  /// Mirrors `thread::spawn` + `sync_channel(1)`: the exporter result is
  /// delivered after the request has been dropped, and a panic leaves no
  /// response behind.
  Future<void> _run(WorksheetPdfExporter exporter, ExcelExportRequest request) async {
    ExportedWorksheetPdf? artifact;
    PrintFailure? failure;
    try {
      artifact = await exporter.exportPdf(request);
    } on PrintFailure catch (error) {
      failure = error;
    } on Object {
      _failed = true;
    } finally {
      request.stagedWorkbook.dropIfNotConsumed();
    }
    if (!_failed) {
      _response = WorkerResponse(
        requestId: request.requestId,
        generation: request.generation,
        artifact: artifact,
        failure: failure,
      );
    }
    _finished = true;
    _done.complete();
  }

  @override
  Future<WorkerPoll> poll(Duration wait) async {
    final ready = _response;
    if (ready != null) {
      _response = null;
      return WorkerPollResponse(ready);
    }
    if (_done.isCompleted) {
      return const WorkerPollDisconnected();
    }
    try {
      await _done.future.timeout(wait);
    } on TimeoutException {
      return const WorkerPollPending();
    }
    final delivered = _response;
    if (delivered != null) {
      _response = null;
      return WorkerPollResponse(delivered);
    }
    return const WorkerPollDisconnected();
  }

  @override
  WorkerJoin joinStatus() {
    if (!_finished) {
      return WorkerJoin.pending;
    }
    return _failed ? WorkerJoin.failed : WorkerJoin.finished;
  }
}

enum _WaitKind { response, cancelled, timedOut, disconnected }

final class _WorkerWait {
  const _WorkerWait(this.kind, [this.response]);
  final _WaitKind kind;
  final WorkerResponse? response;
}

enum _LoserTeardown { finished, failed, cleanupFailed }

/// `ExcelExportPipeline`: the supervisor for worksheet export and submit.
final class ExcelExportPipeline implements ExcelExportPipelinePort {
  ExcelExportPipeline({
    required this.policy,
    required this.exporter,
    required this.submitter,
    ExcelProcessGuards? guards,
    MonotonicClock? clock,
    ExcelWorkerRuntime? runtime,
    this.teardownGrace = const Duration(seconds: 2),
    this.onOperationContended,
  }) : _guards = guards ?? processExcelGuards(),
       clock = clock ?? SystemMonotonicClock(),
       runtime = runtime ?? const FutureExcelWorkerRuntime();

  final WorkbookSourcePolicy policy;
  final WorksheetPdfExporter exporter;
  final ControlledPdfSubmitter submitter;
  final MonotonicClock clock;
  final ExcelWorkerRuntime runtime;
  final Duration teardownGrace;

  /// Test hook invoked each time the operation lock is found held.
  final void Function()? onOperationContended;

  final ExcelProcessGuards _guards;
  int _generation = 0;

  ExcelFlag get _breaker => _guards.breaker;

  ExcelOperationLock get _operation => _guards.operation;

  SplayTreeSet<String> get _quarantine => _guards.quarantine;

  /// Exposes the shared breaker for tests and diagnostics.
  bool get breakerTripped => _breaker.value;

  /// Exposes the current generation for tests.
  int get generation => _generation;

  /// `export_and_submit`.
  Future<PrintSubmissionOutcome> exportAndSubmit(
    PrintJob job,
    AnchoredPath source,
    OsHandle sourceFile,
    CancellationToken cancellation,
  ) async {
    final deadline = clock.now() + Duration(seconds: job.timeoutMinutes * 60);
    if (_breaker.value) {
      throw PrintFailure.unsupported('error.print.excelExportQuarantined');
    }
    await _acquireOperation(deadline, cancellation);
    try {
      return await _exportAndSubmitLocked(
        job,
        source,
        sourceFile,
        cancellation,
        deadline,
      );
    } finally {
      _operation.release();
    }
  }

  Future<PrintSubmissionOutcome> _exportAndSubmitLocked(
    PrintJob job,
    AnchoredPath source,
    OsHandle sourceFile,
    CancellationToken cancellation,
    Duration deadline,
  ) async {
    if (job.pageRange != 'all') {
      throw PrintFailure.unsupportedOptions(
        'error.print.worksheetPageRangeAllOnly',
      );
    }
    final worksheet = job.worksheet ??
        (throw PrintFailure.unsupported(
          'error.print.excelWorksheetMetadataMissing',
        ));
    final safety = job.workbookSafety ??
        (throw PrintFailure.unsupported(
          'error.print.excelSafetyMetadataMissing',
        ));
    await _preWorkerFence(deadline, cancellation);
    policy.validateSource(source, safety);
    await _preWorkerFence(deadline, cancellation);

    final StagedWorkbook staged;
    try {
      staged = StagedWorkbook.stage(
        sourceFile: sourceFile,
        sourcePath: job.path,
        approvedSignature: job.signature,
        cleanupBreaker: _breaker,
        cleanupQuarantine: _quarantine,
      );
    } on PrintFailure catch (failure) {
      throw _preferCleanupReview(failure);
    }
    try {
      await _preWorkerFence(deadline, cancellation);
    } on PrintFailure catch (failure) {
      staged.cleanupQuietly();
      throw _preferCleanupReview(failure);
    }
    final int previousGeneration = _generation;
    _generation = previousGeneration + 1;
    final generation = _generation;
    final requestId = const Uuid().v4();
    final request = ExcelExportRequest(
      requestId: requestId,
      generation: generation,
      stagedWorkbook: staged,
      safety: safety,
      worksheet: worksheet,
      deadline: deadline,
      cancellation: cancellation,
    );
    final unprovenCleanupReview = request.cleanupReview();
    final ExcelWorkerHandle worker;
    try {
      worker = runtime.launch(exporter, request);
    } on PrintFailure catch (failure) {
      staged.cleanupQuietly();
      throw _preferCleanupReview(failure);
    }

    final wait = await _waitForResponse(worker, deadline, cancellation);
    switch (wait.kind) {
      case _WaitKind.cancelled:
        _invalidateGeneration(generation);
        final teardown = await _teardownLoser(worker);
        throw _classifyLoser(
          teardown,
          _cleanCancelled(),
          unprovenCleanupReview,
        );
      case _WaitKind.timedOut:
        _invalidateGeneration(generation);
        final teardown = await _teardownLoser(worker);
        throw _classifyLoser(
          teardown,
          PrintFailure.failed('error.print.excelExportTimedOut'),
          unprovenCleanupReview,
        );
      case _WaitKind.disconnected:
        final teardown = await _teardownLoser(worker);
        throw _classifyLoser(
          teardown,
          PrintFailure.failed('error.print.excelWorkerStopped'),
          unprovenCleanupReview,
        );
      case _WaitKind.response:
        break;
    }
    final response = wait.response!;

    if (!await _finishWorker(worker)) {
      _invalidateGeneration(generation);
      final PrintFailure failure;
      final artifact = response.artifact;
      if (artifact != null) {
        artifact.cleanupQuietly();
        failure = _quarantinedFailure();
      } else {
        failure = response.failure!;
      }
      _tripBreaker();
      throw _preferCleanupReview(failure);
    }
    if (response.requestId != requestId ||
        response.generation != generation ||
        _generation != generation) {
      _invalidateGeneration(generation);
      final artifact = response.artifact;
      if (artifact != null) {
        _finishWithoutSubmit(
          artifact,
          PrintFailure.failed('error.print.excelResponseStale'),
        );
      }
      throw _preferCleanupReview(
        PrintFailure.failed('error.print.excelResponseStale'),
      );
    }
    final artifact = response.artifact;
    if (artifact == null) {
      throw _preferCleanupReview(response.failure!);
    }

    final fenceAfterExport = _exportFenceFailure(
      requestId,
      generation,
      response.requestId,
      response.generation,
      deadline,
      cancellation,
    );
    if (fenceAfterExport != null) {
      _invalidateGeneration(generation);
      _finishWithoutSubmit(artifact, fenceAfterExport);
    }

    final sourceIsUnchanged = _sourceIsUnchanged(source, job.signature);
    if (!sourceIsUnchanged) {
      _finishWithoutSubmit(
        artifact,
        PrintFailure.failed('error.print.workbookChanged'),
      );
    }
    final fenceAfterSourceCheck = _exportFenceFailure(
      requestId,
      generation,
      response.requestId,
      response.generation,
      deadline,
      cancellation,
    );
    if (fenceAfterSourceCheck != null) {
      _invalidateGeneration(generation);
      _finishWithoutSubmit(artifact, fenceAfterSourceCheck);
    }

    try {
      artifact.validateForSubmission();
    } on PrintFailure catch (failure) {
      _finishWithoutSubmit(artifact, failure);
    }
    final fenceAfterValidation = _exportFenceFailure(
      requestId,
      generation,
      response.requestId,
      response.generation,
      deadline,
      cancellation,
    );
    if (fenceAfterValidation != null) {
      _invalidateGeneration(generation);
      _finishWithoutSubmit(artifact, fenceAfterValidation);
    }

    final controlledJob = job.copyWith(pageRange: 'all');
    PrintFailure? submitFailure;
    try {
      await submitter.submitPdf(controlledJob, artifact, cancellation, deadline);
    } on PrintFailure catch (failure) {
      submitFailure = failure;
    } on Object {
      artifact.cleanupQuietly();
      rethrow;
    }
    final cleanupLocator = artifact.cleanupLocator();
    Object? cleanupError;
    try {
      artifact.cleanup();
    } on Object catch (error) {
      cleanupError = error;
    }
    if (cleanupError == null) {
      if (submitFailure != null) {
        throw submitFailure;
      }
      return const PrintSubmissionOutcome();
    }
    _tripBreaker();
    if (submitFailure == null) {
      return PrintSubmissionOutcome(
        cleanupReview: CleanupReview(
          warningKey: _exportCleanupWarningKey,
          locator: cleanupLocator,
        ),
      );
    }
    throw submitFailure
        .withRetryDisposition(RetryDisposition.cleanupQuarantined)
        .withCleanupReview(
          CleanupReview(
            warningKey: _exportCleanupWarningKey,
            locator: cleanupLocator,
          ),
        );
  }

  /// `source.open_read()` + `file_signature_from_open_file(.., true) == job.signature`.
  bool _sourceIsUnchanged(AnchoredPath source, FileSignature expected) {
    OsHandle? reopened;
    try {
      reopened = source.openRead();
      final signature = fileSignatureFromOpenHandle(
        reopened,
        includeSha256: true,
      );
      return signature == expected;
    } on Object {
      return false;
    } finally {
      reopened?.close();
    }
  }

  PrintFailure _preferCleanupReview(PrintFailure failure) {
    final review = _cleanupReview();
    if (review == null) {
      return failure;
    }
    return failure
        .withRetryDisposition(RetryDisposition.cleanupQuarantined)
        .withCleanupReview(review);
  }

  /// First quarantined workspace with a usable file name, in sorted order.
  CleanupReview? _cleanupReview() {
    for (final path in _quarantine) {
      final name = rustFileName(path);
      if (name != null && name.isNotEmpty) {
        return CleanupReview(
          warningKey: _exportCleanupWarningKey,
          locator: name,
        );
      }
    }
    return null;
  }

  /// `acquire_operation`: polls the operation lock until it is free or the
  /// deadline passes. The lock is held when this future completes normally.
  Future<void> _acquireOperation(
    Duration deadline,
    CancellationToken cancellation,
  ) async {
    while (true) {
      await _preWorkerFence(deadline, cancellation);
      if (_operation.poisoned) {
        _tripBreaker();
        throw _quarantinedFailure();
      }
      if (_operation.tryAcquire()) {
        try {
          await _preWorkerFence(deadline, cancellation);
        } on Object {
          _operation.release();
          rethrow;
        }
        return;
      }
      onOperationContended?.call();
      final now = clock.now();
      if (now >= deadline) {
        throw PrintFailure.failed('error.print.excelExportTimedOut');
      }
      await Future<void>.delayed(_minDuration(deadline - now, workerPollInterval));
    }
  }

  Future<void> _preWorkerFence(
    Duration deadline,
    CancellationToken cancellation,
  ) async {
    if (_breaker.value) {
      throw _quarantinedFailure();
    }
    if (cancellation.isCancelled) {
      throw _cleanCancelled();
    }
    if (clock.now() >= deadline) {
      throw PrintFailure.failed('error.print.excelExportTimedOut');
    }
  }

  PrintFailure? _exportFenceFailure(
    String requestId,
    int generation,
    String responseRequestId,
    int responseGeneration,
    Duration deadline,
    CancellationToken cancellation,
  ) {
    if (_breaker.value) {
      return _quarantinedFailure();
    }
    if (responseRequestId != requestId ||
        responseGeneration != generation ||
        _generation != generation) {
      return PrintFailure.failed('error.print.excelResponseStale');
    }
    if (cancellation.isCancelled) {
      return _cleanCancelled();
    }
    if (clock.now() >= deadline) {
      return PrintFailure.failed('error.print.excelExportTimedOut');
    }
    return null;
  }

  Future<_WorkerWait> _waitForResponse(
    ExcelWorkerHandle worker,
    Duration deadline,
    CancellationToken cancellation,
  ) async {
    while (true) {
      if (cancellation.isCancelled) {
        return const _WorkerWait(_WaitKind.cancelled);
      }
      final now = clock.now();
      if (now >= deadline) {
        return const _WorkerWait(_WaitKind.timedOut);
      }
      final wait = _minDuration(deadline - now, workerPollInterval);
      final poll = await worker.poll(wait);
      if (cancellation.isCancelled) {
        _disposePoll(poll);
        return const _WorkerWait(_WaitKind.cancelled);
      }
      if (clock.now() >= deadline) {
        _disposePoll(poll);
        return const _WorkerWait(_WaitKind.timedOut);
      }
      switch (poll) {
        case WorkerPollResponse(:final response):
          return _WorkerWait(_WaitKind.response, response);
        case WorkerPollPending():
          break;
        case WorkerPollDisconnected():
          return const _WorkerWait(_WaitKind.disconnected);
      }
    }
  }

  /// A response that is discarded must release its artifact (Rust `Drop`).
  void _disposePoll(WorkerPoll poll) {
    if (poll is WorkerPollResponse) {
      poll.response.artifact?.cleanupQuietly();
    }
  }

  Future<bool> _finishWorker(ExcelWorkerHandle worker) async {
    final deadline = clock.now() + teardownGrace;
    while (true) {
      switch (worker.joinStatus()) {
        case WorkerJoin.finished:
          return true;
        case WorkerJoin.failed:
          return false;
        case WorkerJoin.pending:
          break;
      }
      final now = clock.now();
      if (now >= deadline) {
        return false;
      }
      final wait = _minDuration(deadline - now, workerPollInterval);
      _disposePoll(await worker.poll(wait));
    }
  }

  Future<_LoserTeardown> _teardownLoser(ExcelWorkerHandle worker) async {
    final deadline = clock.now() + teardownGrace;
    var cleanupFailed = false;
    while (true) {
      switch (worker.joinStatus()) {
        case WorkerJoin.finished:
          cleanupFailed = await _drainLoserResponses(worker) || cleanupFailed;
          return cleanupFailed || _breaker.value
              ? _LoserTeardown.cleanupFailed
              : _LoserTeardown.finished;
        case WorkerJoin.failed:
          cleanupFailed = await _drainLoserResponses(worker) || cleanupFailed;
          return cleanupFailed || _breaker.value
              ? _LoserTeardown.cleanupFailed
              : _LoserTeardown.failed;
        case WorkerJoin.pending:
          break;
      }
      final now = clock.now();
      if (now >= deadline) {
        return cleanupFailed || _breaker.value
            ? _LoserTeardown.cleanupFailed
            : _LoserTeardown.failed;
      }
      final wait = _minDuration(deadline - now, workerPollInterval);
      final poll = await worker.poll(wait);
      if (poll case WorkerPollResponse(:final response)) {
        cleanupFailed = _cleanupLoserResponse(response) || cleanupFailed;
      }
    }
  }

  Future<bool> _drainLoserResponses(ExcelWorkerHandle worker) async {
    var cleanupFailed = false;
    while (true) {
      final poll = await worker.poll(Duration.zero);
      switch (poll) {
        case WorkerPollResponse(:final response):
          cleanupFailed = _cleanupLoserResponse(response) || cleanupFailed;
          break;
        case WorkerPollPending():
        case WorkerPollDisconnected():
          return cleanupFailed;
      }
    }
  }

  bool _cleanupLoserResponse(WorkerResponse response) {
    final artifact = response.artifact;
    if (artifact == null) {
      return false;
    }
    try {
      artifact.cleanup();
      return false;
    } on Object {
      _tripBreaker();
      return true;
    }
  }

  PrintFailure _classifyLoser(
    _LoserTeardown teardown,
    PrintFailure finishedFailure,
    CleanupReview unprovenCleanupReview,
  ) {
    switch (teardown) {
      case _LoserTeardown.cleanupFailed:
      case _LoserTeardown.finished:
        return _preferCleanupReview(finishedFailure);
      case _LoserTeardown.failed:
        _tripBreaker();
        return _attachUnprovenCleanupReview(
          _preferCleanupReview(finishedFailure),
          unprovenCleanupReview,
        );
    }
  }

  /// `finish_without_submit`: always throws. Cleans the artifact first and
  /// trips the breaker if the cleanup cannot be proven.
  Never _finishWithoutSubmit(ExportedWorksheetPdf artifact, PrintFailure failure) {
    final cleanupLocator = artifact.cleanupLocator();
    try {
      artifact.cleanup();
    } on Object {
      _tripBreaker();
      throw failure
          .withRetryDisposition(RetryDisposition.cleanupQuarantined)
          .withCleanupReview(
            CleanupReview(
              warningKey: _exportCleanupWarningKey,
              locator: cleanupLocator,
            ),
          );
    }
    throw failure;
  }

  void _invalidateGeneration(int generation) {
    if (_generation == generation) {
      _generation = generation + 1;
    }
  }

  void _tripBreaker() {
    _breaker.value = true;
  }

  /// Test-only: drops every quarantined workspace and clears the set.
  void clearTestQuarantine() {
    for (final path in _quarantine.toList()) {
      final directory = Directory(path);
      if (directory.existsSync()) {
        directory.deleteSync(recursive: true);
      }
      _quarantine.remove(path);
    }
  }
}

PrintFailure _cleanCancelled() =>
    PrintFailure.cancelled('error.print.excelExportCancelled', uncertain: false);

PrintFailure _quarantinedFailure() =>
    PrintFailure.unsupported('error.print.excelExportQuarantined');

PrintFailure _attachUnprovenCleanupReview(
  PrintFailure failure,
  CleanupReview cleanupReview,
) {
  if (failure.hasCleanupReview) {
    return failure;
  }
  return failure
      .withRetryDisposition(RetryDisposition.cleanupQuarantined)
      .withCleanupReview(cleanupReview);
}

Duration _minDuration(Duration left, Duration right) =>
    left < right ? left : right;

/// `Path::file_name`: the last normal component, or null.
String? rustFileName(String path) {
  final parts = path
      .split(RegExp(r'[\\/]'))
      .where((part) => part.isNotEmpty && part != '.')
      .toList();
  if (parts.isEmpty) {
    return null;
  }
  final last = parts.last;
  if (last == '..') {
    return null;
  }
  return last;
}

String _joinPath(String left, String right) {
  if (left.endsWith(Platform.pathSeparator)) {
    return '$left$right';
  }
  return '$left${Platform.pathSeparator}$right';
}

String _baseName(String path) {
  final name = rustFileName(path);
  return name ?? '';
}

/// `StagedWorkbook`: a private copy of the approved workbook inside a
/// workspace directory. Cleanup runs in the exact Rust order: the workbook
/// anchor, then the root, then the workspace (whose removal can be quarantined).
final class StagedWorkbook {
  StagedWorkbook._({
    required this._workspacePath,
    required this.cleanupBreaker,
    required this.cleanupQuarantine,
  });

  String? _workspacePath;
  AnchoredPath? _workbook;
  CapabilityRoot? _root;
  bool _consumed = false;

  /// Absolute path of the staged workbook copy.
  String workbookPath = '';

  final ExcelFlag cleanupBreaker;
  final SplayTreeSet<String> cleanupQuarantine;

  /// Test probe: drop events in the order they happened.
  List<String>? dropEvents;

  /// Test probe: makes the workspace removal fail (the directory is kept).
  bool cleanupFailure = false;

  /// `StagedWorkbook::stage`.
  static StagedWorkbook stage({
    required OsHandle sourceFile,
    required String sourcePath,
    required FileSignature approvedSignature,
    required ExcelFlag cleanupBreaker,
    required SplayTreeSet<String> cleanupQuarantine,
  }) {
    final Directory workspace;
    try {
      workspace = Directory.systemTemp.createTempSync('docflow-excel-');
    } on Object {
      throw PrintFailure.failed('error.print.excelStagingUnavailable');
    }
    final staged = StagedWorkbook._(
      workspacePath: workspace.path,
      cleanupBreaker: cleanupBreaker,
      cleanupQuarantine: cleanupQuarantine,
    );
    try {
      final CapabilityRoot root;
      try {
        root = CapabilityRoot.open(workspace.path);
      } on Object {
        throw PrintFailure.failed('error.print.excelStagingUnavailable');
      }
      staged._root = root;
      final sourceName = rustFileName(sourcePath);
      if (sourceName == null || sourceName.isEmpty) {
        throw PrintFailure.failed('error.print.excelSourceNameUnavailable');
      }
      staged.workbookPath = _joinPath(workspace.path, sourceName);
      final AnchoredPath workbook;
      try {
        workbook = root.authorize(staged.workbookPath).anchor();
      } on Object {
        throw PrintFailure.failed('error.print.excelStagingUnavailable');
      }
      staged._workbook = workbook;
      try {
        workbook.copyNoClobberFromFile(sourceFile);
      } on Object {
        throw PrintFailure.failed('error.print.excelStagingFailed');
      }
      final String digest;
      try {
        digest = workbook.sha256();
      } on Object {
        throw PrintFailure.failed('error.print.excelStagingVerificationFailed');
      }
      if (digest != (approvedSignature.sha256 ?? '')) {
        throw PrintFailure.failed('error.print.excelStagingVerificationFailed');
      }
      return staged;
    } on Object {
      staged.cleanupQuietly();
      rethrow;
    }
  }

  /// Test fixture: a staged workbook over caller-owned capabilities.
  factory StagedWorkbook.fixture({
    required String workspacePath,
    required AnchoredPath workbook,
    required CapabilityRoot root,
    required String workbookPath,
    required ExcelFlag breaker,
    required SplayTreeSet<String> quarantine,
    List<String>? dropEvents,
    bool cleanupFailure = false,
  }) {
    final staged = StagedWorkbook._(
      workspacePath: workspacePath,
      cleanupBreaker: breaker,
      cleanupQuarantine: quarantine,
    );
    staged._workbook = workbook;
    staged._root = root;
    staged.workbookPath = workbookPath;
    staged.dropEvents = dropEvents;
    staged.cleanupFailure = cleanupFailure;
    return staged;
  }

  /// Test fixture: a workspace holding `book.xlsx` ("workbook" bytes) with
  /// a drop probe, mirroring `StagedWorkbook::fixture_with_drop_probe`.
  static (StagedWorkbook, List<String>) fixtureWithDropProbe() {
    final workspace = Directory.systemTemp.createTempSync('docflow-excel-test-');
    final root = CapabilityRoot.open(workspace.path);
    final workbookPath = _joinPath(workspace.path, 'book.xlsx');
    final workbook = root.authorize(workbookPath).anchor();
    workbook.writeNoClobber('workbook'.codeUnits);
    final events = <String>[];
    final staged = StagedWorkbook.fixture(
      workspacePath: workspace.path,
      workbook: workbook,
      root: root,
      workbookPath: workbookPath,
      breaker: ExcelFlag(),
      quarantine: SplayTreeSet<String>(),
      dropEvents: events,
    );
    return (staged, events);
  }

  /// `quarantine_after_teardown_failure`.
  void quarantineAfterTeardownFailure() {
    cleanupBreaker.value = true;
  }

  /// `workspace_path`: the live workspace directory (throws when cleaned).
  String get workspacePath => _requireWorkspace();

  String cleanupLocator() {
    final path = _workspacePath;
    if (path == null) {
      return _fallbackCleanupLocator;
    }
    final name = _baseName(path);
    return name.isEmpty ? _fallbackCleanupLocator : name;
  }

  String pdfOutputPath() => _joinPath(_requireWorkspace(), 'worksheet.pdf');

  /// Marks the request-level ownership as moved (Rust move semantics).
  void dropIfNotConsumed() {
    if (!_consumed) {
      cleanupQuietly();
    }
  }

  /// `into_exported_pdf`: consumes this staged workbook on success. On
  /// failure the workspace is cleaned up before the error is thrown.
  ExportedWorksheetPdf intoExportedPdf() {
    final pdfPath = pdfOutputPath();
    try {
      final AnchoredPath pdf;
      try {
        final authorized = _root!.authorize(pdfPath);
        final identity = authorized.anchor().identity();
        pdf = authorized.withExpectedIdentity(identity).anchor();
      } on Object {
        throw PrintFailure.failed('error.print.excelPrivatePdfUnavailable');
      }
      final int length;
      try {
        length = pdf.len();
      } on Object {
        throw PrintFailure.failed('error.print.excelPrivatePdfUnavailable');
      }
      if (length == 0) {
        throw PrintFailure.failed('error.print.excelPrivatePdfEmpty');
      }
      _consumed = true;
      return ExportedWorksheetPdf._(pdf: pdf, workspace: this);
    } on Object {
      cleanupQuietly();
      rethrow;
    }
  }

  /// `cleanup`: idempotent; each owned object is released once.
  void cleanup() {
    final workbook = _workbook;
    if (workbook != null) {
      _workbook = null;
      workbook.parent.close();
      _recordDrop('workbook');
    }
    final root = _root;
    if (root != null) {
      _root = null;
      _recordDrop('root');
    }
    final path = _workspacePath;
    if (path == null) {
      return;
    }
    _workspacePath = null;
    _recordDrop('workspace');
    cleanupQuarantine.add(path);
    try {
      if (cleanupFailure) {
        throw const FileSystemException(
          'injected private workspace cleanup failure',
        );
      }
      final directory = Directory(path);
      if (directory.existsSync()) {
        directory.deleteSync(recursive: true);
      }
      cleanupQuarantine.remove(path);
    } on Object {
      cleanupBreaker.value = true;
      rethrow;
    }
  }

  /// `Drop`: cleanup errors are swallowed after the breaker is tripped.
  void cleanupQuietly() {
    try {
      cleanup();
    } on Object {
      // The breaker is already tripped by [cleanup].
    }
  }

  String _requireWorkspace() {
    final path = _workspacePath;
    if (path == null) {
      throw StateError('live staged workbook owns its workspace');
    }
    return path;
  }

  void _recordDrop(String event) {
    dropEvents?.add(event);
  }
}

/// `ExportedWorksheetPdf`: a PDF whose cleanup order is anchor first, then
/// the owning staged workbook.
final class ExportedWorksheetPdf {
  ExportedWorksheetPdf._({required this._pdf, required this._workspace});

  AnchoredPath? _pdf;
  StagedWorkbook? _workspace;
  Uint8List? _validatedBytes;

  /// `validate_for_submission`: bounded read plus PDF envelope check.
  void validateForSubmission() {
    final pdf = _pdf;
    if (pdf == null) {
      throw PrintFailure.failed('error.print.excelExportMalformedPdf');
    }
    final OsHandle source;
    final int declaredLength;
    try {
      source = pdf.openRead();
    } on Object {
      throw PrintFailure.failed('error.print.excelExportMalformedPdf');
    }
    try {
      try {
        declaredLength = source.metadata().size;
      } on Object {
        throw PrintFailure.failed('error.print.excelExportMalformedPdf');
      }
      final Uint8List bytes;
      try {
        bytes = readPdfBytesBoundedSync(
          handleChunks(source),
          declaredLength,
          maxPdfBytes,
        );
      } on PrintFailure {
        rethrow;
      } on Object {
        throw PrintFailure.failed('error.print.excelExportMalformedPdf');
      }
      validatePdfEnvelope(bytes);
      _validatedBytes = bytes;
    } finally {
      source.close();
    }
  }

  /// `validated_bytes`.
  Uint8List validatedBytes() {
    final bytes = _validatedBytes;
    if (bytes == null) {
      throw PrintFailure.failed('error.print.excelExportMalformedPdf');
    }
    return bytes;
  }

  String cleanupLocator() =>
      _workspace?.cleanupLocator() ?? _fallbackCleanupLocator;

  /// `cleanup`: idempotent.
  void cleanup() {
    final pdf = _pdf;
    if (pdf != null) {
      _pdf = null;
      pdf.parent.close();
      _workspace?._recordDrop('pdf');
    }
    final workspace = _workspace;
    if (workspace == null) {
      return;
    }
    _workspace = null;
    workspace.cleanup();
  }

  /// `Drop`.
  void cleanupQuietly() {
    try {
      cleanup();
    } on Object {
      // Breaker already tripped by the workspace cleanup.
    }
  }
}

/// `validate_pdf_envelope`: `%PDF-` prefix, `%%EOF` in the last 1 KiB and a
/// `startxref` marker.
void validatePdfEnvelope(Uint8List bytes) {
  final tailStart = bytes.length > 1024 ? bytes.length - 1024 : 0;
  final tail = Uint8List.sublistView(bytes, tailStart);
  if (!_startsWithAscii(bytes, '%PDF-') ||
      !_containsAscii(tail, '%%EOF') ||
      !_containsAscii(bytes, 'startxref')) {
    throw PrintFailure.failed('error.print.excelExportMalformedPdf');
  }
}

bool _startsWithAscii(Uint8List bytes, String needle) {
  if (bytes.length < needle.length) {
    return false;
  }
  for (var index = 0; index < needle.length; index++) {
    if (bytes[index] != needle.codeUnitAt(index)) {
      return false;
    }
  }
  return true;
}

bool _containsAscii(Uint8List bytes, String needle) {
  final width = needle.length;
  for (var start = 0; start + width <= bytes.length; start++) {
    var matched = true;
    for (var index = 0; index < width; index++) {
      if (bytes[start + index] != needle.codeUnitAt(index)) {
        matched = false;
        break;
      }
    }
    if (matched) {
      return true;
    }
  }
  return false;
}

/// Chunked reader over an open capability handle.
Iterable<List<int>> handleChunks(OsHandle handle, {int chunkSize = 64 * 1024}) sync* {
  final buffer = Uint8List(chunkSize);
  while (true) {
    final read = handle.read(buffer);
    if (read == 0) {
      return;
    }
    yield Uint8List.fromList(buffer.sublist(0, read));
  }
}

/// `read_pdf_bytes_bounded`: rejects a declared length that is zero or above
/// [maxBytes] before reading, then reads at most `maxBytes + 1` bytes and
/// requires the result to match the declared length exactly.
Uint8List readPdfBytesBoundedSync(
  Iterable<List<int>> reader,
  int declaredLength,
  int maxBytes,
) {
  if (declaredLength == 0 || declaredLength > maxBytes) {
    throw PrintFailure.failed('error.print.excelExportMalformedPdf');
  }
  final bytes = BytesBuilder(copy: false);
  final limit = maxBytes + 1;
  for (final chunk in reader) {
    final remaining = limit - bytes.length;
    if (remaining <= 0) {
      break;
    }
    if (chunk.length >= remaining) {
      bytes.add(chunk.sublist(0, remaining));
      break;
    }
    bytes.add(chunk);
  }
  final result = bytes.takeBytes();
  if (result.length != declaredLength || result.length > maxBytes) {
    throw PrintFailure.failed('error.print.excelExportMalformedPdf');
  }
  return result;
}

/// `file_signature_from_open_file` for an open capability handle.
FileSignature fileSignatureFromOpenHandle(
  OsHandle handle, {
  required bool includeSha256,
}) {
  final stat = handle.metadata();
  final digest = includeSha256 ? sha256OpenFile(handle) : null;
  handle.seek(0);
  return FileSignature(size: stat.size, mtimeNs: stat.mtimeNs, sha256: digest);
}
