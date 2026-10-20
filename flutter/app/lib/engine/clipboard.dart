// Port of src-tauri/src/clipboard.rs: bounded clipboard payloads, the Windows
// CF_HTML fragment envelope, and the platform clipboard writer.
//
// The size checks, escaping, CF_HTML offsets (UTF-8 byte offsets), and error
// mapping are pure Dart and are verified by test/engine/e7_clipboard_test.dart.
//
// UNVERIFIED: the platform writers below have not run on Windows or macOS from
// this environment. The pbcopy path and the user32/kernel32 FFI path follow the
// Rust implementation step for step, but must be exercised on a real desktop
// before any acceptance claim. On other platforms the writer reports
// "unsupported", exactly as the Rust `write_platform` fallback does.

import 'dart:async';
import 'dart:convert';
import 'dart:ffi';
import 'dart:io';
import 'dart:typed_data';

import 'package:ffi/ffi.dart';

import 'errors.dart';
import 'rfi_log.dart' show ClipboardPayload;

/// `MAX_CLIPBOARD_BYTES`: 8 MiB, compared in UTF-8 bytes.
const int maxClipboardBytes = 8 * 1024 * 1024;

const String _tableTooLarge = 'clipboard table exceeded the size limit';
const String _textTooLarge = 'clipboard text exceeded the size limit';

/// Platform sink for a clipboard payload (the Rust `write_platform` seam).
/// Implementations throw on any failure; [clipboardWrite] maps it to the
/// protocol error key.
abstract interface class ClipboardAdapter {
  Future<void> write(ClipboardPayload payload);
}

/// Port of `clipboard::write`.
Future<void> clipboardWrite(
  ClipboardPayload payload,
  ClipboardAdapter adapter,
) async {
  if (utf8.encode(payload.html).length > maxClipboardBytes ||
      utf8.encode(payload.plain).length > maxClipboardBytes) {
    throw BackendError.validation(_tableTooLarge);
  }
  try {
    await adapter.write(payload);
  } on Object {
    throw BackendError.requestFailed('clipboard write failed');
  }
}

/// Port of `clipboard::write_plain`: plain text wrapped as an escaped `<pre>`.
Future<void> clipboardWritePlain(String text, ClipboardAdapter adapter) async {
  if (utf8.encode(text).length > maxClipboardBytes) {
    throw BackendError.validation(_textTooLarge);
  }
  final html = text
      .replaceAll('&', '&amp;')
      .replaceAll('<', '&lt;')
      .replaceAll('>', '&gt;')
      .replaceAll('"', '&quot;')
      .replaceAll("'", '&#39;');
  await clipboardWrite(
    ClipboardPayload(html: '<pre>$html</pre>', plain: text, count: 1),
    adapter,
  );
}

const String _startMarker = '<!--StartFragment-->';
const String _endMarker = '<!--EndFragment-->';
const String _documentPrefix =
    '<html><head><meta charset="utf-8"></head><body>';
const String _documentSuffix = '</body></html>';

/// Port of `clipboard::cf_html`. Returns the CF_HTML bytes including the
/// trailing NUL. All offsets are UTF-8 byte offsets, as the format requires.
Uint8List cfHtml(String fragment) {
  if (utf8.encode(fragment).length > maxClipboardBytes) {
    throw BackendError.validation(_tableTooLarge);
  }
  final document =
      '$_documentPrefix$_startMarker$fragment$_endMarker$_documentSuffix';
  String header(int startHtml, int endHtml, int startFragment, int endFragment) =>
      'Version:0.9\r\n'
      'StartHTML:${_pad10(startHtml)}\r\n'
      'EndHTML:${_pad10(endHtml)}\r\n'
      'StartFragment:${_pad10(startFragment)}\r\n'
      'EndFragment:${_pad10(endFragment)}\r\n';

  final startHtml = utf8.encode(header(0, 0, 0, 0)).length;
  // The first occurrence wins, exactly as `str::find` does in the Rust source.
  final startIndex = document.indexOf(_startMarker);
  final endIndex = document.indexOf(_endMarker);
  if (startIndex < 0 || endIndex < 0) {
    throw BackendError.protocol('HTML fragment marker is missing');
  }
  final startFragment =
      startHtml +
      utf8.encode(document.substring(0, startIndex)).length +
      utf8.encode(_startMarker).length;
  final endFragment =
      startHtml + utf8.encode(document.substring(0, endIndex)).length;
  final documentBytes = utf8.encode(document);
  final endHtml = startHtml + documentBytes.length;

  final result = BytesBuilder(copy: false)
    ..add(utf8.encode(header(startHtml, endHtml, startFragment, endFragment)))
    ..add(documentBytes)
    ..addByte(0);
  return result.takeBytes();
}

String _pad10(int value) => value.toString().padLeft(10, '0');

/// The platform clipboard writer. macOS pipes the plain text to
/// `/usr/bin/pbcopy`; Windows publishes CF_HTML and CF_UNICODETEXT through
/// user32/kernel32; every other platform is unsupported.
///
/// UNVERIFIED: nothing in this class has executed on a Windows or macOS desktop.
final class PlatformClipboardAdapter implements ClipboardAdapter {
  const PlatformClipboardAdapter();

  @override
  Future<void> write(ClipboardPayload payload) async {
    if (Platform.isMacOS) {
      await _writeMacos(payload);
    } else if (Platform.isWindows) {
      _writeWindows(payload);
    } else {
      throw UnsupportedError('platform clipboard is unsupported');
    }
  }
}

Future<void> _writeMacos(ClipboardPayload payload) async {
  final child = await Process.start(
    '/usr/bin/pbcopy',
    const [],
    mode: ProcessStartMode.normal,
  );
  final stdoutDrain = child.stdout.drain<void>();
  final stderrDrain = child.stderr.drain<void>();
  child.stdin.add(utf8.encode(payload.plain));
  await child.stdin.close();
  final code = await child.exitCode;
  await Future.wait([stdoutDrain, stderrDrain]);
  if (code != 0) {
    throw StateError('clipboard process failed');
  }
}

// ---- Windows ---------------------------------------------------------------

const int _gmemMoveable = 0x0002;
const int _cfUnicodeText = 13;
const int _openAttempts = 10;
const Duration _openRetryDelay = Duration(milliseconds: 40);

final DynamicLibrary _user32 = DynamicLibrary.open('user32.dll');
final DynamicLibrary _kernel32 = DynamicLibrary.open('kernel32.dll');

final int Function(int) _openClipboard = _user32
    .lookupFunction<Int32 Function(IntPtr), int Function(int)>('OpenClipboard');
final int Function() _closeClipboard = _user32
    .lookupFunction<Int32 Function(), int Function()>('CloseClipboard');
final int Function() _emptyClipboard = _user32
    .lookupFunction<Int32 Function(), int Function()>('EmptyClipboard');
final int Function(Pointer<Utf16>) _registerClipboardFormat = _user32
    .lookupFunction<
      Uint32 Function(Pointer<Utf16>),
      int Function(Pointer<Utf16>)
    >('RegisterClipboardFormatW');
final int Function(int, int) _setClipboardData = _user32
    .lookupFunction<IntPtr Function(Uint32, IntPtr), int Function(int, int)>(
      'SetClipboardData',
    );
final int Function(int, int) _globalAlloc = _kernel32
    .lookupFunction<IntPtr Function(Uint32, IntPtr), int Function(int, int)>(
      'GlobalAlloc',
    );
final Pointer<Uint8> Function(int) _globalLock = _kernel32
    .lookupFunction<Pointer<Uint8> Function(IntPtr), Pointer<Uint8> Function(int)>(
      'GlobalLock',
    );
final int Function(int) _globalUnlock = _kernel32
    .lookupFunction<Int32 Function(IntPtr), int Function(int)>('GlobalUnlock');
final int Function(int) _globalFree = _kernel32
    .lookupFunction<IntPtr Function(IntPtr), int Function(int)>('GlobalFree');

void _writeWindows(ClipboardPayload payload) {
  final html = cfHtml(payload.html);
  final plain = _utf16LeWithNul(payload.plain);
  final formatName = 'HTML Format'.toNativeUtf16();
  final int htmlFormat;
  try {
    htmlFormat = _registerClipboardFormat(formatName);
  } finally {
    calloc.free(formatName);
  }
  if (htmlFormat == 0) {
    throw StateError('RegisterClipboardFormatW failed');
  }

  var opened = false;
  for (var attempt = 0; attempt < _openAttempts; attempt++) {
    if (_openClipboard(0) != 0) {
      opened = true;
      break;
    }
    sleep(_openRetryDelay);
  }
  if (!opened) {
    throw StateError('OpenClipboard failed');
  }
  try {
    if (_emptyClipboard() == 0) {
      throw StateError('EmptyClipboard failed');
    }
    _putWindowsClipboard(htmlFormat, html);
    _putWindowsClipboard(_cfUnicodeText, plain);
  } finally {
    _closeClipboard();
  }
}

/// Moves [data] into a moveable global block and hands it to the clipboard.
/// On success the system owns the block; on failure it is freed here.
void _putWindowsClipboard(int format, Uint8List data) {
  final handle = _stageGlobal(data);
  if (_setClipboardData(format, handle) == 0) {
    _globalFree(handle);
    throw StateError('SetClipboardData failed');
  }
}

int _stageGlobal(Uint8List data) {
  final handle = _globalAlloc(_gmemMoveable, data.length);
  if (handle == 0) {
    throw StateError('GlobalAlloc failed');
  }
  final pointer = _globalLock(handle);
  if (pointer.address == 0) {
    _globalFree(handle);
    throw StateError('GlobalLock failed');
  }
  pointer.asTypedList(data.length).setAll(0, data);
  _globalUnlock(handle);
  return handle;
}

Uint8List _utf16LeWithNul(String value) {
  final units = [...value.codeUnits, 0];
  final bytes = Uint8List(units.length * 2);
  for (var index = 0; index < units.length; index++) {
    bytes[index * 2] = units[index] & 0xff;
    bytes[index * 2 + 1] = (units[index] >> 8) & 0xff;
  }
  return bytes;
}
