// Port of src-tauri/tests/auth_http.rs and the inline tests in src-tauri/src/http.rs.

import 'dart:async';
import 'dart:io';
import 'dart:typed_data';

import 'package:docflow/engine/auth.dart';
import 'package:docflow/engine/credentials.dart';
import 'package:docflow/engine/http.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;

import 'e5_support.dart';

const int _now = 1800000000;

Future<BackendError> _backendErrorOf(Future<Object?> operation) async {
  try {
    await operation;
  } on BackendError catch (error) {
    return error;
  }
  fail('expected a BackendError');
}

AuthService _auth({
  required String settingsPath,
  required CredentialStore credentials,
  required JsonHttp http,
}) {
  return AuthService(
    TestSettingsFile(settingsPath),
    credentials,
    http,
    AuthConfig('http://supabase.test', 'publishable'),
  );
}

HttpResponse _tokenResponse({
  String access = 'access-secret',
  String refresh = 'refresh-secret',
  int? expiresAt,
  String userId = 'user-id',
  String email = 'e@example.com',
}) {
  return HttpResponse.fromJson(200, <String, Object?>{
    'access_token': access,
    'refresh_token': refresh,
    'expires_at': expiresAt ?? _now + 3600,
    'user': <String, Object?>{'id': userId, 'email': email},
  });
}

/// Serves exactly one raw HTTP response on loopback. Returns null when the
/// sandbox forbids sockets (EACCES or EPERM), which the Rust test also treats
/// as a skip.
Future<String?> _oneResponseServer(List<int> response) async {
  final ServerSocket server;
  try {
    server = await ServerSocket.bind(InternetAddress.loopbackIPv4, 0);
  } on SocketException catch (error) {
    if (error.osError?.errorCode == 13 || error.osError?.errorCode == 1) {
      return null;
    }
    rethrow;
  }
  server.listen((socket) async {
    final buffered = BytesBuilder(copy: false);
    final completer = Completer<void>();
    late StreamSubscription<List<int>> subscription;
    subscription = socket.listen(
      (chunk) {
        buffered.add(chunk);
        final bytes = buffered.toBytes();
        final headerEnd = _indexOf(bytes, const [13, 10, 13, 10]);
        if (headerEnd < 0) {
          return;
        }
        final headers = String.fromCharCodes(bytes.sublist(0, headerEnd));
        final lengthMatch = RegExp(
          r'content-length:\s*(\d+)',
          caseSensitive: false,
        ).firstMatch(headers);
        final contentLength = int.tryParse(lengthMatch?.group(1) ?? '') ?? 0;
        if (bytes.length >= headerEnd + 4 + contentLength &&
            !completer.isCompleted) {
          completer.complete();
        }
      },
      onDone: () {
        if (!completer.isCompleted) {
          completer.complete();
        }
      },
    );
    await completer.future;
    await subscription.cancel();
    socket.add(response);
    await socket.flush();
    await socket.close();
    await server.close();
  });
  return 'http://127.0.0.1:${server.port}';
}

int _indexOf(Uint8List haystack, List<int> needle) {
  for (var index = 0; index + needle.length <= haystack.length; index++) {
    var matched = true;
    for (var offset = 0; offset < needle.length; offset++) {
      if (haystack[index + offset] != needle[offset]) {
        matched = false;
        break;
      }
    }
    if (matched) {
      return index;
    }
  }
  return -1;
}

/// Streams a body without a Content-Length, so only the streaming bound applies.
final class _UnlengthedClient extends http.BaseClient {
  _UnlengthedClient(this._body);

  final List<int> _body;

  @override
  Future<http.StreamedResponse> send(http.BaseRequest request) async {
    return http.StreamedResponse(
      Stream<List<int>>.fromIterable(<List<int>>[_body]),
      200,
    );
  }
}

void main() {
  group('http.rs inline tests', () {
    test(
      'bounded body reader rejects an oversized payload without network',
      () async {
        final client = HttpClient(
          const HttpConfig(maxResponseBytes: 64),
          client: _UnlengthedClient(List<int>.filled(65, 0x78)),
        );

        final error = await client
            .sendJson('GET', 'http://supabase.test/x', const [], null)
            .then<HttpError?>(
              (_) => null,
              onError: (Object e) => (e as HttpFailure).error,
            );

        expect(error, HttpError.responseTooLarge);
      },
    );

    test('request bound is enforced before transport', () async {
      final client = HttpClient(const HttpConfig(maxRequestBytes: 8));

      final error = await client
          .sendJson('POST', 'not a URL', const [], <String, Object?>{
            'payload': 'too long',
          })
          .then<HttpError?>(
            (_) => null,
            onError: (Object e) => (e as HttpFailure).error,
          );

      expect(error, HttpError.requestTooLarge);
    });

    test('zero bounds are rejected at construction', () {
      expect(
        () => HttpClient(const HttpConfig(maxRequestBytes: 0)),
        throwsA(isA<HttpFailure>()),
      );
    });
  });

  group('auth_http.rs', () {
    test(
      'json http client enforces response limit without exposing body',
      () async {
        final body = List<int>.filled(65, 0x78);
        final header = utf8Bytes(
          'HTTP/1.1 200 OK\r\nContent-Length: ${body.length}\r\n'
          'Content-Type: application/json\r\nConnection: close\r\n\r\n',
        );
        final base = await _oneResponseServer(<int>[...header, ...body]);
        if (base == null) {
          // Hermetic runners may forbid loopback sockets. The same bound is
          // covered by the transport tests above, as in the Rust suite.
          return;
        }
        final client = HttpClient(
          const HttpConfig(
            connectTimeout: Duration(seconds: 1),
            requestTimeout: Duration(seconds: 2),
            maxRequestBytes: 1024,
            maxResponseBytes: 64,
          ),
        );

        final error = await client
            .sendJson(
              'POST',
              '$base/auth/v1/token',
              const [],
              <String, Object?>{'ok': true},
            )
            .then<HttpFailure?>(
              (_) => null,
              onError: (Object e) => e as HttpFailure,
            );

        expect(error?.error, HttpError.responseTooLarge);
        expect(error.toString().contains('x' * 65), isFalse);
      },
    );

    test('email sign in preserves auth DTO and persists secrets only in credentials', () async {
      final http = FixtureHttp([
        _tokenResponse(expiresAt: _now + 3600),
        HttpResponse.fromJson(200, <Object?>[
          <String, Object?>{
            'id': 'user-id',
            'email': 'e@example.com',
            'username': 'operator',
            'allowed': true,
            'is_admin': false,
            'created_at': '2026-08-25T00:00:00Z',
          },
        ]),
      ]);
      final credentials = FixtureCredentials();
      final directory = Directory.systemTemp.createTempSync('e5-auth');
      addTearDown(() => directory.deleteSync(recursive: true));
      final path = '${directory.path}/settings.json';
      final auth = _auth(
        settingsPath: path,
        credentials: credentials,
        http: http,
      );

      final state = await auth.signIn('e@example.com', 'password', _now);

      expect(state.toJson(), <String, Object?>{
        'signedIn': true,
        'allowed': true,
        'isAdmin': false,
        'session': <String, Object?>{
          'userId': 'user-id',
          'email': 'e@example.com',
          'expiresAt': _now + 3600,
        },
        'user': <String, Object?>{
          'id': 'user-id',
          'email': 'e@example.com',
          'username': 'operator',
          'display': 'operator',
          'allowed': true,
          'isAdmin': false,
          'createdAt': '2026-08-25T00:00:00Z',
        },
      });
      final persisted = File(path).readAsStringSync();
      expect(persisted.contains('access-secret'), isFalse);
      expect(persisted.contains('refresh-secret'), isFalse);
      final stored = await credentials.read();
      expect(stored, isNotNull);
      expect(stored!.matchesTokens('access-secret', 'refresh-secret'), isTrue);

      expect(http.requests[0].method, 'POST');
      expect(
        http.requests[0].url.endsWith('/auth/v1/token?grant_type=password'),
        isTrue,
      );
      expect(
        http.requests[0].body,
        equals(<String, Object?>{
          'email': 'e@example.com',
          'password': 'password',
        }),
      );
      expect(http.requests[1].method, 'GET');
      expect(http.requests[1].url.contains('/rest/v1/app_users?'), isTrue);
    });

    test(
      'sign in keeps verified session when profile fetch is offline',
      () async {
        final http = FixtureHttp([_tokenResponse()]);
        final directory = Directory.systemTemp.createTempSync('e5-auth');
        addTearDown(() => directory.deleteSync(recursive: true));
        final auth = _auth(
          settingsPath: '${directory.path}/settings.json',
          credentials: FixtureCredentials(),
          http: http,
        );

        await _backendErrorOf(auth.signIn('e@example.com', 'password', _now));

        final state = auth.state();
        expect(state.signedIn, isTrue);
        expect(state.allowed, isFalse);
        expect(state.user, isNull);
      },
    );

    test('username sign in fails closed without any network request', () async {
      final http = FixtureHttp(const []);
      final directory = Directory.systemTemp.createTempSync('e5-auth');
      addTearDown(() => directory.deleteSync(recursive: true));
      final auth = _auth(
        settingsPath: '${directory.path}/settings.json',
        credentials: FixtureCredentials(),
        http: http,
      );

      final error = await _backendErrorOf(
        auth.signIn('operator', 'wrong-password', _now),
      );

      expect(error.code, 'auth_failed');
      expect(error.message, 'email_required');
      expect(http.requests, isEmpty);
    });

    test(
      'sign out clears runtime before a delayed failing logout request',
      () async {
        final entered = Completer<void>();
        final http = BlockingLogoutHttp([
          _tokenResponse(),
          HttpResponse.fromJson(200, <Object?>[
            <String, Object?>{
              'id': 'user-id',
              'email': 'e@example.com',
              'allowed': true,
              'is_admin': false,
            },
          ]),
        ], entered);
        final directory = Directory.systemTemp.createTempSync('e5-auth');
        addTearDown(() => directory.deleteSync(recursive: true));
        final auth = _auth(
          settingsPath: '${directory.path}/settings.json',
          credentials: FixtureCredentials(),
          http: http,
        );
        await auth.signIn('e@example.com', 'password', _now);

        final pending = auth.signOut();
        await entered.future.timeout(const Duration(seconds: 2));

        expect(auth.state().signedIn, isFalse);
        http.release();
        expect((await pending).signedIn, isFalse);
      },
    );

    test('sign out remains local after credential delete failure', () async {
      final http = FixtureHttp([
        _tokenResponse(),
        HttpResponse.fromJson(200, <Object?>[
          <String, Object?>{
            'id': 'user-id',
            'email': 'e@example.com',
            'allowed': true,
            'is_admin': false,
          },
        ]),
        HttpResponse.fromJson(204, null),
      ]);
      final directory = Directory.systemTemp.createTempSync('e5-auth');
      addTearDown(() => directory.deleteSync(recursive: true));
      final path = '${directory.path}/settings.json';
      final settings = TestSettingsFile(path);
      final auth = AuthService(
        settings,
        DeleteFailingCredentials(),
        http,
        AuthConfig('http://supabase.test', 'publishable'),
      );
      await auth.signIn('e@example.com', 'password', _now);

      final error = await _backendErrorOf(auth.signOut());

      expect(error.code, 'credential_store_unavailable');
      expect(auth.state().signedIn, isFalse);
      expect(settings.section('session'), isNull);
    });

    test('sign out remains local after settings cleanup failure', () async {
      final http = FixtureHttp([
        _tokenResponse(),
        HttpResponse.fromJson(200, <Object?>[
          <String, Object?>{
            'id': 'user-id',
            'email': 'e@example.com',
            'allowed': true,
            'is_admin': false,
          },
        ]),
        HttpResponse.fromJson(204, null),
      ]);
      final directory = Directory.systemTemp.createTempSync('e5-auth');
      addTearDown(() => directory.deleteSync(recursive: true));
      final settingsParent = Directory('${directory.path}/settings-parent')
        ..createSync();
      final path = '${settingsParent.path}/settings.json';
      final credentials = FixtureCredentials();
      final auth = AuthService(
        TestSettingsFile(path),
        credentials,
        http,
        AuthConfig('http://supabase.test', 'publishable'),
      );
      await auth.signIn('e@example.com', 'password', _now);
      File(path).deleteSync();
      settingsParent.deleteSync();
      File(settingsParent.path).writeAsStringSync('blocks settings directory');

      final error = await _backendErrorOf(auth.signOut());

      expect(error.code, 'settings_write_failed');
      expect(auth.state().signedIn, isFalse);
      expect(await credentials.read(), isNull);
    });

    test('supabase setup exposes no anonymous password checking oracle', () {
      final sql = File('../../docs/supabase_auth.sql').readAsStringSync();

      expect(
        sql,
        contains('drop function if exists public.email_for_login(text, text)'),
      );
      expect(
        sql,
        isNot(contains('create or replace function public.email_for_login')),
      );
      expect(sql, isNot(contains('encrypted_password')));
      expect(
        sql,
        isNot(contains('grant execute on function public.email_for_login')),
      );
    });

    test('restore imports legacy tokens once then atomically removes only plaintext tokens', () async {
      final http = FixtureHttp([
        HttpResponse.fromJson(200, <Object?>[
          <String, Object?>{
            'id': 'user-id',
            'email': 'e@example.com',
            'allowed': true,
            'is_admin': false,
          },
        ]),
      ]);
      final directory = Directory.systemTemp.createTempSync('e5-auth');
      addTearDown(() => directory.deleteSync(recursive: true));
      final path = '${directory.path}/settings.json';
      File(path).writeAsStringSync(
        '{"prefs":{"locale":"az"},"session":{"accessToken":"legacy-access",'
        '"refreshToken":"legacy-refresh","userId":"user-id","email":"e@example.com",'
        '"expiresAt":${_now + 3600},"compatibilityMarker":"keep"},"custom":[1,2,3]}',
      );
      final credentials = FixtureCredentials();
      final auth = _auth(
        settingsPath: path,
        credentials: credentials,
        http: http,
      );

      final state = await auth.restore(_now);

      expect(state.signedIn, isTrue);
      final persisted = File(path).readAsStringSync();
      expect(persisted.contains('legacy-access'), isFalse);
      expect(persisted.contains('legacy-refresh'), isFalse);
      expect(persisted.contains('"compatibilityMarker":"keep"'), isTrue);
      expect(persisted.contains('"locale":"az"'), isTrue);
      expect(persisted.contains('"custom":[1,2,3]'), isTrue);
      final stored = await credentials.read();
      expect(stored, isNotNull);
      expect(stored!.matchesTokens('legacy-access', 'legacy-refresh'), isTrue);
    });

    test('failed secure readback leaves legacy source intact and restores signed out', () async {
      final directory = Directory.systemTemp.createTempSync('e5-auth');
      addTearDown(() => directory.deleteSync(recursive: true));
      final path = '${directory.path}/settings.json';
      File(path).writeAsStringSync(
        '{"prefs":{"locale":"tr"},"session":{"accessToken":"legacy-access",'
        '"refreshToken":"legacy-refresh","userId":"user-id","email":"e@example.com",'
        '"expiresAt":${_now + 3600}}}',
      );
      final auth = _auth(
        settingsPath: path,
        credentials: UnverifiableCredentials(),
        http: FixtureHttp(const []),
      );

      final state = await auth.restore(_now);

      expect(state.signedIn, isFalse);
      final persisted = File(path).readAsStringSync();
      expect(persisted.contains('legacy-access'), isTrue);
      expect(persisted.contains('legacy-refresh'), isTrue);
    });

    test('platform credential backend is native for supported targets', () {
      if (Platform.isMacOS) {
        expect(credentialBackend, 'macos-keychain');
      }
      if (Platform.isWindows) {
        expect(credentialBackend, 'windows-credential-manager');
      }
      if (!Platform.isMacOS && !Platform.isWindows) {
        expect(credentialBackend, 'unsupported');
      }
    });

    test('macOS startup credential read never requests authentication UI', () {
      final source = File('lib/engine/credentials.dart').readAsStringSync();
      final macRead = source
          .split('String? _macRead(String service, String account) {')
          .skip(1)
          .first
          .split('void _macWrite(')
          .first;

      // Read path: keychain UI disabled for the call, authenticated items skipped.
      expect(macRead, contains('sec.setInteraction(0)'));
      expect(macRead, contains('sec.authUiSkip'));
      expect(macRead, isNot(contains('SecKeychainFindGenericPassword')));
      // The symbols behind those calls are bound from the frameworks.
      expect(source, contains("'kSecUseAuthenticationUISkip'"));
      expect(source, contains("'SecKeychainSetUserInteractionAllowed'"));
      expect(source, contains("'SecItemCopyMatching'"));
    });

    test('admin user methods preserve profile DTOs and owner authorization headers', () async {
      final http = FixtureHttp([
        _tokenResponse(
          access: 'admin-access',
          refresh: 'admin-refresh',
          userId: 'admin-id',
          email: 'admin@example.com',
        ),
        HttpResponse.fromJson(200, <Object?>[
          <String, Object?>{
            'id': 'admin-id',
            'email': 'admin@example.com',
            'username': null,
            'allowed': true,
            'is_admin': true,
            'created_at': '2026-08-24T00:00:00Z',
          },
        ]),
        HttpResponse.fromJson(200, <Object?>[
          <String, Object?>{
            'id': 'user-id',
            'email': 'user@example.com',
            'username': 'operator',
            'allowed': false,
            'is_admin': false,
            'created_at': '2026-08-25T00:00:00Z',
          },
        ]),
        HttpResponse.fromJson(204, null),
      ]);
      final directory = Directory.systemTemp.createTempSync('e5-auth');
      addTearDown(() => directory.deleteSync(recursive: true));
      final auth = _auth(
        settingsPath: '${directory.path}/settings.json',
        credentials: FixtureCredentials(),
        http: http,
      );
      await auth.signIn('admin@example.com', 'password', _now);

      final users = await auth.listUsers(_now);
      expect(
        <Object?>[for (final user in users) user.toJson()],
        equals(<Object?>[
          <String, Object?>{
            'id': 'user-id',
            'email': 'user@example.com',
            'username': 'operator',
            'display': 'operator',
            'allowed': false,
            'isAdmin': false,
            'createdAt': '2026-08-25T00:00:00Z',
          },
        ]),
      );
      await auth.setFlags('user-id', allowed: true, now: _now);

      final listRequest = http.requests[2];
      expect(listRequest.method, 'GET');
      expect(
        listRequest.url.endsWith('app_users?select=*&order=created_at.desc'),
        isTrue,
      );
      expect(
        listRequest.hasHeader('Authorization', 'Bearer admin-access'),
        isTrue,
      );
      final patchRequest = http.requests[3];
      expect(patchRequest.method, 'PATCH');
      expect(patchRequest.body, equals(<String, Object?>{'allowed': true}));
      expect(patchRequest.hasHeader('Prefer', 'return=minimal'), isTrue);
    });

    test(
      'restore refreshes an expired secure session without returning tokens',
      () async {
        final http = FixtureHttp([
          _tokenResponse(access: 'new-access', refresh: 'new-refresh'),
          HttpResponse.fromJson(200, <Object?>[
            <String, Object?>{
              'id': 'user-id',
              'email': 'e@example.com',
              'allowed': true,
              'is_admin': false,
            },
          ]),
        ]);
        final credentials = FixtureCredentials(
          StoredSession(
            'old-access',
            'old-refresh',
            'user-id',
            'e@example.com',
            _now - 1,
          ),
        );
        final directory = Directory.systemTemp.createTempSync('e5-auth');
        addTearDown(() => directory.deleteSync(recursive: true));
        final settings = TestSettingsFile('${directory.path}/settings.json')
          ..saveSessionMetadata('user-id', 'e@example.com', _now - 1);
        final auth = AuthService(
          settings,
          credentials,
          http,
          AuthConfig('http://supabase.test', 'publishable'),
        );

        final state = await auth.restore(_now);

        expect(state.signedIn, isTrue);
        expect(state.session!.expiresAt, _now + 3600);
        final stored = await credentials.read();
        expect(stored!.matchesTokens('new-access', 'new-refresh'), isTrue);
        final serialized = auth.state().toJson().toString();
        expect(serialized.contains('new-access'), isFalse);
        expect(serialized.contains('new-refresh'), isFalse);
        expect(
          http.requests[0].url.endsWith(
            '/auth/v1/token?grant_type=refresh_token',
          ),
          isTrue,
        );
        expect(
          http.requests[0].body,
          equals(<String, Object?>{'refresh_token': 'old-refresh'}),
        );
      },
    );
  });
}

Uint8List utf8Bytes(String value) => Uint8List.fromList(value.codeUnits);
