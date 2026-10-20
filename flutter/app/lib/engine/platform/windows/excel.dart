// Port of src-tauri/src/platform/windows/excel.rs.
//
// The COM plan executor (ExcelDispatchSession + execute_plan) is platform
// neutral and fully tested through [ComDispatchBackend]. The concrete
// IDispatch backend ([Win32ComDispatchBackend]) uses dart:ffi directly
// against ole32/oleaut32 vtables.
//
// UNVERIFIED: the Win32 backend has not been compiled or executed on Windows
// in this environment. Vtable offsets, VARIANT/DISPPARAMS layouts and the
// STA apartment handling must be validated on a Windows host with Excel
// installed before any acceptance claim.
library;

import 'dart:ffi';
import 'dart:io';
import 'dart:typed_data';

import 'package:ffi/ffi.dart';

import '../../cap_fs.dart';
import '../../progress.dart' show CancellationToken;
import '../excel.dart';
import '../excel_print_ports.dart';
import '../excel_windows_contract.dart';

/// A COM failure with the Rust diagnostic stage name and HRESULT.
class ComBackendFailure implements Exception {
  const ComBackendFailure(this.stage, this.hresult);

  final String stage;
  final int hresult;

  @override
  String toString() =>
      'excel_com stage=$stage hresult=0x${(hresult & 0xFFFFFFFF).toRadixString(16).toUpperCase().padLeft(8, '0')}';
}

/// An IDispatch reference owned by the session.
abstract interface class ComDispatch {
  /// Releases the COM reference (`Release`).
  void release();
}

/// Values returned by `Invoke`: a COM object, a BSTR string, an I4 or a bool.
typedef ComResult = Object;

/// Abstraction over COM so the plan executor can be verified without Excel.
abstract interface class ComDispatchBackend {
  /// `CoInitializeEx(COINIT_APARTMENTTHREADED)`.
  void initializeSta();

  /// `CoUninitialize`.
  void uninitialize();

  /// `CLSIDFromProgID` + `CoCreateInstance(CLSCTX_LOCAL_SERVER)`.
  ComDispatch createApplication(String progId);

  /// `CLSIDFromProgID` + `GetActiveObject` (running object table). Failures
  /// use stage `resolve` (ProgID unknown) or `active` (not running).
  ComDispatch getActiveObject(String progId);

  /// `GetIDsOfNames` + `Invoke`. Returns the result variant value.
  ComResult invoke(
    ComDispatch target,
    String name,
    DispatchInvokeKind kind,
    List<EncodedVariant> arguments,
    List<int> namedDispids,
  );
}

/// Port of `ExcelDispatchSession`.
final class ExcelDispatchSession {
  ComDispatch? application;
  ComDispatch? workbooks;
  ComDispatch? workbook;
  ComDispatch? worksheets;
  ComDispatch? worksheet;
  bool workbookClosed = false;
  bool applicationQuit = false;

  final ComDispatchBackend backend;

  ExcelDispatchSession(this.backend);

  /// `cleanup`: closes the workbook, quits Excel, releases every reference.
  void cleanup() {
    var failed = false;
    final openWorkbook = workbook;
    if (!workbookClosed && openWorkbook != null) {
      try {
        backend.invoke(
          openWorkbook,
          'Close',
          DispatchInvokeKind.method,
          encodeDispatchParameters([const ComBool(false)], DispatchInvokeKind.method).arguments,
          const [],
        );
        workbookClosed = true;
      } on ComBackendFailure {
        failed = true;
      }
    }
    final app = application;
    if (!applicationQuit && app != null) {
      try {
        backend.invoke(app, 'Quit', DispatchInvokeKind.method, const [], const []);
        applicationQuit = true;
      } on ComBackendFailure {
        failed = true;
      }
    }
    worksheet = null;
    worksheets = null;
    workbook = null;
    workbooks = null;
    application = null;
    if (failed) {
      throw _comFailure();
    }
  }

  /// `release`: drops one object reference named by [object].
  void release(ComObject object) {
    final ComDispatch? released = switch (object) {
      ComObject.worksheet => worksheet,
      ComObject.worksheets => worksheets,
      ComObject.workbook => workbook,
      ComObject.workbooks => workbooks,
      ComObject.application => application,
    };
    if (released == null) {
      throw _contractFailure();
    }
    switch (object) {
      case ComObject.worksheet:
        worksheet = null;
      case ComObject.worksheets:
        worksheets = null;
      case ComObject.workbook:
        workbook = null;
      case ComObject.workbooks:
        workbooks = null;
      case ComObject.application:
        application = null;
    }
    released.release();
  }
}

/// `execute_plan`. [apartment] tracks whether `InitializeSta` is active.
final class ExcelPlanExecutor {
  ExcelPlanExecutor(this.session, this.clock, this.deadline, this.cancellation);

  final ExcelDispatchSession session;
  final Duration Function() clock;
  final Duration deadline;
  final CancellationToken cancellation;
  bool apartmentActive = false;

  void execute(List<ComOperation> plan) {
    for (final operation in plan) {
      checkWorkerFence(clock, deadline, cancellation);
      _run(operation);
    }
  }

  void _run(ComOperation operation) {
    final backend = session.backend;
    switch (operation) {
      case ComInitializeSta():
        if (apartmentActive) {
          throw _contractFailure();
        }
        try {
          backend.initializeSta();
        } on ComBackendFailure catch (error) {
          throw _comFailureAt('initialize_sta', error);
        }
        apartmentActive = true;
      case ComCreateApplication():
        if (!apartmentActive || session.application != null) {
          throw _contractFailure();
        }
        try {
          session.application = backend.createApplication('Excel.Application');
        } on ComBackendFailure catch (error) {
          throw _unavailableAt('create_excel', error);
        }
      case ComSetProperty(:final name, :final value, :final namedPropertyPut):
        if (!namedPropertyPut) {
          throw _contractFailure();
        }
        final application = session.application ?? (throw _contractFailure());
        _invoke(
          application,
          name,
          DispatchInvokeKind.propertyPut,
          [value],
          propertyPut: true,
        );
      case ComOpenWorkbook(:final arguments):
        final application = session.application ?? (throw _contractFailure());
        final workbooks = _invokeDispatch(
          application,
          'Workbooks',
          DispatchInvokeKind.propertyGet,
          const [],
        );
        final workbook = _invokeDispatch(
          workbooks,
          'Open',
          DispatchInvokeKind.method,
          arguments,
        );
        session.workbooks = workbooks;
        session.workbook = workbook;
      case ComResolveWorksheet(:final arguments):
        final workbook = session.workbook ?? (throw _contractFailure());
        final worksheets = _invokeDispatch(
          workbook,
          'Worksheets',
          DispatchInvokeKind.propertyGet,
          const [],
        );
        final worksheet = _invokeDispatch(
          worksheets,
          'Item',
          DispatchInvokeKind.propertyGet,
          arguments,
        );
        session.worksheets = worksheets;
        session.worksheet = worksheet;
      case ComVerifyWorksheet(:final expectedName, :final expectedVisibility):
        final worksheet = session.worksheet ?? (throw _contractFailure());
        final actualName = _variantString(
          _invoke(worksheet, 'Name', DispatchInvokeKind.propertyGet, const []),
        );
        final actualVisibility = _variantI32(
          _invoke(worksheet, 'Visible', DispatchInvokeKind.propertyGet, const []),
        );
        if (actualName != expectedName || actualVisibility != expectedVisibility) {
          throw PrintFailure.failed('error.print.worksheetChanged');
        }
      case ComExportWorksheetPdf(:final arguments):
        final worksheet = session.worksheet ?? (throw _contractFailure());
        try {
          _invoke(worksheet, 'ExportAsFixedFormat', DispatchInvokeKind.method, arguments);
        } on PrintFailure {
          throw PrintFailure.failed('error.print.excelExportFailed');
        }
      case ComCloseWorkbook(:final arguments):
        final workbook = session.workbook ?? (throw _contractFailure());
        _invoke(workbook, 'Close', DispatchInvokeKind.method, arguments);
        session.workbookClosed = true;
      case ComQuitApplication():
        final application = session.application ?? (throw _contractFailure());
        _invoke(application, 'Quit', DispatchInvokeKind.method, const []);
        session.applicationQuit = true;
      case ComReleaseObject(:final object):
        session.release(object);
      case ComUninitializeSta():
        if (session.application != null ||
            session.workbooks != null ||
            session.workbook != null ||
            session.worksheets != null ||
            session.worksheet != null) {
          throw _contractFailure();
        }
        if (!apartmentActive) {
          throw _contractFailure();
        }
        apartmentActive = false;
        try {
          backend.uninitialize();
        } on ComBackendFailure {
          throw _contractFailure();
        }
    }
  }

  ComResult _invoke(
    ComDispatch target,
    String name,
    DispatchInvokeKind kind,
    List<ComValue> arguments, {
    bool propertyPut = false,
  }) {
    validateDispatchMember(name);
    final expectedKind = propertyPut
        ? DispatchInvokeKind.propertyPut
        : kind;
    if (propertyPut && kind != DispatchInvokeKind.propertyPut) {
      throw _contractFailure();
    }
    if (!propertyPut && kind == DispatchInvokeKind.propertyPut) {
      throw _contractFailure();
    }
    final encoded = encodeDispatchParameters(arguments, expectedKind);
    try {
      return session.backend.invoke(
        target,
        name,
        expectedKind,
        encoded.arguments,
        encoded.namedDispids,
      );
    } on ComBackendFailure catch (error) {
      throw _comFailureAt('${_actionFor(expectedKind)}:$name', error);
    }
  }

  ComDispatch _invokeDispatch(
    ComDispatch target,
    String name,
    DispatchInvokeKind kind,
    List<ComValue> arguments,
  ) {
    final value = _invoke(target, name, kind, arguments);
    if (value is ComDispatch) {
      return value;
    }
    throw _comFailure();
  }
}

String _actionFor(DispatchInvokeKind kind) => switch (kind) {
  DispatchInvokeKind.method => 'method',
  DispatchInvokeKind.propertyGet => 'get',
  DispatchInvokeKind.propertyPut => 'set',
};

String _variantString(ComResult value) {
  if (value is String) {
    return value;
  }
  throw _comFailure();
}

int _variantI32(ComResult value) {
  if (value is int) {
    return value;
  }
  throw _comFailure();
}

/// `check_worker_fence`.
void checkWorkerFence(
  Duration Function() clock,
  Duration deadline,
  CancellationToken cancellation,
) {
  if (cancellation.isCancelled) {
    throw PrintFailure.cancelled('error.print.excelExportCancelled', uncertain: false);
  }
  if (clock() >= deadline) {
    throw PrintFailure.failed('error.print.excelExportTimedOut');
  }
}

PrintFailure _contractFailure() =>
    PrintFailure.unsupported('error.print.excelComContractInvalid');

PrintFailure _comFailure() =>
    PrintFailure.failed('error.print.excelComFailed')
        .withDiagnostic('excel_com stage=result_decode');

PrintFailure _comFailureAt(String stage, ComBackendFailure error) =>
    PrintFailure.failed('error.print.excelComFailed')
        .withDiagnostic('excel_com stage=$stage hresult=0x${(error.hresult & 0xFFFFFFFF).toRadixString(16).toUpperCase().padLeft(8, '0')}');

PrintFailure _unavailableAt(String stage, ComBackendFailure error) =>
    PrintFailure.unsupported('error.print.excelUnavailable')
        .withDiagnostic('excel_com stage=$stage hresult=0x${(error.hresult & 0xFFFFFFFF).toRadixString(16).toUpperCase().padLeft(8, '0')}');

/// `WindowsZoneIdentifierReader`: reads at most the zone limit from the
/// `Zone.Identifier` alternate stream of [source].
final class WindowsZoneIdentifierReader implements ZoneIdentifierReader {
  const WindowsZoneIdentifierReader();

  @override
  Uint8List? readZoneIdentifier(AnchoredPath source) {
    final path = '${source.absolute}:Zone.Identifier';
    final file = File(path);
    if (!file.existsSync()) {
      return null;
    }
    final handle = file.openSync();
    final List<int> contents;
    try {
      contents = handle.readSync(maxZoneIdentifierBytes + 1);
    } finally {
      handle.closeSync();
    }
    if (contents.length > maxZoneIdentifierBytes) {
      throw const FileSystemException(
        'Zone.Identifier exceeds the bounded source policy limit',
      );
    }
    return Uint8List.fromList(contents);
  }
}

/// `WindowsExcelExporter`.
final class WindowsExcelExporter implements WorksheetPdfExporter {
  WindowsExcelExporter({
    required this.backendFactory,
    required this.clock,
  });

  /// Creates the COM backend. Production passes [Win32ComDispatchBackend.new].
  final ComDispatchBackend Function() backendFactory;
  final Duration Function() clock;

  @override
  Future<ExportedWorksheetPdf> exportPdf(ExcelExportRequest request) async {
    final staged = request.stagedWorkbook;
    try {
      validateWorkbookSafety(request.safety);
    } on WorkbookSafetyError {
      throw PrintFailure.unsupported('error.print.excelWorkbookUnsafe');
    }
    checkWorkerFence(clock, request.deadline, request.cancellation);

    final workbookPath = _boundedPath(staged.workbookPath);
    final pdfOutputPath = staged.pdfOutputPath();
    final pdfPath = _boundedPath(pdfOutputPath);
    final type = FileSystemEntity.typeSync(pdfOutputPath, followLinks: false);
    if (type != FileSystemEntityType.notFound) {
      throw PrintFailure.failed('error.print.excelExportDestinationExists');
    }
    final List<ComOperation> plan;
    try {
      plan = buildComOperationPlan(
        ComPlanRequest(
          workbookPath: workbookPath,
          pdfPath: pdfPath,
          workbookIndex: request.worksheet.workbookIndex,
          worksheetName: request.worksheet.name,
          worksheetVisibility: request.worksheet.visibility,
        ),
      );
    } on ComPlanError {
      throw PrintFailure.unsupported('error.print.excelComContractInvalid');
    }

    final backend = backendFactory();
    final session = ExcelDispatchSession(backend);
    final executor = ExcelPlanExecutor(
      session,
      clock,
      request.deadline,
      request.cancellation,
    );
    PrintFailure? execution;
    try {
      executor.execute(plan);
    } on PrintFailure catch (failure) {
      execution = failure;
    }
    // Teardown runs whether or not execution succeeded, mirroring the Rust
    // order: session cleanup, then apartment release.
    var teardownFailed = false;
    try {
      session.cleanup();
    } on PrintFailure {
      teardownFailed = true;
    }
    if (executor.apartmentActive) {
      try {
        backend.uninitialize();
      } on ComBackendFailure {
        teardownFailed = true;
      }
      executor.apartmentActive = false;
    }
    if (teardownFailed) {
      staged.quarantineAfterTeardownFailure();
      throw PrintFailure.unsupported('error.print.excelExportQuarantined');
    }
    if (execution != null) {
      throw execution;
    }
    final exported = staged.intoExportedPdf();
    checkWorkerFence(clock, request.deadline, request.cancellation);
    return exported;
  }
}

String _boundedPath(String path) {
  if (path.isEmpty || path.contains('\u0000')) {
    throw PrintFailure.unsupported('error.print.excelComContractInvalid');
  }
  return path;
}

/// Concrete IDispatch backend over dart:ffi.
///
/// UNVERIFIED: not executed on Windows. Layouts assume the x64 ABI:
/// VARIANT = 24 bytes (vt at 0, payload at 8), DISPPARAMS = {rgvarg, rgdispidNamedArgs,
/// cArgs, cNamedArgs}. IDispatch vtable: QueryInterface 0, AddRef 1, Release 2,
/// GetTypeInfoCount 3, GetTypeInfo 4, GetIDsOfNames 5, Invoke 6.
final class Win32ComDispatchBackend implements ComDispatchBackend {
  Win32ComDispatchBackend();

  static final DynamicLibrary _ole32 = DynamicLibrary.open('ole32.dll');
  static final DynamicLibrary _oleaut32 = DynamicLibrary.open('oleaut32.dll');

  static final int Function(Pointer<Void>, int) _coInitializeEx = _ole32
      .lookupFunction<Int32 Function(Pointer<Void>, Uint32), int Function(Pointer<Void>, int)>(
        'CoInitializeEx',
      );
  static final void Function() _coUninitialize = _ole32
      .lookupFunction<Void Function(), void Function()>('CoUninitialize');
  static final int Function(Pointer<Utf16>, Pointer<Uint8>) _clsidFromProgId = _ole32
      .lookupFunction<
        Int32 Function(Pointer<Utf16>, Pointer<Uint8>),
        int Function(Pointer<Utf16>, Pointer<Uint8>)
      >('CLSIDFromProgID');
  static final int Function(Pointer<Uint8>, Pointer<Void>, int, Pointer<Uint8>, Pointer<Pointer<Void>>)
  _coCreateInstance = _ole32.lookupFunction<
    Int32 Function(Pointer<Uint8>, Pointer<Void>, Uint32, Pointer<Uint8>, Pointer<Pointer<Void>>),
    int Function(Pointer<Uint8>, Pointer<Void>, int, Pointer<Uint8>, Pointer<Pointer<Void>>)
  >('CoCreateInstance');
  static final Pointer<Utf16> Function(Pointer<Utf16>) _sysAllocString = _oleaut32
      .lookupFunction<
        Pointer<Utf16> Function(Pointer<Utf16>),
        Pointer<Utf16> Function(Pointer<Utf16>)
      >('SysAllocString');

  static final int Function(Pointer<Uint8>, Pointer<Void>, Pointer<Pointer<Void>>)
  _getActiveObject = _oleaut32.lookupFunction<
    Int32 Function(Pointer<Uint8>, Pointer<Void>, Pointer<Pointer<Void>>),
    int Function(Pointer<Uint8>, Pointer<Void>, Pointer<Pointer<Void>>)
  >('GetActiveObject');

  static const int _vtDate = 7;
  static const int _coinitApartmentThreaded = 0x2;
  static const int _clsctxLocalServer = 0x4;
  static const int _dispatchMethod = 0x1;
  static const int _dispatchPropertyGet = 0x2;
  static const int _dispatchPropertyPut = 0x4;
  static const int _vtI4 = 3;
  static const int _vtBstr = 8;
  static const int _vtError = 10;
  static const int _vtBool = 11;
  static const int _vtDispatch = 9;

  /// IID_NULL and IID_IDispatch {00020400-0000-0000-C000-000000000046}.
  static final Pointer<Uint8> _iidNull = _guidBytes(
    '00000000-0000-0000-0000-000000000000',
  );
  static final Pointer<Uint8> _iidDispatch = _guidBytes(
    '00020400-0000-0000-C000-000000000046',
  );

  static Pointer<Uint8> _guidBytes(String text) {
    final hex = text.replaceAll('-', '');
    final bytes = calloc<Uint8>(16);
    final data1 = int.parse(hex.substring(0, 8), radix: 16);
    bytes[0] = data1 & 0xFF;
    bytes[1] = (data1 >> 8) & 0xFF;
    bytes[2] = (data1 >> 16) & 0xFF;
    bytes[3] = (data1 >> 24) & 0xFF;
    final data2 = int.parse(hex.substring(8, 12), radix: 16);
    bytes[4] = data2 & 0xFF;
    bytes[5] = (data2 >> 8) & 0xFF;
    final data3 = int.parse(hex.substring(12, 16), radix: 16);
    bytes[6] = data3 & 0xFF;
    bytes[7] = (data3 >> 8) & 0xFF;
    for (var index = 0; index < 8; index++) {
      bytes[8 + index] = int.parse(hex.substring(16 + index * 2, 18 + index * 2), radix: 16);
    }
    return bytes;
  }

  @override
  void initializeSta() {
    final hr = _coInitializeEx(nullptr, _coinitApartmentThreaded);
    if (hr < 0 && hr != 1) {
      throw ComBackendFailure('initialize_sta', hr);
    }
  }

  @override
  void uninitialize() => _coUninitialize();

  @override
  ComDispatch createApplication(String progId) {
    final wide = progId.toNativeUtf16();
    final clsid = calloc<Uint8>(16);
    try {
      final hr = _clsidFromProgId(wide, clsid);
      if (hr < 0) {
        throw ComBackendFailure('resolve_excel', hr);
      }
      final out = calloc<Pointer<Void>>();
      try {
        final hr2 = _coCreateInstance(clsid, nullptr, _clsctxLocalServer, _iidDispatch, out);
        if (hr2 < 0) {
          throw ComBackendFailure('create_excel', hr2);
        }
        return _Win32Dispatch(out.value);
      } finally {
        calloc.free(out);
      }
    } finally {
      calloc.free(wide);
      calloc.free(clsid);
    }
  }

  @override
  ComDispatch getActiveObject(String progId) {
    final wide = progId.toNativeUtf16();
    final clsid = calloc<Uint8>(16);
    final out = calloc<Pointer<Void>>();
    try {
      final hr = _clsidFromProgId(wide, clsid);
      if (hr < 0) {
        throw ComBackendFailure('resolve', hr);
      }
      final hrActive = _getActiveObject(clsid, nullptr, out);
      if (hrActive < 0) {
        throw ComBackendFailure('active', hrActive);
      }
      final unknown = out.value;
      final unknownVtable = unknown.cast<Pointer<Pointer<Void>>>().value;
      final queryInterface = (unknownVtable + 0)
          .value
          .cast<NativeFunction<Int32 Function(Pointer<Void>, Pointer<Uint8>, Pointer<Pointer<Void>>)>>()
          .asFunction<int Function(Pointer<Void>, Pointer<Uint8>, Pointer<Pointer<Void>>)>();
      final dispatchOut = calloc<Pointer<Void>>();
      try {
        final hrQi = queryInterface(unknown, _iidDispatch, dispatchOut);
        if (hrQi < 0) {
          throw ComBackendFailure('active', hrQi);
        }
        return _Win32Dispatch(dispatchOut.value);
      } finally {
        calloc.free(dispatchOut);
        _Win32Dispatch(unknown).release();
      }
    } finally {
      calloc.free(out);
      calloc.free(clsid);
      calloc.free(wide);
    }
  }

  @override
  ComResult invoke(
    ComDispatch target,
    String name,
    DispatchInvokeKind kind,
    List<EncodedVariant> arguments,
    List<int> namedDispids,
  ) {
    final dispatch = (target as _Win32Dispatch).pointer;
    final flags = switch (kind) {
      DispatchInvokeKind.method => _dispatchMethod,
      DispatchInvokeKind.propertyGet => _dispatchPropertyGet,
      DispatchInvokeKind.propertyPut => _dispatchPropertyPut,
    };
    final wideName = name.toNativeUtf16();
    final dispIdOut = calloc<Int32>();
    final vtable = dispatch.cast<Pointer<Pointer<Void>>>().value;
    final getIds = (vtable + 5).value.cast<NativeFunction<Int32 Function(Pointer<Void>, Pointer<Uint8>, Pointer<Pointer<Utf16>>, Uint32, Uint32, Pointer<Int32>)>>();
    final getIdsCall = getIds.asFunction<int Function(Pointer<Void>, Pointer<Uint8>, Pointer<Pointer<Utf16>>, int, int, Pointer<Int32>)>();
    final names = calloc<Pointer<Utf16>>();
    names.value = wideName;
    final args = calloc<Uint8>(arguments.length * 24 + 1);
    final namedArgs = calloc<Int32>(namedDispids.isEmpty ? 1 : namedDispids.length);
    final params = calloc<Uint8>(32);
    final result = calloc<Uint8>(24);
    try {
      final hrIds = getIdsCall(dispatch, _iidNull, names, 1, 0, dispIdOut);
      if (hrIds < 0) {
        throw ComBackendFailure('lookup:$name', hrIds);
      }
      for (var index = 0; index < arguments.length; index++) {
        _writeVariant(args + index * 24, arguments[index]);
      }
      for (var index = 0; index < namedDispids.length; index++) {
        namedArgs[index] = namedDispids[index];
      }
      // DISPPARAMS: rgvarg, rgdispidNamedArgs, cArgs, cNamedArgs.
      final paramWords = params.cast<Uint64>();
      paramWords[0] = arguments.isEmpty ? 0 : args.address;
      paramWords[1] = namedDispids.isEmpty ? 0 : namedArgs.address;
      final counts = (params + 16).cast<Uint32>();
      counts[0] = arguments.length;
      counts[1] = namedDispids.length;
      final invokeFn = (vtable + 6).value.cast<NativeFunction<Int32 Function(Pointer<Void>, Int32, Pointer<Uint8>, Uint32, Uint16, Pointer<Uint8>, Pointer<Uint8>, Pointer<Void>, Pointer<Uint32>)>>()
          .asFunction<int Function(Pointer<Void>, int, Pointer<Uint8>, int, int, Pointer<Uint8>, Pointer<Uint8>, Pointer<Void>, Pointer<Uint32>)>();
      final hrInvoke = invokeFn(
        dispatch,
        dispIdOut.value,
        _iidNull,
        0,
        flags,
        params,
        result,
        nullptr,
        nullptr,
      );
      if (hrInvoke < 0) {
        throw ComBackendFailure('${_actionFor(kind)}:$name', hrInvoke);
      }
      return _readVariant(result);
    } finally {
      calloc.free(wideName);
      calloc.free(dispIdOut);
      calloc.free(names);
      calloc.free(args);
      calloc.free(namedArgs);
      calloc.free(params);
      calloc.free(result);
    }
  }

  void _writeVariant(Pointer<Uint8> target, EncodedVariant value) {
    final header = target.cast<Uint16>();
    header[0] = 0;
    switch (value) {
      case EncodedI32(:final value):
        header[0] = _vtI4;
        (target + 8).cast<Int32>().value = value;
      case EncodedBool(:final value):
        header[0] = _vtBool;
        (target + 8).cast<Int16>().value = value ? -1 : 0;
      case EncodedString(:final value):
        header[0] = _vtBstr;
        (target + 8).cast<Pointer<Utf16>>().value = _sysAllocStringFromText(value);
      case EncodedError(:final hresult):
        header[0] = _vtError;
        (target + 8).cast<Int32>().value = hresult;
    }
  }

  Pointer<Utf16> _sysAllocStringFromText(String value) {
    final wide = value.toNativeUtf16();
    try {
      return Pointer<Utf16>.fromAddress(_sysAllocString(wide).address);
    } finally {
      calloc.free(wide);
    }
  }

  ComResult _readVariant(Pointer<Uint8> source) {
    final vt = source.cast<Uint16>().value;
    switch (vt) {
      case _vtI4:
        return (source + 8).cast<Int32>().value;
      case _vtBool:
        return (source + 8).cast<Int16>().value != 0;
      case _vtBstr:
        final bstr = (source + 8).cast<Pointer<Utf16>>().value;
        return bstr.toDartString();
      case _vtDispatch:
        return _Win32Dispatch((source + 8).cast<Pointer<Void>>().value);
      case _vtDate:
        return (source + 8).cast<Double>().value;
      default:
        throw ComBackendFailure('result_decode', vt);
    }
  }
}

final class _Win32Dispatch implements ComDispatch {
  _Win32Dispatch(this.pointer);

  final Pointer<Void> pointer;

  @override
  void release() {
    final vtable = pointer.cast<Pointer<Pointer<Void>>>().value;
    final releaseFn = (vtable + 2).value
        .cast<NativeFunction<Uint32 Function(Pointer<Void>)>>()
        .asFunction<int Function(Pointer<Void>)>();
    releaseFn(pointer);
  }
}
