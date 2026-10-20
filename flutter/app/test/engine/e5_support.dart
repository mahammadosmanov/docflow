// Shared fixtures for the E5 engine tests (auth, cloud, credentials, gemini).
// Port of the fixture types in src-tauri/tests/auth_http.rs, cloud_metadata.rs
// and gemini_http.rs.

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:docflow/engine/auth.dart';
import 'package:docflow/engine/cloud.dart';
import 'package:docflow/engine/credentials.dart';
import 'package:docflow/engine/http.dart';

/// Scripted transport. Responses are consumed in order. Each request is recorded.
final class FixtureHttp implements JsonHttp {
  FixtureHttp(List<HttpResponse> responses) : _responses = List.of(responses);

  final List<HttpResponse> _responses;
  final List<FixtureRequest> requests = <FixtureRequest>[];

  @override
  Future<HttpResponse> sendJson(
    String method,
    String url,
    List<(String, String)> headers,
    Object? body,
  ) async {
    requests.add(
      FixtureRequest(
        method: method,
        url: url,
        headers: List.of(headers),
        body: body,
      ),
    );
    if (_responses.isEmpty) {
      throw const HttpFailure(HttpError.invalidResponse);
    }
    return _responses.removeAt(0);
  }
}

final class FixtureRequest {
  const FixtureRequest({
    required this.method,
    required this.url,
    required this.headers,
    required this.body,
  });

  final String method;
  final String url;
  final List<(String, String)> headers;
  final Object? body;

  bool hasHeader(String name, String value) =>
      headers.any((header) => header.$1 == name && header.$2 == value);
}

/// Holds one session in memory, as the Rust `FixtureCredentials` does.
final class FixtureCredentials implements CredentialStore {
  FixtureCredentials([this.session]);

  StoredSession? session;

  @override
  Future<StoredSession?> read() async => session;

  @override
  Future<void> write(StoredSession value) async {
    session = value;
  }

  @override
  Future<void> delete() async {
    session = null;
  }
}

/// Writes succeed but reads never find the session. Models a store that cannot
/// verify its own write.
final class UnverifiableCredentials implements CredentialStore {
  @override
  Future<StoredSession?> read() async => null;

  @override
  Future<void> write(StoredSession session) async {}

  @override
  Future<void> delete() async {}
}

/// Fails on delete, as the Rust `DeleteFailingCredentials` does.
final class DeleteFailingCredentials implements CredentialStore {
  StoredSession? session;

  @override
  Future<StoredSession?> read() async => session;

  @override
  Future<void> write(StoredSession value) async {
    session = value;
  }

  @override
  Future<void> delete() async =>
      throw const CredentialException(CredentialError.unavailable);
}

/// Blocks the logout request until [release] is called, then fails it.
final class BlockingLogoutHttp implements JsonHttp {
  BlockingLogoutHttp(List<HttpResponse> responses, this._entered)
    : _responses = List.of(responses);

  final List<HttpResponse> _responses;
  final Completer<void> _entered;
  final Completer<void> _released = Completer<void>();

  void release() => _released.complete();

  @override
  Future<HttpResponse> sendJson(
    String method,
    String url,
    List<(String, String)> headers,
    Object? body,
  ) async {
    if (url.endsWith('/auth/v1/logout')) {
      if (!_entered.isCompleted) {
        _entered.complete();
      }
      await _released.future;
      throw const HttpFailure(HttpError.offline);
    }
    if (_responses.isEmpty) {
      throw const HttpFailure(HttpError.invalidResponse);
    }
    return _responses.removeAt(0);
  }
}

/// File-backed [AuthSettings] with the same semantics as the Rust
/// `SettingsStore` for the session operations. The real E4 store replaces it.
///
/// A write goes to disk before the in-memory copy changes, so a failed write
/// leaves both unchanged, as in Rust.
final class TestSettingsFile implements AuthSettings {
  TestSettingsFile(this.path) : _data = _load(path);

  final String path;
  Map<String, Object?> _data;

  static Map<String, Object?> _load(String path) {
    final file = File(path);
    if (!file.existsSync()) {
      return <String, Object?>{};
    }
    return Map<String, Object?>.from(
      jsonDecode(file.readAsStringSync()) as Map<String, Object?>,
    );
  }

  void _persist(Map<String, Object?> next) {
    File(path).writeAsStringSync(jsonEncode(next), flush: true);
  }

  @override
  Map<String, Object?>? section(String name) {
    final value = _data[name];
    return value is Map<String, Object?>
        ? Map<String, Object?>.of(value)
        : null;
  }

  @override
  void saveSection(String name, Map<String, Object?>? value) {
    final next = Map<String, Object?>.of(_data);
    if (value != null) {
      next[name] = value;
    } else {
      next.remove(name);
    }
    _persist(next);
    _data = next;
  }

  @override
  void saveSessionMetadata(String userId, String email, int expiresAt) {
    final next = Map<String, Object?>.of(_data);
    final current = next['session'];
    final session = current is Map<String, Object?>
        ? Map<String, Object?>.of(current)
        : <String, Object?>{};
    session
      ..remove('accessToken')
      ..remove('refreshToken')
      ..['userId'] = userId
      ..['email'] = email
      ..['expiresAt'] = expiresAt;
    next['session'] = session;
    _persist(next);
    _data = next;
  }

  @override
  bool stripLegacySessionTokens(StoredSession expected) {
    final current = _data['session'];
    if (current is! Map<String, Object?>) {
      return false;
    }
    final tokensMatch =
        current['accessToken'] == expected.accessToken &&
        current['refreshToken'] == expected.refreshToken;
    if (!tokensMatch) {
      return false;
    }
    final next = Map<String, Object?>.of(_data);
    final session = Map<String, Object?>.of(current)
      ..remove('accessToken')
      ..remove('refreshToken');
    next['session'] = session;
    _persist(next);
    _data = next;
    return true;
  }
}

/// Pass-through undo policy for the cloud fixture tests.
///
/// The real rule belongs to the undo module (E3). This stand-in reproduces only
/// what the fixture checks: a `copy` step whose destination exists and matches
/// `meta.undoSha256`, and whose source still exists. Replace it with the E3
/// policy when that module lands.
final class FixtureUndoPolicy implements LocalJournalUndoPolicy {
  const FixtureUndoPolicy();

  @override
  bool isSafelyUndoable(Map<String, Object?> entry) {
    final steps = entry['steps'];
    if (steps is! List<Object?> || steps.isEmpty) {
      return false;
    }
    for (final step in steps) {
      if (step is! Map<String, Object?> || step['kind'] != 'copy') {
        return false;
      }
      final source = step['src'];
      final destination = step['dst'];
      final meta = step['meta'];
      if (source is! String ||
          destination is! String ||
          meta is! Map<String, Object?>) {
        return false;
      }
      final expected = meta['undoSha256'];
      final destinationFile = File(destination);
      final sourceFile = File(source);
      if (!destinationFile.existsSync() || !sourceFile.existsSync()) {
        return false;
      }
      if (expected is! String ||
          expected != _sha256Hex(destinationFile.readAsBytesSync())) {
        return false;
      }
    }
    return true;
  }
}

String _sha256Hex(List<int> bytes) => sha256.convert(bytes).toString();

/// Shared helper: a restored AuthService with a valid, unexpired session.
Future<AuthService> restoredAuth(
  Directory directory,
  FixtureHttp http,
  int now,
) async {
  final settings = TestSettingsFile('${directory.path}/settings.json');
  settings.saveSessionMetadata('user-id', 'e@example.com', now + 3600);
  final auth = AuthService(
    settings,
    FixtureCredentials(
      StoredSession(
        'access-secret',
        'refresh-secret',
        'user-id',
        'e@example.com',
        now + 3600,
      ),
    ),
    http,
    AuthConfig('http://supabase.test', 'publishable'),
  );
  await auth.restore(now);
  return auth;
}
