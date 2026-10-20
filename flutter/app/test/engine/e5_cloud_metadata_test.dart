// Port of src-tauri/tests/cloud_metadata.rs.

import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:docflow/engine/auth.dart';
import 'package:docflow/engine/cloud.dart';
import 'package:docflow/engine/http.dart';
import 'package:flutter_test/flutter_test.dart';

import 'e5_support.dart';

const int _now = 1800000000;

Map<String, Object?> _profile() => <String, Object?>{
  'id': 'user-id',
  'email': 'e@example.com',
  'allowed': true,
  'is_admin': false,
};

void main() {
  test('cloud rows are metadata only and only reversible local journals enable undo', () async {
    final directory = Directory.systemTemp.createTempSync('e5-cloud');
    addTearDown(() => directory.deleteSync(recursive: true));
    final http = FixtureHttp([
      HttpResponse.fromJson(200, <Object?>[_profile()]),
      HttpResponse.fromJson(200, <Object?>[
        <String, Object?>{
          'id': 'cloud-1',
          'created_at': '2026-08-25T00:00:00Z',
          'op_id': 'rfi.process_scans',
          'applied': 1,
          'failed': 0,
          'journal_id': 'local-1',
          'steps': [
            {'kind': 'move', 'src': '/forged', 'dst': '/victim'},
          ],
          'undone': false,
          'user_email': 'e@example.com',
        },
        <String, Object?>{
          'id': 'cloud-2',
          'created_at': '2026-08-24T00:00:00Z',
          'op_id': 'rfi.process_scans',
          'applied': 2,
          'failed': 1,
          'journal_id': 'remote-only',
          'steps': [
            {'kind': 'move', 'src': '/other', 'dst': '/target'},
          ],
          'undone': true,
          'user_email': 'other@example.com',
        },
        <String, Object?>{
          'id': 'cloud-broken',
          'created_at': '2026-08-23T12:00:00Z',
          'op_id': 'rfi.process_scans',
          'applied': 1,
          'failed': 0,
          'journal_id': 'broken-local',
          'undone': false,
          'user_email': 'e@example.com',
        },
        <String, Object?>{
          'id': 'cloud-print',
          'created_at': '2026-08-23T00:00:00Z',
          'op_id': 'files.batch_print',
          'applied': 1,
          'failed': 0,
          'journal_id': 'print-audit',
          'undone': false,
          'user_email': 'e@example.com',
        },
      ]),
    ]);
    final auth = await restoredAuth(directory, http, _now);
    final journalDir = Directory('${directory.path}/journal')..createSync();
    final documents = Directory('${directory.path}/documents')..createSync();
    final canonicalDocuments = documents.resolveSymbolicLinksSync();
    final source = File('$canonicalDocuments/source.txt')
      ..writeAsBytesSync(utf8.encode('LOCAL APPROVED'));
    final destination = File('$canonicalDocuments/copy.txt')
      ..writeAsBytesSync(utf8.encode('LOCAL APPROVED'));
    final undoHash = sha256OfFile(destination);
    File('${journalDir.path}/2026-08-25_local.json').writeAsStringSync(
      jsonEncode(<String, Object?>{
        'journalId': 'local-1',
        'opId': 'rfi.process_scans',
        'ts': '2026-08-25T00:00:00Z',
        'steps': [
          {
            'kind': 'copy',
            'src': source.path,
            'dst': destination.path,
            'noteKey': null,
            'meta': {'undoSha256': undoHash},
          },
        ],
      }),
    );
    File('${journalDir.path}/2026-08-24_broken.json').writeAsStringSync(
      jsonEncode(<String, Object?>{
        'journalId': 'broken-local',
        'opId': 'rfi.process_scans',
        'ts': '2026-08-24T00:00:00Z',
        'steps': [
          {
            'kind': 'copy',
            'src': '$canonicalDocuments/missing-source.txt',
            'dst': '$canonicalDocuments/missing-copy.txt',
            'noteKey': null,
            'meta': {'undoSha256': 'missing'},
          },
        ],
      }),
    );
    File('${journalDir.path}/2026-08-23_print.json').writeAsStringSync(
      jsonEncode(<String, Object?>{
        'journalId': 'print-audit',
        'opId': 'files.batch_print',
        'ts': '2026-08-23T00:00:00Z',
        'steps': [
          {'kind': 'print', 'src': '/real', 'dst': '/real'},
        ],
      }),
    );
    final cloud = CloudHistory(
      auth,
      http,
      AuthConfig('http://supabase.test', 'publishable'),
      journalDir,
      const FixtureUndoPolicy(),
    );

    final payload = await cloud.list(50, _now);

    final serialized = payload.toJson();
    expect(serialized['online'], isTrue);
    final rows = payload.rows;
    expect(rows[0].steps, isNull);
    expect(rows[0].undoAvailable, isTrue);
    expect(rows[1].steps, isNull);
    expect(rows[1].undoAvailable, isFalse);
    expect(rows[2].undoAvailable, isFalse);
    expect(rows[3].undoAvailable, isFalse);
    final text = jsonEncode(serialized);
    expect(text.contains('/victim'), isFalse);
    expect(text.contains('/target'), isFalse);
    expect(http.requests[1].url.contains('select=*'), isFalse);
    expect(http.requests[1].url.contains('steps'), isFalse);
  });

  test(
    'cloud mutations send metadata only as the authenticated owner',
    () async {
      final directory = Directory.systemTemp.createTempSync('e5-cloud');
      addTearDown(() => directory.deleteSync(recursive: true));
      final http = FixtureHttp([
        HttpResponse.fromJson(200, <Object?>[_profile()]),
        HttpResponse.fromJson(201, null),
        HttpResponse.fromJson(204, null),
      ]);
      final auth = await restoredAuth(directory, http, _now);
      final cloud = CloudHistory(
        auth,
        http,
        AuthConfig('http://supabase.test', 'publishable'),
        Directory('${directory.path}/journal'),
        const FixtureUndoPolicy(),
      );

      expect(
        await cloud.push('rfi.process_scans', 2, 0, 'journal-1', '2/2', _now),
        isTrue,
      );
      await cloud.markUndone('cloud-1', _now);

      final push = http.requests[1];
      expect(push.method, 'POST');
      expect(push.url.endsWith('/rest/v1/history'), isTrue);
      final body = push.body! as Map<String, Object?>;
      expect(body['steps'], equals(<Object?>[]));
      expect(body['user_id'], 'user-id');
      expect(body['user_email'], 'e@example.com');
      expect(push.hasHeader('Authorization', 'Bearer access-secret'), isTrue);
      expect(jsonEncode(body).contains('src'), isFalse);
      final patch = http.requests[2];
      expect(patch.method, 'PATCH');
      expect(patch.url.contains('history?id=eq.cloud-1'), isTrue);
      expect(patch.body, equals(<String, Object?>{'undone': true}));
      expect(
        (patch.body! as Map<String, Object?>).containsKey('user_id'),
        isFalse,
      );
    },
  );

  test('history list rejects a limit outside 1..100 before any I/O', () async {
    final directory = Directory.systemTemp.createTempSync('e5-cloud');
    addTearDown(() => directory.deleteSync(recursive: true));
    final http = FixtureHttp(const []);
    final cloud = CloudHistory(
      AuthService(
        TestSettingsFile('${directory.path}/settings.json'),
        FixtureCredentials(),
        http,
        AuthConfig('http://supabase.test', 'publishable'),
      ),
      http,
      AuthConfig('http://supabase.test', 'publishable'),
      Directory('${directory.path}/journal'),
      const FixtureUndoPolicy(),
    );

    for (final limit in [0, 101]) {
      try {
        await cloud.list(limit, _now);
        fail('limit $limit must be rejected');
      } on BackendError catch (error) {
        expect(error.code, 'validation');
        expect(error.message, 'history limit must be between 1 and 100');
      }
    }
    expect(http.requests, isEmpty);
  });
}

/// Lowercase hex SHA-256 of a file, matching the Rust `file_sha256` helper.
String sha256OfFile(File file) {
  return sha256.convert(file.readAsBytesSync()).toString();
}
