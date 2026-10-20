// Port of the test module in src-tauri/src/platform/excel_windows_contract.rs.
import 'dart:convert';
import 'dart:typed_data';

import 'package:docflow/engine/cap_fs.dart';
import 'package:docflow/engine/ooxml.dart';
import 'package:docflow/engine/platform/excel_print_ports.dart';
import 'package:docflow/engine/platform/excel_windows_contract.dart';
import 'package:flutter_test/flutter_test.dart';
import 'dart:io';

Uint8List bytesOf(String text) => Uint8List.fromList(utf8.encode(text));

class _FakeZoneReader implements ZoneIdentifierReader {
  _FakeZoneReader(this.read);

  final Object? read; // null = missing, Uint8List = bytes, Exception = error

  @override
  Uint8List? readZoneIdentifier(AnchoredPath source) {
    final value = read;
    if (value is Exception) {
      throw value;
    }
    return value as Uint8List?;
  }
}

ComPlanRequest validRequest() => const ComPlanRequest(
  workbookPath: r'C:\private\source.xlsx',
  pdfPath: r'C:\private\sheet.pdf',
  workbookIndex: 1,
  worksheetName: 'MEC',
  worksheetVisibility: WorksheetVisibility.visible,
);

void main() {
  test('safe zone identifiers are classified', () {
    for (final (zoneId, expected) in [
      (0, ZoneClassification.localMachine),
      (1, ZoneClassification.localIntranet),
      (2, ZoneClassification.trusted),
    ]) {
      final contents = bytesOf('[ZoneTransfer]\r\nZoneId=$zoneId\r\n');
      expect(parseZoneIdentifier(contents), expected);
    }
  });

  test('unsafe, malformed, duplicate and oversized zone identifiers fail closed', () {
    final oversized = Uint8List(maxZoneIdentifierBytes + 1)
      ..fillRange(0, maxZoneIdentifierBytes + 1, 0x20);
    oversized.setRange(0, 26, bytesOf('[ZoneTransfer]\r\nZoneId=0\r\n'));
    final cases = <List<int>>[
      bytesOf('ZoneId=0\r\n'),
      bytesOf('[ZoneTransfer]\r\n'),
      bytesOf('[ZoneTransfer]\r\nZoneId=0\r\nZoneId=1\r\n'),
      bytesOf('[ZoneTransfer]\r\nZoneId=3\r\n'),
      bytesOf('[ZoneTransfer]\r\nZoneId=4\r\n'),
      bytesOf('[ZoneTransfer]\r\nZoneId=5\r\n'),
      bytesOf('[ZoneTransfer]\r\nZoneId=trusted\r\n'),
      [...bytesOf('[ZoneTransfer]\r\nZoneId=0\r\n'), 0xFF],
      oversized,
    ];
    for (final contents in cases) {
      expect(
        () => parseZoneIdentifier(contents),
        throwsA(isA<ZoneIdentifierError>()),
      );
    }
  });

  test('workbook safety allows VBA but rejects every pre-open hazard', () {
    validateWorkbookSafety(const WorkbookSafetyFacts());
    validateWorkbookSafety(const WorkbookSafetyFacts(hasVbaProject: true));
    for (final unsafe in [
      const WorkbookSafetyFacts(hasXlmMacroSheet: true),
      const WorkbookSafetyFacts(hasWorkbookProtection: true),
      const WorkbookSafetyFacts(externalLinkCount: 1),
      const WorkbookSafetyFacts(connectionCount: 1),
      const WorkbookSafetyFacts(queryTableCount: 1),
    ]) {
      expect(
        () => validateWorkbookSafety(unsafe),
        throwsA(isA<WorkbookSafetyError>()),
      );
    }
  });

  test('COM plan locks Excel down in governing order before Open', () {
    final plan = buildComOperationPlan(
      const ComPlanRequest(
        workbookPath: r'C:\private\source.xlsm',
        pdfPath: r'C:\private\sheet.pdf',
        workbookIndex: 2,
        worksheetName: 'MEC',
        worksheetVisibility: WorksheetVisibility.visible,
      ),
    );
    expect(
      plan.takeWhile((op) => op is! ComOpenWorkbook).toList(),
      [
        const ComInitializeSta(),
        const ComCreateApplication(),
        const ComSetProperty(name: 'AutomationSecurity', value: ComI32(3)),
        const ComSetProperty(name: 'EnableEvents', value: ComBool(false)),
        const ComSetProperty(name: 'DisplayAlerts', value: ComBool(false)),
        const ComSetProperty(name: 'AskToUpdateLinks', value: ComBool(false)),
        const ComSetProperty(name: 'Visible', value: ComBool(false)),
      ],
    );
    for (final forbidden in [
      'Calculate',
      'CalculateFull',
      'RefreshAll',
      'UpdateLink',
    ]) {
      expect(
        () => validateDispatchMember(forbidden),
        throwsA(isA<ComPlanError>()),
      );
    }
  });

  test('every property set uses DISPID_PROPERTYPUT', () {
    final plan = buildComOperationPlan(validRequest());
    final setters = plan.whereType<ComSetProperty>().toList();
    expect(setters.length, 5);
    expect(setters.every((op) => op.namedPropertyPut), isTrue);
  });

  test('dispatch encoding binds flags, missing variant, named DISPID and order', () {
    final encoded = encodeDispatchParameters(
      const [
        ComMissing(),
        ComI32(7),
        ComBool(true),
        ComString('final'),
      ],
      DispatchInvokeKind.propertyPut,
    );
    expect(encoded.flags, 4);
    expect(encoded.namedDispids, [-3]);
    expect(encoded.arguments, [
      const EncodedError(dispEParamNotFound),
      const EncodedI32(7),
      const EncodedBool(true),
      const EncodedString('final'),
    ]);
    expect(encodeDispatchParameters(const [], DispatchInvokeKind.method).flags, 1);
    expect(
      encodeDispatchParameters(const [], DispatchInvokeKind.propertyGet).flags,
      2,
    );
    expect(
      encodeDispatchParameters(const [], DispatchInvokeKind.method).namedDispids,
      isEmpty,
    );
  });

  test('COM plan encodes complete reversed dispatch arguments and teardown', () {
    const workbookPath = r'C:\private\source.xlsm';
    const pdfPath = r'C:\private\sheet.pdf';
    final plan = buildComOperationPlan(
      const ComPlanRequest(
        workbookPath: workbookPath,
        pdfPath: pdfPath,
        workbookIndex: 2,
        worksheetName: 'MEC',
        worksheetVisibility: WorksheetVisibility.veryHidden,
      ),
    );
    expect(plan, [
      const ComInitializeSta(),
      const ComCreateApplication(),
      const ComSetProperty(name: 'AutomationSecurity', value: ComI32(3)),
      const ComSetProperty(name: 'EnableEvents', value: ComBool(false)),
      const ComSetProperty(name: 'DisplayAlerts', value: ComBool(false)),
      const ComSetProperty(name: 'AskToUpdateLinks', value: ComBool(false)),
      const ComSetProperty(name: 'Visible', value: ComBool(false)),
      const ComOpenWorkbook(
        arguments: [
          ComI32(0),
          ComMissing(),
          ComBool(false),
          ComMissing(),
          ComBool(false),
          ComMissing(),
          ComMissing(),
          ComMissing(),
          ComBool(true),
          ComString(''),
          ComString(''),
          ComMissing(),
          ComBool(true),
          ComI32(0),
          ComString(workbookPath),
        ],
      ),
      const ComResolveWorksheet(arguments: [ComI32(2)]),
      const ComVerifyWorksheet(expectedName: 'MEC', expectedVisibility: 2),
      const ComExportWorksheetPdf(
        arguments: [
          ComMissing(),
          ComBool(false),
          ComMissing(),
          ComMissing(),
          ComBool(false),
          ComBool(true),
          ComI32(0),
          ComString(pdfPath),
          ComI32(0),
        ],
      ),
      const ComCloseWorkbook(arguments: [ComBool(false)]),
      const ComQuitApplication(),
      const ComReleaseObject(ComObject.worksheet),
      const ComReleaseObject(ComObject.worksheets),
      const ComReleaseObject(ComObject.workbook),
      const ComReleaseObject(ComObject.workbooks),
      const ComReleaseObject(ComObject.application),
      const ComUninitializeSta(),
    ]);
  });

  test('dispatch member whitelist rejects calculation, refresh, link, repair and protected view', () {
    for (final allowed in [
      'AutomationSecurity',
      'EnableEvents',
      'DisplayAlerts',
      'AskToUpdateLinks',
      'Visible',
      'Workbooks',
      'Open',
      'Worksheets',
      'Item',
      'Name',
      'ExportAsFixedFormat',
      'Close',
      'Quit',
    ]) {
      validateDispatchMember(allowed);
    }
    for (final forbidden in [
      'Calculation',
      'CalculateBeforeSave',
      'Calculate',
      'CalculateFull',
      'CalculateFullRebuild',
      'Refresh',
      'RefreshAll',
      'UpdateLink',
      'Repair',
      'OpenAndRepair',
      'ProtectedView',
      'ProtectedViewWindows',
      'Run',
      'Save',
    ]) {
      expect(
        () => validateDispatchMember(forbidden),
        throwsA(isA<ComPlanError>()),
        reason: forbidden,
      );
    }
  });

  test('COM plan rejects unbounded paths, names and invalid worksheet indexes', () {
    final cases = <ComPlanRequest>[
      ComPlanRequest(
        workbookPath: 'w' * (maxDispatchStringChars + 1),
        pdfPath: r'C:\private\sheet.pdf',
        workbookIndex: 1,
        worksheetName: 'MEC',
        worksheetVisibility: WorksheetVisibility.visible,
      ),
      ComPlanRequest(
        workbookPath: r'C:\private\source.xlsx',
        pdfPath: 'p' * (maxDispatchStringChars + 1),
        workbookIndex: 1,
        worksheetName: 'MEC',
        worksheetVisibility: WorksheetVisibility.visible,
      ),
      ComPlanRequest(
        workbookPath: r'C:\private\source.xlsx',
        pdfPath: r'C:\private\sheet.pdf',
        workbookIndex: 1,
        worksheetName: 'n' * (maxDispatchStringChars + 1),
        worksheetVisibility: WorksheetVisibility.visible,
      ),
      ComPlanRequest(
        workbookPath: r'C:\private\source.xlsx',
        pdfPath: r'C:\private\sheet.pdf',
        workbookIndex: 1,
        worksheetName: '',
        worksheetVisibility: WorksheetVisibility.visible,
      ),
      ComPlanRequest(
        workbookPath: r'C:\private\source.xlsx',
        pdfPath: r'C:\private\sheet.pdf',
        workbookIndex: 0,
        worksheetName: 'MEC',
        worksheetVisibility: WorksheetVisibility.visible,
      ),
      ComPlanRequest(
        workbookPath: r'C:\private\source.xlsx',
        pdfPath: r'C:\private\sheet.pdf',
        workbookIndex: 251,
        worksheetName: 'MEC',
        worksheetVisibility: WorksheetVisibility.visible,
      ),
    ];
    for (final request in cases) {
      expect(
        () => buildComOperationPlan(request),
        throwsA(isA<ComPlanError>()),
      );
    }
  });

  test('anchored source policy allows only missing or safe zone stream and safe facts', () {
    final workspace = Directory.systemTemp.createTempSync('docflow-zone-');
    addTearDown(() => workspace.deleteSync(recursive: true));
    final path = '${workspace.path}/source.xlsx';
    File(path).writeAsBytesSync(bytesOf('workbook'));
    final source = CapabilityRoot.open(workspace.path).authorize(path).anchor();
    const safety = WorkbookSafetyFacts();

    for (final read in <Object?>[
      null,
      bytesOf('[ZoneTransfer]\r\nZoneId=0\r\n'),
      bytesOf('[ZoneTransfer]\r\nZoneId=2\r\n'),
    ]) {
      AnchoredWorkbookSourcePolicy(_FakeZoneReader(read))
          .validateSource(source, safety);
    }

    for (final read in <Object?>[
      bytesOf('[ZoneTransfer]\r\nZoneId=3\r\n'),
      bytesOf('malformed'),
      Exception('injected ADS read failure'),
    ]) {
      final failure = _captureFailure(
        () => AnchoredWorkbookSourcePolicy(_FakeZoneReader(read))
            .validateSource(source, safety),
      );
      expect(failure.message, 'error.print.excelSourceMotwBlocked');
    }

    final failure = _captureFailure(
      () => AnchoredWorkbookSourcePolicy(_FakeZoneReader(null)).validateSource(
        source,
        const WorkbookSafetyFacts(hasWorkbookProtection: true),
      ),
    );
    expect(failure.message, 'error.print.excelWorkbookUnsafe');
  });
}

PrintFailure _captureFailure(void Function() body) {
  try {
    body();
  } on PrintFailure catch (failure) {
    return failure;
  }
  throw StateError('expected a PrintFailure');
}
