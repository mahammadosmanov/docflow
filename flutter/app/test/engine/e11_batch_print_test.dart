// Port of the job-validation and persisted-metadata tests in
// src-tauri/tests/batch_print.rs (worksheet_job_validation_*,
// legacy_file_job_*, persisted_job_metadata_*).
import 'dart:io';

import 'package:docflow/engine/errors.dart';
import 'package:docflow/engine/paths.dart';
import 'package:docflow/engine/progress.dart';
import 'package:docflow/engine/operations/batch_print.dart';
import 'package:docflow/engine/ooxml.dart';
import 'package:docflow/engine/print_queue.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

import 'e12_excel_support.dart' show descriptor, safety;

PrintJob _validWorksheet() => PrintJob(
  jobId: 'worksheet-c1',
  path: '/approved/book.xlsx',
  printer: 'Office Printer',
  verb: 'docflow',
  format: 'XLSX',
  signature: FileSignature(size: 42, mtimeNs: 99, sha256: 'a' * 64),
  worksheet: descriptor('MEC', 1),
  workbookSafety: safety(),
);

Map<String, Object?> _legacy() => {
  'jobId': 'legacy-c1',
  'path': '/approved/legacy.pdf',
  'printer': 'Office Printer',
  'verb': 'docflow',
  'format': 'PDF',
  'signature': {'size': 12, 'mtimeNs': 34},
};

final class _Adapter implements PrintAdapter {
  _Adapter({required this.printers, this.defaultPrinter});

  final List<String> printers;
  final String? defaultPrinter;
  String? unsupportedPaper;
  int inspected = 0;
  final List<PrintJob> submitted = [];

  @override
  PrinterInventory inventory() => PrinterInventory(
    printers: printers,
    defaultPrinter: defaultPrinter,
  ).normalized();

  @override
  PrintJobSupport inspect(String path, String printer, PrintSettings settings) {
    inspected += 1;
    if (unsupportedPaper == settings.paper) {
      throw PrintFailure.unsupportedOptions(
        'paper is unsupported by the test printer',
      );
    }
    return PrintJobSupport.controlledPdf();
  }

  @override
  void submit(PrintJob job, bool Function() cancelled) => submitted.add(job);
}

void main() {
  test(
    'forged job with an invalid page range fails validation before submit',
    () {
      final job = PrintJob(
        jobId: 'forged-c1',
        path: '/approved/forged.pdf',
        printer: 'Office Printer',
        verb: 'docflow',
        format: 'PDF',
        pageRange: '0',
      );
      expect(
        () => validateJob(job),
        throwsA(
          isA<BackendError>().having(
            (e) => e.message,
            'message',
            'error.print.pageRangeInvalid',
          ),
        ),
      );
    },
  );

  test(
    'unknown printer and unsupported options fail before any submission',
    () {
      final root = Directory.systemTemp.createTempSync('docflow-batch-print-');
      addTearDown(() => root.deleteSync(recursive: true));
      final file = p.join(root.path, 'one.pdf');
      File(file).writeAsBytesSync([1, 2, 3]);
      final adapter = _Adapter(
        printers: const ['Office Printer'],
        defaultPrinter: 'Office Printer',
      );
      final service = PrintService(
        queueRoot: p.join(root.path, 'print-queues'),
        journalRoot: p.join(root.path, 'journal'),
        ttl: const Duration(minutes: 5),
        mutationLock: GlobalMutationLock(),
        adapter: adapter,
      );
      const caller = CallerIdentity('owner', 'session');

      final malicious = BatchPrintParams(
        files: [file],
        printer: 'Office Printer"; Remove-Item C:\\*',
      );
      final plan = service.preview(caller, malicious, (_) {});
      expect(plan.steps, isEmpty);
      expect(plan.warnings.first.key, 'warn.print.printerUnavailable');
      expect(adapter.inspected, 0);
      expect(adapter.submitted, isEmpty);

      adapter.unsupportedPaper = 'A3';
      final unsupported = BatchPrintParams(
        files: [file],
        printer: 'Office Printer',
        paper: 'A3',
      );
      final second = service.preview(caller, unsupported, (_) {});
      expect(second.steps, isEmpty);
      expect(second.warnings.first.key, 'warn.print.unsupportedOptions');
      expect(adapter.submitted, isEmpty);
    },
  );

  test(
    'worksheet job validation requires typed sha, safety and all-only range',
    () {
      validateJob(_validWorksheet());

      final missingSha = _validWorksheet()
        ..signature = const FileSignature(size: 42, mtimeNs: 99);
      expect(() => validateJob(missingSha), throwsA(isA<BackendError>()));

      final uppercaseSha = _validWorksheet()
        ..signature = FileSignature(size: 42, mtimeNs: 99, sha256: 'A' * 64);
      expect(() => validateJob(uppercaseSha), throwsA(isA<BackendError>()));

      final missingSafety = _validWorksheet()..workbookSafety = null;
      expect(() => validateJob(missingSafety), throwsA(isA<BackendError>()));

      final unsafe = _validWorksheet()
        ..workbookSafety = const WorkbookSafetyFacts(externalLinkCount: 1);
      expect(() => validateJob(unsafe), throwsA(isA<BackendError>()));

      final ranged = _validWorksheet()..pageRange = '1';
      expect(
        () => validateJob(ranged),
        throwsA(
          isA<BackendError>().having(
            (e) => e.message,
            'message',
            'error.print.worksheetPageRangeAllOnly',
          ),
        ),
      );
    },
  );

  test('legacy file job defaults optional worksheet metadata and sha256', () {
    final legacy = PrintJob.fromJson(_legacy());
    expect(legacy.signature.size, 12);
    expect(legacy.signature.mtimeNs, 34);
    expect(legacy.signature.sha256, isNull);
    expect(legacy.worksheet, isNull);
    expect(legacy.workbookSafety, isNull);
    validateJob(legacy);
    expect(legacy.toJson()['signature'], {'size': 12, 'mtimeNs': 34});
  });

  test('persisted job metadata distinguishes absence from null, malformed and orphans', () {
    final legacy = _legacy();
    final parsed = PrintJob.fromJson(legacy);
    expect(parsed.worksheet, isNull);
    expect(parsed.workbookSafety, isNull);
    expect(parsed.signature.sha256, isNull);

    final validWorksheet = <String, Object?>{
      ..._legacy(),
      'signature': {'size': 12, 'mtimeNs': 34, 'sha256': 'a' * 64},
      'worksheet': _worksheetJson(),
      'workbookSafety': workbookSafetyToJson(safety()),
    };
    expect(PrintJob.fromJson(validWorksheet).worksheet, isNotNull);

    expect(
      () => PrintJob.fromJson({...legacy, 'worksheet': null}),
      throwsFormatException,
    );
    expect(
      () => PrintJob.fromJson({
        ...validWorksheet,
        'worksheet': {'name': 'MEC'},
      }),
      throwsFormatException,
    );
    expect(
      () => PrintJob.fromJson({...legacy, 'workbookSafety': null}),
      throwsFormatException,
    );
    expect(
      () => PrintJob.fromJson({
        ...legacy,
        'worksheet': validWorksheet['worksheet'],
      }),
      throwsFormatException,
    );
    expect(
      () => PrintJob.fromJson({
        ...legacy,
        'workbookSafety': validWorksheet['workbookSafety'],
      }),
      throwsFormatException,
    );
    expect(
      () => PrintJob.fromJson({
        ...legacy,
        'signature': {'size': 12, 'mtimeNs': 34, 'sha256': 'a' * 64},
      }),
      throwsFormatException,
    );
    for (final missing in ['signature', 'workbookSafety']) {
      final orphan = Map<String, Object?>.from(validWorksheet)..remove(missing);
      expect(
        () => PrintJob.fromJson(orphan),
        throwsFormatException,
        reason: 'worksheet metadata without $missing must fail',
      );
    }
  });
}

Map<String, Object?> _worksheetJson() =>
    worksheetDescriptorToJson(descriptor('MEC', 1));
