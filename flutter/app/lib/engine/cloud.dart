// Port of src-tauri/src/cloud.rs.
//
// Cloud history is audit metadata only. Rows from Supabase never carry steps or
// paths, and only a local journal on this machine can enable undo. Cloud rows
// are matched to local journals by journal id.

import 'dart:convert';
import 'dart:io';

import 'auth.dart';
import 'credentials.dart';
import 'http.dart';

const int _maxHistoryLimit = 100;
const int _maxIdChars = 128;
const int _maxSummaryChars = 500;

/// History row exposed to the UI. `steps` is always null for cloud rows.
final class HistoryRow {
  const HistoryRow({
    required this.when,
    required this.opId,
    required this.moves,
    required this.failed,
    required this.journalId,
    required this.steps,
    required this.cloudId,
    required this.undone,
    required this.undoAvailable,
    required this.fromCloud,
    required this.userEmail,
  });

  final String when;
  final String opId;
  final int moves;
  final int failed;
  final String journalId;
  final List<Object?>? steps;
  final String? cloudId;
  final bool undone;
  final bool undoAvailable;
  final bool fromCloud;
  final String userEmail;

  Map<String, Object?> toJson() => <String, Object?>{
    'when': when,
    'opId': opId,
    'moves': moves,
    'failed': failed,
    'journalId': journalId,
    'steps': steps,
    'cloudId': cloudId,
    'undone': undone,
    'undoAvailable': undoAvailable,
    'fromCloud': fromCloud,
    'userEmail': userEmail,
  };
}

final class HistoryPayload {
  const HistoryPayload({required this.online, required this.rows});

  final bool online;
  final List<HistoryRow> rows;

  Map<String, Object?> toJson() => <String, Object?>{
    'online': online,
    'rows': <Object?>[for (final row in rows) row.toJson()],
  };
}

/// Decides whether a local journal may be undone. Implemented by the undo
/// module (E3).
///
/// [entry] is the raw decoded journal JSON object. Implementations must return
/// false when the entry does not parse as a journal record, which is the same
/// as the Rust `from_value::<JournalRecord>(...).ok()` check.
abstract interface class LocalJournalUndoPolicy {
  bool isSafelyUndoable(Map<String, Object?> entry);
}

final class CloudHistory {
  // Positional, mirroring Rust `CloudHistory::new(auth, http, config, journal_dir)`
  // plus the undo policy seam.
  CloudHistory(
    this._auth,
    this._http,
    this._config,
    this._journalDir,
    this._undoPolicy,
  );

  final AuthService _auth;
  final JsonHttp _http;
  final AuthConfig _config;
  final Directory _journalDir;
  final LocalJournalUndoPolicy _undoPolicy;

  /// Returns cloud rows when online. Offline, or on any cloud error, it returns
  /// the local journal rows with `online: false`.
  Future<HistoryPayload> list(int limit, int now) async {
    if (limit <= 0 || limit > _maxHistoryLimit) {
      throw BackendError.validation('history limit must be between 1 and 100');
    }
    final local = _readLocalHistory(_journalDir, limit, _undoPolicy);
    final localIds = <String>{
      for (final row in local)
        if (row.undoAvailable && row.journalId.isNotEmpty) row.journalId,
    };

    final StoredSession session;
    try {
      session = await _auth.sessionForCloud(now);
    } catch (_) {
      return HistoryPayload(online: false, rows: local);
    }
    const path =
        '/rest/v1/history?select=id,created_at,op_id,applied,failed,journal_id,undone,user_email&order=created_at.desc&limit=';
    final Object? raw;
    try {
      final response = await _request('GET', '$path$limit', session);
      raw = _cloudJson(response);
    } catch (_) {
      return HistoryPayload(online: false, rows: local);
    }

    final rows = <HistoryRow>[];
    if (raw is List<Object?>) {
      for (final entry in raw) {
        if (entry is! Map<String, Object?>) {
          continue;
        }
        final journalId = _stringValue(entry['journal_id']);
        rows.add(
          HistoryRow(
            when: _stringValue(entry['created_at']),
            opId: _stringValue(entry['op_id']),
            moves: _unsignedValue(entry['applied']),
            failed: _unsignedValue(entry['failed']),
            journalId: journalId,
            // Cloud-authored steps and paths are never deserialized.
            steps: null,
            cloudId: _stringValue(entry['id']),
            undone: entry['undone'] == true,
            undoAvailable: localIds.contains(journalId),
            fromCloud: true,
            userEmail: _stringValue(entry['user_email']),
          ),
        );
      }
    }
    return HistoryPayload(online: true, rows: rows);
  }

  /// Records a completed operation in the cloud. Returns true only on a 2xx.
  Future<bool> push(
    String opId,
    int applied,
    int failed,
    String journalId,
    String summary,
    int now,
  ) async {
    if (!_bounded(opId, _maxIdChars) ||
        !_bounded(journalId, _maxIdChars) ||
        !_bounded(summary, _maxSummaryChars) ||
        journalId.isEmpty) {
      return false;
    }
    final StoredSession session;
    try {
      session = await _auth.sessionForCloud(now);
    } catch (_) {
      return false;
    }
    final environment = Platform.environment;
    final device = environment['COMPUTERNAME'] ?? environment['HOSTNAME'] ?? '';
    final body = <String, Object?>{
      'device': device,
      'op_id': opId,
      'applied': applied,
      'failed': failed,
      'journal_id': journalId,
      'summary': summary,
      'steps': const <Object?>[],
      'user_id': session.userId,
      'user_email': session.email,
    };
    try {
      final response = await _request(
        'POST',
        '/rest/v1/history',
        session,
        extraHeaders: const [('Prefer', 'return=minimal')],
        body: body,
      );
      return response.status >= 200 && response.status < 300;
    } catch (_) {
      return false;
    }
  }

  /// Marks a cloud history row as undone. Failures are ignored: the local
  /// journal remains the authority for undo.
  Future<void> markUndone(String entryId, int now) async {
    if (entryId.isEmpty || !_bounded(entryId, _maxIdChars)) {
      return;
    }
    final StoredSession session;
    try {
      session = await _auth.sessionForCloud(now);
    } catch (_) {
      return;
    }
    final safeId = percentEncodeQueryComponent(entryId);
    try {
      await _request(
        'PATCH',
        '/rest/v1/history?id=eq.$safeId',
        session,
        extraHeaders: const [('Prefer', 'return=minimal')],
        body: <String, Object?>{'undone': true},
      );
    } catch (_) {
      // Ignored by design, as in the Rust implementation.
    }
  }

  Future<HttpResponse> _request(
    String method,
    String path,
    StoredSession session, {
    List<(String, String)> extraHeaders = const [],
    Object? body,
  }) async {
    try {
      return await _http.sendJson(
        method,
        '${_config.url}$path',
        <(String, String)>[
          ('apikey', _config.key),
          ('Authorization', 'Bearer ${session.accessToken}'),
          ...extraHeaders,
        ],
        body,
      );
    } on HttpFailure catch (failure) {
      throw _mapHttpError(failure.error);
    }
  }
}

/// Reads the newest [limit] journal files by file name and reports which ones
/// are undoable. Unreadable or non-JSON files are skipped.
List<HistoryRow> _readLocalHistory(
  Directory journalDir,
  int limit,
  LocalJournalUndoPolicy policy,
) {
  final List<FileSystemEntity> entries;
  try {
    entries = journalDir.listSync(followLinks: false);
  } catch (_) {
    return const <HistoryRow>[];
  }
  final files = <String>[
    for (final entry in entries)
      if (_hasJsonExtension(entry.path)) entry.path,
  ]..sort((left, right) => _fileName(right).compareTo(_fileName(left)));

  final rows = <HistoryRow>[];
  for (final path in files.take(limit)) {
    final Object? decoded;
    try {
      decoded = jsonDecode(utf8.decode(File(path).readAsBytesSync()));
    } catch (_) {
      continue;
    }
    if (decoded is! Map<String, Object?>) {
      continue;
    }
    final steps = decoded['steps'];
    final moves = steps is List<Object?>
        ? steps.where((step) {
            if (step is! Map<String, Object?>) {
              return false;
            }
            final kind = step['kind'];
            return kind == 'move' ||
                kind == 'copy' ||
                kind == 'rename' ||
                kind == 'restore_backup';
          }).length
        : 0;
    rows.add(
      HistoryRow(
        when: _stringValue(decoded['ts']),
        opId: _stringValue(decoded['opId']),
        moves: moves,
        failed: 0,
        journalId: _stringValue(decoded['journalId']),
        steps: null,
        cloudId: null,
        undone: false,
        undoAvailable: policy.isSafelyUndoable(decoded),
        fromCloud: false,
        userEmail: '',
      ),
    );
  }
  return rows;
}

/// Matches Rust `Path::extension() == "json"`: the name must contain a dot that
/// is not its first character, and the text after the last dot must be `json`.
bool _hasJsonExtension(String path) {
  final name = _fileName(path);
  final dot = name.lastIndexOf('.');
  return dot > 0 && name.substring(dot + 1) == 'json';
}

String _fileName(String path) {
  final separator = path.lastIndexOf(Platform.pathSeparator);
  return separator < 0 ? path : path.substring(separator + 1);
}

Object? _cloudJson(HttpResponse response) {
  if (response.status < 200 || response.status >= 300) {
    throw BackendError.requestFailed('cloud_history_http_${response.status}');
  }
  try {
    return response.json();
  } on HttpFailure {
    throw BackendError.requestFailed('cloud_history_invalid_response');
  }
}

bool _bounded(String value, int maximum) => value.runes.length <= maximum;

String _stringValue(Object? value) => value is String ? value : '';

int _unsignedValue(Object? value) => value is int && value >= 0 ? value : 0;

BackendError _mapHttpError(HttpError error) => switch (error) {
  HttpError.timeout => BackendError.networkTimeout(),
  HttpError.responseTooLarge => BackendError.responseTooLarge(),
  HttpError.offline => BackendError.offline(),
  _ => BackendError.requestFailed(error.message),
};
