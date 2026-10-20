/// Port of src-tauri/src/platform/windows/printing.rs: the Windows print
/// adapter (WINSPOOL, GDI, GDI+, Shell) and the controlled PDF submitter.
///
/// UNVERIFIED: every native call below is a dart:ffi binding to a Win32 export
/// (winspool.drv, gdi32, gdiplus, shell32, shlwapi, kernel32). It was written
/// and analyzed on Linux and has NOT been executed on Windows. Printer, spooler,
/// and driver behavior (DEVMODE acceptance, StartDoc/EndDoc semantics, job
/// release timing, GDI+ codecs, ShellExecute verbs) are unverified. No test in
/// this repository proves Windows or printer acceptance.
///
/// Known gap: rendering PDF pages requires Windows.Data.Pdf, which has no
/// binding yet (see `windows_pdf.dart`). Controlled PDF submission therefore
/// fails closed with a preparation failure instead of printing fabricated
/// pixels. Pure logic (settings mapping, scaling, observation classification,
/// PNG decoding) is verified on every host.
library;

import 'dart:ffi';
import 'dart:io' show sleep;
import 'dart:math' as math;
import 'dart:typed_data';

import 'package:ffi/ffi.dart';
import 'package:path/path.dart' as p;

import '../../errors.dart';
import '../../page_range.dart';
import '../../print_queue.dart';
import '../../progress.dart';
import '../../windows_pdf.dart' as windows_pdf;
import '../controlled_submit.dart';
import '../excel.dart';
import '../mod.dart';
import '../png_decode.dart';

const int _pdfDpi = 300;
const Duration _pollInterval = Duration(milliseconds: 250);
const Duration _handlerJobTimeout = Duration(seconds: 30);
const Duration _observationErrorGrace = Duration(seconds: 3);
const List<String> _controlledImageFormats = [
  'png',
  'jpg',
  'jpeg',
  'bmp',
  'tif',
  'tiff',
];

// Win32 constants (winspool.h, wingdi.h, shlwapi.h, winuser.h).
const int _printerEnumLocal = 0x2;
const int _printerEnumConnections = 0x4;
const int _dmOut = 0x2; // DM_OUT_BUFFER
const int _dmIn = 0x8; // DM_IN_BUFFER
const int _dmOrientationField = 0x1;
const int _dmPaperSizeField = 0x2;
const int _dmColorField = 0x800;
const int _dmDuplexField = 0x1000;
const int _dmpaperLetter = 1;
const int _dmpaperLegal = 5;
const int _dmpaperA3 = 8;
const int _dmpaperA4 = 9;
const int _dmpaperA5 = 11;
const int _dmorientPortrait = 1;
const int _dmorientLandscape = 2;
const int _dmcolorMonochrome = 1;
const int _dmcolorColor = 2;
const int _dmdupSimplex = 1;
const int _dmdupVertical = 2;
const int _dmdupHorizontal = 3;
const int _devModeSize = 220; // sizeof(DEVMODEW) on x64
const int _horzres = 8;
const int _vertres = 10;
const int _logpixelsx = 88;
const int _logpixelsy = 90;
const int _coloroncolor = 3;
const int _halftone = 4;
const int _srccopy = 0x00CC0020;
const int _dibRgbColors = 0;
const int _biRgb = 0;
const int _jobControlCancel = 3;
const int _assocfInitIgnoreUnknown = 0x400;
const int _assocstrCommand = 1;
const int _swHide = 0;
const int _gdipInterpolationHighQualityBicubic = 7;
const int _gdipOk = 0;
const int _maxWideChars = 32768;

// ---- Native struct layouts (x64) ------------------------------------------

final class _DocInfoW extends Struct {
  @Int32()
  external int cbSize;

  external Pointer<Utf16> lpszDocName;
  external Pointer<Utf16> lpszOutput;
  external Pointer<Utf16> lpszDatatype;

  @Uint32()
  external int fwType;
}

final class _PrinterInfo4W extends Struct {
  external Pointer<Utf16> pPrinterName;
  external Pointer<Utf16> pServerName;

  @Uint32()
  external int attributes;
}

final class _JobInfo1W extends Struct {
  @Uint32()
  external int jobId;

  external Pointer<Utf16> pPrinterName;
  external Pointer<Utf16> pMachineName;
  external Pointer<Utf16> pUserName;
  external Pointer<Utf16> pDocument;
  external Pointer<Utf16> pDatatype;
  external Pointer<Utf16> pStatus;

  @Uint32()
  external int status;
}

final class _GdiplusStartupInput extends Struct {
  @Uint32()
  external int gdiplusVersion;

  external Pointer<Void> debugEventCallback;

  @Int32()
  external int suppressBackgroundThread;

  @Int32()
  external int suppressExternalCodecs;
}

// ---- Function bindings -----------------------------------------------------

typedef _EnumPrintersWN =
    Int32 Function(
      Uint32 flags,
      Pointer<Void> name,
      Uint32 level,
      Pointer<Uint8> buffer,
      Uint32 bufferBytes,
      Pointer<Uint32> needed,
      Pointer<Uint32> returned,
    );
typedef _EnumPrintersWD =
    int Function(
      int flags,
      Pointer<Void> name,
      int level,
      Pointer<Uint8> buffer,
      int bufferBytes,
      Pointer<Uint32> needed,
      Pointer<Uint32> returned,
    );
typedef _GetDefaultPrinterWN =
    Int32 Function(Pointer<Uint16> buffer, Pointer<Uint32> length);
typedef _GetDefaultPrinterWD =
    int Function(Pointer<Uint16> buffer, Pointer<Uint32> length);
typedef _OpenPrinterWN =
    Int32 Function(
      Pointer<Utf16> name,
      Pointer<Pointer<Void>> handle,
      Pointer<Void> defaults,
    );
typedef _OpenPrinterWD =
    int Function(
      Pointer<Utf16> name,
      Pointer<Pointer<Void>> handle,
      Pointer<Void> defaults,
    );
typedef _ClosePrinterN = Int32 Function(Pointer<Void> handle);
typedef _ClosePrinterD = int Function(Pointer<Void> handle);
typedef _DocumentPropertiesWN =
    Int32 Function(
      Pointer<Void> hwnd,
      Pointer<Void> handle,
      Pointer<Utf16> name,
      Pointer<Uint8> output,
      Pointer<Uint8> input,
      Uint32 mode,
    );
typedef _DocumentPropertiesWD =
    int Function(
      Pointer<Void> hwnd,
      Pointer<Void> handle,
      Pointer<Utf16> name,
      Pointer<Uint8> output,
      Pointer<Uint8> input,
      int mode,
    );
typedef _EnumJobsWN =
    Int32 Function(
      Pointer<Void> handle,
      Uint32 first,
      Uint32 count,
      Uint32 level,
      Pointer<Uint8> buffer,
      Uint32 bufferBytes,
      Pointer<Uint32> needed,
      Pointer<Uint32> returned,
    );
typedef _EnumJobsWD =
    int Function(
      Pointer<Void> handle,
      int first,
      int count,
      int level,
      Pointer<Uint8> buffer,
      int bufferBytes,
      Pointer<Uint32> needed,
      Pointer<Uint32> returned,
    );
typedef _GetJobWN =
    Int32 Function(
      Pointer<Void> handle,
      Uint32 jobId,
      Uint32 level,
      Pointer<Uint8> buffer,
      Uint32 bufferBytes,
      Pointer<Uint32> needed,
    );
typedef _GetJobWD =
    int Function(
      Pointer<Void> handle,
      int jobId,
      int level,
      Pointer<Uint8> buffer,
      int bufferBytes,
      Pointer<Uint32> needed,
    );
typedef _SetJobWN =
    Int32 Function(
      Pointer<Void> handle,
      Uint32 jobId,
      Uint32 level,
      Pointer<Uint8> info,
      Uint32 command,
    );
typedef _SetJobWD =
    int Function(
      Pointer<Void> handle,
      int jobId,
      int level,
      Pointer<Uint8> info,
      int command,
    );

typedef _CreateDCWN =
    Pointer<Void> Function(
      Pointer<Utf16> driver,
      Pointer<Utf16> device,
      Pointer<Utf16> port,
      Pointer<Uint8> devmode,
    );
typedef _CreateDCWD =
    Pointer<Void> Function(
      Pointer<Utf16> driver,
      Pointer<Utf16> device,
      Pointer<Utf16> port,
      Pointer<Uint8> devmode,
    );
typedef _DeleteDCN = Int32 Function(Pointer<Void> hdc);
typedef _DeleteDCD = int Function(Pointer<Void> hdc);
typedef _GetDeviceCapsN = Int32 Function(Pointer<Void> hdc, Int32 index);
typedef _GetDeviceCapsD = int Function(Pointer<Void> hdc, int index);
typedef _StartDocWN =
    Int32 Function(Pointer<Void> hdc, Pointer<_DocInfoW> info);
typedef _StartDocWD = int Function(Pointer<Void> hdc, Pointer<_DocInfoW> info);
typedef _HdcStatusN = Int32 Function(Pointer<Void> hdc);
typedef _HdcStatusD = int Function(Pointer<Void> hdc);
typedef _ResetDCWN =
    Pointer<Void> Function(Pointer<Void> hdc, Pointer<Uint8> devmode);
typedef _ResetDCWD =
    Pointer<Void> Function(Pointer<Void> hdc, Pointer<Uint8> devmode);
typedef _SetStretchBltModeN = Int32 Function(Pointer<Void> hdc, Int32 mode);
typedef _SetStretchBltModeD = int Function(Pointer<Void> hdc, int mode);
typedef _StretchDIBitsN =
    Int32 Function(
      Pointer<Void> hdc,
      Int32 x,
      Int32 y,
      Int32 width,
      Int32 height,
      Int32 srcX,
      Int32 srcY,
      Int32 srcWidth,
      Int32 srcHeight,
      Pointer<Uint8> bits,
      Pointer<Uint8> info,
      Uint32 usage,
      Uint32 rop,
    );
typedef _StretchDIBitsD =
    int Function(
      Pointer<Void> hdc,
      int x,
      int y,
      int width,
      int height,
      int srcX,
      int srcY,
      int srcWidth,
      int srcHeight,
      Pointer<Uint8> bits,
      Pointer<Uint8> info,
      int usage,
      int rop,
    );

typedef _ShellExecuteWN =
    IntPtr Function(
      Pointer<Void> hwnd,
      Pointer<Utf16> operation,
      Pointer<Utf16> file,
      Pointer<Utf16> parameters,
      Pointer<Utf16> directory,
      Int32 show,
    );
typedef _ShellExecuteWD =
    int Function(
      Pointer<Void> hwnd,
      Pointer<Utf16> operation,
      Pointer<Utf16> file,
      Pointer<Utf16> parameters,
      Pointer<Utf16> directory,
      int show,
    );
typedef _AssocQueryStringWN =
    Int32 Function(
      Uint32 flags,
      Int32 type,
      Pointer<Utf16> association,
      Pointer<Utf16> extra,
      Pointer<Uint16> output,
      Pointer<Uint32> length,
    );
typedef _AssocQueryStringWD =
    int Function(
      int flags,
      int type,
      Pointer<Utf16> association,
      Pointer<Utf16> extra,
      Pointer<Uint16> output,
      Pointer<Uint32> length,
    );
typedef _GetLastErrorN = Uint32 Function();
typedef _GetLastErrorD = int Function();

typedef _GdiplusStartupN =
    Int32 Function(
      Pointer<IntPtr> token,
      Pointer<_GdiplusStartupInput> input,
      Pointer<Void> output,
    );
typedef _GdiplusStartupD =
    int Function(
      Pointer<IntPtr> token,
      Pointer<_GdiplusStartupInput> input,
      Pointer<Void> output,
    );
typedef _GdiplusShutdownN = Void Function(IntPtr token);
typedef _GdiplusShutdownD = void Function(int token);
typedef _GdipLoadImageFromFileN =
    Int32 Function(Pointer<Utf16> path, Pointer<Pointer<Void>> image);
typedef _GdipLoadImageFromFileD =
    int Function(Pointer<Utf16> path, Pointer<Pointer<Void>> image);
typedef _GdipImageN = Int32 Function(Pointer<Void> image);
typedef _GdipImageD = int Function(Pointer<Void> image);
typedef _GdipCountN =
    Int32 Function(Pointer<Void> image, Pointer<Uint32> count);
typedef _GdipCountD = int Function(Pointer<Void> image, Pointer<Uint32> count);
typedef _GdipDimListN =
    Int32 Function(Pointer<Void> image, Pointer<Uint8> guids, Uint32 count);
typedef _GdipDimListD =
    int Function(Pointer<Void> image, Pointer<Uint8> guids, int count);
typedef _GdipFrameCountN =
    Int32 Function(
      Pointer<Void> image,
      Pointer<Uint8> dimension,
      Pointer<Uint32> count,
    );
typedef _GdipFrameCountD =
    int Function(
      Pointer<Void> image,
      Pointer<Uint8> dimension,
      Pointer<Uint32> count,
    );
typedef _GdipSelectFrameN =
    Int32 Function(Pointer<Void> image, Pointer<Uint8> dimension, Uint32 frame);
typedef _GdipSelectFrameD =
    int Function(Pointer<Void> image, Pointer<Uint8> dimension, int frame);
typedef _GdipUintN =
    Int32 Function(Pointer<Void> image, Pointer<Uint32> value);
typedef _GdipUintD = int Function(Pointer<Void> image, Pointer<Uint32> value);
typedef _GdipFloatN =
    Int32 Function(Pointer<Void> image, Pointer<Float> value);
typedef _GdipFloatD = int Function(Pointer<Void> image, Pointer<Float> value);
typedef _GdipCreateFromHdcN =
    Int32 Function(Pointer<Void> hdc, Pointer<Pointer<Void>> graphics);
typedef _GdipCreateFromHdcD =
    int Function(Pointer<Void> hdc, Pointer<Pointer<Void>> graphics);
typedef _GdipSetInterpolationN =
    Int32 Function(Pointer<Void> graphics, Int32 mode);
typedef _GdipSetInterpolationD = int Function(Pointer<Void> graphics, int mode);
typedef _GdipDrawImageRectN =
    Int32 Function(
      Pointer<Void> graphics,
      Pointer<Void> image,
      Int32 x,
      Int32 y,
      Int32 width,
      Int32 height,
    );
typedef _GdipDrawImageRectD =
    int Function(
      Pointer<Void> graphics,
      Pointer<Void> image,
      int x,
      int y,
      int width,
      int height,
    );

/// Lazily-resolved Win32 entry points. Nothing is loaded until first use, so
/// pure helpers and non-Windows test runs never touch these libraries.
final class _Win32 {
  _Win32()
    : _winspool = DynamicLibrary.open('winspool.drv'),
      _gdi32 = DynamicLibrary.open('gdi32.dll'),
      _shell32 = DynamicLibrary.open('shell32.dll'),
      _shlwapi = DynamicLibrary.open('shlwapi.dll'),
      _kernel32 = DynamicLibrary.open('kernel32.dll'),
      _gdiplus = DynamicLibrary.open('gdiplus.dll');

  final DynamicLibrary _winspool;
  final DynamicLibrary _gdi32;
  final DynamicLibrary _shell32;
  final DynamicLibrary _shlwapi;
  final DynamicLibrary _kernel32;
  final DynamicLibrary _gdiplus;

  late final _EnumPrintersWD enumPrintersW = _winspool
      .lookupFunction<_EnumPrintersWN, _EnumPrintersWD>('EnumPrintersW');
  late final _GetDefaultPrinterWD getDefaultPrinterW = _winspool
      .lookupFunction<_GetDefaultPrinterWN, _GetDefaultPrinterWD>(
        'GetDefaultPrinterW',
      );
  late final _OpenPrinterWD openPrinterW = _winspool
      .lookupFunction<_OpenPrinterWN, _OpenPrinterWD>('OpenPrinterW');
  late final _ClosePrinterD closePrinter = _winspool
      .lookupFunction<_ClosePrinterN, _ClosePrinterD>('ClosePrinter');
  late final _DocumentPropertiesWD documentPropertiesW = _winspool
      .lookupFunction<_DocumentPropertiesWN, _DocumentPropertiesWD>(
        'DocumentPropertiesW',
      );
  late final _EnumJobsWD enumJobsW = _winspool
      .lookupFunction<_EnumJobsWN, _EnumJobsWD>('EnumJobsW');
  late final _GetJobWD getJobW = _winspool.lookupFunction<_GetJobWN, _GetJobWD>(
    'GetJobW',
  );
  late final _SetJobWD setJobW = _winspool.lookupFunction<_SetJobWN, _SetJobWD>(
    'SetJobW',
  );

  late final _CreateDCWD createDCW = _gdi32
      .lookupFunction<_CreateDCWN, _CreateDCWD>('CreateDCW');
  late final _DeleteDCD deleteDC = _gdi32.lookupFunction<_DeleteDCN, _DeleteDCD>(
    'DeleteDC',
  );
  late final _GetDeviceCapsD getDeviceCaps = _gdi32
      .lookupFunction<_GetDeviceCapsN, _GetDeviceCapsD>('GetDeviceCaps');
  late final _StartDocWD startDocW = _gdi32
      .lookupFunction<_StartDocWN, _StartDocWD>('StartDocW');
  late final _HdcStatusD endDoc = _gdi32.lookupFunction<_HdcStatusN, _HdcStatusD>(
    'EndDoc',
  );
  late final _HdcStatusD abortDoc = _gdi32
      .lookupFunction<_HdcStatusN, _HdcStatusD>('AbortDoc');
  late final _HdcStatusD startPage = _gdi32
      .lookupFunction<_HdcStatusN, _HdcStatusD>('StartPage');
  late final _HdcStatusD endPage = _gdi32
      .lookupFunction<_HdcStatusN, _HdcStatusD>('EndPage');
  late final _ResetDCWD resetDCW = _gdi32
      .lookupFunction<_ResetDCWN, _ResetDCWD>('ResetDCW');
  late final _SetStretchBltModeD setStretchBltMode = _gdi32
      .lookupFunction<_SetStretchBltModeN, _SetStretchBltModeD>(
        'SetStretchBltMode',
      );
  late final _StretchDIBitsD stretchDIBits = _gdi32
      .lookupFunction<_StretchDIBitsN, _StretchDIBitsD>('StretchDIBits');

  late final _ShellExecuteWD shellExecuteW = _shell32
      .lookupFunction<_ShellExecuteWN, _ShellExecuteWD>('ShellExecuteW');
  late final _AssocQueryStringWD assocQueryStringW = _shlwapi
      .lookupFunction<_AssocQueryStringWN, _AssocQueryStringWD>(
        'AssocQueryStringW',
      );
  late final _GetLastErrorD getLastError = _kernel32
      .lookupFunction<_GetLastErrorN, _GetLastErrorD>('GetLastError');

  late final _GdiplusStartupD gdiplusStartup = _gdiplus
      .lookupFunction<_GdiplusStartupN, _GdiplusStartupD>('GdiplusStartup');
  late final _GdiplusShutdownD gdiplusShutdown = _gdiplus
      .lookupFunction<_GdiplusShutdownN, _GdiplusShutdownD>('GdiplusShutdown');
  late final _GdipLoadImageFromFileD gdipLoadImageFromFile = _gdiplus
      .lookupFunction<_GdipLoadImageFromFileN, _GdipLoadImageFromFileD>(
        'GdipLoadImageFromFile',
      );
  late final _GdipImageD gdipDisposeImage = _gdiplus
      .lookupFunction<_GdipImageN, _GdipImageD>('GdipDisposeImage');
  late final _GdipCountD gdipImageGetFrameDimensionsCount = _gdiplus
      .lookupFunction<_GdipCountN, _GdipCountD>(
        'GdipImageGetFrameDimensionsCount',
      );
  late final _GdipDimListD gdipImageGetFrameDimensionsList = _gdiplus
      .lookupFunction<_GdipDimListN, _GdipDimListD>(
        'GdipImageGetFrameDimensionsList',
      );
  late final _GdipFrameCountD gdipImageGetFrameCount = _gdiplus
      .lookupFunction<_GdipFrameCountN, _GdipFrameCountD>(
        'GdipImageGetFrameCount',
      );
  late final _GdipSelectFrameD gdipImageSelectActiveFrame = _gdiplus
      .lookupFunction<_GdipSelectFrameN, _GdipSelectFrameD>(
        'GdipImageSelectActiveFrame',
      );
  late final _GdipUintD gdipGetImageWidth = _gdiplus
      .lookupFunction<_GdipUintN, _GdipUintD>('GdipGetImageWidth');
  late final _GdipUintD gdipGetImageHeight = _gdiplus
      .lookupFunction<_GdipUintN, _GdipUintD>('GdipGetImageHeight');
  late final _GdipFloatD gdipGetImageHorizontalResolution = _gdiplus
      .lookupFunction<_GdipFloatN, _GdipFloatD>(
        'GdipGetImageHorizontalResolution',
      );
  late final _GdipFloatD gdipGetImageVerticalResolution = _gdiplus
      .lookupFunction<_GdipFloatN, _GdipFloatD>(
        'GdipGetImageVerticalResolution',
      );
  late final _GdipCreateFromHdcD gdipCreateFromHdc = _gdiplus
      .lookupFunction<_GdipCreateFromHdcN, _GdipCreateFromHdcD>(
        'GdipCreateFromHDC',
      );
  late final _GdipSetInterpolationD gdipSetInterpolationMode = _gdiplus
      .lookupFunction<_GdipSetInterpolationN, _GdipSetInterpolationD>(
        'GdipSetInterpolationMode',
      );
  late final _GdipDrawImageRectD gdipDrawImageRectI = _gdiplus
      .lookupFunction<_GdipDrawImageRectN, _GdipDrawImageRectD>(
        'GdipDrawImageRectI',
      );
  late final _GdipImageD gdipDeleteGraphics = _gdiplus
      .lookupFunction<_GdipImageN, _GdipImageD>('GdipDeleteGraphics');
}

final _Win32 _w = _Win32();

// ---- Native helpers ----------------------------------------------------------

Pointer<Uint8> _alloc(int bytes) => calloc<Uint8>(bytes <= 0 ? 1 : bytes);

/// Port of `wide_buffer_to_string`: the text before the first NUL, or null
/// when there is no NUL or the text is empty.
String? _wideBufferToString(List<int> buffer) {
  final length = buffer.indexOf(0);
  if (length < 0 || length == 0) {
    return null;
  }
  return String.fromCharCodes(buffer.sublist(0, length));
}

/// Port of `wide_ptr_to_string`: bounded read of a NUL-terminated string.
String? _widePtrToString(Pointer<Utf16> pointer) {
  if (pointer == nullptr) {
    return null;
  }
  final units = pointer.cast<Uint16>();
  var length = 0;
  while (length < _maxWideChars && units[length] != 0) {
    length++;
  }
  if (length == 0 || length == _maxWideChars) {
    return null;
  }
  return String.fromCharCodes(units.asTypedList(length));
}

int _lastError() => _w.getLastError();

/// Port of the `DEVMODEW` view used by this adapter. Offsets are the x64
/// layout: dmFields at 72, dmOrientation at 76, dmPaperSize at 78, dmColor at
/// 92, and dmDuplex at 94.
final class _DevMode {
  _DevMode(this.size) : pointer = _alloc(size);

  final int size;
  final Pointer<Uint8> pointer;

  ByteData get _data => ByteData.sublistView(pointer.asTypedList(size));

  int get fields => _data.getUint32(72, Endian.little);
  set fields(int value) => _data.setUint32(72, value, Endian.little);
  int get orientation => _data.getInt16(76, Endian.little);
  set orientation(int value) => _data.setInt16(76, value, Endian.little);
  int get paperSize => _data.getInt16(78, Endian.little);
  set paperSize(int value) => _data.setInt16(78, value, Endian.little);
  int get color => _data.getInt16(92, Endian.little);
  set color(int value) => _data.setInt16(92, value, Endian.little);
  int get duplex => _data.getInt16(94, Endian.little);
  set duplex(int value) => _data.setInt16(94, value, Endian.little);

  void free() => calloc.free(pointer);
}

final class _PrinterHandle {
  _PrinterHandle._(this.handle);

  final Pointer<Void> handle;

  /// Port of `PrinterHandle::open`.
  static _PrinterHandle open(String printer) {
    final name = printer.toNativeUtf16();
    final out = calloc<Pointer<Void>>();
    try {
      if (_w.openPrinterW(name, out, nullptr) == 0) {
        throw PrintFailure.failed('Windows could not open the printer');
      }
      return _PrinterHandle._(out.value);
    } finally {
      calloc.free(name);
      calloc.free(out);
    }
  }

  void close() => _w.closePrinter(handle);
}

final class _DeviceContext {
  _DeviceContext(this.hdc);

  Pointer<Void> hdc;

  void close() => _w.deleteDC(hdc);
}

// ---- Settings to DEVMODE mapping --------------------------------------------

/// Port of `RequestedDevMode`.
final class RequestedDevMode {
  const RequestedDevMode({this.paper, this.orientation, this.color, this.duplex});

  final int? paper;
  final int? orientation;
  final int? color;
  final int? duplex;

  @override
  bool operator ==(Object other) =>
      other is RequestedDevMode &&
      other.paper == paper &&
      other.orientation == orientation &&
      other.color == color &&
      other.duplex == duplex;

  @override
  int get hashCode => Object.hash(paper, orientation, color, duplex);

  @override
  String toString() =>
      'RequestedDevMode($paper, $orientation, $color, $duplex)';
}

/// Port of `requested_devmode`. Rejects unmapped values before any native call.
RequestedDevMode requestedDevmode(PrintSettings settings) {
  final int? paper = switch (settings.paper) {
    'auto' => null,
    'A3' => _dmpaperA3,
    'A4' => _dmpaperA4,
    'A5' => _dmpaperA5,
    'Letter' => _dmpaperLetter,
    'Legal' => _dmpaperLegal,
    _ => throw PrintFailure.unsupportedOptions(
      'Windows does not support the selected paper size',
    ),
  };
  final int? orientation = switch (settings.orientation) {
    'auto' => null,
    'portrait' => _dmorientPortrait,
    'landscape' => _dmorientLandscape,
    _ => throw PrintFailure.unsupportedOptions(
      'Windows does not support the selected orientation',
    ),
  };
  final int? color = switch (settings.color) {
    'printer' => null,
    'color' => _dmcolorColor,
    'grayscale' => _dmcolorMonochrome,
    _ => throw PrintFailure.unsupportedOptions(
      'Windows does not support the selected color mode',
    ),
  };
  final int? duplex = switch (settings.duplex) {
    'printer' => null,
    'simplex' => _dmdupSimplex,
    'long' => _dmdupVertical,
    'short' => _dmdupHorizontal,
    _ => throw PrintFailure.unsupportedOptions(
      'Windows does not support the selected duplex mode',
    ),
  };
  if (!const {'fit', 'actual', 'shrink'}.contains(settings.scaling)) {
    throw PrintFailure.unsupportedOptions(
      'Windows does not support the selected scaling mode',
    );
  }
  return RequestedDevMode(
    paper: paper,
    orientation: orientation,
    color: color,
    duplex: duplex,
  );
}

/// Port of `validated_devmode`: reads printer defaults, applies the requested
/// fields, and asks the driver to validate them. Returns a buffer the caller
/// frees.
_DevMode _validatedDevmode(String printer, PrintSettings settings) {
  final requested = requestedDevmode(settings);
  final printerName = printer.toNativeUtf16();
  final handle = _PrinterHandle.open(printer);
  try {
    final size = _w.documentPropertiesW(
      nullptr,
      handle.handle,
      printerName,
      nullptr,
      nullptr,
      0,
    );
    if (size < _devModeSize) {
      throw PrintFailure.unsupportedOptions(
        'Windows printer settings are unavailable',
      );
    }
    final devmode = _DevMode(size);
    try {
      if (_w.documentPropertiesW(
            nullptr,
            handle.handle,
            printerName,
            devmode.pointer,
            nullptr,
            _dmOut,
          ) <
          0) {
        throw PrintFailure.unsupportedOptions(
          'Windows printer defaults could not be read',
        );
      }
      _applyRequestedDevmode(devmode, requested);
      if (_w.documentPropertiesW(
            nullptr,
            handle.handle,
            printerName,
            devmode.pointer,
            devmode.pointer,
            _dmIn | _dmOut,
          ) <
          0) {
        throw PrintFailure.unsupportedOptions(
          'the printer rejected the selected options',
        );
      }
      if (!_devmodeMatches(devmode, requested)) {
        throw PrintFailure.unsupportedOptions(
          'the printer changed an unsupported selected option',
        );
      }
      return devmode;
    } on Object {
      devmode.free();
      rethrow;
    }
  } finally {
    handle.close();
    calloc.free(printerName);
  }
}

/// Port of `apply_requested_devmode`.
void _applyRequestedDevmode(_DevMode devmode, RequestedDevMode requested) {
  if (requested.paper != null) {
    devmode.fields = devmode.fields | _dmPaperSizeField;
    devmode.paperSize = requested.paper!;
  }
  if (requested.orientation != null) {
    devmode.fields = devmode.fields | _dmOrientationField;
    devmode.orientation = requested.orientation!;
  }
  if (requested.color != null) {
    devmode.fields = devmode.fields | _dmColorField;
    devmode.color = requested.color!;
  }
  if (requested.duplex != null) {
    devmode.fields = devmode.fields | _dmDuplexField;
    devmode.duplex = requested.duplex!;
  }
}

/// Port of `devmode_matches`.
bool _devmodeMatches(_DevMode devmode, RequestedDevMode requested) =>
    (requested.paper == null || devmode.paperSize == requested.paper) &&
    (requested.orientation == null ||
        devmode.orientation == requested.orientation) &&
    (requested.color == null || devmode.color == requested.color) &&
    (requested.duplex == null || devmode.duplex == requested.duplex);

// ---- Printer and job enumeration ----------------------------------------------

List<String> _enumeratePrinters() {
  const flags = _printerEnumLocal | _printerEnumConnections;
  final needed = calloc<Uint32>();
  final returned = calloc<Uint32>();
  try {
    _w.enumPrintersW(flags, nullptr, 4, nullptr, 0, needed, returned);
    if (needed.value == 0) {
      return const [];
    }
    final size = needed.value;
    final buffer = _alloc(size);
    try {
      final ok = _w.enumPrintersW(
        flags,
        nullptr,
        4,
        buffer,
        size,
        needed,
        returned,
      );
      if (ok == 0) {
        throw PrintFailure.failed('Windows printer enumeration failed');
      }
      final entries = buffer.cast<_PrinterInfo4W>();
      final names = <String>[];
      for (var i = 0; i < returned.value; i++) {
        final name = _widePtrToString((entries + i).ref.pPrinterName);
        if (name != null) {
          names.add(name);
        }
      }
      return names;
    } finally {
      calloc.free(buffer);
    }
  } finally {
    calloc.free(needed);
    calloc.free(returned);
  }
}

String? _defaultPrinter() {
  final length = calloc<Uint32>();
  try {
    _w.getDefaultPrinterW(nullptr, length);
    if (length.value == 0) {
      return null;
    }
    final buffer = calloc<Uint16>(length.value);
    try {
      if (_w.getDefaultPrinterW(buffer, length) == 0) {
        return null;
      }
      return _wideBufferToString(buffer.asTypedList(length.value));
    } finally {
      calloc.free(buffer);
    }
  } finally {
    calloc.free(length);
  }
}

final class _SpoolJob {
  const _SpoolJob(this.id, this.document);

  final int id;
  final String document;
}

/// Port of `jobs`: enumerates every job on [printer] (level 1).
List<_SpoolJob> _jobs(String printer) {
  final handle = _PrinterHandle.open(printer);
  try {
    final needed = calloc<Uint32>();
    final returned = calloc<Uint32>();
    try {
      _w.enumJobsW(handle.handle, 0, 0xFFFFFFFF, 1, nullptr, 0, needed, returned);
      if (needed.value == 0) {
        return const [];
      }
      final size = needed.value;
      final buffer = _alloc(size);
      try {
        final ok = _w.enumJobsW(
          handle.handle,
          0,
          0xFFFFFFFF,
          1,
          buffer,
          size,
          needed,
          returned,
        );
        if (ok == 0) {
          throw PrintFailure.failed(
            'Windows print queue could not be enumerated',
          );
        }
        final entries = buffer.cast<_JobInfo1W>();
        return [
          for (var i = 0; i < returned.value; i++)
            _SpoolJob(
              (entries + i).ref.jobId,
              _widePtrToString((entries + i).ref.pDocument) ?? '',
            ),
        ];
      } finally {
        calloc.free(buffer);
      }
    } finally {
      calloc.free(needed);
      calloc.free(returned);
    }
  } finally {
    handle.close();
  }
}

Set<int> _jobIds(String printer) => {for (final job in _jobs(printer)) job.id};

/// Port of `job_status`: up to three attempts to read JOB_INFO_1 status.
/// Returns null when the spooler has released the record.
int? _jobStatus(Pointer<Void> handle, int jobId) {
  for (var attempt = 0; attempt < 3; attempt++) {
    final needed = calloc<Uint32>();
    try {
      _w.getJobW(handle, jobId, 1, nullptr, 0, needed);
      if (needed.value == 0) {
        return switch (classifyWindowsJobQueryError(_lastError())) {
          WindowsJobQueryError.released => null,
          _ => throw windowsPostSubmissionObservationError(
            WindowsPostSubmissionObservation.jobRead,
          ),
        };
      }
      final size = needed.value;
      final buffer = _alloc(size);
      try {
        if (_w.getJobW(handle, jobId, 1, buffer, size, needed) != 0) {
          return buffer.cast<_JobInfo1W>().ref.status;
        }
        // EPSON and other fast-purging drivers can release the record between
        // the sizing call and the buffered call.
        switch (classifyWindowsJobQueryError(_lastError())) {
          case WindowsJobQueryError.released:
            return null;
          case WindowsJobQueryError.retryWithLargerBuffer:
            continue;
          case WindowsJobQueryError.uncertain:
            throw windowsPostSubmissionObservationError(
              WindowsPostSubmissionObservation.jobRead,
            );
        }
      } finally {
        calloc.free(buffer);
      }
    } finally {
      calloc.free(needed);
    }
  }
  throw windowsPostSubmissionObservationError(
    WindowsPostSubmissionObservation.jobRead,
  );
}

/// Port of `wait_for_job`. Polls spool state until release, completion,
/// cancellation, or timeout. Cancellation and timeout send one JOB_CONTROL_CANCEL.
void _waitForJob(
  String printer,
  int jobId,
  DateTime deadline,
  bool Function() cancelled,
) {
  final observationStarted = DateTime.now();
  final handle = _awaitPrinterHandle(
    printer,
    deadline,
    cancelled,
    observationStarted,
  );
  try {
    DateTime? readErrorSince;
    while (true) {
      if (cancelled()) {
        final requestAccepted =
            _w.setJobW(handle.handle, jobId, 0, nullptr, _jobControlCancel) !=
            0;
        throw windowsPostSubmissionCancellation(
          requestAccepted: requestAccepted,
        );
      }
      if (!DateTime.now().isBefore(deadline)) {
        _w.setJobW(handle.handle, jobId, 0, nullptr, _jobControlCancel);
        throw PrintFailure.uncertain(
          'Windows spool job did not finish before its timeout',
        );
      }
      final int? status;
      try {
        status = _jobStatus(handle.handle, jobId);
      } on PrintFailure {
        final firstError = readErrorSince ??= DateTime.now();
        if (DateTime.now().difference(firstError) >= _observationErrorGrace) {
          rethrow;
        }
        sleep(_pollInterval);
        continue;
      }
      // EndDoc succeeded before this observer starts. Drivers normally remove
      // a completed job at once, so absence is the normal "released" state.
      switch (classifyWindowsSubmittedJobObservation(status)) {
        case WindowsSubmittedJobStatus.complete:
          return;
        case WindowsSubmittedJobStatus.uncertain:
          throw windowsPostSubmissionObservationError(
            WindowsPostSubmissionObservation.statusFault,
          );
        case WindowsSubmittedJobStatus.pending:
          readErrorSince = null;
          sleep(_pollInterval);
      }
    }
  } finally {
    handle.close();
  }
}

_PrinterHandle _awaitPrinterHandle(
  String printer,
  DateTime deadline,
  bool Function() cancelled,
  DateTime started,
) {
  while (true) {
    try {
      return _PrinterHandle.open(printer);
    } on PrintFailure {
      if (cancelled()) {
        throw windowsPostSubmissionCancellation(requestAccepted: false);
      }
      if (!DateTime.now().isBefore(deadline)) {
        throw PrintFailure.uncertain(
          'Windows spool job did not finish before its timeout',
        );
      }
      if (DateTime.now().difference(started) < _observationErrorGrace) {
        sleep(_pollInterval);
        continue;
      }
      throw windowsPostSubmissionObservationError(
        WindowsPostSubmissionObservation.printerOpen,
      );
    }
  }
}

// ---- Pure scaling and destination rules --------------------------------------

/// Port of `page_scale`.
double pageScale(
  int imageWidth,
  int imageHeight,
  int printableWidth,
  int printableHeight,
  int dpiX,
  int dpiY,
  String scaling,
) {
  if (imageWidth <= 0 ||
      imageHeight <= 0 ||
      printableWidth <= 0 ||
      printableHeight <= 0 ||
      dpiX <= 0 ||
      dpiY <= 0) {
    throw PrintFailure.unsupportedOptions(
      'Windows printer has invalid page metrics',
    );
  }
  final fit = math.min(
    printableWidth / imageWidth,
    printableHeight / imageHeight,
  );
  final actual = math.min(dpiX, dpiY) / _pdfDpi;
  return switch (scaling) {
    'fit' => fit,
    'shrink' => math.min(fit, actual),
    'actual' => actual,
    _ => throw PrintFailure.unsupportedOptions(
      'Windows does not support the selected scaling mode',
    ),
  };
}

/// Port of `image_destination`.
(int, int) imageDestination(
  int imageWidth,
  int imageHeight,
  double imageDpiX,
  double imageDpiY,
  int printableWidth,
  int printableHeight,
  int printerDpiX,
  int printerDpiY,
  String scaling,
) {
  if (imageWidth <= 0 ||
      imageHeight <= 0 ||
      imageDpiX <= 0.0 ||
      imageDpiY <= 0.0 ||
      printableWidth <= 0 ||
      printableHeight <= 0 ||
      printerDpiX <= 0 ||
      printerDpiY <= 0) {
    throw PrintFailure.unsupportedOptions(
      'Windows printer or image has invalid page metrics',
    );
  }
  final actualWidth = imageWidth * printerDpiX / imageDpiX;
  final actualHeight = imageHeight * printerDpiY / imageDpiY;
  final fit = math.min(
    printableWidth / actualWidth,
    printableHeight / actualHeight,
  );
  final scale = switch (scaling) {
    'fit' => fit,
    'shrink' => math.min(fit, 1.0),
    'actual' => 1.0,
    _ => throw PrintFailure.unsupportedOptions(
      'Windows does not support the selected scaling mode',
    ),
  };
  return (
    math.max(1, (actualWidth * scale).round()),
    math.max(1, (actualHeight * scale).round()),
  );
}

// ---- Device-context operations ----------------------------------------------

/// Port of `reset_auto_orientation`. Automatic orientation follows the page's
/// aspect ratio; any other orientation setting is left untouched.
void _resetAutoOrientation(
  _DeviceContext context,
  _DevMode devmode,
  PrintSettings settings,
  int width,
  int height,
) {
  if (settings.orientation != 'auto') {
    return;
  }
  final orientation = width > height ? _dmorientLandscape : _dmorientPortrait;
  devmode.fields = devmode.fields | _dmOrientationField;
  devmode.orientation = orientation;
  final reset = _w.resetDCW(context.hdc, devmode.pointer);
  if (reset == nullptr) {
    throw PrintFailure.unsupportedOptions(
      'the printer rejected automatic page orientation',
    );
  }
  context.hdc = reset;
}

/// Port of `draw_bitmap`: fits a 24-bit BGR bitmap into the printable area.
void _drawBitmap(Pointer<Void> hdc, PngBgrBitmap bitmap, String scaling) {
  final printableWidth = _w.getDeviceCaps(hdc, _horzres);
  final printableHeight = _w.getDeviceCaps(hdc, _vertres);
  final dpiX = _w.getDeviceCaps(hdc, _logpixelsx);
  final dpiY = _w.getDeviceCaps(hdc, _logpixelsy);
  if (printableWidth <= 0 || printableHeight <= 0) {
    throw PrintFailure.unsupportedOptions(
      'Windows printer has no printable page area',
    );
  }
  final scale = pageScale(
    bitmap.width,
    bitmap.height,
    printableWidth,
    printableHeight,
    dpiX,
    dpiY,
    scaling,
  );
  final width = (bitmap.width * scale).round();
  final height = (bitmap.height * scale).round();
  final x = (printableWidth - width) ~/ 2;
  final y = (printableHeight - height) ~/ 2;
  final info = _alloc(40);
  final bits = _alloc(bitmap.bgr.length);
  try {
    final header = ByteData.sublistView(info.asTypedList(40));
    header.setUint32(0, 40, Endian.little); // biSize
    header.setInt32(4, bitmap.width, Endian.little); // biWidth
    header.setInt32(8, -bitmap.height, Endian.little); // biHeight (top-down)
    header.setUint16(12, 1, Endian.little); // biPlanes
    header.setUint16(14, 24, Endian.little); // biBitCount
    header.setUint32(16, _biRgb, Endian.little); // biCompression
    bits.asTypedList(bitmap.bgr.length).setAll(0, bitmap.bgr);
    _w.setStretchBltMode(hdc, scale < 1.0 ? _halftone : _coloroncolor);
    final copied = _w.stretchDIBits(
      hdc,
      x,
      y,
      width,
      height,
      0,
      0,
      bitmap.width,
      bitmap.height,
      bits,
      info,
      _dibRgbColors,
      _srccopy,
    );
    if (copied <= 0) {
      throw PrintFailure.failed('Windows could not draw a rendered PDF page');
    }
  } finally {
    calloc.free(info);
    calloc.free(bits);
  }
}

// ---- Windows.Data.Pdf seam (UNVERIFIED) --------------------------------------

/// One in-memory PDF, the `InMemoryPdf` surface of `windows_pdf.rs`.
abstract interface class WindowsInMemoryPdf {
  int pageCount();
  Uint8List renderPagePng(int pageIndex, {required int dpi});
}

/// Source of PDF pages for the Windows print path. The production
/// implementation is [DefaultWindowsPdfRenderBackend]. Tests inject a fake to
/// drive the controlled submit sequence without Windows.
abstract interface class WindowsPdfRenderBackend {
  /// `InMemoryPdf::load`. Throws [PrintFailure] when the bytes are not a PDF
  /// this backend can open.
  WindowsInMemoryPdf load(Uint8List bytes);

  /// `windows_pdf::page_count(path)`.
  int filePageCount(String path);

  /// `windows_pdf::render_page_png(path, dpi, page)`, returned as bytes.
  Uint8List renderFilePagePng(String path, int pageIndex, {required int dpi});
}

/// Production backend. The file-path page count uses the verified pure-Dart
/// parser. Rendering and in-memory loading go through Windows.Data.Pdf, which
/// has no Dart binding yet, so they fail closed.
final class DefaultWindowsPdfRenderBackend implements WindowsPdfRenderBackend {
  const DefaultWindowsPdfRenderBackend();

  static const String _unavailable =
      'Windows PDF rendering is unavailable on this platform';

  @override
  WindowsInMemoryPdf load(Uint8List bytes) =>
      throw PrintFailure.failed(_unavailable);

  @override
  int filePageCount(String path) {
    try {
      return windows_pdf.countPdfPages(path);
    } on BackendError catch (error) {
      throw PrintFailure.failed(error.message);
    }
  }

  @override
  Uint8List renderFilePagePng(String path, int pageIndex, {required int dpi}) {
    try {
      return const windows_pdf.WindowsPdfRasterizer().renderPagePng(
        path,
        pageIndex,
        dpi: dpi,
      );
    } on BackendError catch (error) {
      throw PrintFailure.failed(error.message);
    }
  }
}

// ---- Print routes -----------------------------------------------------------

/// Port of `render_pdf_pages` (path route).
void _renderPdfPages(
  _DeviceContext context,
  _DevMode devmode,
  String path,
  PrintSettings settings,
  DateTime deadline,
  bool Function() cancelled,
  WindowsPdfRenderBackend pdf,
) {
  final pageCount = pdf.filePageCount(path);
  final pages = _selectPages(settings.pageRange, pageCount);
  for (final page in pages) {
    if (cancelled()) {
      throw PrintFailure.cancelled(
        'Windows PDF job was cancelled while rendering',
        uncertain: false,
      );
    }
    if (!DateTime.now().isBefore(deadline)) {
      throw PrintFailure.failed('Windows PDF rendering exceeded the print timeout');
    }
    final bitmap = decodePngBgr(pdf.renderFilePagePng(path, page, dpi: _pdfDpi));
    _resetAutoOrientation(context, devmode, settings, bitmap.width, bitmap.height);
    _startPdfPage(context);
    _drawAndEndPage(context, bitmap, settings.scaling);
  }
}

/// Port of `render_in_memory_pdf_pages` (controlled route). Each page's bytes
/// are consumed as rendered; no path is reopened.
void _renderInMemoryPdfPages(
  _DeviceContext context,
  _DevMode devmode,
  WindowsInMemoryPdf document,
  List<int> pages,
  PrintSettings settings,
  DateTime deadline,
  bool Function() cancelled,
) {
  for (final page in pages) {
    if (cancelled()) {
      throw PrintFailure.cancelled(
        'Windows PDF job was cancelled while rendering',
        uncertain: false,
      );
    }
    if (!DateTime.now().isBefore(deadline)) {
      throw PrintFailure.failed('Windows PDF rendering exceeded the print timeout');
    }
    final bitmap = consumeControlledPageBytes<PngBgrBitmap>(
      () => document.renderPagePng(page, dpi: _pdfDpi),
      (bytes) => decodePngBgr(Uint8List.fromList(bytes)),
    );
    _resetAutoOrientation(context, devmode, settings, bitmap.width, bitmap.height);
    _startPdfPage(context);
    _drawAndEndPage(context, bitmap, settings.scaling);
  }
}

void _startPdfPage(_DeviceContext context) {
  if (_w.startPage(context.hdc) <= 0) {
    throw PrintFailure.failed('Windows could not start a PDF page');
  }
}

/// Draws the bitmap, always ends the page, then reports the first failure.
void _drawAndEndPage(_DeviceContext context, PngBgrBitmap bitmap, String scaling) {
  PrintFailure? printed;
  try {
    _drawBitmap(context.hdc, bitmap, scaling);
  } on PrintFailure catch (failure) {
    printed = failure;
  }
  final ended = _w.endPage(context.hdc) > 0;
  if (printed != null) {
    throw printed;
  }
  if (!ended) {
    throw PrintFailure.failed('Windows could not finish a PDF page');
  }
}

List<int> _selectPages(String range, int pageCount) {
  try {
    return selectedPdfPages(range, pageCount);
  } on BackendError catch (error) {
    throw PrintFailure.failed(error.message);
  }
}

/// Port of `draw_image_frames`.
void _drawImageFrames(
  _DeviceContext context,
  _DevMode devmode,
  _GdiPlusImage image,
  Uint8List? dimension,
  int frames,
  PrintSettings settings,
  DateTime deadline,
  bool Function() cancelled,
) {
  for (var frame = 0; frame < frames; frame++) {
    if (cancelled()) {
      throw PrintFailure.cancelled(
        'Windows image job was cancelled while rendering',
        uncertain: false,
      );
    }
    if (!DateTime.now().isBefore(deadline)) {
      throw PrintFailure.failed('Windows image rendering exceeded the print timeout');
    }
    image.selectFrame(dimension, frame);
    final metrics = image.metrics();
    _resetAutoOrientation(context, devmode, settings, metrics.width, metrics.height);
    final printableWidth = _w.getDeviceCaps(context.hdc, _horzres);
    final printableHeight = _w.getDeviceCaps(context.hdc, _vertres);
    final printerDpiX = _w.getDeviceCaps(context.hdc, _logpixelsx);
    final printerDpiY = _w.getDeviceCaps(context.hdc, _logpixelsy);
    final (destinationWidth, destinationHeight) = imageDestination(
      metrics.width,
      metrics.height,
      metrics.dpiX,
      metrics.dpiY,
      printableWidth,
      printableHeight,
      printerDpiX,
      printerDpiY,
      settings.scaling,
    );
    final x = (printableWidth - destinationWidth) ~/ 2;
    final y = (printableHeight - destinationHeight) ~/ 2;
    if (_w.startPage(context.hdc) <= 0) {
      throw PrintFailure.failed('Windows could not start an image page');
    }
    var drawn = false;
    final graphicsOut = calloc<Pointer<Void>>();
    try {
      final status = _w.gdipCreateFromHdc(context.hdc, graphicsOut);
      final graphics = graphicsOut.value;
      if (status == _gdipOk && graphics != nullptr) {
        final interpolation =
            _w.gdipSetInterpolationMode(
              graphics,
              _gdipInterpolationHighQualityBicubic,
            ) ==
            _gdipOk;
        drawn =
            interpolation &&
            _w.gdipDrawImageRectI(
                  graphics,
                  image.image,
                  x,
                  y,
                  destinationWidth,
                  destinationHeight,
                ) ==
                _gdipOk;
        _w.gdipDeleteGraphics(graphics);
      }
    } finally {
      calloc.free(graphicsOut);
    }
    final ended = _w.endPage(context.hdc) > 0;
    if (!drawn || !ended) {
      throw PrintFailure.failed('Windows could not draw an image page');
    }
  }
}

/// Port of `GdiPlusRuntime`.
final class _GdiPlusRuntime {
  _GdiPlusRuntime._(this.token);

  final int token;

  static _GdiPlusRuntime start() {
    final input = calloc<_GdiplusStartupInput>();
    final token = calloc<IntPtr>();
    try {
      input.ref.gdiplusVersion = 1;
      input.ref.debugEventCallback = nullptr;
      input.ref.suppressBackgroundThread = 0;
      input.ref.suppressExternalCodecs = 1;
      final status = _w.gdiplusStartup(token, input, nullptr);
      if (status != _gdipOk || token.value == 0) {
        throw PrintFailure.unsupported('Windows image codecs are unavailable');
      }
      return _GdiPlusRuntime._(token.value);
    } finally {
      calloc.free(input);
      calloc.free(token);
    }
  }

  void shutdown() => _w.gdiplusShutdown(token);
}

/// Port of `GdiPlusImage`.
final class _GdiPlusImage {
  _GdiPlusImage._(this.image);

  final Pointer<Void> image;

  static _GdiPlusImage load(String path) {
    final wide = path.toNativeUtf16();
    final out = calloc<Pointer<Void>>();
    try {
      if (_w.gdipLoadImageFromFile(wide, out) != _gdipOk ||
          out.value == nullptr) {
        throw PrintFailure.unsupported('Windows could not decode this image');
      }
      return _GdiPlusImage._(out.value);
    } finally {
      calloc.free(wide);
      calloc.free(out);
    }
  }

  void dispose() => _w.gdipDisposeImage(image);

  /// Port of `frame_dimension`. Returns the frame-dimension GUID that holds the
  /// most frames, or null with one frame when there is no dimension.
  (Uint8List?, int) frameDimension() {
    final count = calloc<Uint32>();
    try {
      if (_w.gdipImageGetFrameDimensionsCount(image, count) != _gdipOk) {
        throw PrintFailure.failed('Windows could not inspect image frames');
      }
      final dimensionCount = count.value;
      if (dimensionCount == 0) {
        return (null, 1);
      }
      if (dimensionCount > 16) {
        throw PrintFailure.unsupported(
          'Windows image has too many frame dimensions',
        );
      }
      final guids = _alloc(dimensionCount * 16);
      try {
        if (_w.gdipImageGetFrameDimensionsList(image, guids, dimensionCount) !=
            _gdipOk) {
          throw PrintFailure.failed(
            'Windows could not inspect image frame dimensions',
          );
        }
        final all = guids.asTypedList(dimensionCount * 16);
        Uint8List? selected;
        var selectedCount = 0;
        for (var i = 0; i < dimensionCount; i++) {
          final guid = Uint8List.fromList(all.sublist(i * 16, i * 16 + 16));
          final guidPointer = _alloc(16);
          final frames = calloc<Uint32>();
          try {
            guidPointer.asTypedList(16).setAll(0, guid);
            if (_w.gdipImageGetFrameCount(image, guidPointer, frames) !=
                _gdipOk) {
              throw PrintFailure.failed('Windows could not count image frames');
            }
            if (frames.value > selectedCount) {
              selected = guid;
              selectedCount = frames.value;
            }
          } finally {
            calloc.free(guidPointer);
            calloc.free(frames);
          }
        }
        return (selected, selectedCount);
      } finally {
        calloc.free(guids);
      }
    } finally {
      calloc.free(count);
    }
  }

  void selectFrame(Uint8List? dimension, int frame) {
    if (dimension == null) {
      return;
    }
    final guid = _alloc(16);
    try {
      guid.asTypedList(16).setAll(0, dimension);
      if (_w.gdipImageSelectActiveFrame(image, guid, frame) != _gdipOk) {
        throw PrintFailure.failed('Windows could not select an image frame');
      }
    } finally {
      calloc.free(guid);
    }
  }

  /// Port of `metrics`: pixel size and DPI, with 96 DPI when the stored value
  /// is missing or invalid.
  ({int width, int height, double dpiX, double dpiY}) metrics() {
    final width = calloc<Uint32>();
    final height = calloc<Uint32>();
    final dpiX = calloc<Float>();
    final dpiY = calloc<Float>();
    try {
      final failed =
          _w.gdipGetImageWidth(image, width) != _gdipOk ||
          _w.gdipGetImageHeight(image, height) != _gdipOk ||
          _w.gdipGetImageHorizontalResolution(image, dpiX) != _gdipOk ||
          _w.gdipGetImageVerticalResolution(image, dpiY) != _gdipOk ||
          width.value == 0 ||
          height.value == 0;
      if (failed) {
        throw PrintFailure.unsupported('Windows image has invalid dimensions');
      }
      if (width.value > 0x7fffffff) {
        throw PrintFailure.unsupported('Windows image is too wide');
      }
      if (height.value > 0x7fffffff) {
        throw PrintFailure.unsupported('Windows image is too tall');
      }
      final x = dpiX.value;
      final y = dpiY.value;
      return (
        width: width.value,
        height: height.value,
        dpiX: x.isFinite && x >= 1.0 ? x : 96.0,
        dpiY: y.isFinite && y >= 1.0 ? y : 96.0,
      );
    } finally {
      calloc.free(width);
      calloc.free(height);
      calloc.free(dpiX);
      calloc.free(dpiY);
    }
  }
}

/// Port of `has_print_handler`: the registered command for `.ext` and [action]
/// is non-empty.
bool _hasPrintHandler(String extension, String action) {
  final association = '.$extension'.toNativeUtf16();
  final verb = action.toNativeUtf16();
  final length = calloc<Uint32>();
  final command = calloc<Uint16>(_maxWideChars);
  try {
    length.value = _maxWideChars;
    final hresult = _w.assocQueryStringW(
      _assocfInitIgnoreUnknown,
      _assocstrCommand,
      association,
      verb,
      command,
      length,
    );
    return hresult >= 0 &&
        length.value > 1 &&
        _wideBufferToString(command.asTypedList(_maxWideChars)) != null;
  } finally {
    calloc.free(association);
    calloc.free(verb);
    calloc.free(length);
    calloc.free(command);
  }
}

/// Port of `print_pdf` (docflow route, PDF file).
void _printPdf(
  PrintJob job,
  bool Function() cancelled,
  WindowsPdfRenderBackend pdf,
) {
  final devmode = _validatedDevmode(job.printer, job.settings());
  try {
    final printerName = job.printer.toNativeUtf16();
    final driver = 'WINSPOOL'.toNativeUtf16();
    final hdc = _w.createDCW(driver, printerName, nullptr, devmode.pointer);
    calloc.free(driver);
    calloc.free(printerName);
    if (hdc == nullptr) {
      throw PrintFailure.failed(
        'Windows could not create the printer device context',
      );
    }
    final context = _DeviceContext(hdc);
    try {
      final jobId = _startDoc(
        context,
        'DocFlow - ${_displayName(job.path)}',
      );
      if (jobId <= 0) {
        throw PrintFailure.failed('Windows rejected the PDF print job');
      }
      final deadline = DateTime.now().add(
        Duration(seconds: job.timeoutMinutes * 60),
      );
      PrintFailure? error;
      try {
        _renderPdfPages(
          context,
          devmode,
          job.path,
          job.settings(),
          deadline,
          cancelled,
          pdf,
        );
      } on PrintFailure catch (failure) {
        error = failure;
      }
      if (error != null) {
        _w.abortDoc(context.hdc);
        throw windowsPostSubmissionFailure(error);
      }
      if (_w.endDoc(context.hdc) <= 0) {
        throw PrintFailure.uncertain(
          'Windows could not confirm completion of the PDF spool submission',
        );
      }
      _waitForJob(job.printer, jobId, deadline, cancelled);
    } finally {
      context.close();
    }
  } finally {
    devmode.free();
  }
}

/// Port of `print_image` (docflow route, raster image).
void _printImage(
  PrintJob job,
  bool Function() cancelled,
) {
  final runtime = _GdiPlusRuntime.start();
  try {
    final image = _GdiPlusImage.load(job.path);
    try {
      final (frameDimension, frameCount) = image.frameDimension();
      validateWindowsImageFrameCount(frameCount);
      if (cancelled()) {
        throw PrintFailure.cancelled(
          'cancelled before Windows image print submission',
          uncertain: false,
        );
      }
      final devmode = _validatedDevmode(job.printer, job.settings());
      try {
        final printerName = job.printer.toNativeUtf16();
        final driver = 'WINSPOOL'.toNativeUtf16();
        final hdc = _w.createDCW(driver, printerName, nullptr, devmode.pointer);
        calloc.free(driver);
        calloc.free(printerName);
        if (hdc == nullptr) {
          throw PrintFailure.failed(
            'Windows could not create the image printer device context',
          );
        }
        final context = _DeviceContext(hdc);
        try {
          final jobId = _startDoc(
            context,
            'DocFlow - ${_displayName(job.path)}',
          );
          if (jobId <= 0) {
            throw PrintFailure.failed('Windows rejected the image print job');
          }
          final deadline = DateTime.now().add(
            Duration(seconds: job.timeoutMinutes * 60),
          );
          PrintFailure? error;
          try {
            _drawImageFrames(
              context,
              devmode,
              image,
              frameDimension,
              frameCount,
              job.settings(),
              deadline,
              cancelled,
            );
          } on PrintFailure catch (failure) {
            error = failure;
          }
          if (error != null) {
            _w.abortDoc(context.hdc);
            throw windowsPostSubmissionFailure(error);
          }
          if (_w.endDoc(context.hdc) <= 0) {
            throw PrintFailure.uncertain(
              'Windows could not confirm completion of the image spool submission',
            );
          }
          _waitForJob(job.printer, jobId, deadline, cancelled);
        } finally {
          context.close();
        }
      } finally {
        devmode.free();
      }
    } finally {
      image.dispose();
    }
  } finally {
    runtime.shutdown();
  }
}

/// Port of `print_with_native_application` (printto and print verbs).
void _printWithNativeApplication(PrintJob job, bool Function() cancelled) {
  final deadline = DateTime.now().add(Duration(seconds: job.timeoutMinutes * 60));
  if (job.verb == 'print' &&
      _currentInventory().defaultPrinter != job.printer) {
    throw PrintFailure.failed('the default printer changed after preview');
  }
  final knownJobs = _jobIds(job.printer);
  final verb = job.verb.toNativeUtf16();
  final file = job.path.toNativeUtf16();
  final parameters = job.verb == 'printto'
      ? '"${job.printer}"'.toNativeUtf16()
      : nullptr.cast<Utf16>();
  final int result;
  try {
    // ShellExecuteW returns an HINSTANCE; values of 32 or less are errors.
    result = _w.shellExecuteW(nullptr, verb, file, parameters, nullptr, _swHide);
  } finally {
    calloc.free(verb);
    calloc.free(file);
    if (parameters != nullptr) {
      calloc.free(parameters);
    }
  }
  if (result <= 32) {
    throw PrintFailure.failed(
      'the native Windows application rejected the print request',
    );
  }
  final soon = DateTime.now().add(_handlerJobTimeout);
  final acquisitionDeadline = soon.isBefore(deadline) ? soon : deadline;
  final jobId = _acquireNativeJob(
    job.printer,
    knownJobs,
    _displayName(job.path).toLowerCase(),
    acquisitionDeadline,
    cancelled,
  );
  _waitForJob(job.printer, jobId, deadline, cancelled);
}

/// Identifies the one new spool job created by a native application. Ambiguous
/// observations keep polling until the acquisition deadline.
int _acquireNativeJob(
  String printer,
  Set<int> knownJobs,
  String title,
  DateTime acquisitionDeadline,
  bool Function() cancelled,
) {
  DateTime? enumerationErrorSince;
  while (true) {
    if (cancelled()) {
      throw PrintFailure.cancelled(
        'the native application was started but no spool job was identified',
        uncertain: true,
      );
    }
    final List<_SpoolJob> observed;
    try {
      observed = _jobs(printer);
      enumerationErrorSince = null;
    } on PrintFailure {
      final firstError = enumerationErrorSince ??= DateTime.now();
      if (DateTime.now().difference(firstError) >= _observationErrorGrace) {
        throw windowsPostSubmissionObservationError(
          WindowsPostSubmissionObservation.jobEnumeration,
        );
      }
      sleep(_pollInterval);
      continue;
    }
    final candidates = [
      for (final candidate in observed) (candidate.id, candidate.document),
    ];
    final selected = selectWindowsSubmittedJob(knownJobs, candidates, title);
    if (selected != null) {
      return selected;
    }
    if (!DateTime.now().isBefore(acquisitionDeadline)) {
      throw PrintFailure.submittedUnconfirmed(
        'the native application did not expose an identifiable spool job',
      );
    }
    sleep(_pollInterval);
  }
}

// ---- Document info and display helpers --------------------------------------

/// StartDocW with a DOCINFOW whose title is freed before returning.
int _startDoc(_DeviceContext context, String title) {
  final docInfo = _newDocInfo(title);
  try {
    return _w.startDocW(context.hdc, docInfo.info);
  } finally {
    docInfo.free();
  }
}

final class _DocInfo {
  _DocInfo(this.info, this._title);

  final Pointer<_DocInfoW> info;
  final Pointer<Utf16> _title;

  void free() {
    calloc.free(_title);
    calloc.free(info);
  }
}

_DocInfo _newDocInfo(String title) {
  final wide = title.toNativeUtf16();
  final info = calloc<_DocInfoW>();
  info.ref
    ..cbSize = sizeOf<_DocInfoW>()
    ..lpszDocName = wide
    ..lpszOutput = nullptr
    ..lpszDatatype = nullptr
    ..fwType = 0;
  return _DocInfo(info, wide);
}

String _displayName(String path) {
  final name = _windowsPath.basename(path);
  return name.isEmpty ? 'document' : name;
}

final p.Context _windowsPath = p.Context(style: p.Style.windows);

/// Port of `safe_extension`: 1 to 16 ASCII alphanumeric characters.
bool safeExtension(String extension) =>
    extension.isNotEmpty &&
    extension.length <= 16 &&
    extension.codeUnits.every(
      (unit) =>
          (unit >= 0x30 && unit <= 0x39) ||
          (unit >= 0x41 && unit <= 0x5A) ||
          (unit >= 0x61 && unit <= 0x7A),
    );

String _extension(String path) {
  final extension = _windowsPath.extension(path);
  final text = extension.isEmpty ? '' : extension.substring(1);
  return String.fromCharCodes(
    text.codeUnits.map((unit) => unit >= 0x41 && unit <= 0x5A ? unit + 32 : unit),
  );
}

/// Port of `submit_controlled_pdf`: validates the page-range choice, loads the
/// PDF from the exact validated bytes, and drives the ordered submit sequence.
void _submitControlledPdf(
  PrintJob job,
  Uint8List pdfBytes,
  CancellationToken cancellation,
  DateTime deadline,
  WindowsPdfRenderBackend pdf,
) {
  try {
    _submitControlledPdfInner(job, pdfBytes, cancellation, deadline, pdf);
  } on PrintFailure catch (failure) {
    throw mapControlledPrintFailure(failure);
  }
}

void _submitControlledPdfInner(
  PrintJob job,
  Uint8List pdfBytes,
  CancellationToken cancellation,
  DateTime deadline,
  WindowsPdfRenderBackend pdf,
) {
  if (job.pageRange != 'all') {
    throw PrintFailure.unsupportedOptions('error.print.worksheetPageRangeAllOnly');
  }
  final WindowsInMemoryPdf document;
  try {
    document = pdf.load(pdfBytes);
  } on PrintFailure {
    throw PrintFailure.failed(
      'error.print.excelExportMalformedPdf',
    ).withDiagnostic('excel_pdf stage=native_load');
  }
  final int pageCount;
  try {
    pageCount = document.pageCount();
  } on PrintFailure {
    throw PrintFailure.failed(
      'error.print.excelExportMalformedPdf',
    ).withDiagnostic('excel_pdf stage=native_page_count');
  }
  final pages = _selectPages('all', pageCount);
  final settings = job.settings();
  final devmode = _validatedDevmode(job.printer, settings);
  try {
    final printerName = job.printer.toNativeUtf16();
    final driver = 'WINSPOOL'.toNativeUtf16();
    final hdc = _w.createDCW(driver, printerName, nullptr, devmode.pointer);
    calloc.free(driver);
    calloc.free(printerName);
    if (hdc == nullptr) {
      throw PrintFailure.failed('error.print.excelPrinterContextUnavailable');
    }
    final context = _DeviceContext(hdc);
    final docInfo = _newDocInfo('DocFlow - Excel worksheet');
    try {
      final effects = _ControlledEffects(
        context: context,
        devmode: devmode,
        docInfo: docInfo.info,
        document: document,
        pages: pages,
        settings: settings,
        printer: job.printer,
        cancellation: cancellation,
        deadline: deadline,
      );
      executeControlledSubmit<int>(
        _ControlledSignals(cancellation, deadline),
        effects,
      );
    } finally {
      docInfo.free();
      context.close();
    }
  } finally {
    devmode.free();
  }
}

final class _ControlledSignals implements SubmitSignals {
  const _ControlledSignals(this.cancellation, this.deadline);

  final CancellationToken cancellation;
  final DateTime deadline;

  @override
  SubmitSnapshot sample() => SubmitSnapshot(
    cancelled: cancellation.isCancelled,
    deadlineExpired: !DateTime.now().isBefore(deadline),
  );
}

final class _ControlledEffects implements ControlledSubmitEffects<int> {
  _ControlledEffects({
    required this.context,
    required this.devmode,
    required this.docInfo,
    required this.document,
    required this.pages,
    required this.settings,
    required this.printer,
    required this.cancellation,
    required this.deadline,
  });

  final _DeviceContext context;
  final _DevMode devmode;
  final Pointer<_DocInfoW> docInfo;
  final WindowsInMemoryPdf document;
  final List<int> pages;
  final PrintSettings settings;
  final String printer;
  final CancellationToken cancellation;
  final DateTime deadline;

  @override
  int startDoc() {
    final jobId = _w.startDocW(context.hdc, docInfo);
    if (jobId <= 0) {
      throw PrintFailure.failed('error.print.excelPrintJobRejected');
    }
    return jobId;
  }

  @override
  bool abortDoc() => _w.abortDoc(context.hdc) > 0;

  @override
  void renderPages() => _renderInMemoryPdfPages(
    context,
    devmode,
    document,
    pages,
    settings,
    deadline,
    () => cancellation.isCancelled,
  );

  @override
  void endDoc() {
    if (_w.endDoc(context.hdc) <= 0) {
      throw PrintFailure.failed('error.print.excelPrintCompletionUnconfirmed');
    }
  }

  @override
  void observeJob(int jobId) => _waitForJob(
    printer,
    jobId,
    deadline,
    () => cancellation.isCancelled,
  );
}

// ---- Public adapters ---------------------------------------------------------

/// Port of `WindowsPrintAdapter::inventory` (shared with the native route).
PrinterInventory _currentInventory() => PrinterInventory(
  printers: _enumeratePrinters(),
  defaultPrinter: _defaultPrinter(),
).normalized();

/// Port of `WindowsPrintAdapter`.
final class WindowsPrintAdapter implements PrintAdapter {
  WindowsPrintAdapter({
    WindowsPdfRenderBackend pdfBackend = const DefaultWindowsPdfRenderBackend(),
  }) : _pdf = pdfBackend;

  final WindowsPdfRenderBackend _pdf;

  @override
  PrinterInventory inventory() => _currentInventory();

  @override
  PrintJobSupport inspect(String path, String printer, PrintSettings settings) {
    final inventory = this.inventory();
    if (!inventory.contains(printer)) {
      throw PrintFailure.failed('printer is not available');
    }
    final extension = _extension(path);
    if (extension == 'pdf') {
      _validatedDevmode(printer, settings).free();
      final pageCount = _pdf.filePageCount(path);
      return PrintJobSupport.controlledPdf(pageCount: pageCount);
    }
    if (_controlledImageFormats.contains(extension)) {
      final runtime = _GdiPlusRuntime.start();
      try {
        final image = _GdiPlusImage.load(path);
        try {
          final (_, frameCount) = image.frameDimension();
          validateWindowsImageFrameCount(frameCount);
        } finally {
          image.dispose();
        }
      } finally {
        runtime.shutdown();
      }
      _validatedDevmode(printer, settings).free();
      return PrintJobSupport.singlePage();
    }
    if (!safeExtension(extension)) {
      throw PrintFailure.unsupported(
        'Windows has no approved native print route for this file type',
      );
    }
    if (_hasPrintHandler(extension, 'printto')) {
      return PrintJobSupport.nativeApplication();
    }
    if (inventory.defaultPrinter == printer &&
        _hasPrintHandler(extension, 'print')) {
      return const PrintJobSupport(
        verb: 'print',
        settingsLimited: true,
        pageRange: AllOnlyPageRangeCapability(),
      );
    }
    throw PrintFailure.unsupported(
      'the native application required to print this file type is unavailable',
    );
  }

  @override
  void submit(PrintJob job, bool Function() cancelled) {
    if (cancelled()) {
      throw PrintFailure.cancelled(
        'cancelled before Windows print submission',
        uncertain: false,
      );
    }
    final support = inspect(job.path, job.printer, job.settings());
    if (support.verb != job.verb) {
      throw PrintFailure.failed('Windows print capability changed after preview');
    }
    if (job.verb == 'docflow') {
      if (_extension(job.path) == 'pdf') {
        _printPdf(job, cancelled, _pdf);
      } else {
        _printImage(job, cancelled);
      }
    } else {
      _printWithNativeApplication(job, cancelled);
    }
  }
}

/// Port of `WindowsControlledPdfSubmitter`. [submitPdf] mirrors
/// `ControlledPdfSubmitter::submit_pdf`, and the class implements that
/// interface on the shared `print_queue.dart` job type. [deadline] is the
/// remaining time budget from submission, converted to an absolute instant as
/// the Rust `Instant` deadline is.
final class WindowsControlledPdfSubmitter implements ControlledPdfSubmitter {
  const WindowsControlledPdfSubmitter({
    this.pdfBackend = const DefaultWindowsPdfRenderBackend(),
  });

  final WindowsPdfRenderBackend pdfBackend;

  @override
  Future<void> submitPdf(
    PrintJob job,
    ExportedWorksheetPdf pdf,
    CancellationToken cancellation,
    Duration deadline,
  ) async {
    _submitControlledPdf(
      job,
      pdf.validatedBytes(),
      cancellation,
      DateTime.now().add(deadline),
      pdfBackend,
    );
  }
}
