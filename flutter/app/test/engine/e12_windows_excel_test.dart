// Verifies the platform-neutral COM plan executor and WindowsExcelExporter
// against a fake IDispatch backend. The real Win32 backend is UNVERIFIED on
// Linux (see lib/engine/platform/windows/excel.dart).
import 'dart:io';

import 'package:docflow/engine/cap_fs.dart';
import 'package:docflow/engine/platform/excel.dart';
import 'package:docflow/engine/platform/excel_print_ports.dart';
import 'package:docflow/engine/platform/excel_windows_contract.dart';
import 'package:docflow/engine/platform/windows/excel.dart';
import 'package:docflow/engine/ooxml.dart';
import 'package:docflow/engine/progress.dart';
import 'package:flutter_test/flutter_test.dart';

import 'e12_excel_support.dart';

class _FakeDispatch implements ComDispatch {
  _FakeDispatch(this.name, this.backend);

  final String name;
  final _FakeBackend backend;
  bool released = false;

  @override
  void release() {
    released = true;
    backend.calls.add('release:$name');
  }
}

class _FakeBackend implements ComDispatchBackend {
  _FakeBackend({this.failInvoke});

  final calls = <String>[];
  final String? failInvoke;
  final dispatches = <_FakeDispatch>[];

  @override
  void initializeSta() => calls.add('init');

  @override
  void uninitialize() => calls.add('uninit');

  @override
  ComDispatch createApplication(String progId) {
    calls.add('create:$progId');
    final dispatch = _FakeDispatch('Application', this);
    dispatches.add(dispatch);
    return dispatch;
  }

  @override
  ComDispatch getActiveObject(String progId) {
    calls.add('active:$progId');
    return _FakeDispatch('Application', this);
  }

  @override
  ComResult invoke(
    ComDispatch target,
    String name,
    DispatchInvokeKind kind,
    List<EncodedVariant> arguments,
    List<int> namedDispids,
  ) {
    final owner = (target as _FakeDispatch).name;
    calls.add('$name:${kind.name}:$owner');
    if (name == failInvoke) {
      throw const ComBackendFailure('method:fake', 0x800A03EC);
    }
    switch (name) {
      case 'Workbooks':
      case 'Worksheets':
      case 'Open':
      case 'Item':
        final dispatch = _FakeDispatch(name, this);
        dispatches.add(dispatch);
        return dispatch;
      case 'Name':
        return 'MEC';
      case 'Visible':
        return -1;
      case 'ExportAsFixedFormat':
        final pdfArg = arguments[7] as EncodedString;
        File(pdfArg.value).writeAsBytesSync(fixturePdfBytes(2));
        return true;
      default:
        return true;
    }
  }
}

ComPlanRequest _request(String workbook, String pdf) => ComPlanRequest(
  workbookPath: workbook,
  pdfPath: pdf,
  workbookIndex: 1,
  worksheetName: 'MEC',
  worksheetVisibility: WorksheetVisibility.visible,
);

void main() {
  test('plan executor runs the locked-down sequence and tears down cleanly', () {
    final backend = _FakeBackend();
    final session = ExcelDispatchSession(backend);
    final plan = buildComOperationPlan(
      _request(r'C:\private\book.xlsx', r'C:\private\sheet.pdf'),
    );
    final executor = ExcelPlanExecutor(
      session,
      () => Duration.zero,
      const Duration(seconds: 60),
      CancellationRegistry().register('plan'),
    );

    executor.execute(plan);

    expect(backend.calls.first, 'init');
    expect(backend.calls[1], 'create:Excel.Application');
    expect(backend.calls.indexOf('Open:method:Workbooks') > 0, isTrue);
    expect(backend.calls.last, 'uninit');
    expect(session.application, isNull);
    expect(session.workbook, isNull);
  });

  test('a failing Open is reported as a COM failure with a diagnostic stage', () {
    final backend = _FakeBackend(failInvoke: 'Workbooks');
    final session = ExcelDispatchSession(backend);
    final executor = ExcelPlanExecutor(
      session,
      () => Duration.zero,
      const Duration(seconds: 60),
      CancellationRegistry().register('plan'),
    );
    final plan = buildComOperationPlan(
      _request(r'C:\private\book.xlsx', r'C:\private\sheet.pdf'),
    );

    expect(
      () => executor.execute(plan),
      throwsA(
        isA<PrintFailure>().having(
          (failure) => failure.message,
          'message',
          'error.print.excelComFailed',
        ),
      ),
    );
  });

  test('cancellation before the first operation performs no COM calls', () {
    final backend = _FakeBackend();
    final session = ExcelDispatchSession(backend);
    final cancellations = CancellationRegistry();
    final token = cancellations.register('cancelled');
    cancellations.cancel('cancelled');
    final executor = ExcelPlanExecutor(
      session,
      () => Duration.zero,
      const Duration(seconds: 60),
      token,
    );

    expect(
      () => executor.execute(
        buildComOperationPlan(
          _request(r'C:\private\book.xlsx', r'C:\private\sheet.pdf'),
        ),
      ),
      throwsA(isA<PrintFailure>()),
    );
    expect(backend.calls, isEmpty);
  });

  test('exporter produces a validated PDF through the fake COM backend', () async {
    final fixture = sourceFixture();
    addTearDown(fixture.dispose);
    final backend = _FakeBackend();
    final exporter = WindowsExcelExporter(
      backendFactory: () => backend,
      clock: () => Duration.zero,
    );
    final pipeline = ExcelExportPipeline(
      policy: const _AllowAll(),
      exporter: exporter,
      submitter: _AcceptSubmitter(),
      guards: ExcelProcessGuards(),
      clock: FixedClock(Duration.zero),
    );

    await pipeline.exportAndSubmit(
      worksheetJob(fixture.source.absolute, fixture.signature, 'MEC', 1),
      fixture.source,
      fixture.openSource(),
      CancellationRegistry().register('windows-fake'),
    );

    expect(backend.calls.contains('ExportAsFixedFormat:method:Item'), isTrue);
    expect(backend.calls.contains('Close:method:Open'), isTrue);
    expect(backend.calls.contains('Quit:method:Application'), isTrue);
    expect(backend.calls.last, 'uninit');
  });
}

class _AllowAll implements WorkbookSourcePolicy {
  const _AllowAll();

  @override
  void validateSource(AnchoredPath source, WorkbookSafetyFacts safety) {}
}

class _AcceptSubmitter implements ControlledPdfSubmitter {
  @override
  Future<void> submitPdf(
    PrintJob job,
    ExportedWorksheetPdf pdf,
    CancellationToken cancellation,
    Duration deadline,
  ) async {
    pdf.validatedBytes();
  }
}
