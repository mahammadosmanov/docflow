// Port of the 12 PrintService-backed Excel cases in `src-tauri/src/platform/excel.rs`
// (retry, cleanup evidence through queue and journal, enabled-row ordering,
// persisted resume, source mutation and cancellation). The Rust `PreviewAdapter`
// is [FakeBatchAdapter]; the Rust `write_workbook` is [writeWorkbook].
@TestOn('vm')
library;

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:docflow/engine/contracts.dart';
import 'package:docflow/engine/errors.dart';
import 'package:docflow/engine/journal.dart';
import 'package:docflow/engine/platform/excel.dart';
import 'package:docflow/engine/print_queue.dart';
import 'package:docflow/engine/progress.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

import 'e11_batch_support.dart';
import 'e12_excel_support.dart';

PrintService _service(
  Directory root,
  ExcelExportPipelinePort pipeline, {
  PrintAdapter? adapter,
}) => PrintService(
  queueRoot: p.join(root.path, 'queues'),
  journalRoot: p.join(root.path, 'journals'),
  ttl: const Duration(minutes: 5),
  mutationLock: GlobalMutationLock(),
  adapter: adapter ?? FakeBatchAdapter(),
  excelPipeline: pipeline,
);

/// Rust `write_workbook`: MEC (enabled), Hidden (hidden), CIV (enabled).
void _writeBook(String path) => writeWorkbook(path, [
  ('MEC', 17, null),
  ('Hidden', 44, 'hidden'),
  ('CIV', 19, null),
]);

BatchPrintParams _params(String workbook, {int retries = 0}) =>
    BatchPrintParams(
      files: [workbook],
      printer: 'Office Printer',
      retries: retries,
      timeoutMinutes: 1,
    );

List<Map<String, Object?>> _rows(Plan plan) =>
    (jsonDecode(jsonEncode((plan.meta['editor']! as Map)['rows'])) as List)
        .cast<Map<String, Object?>>();

Map<String, Object?> _queue(Directory root, String queueId) => jsonDecode(
  File(p.join(root.path, 'queues', '$queueId.json')).readAsStringSync(),
) as Map<String, Object?>;

JournalRecord _journal(Directory root, String journalId) =>
    JournalStore(p.join(root.path, 'journals')).read(journalId);

/// Shared pipeline helper: each pipeline owns its guards, cleaned up at end.
ExcelExportPipeline _pipeline({
  required List<String> events,
  required WorksheetPdfExporter exporter,
  required ControlledPdfSubmitter submitter,
  ExcelProcessGuards? guards,
}) => ExcelExportPipeline(
  policy: RecordingPolicy(events),
  exporter: exporter,
  submitter: submitter,
  guards: guards ?? ExcelProcessGuards(),
  runtime: ImmediateRuntime(),
  teardownGrace: const Duration(seconds: 1),
);

void main() {
  late Directory root;
  setUp(() => root = batchTempDir('ps-excel'));
  tearDown(() {
    if (root.existsSync()) {
      root.deleteSync(recursive: true);
    }
  });

  final cleanupKey = 'error.print.exportCleanupRequiresReview';
  final failedOutcome = PrintFailure.failed(
    'error.print.syntheticPreEffectFailure',
  );

  test('retry re-exports and never reuses a prior controlled PDF', () async {
    final workbook = p.join(root.path, 'book.xlsx');
    _writeBook(workbook);
    final events = <String>[];
    final exporter = SequencedPdfExporter(events);
    final submitter = FailOnceCapturingSubmitter(events, []);
    final service = _service(
      root,
      _pipeline(events: events, exporter: exporter, submitter: submitter),
    );
    final plan = service.preview(
      batchCaller,
      _params(workbook, retries: 1),
      (_) {},
    );
    final rows = _rows(plan);
    rows[2]['enabled'] = false;
    final revised = service.revise(batchCaller, plan.planId, rows);

    final result = await service.apply(revised.planId, batchCaller, (_) {});

    expect(result.applied, 1);
    expect(result.failed, 0);
    expect(exporter.attempts, 2);
    expect(submitter.attempts, 2);
    expect(events, [
      'policy',
      'export',
      'submit',
      'policy',
      'export',
      'submit',
    ]);
    expect(submitter.submitted, hasLength(2));
    expect(submitter.submitted[0], isNot(equals(submitter.submitted[1])));
  });

  test(
    'completed cleanup warning is reported without retrying export or submit',
    () async {
      final workbook = p.join(root.path, 'book.xlsx');
      _writeBook(workbook);
      final events = <String>[];
      final service = _service(
        root,
        _pipeline(
          events: events,
          exporter: CleanupFailingExporter(events),
          submitter: OutcomeSubmitter(events, null),
        ),
      );
      final plan = service.preview(
        batchCaller,
        _params(workbook, retries: 3),
        (_) {},
      );
      final rows = _rows(plan);
      rows[2]['enabled'] = false;
      final revised = service.revise(batchCaller, plan.planId, rows);

      final result = await service.apply(revised.planId, batchCaller, (_) {});

      expect(result.applied, 1);
      expect(result.failed, 0);
      expect(events, ['policy', 'export', 'submit']);
      expect(result.errors, hasLength(1));
      expect(result.errors.single.key, cleanupKey);
      expect(result.errors.single.subject, startsWith('docflow-excel-'));
      final queue = _queue(root, result.meta['queueId']! as String);
      expect(
        (((queue['jobs']! as List)[0] as Map)['cleanupReview']
            as Map)['warningKey'],
        cleanupKey,
      );
      final journal = _journal(root, result.meta['auditJournalId']! as String);
      expect(journal.cleanupReviews, hasLength(1));
      expect(journal.cleanupReviews.single.warningKey, cleanupKey);
      expect(
        journal.cleanupReviews.single.locator,
        result.errors.single.subject,
      );
    },
  );

  test('cleanup locator is durable when outcome journal write fails', () async {
    final workbook = p.join(root.path, 'book.xlsx');
    _writeBook(workbook);
    final events = <String>[];
    final service = _service(
      root,
      _pipeline(
        events: events,
        exporter: CleanupFailingExporter(events),
        submitter: OutcomeSubmitter(events, null),
      ),
    )..journalWriteFailOn = 3;
    final plan = service.preview(batchCaller, _params(workbook), (_) {});
    final rows = _rows(plan);
    rows[2]['enabled'] = false;
    final revised = service.revise(batchCaller, plan.planId, rows);
    final queueId = revised.meta['queueId']! as String;

    await expectLater(
      service.apply(revised.planId, batchCaller, (_) {}),
      throwsA(isA<Object>()),
    );

    final queue = _queue(root, queueId);
    final review = ((queue['jobs']! as List)[0] as Map)['cleanupReview'] as Map;
    expect(review['warningKey'], cleanupKey);
    expect(review['locator'] as String, startsWith('docflow-excel-'));
    expect(events, ['policy', 'export', 'submit']);
  });

  test(
    'cleanup locator is journaled when evidence queue checkpoint fails',
    () async {
      final workbook = p.join(root.path, 'book.xlsx');
      _writeBook(workbook);
      final events = <String>[];
      final service = _service(
        root,
        _pipeline(
          events: events,
          exporter: CleanupFailingExporter(events),
          submitter: OutcomeSubmitter(events, failedOutcome),
        ),
      );
      service.queues.saveFailOn = 3;
      final plan = service.preview(batchCaller, _params(workbook), (_) {});
      final rows = _rows(plan);
      rows[2]['enabled'] = false;
      final revised = service.revise(batchCaller, plan.planId, rows);

      Object? error;
      try {
        await service.apply(revised.planId, batchCaller, (_) {});
      } on BackendError catch (e) {
        error = e;
      }

      final journalFiles = Directory(p.join(root.path, 'journals'))
          .listSync()
          .whereType<File>()
          .where((f) => f.path.endsWith('.json'))
          .toList();
      expect(journalFiles, hasLength(1));
      final journal = jsonDecode(
        journalFiles.single.readAsStringSync(),
      ) as Map<String, Object?>;
      final locator =
          ((journal['cleanupReviews']! as List)[0] as Map)['locator'] as String;
      expect(locator, startsWith('docflow-excel-'));
      expect(error, isA<BackendError>());
      final message = (error! as BackendError).message;
      expect(message, contains('injected print queue save failure'));
      expect(message, contains('primaryClassification=failed'));
      expect(message, contains('warningKey=$cleanupKey'));
      expect(message, contains('locator=$locator'));
      expect(events, ['policy', 'export', 'submit']);
    },
  );

  test(
    'restart resume second cleanup failure preserves both distinct locators',
    () async {
      final workbook = p.join(root.path, 'book.xlsx');
      _writeBook(workbook);
      final firstEvents = <String>[];
      final firstService = _service(
        root,
        _pipeline(
          events: firstEvents,
          exporter: CleanupFailingExporter(firstEvents),
          submitter: OutcomeSubmitter(firstEvents, failedOutcome),
        ),
      )..journalWriteFailOn = 3;
      final plan = firstService.preview(batchCaller, _params(workbook), (_) {});
      final rows = _rows(plan);
      rows[2]['enabled'] = false;
      final revised = firstService.revise(batchCaller, plan.planId, rows);
      final queueId = revised.meta['queueId']! as String;
      await expectLater(
        firstService.apply(revised.planId, batchCaller, (_) {}),
        throwsA(isA<Object>()),
      );
      final firstLocator =
          (((_queue(root, queueId)['jobs']! as List)[0] as Map)['cleanupReview']
                  as Map)['locator']
              as String;

      final secondEvents = <String>[];
      final restarted = _service(
        root,
        _pipeline(
          events: secondEvents,
          exporter: CleanupFailingExporter(secondEvents),
          submitter: OutcomeSubmitter(secondEvents, failedOutcome),
        ),
      );
      final resumed = await restarted.resume(batchCaller, queueId);
      expect([
        for (final job in resumed.meta['jobs']! as List)
          (job as Map).containsKey('cleanupReview'),
      ], everyElement(isFalse));
      final resumeRows = _rows(resumed);
      resumeRows[0]['enabled'] = true;
      final resumedRevised = restarted.revise(
        batchCaller,
        resumed.planId,
        resumeRows,
      );
      expect([
        for (final job in resumedRevised.meta['jobs']! as List)
          (job as Map).containsKey('cleanupReview'),
      ], everyElement(isFalse));

      final result = await restarted.apply(
        resumedRevised.planId,
        batchCaller,
        (_) {},
      );
      final secondLocator = result.errors
          .firstWhere((w) => w.key == cleanupKey)
          .subject;
      expect(secondLocator, isNot(firstLocator));
      final cleanup = (_queue(root, queueId)['jobs']! as List)[0] as Map;
      final locators = [
        for (final review in (cleanup['cleanupReview'] as List))
          (review as Map)['locator'],
      ];
      expect(locators, [firstLocator, secondLocator]);
      final journal = _journal(root, result.meta['auditJournalId']! as String);
      expect(journal.cleanupReviews.map((r) => r.locator).toList(), [
        firstLocator,
        secondLocator,
      ]);
      expect(firstEvents, ['policy', 'export', 'submit']);
      expect(secondEvents, ['policy', 'export', 'submit']);
    },
  );

  test(
    'failed cleanup warning is not retried or counted as another attempt',
    () async {
      final workbook = p.join(root.path, 'book.xlsx');
      _writeBook(workbook);
      final events = <String>[];
      final service = _service(
        root,
        _pipeline(
          events: events,
          exporter: CleanupFailingExporter(events),
          submitter: OutcomeSubmitter(events, failedOutcome),
        ),
      );
      final plan = service.preview(
        batchCaller,
        _params(workbook, retries: 3),
        (_) {},
      );
      final rows = _rows(plan);
      rows[2]['enabled'] = false;
      final revised = service.revise(batchCaller, plan.planId, rows);

      final result = await service.apply(revised.planId, batchCaller, (_) {});

      final queue = _queue(root, result.meta['queueId']! as String);
      expect(((queue['jobs']! as List)[0] as Map)['attempts'], 1);
      expect(
        (((queue['jobs']! as List)[0] as Map)['cleanupReview']
            as Map)['warningKey'],
        cleanupKey,
      );
      final journal = _journal(root, result.meta['auditJournalId']! as String);
      expect(journal.cleanupReviews, hasLength(1));
      expect(journal.cleanupReviews.single.warningKey, cleanupKey);
      expect(events, ['policy', 'export', 'submit']);
      expect(result.failed, 1);
      expect(result.errors.any((w) => w.key == cleanupKey), isTrue);
    },
  );

  test(
    'uncertain cleanup warning is reported without retrying export or submit',
    () async {
      final workbook = p.join(root.path, 'book.xlsx');
      _writeBook(workbook);
      final events = <String>[];
      final service = _service(
        root,
        _pipeline(
          events: events,
          exporter: CleanupFailingExporter(events),
          submitter: OutcomeSubmitter(
            events,
            PrintFailure.uncertain('printer outcome unknown'),
          ),
        ),
      );
      final plan = service.preview(
        batchCaller,
        _params(workbook, retries: 3),
        (_) {},
      );
      final rows = _rows(plan);
      rows[2]['enabled'] = false;
      final revised = service.revise(batchCaller, plan.planId, rows);

      final result = await service.apply(revised.planId, batchCaller, (_) {});

      expect(events, ['policy', 'export', 'submit']);
      expect(
        result.errors.any(
          (w) =>
              w.key == 'warn.print.stateUncertain' && w.subject == 'book.xlsx',
        ),
        isTrue,
      );
      final cleanup = result.errors.firstWhere((w) => w.key == cleanupKey);
      expect(cleanup.subject, startsWith('docflow-excel-'));
      final queue = _queue(root, result.meta['queueId']! as String);
      expect(((queue['jobs']! as List)[0] as Map)['status'], 'uncertain');
      expect(
        (((queue['jobs']! as List)[0] as Map)['cleanupReview']
            as Map)['locator'],
        cleanup.subject,
      );
      final journal = _journal(root, result.meta['auditJournalId']! as String);
      expect(journal.state, JournalState.recoveryRequired);
      expect(journal.cleanupReviews, hasLength(1));
      expect(journal.cleanupReviews.single.locator, cleanup.subject);
    },
  );

  test(
    'cancelled cleanup warning survives queue and journal readback',
    () async {
      final workbook = p.join(root.path, 'book.xlsx');
      _writeBook(workbook);
      final events = <String>[];
      final service = _service(
        root,
        _pipeline(
          events: events,
          exporter: CleanupFailingExporter(events),
          submitter: OutcomeSubmitter(
            events,
            PrintFailure.cancelled(
              'error.print.syntheticCancelled',
              uncertain: false,
            ),
          ),
        ),
      );
      final plan = service.preview(
        batchCaller,
        _params(workbook, retries: 3),
        (_) {},
      );
      final rows = _rows(plan);
      rows[2]['enabled'] = false;
      final revised = service.revise(batchCaller, plan.planId, rows);

      final result = await service.apply(revised.planId, batchCaller, (_) {});

      final cleanup = result.errors.firstWhere((w) => w.key == cleanupKey);
      final queue = _queue(root, result.meta['queueId']! as String);
      expect(((queue['jobs']! as List)[0] as Map)['status'], 'cancelled');
      expect(((queue['jobs']! as List)[0] as Map)['attempts'], 1);
      expect(
        (((queue['jobs']! as List)[0] as Map)['cleanupReview']
            as Map)['locator'],
        cleanup.subject,
      );
      final journal = _journal(root, result.meta['auditJournalId']! as String);
      expect(journal.state, JournalState.cancelledClean);
      expect(journal.cleanupReviews.map((r) => r.locator).toList(), [
        cleanup.subject,
      ]);
      expect(events, ['policy', 'export', 'submit']);
    },
  );

  test('print service exports only enabled rows in revised order', () async {
    final workbook = p.join(root.path, 'book.xlsx');
    _writeBook(workbook);
    final events = <String>[];
    final requests = <RequestObservation>[];
    final submitted = <SubmittedPdf>[];
    final service = _service(
      root,
      _pipeline(
        events: events,
        exporter: RecordingExporter(events, requests),
        submitter: RecordingSubmitter(events, submitted),
      ),
    );
    final plan = service.preview(batchCaller, _params(workbook), (_) {});
    final rows = _rows(plan);
    final revised = service.revise(batchCaller, plan.planId, [
      rows[2],
      rows[1],
      rows[0],
    ]);

    final result = await service.apply(revised.planId, batchCaller, (_) {});

    expect(result.applied, 2);
    expect([for (final r in requests) r.worksheet.name], ['CIV', 'MEC']);
    expect(
      [for (final s in submitted) (s.worksheet, s.pageRange)],
      [('CIV', 'all'), ('MEC', 'all')],
    );
  });

  test('persisted worksheet resume revalidates re-exports and submits in revised order', () async {
    final workbook = p.join(root.path, 'book.xlsx');
    _writeBook(workbook);
    final firstEvents = <String>[];
    final firstService = _service(
      root,
      _pipeline(
        events: firstEvents,
        exporter: BytesExporter(firstEvents, multiPagePdf(1)),
        submitter: OutcomeSubmitter(firstEvents, failedOutcome),
      ),
    );
    final plan = firstService.preview(batchCaller, _params(workbook), (_) {});
    final first = await firstService.apply(plan.planId, batchCaller, (_) {});
    expect((first.applied, first.failed), (0, 1));
    expect(firstEvents, ['policy', 'export', 'submit']);
    final queueId = first.meta['queueId']! as String;
    final ambient = File(p.join(root.path, 'ambient-export.pdf'))
      ..writeAsBytesSync(multiPagePdf(1));
    final persisted = _queue(root, queueId);
    for (final job in persisted['jobs']! as List) {
      (job as Map)['exportedPdfPath'] = ambient.path;
      job['cleanupReview'] = {
        'warningKey': cleanupKey,
        'locator': 'ambient-export.pdf',
      };
    }
    File(p.join(root.path, 'queues', '$queueId.json'))
        .writeAsStringSync(jsonEncode(persisted));

    final resumedEvents = <String>[];
    final requests = <RequestObservation>[];
    final submitted = <SubmittedPdf>[];
    final resumedService = _service(
      root,
      _pipeline(
        events: resumedEvents,
        exporter: RecordingExporter(resumedEvents, requests),
        submitter: RecordingSubmitter(resumedEvents, submitted),
      ),
    );
    final resumed = await resumedService.resume(batchCaller, queueId);
    final rows = _rows(resumed);
    Map<String, Object?> byName(String name) =>
        rows.firstWhere((row) => row['worksheetName'] == name);
    final revised = resumedService.revise(batchCaller, resumed.planId, [
      byName('CIV'),
      byName('MEC'),
    ]);

    final result = await resumedService.apply(
      revised.planId,
      batchCaller,
      (_) {},
    );

    expect((result.applied, result.failed), (2, 0));
    expect(resumedEvents, [
      'policy',
      'export',
      'submit',
      'policy',
      'export',
      'submit',
    ]);
    final approved = File(workbook).readAsBytesSync();
    expect(requests, hasLength(2));
    expect(requests[0].requestId, isNot(requests[1].requestId));
    expect([for (final r in requests) r.worksheet.name], ['CIV', 'MEC']);
    expect([for (final r in requests) r.workbookBytes], everyElement(approved));
    expect([for (final s in submitted) s.worksheet], ['CIV', 'MEC']);
    expect([for (final s in submitted) s.pdf], everyElement(multiPagePdf(2)));
    final after = _queue(root, queueId);
    final afterJobs = (after['jobs']! as List).cast<Map<String, Object?>>();
    expect(
      [for (final job in afterJobs) ((job['worksheet']! as Map)['name'])],
      ['CIV', 'MEC'],
    );
    expect(afterJobs[0].containsKey('exportedPdfPath'), isFalse);
    expect(afterJobs[0]['cleanupReview'], {
      'warningKey': cleanupKey,
      'locator': 'ambient-export.pdf',
    });
    expect(afterJobs[1].containsKey('exportedPdfPath'), isFalse);
    expect(afterJobs[1]['cleanupReview'], {
      'warningKey': cleanupKey,
      'locator': 'ambient-export.pdf',
    });
  });

  test(
    'print service source mutation after export blocks all submission',
    () async {
      final workbook = p.join(root.path, 'book.xlsx');
      _writeBook(workbook);
      final events = <String>[];
      final submitted = <SubmittedPdf>[];
      final stagedPaths = <String>[];
      final service = _service(
        root,
        _pipeline(
          events: events,
          exporter: _SourceMutatingExporter(events, workbook, stagedPaths),
          submitter: RecordingSubmitter(events, submitted),
        ),
      );
      final plan = service.preview(batchCaller, _params(workbook), (_) {});
      final rows = _rows(plan);
      rows[1]['enabled'] = false;
      rows[2]['enabled'] = false;
      final revised = service.revise(batchCaller, plan.planId, rows);

      final result = await service.apply(revised.planId, batchCaller, (_) {});

      expect((result.applied, result.failed), (0, 1));
      expect(result.errors.first.key, 'error.print.workbookChanged');
      expect(events, ['policy', 'export']);
      expect(submitted, isEmpty);
      expect(stagedPaths, hasLength(1));
      expect(File(stagedPaths.single).existsSync(), isFalse);
    },
  );

  test('print service cancel reaches blocked export token cleanly', () async {
    final workbook = p.join(root.path, 'book.xlsx');
    _writeBook(workbook);
    final events = <String>[];
    final submitted = <SubmittedPdf>[];
    final entered = Completer<void>();
    final service = _service(
      root,
      _pipeline(
        events: events,
        exporter: CancellationBlockingExporter(events, entered),
        submitter: RecordingSubmitter(events, submitted),
      ),
    );
    final plan = service.preview(batchCaller, _params(workbook), (_) {});
    final rows = _rows(plan);
    rows[1]['enabled'] = false;
    rows[2]['enabled'] = false;
    final revised = service.revise(batchCaller, plan.planId, rows);

    final applying = service.apply(revised.planId, batchCaller, (_) {});
    await entered.future.timeout(const Duration(seconds: 5));
    expect(service.cancel(revised.planId), isTrue);
    final result = await applying;

    final queue = _queue(root, result.meta['queueId']! as String);
    expect(((queue['jobs']! as List)[0] as Map)['status'], 'cancelled');
    expect(submitted, isEmpty);
  });
}

/// Overwrites the approved workbook during export, like the Rust
/// `SourceMutatingExporter`, and records the staged workspace path.
final class _SourceMutatingExporter implements WorksheetPdfExporter {
  _SourceMutatingExporter(this.events, this.sourcePath, this.stagedPaths);

  final List<String> events;
  final String sourcePath;
  final List<String> stagedPaths;

  @override
  Future<ExportedWorksheetPdf> exportPdf(ExcelExportRequest request) async {
    events.add('export');
    stagedPaths.add(request.stagedWorkbook.workspacePath);
    File(sourcePath)
        .writeAsBytesSync('changed after the approved preview'.codeUnits);
    return fixtureExportedPdf(request.stagedWorkbook);
  }
}
