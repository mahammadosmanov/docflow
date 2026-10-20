// Process-guard and localization checks for the Excel pipeline.
// Port of production_excel_failures_are_localization_keys and the
// process-guard tests in src-tauri/src/platform/excel.rs. This file runs in
// its own test isolate, so the process-wide breaker it trips cannot leak into
// other test files.
import 'dart:io';

import 'package:docflow/engine/platform/excel.dart';
import 'package:docflow/engine/platform/excel_print_ports.dart';
import 'package:docflow/engine/progress.dart';
import 'package:flutter_test/flutter_test.dart';

import 'e12_excel_support.dart';

void main() {
  test('production Excel failures are localization keys', () {
    final source = File('lib/engine/platform/excel.dart').readAsStringSync();
    final literal = RegExp(
      r'PrintFailure\.(?:failed|unsupported|unsupportedOptions|uncertain|cancelled)\(\s*'
      r"'([^']+)'",
    );
    final raw = literal
        .allMatches(source)
        .map((match) => match.group(1)!)
        .where((message) => !message.startsWith('error.'))
        .toList();
    expect(raw, isEmpty, reason: 'unlocalized Excel failures: $raw');
  });

  test('process guard acquisitions share lifetime state', () {
    final first = processExcelGuards();
    final second = processExcelGuards();

    expect(identical(first.breaker, second.breaker), isTrue);
    expect(identical(first.operation, second.operation), isTrue);
    expect(identical(first.quarantine, second.quarantine), isTrue);
  });

  test('distinct pipelines sharing guards share the tripped breaker', () async {
    final guards = ExcelProcessGuards();
    final events = <String>[];
    final submitted = <SubmittedPdf>[];
    final tripping = ExcelExportPipeline(
      policy: RecordingPolicy(events),
      exporter: CleanupFailingExporter(events),
      submitter: OutcomeSubmitter(events, PrintFailure.failed('x')),
      guards: guards,
      clock: FixedClock(Duration.zero),
      runtime: ImmediateRuntime(generationDelta: 1),
    );
    final other = ExcelExportPipeline(
      policy: RecordingPolicy(events),
      exporter: RecordingExporter(events, []),
      submitter: RecordingSubmitter(events, submitted),
      guards: guards,
      clock: FixedClock(Duration.zero),
    );
    final fixture = sourceFixture();
    addTearDown(fixture.dispose);
    final job = worksheetJob(fixture.source.absolute, fixture.signature, 'MEC', 1);
    final cancellation = CancellationRegistry().register('shared');

    await expectLater(
      tripping.exportAndSubmit(job, fixture.source, fixture.openSource(), cancellation),
      throwsA(isA<PrintFailure>()),
    );
    expect(guards.breaker.value, isTrue);

    await expectLater(
      other.exportAndSubmit(job, fixture.source, fixture.openSource(), cancellation),
      throwsA(
        isA<PrintFailure>().having(
          (failure) => failure.message,
          'message',
          'error.print.excelExportQuarantined',
        ),
      ),
    );
    expect(submitted, isEmpty);
    tripping.clearTestQuarantine();
  });
}
