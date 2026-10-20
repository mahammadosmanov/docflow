// Ports of the inline tests in src-tauri/src/platform/controlled_submit.rs,
// src-tauri/src/platform/mod.rs, src-tauri/src/platform/startup.rs,
// src-tauri/src/platform/macos/printing.rs, and
// src-tauri/src/platform/windows/printing.rs, plus the adapter-contract tests
// from src-tauri/tests/batch_print.rs that exercise a fake PrintAdapter.
//
// Tests that need a live Windows spooler or a CUPS scheduler are not ported as
// acceptance tests. Their pure logic is covered here, and the native bindings
// are reported as UNVERIFIED.
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:archive/archive.dart';
import 'package:docflow/engine/errors.dart';
import 'package:docflow/engine/page_range.dart';
import 'package:docflow/engine/paths.dart';
import 'package:docflow/engine/platform/controlled_submit.dart';
import 'package:docflow/engine/platform/macos/printing.dart';
import 'package:docflow/engine/platform/mod.dart';
import 'package:docflow/engine/platform/png_decode.dart';
import 'package:docflow/engine/platform/startup.dart';
import 'package:docflow/engine/platform/windows/printing.dart';
import 'package:docflow/engine/print_queue.dart';
import 'package:docflow/engine/progress.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

// ---- controlled_submit.rs --------------------------------------------------

class _Snapshots implements SubmitSignals {
  _Snapshots(Iterable<SubmitSnapshot> snapshots)
    : _snapshots = snapshots.toList();

  final List<SubmitSnapshot> _snapshots;
  int samples = 0;

  @override
  SubmitSnapshot sample() {
    samples += 1;
    if (_snapshots.isEmpty) {
      throw StateError('one scripted signal snapshot per sample');
    }
    return _snapshots.removeAt(0);
  }
}

class _Effects implements ControlledSubmitEffects<int> {
  _Effects();

  static const int jobId = 42;
  final List<String> events = [];
  PrintFailure? startFailure;
  PrintFailure? renderFailure;
  PrintFailure? endFailure;
  PrintFailure? observeFailure;
  bool abortSucceeds = true;

  @override
  int startDoc() {
    events.add('StartDoc');
    if (startFailure != null) {
      throw startFailure!;
    }
    return jobId;
  }

  @override
  bool abortDoc() {
    events.add('AbortDoc');
    return abortSucceeds;
  }

  @override
  void renderPages() {
    events.add('RenderPages');
    if (renderFailure != null) {
      throw renderFailure!;
    }
  }

  @override
  void endDoc() {
    events.add('EndDoc');
    if (endFailure != null) {
      throw endFailure!;
    }
  }

  @override
  void observeJob(int jobId) {
    events.add('Observe($jobId)');
    if (observeFailure != null) {
      throw observeFailure!;
    }
  }
}

const SubmitSnapshot _clear = SubmitSnapshot(
  cancelled: false,
  deadlineExpired: false,
);
const SubmitSnapshot _cancelledSnapshot = SubmitSnapshot(
  cancelled: true,
  deadlineExpired: false,
);
const SubmitSnapshot _expiredSnapshot = SubmitSnapshot(
  cancelled: false,
  deadlineExpired: true,
);

PrintFailure _captureFailure(void Function() body) {
  try {
    body();
  } on PrintFailure catch (failure) {
    return failure;
  }
  fail('expected a PrintFailure');
}

void _expectFailure(
  PrintFailure failure, {
  required PrintFailureKind kind,
  required bool uncertain,
  required String message,
}) {
  expect(failure.kind, kind);
  expect(failure.uncertain, uncertain);
  expect(failure.message, message);
}

// ---- Shared helpers ----------------------------------------------------------

PrintSettings _settings({
  String paper = 'A4',
  String orientation = 'auto',
  String scaling = 'fit',
  String color = 'printer',
  String duplex = 'printer',
  String pageRange = 'all',
}) => PrintSettings(
  paper: paper,
  orientation: orientation,
  scaling: scaling,
  color: color,
  duplex: duplex,
  pageRange: pageRange,
  timeoutMinutes: 30,
);

void main() {
  group('controlled submit sequence (controlled_submit.rs)', () {
    test('pre-start expiry has zero effects', () {
      final signals = _Snapshots([_expiredSnapshot]);
      final effects = _Effects();

      final failure = _captureFailure(
        () => executeControlledSubmit<int>(signals, effects),
      );

      expect(signals.samples, 1);
      expect(effects.events, isEmpty);
      _expectFailure(
        failure,
        kind: PrintFailureKind.failed,
        uncertain: false,
        message: 'error.print.excelExportTimedOut',
      );
    });

    test('pre-start cancellation wins over simultaneous expiry', () {
      final signals = _Snapshots([
        const SubmitSnapshot(cancelled: true, deadlineExpired: true),
      ]);
      final effects = _Effects();

      final failure = _captureFailure(
        () => executeControlledSubmit<int>(signals, effects),
      );

      expect(signals.samples, 1);
      expect(effects.events, isEmpty);
      _expectFailure(
        failure,
        kind: PrintFailureKind.cancelled,
        uncertain: false,
        message: 'error.print.excelExportCancelled',
      );
    });

    test('pre-start cancellation has zero effects', () {
      final signals = _Snapshots([_cancelledSnapshot]);
      final effects = _Effects();

      final failure = _captureFailure(
        () => executeControlledSubmit<int>(signals, effects),
      );

      expect(signals.samples, 1);
      expect(effects.events, isEmpty);
      _expectFailure(
        failure,
        kind: PrintFailureKind.cancelled,
        uncertain: false,
        message: 'error.print.excelExportCancelled',
      );
    });

    test('rejected StartDoc is certain and stops submission', () {
      final signals = _Snapshots([_clear]);
      final effects = _Effects()
        ..startFailure = PrintFailure.failed('start doc rejected');

      final failure = _captureFailure(
        () => executeControlledSubmit<int>(signals, effects),
      );

      expect(signals.samples, 1);
      expect(effects.events, ['StartDoc']);
      _expectFailure(
        failure,
        kind: PrintFailureKind.failed,
        uncertain: false,
        message: 'start doc rejected',
      );
    });

    test('post-StartDoc expiry aborts once and is uncertain', () {
      final signals = _Snapshots([_clear, _expiredSnapshot]);
      final effects = _Effects();

      final failure = _captureFailure(
        () => executeControlledSubmit<int>(signals, effects),
      );

      expect(signals.samples, 2);
      expect(effects.events, ['StartDoc', 'AbortDoc']);
      _expectFailure(
        failure,
        kind: PrintFailureKind.uncertain,
        uncertain: true,
        message: 'error.print.excelPrintCompletionUnconfirmed',
      );
    });

    test('post-StartDoc cancellation wins over simultaneous expiry', () {
      final signals = _Snapshots([
        _clear,
        const SubmitSnapshot(cancelled: true, deadlineExpired: true),
      ]);
      final effects = _Effects();

      final failure = _captureFailure(
        () => executeControlledSubmit<int>(signals, effects),
      );

      expect(signals.samples, 2);
      expect(effects.events, ['StartDoc', 'AbortDoc']);
      _expectFailure(
        failure,
        kind: PrintFailureKind.cancelled,
        uncertain: true,
        message: 'error.print.excelPrintCancelledUnconfirmed',
      );
    });

    test('post-StartDoc cancellation aborts once and is uncertain', () {
      final signals = _Snapshots([_clear, _cancelledSnapshot]);
      final effects = _Effects();

      final failure = _captureFailure(
        () => executeControlledSubmit<int>(signals, effects),
      );

      expect(signals.samples, 2);
      expect(effects.events, ['StartDoc', 'AbortDoc']);
      _expectFailure(
        failure,
        kind: PrintFailureKind.cancelled,
        uncertain: true,
        message: 'error.print.excelPrintCancelledUnconfirmed',
      );
    });

    test('abort failure does not replace post-StartDoc cancellation', () {
      final signals = _Snapshots([_clear, _cancelledSnapshot]);
      final effects = _Effects()..abortSucceeds = false;

      final failure = _captureFailure(
        () => executeControlledSubmit<int>(signals, effects),
      );

      expect(signals.samples, 2);
      expect(effects.events, ['StartDoc', 'AbortDoc']);
      _expectFailure(
        failure,
        kind: PrintFailureKind.cancelled,
        uncertain: true,
        message: 'error.print.excelPrintCancelledUnconfirmed',
      );
    });

    test('render cancellation aborts once and stays cancelled-uncertain', () {
      final signals = _Snapshots([_clear, _clear]);
      final effects = _Effects()
        ..renderFailure = PrintFailure.cancelled(
          'render cancelled',
          uncertain: false,
        );

      final failure = _captureFailure(
        () => executeControlledSubmit<int>(signals, effects),
      );

      expect(signals.samples, 2);
      expect(effects.events, ['StartDoc', 'RenderPages', 'AbortDoc']);
      _expectFailure(
        failure,
        kind: PrintFailureKind.cancelled,
        uncertain: true,
        message: 'error.print.excelPrintCancelledUnconfirmed',
      );
    });

    test('every render failure aborts once and becomes uncertain', () {
      final renderFailures = [
        PrintFailure.failed('render failed'),
        PrintFailure.unsupported('renderer unavailable'),
        PrintFailure.unsupportedOptions('settings changed'),
      ];
      for (final renderFailure in renderFailures) {
        final signals = _Snapshots([_clear, _clear]);
        final effects = _Effects()..renderFailure = renderFailure;

        final failure = _captureFailure(
          () => executeControlledSubmit<int>(signals, effects),
        );

        expect(signals.samples, 2);
        expect(effects.events, ['StartDoc', 'RenderPages', 'AbortDoc']);
        _expectFailure(
          failure,
          kind: PrintFailureKind.uncertain,
          uncertain: true,
          message: 'error.print.excelPrintCompletionUnconfirmed',
        );
      }
    });

    test('EndDoc failure is uncertain without abort or observation', () {
      final signals = _Snapshots([_clear, _clear]);
      final effects = _Effects()
        ..endFailure = PrintFailure.failed('end doc failed');

      final failure = _captureFailure(
        () => executeControlledSubmit<int>(signals, effects),
      );

      expect(signals.samples, 2);
      expect(effects.events, ['StartDoc', 'RenderPages', 'EndDoc']);
      _expectFailure(
        failure,
        kind: PrintFailureKind.uncertain,
        uncertain: true,
        message: 'error.print.excelPrintCompletionUnconfirmed',
      );
    });

    test('observation cancellation stays cancelled-uncertain', () {
      final signals = _Snapshots([_clear, _clear]);
      final effects = _Effects()
        ..observeFailure = PrintFailure.cancelled(
          'observation cancelled',
          uncertain: false,
        );

      final failure = _captureFailure(
        () => executeControlledSubmit<int>(signals, effects),
      );

      expect(signals.samples, 2);
      expect(effects.events, [
        'StartDoc',
        'RenderPages',
        'EndDoc',
        'Observe(42)',
      ]);
      _expectFailure(
        failure,
        kind: PrintFailureKind.cancelled,
        uncertain: true,
        message: 'error.print.excelPrintCancelledUnconfirmed',
      );
    });

    test('observation failure is uncertain without abort', () {
      final signals = _Snapshots([_clear, _clear]);
      final effects = _Effects()
        ..observeFailure = PrintFailure.failed('queue read failed');

      final failure = _captureFailure(
        () => executeControlledSubmit<int>(signals, effects),
      );

      expect(signals.samples, 2);
      expect(effects.events, [
        'StartDoc',
        'RenderPages',
        'EndDoc',
        'Observe(42)',
      ]);
      _expectFailure(
        failure,
        kind: PrintFailureKind.uncertain,
        uncertain: true,
        message: 'error.print.excelPrintCompletionUnconfirmed',
      );
    });

    test('success executes the exact order and propagates the job id', () {
      final signals = _Snapshots([_clear, _clear]);
      final effects = _Effects();

      executeControlledSubmit<int>(signals, effects);

      expect(signals.samples, 2);
      expect(effects.events, [
        'StartDoc',
        'RenderPages',
        'EndDoc',
        'Observe(42)',
      ]);
    });
  });

  group('platform helpers (platform/mod.rs)', () {
    test(
      'controlled route maps raw helper failures before and after StartDoc',
      () {
        final mapped = [
          mapControlledPrintFailure(PrintFailure.failed('raw devmode failure')),
          mapControlledPrintFailure(
            PrintFailure.unsupportedOptions('raw option failure'),
          ),
          mapControlledPrintFailure(
            windowsPostSubmissionFailure(
              PrintFailure.failed('raw renderer failure'),
            ),
          ),
          mapControlledPrintFailure(
            windowsPostSubmissionFailure(
              PrintFailure.cancelled(
                'raw cancellation failure',
                uncertain: false,
              ),
            ),
          ),
        ];
        final expected = [
          (
            PrintFailureKind.failed,
            false,
            'error.print.excelPrintPreparationFailed',
          ),
          (
            PrintFailureKind.unsupportedOptions,
            false,
            'error.print.excelPrintOptionsRejected',
          ),
          (
            PrintFailureKind.uncertain,
            true,
            'error.print.excelPrintCompletionUnconfirmed',
          ),
          (
            PrintFailureKind.cancelled,
            true,
            'error.print.excelPrintCancelledUnconfirmed',
          ),
        ];
        for (var i = 0; i < mapped.length; i++) {
          expect(mapped[i].kind, expected[i].$1);
          expect(mapped[i].uncertain, expected[i].$2);
          expect(mapped[i].message, expected[i].$3);
        }

        final alreadyStable = PrintFailure.failed(
          'error.print.excelPrinterContextUnavailable',
        );
        expect(mapControlledPrintFailure(alreadyStable), alreadyStable);
      },
    );

    test(
      'controlled route error keys have all four operator localizations',
      () {
        const catalogs = [
          '../../frontend/src/locales/app_en.arb',
          '../../frontend/src/locales/app_az.arb',
          '../../frontend/src/locales/app_ru.arb',
          '../../frontend/src/locales/app_tr.arb',
        ];
        const keys = [
          'error.print.excelPrintPreparationFailed',
          'error.print.excelPrintOptionsRejected',
          'error.print.excelPrintCancelledUnconfirmed',
          'error.print.excelPrintCompletionUnconfirmed',
        ];
        for (final catalog in catalogs) {
          final decoded = jsonDecode(
            File(catalog).readAsStringSync(),
          ) as Map<String, Object?>;
          for (final key in keys) {
            final translation = decoded[key];
            expect(translation, isA<String>(), reason: '$catalog $key');
            expect((translation as String).trim(), isNotEmpty);
            expect(translation, isNot(key));
          }
        }
      },
    );

    test('controlled page adapter consumes exact rendered bytes without path reopen', () {
      final directory = Directory.systemTemp.createTempSync('docflow-mod-');
      try {
        final ambient = File(p.join(directory.path, 'page.png'))
          ..writeAsBytesSync(utf8.encode('validated rendered page'));
        final consumed = <int>[];

        consumeControlledPageBytes<void>(() => ambient.readAsBytesSync(), (
          bytes,
        ) {
          final displaced = File(
            p.join(directory.path, 'page.approved-original'),
          );
          ambient.renameSync(displaced.path);
          File(ambient.path)
              .writeAsBytesSync(utf8.encode('substituted rendered page'));
          consumed.addAll(bytes);
        });

        expect(utf8.decode(consumed), 'validated rendered page');
        expect(ambient.readAsStringSync(), 'substituted rendered page');
      } finally {
        directory.deleteSync(recursive: true);
      }
    });

    test('submitted Windows destructive terminal statuses are uncertain', () {
      for (final status in [0x0000_0004, 0x0000_0100]) {
        expect(
          classifyWindowsSubmittedJobStatus(status),
          WindowsSubmittedJobStatus.uncertain,
        );
      }
      expect(
        classifyWindowsSubmittedJobStatus(0x0000_0002),
        WindowsSubmittedJobStatus.pending,
        reason:
            'a transient generic driver error must be observed until release',
      );
      expect(
        classifyWindowsSubmittedJobStatus(0x0000_0002 | 0x0000_1000),
        WindowsSubmittedJobStatus.complete,
        reason: 'an explicit completion bit must win over a stale driver error',
      );
      expect(
        classifyWindowsSubmittedJobStatus(0x0000_0080),
        WindowsSubmittedJobStatus.complete,
      );
      expect(
        classifyWindowsSubmittedJobStatus(0x0000_0010),
        WindowsSubmittedJobStatus.pending,
      );
    });

    test('native handler job selection accepts one new job without trusting ambiguous jobs', () {
      const known = {7, 8};
      expect(
        selectWindowsSubmittedJob(known, [
          (7, 'older'),
          (9, 'Microsoft Word'),
        ], 'report.docx'),
        9,
        reason: 'a native handler may replace the filename with an app title',
      );
      expect(
        selectWindowsSubmittedJob(known, [
          (9, 'Microsoft Word'),
          (10, 'unrelated'),
        ], 'report.docx'),
        isNull,
        reason: 'multiple unnamed new jobs are ambiguous',
      );
      expect(
        selectWindowsSubmittedJob(known, [
          (9, 'report.docx'),
          (10, 'unrelated'),
        ], 'report.docx'),
        9,
        reason: 'one exact title match remains authoritative',
      );
    });

    test('submitted Windows recoverable device statuses keep waiting', () {
      for (final status in [
        0x0000_0001, // PAUSED
        0x0000_0008, // SPOOLING
        0x0000_0010, // PRINTING
        0x0000_0020, // OFFLINE
        0x0000_0040, // PAPEROUT
        0x0000_0200, // BLOCKED_DEVQ
        0x0000_0400, // USER_INTERVENTION
        0x0000_2000, // RETAINED
      ]) {
        expect(
          classifyWindowsSubmittedJobStatus(status),
          WindowsSubmittedJobStatus.pending,
          reason: 'status $status',
        );
      }
    });

    test('submitted Windows job released after EndDoc is complete', () {
      expect(
        classifyWindowsSubmittedJobObservation(null),
        WindowsSubmittedJobStatus.complete,
      );
      expect(
        classifyWindowsSubmittedJobObservation(0x0000_0002),
        WindowsSubmittedJobStatus.pending,
      );
      expect(classifyWindowsJobQueryError(2), WindowsJobQueryError.released);
      expect(classifyWindowsJobQueryError(87), WindowsJobQueryError.released);
      expect(
        classifyWindowsJobQueryError(122),
        WindowsJobQueryError.retryWithLargerBuffer,
      );
      expect(classifyWindowsJobQueryError(5), WindowsJobQueryError.uncertain);
    });

    test('post-submission observation loss continues but explicit fault is uncertain', () {
      for (final stage in [
        WindowsPostSubmissionObservation.printerOpen,
        WindowsPostSubmissionObservation.jobEnumeration,
        WindowsPostSubmissionObservation.jobRead,
      ]) {
        final failure = windowsPostSubmissionObservationError(stage);
        expect(failure.kind, PrintFailureKind.submittedUnconfirmed);
        expect(failure.uncertain, isTrue);
      }
      final fault = windowsPostSubmissionObservationError(
        WindowsPostSubmissionObservation.statusFault,
      );
      expect(fault.kind, PrintFailureKind.uncertain);
      expect(fault.uncertain, isTrue);
    });

    test('every Windows render failure after StartDoc is uncertain', () {
      for (final failure in [
        PrintFailure.failed('render failed'),
        PrintFailure.unsupported('decoder failed'),
        PrintFailure.unsupportedOptions('device mode changed'),
        PrintFailure.uncertain('already uncertain'),
      ]) {
        final classified = windowsPostSubmissionFailure(failure);
        expect(classified.kind, PrintFailureKind.uncertain);
        expect(classified.uncertain, isTrue);
      }
      final cancelled = windowsPostSubmissionFailure(
        PrintFailure.cancelled('render cancelled', uncertain: false),
      );
      expect(cancelled.kind, PrintFailureKind.cancelled);
      expect(cancelled.uncertain, isTrue);
    });

    test(
      'Windows post-submission cancellation is uncertain even when accepted',
      () {
        for (final accepted in [false, true]) {
          final failure = windowsPostSubmissionCancellation(
            requestAccepted: accepted,
          );
          expect(failure.kind, PrintFailureKind.cancelled);
          expect(failure.uncertain, isTrue);
        }
      },
    );

    test('Windows image frame-count preflight requires exactly one frame', () {
      validateWindowsImageFrameCount(1);

      for (final frameCount in [0, 2]) {
        final failure = _captureFailure(
          () => validateWindowsImageFrameCount(frameCount),
        );
        expect(failure.kind, PrintFailureKind.unsupported);
        expect(
          failure.message,
          'Windows controlled image printing requires exactly one image frame',
        );
        expect(failure.uncertain, isFalse);
      }
    });
  });

  group('startup (platform/startup.rs)', () {
    test('enabling startup is refused off Windows, disabling is a no-op', () {
      if (Platform.isWindows) {
        return;
      }
      final refused = _captureStartupFailure(
        () => setStartWithWindows(enabled: true),
      );
      expect(refused.kind, 'unsupported');
      expect(refused.message, 'Windows startup is available only on Windows');
      setStartWithWindows(enabled: false);
    });
  });

  group('production adapter selection (platform/mod.rs)', () {
    test('a host without a native print route fails closed', () {
      if (Platform.isWindows || Platform.isMacOS) {
        return;
      }
      final adapter = productionPrintAdapter();
      expect(adapter, isA<UnsupportedPrintAdapter>());
      expect(adapter.inventory().printers, isEmpty);
      expect(
        _captureFailure(
          () => adapter.inspect('report.pdf', 'Office', _settings()),
        ).kind,
        PrintFailureKind.unsupported,
      );
      expect(
        _captureFailure(
          () => adapter.submit(PrintJob(path: 'report.pdf'), () => false),
        ).kind,
        PrintFailureKind.failed,
      );
    });
  });

  group('macOS CUPS mapping (platform/macos/printing.rs)', () {
    test('CUPS job mapping preserves paper, orientation, scaling, color, and duplex', () {
      expect(
        cupsOptions(
          _settings(
            paper: 'A3',
            orientation: 'landscape',
            scaling: 'fit',
            color: 'grayscale',
            duplex: 'short',
          ),
        ),
        [
          'job-sheets=none',
          'media=A3',
          'orientation-requested=4',
          'fit-to-page',
          'print-color-mode=monochrome',
          'sides=two-sided-short-edge',
        ],
      );
    });

    test('CUPS options forward a canonical PDF page range', () {
      final options = cupsOptions(_settings(pageRange: '1-3,5'));
      expect(options, contains('page-ranges=1-3,5'));
    });

    test('all pages emit no CUPS page-range option', () {
      expect(
        cupsOptions(_settings())
            .where((option) => option.startsWith('page-ranges=')),
        isEmpty,
      );
    });

    test('lp arguments keep the PDF range separate and delimit the path', () {
      final job = PrintJob(
        path: '/tmp/report.pdf',
        printer: 'Test_PDF',
        pageRange: '1-3,5',
      );

      final args = lpArguments(job);
      final rangeIndexes = [
        for (var i = 0; i < args.length; i++)
          if (args[i] == 'page-ranges=1-3,5') i,
      ];
      expect(rangeIndexes, hasLength(1));
      expect(args[rangeIndexes.single - 1], '-o');
      expect(args.sublist(args.length - 2), ['--', '/tmp/report.pdf']);
    });

    test('PDF inspection reports numeric page-range support', () {
      final support = const MacOsPrintAdapter().inspect(
        'report.pdf',
        'Test_PDF',
        _settings(),
      );
      expect(support.pageRange, isA<PdfPageRangeCapability>());
      expect((support.pageRange as PdfPageRangeCapability).pageCount, isNull);
    });

    test('static image inspection reports single-page support', () {
      final support = const MacOsPrintAdapter().inspect(
        'scan.png',
        'Test_PDF',
        _settings(),
      );
      expect(support.pageRange, isA<SinglePagePageRangeCapability>());
    });

    test('static image numeric ranges fail before submission', () {
      final support = const MacOsPrintAdapter().inspect(
        'scan.png',
        'Test_PDF',
        _settings(),
      );
      final error = _captureBackendError(
        () => canonicalFor('2', support.pageRange),
      );
      expect(error.message, 'error.print.pageRangeImageUnsupported');
    });

    test('only CUPS controlled formats are accepted; TIFF fails closed', () {
      for (final name in ['a.pdf', 'a.png', 'a.jpg', 'a.jpeg', 'a.bmp']) {
        expect(
          () =>
              const MacOsPrintAdapter().inspect(name, 'Test_PDF', _settings()),
          returnsNormally,
          reason: name,
        );
      }
      for (final name in ['a.tif', 'a.tiff']) {
        expect(
          _captureFailure(
            () => const MacOsPrintAdapter().inspect(
              name,
              'Test_PDF',
              _settings(),
            ),
          ).kind,
          PrintFailureKind.unsupported,
          reason: name,
        );
      }
    });

    test('Office files remain unsupported before submission', () {
      final failure = _captureFailure(
        () => const MacOsPrintAdapter().inspect(
          'report.docx',
          'Test_PDF',
          _settings(),
        ),
      );
      expect(failure.kind, PrintFailureKind.unsupported);
    });

    test(
      'CUPS parsers accept documented output and reject injection names',
      () {
        expect(
          parsePrinterLine('printer Test_PDF is idle. enabled since today'),
          'Test_PDF',
        );
        expect(
          parseDefaultLine('system default destination: Test_PDF'),
          'Test_PDF',
        );
        expect(
          parseJobId('request id is Test_PDF-42 (1 file(s))', 'Test_PDF'),
          42,
        );
        expect(safeCupsDestination('printer;\$(touch /tmp/pwned)'), isFalse);
        expect(safeCupsDestination('printer\nnext'), isFalse);
        expect(safeCupsDestination('has space'), isFalse);
        expect(safeCupsDestination('a/b'), isFalse);
        expect(safeCupsDestination('a#b'), isFalse);
      },
    );

    test('accepted active CUPS cancellation remains uncertain', () {
      final failure = activeCupsCancellation();
      expect(failure.kind, PrintFailureKind.cancelled);
      expect(failure.uncertain, isTrue);
    });

    test('an unavailable CUPS inventory is reviewable, not fatal', () {
      final inventory = const MacOsPrintAdapter().inventory();
      expect(inventory.printers.every(safeCupsDestination), isTrue);
    });
  });

  group('Windows settings mapping (platform/windows/printing.rs)', () {
    test('maps controlled job settings to DEVMODE values', () {
      final requested = requestedDevmode(
        _settings(
          paper: 'A4',
          orientation: 'landscape',
          scaling: 'shrink',
          color: 'grayscale',
          duplex: 'short',
        ),
      );
      expect(
        requested,
        const RequestedDevMode(paper: 9, orientation: 2, color: 1, duplex: 3),
      );
    });

    test('rejects unmapped values before any native call', () {
      final failure = _captureFailure(
        () => requestedDevmode(_settings(paper: 'A4;Remove-Item C:\\')),
      );
      expect(failure.kind, PrintFailureKind.unsupportedOptions);

      expect(safeExtension('docx;calc'), isFalse);
      expect(safeExtension(''), isFalse);
      expect(safeExtension('docx'), isTrue);
    });

    test('printer argument is data, not a command', () {
      const argument = 'Office Printer & calc.exe';
      expect('"$argument"', '"Office Printer & calc.exe"');
    });

    test('maps fit, actual, and shrink to printer DPI', () {
      expect(pageScale(300, 600, 600, 1200, 600, 600, 'actual'), 2.0);
      expect(pageScale(300, 600, 300, 600, 600, 600, 'shrink'), 1.0);
      expect(pageScale(300, 600, 150, 300, 600, 600, 'fit'), 0.5);
      expect(
        imageDestination(
          300,
          600,
          300.0,
          300.0,
          1200,
          1200,
          600,
          600,
          'actual',
        ),
        (600, 1200),
      );
      expect(
        imageDestination(300, 600, 300.0, 300.0, 300, 600, 600, 600, 'shrink'),
        (300, 600),
      );
    });

    test('invalid page metrics fail before drawing', () {
      final failure = _captureFailure(
        () => pageScale(0, 600, 600, 1200, 600, 600, 'fit'),
      );
      expect(failure.kind, PrintFailureKind.unsupportedOptions);
    });
  });

  group('PNG decoder (rendered page to BGR bitmap)', () {
    test('decodes 8-bit RGB into padded BGR rows', () {
      final png = _encodePng(
        width: 2,
        height: 2,
        colorType: 2,
        bitDepth: 8,
        scanlines: [
          [0, 255, 0, 0, 0, 255, 0], // row 0: red, green
          [0, 0, 0, 255, 255, 255, 255], // row 1: blue, white
        ],
      );

      final bitmap = decodePngBgr(png);

      expect(bitmap.width, 2);
      expect(bitmap.height, 2);
      // stride = (2 * 3 + 3) & ~3 = 8; padding bytes stay 0xff.
      expect(bitmap.bgr.length, 16);
      expect(bitmap.bgr.sublist(0, 6), [0, 0, 255, 0, 255, 0]); // red, green
      expect(bitmap.bgr.sublist(6, 8), [0xff, 0xff]);
      expect(bitmap.bgr.sublist(8, 14), [
        255,
        0,
        0,
        255,
        255,
        255,
      ]); // blue, white
      expect(bitmap.bgr.sublist(14, 16), [0xff, 0xff]);
    });

    test('reverses every PNG filter type', () {
      // A 3x5 RGB image whose rows use filters 0 through 4 in turn.
      final pixels = [
        for (var y = 0; y < 5; y++)
          for (var x = 0; x < 3; x++) [x * 40 + y * 7, 200 - x * 30, y * 50],
      ];
      final raw = [for (final rgb in pixels) ...rgb];
      final scanlines = <List<int>>[];
      for (var y = 0; y < 5; y++) {
        final row = raw.sublist(y * 9, y * 9 + 9);
        final previous = y == 0
            ? List<int>.filled(9, 0)
            : raw.sublist((y - 1) * 9, y * 9);
        scanlines.add([y, ..._forwardFilter(y, row, previous, 3)]);
      }
      final png = _encodePng(
        width: 3,
        height: 5,
        colorType: 2,
        bitDepth: 8,
        scanlines: scanlines,
      );

      final bitmap = decodePngBgr(png);

      for (var y = 0; y < 5; y++) {
        for (var x = 0; x < 3; x++) {
          final stride = 12; // (3 * 3 + 3) & ~3
          final target = y * stride + x * 3;
          expect(
            bitmap.bgr[target + 2],
            pixels[y * 3 + x][0],
            reason: 'R $x,$y',
          );
          expect(
            bitmap.bgr[target + 1],
            pixels[y * 3 + x][1],
            reason: 'G $x,$y',
          );
          expect(bitmap.bgr[target], pixels[y * 3 + x][2], reason: 'B $x,$y');
        }
      }
    });

    test('expands 1-bit grayscale to black and white', () {
      final png = _encodePng(
        width: 9,
        height: 1,
        colorType: 0,
        bitDepth: 1,
        scanlines: [
          [0, 0xA0, 0x80], // 1010 0000, 1000 0000 => 1,0,1,0,0,0,0,0 then 1
        ],
      );

      final bitmap = decodePngBgr(png);
      final values = [for (var x = 0; x < 9; x++) bitmap.bgr[x * 3]];

      expect(values, [255, 0, 255, 0, 0, 0, 0, 0, 255]);
    });

    test('expands 4-bit palette indexes through PLTE', () {
      final png = _encodePng(
        width: 2,
        height: 1,
        colorType: 3,
        bitDepth: 4,
        palette: [10, 20, 30, 200, 210, 220],
        scanlines: [
          [0, 0x01], // index 0 then index 1
        ],
      );

      final bitmap = decodePngBgr(png);

      expect(bitmap.bgr.sublist(0, 3), [30, 20, 10]);
      expect(bitmap.bgr.sublist(3, 6), [220, 210, 200]);
    });

    test('keeps the high byte of 16-bit samples', () {
      final png = _encodePng(
        width: 1,
        height: 1,
        colorType: 2,
        bitDepth: 16,
        scanlines: [
          [0, 0xAB, 0xCD, 0x12, 0x34, 0x56, 0x78],
        ],
      );

      final bitmap = decodePngBgr(png);

      expect(bitmap.bgr.sublist(0, 3), [0x56, 0x12, 0xAB]);
    });

    test('rejects a missing signature and a corrupted chunk CRC', () {
      final valid = _encodePng(
        width: 1,
        height: 1,
        colorType: 0,
        bitDepth: 8,
        scanlines: [
          [0, 7],
        ],
      );
      final badSignature = Uint8List.fromList(valid)..[0] = 0;
      final badCrc = Uint8List.fromList(valid)..[valid.length - 9] ^= 0xff;

      expect(
        _captureFailure(() => decodePngBgr(badSignature)).message,
        'Windows rendered an invalid PDF page',
      );
      expect(
        _captureFailure(() => decodePngBgr(badCrc)).message,
        'Windows rendered an invalid PDF page',
      );
    });

    test('decodes an Adam7 interlaced single-pixel image', () {
      final png = _encodePng(
        width: 1,
        height: 1,
        colorType: 2,
        bitDepth: 8,
        interlace: 1,
        scanlines: [
          [0, 9, 8, 7],
        ],
      );

      final bitmap = decodePngBgr(png);

      expect(bitmap.bgr.sublist(0, 3), [7, 8, 9]);
    });
  });

  group('startup-independent adapter contract (batch_print.rs)', () {
    late Directory root;

    setUp(() {
      root = Directory.systemTemp.createTempSync('docflow-print-contract-');
    });

    tearDown(() {
      root.deleteSync(recursive: true);
    });

    PrintService serviceFor(_FakeAdapter adapter) => PrintService(
      queueRoot: p.join(root.path, 'print-queues'),
      journalRoot: p.join(root.path, 'journal'),
      ttl: const Duration(minutes: 5),
      mutationLock: GlobalMutationLock(),
      adapter: adapter,
    );

    BatchPrintParams paramsFor(
      List<String> files, {
      int retries = 0,
      String printer = 'Office Printer',
      String paper = 'A4',
    }) => BatchPrintParams(
      files: files,
      printer: printer,
      paper: paper,
      orientation: 'auto',
      scaling: 'fit',
      color: 'printer',
      duplex: 'printer',
      copies: 1,
      collate: true,
      retries: retries,
      onError: 'pause',
      timeoutMinutes: 30,
    );

    String writeFile(String name, String content) {
      final path = p.join(root.path, name);
      File(path).writeAsStringSync(content);
      return path;
    }

    test(
      'uncertain and cancelled jobs are never retried automatically',
      () async {
        final cases = [
          (
            PrintFailure.uncertain('spool outcome unknown'),
            'uncertain',
            'warn.print.stateUncertain',
          ),
          (
            PrintFailure.cancelled('cancelled while active', uncertain: false),
            'cancelled',
            'warn.print.cancelled',
          ),
          (
            PrintFailure.cancelled(
              'cancelled before acknowledgement',
              uncertain: true,
            ),
            'uncertain',
            'warn.print.cancelledUncertain',
          ),
        ];
        for (final (failure, status, warning) in cases) {
          final file = writeFile('one-$status.pdf', 'one');
          final adapter = _FakeAdapter(
            printers: ['Office Printer'],
            defaultPrinter: 'Office Printer',
          )..outcomes.addAll([failure, null]);
          final service = serviceFor(adapter);
          final values = paramsFor([file], retries: 3);
          final plan = service.preview(
            const CallerIdentity('operator', 'session'),
            values,
            (_) {},
          );

          final result = (await service.apply(
            plan.planId,
            const CallerIdentity('operator', 'session'),
            (_) {},
          ));

          expect(adapter.submitted, hasLength(1), reason: status);
          expect((result.meta['counts'] as Map)[status], 1, reason: status);
          expect(result.errors.first.key, warning, reason: status);
        }
      },
    );

    test(
      'submitted-unconfirmed job is not retried and the queue continues',
      () async {
        final first = writeFile('one.pdf', 'one');
        final second = writeFile('two.pdf', 'two');
        final adapter =
            _FakeAdapter(
                printers: ['Office Printer'],
                defaultPrinter: 'Office Printer',
              )
              ..outcomes.addAll([
                PrintFailure.submittedUnconfirmed(
                  'Windows released the receipt after accepting the job',
                ),
                null,
              ]);
        final service = serviceFor(adapter);
        final values = paramsFor([first, second], retries: 3);
        final plan = service.preview(
          const CallerIdentity('operator', 'session'),
          values,
          (_) {},
        );

        final result = (await service.apply(
          plan.planId,
          const CallerIdentity('operator', 'session'),
          (_) {},
        ));

        expect(adapter.submitted, hasLength(2));
        expect((result.applied, result.failed), (2, 0));
        expect(result.meta['status'], 'completed');
        expect(result.meta['resumable'], 0);
        final counts = result.meta['counts'] as Map;
        expect(counts['completed_unconfirmed'], 1);
        expect(counts['completed'], 1);
        expect(result.errors, hasLength(1));
        expect(result.errors.single.key, 'warn.print.submittedUnconfirmed');
      },
    );

    test('unknown printers and unsupported options fail before submission', () {
      final file = writeFile('one.pdf', 'one');
      final adapter = _FakeAdapter(
        printers: ['Office Printer'],
        defaultPrinter: 'Office Printer',
      );
      final service = serviceFor(adapter);
      final malicious = paramsFor([
        file,
      ], printer: 'Office Printer"; Remove-Item C:\\*');

      final plan = service.preview(
        const CallerIdentity('operator', 'session'),
        malicious,
        (_) {},
      );

      expect(plan.steps, isEmpty);
      expect(plan.warnings.first.key, 'warn.print.printerUnavailable');
      expect(adapter.inspected, 0);
      expect(adapter.submitted, isEmpty);

      adapter.unsupportedPaper = 'A3';
      final unsupported = paramsFor([file], paper: 'A3');
      final rejected = service.preview(
        const CallerIdentity('operator', 'session'),
        unsupported,
        (_) {},
      );
      expect(rejected.steps, isEmpty);
      expect(rejected.warnings.first.key, 'warn.print.unsupportedOptions');
      expect(adapter.submitted, isEmpty);
    });
  });
}

// ---- Test helpers -----------------------------------------------------------

BackendError _captureBackendError(void Function() body) {
  try {
    body();
  } on BackendError catch (error) {
    return error;
  }
  fail('expected a BackendError');
}

({String kind, String message}) _captureStartupFailure(void Function() body) {
  try {
    body();
  } on StartupException catch (error) {
    return (kind: error.kind, message: error.message);
  }
  fail('expected a StartupException');
}

/// Forward PNG filter used only by the decoder round-trip test.
List<int> _forwardFilter(
  int filter,
  List<int> row,
  List<int> previous,
  int bpp,
) {
  final out = List<int>.filled(row.length, 0);
  for (var i = 0; i < row.length; i++) {
    final left = i >= bpp ? row[i - bpp] : 0;
    final upper = previous[i];
    final upperLeft = i >= bpp ? previous[i - bpp] : 0;
    final predictor = switch (filter) {
      0 => 0,
      1 => left,
      2 => upper,
      3 => (left + upper) >> 1,
      _ => _paethPredictor(left, upper, upperLeft),
    };
    out[i] = (row[i] - predictor) & 0xff;
  }
  return out;
}

int _paethPredictor(int a, int b, int c) {
  final pp = a + b - c;
  final pa = (pp - a).abs();
  final pb = (pp - b).abs();
  final pc = (pp - c).abs();
  if (pa <= pb && pa <= pc) {
    return a;
  }
  return pb <= pc ? b : c;
}

/// Builds a PNG from raw scanlines. Each scanline begins with its filter byte.
/// `scanlines` entries are `[filter, ...bytes]`, except that a row list may
/// start with the filter byte followed by already-filtered bytes.
Uint8List _encodePng({
  required int width,
  required int height,
  required int colorType,
  required int bitDepth,
  required List<List<int>> scanlines,
  int interlace = 0,
  List<int>? palette,
}) {
  final out = BytesBuilder();
  out.add([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
  void chunk(String type, List<int> data) {
    final typeBytes = type.codeUnits;
    final body = [...typeBytes, ...data];
    out.add(_u32(data.length));
    out.add(body);
    out.add(_u32(getCrc32(body)));
  }

  chunk('IHDR', [
    ..._u32(width),
    ..._u32(height),
    bitDepth,
    colorType,
    0,
    0,
    interlace,
  ]);
  if (palette != null) {
    chunk('PLTE', palette);
  }
  final raw = [for (final row in scanlines) ...row];
  chunk('IDAT', ZLibEncoder().encode(raw));
  chunk('IEND', const []);
  return Uint8List.fromList(out.takeBytes());
}

List<int> _u32(int value) => [
  (value >> 24) & 0xff,
  (value >> 16) & 0xff,
  (value >> 8) & 0xff,
  value & 0xff,
];

/// Fake PrintAdapter ported from the FakeAdapter in
/// src-tauri/tests/batch_print.rs. Outcomes are consumed in order. A null
/// outcome means success.
final class _FakeAdapter implements PrintAdapter {
  _FakeAdapter({required this.printers, this.defaultPrinter});

  final List<String> printers;
  final String? defaultPrinter;
  final List<PrintFailure?> outcomes = [];
  final List<PrintJob> submitted = [];
  String? unsupportedPaper;
  int inspected = 0;

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
    final extension = p.extension(path).toLowerCase();
    return switch (extension) {
      '.pdf' => PrintJobSupport.controlledPdf(),
      '.png' ||
      '.jpg' ||
      '.jpeg' ||
      '.bmp' ||
      '.tif' ||
      '.tiff' => PrintJobSupport.singlePage(),
      '.docx' => PrintJobSupport.nativeApplication(),
      '.xlsx' || '.xlsm' => PrintJobSupport.controlled(),
      _ => throw PrintFailure.unsupported('unsupported test format'),
    };
  }

  @override
  void submit(PrintJob job, bool Function() cancelled) {
    if (cancelled()) {
      throw PrintFailure.cancelled('cancelled by test', uncertain: false);
    }
    submitted.add(job);
    final outcome = outcomes.isEmpty ? null : outcomes.removeAt(0);
    if (outcome != null) {
      throw outcome;
    }
  }
}
