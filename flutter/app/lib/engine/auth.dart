// Port of src-tauri/src/auth.rs.
//
// Supabase email/password auth, session restore and refresh, admin user
// management, and secure session persistence. Tokens stay in this layer:
// AuthState and the other DTOs returned here carry no token fields.
//
// Placement note: [BackendError], [AppUser], [AuthState] and [AuthSessionView]
// mirror the Rust `errors.rs` and `contracts.rs`, which belong to the E1 worker.
// They live here until E1 lands, and then this file should import them instead.

import 'dart:convert';

import 'credentials.dart';
import 'http.dart';

const int _maxIdentifierChars = 320;
const int _maxPasswordChars = 1024;
const int _maxUsernameChars = 24;
const int _maxUserIdChars = 128;

/// Stable error envelope, matching `BackendError` (code + message).
final class BackendError implements Exception {
  const BackendError(this.code, this.message);

  factory BackendError.validation(String message) =>
      BackendError('validation', message);

  factory BackendError.authFailed(String message) =>
      BackendError('auth_failed', message);

  factory BackendError.authRequired() =>
      const BackendError('auth_required', 'not_authenticated');

  factory BackendError.permissionDenied() =>
      const BackendError('permission_denied', 'permission_denied');

  factory BackendError.credentialStore() => const BackendError(
    'credential_store_unavailable',
    'secure credential storage is unavailable',
  );

  factory BackendError.settingsWrite() => const BackendError(
    'settings_write_failed',
    'settings could not be saved',
  );

  factory BackendError.offline() =>
      const BackendError('offline', 'network is unavailable');

  factory BackendError.networkTimeout() =>
      const BackendError('network_timeout', 'network request timed out');

  factory BackendError.responseTooLarge() => const BackendError(
    'network_response_too_large',
    'network response exceeded the size limit',
  );

  factory BackendError.requestFailed(String message) =>
      BackendError('backend_request_failed', message);

  final String code;
  final String message;

  Map<String, Object?> toJson() => <String, Object?>{
    'code': code,
    'message': message,
  };

  @override
  String toString() => 'BackendError($code: $message)';
}

/// Supabase project endpoint and publishable key.
final class AuthConfig {
  AuthConfig(String url, this.key) : url = url.replaceAll(RegExp(r'/+$'), '');

  final String url;
  final String key;
}

/// Public profile row from `app_users`, as the UI receives it.
final class AppUser {
  const AppUser({
    required this.id,
    required this.email,
    required this.username,
    required this.display,
    required this.allowed,
    required this.isAdmin,
    required this.createdAt,
  });

  final String id;
  final String email;
  final String? username;
  final String display;
  final bool allowed;
  final bool isAdmin;
  final String createdAt;

  Map<String, Object?> toJson() => <String, Object?>{
    'id': id,
    'email': email,
    'username': username,
    'display': display,
    'allowed': allowed,
    'isAdmin': isAdmin,
    'createdAt': createdAt,
  };
}

/// Non-secret view of the session. The token fields are never exposed.
final class AuthSessionView {
  const AuthSessionView({
    required this.userId,
    required this.email,
    required this.expiresAt,
  });

  final String userId;
  final String email;
  final int expiresAt;

  Map<String, Object?> toJson() => <String, Object?>{
    'userId': userId,
    'email': email,
    'expiresAt': expiresAt,
  };
}

final class AuthState {
  const AuthState({
    required this.signedIn,
    required this.allowed,
    required this.isAdmin,
    required this.session,
    required this.user,
  });

  final bool signedIn;
  final bool allowed;
  final bool isAdmin;
  final AuthSessionView? session;
  final AppUser? user;

  Map<String, Object?> toJson() => <String, Object?>{
    'signedIn': signedIn,
    'allowed': allowed,
    'isAdmin': isAdmin,
    'session': session?.toJson(),
    'user': user?.toJson(),
  };
}

/// Non-secret settings operations used by auth. The settings store (E4) must
/// implement this. Methods throw on I/O failure, as the Rust `io::Result` does.
abstract interface class AuthSettings {
  Map<String, Object?>? section(String name);

  void saveSection(String name, Map<String, Object?>? value);

  /// Writes the public session fields and removes any legacy token fields.
  void saveSessionMetadata(String userId, String email, int expiresAt);

  /// Removes the legacy plaintext tokens only when they match [expected].
  /// Returns false when nothing matched. Other keys in the section are kept.
  bool stripLegacySessionTokens(StoredSession expected);
}

final class _Runtime {
  StoredSession? session;
  AppUser? me;
}

class AuthService {
  // Positional, mirroring Rust `AuthService::new(settings, credentials, http, config)`.
  AuthService(this._settings, this._credentials, this._http, this._config);

  final AuthSettings _settings;
  final CredentialStore _credentials;
  final JsonHttp _http;
  final AuthConfig _config;
  final _Runtime _runtime = _Runtime();

  AuthState state() => _stateFrom(_runtime);

  Future<AuthState> restore(int now) async {
    final legacyRaw = _settings.section('session');
    final legacy = legacyRaw == null
        ? null
        : _storedSessionFromLegacy(legacyRaw);
    StoredSession session;
    if (legacy != null) {
      if (!await _importLegacy(legacy)) {
        return _clearRuntime();
      }
      session = legacy;
    } else {
      final StoredSession? stored;
      try {
        stored = await _credentials.read();
      } catch (_) {
        return _clearRuntime();
      }
      if (stored == null) {
        return _clearRuntime();
      }
      session = stored;
    }

    if (session.expiredAt(now)) {
      try {
        session = await _refreshSession(session, now);
      } catch (_) {
        await _ignoreFailure(_credentials.delete());
        _ignoreSyncFailure(() => _settings.saveSection('session', null));
        return _clearRuntime();
      }
    }
    final me = await _loadUserOrNull(session);
    _runtime
      ..session = session
      ..me = me;
    return state();
  }

  Future<AuthState> signIn(String identifier, String password, int now) async {
    final login = identifier.trim();
    _validateBounded('identifier', login, _maxIdentifierChars);
    _validateBounded('password', password, _maxPasswordChars);

    if (!login.contains('@')) {
      throw BackendError.authFailed('email_required');
    }

    final response = await _request(
      'POST',
      '/auth/v1/token?grant_type=password',
      authed: false,
      body: <String, Object?>{'email': login, 'password': password},
    );
    final raw = _requireAuthJson(response);
    final session = _parseTokenResponse(raw, now);
    await _persistSession(session);
    _runtime
      ..session = session
      ..me = null;
    final me = await _loadUser(session);
    _runtime
      ..session = session
      ..me = me;
    return state();
  }

  Future<void> signUp(String email, String password, String username) async {
    _validateBounded('email', email, _maxIdentifierChars);
    _validateBounded('password', password, _maxPasswordChars);
    _validateBounded('username', username, _maxUsernameChars);
    final response = await _request(
      'POST',
      '/auth/v1/signup',
      authed: false,
      body: <String, Object?>{
        'email': email,
        'password': password,
        'data': <String, Object?>{'username': username.trim()},
      },
    );
    _requireAuthJson(response);
  }

  /// Returns false for invalid names and for taken ones. Transport or server
  /// failures return true so that a transient outage never blocks signup.
  Future<bool> usernameAvailable(String username) async {
    if (!_usernameRuleOk(username)) {
      return false;
    }
    try {
      final response = await _request(
        'POST',
        '/rest/v1/rpc/username_available',
        authed: false,
        body: <String, Object?>{'p_username': username.trim()},
      );
      return _requireAuthJson(response) == true;
    } catch (_) {
      return true;
    }
  }

  Future<AuthState> loadMe(int now) async {
    final session = await _validSession(now);
    final me = await _loadUser(session);
    _runtime
      ..session = session
      ..me = me;
    return state();
  }

  Future<AuthState> setUsername(String username, int now) async {
    _validateBounded('username', username, _maxUsernameChars);
    final session = await _validSession(now);
    final response = await _requestWithSession(
      'POST',
      '/rest/v1/rpc/set_username',
      session,
      body: <String, Object?>{'p_username': username.trim()},
    );
    _requireAuthJson(response);
    return loadMe(now);
  }

  Future<List<AppUser>> listUsers(int now) async {
    _requireAdmin();
    final session = await _validSession(now);
    final response = await _requestWithSession(
      'GET',
      '/rest/v1/app_users?select=*&order=created_at.desc',
      session,
    );
    final raw = _requireAuthJson(response);
    if (raw is! List<Object?>) {
      return const <AppUser>[];
    }
    return <AppUser>[
      for (final entry in raw)
        if (entry is Map<String, Object?>) _appUserFromJson(entry),
    ];
  }

  Future<void> setFlags(
    String userId, {
    bool? allowed,
    bool? isAdmin,
    required int now,
  }) async {
    _requireAdmin();
    _validateBounded('userId', userId, _maxUserIdChars);
    final patch = <String, Object?>{'allowed': ?allowed, 'is_admin': ?isAdmin};
    if (patch.isEmpty) {
      return;
    }
    final session = await _validSession(now);
    final safeId = percentEncodeQueryComponent(userId);
    final response = await _requestWithSession(
      'PATCH',
      '/rest/v1/app_users?id=eq.$safeId',
      session,
      extraHeaders: const [('Prefer', 'return=minimal')],
      body: patch,
    );
    _requireAuthJson(response);
  }

  /// Clears the runtime before any network call, so a slow or failing logout
  /// can never leave the app signed in. Credential and settings failures are
  /// reported after the local clear.
  Future<AuthState> signOut() async {
    final session = _runtime.session;
    _runtime
      ..session = null
      ..me = null;
    final signedOut = state();
    if (session != null) {
      await _ignoreFailure(
        _requestWithSession('POST', '/auth/v1/logout', session),
      );
    }
    Object? credentialFailure;
    Object? settingsFailure;
    try {
      await _credentials.delete();
    } catch (error) {
      credentialFailure = error;
    }
    try {
      _settings.saveSection('session', null);
    } catch (error) {
      settingsFailure = error;
    }
    if (credentialFailure != null) {
      throw BackendError.credentialStore();
    }
    if (settingsFailure != null) {
      throw BackendError.settingsWrite();
    }
    return signedOut;
  }

  /// Gate for cloud calls: the user must be allowed and the session must be valid.
  Future<StoredSession> sessionForCloud(int now) async {
    if (!state().allowed) {
      throw BackendError.permissionDenied();
    }
    return _validSession(now);
  }

  Future<AppUser?> _loadUser(StoredSession session) async {
    final userId = percentEncodeQueryComponent(session.userId);
    final response = await _requestWithSession(
      'GET',
      '/rest/v1/app_users?select=*&id=eq.$userId',
      session,
    );
    final raw = _requireAuthJson(response);
    if (raw is! List<Object?> || raw.isEmpty) {
      return null;
    }
    final first = raw.first;
    return first is Map<String, Object?> ? _appUserFromJson(first) : null;
  }

  Future<AppUser?> _loadUserOrNull(StoredSession session) async {
    try {
      return await _loadUser(session);
    } catch (_) {
      return null;
    }
  }

  Future<void> _persistSession(StoredSession session) async {
    try {
      await _credentials.write(session);
    } catch (_) {
      throw BackendError.credentialStore();
    }
    final StoredSession? stored;
    try {
      stored = await _credentials.read();
    } catch (_) {
      throw BackendError.credentialStore();
    }
    if (stored == null || stored != session) {
      await _ignoreFailure(_credentials.delete());
      throw BackendError.credentialStore();
    }
    try {
      _settings.saveSessionMetadata(
        session.userId,
        session.email,
        session.expiresAt,
      );
    } catch (_) {
      await _ignoreFailure(_credentials.delete());
      throw BackendError.settingsWrite();
    }
  }

  /// Migrates legacy plaintext tokens into the secure store. Returns false when
  /// the write cannot be verified or the legacy tokens cannot be removed. In
  /// that case the legacy source is left intact.
  Future<bool> _importLegacy(StoredSession session) async {
    try {
      await _credentials.write(session);
    } catch (_) {
      return false;
    }
    StoredSession? stored;
    try {
      stored = await _credentials.read();
    } catch (_) {
      stored = null;
    }
    if (stored == null || stored != session) {
      await _ignoreFailure(_credentials.delete());
      return false;
    }
    try {
      if (_settings.stripLegacySessionTokens(session)) {
        return true;
      }
    } catch (_) {
      // Handled below, identically to a non-match.
    }
    await _ignoreFailure(_credentials.delete());
    return false;
  }

  AuthState _clearRuntime() {
    _runtime
      ..session = null
      ..me = null;
    return state();
  }

  Future<HttpResponse> _request(
    String method,
    String path, {
    required bool authed,
    Object? body,
  }) {
    final session = authed ? _runtime.session : null;
    if (session != null) {
      return _requestWithSession(method, path, session, body: body);
    }
    return _send(method, '${_config.url}$path', <(String, String)>[
      ('apikey', _config.key),
    ], body);
  }

  Future<HttpResponse> _requestWithSession(
    String method,
    String path,
    StoredSession session, {
    List<(String, String)> extraHeaders = const [],
    Object? body,
  }) {
    return _send(method, '${_config.url}$path', <(String, String)>[
      ('apikey', _config.key),
      ('Authorization', 'Bearer ${session.accessToken}'),
      ...extraHeaders,
    ], body);
  }

  Future<HttpResponse> _send(
    String method,
    String url,
    List<(String, String)> headers,
    Object? body,
  ) async {
    try {
      return await _http.sendJson(method, url, headers, body);
    } on HttpFailure catch (failure) {
      throw _mapHttpError(failure.error);
    }
  }

  Future<StoredSession> _validSession(int now) async {
    final session = _runtime.session;
    if (session == null) {
      throw BackendError.authRequired();
    }
    if (!session.expiredAt(now)) {
      return session;
    }
    final refreshed = await _refreshSession(session, now);
    _runtime.session = refreshed;
    return refreshed;
  }

  Future<StoredSession> _refreshSession(StoredSession session, int now) async {
    final response = await _request(
      'POST',
      '/auth/v1/token?grant_type=refresh_token',
      authed: false,
      body: <String, Object?>{'refresh_token': session.refreshToken},
    );
    final raw = _requireAuthJson(response);
    final refreshed = _parseTokenResponse(raw, now);
    await _persistSession(refreshed);
    return refreshed;
  }

  void _requireAdmin() {
    if (_runtime.session == null) {
      throw BackendError.authRequired();
    }
    if (_runtime.me?.isAdmin != true) {
      throw BackendError.permissionDenied();
    }
  }
}

/// Percent-encodes a query component the way the Rust `QUERY_COMPONENT`
/// AsciiSet does. Controls, the listed ASCII delimiters and every non-ASCII
/// UTF-8 byte are encoded. Hex digits are uppercase.
String percentEncodeQueryComponent(String value) {
  final buffer = StringBuffer();
  for (final byte in utf8.encode(value)) {
    if (_queryComponentEncodes(byte)) {
      buffer
        ..write('%')
        ..write(byte.toRadixString(16).toUpperCase().padLeft(2, '0'));
    } else {
      buffer.writeCharCode(byte);
    }
  }
  return buffer.toString();
}

const Set<int> _queryComponentDelimiters = <int>{
  0x20,
  0x22,
  0x23,
  0x25,
  0x26,
  0x27,
  0x2b,
  0x2c,
  0x2f,
  0x3a,
  0x3b,
  0x3c,
  0x3d,
  0x3e,
  0x3f,
  0x40,
  0x5b,
  0x5c,
  0x5d,
  0x5e,
  0x60,
  0x7b,
  0x7c,
  0x7d,
};

bool _queryComponentEncodes(int byte) =>
    byte >= 0x80 ||
    byte < 0x20 ||
    byte == 0x7f ||
    _queryComponentDelimiters.contains(byte);

void _validateBounded(String name, String value, int maximum) {
  if (value.runes.length > maximum) {
    throw BackendError.validation('$name exceeded the size limit');
  }
}

bool _usernameRuleOk(String value) {
  final cleaned = value.trim();
  final length = utf8.encode(cleaned).length;
  if (length < 3 || length > _maxUsernameChars) {
    return false;
  }
  for (final byte in utf8.encode(cleaned)) {
    final alphanumeric =
        (byte >= 0x30 && byte <= 0x39) ||
        (byte >= 0x41 && byte <= 0x5a) ||
        (byte >= 0x61 && byte <= 0x7a);
    final separator = byte == 0x2e || byte == 0x5f || byte == 0x2d;
    if (!alphanumeric && !separator) {
      return false;
    }
  }
  return true;
}

StoredSession _parseTokenResponse(Object? raw, int now) {
  final map = raw is Map<String, Object?> ? raw : const <String, Object?>{};
  final accessToken = _nonEmptyString(map['access_token']);
  final refreshToken = _nonEmptyString(map['refresh_token']);
  if (accessToken == null || refreshToken == null) {
    throw BackendError.authFailed('invalid_token_response');
  }
  final user = map['user'];
  final userMap = user is Map<String, Object?>
      ? user
      : const <String, Object?>{};
  final userId = _stringValue(userMap['id']);
  final email = _stringValue(userMap['email']);
  final expiresIn = map['expires_in'] is int ? map['expires_in'] as int : 3600;
  final expiresAt = map['expires_at'] is int
      ? map['expires_at'] as int
      : _saturatingAdd(now, expiresIn);
  return StoredSession(accessToken, refreshToken, userId, email, expiresAt);
}

String? _nonEmptyString(Object? value) =>
    value is String && value.isNotEmpty ? value : null;

int _saturatingAdd(int left, int right) {
  const maximum = 9223372036854775807;
  const minimum = -9223372036854775808;
  if (right > 0 && left > maximum - right) {
    return maximum;
  }
  if (right < 0 && left < minimum - right) {
    return minimum;
  }
  return left + right;
}

StoredSession? _storedSessionFromLegacy(Map<String, Object?> raw) {
  final accessToken = raw['accessToken'];
  final refreshToken = raw['refreshToken'];
  if (accessToken is! String || refreshToken is! String) {
    return null;
  }
  final expiresAt = raw['expiresAt'];
  return StoredSession(
    accessToken,
    refreshToken,
    _stringValue(raw['userId']),
    _stringValue(raw['email']),
    expiresAt is int ? expiresAt : 0,
  );
}

AppUser _appUserFromJson(Map<String, Object?> raw) {
  final email = _stringValue(raw['email']);
  final username = raw['username'] is String ? raw['username'] as String : null;
  final display = username != null && username.isNotEmpty ? username : email;
  return AppUser(
    id: _stringValue(raw['id']),
    email: email,
    username: username,
    display: display,
    allowed: raw['allowed'] == true,
    isAdmin: raw['is_admin'] == true,
    createdAt: _stringValue(raw['created_at']),
  );
}

String _stringValue(Object? value) => value is String ? value : '';

AuthState _stateFrom(_Runtime runtime) {
  final session = runtime.session;
  final me = runtime.me;
  return AuthState(
    signedIn: session != null,
    allowed: me?.allowed ?? false,
    isAdmin: me?.isAdmin ?? false,
    session: session == null
        ? null
        : AuthSessionView(
            userId: session.userId,
            email: session.email,
            expiresAt: session.expiresAt,
          ),
    user: me,
  );
}

/// Turns a non-2xx Supabase response into a stable, non-secret message.
Object? _requireAuthJson(HttpResponse response) {
  if (response.status >= 200 && response.status < 300) {
    try {
      return response.json();
    } on HttpFailure catch (failure) {
      throw _mapHttpError(failure.error);
    }
  }
  Object? raw;
  try {
    raw = response.json();
  } catch (_) {
    raw = null;
  }
  String? message;
  if (raw is Map<String, Object?>) {
    for (final key in const ['error_description', 'msg', 'message', 'error']) {
      final candidate = raw[key];
      if (candidate is String) {
        message = candidate;
        break;
      }
    }
  }
  throw BackendError.authFailed(
    message != null
        ? _stableAuthMessage(message)
        : 'auth_http_${response.status}',
  );
}

String _stableAuthMessage(String message) {
  final lower = message.toLowerCase();
  if (lower.contains('invalid login') ||
      lower.contains('invalid_credentials')) {
    return 'invalid_credentials';
  }
  if (lower.contains('email not confirmed') ||
      lower.contains('email_not_confirmed')) {
    return 'email_not_confirmed';
  }
  if (lower.contains('already registered') ||
      lower.contains('user_already_exists')) {
    return 'user_already_exists';
  }
  if (lower.contains('weak_password') || lower.contains('password should be')) {
    return 'weak_password';
  }
  if (lower.contains('rate limit') || lower.contains('too many')) {
    return 'rate_limit';
  }
  if (lower.contains('username_taken')) {
    return 'username_taken';
  }
  if (lower.contains('username_invalid')) {
    return 'username_invalid';
  }
  return 'auth_request_failed';
}

BackendError _mapHttpError(HttpError error) => switch (error) {
  HttpError.timeout => BackendError.networkTimeout(),
  HttpError.responseTooLarge => BackendError.responseTooLarge(),
  HttpError.offline => BackendError.offline(),
  HttpError.invalidRequest ||
  HttpError.requestTooLarge ||
  HttpError.invalidResponse => BackendError.requestFailed(error.message),
};

Future<void> _ignoreFailure<T>(Future<T> operation) async {
  try {
    await operation;
  } catch (_) {
    // Cleanup failures are ignored, as the Rust `let _ =` calls do.
  }
}

void _ignoreSyncFailure(void Function() operation) {
  try {
    operation();
  } catch (_) {
    // Cleanup failures are ignored, as the Rust `let _ =` calls do.
  }
}
