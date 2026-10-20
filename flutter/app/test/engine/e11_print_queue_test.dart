// Ports of the unit tests in src-tauri/src/print_queue.rs (tests module).
import 'package:docflow/engine/journal.dart';
import 'package:docflow/engine/ooxml.dart';
import 'package:docflow/engine/print_queue.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('diagnostic detail is persisted without replacing the localized error', () {
    final failure = PrintFailure.failed(
      'error.print.excelComFailed',
    ).withDiagnostic('excel_com stage=open hresult=0x800A03EC');

    expect(failure.message, 'error.print.excelComFailed');
    expect(failure.diagnosticDetail, 'excel_com stage=open hresult=0x800A03EC');
  });

  test('printer diagnostic reports routes and outcomes without file paths', () {
    final job = PrintJob(
      jobId: 'job-1',
      path: r'C:\Private\RFI No. secret.pdf',
      printer: 'EPSON L15160 Series',
      format: 'pdf',
      verb: 'docflow',
      status: 'completed_unconfirmed',
      attempts: 1,
      error: 'driver released the spool record',
    );
    final report = queueDiagnostic(
      QueueState(
        version: 1,
        queueId: 'queue-1',
        ownerId: 'owner',
        sessionId: 'session',
        createdAt: '2026-10-20T00:00:00.000000+00:00',
        updatedAt: '2026-10-20T00:00:00.000000+00:00',
        status: 'completed',
        jobs: [job],
      ),
    );

    final encoded = report.toJson().toString();
    expect(report.printer, 'EPSON L15160 Series');
    expect(report.jobs.first.route, 'docflow');
    expect(report.jobs.first.status, 'completed_unconfirmed');
    expect(encoded.contains('Private'), isFalse);
    expect(encoded.contains('secret.pdf'), isFalse);
  });

  test('cleanup review wire collection deduplicates exact locator', () {
    final evidence = {
      'warningKey': 'error.print.exportCleanupRequiresReview',
      'locator': 'docflow-excel-retained',
    };
    final job = PrintJob().toJson()
      ..['cleanupReview'] = [evidence, evidence];

    final deduplicated = PrintJob.fromJson(job);

    expect(deduplicated.toJson()['cleanupReview'], evidence);
  });

  test('cleanup review wire collection enforces its distinct locator bound', () {
    Map<String, Object?> evidence(int index) => {
      'warningKey': 'error.print.exportCleanupRequiresReview',
      'locator': 'docflow-excel-$index',
    };
    final job = PrintJob().toJson()
      ..['cleanupReview'] = [for (var i = 0; i < 16; i++) evidence(i)];
    final accepted = PrintJob.fromJson(job);
    expect(
      (accepted.toJson()['cleanupReview'] as List).length,
      16,
    );

    job['cleanupReview'] = [for (var i = 0; i < 17; i++) evidence(i)];
    expect(
      () => PrintJob.fromJson(job),
      throwsA(predicate((Object e) => e.toString().contains('cleanup review evidence limit'))),
    );
  });

  test('cleanup evidence is not retry authority but quarantine disposition is', () {
    const review = CleanupReview(
      warningKey: 'error.print.exportCleanupRequiresReview',
      locator: 'docflow-excel-test',
    );
    final retryable = PrintFailure.failed(
      'pre-effect failure',
    ).withCleanupReview(review);

    expect(retryable.retryDisposition, RetryDisposition.retryable);
    expect(retryable.cleanupReview, review);

    final quarantined = retryable.withRetryDisposition(
      RetryDisposition.cleanupQuarantined,
    );
    expect(quarantined.retryDisposition, isNot(RetryDisposition.retryable));
    expect(quarantined.cleanupReview, review);
  });

  test('protected workbook safety is unsafe for resume and apply', () {
    expect(
      workbookSafetyIsUnsafe(
        const WorkbookSafetyFacts(hasWorkbookProtection: true),
      ),
      isTrue,
    );
  });

  test('printer inventory normalizes names, default first, and dedups case-insensitively', () {
    final inventory = PrinterInventory(
      printers: const ['beta', 'Alpha', 'alpha', 'bad"name', 'Gamma'],
      defaultPrinter: 'Gamma',
    ).normalized();

    expect(inventory.printers, ['Gamma', 'Alpha', 'beta']);
    expect(inventory.defaultPrinter, 'Gamma');
    expect(inventory.contains('beta'), isTrue);
    expect(inventory.contains('bad"name'), isFalse);
  });

  test('persisted worksheet metadata must be complete', () {
    final json = PrintJob().toJson()..['worksheet'] = {
      'workbookIndex': 1,
      'sheetId': 1,
      'relationshipId': 'rId1',
      'relationshipType': 'worksheet',
      'partPath': 'xl/worksheets/sheet1.xml',
      'name': 'Sheet1',
      'visibility': 'visible',
    };
    expect(() => PrintJob.fromJson(json), throwsFormatException);
  });
}
