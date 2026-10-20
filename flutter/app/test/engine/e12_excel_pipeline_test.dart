// Port of the export_supervisor tests in src-tauri/src/platform/excel.rs that
// exercise ExcelExportPipeline without the print queue (PrintService-backed
// tests are ported separately with the E11 print queue).
import 'dart:convert';
import 'dart:io';
import 'dart:collection';
import 'dart:typed_data';

import 'package:docflow/engine/cap_fs.dart';
import 'package:docflow/engine/ooxml.dart';
import 'package:docflow/engine/platform/excel.dart';
import 'package:docflow/engine/platform/excel_print_ports.dart';
import 'package:docflow/engine/platform/excel_windows_contract.dart';
import 'package:docflow/engine/progress.dart';
import 'package:flutter_test/flutter_test.dart';

import 'e12_excel_support.dart';

ExcelExportPipeline pipelineFor({
  required WorkbookSourcePolicy policy,
  required WorksheetPdfExporter exporter,
  required ControlledPdfSubmitter submitter,
  MonotonicClock? clock,
  ExcelWorkerRuntime? runtime,
  Duration teardownGrace = const Duration(seconds: 1),
}) => ExcelExportPipeline(
  policy: policy,
  exporter: exporter,
  submitter: submitter,
  guards: ExcelProcessGuards(),
  clock: clock,
  runtime: runtime,
  teardownGrace: teardownGrace,
);

Future<PrintFailure> captureFailureAsync(
  Future<Object?> Function() body,
) async {
  try {
    await body();
  } on PrintFailure catch (failure) {
    return failure;
  }
  throw StateError('expected a PrintFailure');
}

void main() {
  test('exports enabled worksheets in exact order', () async {
    final events = <String>[];
    final requests = <RequestObservation>[];
    final submitted = <SubmittedPdf>[];
    final pipeline = pipelineFor(
      policy: RecordingPolicy(events),
      exporter: RecordingExporter(events, requests),
      submitter: RecordingSubmitter(events, submitted),
    );
    final cancellation = CancellationRegistry().register('plan');
    final fixture = sourceFixture();
    addTearDown(fixture.dispose);

    for (final (name, index) in [('CIV', 2), ('MEC', 1)]) {
      await pipeline.exportAndSubmit(
        worksheetJob(fixture.source.absolute, fixture.signature, name, index),
        fixture.source,
        fixture.openSource(),
        cancellation,
      );
    }

    expect(events, [
      'policy',
      'export',
      'submit',
      'policy',
      'export',
      'submit',
    ]);
    expect(requests.length, 2);
    expect(requests[0].requestId != requests[1].requestId, isTrue);
    expect(requests[0].generation, 1);
    expect(requests[1].generation, 2);
    expect(requests[0].safety, safety());
    expect(requests[0].worksheet, descriptor('CIV', 2));
    expect(requests[0].workbookBytes, utf8.encode(approvedWorkbookText));
    expect(requests[0].cancelled, isFalse);
    final expectedPdf = multiPagePdf(2);
    expect(submitted.length, 2);
    for (var index = 0; index < submitted.length; index++) {
      final submission = submitted[index];
      final expectedName = ['CIV', 'MEC'][index];
      expect(submission.worksheet, expectedName);
      expect(submission.printer, 'Office Printer');
      expect(submission.settings.paper, 'A4');
      expect(submission.settings.orientation, 'auto');
      expect(submission.settings.scaling, 'fit');
      expect(submission.settings.color, 'printer');
      expect(submission.settings.duplex, 'printer');
      expect(submission.settings.pageRange, 'all');
      expect(submission.settings.timeoutMinutes, 1);
      expect(submission.pageRange, 'all');
      expect(submission.pdf, expectedPdf);
      expect(identical(submission.cancellation, cancellation), isTrue);
      expect(submission.deadline, requests[index].deadline);
    }
  });

  test('submitter receives the exact export request deadline', () async {
    final base = const Duration(seconds: 5);
    final events = <String>[];
    final requests = <RequestObservation>[];
    final deadlines = <Duration>[];
    final pipeline = pipelineFor(
      policy: RecordingPolicy(events),
      exporter: RecordingExporter(events, requests),
      submitter: DeadlineRecordingSubmitter(deadlines),
      clock: FixedClock(base),
      runtime: ImmediateRuntime(),
    );
    final fixture = sourceFixture();
    addTearDown(fixture.dispose);
    final job = worksheetJob(
      fixture.source.absolute,
      fixture.signature,
      'MEC',
      1,
    );

    await pipeline.exportAndSubmit(
      job,
      fixture.source,
      fixture.openSource(),
      CancellationRegistry().register('deadline'),
    );

    final expected = base + const Duration(seconds: 60);
    expect(requests[0].deadline, expected);
    expect(deadlines, [expected]);
  });

  test(
    'source policy receives exact anchored identity and safety facts',
    () async {
      final events = <String>[];
      final observations = <PolicyObservation>[];
      final pipeline = pipelineFor(
        policy: CapturingPolicy(events, observations),
        exporter: RecordingExporter(events, []),
        submitter: RecordingSubmitter(events, []),
      );
      final fixture = sourceFixture();
      addTearDown(fixture.dispose);
      final expectedSafety = WorkbookSafetyFacts(hasVbaProject: true);
      final job = PrintJob(
        jobId: 'worksheet-1',
        path: fixture.source.absolute,
        printer: 'Office Printer',
        verb: 'docflow',
        format: 'XLSX',
        paper: 'A4',
        orientation: 'auto',
        scaling: 'fit',
        color: 'printer',
        duplex: 'printer',
        pageRange: 'all',
        timeoutMinutes: 1,
        signature: fixture.signature,
        worksheet: descriptor('MEC', 1),
        workbookSafety: expectedSafety,
      );

      await pipeline.exportAndSubmit(
        job,
        fixture.source,
        fixture.openSource(),
        CancellationRegistry().register('plan'),
      );

      expect(observations, [
        PolicyObservation(
          absolute: fixture.source.absolute,
          identity: fixture.source.identity(),
          safety: expectedSafety,
        ),
      ]);
      expect(events, ['policy', 'export', 'submit']);
    },
  );

  test(
    'cancellation before export has zero policy, export and submit calls',
    () async {
      final events = <String>[];
      final requests = <RequestObservation>[];
      final submitted = <SubmittedPdf>[];
      final pipeline = pipelineFor(
        policy: RecordingPolicy(events),
        exporter: RecordingExporter(events, requests),
        submitter: RecordingSubmitter(events, submitted),
      );
      final cancellations = CancellationRegistry();
      final cancellation = cancellations.register('cancel-before-export');
      expect(cancellations.cancel('cancel-before-export'), isTrue);
      final fixture = sourceFixture();
      addTearDown(fixture.dispose);
      final job = worksheetJob(
        fixture.source.absolute,
        fixture.signature,
        'MEC',
        1,
      );

      final failure = await captureFailureAsync(
        () => pipeline.exportAndSubmit(
          job,
          fixture.source,
          fixture.openSource(),
          cancellation,
        ),
      );

      expect(failure.kind, PrintFailureKind.cancelled);
      expect(failure.message, 'error.print.excelExportCancelled');
      expect(failure.uncertain, isFalse);
      expect(events, isEmpty);
      expect(requests, isEmpty);
      expect(submitted, isEmpty);
    },
  );

  test('source policy rejection precedes staging and worker launch', () async {
    final events = <String>[];
    final requests = <RequestObservation>[];
    final submitted = <SubmittedPdf>[];
    final pipeline = pipelineFor(
      policy: RejectingPolicy(events),
      exporter: RecordingExporter(events, requests),
      submitter: RecordingSubmitter(events, submitted),
    );
    final fixture = sourceFixture();
    addTearDown(fixture.dispose);
    final job = worksheetJob(
      fixture.source.absolute,
      fixture.signature,
      'MEC',
      1,
    );

    final failure = await captureFailureAsync(
      () => pipeline.exportAndSubmit(
        job,
        fixture.source,
        fixture.openSource(),
        CancellationRegistry().register('plan'),
      ),
    );

    expect(failure.kind, PrintFailureKind.unsupported);
    expect(events, ['policy']);
    expect(requests, isEmpty);
    expect(submitted, isEmpty);
  });

  test(
    'internet-zone policy rejection has zero staging, worker and submit calls',
    () async {
      final events = <String>[];
      final requests = <RequestObservation>[];
      final submitted = <SubmittedPdf>[];
      final launches = <int>[];
      final pipeline = pipelineFor(
        policy: AnchoredWorkbookSourcePolicy(_InternetZoneReader()),
        exporter: RecordingExporter(events, requests),
        submitter: RecordingSubmitter(events, submitted),
        runtime: LaunchCountingRuntime(launches),
        clock: SystemMonotonicClock(),
        teardownGrace: const Duration(seconds: 2),
      );
      final fixture = sourceFixture();
      addTearDown(fixture.dispose);
      final job = worksheetJob(
        fixture.source.absolute,
        fixture.signature,
        'MEC',
        1,
      );

      final failure = await captureFailureAsync(
        () => pipeline.exportAndSubmit(
          job,
          fixture.source,
          fixture.openSource(),
          CancellationRegistry().register('plan'),
        ),
      );

      expect(failure.message, 'error.print.excelSourceMotwBlocked');
      expect(launches, isEmpty);
      expect(events, isEmpty);
      expect(requests, isEmpty);
      expect(submitted, isEmpty);
    },
  );

  test(
    'non-all range is rejected before policy, staging or worker launch',
    () async {
      final events = <String>[];
      final requests = <RequestObservation>[];
      final submitted = <SubmittedPdf>[];
      final pipeline = pipelineFor(
        policy: RecordingPolicy(events),
        exporter: RecordingExporter(events, requests),
        submitter: RecordingSubmitter(events, submitted),
      );
      final fixture = sourceFixture();
      addTearDown(fixture.dispose);
      final base = worksheetJob(
        fixture.source.absolute,
        fixture.signature,
        'MEC',
        1,
      );
      final job = base.copyWith(pageRange: '1-2');

      final failure = await captureFailureAsync(
        () => pipeline.exportAndSubmit(
          job,
          fixture.source,
          fixture.openSource(),
          CancellationRegistry().register('plan'),
        ),
      );

      expect(failure.kind, PrintFailureKind.unsupportedOptions);
      expect(events, isEmpty);
      expect(requests, isEmpty);
      expect(submitted, isEmpty);
    },
  );

  test('empty or malformed PDF never reaches the submitter', () async {
    for (final bytes in [<int>[], utf8.encode('%PDF-1.4\nmalformed')]) {
      final events = <String>[];
      final submitted = <SubmittedPdf>[];
      final pipeline = pipelineFor(
        policy: RecordingPolicy(events),
        exporter: BytesExporter(events, bytes),
        submitter: RecordingSubmitter(events, submitted),
      );
      final fixture = sourceFixture();
      final job = worksheetJob(
        fixture.source.absolute,
        fixture.signature,
        'MEC',
        1,
      );

      final failure = await captureFailureAsync(
        () => pipeline.exportAndSubmit(
          job,
          fixture.source,
          fixture.openSource(),
          CancellationRegistry().register('invalid-pdf'),
        ),
      );

      expect(failure.kind, PrintFailureKind.failed);
      expect(failure.uncertain, isFalse);
      expect(submitted, isEmpty);
      expect(events, ['policy', 'export']);
      fixture.dispose();
    }
  });

  test(
    'a valid Excel-compatible PDF envelope is accepted and submitted',
    () async {
      final events = <String>[];
      final submitted = <SubmittedPdf>[];
      final pipeline = pipelineFor(
        policy: RecordingPolicy(events),
        exporter: BytesExporter(
          events,
          utf8.encode(
            '%PDF-1.7\nExcel-compatible objects\nstartxref\n0\n%%EOF\r\n',
          ),
        ),
        submitter: RecordingSubmitter(events, submitted),
      );
      final fixture = sourceFixture();
      addTearDown(fixture.dispose);
      final job = worksheetJob(
        fixture.source.absolute,
        fixture.signature,
        'MEC',
        1,
      );

      await pipeline.exportAndSubmit(
        job,
        fixture.source,
        fixture.openSource(),
        CancellationRegistry().register('platform-pdf-decoder'),
      );

      expect(events, ['policy', 'export', 'submit']);
      expect(submitted.length, 1);
    },
  );

  test('artifact drop closes anchors and root before workspace cleanup', () {
    final (staged, events) = StagedWorkbook.fixtureWithDropProbe();
    final exported = fixtureExportedPdf(staged);

    exported.cleanup();

    expect(events, ['pdf', 'workbook', 'root', 'workspace']);
  });

  test('exporter teardown failure marker trips the breaker', () {
    final (staged, _) = StagedWorkbook.fixtureWithDropProbe();
    expect(staged.cleanupBreaker.value, isFalse);
    staged.quarantineAfterTeardownFailure();
    expect(staged.cleanupBreaker.value, isTrue);
    staged.cleanupQuietly();
  });

  test('cleanup failure retains a process-owned workspace locator', () {
    final workspace = Directory.systemTemp.createTempSync(
      'docflow-excel-retain-',
    );
    final root = CapabilityRoot.open(workspace.path);
    final workbookPath = '${workspace.path}/book.xlsx';
    final workbook = root.authorize(workbookPath).anchor();
    workbook.writeNoClobber(utf8.encode('private workbook'));
    final breaker = ExcelFlag();
    final quarantine = SplayTreeSet<String>();
    final staged = StagedWorkbook.fixture(
      workspacePath: workspace.path,
      workbook: workbook,
      root: root,
      workbookPath: workbookPath,
      breaker: breaker,
      quarantine: quarantine,
      cleanupFailure: true,
    );

    expect(() => staged.cleanup(), throwsA(isA<FileSystemException>()));

    expect(breaker.value, isTrue);
    expect(quarantine.toList(), [workspace.path]);
    expect(File(workbookPath).readAsStringSync(), 'private workbook');
    workspace.deleteSync(recursive: true);
    quarantine.clear();
  });

  test('generation mismatch destroys the PDF without submit', () async {
    final events = <String>[];
    final requests = <RequestObservation>[];
    final submitted = <SubmittedPdf>[];
    final pipeline = pipelineFor(
      policy: RecordingPolicy(events),
      exporter: RecordingExporter(events, requests),
      submitter: RecordingSubmitter(events, submitted),
      clock: FixedClock(Duration.zero),
      runtime: ImmediateRuntime(generationDelta: 1),
    );
    final fixture = sourceFixture();
    addTearDown(fixture.dispose);
    final job = worksheetJob(
      fixture.source.absolute,
      fixture.signature,
      'MEC',
      1,
    );

    final failure = await captureFailureAsync(
      () => pipeline.exportAndSubmit(
        job,
        fixture.source,
        fixture.openSource(),
        CancellationRegistry().register('plan'),
      ),
    );

    expect(failure.kind, PrintFailureKind.failed);
    expect(failure.uncertain, isFalse);
    expect(submitted, isEmpty);
    expect(events, ['policy', 'export']);
  });

  test(
    'deadline crossing during source revalidation prevents submit',
    () async {
      final events = <String>[];
      final requests = <RequestObservation>[];
      final submitted = <SubmittedPdf>[];
      final pipeline = pipelineFor(
        policy: RecordingPolicy(events),
        exporter: RecordingExporter(events, requests),
        submitter: RecordingSubmitter(events, submitted),
        clock: DeadlineCrossingClock(),
        runtime: ImmediateRuntime(),
      );
      final fixture = sourceFixture();
      addTearDown(fixture.dispose);
      final job = worksheetJob(
        fixture.source.absolute,
        fixture.signature,
        'MEC',
        1,
      );

      final failure = await captureFailureAsync(
        () => pipeline.exportAndSubmit(
          job,
          fixture.source,
          fixture.openSource(),
          CancellationRegistry().register('plan'),
        ),
      );

      expect(failure.kind, PrintFailureKind.failed);
      expect(failure.message, 'error.print.excelExportTimedOut');
      expect(failure.uncertain, isFalse);
      expect(events, ['policy', 'export']);
      expect(submitted, isEmpty);
    },
  );

  test(
    'worker join failure after response trips breaker without submit',
    () async {
      final events = <String>[];
      final requests = <RequestObservation>[];
      final submitted = <SubmittedPdf>[];
      final pipeline = pipelineFor(
        policy: RecordingPolicy(events),
        exporter: RecordingExporter(events, requests),
        submitter: RecordingSubmitter(events, submitted),
        clock: FixedClock(Duration.zero),
        runtime: ImmediateRuntime(join: WorkerJoin.failed),
      );
      final fixture = sourceFixture();
      addTearDown(fixture.dispose);
      final job = worksheetJob(
        fixture.source.absolute,
        fixture.signature,
        'MEC',
        1,
      );
      final cancellation = CancellationRegistry().register('plan');

      final first = await captureFailureAsync(
        () => pipeline.exportAndSubmit(
          job,
          fixture.source,
          fixture.openSource(),
          cancellation,
        ),
      );
      final second = await captureFailureAsync(
        () => pipeline.exportAndSubmit(
          job,
          fixture.source,
          fixture.openSource(),
          cancellation,
        ),
      );

      expect(first.kind, PrintFailureKind.unsupported);
      expect(second.kind, PrintFailureKind.unsupported);
      expect(events, ['policy', 'export']);
      expect(submitted, isEmpty);
      pipeline.clearTestQuarantine();
    },
  );

  test(
    'real worker panic disconnects, cleans staging and trips breaker',
    () async {
      final events = <String>[];
      final stagedPaths = <String>[];
      final submitted = <SubmittedPdf>[];
      final pipeline = pipelineFor(
        policy: RecordingPolicy(events),
        exporter: PanickingExporter(events, stagedPaths),
        submitter: RecordingSubmitter(events, submitted),
      );
      final fixture = sourceFixture();
      addTearDown(fixture.dispose);
      final job = worksheetJob(
        fixture.source.absolute,
        fixture.signature,
        'MEC',
        1,
      );
      final cancellation = CancellationRegistry().register('plan');

      final first = await captureFailureAsync(
        () => pipeline.exportAndSubmit(
          job,
          fixture.source,
          fixture.openSource(),
          cancellation,
        ),
      );
      final second = await captureFailureAsync(
        () => pipeline.exportAndSubmit(
          job,
          fixture.source,
          fixture.openSource(),
          cancellation,
        ),
      );

      expect(first.kind, PrintFailureKind.failed);
      expect(first.message, 'error.print.excelWorkerStopped');
      expect(second.kind, PrintFailureKind.unsupported);
      expect(events, ['policy', 'export']);
      expect(submitted, isEmpty);
      expect(stagedPaths.length, 1);
      expect(Directory(stagedPaths[0]).existsSync(), isFalse);
      pipeline.clearTestQuarantine();
    },
  );

  test(
    'pre-submit cleanup failure preserves primary failure and trips breaker',
    () async {
      final events = <String>[];
      final submitted = <SubmittedPdf>[];
      final pipeline = pipelineFor(
        policy: RecordingPolicy(events),
        exporter: CleanupFailingExporter(events),
        submitter: RecordingSubmitter(events, submitted),
        clock: FixedClock(Duration.zero),
        runtime: ImmediateRuntime(generationDelta: 1),
      );
      final fixture = sourceFixture();
      addTearDown(fixture.dispose);
      final job = worksheetJob(
        fixture.source.absolute,
        fixture.signature,
        'MEC',
        1,
      );
      final cancellation = CancellationRegistry().register('plan');

      final first = await captureFailureAsync(
        () => pipeline.exportAndSubmit(
          job,
          fixture.source,
          fixture.openSource(),
          cancellation,
        ),
      );
      final cleanup = first.cleanupReview!;
      expect(first.kind, PrintFailureKind.failed);
      expect(first.message, 'error.print.excelResponseStale');
      expect(first.uncertain, isFalse);
      expect(cleanup.warningKey, 'error.print.exportCleanupRequiresReview');
      expect(cleanup.locator.startsWith('docflow-excel-'), isTrue);

      final second = await captureFailureAsync(
        () => pipeline.exportAndSubmit(
          job,
          fixture.source,
          fixture.openSource(),
          cancellation,
        ),
      );
      expect(second.kind, PrintFailureKind.unsupported);
      expect(second.message, 'error.print.excelExportQuarantined');
      expect(second.cleanupReview, isNull);
      expect(events, ['policy', 'export']);
      expect(submitted, isEmpty);
      pipeline.clearTestQuarantine();
    },
  );

  test(
    'exporter error with cleanup failure trips breaker before retry',
    () async {
      final events = <String>[];
      final submitted = <SubmittedPdf>[];
      final pipeline = pipelineFor(
        policy: RecordingPolicy(events),
        exporter: ErrorWithCleanupFailureExporter(events),
        submitter: RecordingSubmitter(events, submitted),
      );
      final fixture = sourceFixture();
      addTearDown(fixture.dispose);
      final job = worksheetJob(
        fixture.source.absolute,
        fixture.signature,
        'MEC',
        1,
      );
      final cancellation = CancellationRegistry().register('plan');

      final first = await captureFailureAsync(
        () => pipeline.exportAndSubmit(
          job,
          fixture.source,
          fixture.openSource(),
          cancellation,
        ),
      );
      expect(first.kind, PrintFailureKind.failed);
      expect(first.message, 'fake export failed');
      expect(first.uncertain, isFalse);
      expect(
        first.cleanupReview!.warningKey,
        'error.print.exportCleanupRequiresReview',
      );
      expect(first.cleanupReview!.locator.startsWith('docflow-excel-'), isTrue);

      final second = await captureFailureAsync(
        () => pipeline.exportAndSubmit(
          job,
          fixture.source,
          fixture.openSource(),
          cancellation,
        ),
      );
      expect(second.kind, PrintFailureKind.unsupported);
      expect(events, ['policy', 'export']);
      expect(submitted, isEmpty);
      pipeline.clearTestQuarantine();
    },
  );

  test('post-submit cleanup failure completes with a non-sensitive warning locator', () async {
    final events = <String>[];
    final pipeline = pipelineFor(
      policy: RecordingPolicy(events),
      exporter: CleanupFailingExporter(events),
      submitter: OutcomeSubmitter(events, null),
    );
    final fixture = sourceFixture();
    addTearDown(fixture.dispose);
    final job = worksheetJob(
      fixture.source.absolute,
      fixture.signature,
      'MEC',
      1,
    );

    final outcome = await pipeline.exportAndSubmit(
      job,
      fixture.source,
      fixture.openSource(),
      CancellationRegistry().register('plan'),
    );
    final review = outcome.cleanupReview!;

    expect(review.warningKey, 'error.print.exportCleanupRequiresReview');
    expect(review.locator.startsWith('docflow-excel-'), isTrue);
    expect(review.locator.contains(Platform.pathSeparator), isFalse);
    expect(events, ['policy', 'export', 'submit']);
    pipeline.clearTestQuarantine();
  });

  test(
    'post-submit cleanup failure preserves renderer failure classification',
    () async {
      for (final renderer in [
        PrintFailure.uncertain('printer outcome unknown'),
        PrintFailure.cancelled('cancel outcome unknown', uncertain: true),
        PrintFailure.failed('printer rejected before effect'),
      ]) {
        final events = <String>[];
        final pipeline = pipelineFor(
          policy: RecordingPolicy(events),
          exporter: CleanupFailingExporter(events),
          submitter: OutcomeSubmitter(events, renderer),
        );
        final fixture = sourceFixture();
        final job = worksheetJob(
          fixture.source.absolute,
          fixture.signature,
          'MEC',
          1,
        );

        final actual = await captureFailureAsync(
          () => pipeline.exportAndSubmit(
            job,
            fixture.source,
            fixture.openSource(),
            CancellationRegistry().register('plan'),
          ),
        );
        final review = actual.cleanupReview!;

        expect(actual.kind, renderer.kind);
        expect(actual.message, renderer.message);
        expect(actual.uncertain, renderer.uncertain);
        expect(review.warningKey, 'error.print.exportCleanupRequiresReview');
        expect(review.locator.startsWith('docflow-excel-'), isTrue);
        expect(events, ['policy', 'export', 'submit']);
        pipeline.clearTestQuarantine();
        fixture.dispose();
      }
    },
  );

  test('source replacement after export blocks all submission', () async {
    final events = <String>[];
    final submitted = <SubmittedPdf>[];
    final fixture = sourceFixture();
    addTearDown(fixture.dispose);
    final pipeline = pipelineFor(
      policy: RecordingPolicy(events),
      exporter: _SourceReplacingExporter(events, fixture.source.absolute),
      submitter: RecordingSubmitter(events, submitted),
    );
    final job = worksheetJob(
      fixture.source.absolute,
      fixture.signature,
      'MEC',
      1,
    );

    final failure = await captureFailureAsync(
      () => pipeline.exportAndSubmit(
        job,
        fixture.source,
        fixture.openSource(),
        CancellationRegistry().register('source-swap'),
      ),
    );

    expect(failure.message, 'error.print.workbookChanged');
    expect(events, ['policy', 'export']);
    expect(submitted, isEmpty);
    pipeline.clearTestQuarantine();
  });

  test('oversized PDF is rejected before the reader is touched', () {
    var touched = false;
    Iterable<List<int>> reader() sync* {
      touched = true;
      yield [0];
    }

    final failure = _captureSyncFailure(
      () => readPdfBytesBoundedSync(reader(), 65, 64),
    );

    expect(failure.message, 'error.print.excelExportMalformedPdf');
    expect(touched, isFalse);
  });

  test('concurrently growing PDF is rejected by the bounded read', () {
    final failure = _captureSyncFailure(
      () => readPdfBytesBoundedSync(
        [Uint8List.fromList(List<int>.filled(65, 0x78))],
        64,
        64,
      ),
    );
    expect(failure.message, 'error.print.excelExportMalformedPdf');
  });

  test('validated PDF bytes survive visible leaf substitution', () {
    final (staged, _) = StagedWorkbook.fixtureWithDropProbe();
    final expected = multiPagePdf(2);
    final pdfPath = staged.pdfOutputPath();
    File(pdfPath).writeAsBytesSync(expected);
    final exported = staged.intoExportedPdf();

    exported.validateForSubmission();
    final displaced = '$pdfPath.validated';
    File(pdfPath).renameSync(displaced);
    File(pdfPath).writeAsBytesSync(utf8.encode('%PDF-1.4\nsubstituted'));

    expect(exported.validatedBytes(), expected);
    expect(File(pdfPath).readAsBytesSync() == expected, isFalse);
    File(pdfPath).deleteSync();
    exported.cleanupQuietly();
  });

  test('exported PDF rejects visible leaf substitution before validation', () {
    final (staged, _) = StagedWorkbook.fixtureWithDropProbe();
    final pdfPath = staged.pdfOutputPath();
    File(pdfPath).writeAsBytesSync(multiPagePdf(2));
    final exported = staged.intoExportedPdf();

    File(pdfPath).renameSync('$pdfPath.original');
    File(pdfPath).writeAsBytesSync(multiPagePdf(1));

    final failure = _captureSyncFailure(exported.validateForSubmission);
    expect(failure.kind, PrintFailureKind.failed);
    expect(failure.uncertain, isFalse);
    expect(() => exported.validatedBytes(), throwsA(isA<PrintFailure>()));
    File(pdfPath).deleteSync();
    exported.cleanupQuietly();
  });

  group('cancel and timeout (export_supervisor)', () {
    ExcelExportPipeline guardedPipeline({
      required List<String> events,
      required List<RequestObservation> requests,
      required List<SubmittedPdf> submitted,
      required WorksheetPdfExporter exporter,
      required ExcelWorkerRuntime runtime,
      ExcelProcessGuards? guards,
      MonotonicClock? clock,
    }) => ExcelExportPipeline(
      policy: RecordingPolicy(events),
      exporter: exporter,
      submitter: RecordingSubmitter(events, submitted),
      guards: guards ?? ExcelProcessGuards(),
      clock: clock,
      runtime: runtime,
      teardownGrace: const Duration(seconds: 1),
    );

    void expectCleanupReview(
      ExcelExportPipeline pipeline,
      ExcelProcessGuards guards,
      PrintFailure failure,
      PrintFailureKind kind,
      String message,
      List<String> events,
      List<SubmittedPdf> submitted,
    ) {
      expect(failure.kind, kind);
      expect(failure.message, message);
      expect(failure.uncertain, isFalse);
      final review = failure.cleanupReview!;
      expect(review.warningKey, 'error.print.exportCleanupRequiresReview');
      expect(review.locator, startsWith('docflow-excel-'));
      expect(pipeline.breakerTripped, isTrue);
      expect(guards.quarantine, hasLength(1));
      expect(Directory(guards.quarantine.single).existsSync(), isTrue);
      expect(events, ['policy', 'export']);
      expect(submitted, isEmpty);
      for (final path in guards.quarantine) {
        Directory(path).deleteSync(recursive: true);
      }
      guards.quarantine.clear();
    }

    test(
      'error response cleanup failure attaches review to cancelled primary',
      () async {
        final events = <String>[];
        final submitted = <SubmittedPdf>[];
        final registry = CancellationRegistry();
        final token = registry.register('cleanup-error-failed-join');
        final guards = ExcelProcessGuards();
        final pipeline = guardedPipeline(
          events: events,
          requests: [],
          submitted: submitted,
          exporter: ErrorWithCleanupFailureExporter(events),
          runtime: ImmediateRuntime(
            join: WorkerJoin.failed,
            afterExport: (_) => registry.cancel('cleanup-error-failed-join'),
          ),
          guards: guards,
        );
        final fixture = sourceFixture();
        addTearDown(fixture.dispose);
        final job = worksheetJob(
          fixture.source.absolute,
          fixture.signature,
          'MEC',
          1,
        );
        final failure = await captureFailureAsync(
          () => pipeline.exportAndSubmit(
            job,
            fixture.source,
            fixture.openSource(),
            token,
          ),
        );
        expectCleanupReview(
          pipeline,
          guards,
          failure,
          PrintFailureKind.cancelled,
          'error.print.excelExportCancelled',
          events,
          submitted,
        );
      },
    );

    test('received response cleanup failure attaches review to quarantined primary', () async {
      final events = <String>[];
      final submitted = <SubmittedPdf>[];
      final guards = ExcelProcessGuards();
      final pipeline = guardedPipeline(
        events: events,
        requests: [],
        submitted: submitted,
        exporter: CleanupFailingExporter(events),
        runtime: ImmediateRuntime(join: WorkerJoin.failed),
        guards: guards,
      );
      final fixture = sourceFixture();
      addTearDown(fixture.dispose);
      final job = worksheetJob(
        fixture.source.absolute,
        fixture.signature,
        'MEC',
        1,
      );
      final failure = await captureFailureAsync(
        () => pipeline.exportAndSubmit(
          job,
          fixture.source,
          fixture.openSource(),
          CancellationRegistry().register('cleanup-response-join'),
        ),
      );
      expectCleanupReview(
        pipeline,
        guards,
        failure,
        PrintFailureKind.unsupported,
        'error.print.excelExportQuarantined',
        events,
        submitted,
      );
    });

    test(
      'cancelled failed-join loser cleanup failure preserves cancelled primary',
      () async {
        final events = <String>[];
        final submitted = <SubmittedPdf>[];
        final registry = CancellationRegistry();
        final token = registry.register('cleanup-cancel-failed-join');
        final guards = ExcelProcessGuards();
        final pipeline = guardedPipeline(
          events: events,
          requests: [],
          submitted: submitted,
          exporter: CleanupFailingExporter(events),
          runtime: ImmediateRuntime(
            join: WorkerJoin.failed,
            afterExport: (_) => registry.cancel('cleanup-cancel-failed-join'),
          ),
          guards: guards,
        );
        final fixture = sourceFixture();
        addTearDown(fixture.dispose);
        final job = worksheetJob(
          fixture.source.absolute,
          fixture.signature,
          'MEC',
          1,
        );
        final failure = await captureFailureAsync(
          () => pipeline.exportAndSubmit(
            job,
            fixture.source,
            fixture.openSource(),
            token,
          ),
        );
        expectCleanupReview(
          pipeline,
          guards,
          failure,
          PrintFailureKind.cancelled,
          'error.print.excelExportCancelled',
          events,
          submitted,
        );
      },
    );

    test(
      'breaker tripped by one pipeline rejects a distinct pipeline',
      () async {
        final events = <String>[];
        final submitted = <SubmittedPdf>[];
        final guards = ExcelProcessGuards();
        final first = guardedPipeline(
          events: events,
          requests: [],
          submitted: submitted,
          exporter: RecordingExporter(events, []),
          runtime: ImmediateRuntime(join: WorkerJoin.failed),
          guards: guards,
        );
        final second = guardedPipeline(
          events: events,
          requests: [],
          submitted: submitted,
          exporter: RecordingExporter(events, []),
          runtime: ImmediateRuntime(),
          guards: guards,
        );
        final fixture = sourceFixture();
        addTearDown(fixture.dispose);
        final job = worksheetJob(
          fixture.source.absolute,
          fixture.signature,
          'MEC',
          1,
        );
        final cancellation = CancellationRegistry().register('plan');
        final firstFailure = await captureFailureAsync(
          () => first.exportAndSubmit(
            job,
            fixture.source,
            fixture.openSource(),
            cancellation,
          ),
        );
        final secondFailure = await captureFailureAsync(
          () => second.exportAndSubmit(
            job,
            fixture.source,
            fixture.openSource(),
            cancellation,
          ),
        );
        expect(firstFailure.kind, PrintFailureKind.unsupported);
        expect(secondFailure.kind, PrintFailureKind.unsupported);
        expect(events, ['policy', 'export']);
        expect(submitted, isEmpty);
        guards.quarantine.clear();
      },
    );

    test(
      'operation acquisition deadline expires before policy or staging',
      () async {
        final events = <String>[];
        final submitted = <SubmittedPdf>[];
        final guards = ExcelProcessGuards();
        final pipeline = guardedPipeline(
          events: events,
          requests: [],
          submitted: submitted,
          exporter: RecordingExporter(events, []),
          runtime: ImmediateRuntime(),
          guards: guards,
        );
        expect(guards.operation.tryAcquire(), isTrue);
        addTearDown(guards.operation.release);
        final fixture = sourceFixture();
        addTearDown(fixture.dispose);
        final job = worksheetJob(
          fixture.source.absolute,
          fixture.signature,
          'MEC',
          1,
          timeoutMinutes: 0,
        );
        final failure = await captureFailureAsync(
          () => pipeline.exportAndSubmit(
            job,
            fixture.source,
            fixture.openSource(),
            CancellationRegistry().register('deadline'),
          ),
        );
        expect(failure.kind, PrintFailureKind.failed);
        expect(failure.message, 'error.print.excelExportTimedOut');
        expect(events, isEmpty);
        expect(submitted, isEmpty);
      },
    );
  });
}

class _InternetZoneReader implements ZoneIdentifierReader {
  @override
  Uint8List? readZoneIdentifier(AnchoredPath source) =>
      Uint8List.fromList(utf8.encode('[ZoneTransfer]\r\nZoneId=3\r\n'));
}

class _SourceReplacingExporter implements WorksheetPdfExporter {
  _SourceReplacingExporter(this.events, this.sourcePath);

  final List<String> events;
  final String sourcePath;

  @override
  Future<ExportedWorksheetPdf> exportPdf(ExcelExportRequest request) async {
    events.add('export');
    final artifact = fixtureExportedPdf(request.stagedWorkbook);
    File(sourcePath).renameSync('$sourcePath.approved-original');
    File(sourcePath)
        .writeAsBytesSync(utf8.encode('valid replacement workbook bytes'));
    return artifact;
  }
}

PrintFailure _captureSyncFailure(void Function() body) {
  try {
    body();
  } on PrintFailure catch (failure) {
    return failure;
  }
  throw StateError('expected a PrintFailure');
}
