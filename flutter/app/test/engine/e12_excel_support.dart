// Shared fixtures and fakes for the E12 Excel export pipeline tests.
// Port of the test module in src-tauri/src/platform/excel.rs.
import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:docflow/engine/cap_fs.dart';
import 'package:docflow/engine/journal.dart' show FilesystemObjectIdentity;
import 'package:docflow/engine/ooxml.dart';
import 'package:docflow/engine/platform/excel.dart';
import 'package:docflow/engine/platform/excel_print_ports.dart';
import 'package:docflow/engine/progress.dart' show CancellationToken;
import 'package:flutter_test/flutter_test.dart';

const String approvedWorkbookText = 'approved workbook bytes';

WorksheetDescriptor descriptor(String name, int workbookIndex) =>
    WorksheetDescriptor(
      workbookIndex: workbookIndex,
      sheetId: workbookIndex,
      relationshipId: 'rId$workbookIndex',
      relationshipType: 'worksheet',
      partPath: 'xl/worksheets/sheet$workbookIndex.xml',
      name: name,
      visibility: WorksheetVisibility.visible,
    );

WorkbookSafetyFacts safety() => const WorkbookSafetyFacts();

PrintJob worksheetJob(
  String sourcePath,
  FileSignature signature,
  String name,
  int workbookIndex, {
  int timeoutMinutes = 1,
}) => PrintJob(
  jobId: 'worksheet-$workbookIndex',
  path: sourcePath,
  printer: 'Office Printer',
  verb: 'docflow',
  format: 'XLSX',
  paper: 'A4',
  orientation: 'auto',
  scaling: 'fit',
  color: 'printer',
  duplex: 'printer',
  pageRange: 'all',
  timeoutMinutes: timeoutMinutes,
  signature: signature,
  worksheet: descriptor(name, workbookIndex),
  workbookSafety: safety(),
);

/// Fixture PDF with [pageCount] pages (`fixture_pdf_bytes`).
Uint8List fixturePdfBytes(int pageCount) {
  final pageIds = List<int>.generate(pageCount, (index) => index + 3);
  final objects = <String>[
    '<< /Type /Catalog /Pages 2 0 R >>',
    '<< /Type /Pages /Kids [${pageIds.map((id) => '$id 0 R').join(' ')}] /Count $pageCount >>',
    for (var _ in pageIds)
      '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 10 10] >>',
  ];
  final out = BytesBuilder();
  out.add(latin1.encode('%PDF-1.4\n'));
  final offsets = <int>[];
  for (var index = 0; index < objects.length; index++) {
    offsets.add(out.length);
    out.add(latin1.encode('${index + 1} 0 obj\n${objects[index]}\nendobj\n'));
  }
  final xref = out.length;
  out.add(latin1.encode('xref\n0 ${objects.length + 1}\n0000000000 65535 f \n'));
  for (final offset in offsets) {
    out.add(latin1.encode('${offset.toString().padLeft(10, '0')} 00000 n \n'));
  }
  out.add(
    latin1.encode(
      'trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n$xref\n%%EOF\n',
    ),
  );
  return out.takeBytes();
}

Uint8List multiPagePdf(int pageCount) => fixturePdfBytes(pageCount);

/// `fixture_exported_pdf`: writes a valid PDF and consumes the staged workbook.
ExportedWorksheetPdf fixtureExportedPdf(StagedWorkbook staged) {
  File(staged.pdfOutputPath()).writeAsBytesSync(fixturePdfBytes(2));
  return staged.intoExportedPdf();
}

/// Source fixture: a directory holding `book.xlsx` with approved bytes.
class SourceFixture {
  SourceFixture(this.directory, this.source, this.signature);

  final Directory directory;
  final AnchoredPath source;
  final FileSignature signature;

  OsHandle openSource() => source.openRead();

  void dispose() {
    if (directory.existsSync()) {
      directory.deleteSync(recursive: true);
    }
  }
}

SourceFixture sourceFixture() {
  final directory = Directory.systemTemp.createTempSync('docflow-excel-src-');
  final path = '${directory.path}/book.xlsx';
  File(path).writeAsBytesSync(utf8.encode(approvedWorkbookText));
  final root = CapabilityRoot.open(directory.path);
  final source = root.authorize(path).anchor();
  final handle = source.openRead();
  final FileSignature signature;
  try {
    signature = fileSignatureFromOpenHandle(handle, includeSha256: true);
  } finally {
    handle.close();
  }
  return SourceFixture(directory, source, signature);
}

class RequestObservation {
  RequestObservation({
    required this.requestId,
    required this.generation,
    required this.safety,
    required this.worksheet,
    required this.deadline,
    required this.workbookBytes,
    required this.cancelled,
  });

  final String requestId;
  final int generation;
  final WorkbookSafetyFacts safety;
  final WorksheetDescriptor worksheet;
  final Duration deadline;
  final List<int> workbookBytes;
  final bool cancelled;
}

class PolicyObservation {
  PolicyObservation({
    required this.absolute,
    required this.identity,
    required this.safety,
  });

  final String absolute;
  final FilesystemObjectIdentity identity;
  final WorkbookSafetyFacts safety;

  @override
  bool operator ==(Object other) =>
      other is PolicyObservation &&
      other.absolute == absolute &&
      other.identity == identity &&
      other.safety == safety;

  @override
  int get hashCode => Object.hash(absolute, identity, safety);
}

class SubmittedPdf {
  SubmittedPdf({
    required this.worksheet,
    required this.printer,
    required this.settings,
    required this.pageRange,
    required this.pdf,
    required this.cancellation,
    required this.deadline,
  });

  final String worksheet;
  final String printer;
  final PrintSettings settings;
  final String pageRange;
  final Uint8List pdf;
  final CancellationToken cancellation;
  final Duration deadline;
}

/// Policy that records calls. [events] collects "policy".
class RecordingPolicy implements WorkbookSourcePolicy {
  RecordingPolicy(this.events);

  final List<String> events;

  @override
  void validateSource(AnchoredPath source, WorkbookSafetyFacts safety) {
    events.add('policy');
  }
}

class CapturingPolicy implements WorkbookSourcePolicy {
  CapturingPolicy(this.events, this.observations);

  final List<String> events;
  final List<PolicyObservation> observations;

  @override
  void validateSource(AnchoredPath source, WorkbookSafetyFacts safety) {
    events.add('policy');
    observations.add(
      PolicyObservation(
        absolute: source.absolute,
        identity: source.identity(),
        safety: safety,
      ),
    );
  }
}

class RejectingPolicy implements WorkbookSourcePolicy {
  RejectingPolicy(this.events);

  final List<String> events;

  @override
  void validateSource(AnchoredPath source, WorkbookSafetyFacts safety) {
    events.add('policy');
    throw PrintFailure.unsupported('source policy rejected workbook');
  }
}

/// Exporter that records the request and returns a fixture PDF.
class RecordingExporter implements WorksheetPdfExporter {
  RecordingExporter(this.events, this.requests);

  final List<String> events;
  final List<RequestObservation> requests;

  @override
  Future<ExportedWorksheetPdf> exportPdf(ExcelExportRequest request) async {
    events.add('export');
    requests.add(
      RequestObservation(
        requestId: request.requestId,
        generation: request.generation,
        safety: request.safety,
        worksheet: request.worksheet,
        deadline: request.deadline,
        workbookBytes: File(request.stagedWorkbook.workbookPath).readAsBytesSync(),
        cancelled: request.cancellation.isCancelled,
      ),
    );
    return fixtureExportedPdf(request.stagedWorkbook);
  }
}

/// Exporter returning distinct PDFs per attempt.
class SequencedPdfExporter implements WorksheetPdfExporter {
  SequencedPdfExporter(this.events);

  final List<String> events;
  int attempts = 0;

  @override
  Future<ExportedWorksheetPdf> exportPdf(ExcelExportRequest request) async {
    events.add('export');
    attempts += 1;
    final bytes = BytesBuilder()
      ..add(multiPagePdf(2))
      ..add(utf8.encode('\n% attempt-$attempts\n'));
    File(request.stagedWorkbook.pdfOutputPath()).writeAsBytesSync(bytes.takeBytes());
    return request.stagedWorkbook.intoExportedPdf();
  }
}

/// Exporter that writes [bytes] as the PDF.
class BytesExporter implements WorksheetPdfExporter {
  BytesExporter(this.events, this.bytes);

  final List<String> events;
  final List<int> bytes;

  @override
  Future<ExportedWorksheetPdf> exportPdf(ExcelExportRequest request) async {
    events.add('export');
    File(request.stagedWorkbook.pdfOutputPath()).writeAsBytesSync(bytes);
    return request.stagedWorkbook.intoExportedPdf();
  }
}

/// Exporter that flags cleanup failure and returns a valid artifact.
class CleanupFailingExporter implements WorksheetPdfExporter {
  CleanupFailingExporter(this.events);

  final List<String> events;

  @override
  Future<ExportedWorksheetPdf> exportPdf(ExcelExportRequest request) async {
    events.add('export');
    request.stagedWorkbook.cleanupFailure = true;
    return fixtureExportedPdf(request.stagedWorkbook);
  }
}

/// Exporter that fails with cleanup failure armed on the staged workbook.
class ErrorWithCleanupFailureExporter implements WorksheetPdfExporter {
  ErrorWithCleanupFailureExporter(this.events);

  final List<String> events;

  @override
  Future<ExportedWorksheetPdf> exportPdf(ExcelExportRequest request) async {
    events.add('export');
    request.stagedWorkbook.cleanupFailure = true;
    throw PrintFailure.failed('fake export failed');
  }
}

/// Exporter that throws a non-PrintFailure error (a worker panic).
class PanickingExporter implements WorksheetPdfExporter {
  PanickingExporter(this.events, this.stagedPaths);

  final List<String> events;
  final List<String> stagedPaths;

  @override
  Future<ExportedWorksheetPdf> exportPdf(ExcelExportRequest request) async {
    events.add('export');
    stagedPaths.add(request.stagedWorkbook.workspacePath);
    throw StateError('fake Excel worker panic');
  }
}

/// Exporter that blocks until cancellation, then reports cancelled.
class CancellationBlockingExporter implements WorksheetPdfExporter {
  CancellationBlockingExporter(this.events, this.entered);

  final List<String> events;
  final Completer<void> entered;

  @override
  Future<ExportedWorksheetPdf> exportPdf(ExcelExportRequest request) async {
    events.add('export');
    entered.complete();
    while (!request.cancellation.isCancelled) {
      await Future<void>.delayed(const Duration(milliseconds: 10));
    }
    throw PrintFailure.cancelled('fake export cancelled', uncertain: false);
  }
}

class RecordingSubmitter implements ControlledPdfSubmitter {
  RecordingSubmitter(this.events, this.submitted);

  final List<String> events;
  final List<SubmittedPdf> submitted;

  @override
  Future<void> submitPdf(
    PrintJob job,
    ExportedWorksheetPdf pdf,
    CancellationToken cancellation,
    Duration deadline,
  ) async {
    if (cancellation.isCancelled) {
      throw StateError('submitter received a cancelled token');
    }
    expect(job.printer, 'Office Printer');
    expect(job.paper, 'A4');
    expect(job.orientation, 'auto');
    expect(job.scaling, 'fit');
    expect(job.color, 'printer');
    expect(job.duplex, 'printer');
    expect(job.pageRange, 'all');
    events.add('submit');
    submitted.add(
      SubmittedPdf(
        worksheet: job.worksheet!.name,
        printer: job.printer,
        settings: job.settings(),
        pageRange: job.pageRange,
        pdf: pdf.validatedBytes(),
        cancellation: cancellation,
        deadline: deadline,
      ),
    );
  }
}

class DeadlineRecordingSubmitter implements ControlledPdfSubmitter {
  DeadlineRecordingSubmitter(this.deadlines);

  final List<Duration> deadlines;

  @override
  Future<void> submitPdf(
    PrintJob job,
    ExportedWorksheetPdf pdf,
    CancellationToken cancellation,
    Duration deadline,
  ) async {
    pdf.validatedBytes();
    deadlines.add(deadline);
  }
}

/// Submitter whose outcome is fixed: null means success.
class OutcomeSubmitter implements ControlledPdfSubmitter {
  OutcomeSubmitter(this.events, this.outcome);

  final List<String> events;
  final PrintFailure? outcome;

  @override
  Future<void> submitPdf(
    PrintJob job,
    ExportedWorksheetPdf pdf,
    CancellationToken cancellation,
    Duration deadline,
  ) async {
    events.add('submit');
    if (outcome != null) {
      throw outcome!;
    }
  }
}

/// Submitter that fails its first attempt before effect, then succeeds.
class FailOnceCapturingSubmitter implements ControlledPdfSubmitter {
  FailOnceCapturingSubmitter(this.events, this.submitted);

  final List<String> events;
  final List<List<int>> submitted;
  int attempts = 0;

  @override
  Future<void> submitPdf(
    PrintJob job,
    ExportedWorksheetPdf pdf,
    CancellationToken cancellation,
    Duration deadline,
  ) async {
    events.add('submit');
    submitted.add(pdf.validatedBytes());
    attempts += 1;
    if (attempts == 1) {
      throw PrintFailure.failed('error.print.syntheticPreEffectFailure');
    }
  }
}

class FixedClock implements MonotonicClock {
  FixedClock(this.instant);

  final Duration instant;

  @override
  Duration now() => instant;
}

/// Clock that jumps forward on [advance] and ticks one second per read after.
class AdvancingClock implements MonotonicClock {
  Duration _now = Duration.zero;
  bool _ticking = false;

  void advance() {
    _now += const Duration(seconds: 61);
    _ticking = true;
  }

  @override
  Duration now() {
    final current = _now;
    if (_ticking) {
      _now += const Duration(seconds: 1);
    }
    return current;
  }
}

/// Clock whose reads cross the deadline after nine calls.
class DeadlineCrossingClock implements MonotonicClock {
  int calls = 0;

  @override
  Duration now() {
    final current = calls >= 9 ? const Duration(seconds: 61) : Duration.zero;
    calls += 1;
    return current;
  }
}

/// Worker whose response is delivered immediately, with a fixed join status.
class ImmediateWorker implements ExcelWorkerHandle {
  ImmediateWorker(this._response, {this.join = WorkerJoin.finished});

  WorkerResponse? _response;
  final WorkerJoin join;

  @override
  Future<WorkerPoll> poll(Duration wait) async {
    final response = _response;
    _response = null;
    return response == null
        ? const WorkerPollDisconnected()
        : WorkerPollResponse(response);
  }

  @override
  WorkerJoin joinStatus() => join;
}

/// Runtime that runs the exporter at launch and returns its response.
class ImmediateRuntime implements ExcelWorkerRuntime {
  ImmediateRuntime({this.join = WorkerJoin.finished, this.generationDelta = 0,
      this.beforeLaunch, this.afterExport});

  final WorkerJoin join;
  final int generationDelta;
  final void Function()? beforeLaunch;
  final void Function(ExcelExportRequest request)? afterExport;

  @override
  ExcelWorkerHandle launch(
    WorksheetPdfExporter exporter,
    ExcelExportRequest request,
  ) {
    beforeLaunch?.call();
    final requestId = request.requestId;
    final generation = request.generation;
    final future = _runCapturing(exporter, request);
    return _DeferredWorker(
      future.then(
        (result) => WorkerResponse(
          requestId: requestId,
          generation: generation + generationDelta,
          artifact: result.$1,
          failure: result.$2,
        ),
      ),
      join,
      () => afterExport?.call(request),
    );
  }
}

Future<(ExportedWorksheetPdf?, PrintFailure?)> _runCapturing(
  WorksheetPdfExporter exporter,
  ExcelExportRequest request,
) async {
  try {
    final artifact = await exporter.exportPdf(request);
    return (artifact, null);
  } on PrintFailure catch (failure) {
    return (null, failure);
  } finally {
    request.stagedWorkbook.dropIfNotConsumed();
  }
}

class _DeferredWorker implements ExcelWorkerHandle {
  _DeferredWorker(this._response, this._join, this._after);

  final Future<WorkerResponse> _response;
  final WorkerJoin _join;
  final void Function() _after;
  bool _delivered = false;

  @override
  Future<WorkerPoll> poll(Duration wait) async {
    if (_delivered) {
      return const WorkerPollDisconnected();
    }
    _delivered = true;
    final response = await _response;
    _after();
    return WorkerPollResponse(response);
  }

  @override
  WorkerJoin joinStatus() => _join;
}

/// Runtime whose worker never responds and never joins; the clock advances
/// on launch so the pipeline times out.
class StuckRuntime implements ExcelWorkerRuntime {
  StuckRuntime(this.clock);

  final AdvancingClock clock;

  @override
  ExcelWorkerHandle launch(
    WorksheetPdfExporter exporter,
    ExcelExportRequest request,
  ) {
    clock.advance();
    return _StuckWorker();
  }
}

class _StuckWorker implements ExcelWorkerHandle {
  @override
  Future<WorkerPoll> poll(Duration wait) async => const WorkerPollPending();

  @override
  WorkerJoin joinStatus() => WorkerJoin.pending;
}

/// Runtime that always fails to launch, counting calls.
class LaunchCountingRuntime implements ExcelWorkerRuntime {
  LaunchCountingRuntime(this.launches);

  final List<int> launches;

  @override
  ExcelWorkerHandle launch(
    WorksheetPdfExporter exporter,
    ExcelExportRequest request,
  ) {
    launches.add(1);
    throw PrintFailure.failed('worker must not launch');
  }
}

