/// Port of src-tauri/src/platform/startup.rs: the per-user "start with
/// Windows" Run-key registration.
///
/// UNVERIFIED: the Windows branch calls advapi32 registry functions through
/// dart:ffi. It has been compiled and reviewed on Linux only; it has not been
/// run against a real Windows registry. The non-Windows branch is fully
/// exercised by tests.
library;

import 'dart:ffi';
import 'dart:io' show Platform;

import 'package:ffi/ffi.dart';

const String _runSubkey = r'Software\Microsoft\Windows\CurrentVersion\Run';
const String _valueName = 'DocFlow';

/// Port of the `io::Error` values returned by `set_start_with_windows`.
class StartupException implements Exception {
  const StartupException(this.kind, this.message, {this.osCode});

  /// Mirrors `io::ErrorKind`: `unsupported`, `invalidInput`, or `os`.
  final String kind;
  final String message;
  final int? osCode;

  @override
  String toString() => 'StartupException($kind, $message, osCode: $osCode)';
}

const int _hkeyCurrentUser = 0x80000001; // sign-extended in the native call
const int _keySetValue = 0x0002;
const int _regOptionNonVolatile = 0;
const int _regSz = 1;
const int _errorFileNotFound = 2;

typedef _RegCreateKeyExWNative =
    Int32 Function(
      Pointer<Void> hKey,
      Pointer<Utf16> subKey,
      Uint32 reserved,
      Pointer<Utf16> classStr,
      Uint32 options,
      Uint32 samDesired,
      Pointer<Void> security,
      Pointer<Pointer<Void>> result,
      Pointer<Uint32> disposition,
    );
typedef _RegCreateKeyExWDart =
    int Function(
      Pointer<Void> hKey,
      Pointer<Utf16> subKey,
      int reserved,
      Pointer<Utf16> classStr,
      int options,
      int samDesired,
      Pointer<Void> security,
      Pointer<Pointer<Void>> result,
      Pointer<Uint32> disposition,
    );

typedef _RegSetValueExWNative =
    Int32 Function(
      Pointer<Void> hKey,
      Pointer<Utf16> valueName,
      Uint32 reserved,
      Uint32 type,
      Pointer<Uint8> data,
      Uint32 cbData,
    );
typedef _RegSetValueExWDart =
    int Function(
      Pointer<Void> hKey,
      Pointer<Utf16> valueName,
      int reserved,
      int type,
      Pointer<Uint8> data,
      int cbData,
    );

typedef _RegDeleteValueWNative =
    Int32 Function(Pointer<Void> hKey, Pointer<Utf16> valueName);
typedef _RegDeleteValueWDart =
    int Function(Pointer<Void> hKey, Pointer<Utf16> valueName);

typedef _RegCloseKeyNative = Int32 Function(Pointer<Void> hKey);
typedef _RegCloseKeyDart = int Function(Pointer<Void> hKey);

/// Port of `set_start_with_windows`. Enabling writes
/// `"<current executable>"` under HKCU Run/DocFlow. Disabling removes the
/// value and treats an already-missing value as success.
void setStartWithWindows({required bool enabled}) {
  if (!Platform.isWindows) {
    if (enabled) {
      throw const StartupException(
        'unsupported',
        'Windows startup is available only on Windows',
      );
    }
    return;
  }
  _setStartWithWindowsNative(enabled);
}

void _setStartWithWindowsNative(bool enabled) {
  final advapi = DynamicLibrary.open('advapi32.dll');
  final regCreateKeyExW = advapi
      .lookupFunction<_RegCreateKeyExWNative, _RegCreateKeyExWDart>(
        'RegCreateKeyExW',
      );
  final regSetValueExW = advapi
      .lookupFunction<_RegSetValueExWNative, _RegSetValueExWDart>(
        'RegSetValueExW',
      );
  final regDeleteValueW = advapi
      .lookupFunction<_RegDeleteValueWNative, _RegDeleteValueWDart>(
        'RegDeleteValueW',
      );
  final regCloseKey = advapi.lookupFunction<_RegCloseKeyNative, _RegCloseKeyDart>(
    'RegCloseKey',
  );

  final hkeyCurrentUser = Pointer<Void>.fromAddress(
    _hkeyCurrentUser | 0xFFFFFFFF00000000,
  );
  final subkey = _runSubkey.toNativeUtf16();
  final valueName = _valueName.toNativeUtf16();
  final keyOut = calloc<Pointer<Void>>();
  final disposition = calloc<Uint32>();
  final noClass = ''.toNativeUtf16();
  try {
    final status = regCreateKeyExW(
      hkeyCurrentUser,
      subkey,
      0,
      noClass,
      _regOptionNonVolatile,
      _keySetValue,
      nullptr,
      keyOut,
      disposition,
    );
    if (status != 0) {
      throw StartupException(
        'os',
        'registry key could not be opened',
        osCode: status,
      );
    }
    final key = keyOut.value;
    try {
      if (enabled) {
        final executable = Platform.resolvedExecutable;
        if (executable.contains('"')) {
          throw const StartupException(
            'invalidInput',
            'application path contains a quote',
          );
        }
        final command = '"$executable"'.toNativeUtf16();
        try {
          final units = _utf16Length(command);
          final bytes = units * 2;
          final status = regSetValueExW(
            key,
            valueName,
            0,
            _regSz,
            command.cast<Uint8>(),
            bytes,
          );
          if (status != 0) {
            throw StartupException(
              'os',
              'startup value could not be written',
              osCode: status,
            );
          }
        } finally {
          calloc.free(command);
        }
      } else {
        final status = regDeleteValueW(key, valueName);
        if (status != 0 && status != _errorFileNotFound) {
          throw StartupException(
            'os',
            'startup value could not be removed',
            osCode: status,
          );
        }
      }
    } finally {
      regCloseKey(key);
    }
  } finally {
    calloc.free(subkey);
    calloc.free(valueName);
    calloc.free(noClass);
    calloc.free(keyOut);
    calloc.free(disposition);
  }
}

/// Length in UTF-16 code units including the terminating NUL, matching
/// Rust's `wide()` helper which appends one zero unit.
int _utf16Length(Pointer<Utf16> value) {
  var length = 0;
  while (value.cast<Uint16>()[length] != 0) {
    length++;
  }
  return length + 1;
}
