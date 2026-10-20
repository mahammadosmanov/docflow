/// Port of src-tauri/src/scan_index.rs.
///
/// Durable, owner-scoped analysis cache and scan audit ledger, stored in SQLite
/// through package:sqlite3. The index is an optimization only. Mutating
/// operations must continue to pin and revalidate their own hashes and
/// filesystem identities before Apply.
///
/// Errors are reported as [ScanIndexException] carrying the same messages as the
/// Rust `Result<_, String>` values.
library;

import 'dart:convert';
import 'dart:io';

import 'package:path/path.dart' as p;
import 'package:sqlite3/sqlite3.dart';
import 'package:uuid/uuid.dart';

import 'journal.dart' show FilesystemObjectIdentity, readFilesystemObjectIdentity;

const int scanIndexSchemaVersion = 1;
const int _maxKeyChars = 32768;
const int _maxOutcomeBytes = 16 * 1024 * 1024;
const Uuid _uuid = Uuid();

/// Error from the scan index (the Rust `String` error).
class ScanIndexException implements Exception {
  const ScanIndexException(this.message);

  final String message;

  @override
  String toString() => message;
}

/// Port of `FileFingerprint`: size, modification time (nanoseconds) and the
/// filesystem identity of the analyzed file.
class FileFingerprint {
  const FileFingerprint({
    required this.size,
    required this.modifiedNs,
    required this.identity,
  });

  final int size;
  final int modifiedNs;
  final FilesystemObjectIdentity identity;

  Map<String, Object?> toJson() => {
    'size': size,
    'modifiedNs': modifiedNs,
    'identity': identity.toJson(),
  };

  factory FileFingerprint.fromJson(Map<String, Object?> json) {
    const allowed = {'size', 'modifiedNs', 'identity'};
    for (final key in json.keys) {
      if (!allowed.contains(key)) {
        throw FormatException('unknown field: $key');
      }
    }
    return FileFingerprint(
      size: (json['size']! as num).toInt(),
      modifiedNs: (json['modifiedNs']! as num).toInt(),
      identity: FilesystemObjectIdentity.fromJson(
        json['identity']! as Map<String, Object?>,
      ),
    );
  }

  /// Port of `FileFingerprint::read`.
  static FileFingerprint read(String path) {
    final FileStat stat;
    final DateTime modified;
    final FileSystemEntityType type;
    try {
      stat = FileStat.statSync(path);
      type = stat.type;
      modified = stat.modified;
    } on FileSystemException catch (error) {
      throw ScanIndexException('open fingerprint source: $error');
    }
    if (type != FileSystemEntityType.file) {
      throw const ScanIndexException(
        'fingerprint source is not a regular file',
      );
    }
    final modifiedSinceEpoch = modified.toUtc().difference(
      DateTime.utc(1970),
    );
    if (modifiedSinceEpoch.isNegative) {
      throw const ScanIndexException(
        'fingerprint modification time predates the Unix epoch',
      );
    }
    final FilesystemObjectIdentity identity;
    try {
      identity = readFilesystemObjectIdentity(path);
    } on Object catch (error) {
      throw ScanIndexException('read fingerprint identity: $error');
    }
    return FileFingerprint(
      size: stat.size,
      modifiedNs: modifiedSinceEpoch.inMicroseconds * 1000,
      identity: identity,
    );
  }

  @override
  bool operator ==(Object other) =>
      other is FileFingerprint &&
      other.size == size &&
      other.modifiedNs == modifiedNs &&
      other.identity == identity;

  @override
  int get hashCode => Object.hash(size, modifiedNs, identity);
}

/// Port of `ScanIndexStats`. The last two fields default when absent.
class ScanIndexStats {
  const ScanIndexStats({
    this.discovered = 0,
    this.reused = 0,
    this.analyzed = 0,
    this.failed = 0,
    this.priorInterruptedRuns = 0,
    this.forceRescan = false,
    this.parallelAnalysis = false,
    this.analysisWorkers = 0,
  });

  final int discovered;
  final int reused;
  final int analyzed;
  final int failed;
  final int priorInterruptedRuns;
  final bool forceRescan;
  final bool parallelAnalysis;
  final int analysisWorkers;

  Map<String, Object?> toJson() => {
    'discovered': discovered,
    'reused': reused,
    'analyzed': analyzed,
    'failed': failed,
    'priorInterruptedRuns': priorInterruptedRuns,
    'forceRescan': forceRescan,
    'parallelAnalysis': parallelAnalysis,
    'analysisWorkers': analysisWorkers,
  };

  factory ScanIndexStats.fromJson(Map<String, Object?> json) => ScanIndexStats(
    discovered: (json['discovered']! as num).toInt(),
    reused: (json['reused']! as num).toInt(),
    analyzed: (json['analyzed']! as num).toInt(),
    failed: (json['failed']! as num).toInt(),
    priorInterruptedRuns: (json['priorInterruptedRuns']! as num).toInt(),
    forceRescan: json['forceRescan']! as bool,
    parallelAnalysis: (json['parallelAnalysis'] as bool?) ?? false,
    analysisWorkers: (json['analysisWorkers'] as num?)?.toInt() ?? 0,
  );
}

/// One scan run. Port of `ScanRun`.
///
/// Dart has no `Drop`: call [complete] when the scan finishes. A run that is
/// neither completed nor abandoned is recorded as interrupted by the next run
/// of the same scope, exactly as Rust does on drop. [abandon] makes that
/// explicit.
class ScanRun {
  ScanRun._(
    this._index,
    this.id,
    this._ownerId,
    this._operationId,
    this._profile,
    this._root,
  );

  final ScanIndex _index;
  final String id;
  final String _ownerId;
  final String _operationId;
  final String _profile;
  final String _root;
  bool _completed = false;

  /// Port of `lookup`. [decode] converts the cached JSON into `T`.
  T? lookup<T>(
    String relativePath,
    FileFingerprint fingerprint,
    T Function(Object? json) decode,
  ) => _index._lookup(
    _ownerId,
    _operationId,
    _profile,
    _root,
    relativePath,
    fingerprint,
    decode,
  );

  /// Port of `record_success`. [outcome] must be JSON-encodable.
  void recordSuccess(
    String relativePath,
    FileFingerprint fingerprint,
    Object? outcome,
  ) => _index._recordSuccess(
    id,
    _ownerId,
    _operationId,
    _profile,
    _root,
    relativePath,
    fingerprint,
    outcome,
  );

  /// Port of `record_failure`.
  void recordFailure(String relativePath, String message) =>
      _index._recordFailure(
        id,
        _ownerId,
        _operationId,
        _profile,
        _root,
        relativePath,
        message,
      );

  /// Port of `complete`: marks this run completed with its final stats.
  void complete(ScanIndexStats stats) {
    _index._completeRun(id, stats);
    _completed = true;
  }

  /// Marks an unfinished run interrupted (the Rust `Drop` behaviour).
  void abandon() {
    if (!_completed) {
      _index._abandonRun(id);
      _completed = true;
    }
  }
}

/// Port of `ScanIndex`.
class ScanIndex {
  ScanIndex._(this._db, this.path, this._instanceId);

  /// Opens (creating when needed) the durable index at [path].
  factory ScanIndex.open(String path) {
    try {
      Directory(p.dirname(path)).createSync(recursive: true);
    } on FileSystemException catch (error) {
      throw ScanIndexException('create scan index directory: $error');
    }
    final Database db;
    try {
      db = sqlite3.open(path);
    } on SqliteException catch (error) {
      throw ScanIndexException('open durable scan index: $error');
    }
    try {
      final existingVersion = db.userVersion;
      if (existingVersion > scanIndexSchemaVersion) {
        throw ScanIndexException(
          'scan index schema $existingVersion is newer than supported schema '
          '$scanIndexSchemaVersion',
        );
      }
      for (final statement in _schemaStatements) {
        db.execute(statement);
      }
      db.userVersion = scanIndexSchemaVersion;
    } on SqliteException catch (error) {
      db.close();
      throw ScanIndexException('initialize durable scan index: $error');
    } on ScanIndexException {
      db.close();
      rethrow;
    }
    return ScanIndex._(db, path, _uuid.v4());
  }

  static const List<String> _schemaStatements = [
    'PRAGMA journal_mode=WAL',
    'PRAGMA synchronous=FULL',
    'PRAGMA foreign_keys=ON',
    'PRAGMA busy_timeout=5000',
    '''CREATE TABLE IF NOT EXISTS scan_runs (
       run_id TEXT PRIMARY KEY,
       instance_id TEXT NOT NULL,
       owner_id TEXT NOT NULL,
       operation_id TEXT NOT NULL,
       profile TEXT NOT NULL,
       root TEXT NOT NULL,
       started_at_ms INTEGER NOT NULL,
       finished_at_ms INTEGER,
       status TEXT NOT NULL CHECK(status IN ('running','completed','interrupted')),
       force_rescan INTEGER NOT NULL,
       discovered INTEGER NOT NULL DEFAULT 0,
       reused INTEGER NOT NULL DEFAULT 0,
       analyzed INTEGER NOT NULL DEFAULT 0,
       failed INTEGER NOT NULL DEFAULT 0,
       prior_interrupted_runs INTEGER NOT NULL DEFAULT 0
     )''',
    '''CREATE INDEX IF NOT EXISTS scan_runs_scope
       ON scan_runs(owner_id, operation_id, profile, root, started_at_ms)''',
    '''CREATE TABLE IF NOT EXISTS scan_entries (
       owner_id TEXT NOT NULL,
       operation_id TEXT NOT NULL,
       profile TEXT NOT NULL,
       root TEXT NOT NULL,
       relative_path TEXT NOT NULL,
       fingerprint_json TEXT NOT NULL,
       outcome_json TEXT NOT NULL,
       updated_at_ms INTEGER NOT NULL,
       run_id TEXT NOT NULL,
       PRIMARY KEY(owner_id, operation_id, profile, root, relative_path)
     )''',
    '''CREATE TABLE IF NOT EXISTS scan_events (
       event_id INTEGER PRIMARY KEY AUTOINCREMENT,
       run_id TEXT NOT NULL,
       owner_id TEXT NOT NULL,
       operation_id TEXT NOT NULL,
       profile TEXT NOT NULL,
       root TEXT NOT NULL,
       relative_path TEXT NOT NULL,
       recorded_at_ms INTEGER NOT NULL,
       outcome TEXT NOT NULL CHECK(outcome IN ('analyzed','failed')),
       detail TEXT NOT NULL
     )''',
  ];

  final Database _db;
  final String path;
  final String _instanceId;

  /// Port of `begin_run`. Returns the run and the number of interrupted runs
  /// since the last completed run of the same scope.
  (ScanRun, int) beginRun(
    String ownerId,
    String operationId,
    String profile,
    String root, {
    required bool forceRescan,
  }) {
    _validateKey(ownerId);
    _validateKey(operationId);
    _validateKey(profile);
    _validateKey(root);
    final now = _nowMs();
    _transaction(() {
      final activeHere =
          _db.select(
                '''SELECT COUNT(*) FROM scan_runs
                 WHERE owner_id=?1 AND operation_id=?2 AND profile=?3 AND root=?4
                   AND status='running' AND instance_id=?5''',
                <Object?>[ownerId, operationId, profile, root, _instanceId],
              ).first.columnAt(0)
              as int;
      if (activeHere != 0) {
        throw const ScanIndexException(
          'the same scan is already running in this app instance',
        );
      }
      _db.execute(
        '''UPDATE scan_runs SET status='interrupted', finished_at_ms=?1
           WHERE owner_id=?2 AND operation_id=?3 AND profile=?4 AND root=?5
             AND status='running' AND instance_id<>?6''',
        <Object?>[now, ownerId, operationId, profile, root, _instanceId],
      );
    });
    final prior =
        (_db.select(
              '''SELECT COUNT(*) FROM scan_runs
               WHERE owner_id=?1 AND operation_id=?2 AND profile=?3 AND root=?4
                 AND status='interrupted'
                 AND started_at_ms > COALESCE((
                   SELECT MAX(started_at_ms) FROM scan_runs
                   WHERE owner_id=?1 AND operation_id=?2 AND profile=?3 AND root=?4
                     AND status='completed'
                 ), -1)''',
              <Object?>[ownerId, operationId, profile, root],
            ).first.columnAt(0)
            as int);
    final id = _uuid.v4();
    _transaction(() {
      _db.execute(
        '''INSERT INTO scan_runs
           (run_id,instance_id,owner_id,operation_id,profile,root,started_at_ms,status,force_rescan,prior_interrupted_runs)
           VALUES (?1,?2,?3,?4,?5,?6,?7,'running',?8,?9)''',
        [
          id,
          _instanceId,
          ownerId,
          operationId,
          profile,
          root,
          now,
          forceRescan ? 1 : 0,
          _countI64(prior),
        ],
      );
    });
    return (
      ScanRun._(this, id, ownerId, operationId, profile, root),
      prior,
    );
  }

  T? _lookup<T>(
    String owner,
    String operation,
    String profile,
    String root,
    String relativePath,
    FileFingerprint fingerprint,
    T Function(Object? json) decode,
  ) {
    final relative = _normalizedRelative(relativePath);
    final fingerprintJson = _encodeBounded(fingerprint.toJson());
    final rows = _db.select(
      '''SELECT outcome_json FROM scan_entries
         WHERE owner_id=?1 AND operation_id=?2 AND profile=?3 AND root=?4
           AND relative_path=?5 AND fingerprint_json=?6''',
      [owner, operation, profile, root, relative, fingerprintJson],
    );
    if (rows.isEmpty) {
      return null;
    }
    final Object? decoded;
    try {
      decoded = jsonDecode(rows.first.columnAt(0) as String);
    } on FormatException catch (error) {
      throw ScanIndexException('decode cached scan outcome: $error');
    }
    return decode(decoded);
  }

  void _recordSuccess(
    String runId,
    String owner,
    String operation,
    String profile,
    String root,
    String relativePath,
    FileFingerprint fingerprint,
    Object? outcome,
  ) {
    final relative = _normalizedRelative(relativePath);
    final fingerprintJson = _encodeBounded(fingerprint.toJson());
    final outcomeJson = _encodeBounded(outcome);
    final now = _nowMs();
    _transaction(() {
      _db.execute(
        '''INSERT INTO scan_entries
           (owner_id,operation_id,profile,root,relative_path,fingerprint_json,outcome_json,updated_at_ms,run_id)
           VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
           ON CONFLICT(owner_id,operation_id,profile,root,relative_path) DO UPDATE SET
             fingerprint_json=excluded.fingerprint_json,
             outcome_json=excluded.outcome_json,
             updated_at_ms=excluded.updated_at_ms,
             run_id=excluded.run_id''',
        [
          owner,
          operation,
          profile,
          root,
          relative,
          fingerprintJson,
          outcomeJson,
          now,
          runId,
        ],
      );
      _db.execute(
        '''INSERT INTO scan_events
           (run_id,owner_id,operation_id,profile,root,relative_path,recorded_at_ms,outcome,detail)
           VALUES (?1,?2,?3,?4,?5,?6,?7,'analyzed','')''',
        [runId, owner, operation, profile, root, relative, now],
      );
    });
  }

  void _recordFailure(
    String runId,
    String owner,
    String operation,
    String profile,
    String root,
    String relativePath,
    String message,
  ) {
    final relative = _normalizedRelative(relativePath);
    final detail = message.runes.take(2048).map(String.fromCharCode).join();
    _db.execute(
      '''INSERT INTO scan_events
         (run_id,owner_id,operation_id,profile,root,relative_path,recorded_at_ms,outcome,detail)
         VALUES (?1,?2,?3,?4,?5,?6,?7,'failed',?8)''',
      [runId, owner, operation, profile, root, relative, _nowMs(), detail],
    );
  }

  void _completeRun(String runId, ScanIndexStats stats) {
    _db.execute(
      '''UPDATE scan_runs SET status='completed', finished_at_ms=?1,
           discovered=?2,reused=?3,analyzed=?4,failed=?5,prior_interrupted_runs=?6
         WHERE run_id=?7 AND status='running' ''',
      [
        _nowMs(),
        _countI64(stats.discovered),
        _countI64(stats.reused),
        _countI64(stats.analyzed),
        _countI64(stats.failed),
        _countI64(stats.priorInterruptedRuns),
        runId,
      ],
    );
    if (_db.updatedRows != 1) {
      throw const ScanIndexException(
        'scan run was not active while completing',
      );
    }
  }

  void _abandonRun(String runId) {
    _db.execute(
      '''UPDATE scan_runs SET status='interrupted', finished_at_ms=?1
         WHERE run_id=?2 AND status='running' ''',
      [_nowMs(), runId],
    );
  }

  /// Runs [body] inside a write transaction and rolls back on failure.
  void _transaction(void Function() body) {
    _db.execute('BEGIN');
    try {
      body();
      _db.execute('COMMIT');
    } on Object {
      try {
        _db.execute('ROLLBACK');
      } on SqliteException {
        // The original error is the one that matters.
      }
      rethrow;
    }
  }

  /// Closes the underlying database connection.
  void close() => _db.close();
}

String _normalizedRelative(String path) {
  if (p.isAbsolute(path) ||
      p.split(path).any((component) => component == '..')) {
    throw const ScanIndexException(
      'scan index path must be relative and confined',
    );
  }
  final value = path.replaceAll('\\', '/');
  _validateKey(value);
  return value;
}

void _validateKey(String value) {
  if (value.isEmpty || value.runes.length > _maxKeyChars || value.contains('\u0000')) {
    throw const ScanIndexException(
      'scan index key is invalid or exceeds its bound',
    );
  }
}

String _encodeBounded(Object? value) {
  final encoded = jsonEncode(value);
  if (utf8.encode(encoded).length > _maxOutcomeBytes) {
    throw const ScanIndexException(
      'scan index value exceeds its storage bound',
    );
  }
  return encoded;
}

int _nowMs() => DateTime.now().millisecondsSinceEpoch;

int _countI64(int value) => value;
