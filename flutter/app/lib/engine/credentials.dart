// Port of src-tauri/src/credentials.rs.
//
// Layering:
//   CredentialStore         session-level contract used by auth (Rust trait).
//   SessionCredentialStore  serializes a StoredSession through a SecureStore.
//   SecureStore             key/value secret seam shaped after flutter_secure_storage
//                           (read / write / delete). Platform adapters implement it.
//
// Tokens live only in this layer and in the auth service. StoredSession has no
// general JSON view, and its toString redacts both tokens.

import 'dart:convert';
import 'dart:ffi';
import 'dart:io';
import 'dart:typed_data';

import 'package:ffi/ffi.dart' show calloc;

/// Service and account identities preserved from the Rust build so that secrets
/// written by the Rust app are found by the Dart app after the migration.
const String secureStoreService = 'com.docflow.desktop.supabase';
const String sessionCredentialAccount = 'session-v1';

enum CredentialError {
  unavailable('secure credential storage is unavailable'),
  invalidData('secure credential data is invalid');

  const CredentialError(this.message);

  final String message;
}

class CredentialException implements Exception {
  const CredentialException(this.error);

  final CredentialError error;

  @override
  String toString() => 'CredentialException(${error.name})';
}

int _saturatingSub(int value, int amount) {
  const minimum = -9223372036854775808;
  if (value < minimum + amount) {
    return minimum;
  }
  return value - amount;
}

/// Session secrets. The tokens are private and have no JSON view except
/// [toCredentialJson], which only the credential store calls.
final class StoredSession {
  StoredSession(
    this._accessToken,
    this._refreshToken,
    this.userId,
    this.email,
    this.expiresAt,
  );

  /// Decodes the Rust-compatible payload. Every field is required, as with serde.
  factory StoredSession.fromCredentialJson(Object? raw) {
    if (raw is! Map<String, Object?>) {
      throw const CredentialException(CredentialError.invalidData);
    }
    final access = raw['accessToken'];
    final refresh = raw['refreshToken'];
    final userId = raw['userId'];
    final email = raw['email'];
    final expiresAt = raw['expiresAt'];
    if (access is! String ||
        refresh is! String ||
        userId is! String ||
        email is! String ||
        expiresAt is! int) {
      throw const CredentialException(CredentialError.invalidData);
    }
    return StoredSession(access, refresh, userId, email, expiresAt);
  }

  final String _accessToken;
  final String _refreshToken;
  final String userId;
  final String email;
  final int expiresAt;

  String get accessToken => _accessToken;

  String get refreshToken => _refreshToken;

  /// Sessions count as expired 60 seconds early, as in Rust.
  bool expiredAt(int now) => now >= _saturatingSub(expiresAt, 60);

  bool matchesTokens(String accessToken, String refreshToken) =>
      _accessToken == accessToken && _refreshToken == refreshToken;

  Map<String, Object?> toCredentialJson() => <String, Object?>{
    'accessToken': _accessToken,
    'refreshToken': _refreshToken,
    'userId': userId,
    'email': email,
    'expiresAt': expiresAt,
  };

  @override
  bool operator ==(Object other) =>
      other is StoredSession &&
      other._accessToken == _accessToken &&
      other._refreshToken == _refreshToken &&
      other.userId == userId &&
      other.email == email &&
      other.expiresAt == expiresAt;

  @override
  int get hashCode =>
      Object.hash(_accessToken, _refreshToken, userId, email, expiresAt);

  @override
  String toString() =>
      'StoredSession(userId: $userId, email: $email, expiresAt: $expiresAt)';
}

/// Session-level credential contract (Rust `CredentialStore` trait).
abstract interface class CredentialStore {
  Future<StoredSession?> read();

  Future<void> write(StoredSession session);

  Future<void> delete();
}

/// Secret key/value seam shaped after flutter_secure_storage, so the plugin can
/// replace a platform adapter without touching auth. Implementations throw
/// [CredentialException] on failure.
abstract interface class SecureStore {
  Future<String?> read(String key);

  Future<void> write(String key, String value);

  Future<void> delete(String key);
}

/// Serializes sessions as JSON under [sessionCredentialAccount]. A stored value
/// that does not decode is reported as `invalidData`.
final class SessionCredentialStore implements CredentialStore {
  const SessionCredentialStore(this._store);

  final SecureStore _store;

  @override
  Future<StoredSession?> read() async {
    final encoded = await _store.read(sessionCredentialAccount);
    if (encoded == null) {
      return null;
    }
    final Object? decoded;
    try {
      decoded = jsonDecode(encoded);
    } on FormatException {
      throw const CredentialException(CredentialError.invalidData);
    }
    return StoredSession.fromCredentialJson(decoded);
  }

  @override
  Future<void> write(StoredSession session) => _store.write(
    sessionCredentialAccount,
    jsonEncode(session.toCredentialJson()),
  );

  @override
  Future<void> delete() => _store.delete(sessionCredentialAccount);
}

/// In-memory store. Secrets exist only for the life of the process.
final class MemorySecureStore implements SecureStore {
  final Map<String, String> _values = <String, String>{};

  @override
  Future<String?> read(String key) async => _values[key];

  @override
  Future<void> write(String key, String value) async {
    _values[key] = value;
  }

  @override
  Future<void> delete(String key) async {
    _values.remove(key);
  }
}

final RegExp _safeFileKey = RegExp(r'^[A-Za-z0-9._-]{1,128}$');

/// Plaintext file store for development and tests only.
///
/// This is NOT a secure store: secrets are written unencrypted, and file
/// permissions are the operating-system default. [platformSecureStore] never
/// selects it, so release builds cannot fall back to it.
final class FileSecureStore implements SecureStore {
  const FileSecureStore(this.directory);

  final Directory directory;

  File _fileFor(String key) {
    if (!_safeFileKey.hasMatch(key)) {
      throw const CredentialException(CredentialError.unavailable);
    }
    return File('${directory.path}${Platform.pathSeparator}$key');
  }

  @override
  Future<String?> read(String key) async {
    final file = _fileFor(key);
    try {
      if (!file.existsSync()) {
        return null;
      }
      return file.readAsStringSync();
    } on FileSystemException {
      throw const CredentialException(CredentialError.unavailable);
    }
  }

  @override
  Future<void> write(String key, String value) async {
    final file = _fileFor(key);
    final temporary = File('${file.path}.tmp');
    try {
      directory.createSync(recursive: true);
      temporary.writeAsStringSync(value, flush: true);
      temporary.renameSync(file.path);
    } on FileSystemException {
      throw const CredentialException(CredentialError.unavailable);
    }
  }

  @override
  Future<void> delete(String key) async {
    final file = _fileFor(key);
    try {
      if (file.existsSync()) {
        file.deleteSync();
      }
    } on FileSystemException {
      throw const CredentialException(CredentialError.unavailable);
    }
  }
}

/// Used where no native adapter exists. Mirrors the Rust `OsCredentialStore`
/// on unsupported platforms, where every operation is unavailable.
final class UnsupportedSecureStore implements SecureStore {
  const UnsupportedSecureStore();

  @override
  Future<String?> read(String key) async =>
      throw const CredentialException(CredentialError.unavailable);

  @override
  Future<void> write(String key, String value) async =>
      throw const CredentialException(CredentialError.unavailable);

  @override
  Future<void> delete(String key) async =>
      throw const CredentialException(CredentialError.unavailable);
}

/// Backend identifier, matching `OsCredentialStore::BACKEND`.
String get credentialBackend {
  if (Platform.isMacOS) {
    return 'macos-keychain';
  }
  if (Platform.isWindows) {
    return 'windows-credential-manager';
  }
  return 'unsupported';
}

/// Native adapter for this platform. Linux has none, as in Rust.
SecureStore platformSecureStore() {
  if (Platform.isMacOS) {
    return const MacOsKeychainSecureStore(secureStoreService);
  }
  if (Platform.isWindows) {
    return const WindowsCredentialSecureStore(secureStoreService);
  }
  return const UnsupportedSecureStore();
}

// ---------------------------------------------------------------------------
// macOS Keychain adapter (generic password items through Security.framework).
//
// UNVERIFIED: written against the Security and CoreFoundation C APIs. It has not
// been compiled or run on macOS in this environment and needs a macOS run plus
// the packaged-app smoke test before it is trusted. Reads disable keychain UI
// and skip authenticated items, as the Rust adapter does, so startup never
// shows an authentication prompt.
// ---------------------------------------------------------------------------

const int _errSecItemNotFound = -25300;
const int _cfStringEncodingUtf8 = 0x08000100;

typedef _SecItemCall = Int32 Function(Pointer<Void>, Pointer<Pointer<Void>>);
typedef _SecItemCallDart = int Function(Pointer<Void>, Pointer<Pointer<Void>>);
typedef _SecItemUpdateNative = Int32 Function(Pointer<Void>, Pointer<Void>);
typedef _SecItemUpdateDart = int Function(Pointer<Void>, Pointer<Void>);
typedef _SecItemDeleteNative = Int32 Function(Pointer<Void>);
typedef _SecItemDeleteDart = int Function(Pointer<Void>);
typedef _SecInteractionGetNative = Int32 Function(Pointer<Uint8>);
typedef _SecInteractionGetDart = int Function(Pointer<Uint8>);
typedef _SecInteractionSetNative = Int32 Function(Uint8);
typedef _SecInteractionSetDart = int Function(int);
typedef _CFDictionaryCreateNative = Pointer<Void> Function(
  Pointer<Void>,
  IntPtr,
  Pointer<Void>,
  Pointer<Void>,
);
typedef _CFDictionaryCreateDart = Pointer<Void> Function(
  Pointer<Void>,
  int,
  Pointer<Void>,
  Pointer<Void>,
);
typedef _CFDictionarySetNative = Void Function(
  Pointer<Void>,
  Pointer<Void>,
  Pointer<Void>,
);
typedef _CFDictionarySetDart = void Function(
  Pointer<Void>,
  Pointer<Void>,
  Pointer<Void>,
);
typedef _CFDataCreateNative = Pointer<Void> Function(
  Pointer<Void>,
  Pointer<Uint8>,
  IntPtr,
);
typedef _CFDataCreateDart = Pointer<Void> Function(
  Pointer<Void>,
  Pointer<Uint8>,
  int,
);
typedef _CFDataLengthNative = IntPtr Function(Pointer<Void>);
typedef _CFDataLengthDart = int Function(Pointer<Void>);
typedef _CFDataBytesNative = Pointer<Uint8> Function(Pointer<Void>);
typedef _CFDataBytesDart = Pointer<Uint8> Function(Pointer<Void>);
typedef _CFStringCreateNative = Pointer<Void> Function(
  Pointer<Void>,
  Pointer<Uint8>,
  Uint32,
);
typedef _CFStringCreateDart = Pointer<Void> Function(
  Pointer<Void>,
  Pointer<Uint8>,
  int,
);
typedef _CFReleaseNative = Void Function(Pointer<Void>);
typedef _CFReleaseDart = void Function(Pointer<Void>);
typedef _CFTypeIdNative = UintPtr Function(Pointer<Void>);
typedef _CFTypeIdDart = int Function(Pointer<Void>);
typedef _CFNoArgTypeIdNative = UintPtr Function();
typedef _CFNoArgTypeIdDart = int Function();

/// Lazily bound Security and CoreFoundation symbols. Constants are read from
/// the data symbols that the frameworks export.
final class _MacSecurity {
  _MacSecurity._()
    : _security = DynamicLibrary.open(
        '/System/Library/Frameworks/Security.framework/Security',
      ),
      _coreFoundation = DynamicLibrary.open(
        '/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation',
      ) {
    _copyMatching = _security.lookupFunction<_SecItemCall, _SecItemCallDart>(
      'SecItemCopyMatching',
    );
    _add = _security.lookupFunction<_SecItemCall, _SecItemCallDart>(
      'SecItemAdd',
    );
    _update = _security
        .lookupFunction<_SecItemUpdateNative, _SecItemUpdateDart>(
          'SecItemUpdate',
        );
    _delete = _security
        .lookupFunction<_SecItemDeleteNative, _SecItemDeleteDart>(
          'SecItemDelete',
        );
    _getInteraction = _security
        .lookupFunction<_SecInteractionGetNative, _SecInteractionGetDart>(
          'SecKeychainGetUserInteractionAllowed',
        );
    _setInteraction = _security
        .lookupFunction<_SecInteractionSetNative, _SecInteractionSetDart>(
          'SecKeychainSetUserInteractionAllowed',
        );
    _dictionaryCreate = _coreFoundation
        .lookupFunction<_CFDictionaryCreateNative, _CFDictionaryCreateDart>(
          'CFDictionaryCreateMutable',
        );
    _dictionarySet = _coreFoundation
        .lookupFunction<_CFDictionarySetNative, _CFDictionarySetDart>(
          'CFDictionarySetValue',
        );
    _dataCreate = _coreFoundation
        .lookupFunction<_CFDataCreateNative, _CFDataCreateDart>('CFDataCreate');
    _dataLength = _coreFoundation
        .lookupFunction<_CFDataLengthNative, _CFDataLengthDart>(
          'CFDataGetLength',
        );
    _dataBytes = _coreFoundation
        .lookupFunction<_CFDataBytesNative, _CFDataBytesDart>(
          'CFDataGetBytePtr',
        );
    _stringCreate = _coreFoundation
        .lookupFunction<_CFStringCreateNative, _CFStringCreateDart>(
          'CFStringCreateWithCString',
        );
    _release = _coreFoundation.lookupFunction<_CFReleaseNative, _CFReleaseDart>(
      'CFRelease',
    );
    _typeId = _coreFoundation.lookupFunction<_CFTypeIdNative, _CFTypeIdDart>(
      'CFGetTypeID',
    );
    _dataTypeId = _coreFoundation
        .lookupFunction<_CFNoArgTypeIdNative, _CFNoArgTypeIdDart>(
          'CFDataGetTypeID',
        );
    _keyCallbacks = _coreFoundation.lookup<Void>(
      'kCFTypeDictionaryKeyCallBacks',
    );
    _valueCallbacks = _coreFoundation.lookup<Void>(
      'kCFTypeDictionaryValueCallBacks',
    );
    _classKey = _constant(_security, 'kSecClass');
    _genericPasswordClass = _constant(_security, 'kSecClassGenericPassword');
    _serviceKey = _constant(_security, 'kSecAttrService');
    _accountKey = _constant(_security, 'kSecAttrAccount');
    _valueKey = _constant(_security, 'kSecValueData');
    _returnDataKey = _constant(_security, 'kSecReturnData');
    _matchLimitKey = _constant(_security, 'kSecMatchLimit');
    _matchLimitOne = _constant(_security, 'kSecMatchLimitOne');
    _authUiKey = _constant(_security, 'kSecUseAuthenticationUI');
    _authUiSkip = _constant(_security, 'kSecUseAuthenticationUISkip');
    _cfTrue = _constant(_coreFoundation, 'kCFBooleanTrue');
  }

  static _MacSecurity? _instance;

  static _MacSecurity get instance => _instance ??= _MacSecurity._();

  final DynamicLibrary _security;
  final DynamicLibrary _coreFoundation;
  late final _SecItemCallDart _copyMatching;
  late final _SecItemCallDart _add;
  late final _SecItemUpdateDart _update;
  late final _SecItemDeleteDart _delete;
  late final _SecInteractionGetDart _getInteraction;
  late final _SecInteractionSetDart _setInteraction;
  late final _CFDictionaryCreateDart _dictionaryCreate;
  late final _CFDictionarySetDart _dictionarySet;
  late final _CFDataCreateDart _dataCreate;
  late final _CFDataLengthDart _dataLength;
  late final _CFDataBytesDart _dataBytes;
  late final _CFStringCreateDart _stringCreate;
  late final _CFReleaseDart _release;
  late final _CFTypeIdDart _typeId;
  late final _CFNoArgTypeIdDart _dataTypeId;
  late final Pointer<Void> _keyCallbacks;
  late final Pointer<Void> _valueCallbacks;
  late final Pointer<Void> _classKey;
  late final Pointer<Void> _genericPasswordClass;
  late final Pointer<Void> _serviceKey;
  late final Pointer<Void> _accountKey;
  late final Pointer<Void> _valueKey;
  late final Pointer<Void> _returnDataKey;
  late final Pointer<Void> _matchLimitKey;
  late final Pointer<Void> _matchLimitOne;
  late final Pointer<Void> _authUiKey;
  late final Pointer<Void> _authUiSkip;
  late final Pointer<Void> _cfTrue;

  static Pointer<Void> _constant(DynamicLibrary library, String name) =>
      library.lookup<Pointer<Void>>(name).value;

  int copyMatching(Pointer<Void> query, Pointer<Pointer<Void>> result) =>
      _copyMatching(query, result);

  int add(Pointer<Void> attributes) => _add(attributes, nullptr);

  int update(Pointer<Void> query, Pointer<Void> attributes) =>
      _update(query, attributes);

  int deleteItem(Pointer<Void> query) => _delete(query);

  int getInteraction(Pointer<Uint8> state) => _getInteraction(state);

  int setInteraction(int state) => _setInteraction(state);

  /// Creates a CFDictionary. The dictionary retains its keys and values, so the
  /// caller may release them as soon as this returns.
  Pointer<Void> dictionary(List<(Pointer<Void>, Pointer<Void>)> entries) {
    final created = _dictionaryCreate(
      nullptr,
      entries.length,
      _keyCallbacks,
      _valueCallbacks,
    );
    for (final (key, value) in entries) {
      _dictionarySet(created, key, value);
    }
    return created;
  }

  /// Builds a generic-password query for [service] and [account], plus [extra]
  /// entries. Owned by the caller, who must release it.
  Pointer<Void> genericQuery(
    String service,
    String account, {
    List<(Pointer<Void>, Pointer<Void>)> extra = const [],
  }) {
    final serviceRef = cfString(service);
    final accountRef = cfString(account);
    try {
      return dictionary([
        (_classKey, _genericPasswordClass),
        (_serviceKey, serviceRef),
        (_accountKey, accountRef),
        ...extra,
      ]);
    } finally {
      release(serviceRef);
      release(accountRef);
    }
  }

  Pointer<Void> get valueKey => _valueKey;

  Pointer<Void> get returnDataKey => _returnDataKey;

  Pointer<Void> get matchLimitKey => _matchLimitKey;

  Pointer<Void> get matchLimitOne => _matchLimitOne;

  Pointer<Void> get authUiKey => _authUiKey;

  Pointer<Void> get authUiSkip => _authUiSkip;

  Pointer<Void> get cfTrue => _cfTrue;

  Pointer<Void> cfData(Uint8List bytes) {
    final buffer = calloc<Uint8>(bytes.isEmpty ? 1 : bytes.length);
    try {
      buffer.asTypedList(bytes.length).setAll(0, bytes);
      return _dataCreate(nullptr, buffer, bytes.length);
    } finally {
      calloc.free(buffer);
    }
  }

  Pointer<Void> cfString(String value) {
    final encoded = utf8.encode(value);
    final buffer = calloc<Uint8>(encoded.length + 1);
    try {
      buffer.asTypedList(encoded.length + 1).setAll(0, encoded);
      return _stringCreate(nullptr, buffer, _cfStringEncodingUtf8);
    } finally {
      calloc.free(buffer);
    }
  }

  void release(Pointer<Void> reference) {
    if (reference != nullptr) {
      _release(reference);
    }
  }

  bool isCfData(Pointer<Void> reference) => _typeId(reference) == _dataTypeId();

  Uint8List dataBytes(Pointer<Void> data) {
    final length = _dataLength(data);
    final pointer = _dataBytes(data);
    if (length == 0 || pointer == nullptr) {
      return Uint8List(0);
    }
    return Uint8List.fromList(pointer.asTypedList(length));
  }
}

final class MacOsKeychainSecureStore implements SecureStore {
  const MacOsKeychainSecureStore(this.service);

  final String service;

  @override
  Future<String?> read(String key) async => _macRead(service, key);

  @override
  Future<void> write(String key, String value) async {
    _macWrite(service, key, Uint8List.fromList(utf8.encode(value)));
  }

  @override
  Future<void> delete(String key) async => _macDelete(service, key);
}

String? _macRead(String service, String account) {
  final sec = _MacSecurity.instance;
  final previous = calloc<Uint8>();
  try {
    if (sec.getInteraction(previous) != 0) {
      throw const CredentialException(CredentialError.unavailable);
    }
    final wasAllowed = previous.value;
    if (sec.setInteraction(0) != 0) {
      throw const CredentialException(CredentialError.unavailable);
    }
    try {
      return _macReadWithoutInteraction(sec, service, account);
    } finally {
      sec.setInteraction(wasAllowed);
    }
  } finally {
    calloc.free(previous);
  }
}

String? _macReadWithoutInteraction(
  _MacSecurity sec,
  String service,
  String account,
) {
  final query = sec.genericQuery(
    service,
    account,
    extra: [
      (sec.returnDataKey, sec.cfTrue),
      (sec.matchLimitKey, sec.matchLimitOne),
      (sec.authUiKey, sec.authUiSkip),
    ],
  );
  final result = calloc<Pointer<Void>>();
  try {
    final status = sec.copyMatching(query, result);
    if (status == _errSecItemNotFound) {
      return null;
    }
    if (status != 0) {
      throw const CredentialException(CredentialError.unavailable);
    }
    final item = result.value;
    if (item == nullptr) {
      return null;
    }
    try {
      if (!sec.isCfData(item)) {
        throw const CredentialException(CredentialError.invalidData);
      }
      return utf8.decode(sec.dataBytes(item), allowMalformed: false);
    } on FormatException {
      throw const CredentialException(CredentialError.invalidData);
    } finally {
      sec.release(item);
    }
  } finally {
    calloc.free(result);
    sec.release(query);
  }
}

void _macWrite(String service, String account, Uint8List data) {
  final sec = _MacSecurity.instance;
  final value = sec.cfData(data);
  try {
    final query = sec.genericQuery(service, account);
    try {
      final attributes = sec.dictionary([(sec.valueKey, value)]);
      try {
        var status = sec.update(query, attributes);
        if (status == _errSecItemNotFound) {
          final addition = sec.genericQuery(
            service,
            account,
            extra: [(sec.valueKey, value)],
          );
          try {
            status = sec.add(addition);
          } finally {
            sec.release(addition);
          }
        }
        if (status != 0) {
          throw const CredentialException(CredentialError.unavailable);
        }
      } finally {
        sec.release(attributes);
      }
    } finally {
      sec.release(query);
    }
  } finally {
    sec.release(value);
  }
}

void _macDelete(String service, String account) {
  final sec = _MacSecurity.instance;
  final query = sec.genericQuery(service, account);
  try {
    final status = sec.deleteItem(query);
    if (status != 0 && status != _errSecItemNotFound) {
      throw const CredentialException(CredentialError.unavailable);
    }
  } finally {
    sec.release(query);
  }
}

// ---------------------------------------------------------------------------
// Windows Credential Manager adapter (advapi32 generic credentials).
//
// UNVERIFIED: written against the Win32 CredReadW, CredWriteW, CredDeleteW and
// CredFree signatures and the CREDENTIALW layout. dart:ffi derives the struct
// offsets from the platform ABI. It has not been executed on Windows here and
// needs a Windows run plus the packaged-app smoke test before it is trusted.
//
// Credential identity is the target name (the service). Windows keeps one
// generic credential per target, so the key is stored as UserName and only one
// key per service is supported, as in the Rust build.
// ---------------------------------------------------------------------------

const int _credTypeGeneric = 1;
const int _credPersistLocalMachine = 2;
const int _errorNotFound = 1168;

final class _CredentialW extends Struct {
  @Uint32()
  external int flags;

  @Uint32()
  external int type;

  external Pointer<Uint16> targetName;

  external Pointer<Uint16> comment;

  @Uint32()
  external int lastWrittenLow;

  @Uint32()
  external int lastWrittenHigh;

  @Uint32()
  external int credentialBlobSize;

  external Pointer<Uint8> credentialBlob;

  @Uint32()
  external int persist;

  @Uint32()
  external int attributeCount;

  external Pointer<Void> attributes;

  external Pointer<Uint16> targetAlias;

  external Pointer<Uint16> userName;
}

typedef _CredReadNative = Int32 Function(
  Pointer<Uint16>,
  Uint32,
  Uint32,
  Pointer<Pointer<_CredentialW>>,
);
typedef _CredReadDart = int Function(
  Pointer<Uint16>,
  int,
  int,
  Pointer<Pointer<_CredentialW>>,
);
typedef _CredWriteNative = Int32 Function(Pointer<_CredentialW>, Uint32);
typedef _CredWriteDart = int Function(Pointer<_CredentialW>, int);
typedef _CredDeleteNative = Int32 Function(Pointer<Uint16>, Uint32, Uint32);
typedef _CredDeleteDart = int Function(Pointer<Uint16>, int, int);
typedef _CredFreeNative = Void Function(Pointer<Void>);
typedef _CredFreeDart = void Function(Pointer<Void>);
typedef _GetLastErrorNative = Uint32 Function();
typedef _GetLastErrorDart = int Function();

final class _WinCredentials {
  _WinCredentials._()
    : _advapi = DynamicLibrary.open('advapi32.dll'),
      _kernel = DynamicLibrary.open('kernel32.dll') {
    _read = _advapi.lookupFunction<_CredReadNative, _CredReadDart>('CredReadW');
    _write = _advapi.lookupFunction<_CredWriteNative, _CredWriteDart>(
      'CredWriteW',
    );
    _delete = _advapi.lookupFunction<_CredDeleteNative, _CredDeleteDart>(
      'CredDeleteW',
    );
    _free = _advapi.lookupFunction<_CredFreeNative, _CredFreeDart>('CredFree');
    _lastError = _kernel.lookupFunction<_GetLastErrorNative, _GetLastErrorDart>(
      'GetLastError',
    );
  }

  static final _WinCredentials instance = _WinCredentials._();

  final DynamicLibrary _advapi;
  final DynamicLibrary _kernel;
  late final _CredReadDart _read;
  late final _CredWriteDart _write;
  late final _CredDeleteDart _delete;
  late final _CredFreeDart _free;
  late final _GetLastErrorDart _lastError;

  int read(Pointer<Uint16> target, Pointer<Pointer<_CredentialW>> out) =>
      _read(target, _credTypeGeneric, 0, out);

  int write(Pointer<_CredentialW> credential) => _write(credential, 0);

  int delete(Pointer<Uint16> target) => _delete(target, _credTypeGeneric, 0);

  void free(Pointer<Void> buffer) => _free(buffer);

  /// Must be called immediately after a failing Win32 call on the same thread.
  int lastError() => _lastError();
}

/// Allocates a NUL-terminated UTF-16 buffer. The caller frees it with calloc.
Pointer<Uint16> _allocWide(String value) {
  final units = value.codeUnits;
  final buffer = calloc<Uint16>(units.length + 1);
  for (var index = 0; index < units.length; index++) {
    buffer[index] = units[index];
  }
  return buffer;
}

final class WindowsCredentialSecureStore implements SecureStore {
  const WindowsCredentialSecureStore(this.service);

  final String service;

  @override
  Future<String?> read(String key) async => _winRead(service);

  @override
  Future<void> write(String key, String value) async {
    _winWrite(service, key, Uint8List.fromList(utf8.encode(value)));
  }

  @override
  Future<void> delete(String key) async => _winDelete(service);
}

String? _winRead(String target) {
  final native = _WinCredentials.instance;
  final targetName = _allocWide(target);
  final out = calloc<Pointer<_CredentialW>>();
  try {
    if (native.read(targetName, out) == 0) {
      if (native.lastError() == _errorNotFound) {
        return null;
      }
      throw const CredentialException(CredentialError.unavailable);
    }
    final credential = out.value;
    if (credential == nullptr) {
      throw const CredentialException(CredentialError.invalidData);
    }
    try {
      final size = credential.ref.credentialBlobSize;
      final blob = credential.ref.credentialBlob;
      final bytes = size == 0 || blob == nullptr
          ? Uint8List(0)
          : Uint8List.fromList(blob.asTypedList(size));
      return utf8.decode(bytes, allowMalformed: false);
    } on FormatException {
      throw const CredentialException(CredentialError.invalidData);
    } finally {
      native.free(credential.cast<Void>());
    }
  } finally {
    calloc.free(targetName);
    calloc.free(out);
  }
}

void _winWrite(String target, String userName, Uint8List data) {
  if (data.length > 0xFFFFFFFF) {
    throw const CredentialException(CredentialError.invalidData);
  }
  final native = _WinCredentials.instance;
  final targetName = _allocWide(target);
  final account = _allocWide(userName);
  final blob = calloc<Uint8>(data.isEmpty ? 1 : data.length);
  final credential = calloc<_CredentialW>();
  try {
    blob.asTypedList(data.length).setAll(0, data);
    credential.ref
      ..flags = 0
      ..type = _credTypeGeneric
      ..targetName = targetName
      ..credentialBlobSize = data.length
      ..credentialBlob = blob
      ..persist = _credPersistLocalMachine
      ..attributeCount = 0
      ..userName = account;
    if (native.write(credential) == 0) {
      throw const CredentialException(CredentialError.unavailable);
    }
  } finally {
    calloc.free(credential);
    calloc.free(blob);
    calloc.free(account);
    calloc.free(targetName);
  }
}

void _winDelete(String target) {
  final native = _WinCredentials.instance;
  final targetName = _allocWide(target);
  try {
    if (native.delete(targetName) != 0) {
      return;
    }
    if (native.lastError() != _errorNotFound) {
      throw const CredentialException(CredentialError.unavailable);
    }
  } finally {
    calloc.free(targetName);
  }
}
