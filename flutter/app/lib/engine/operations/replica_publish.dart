/// Port of src-tauri/src/operations/replica_publish.rs.
///
/// Source-authoritative, backup-first publication to two RFI System replicas.
/// Preview only: builds an immutable plan; Apply lives in the executor.
///
/// Dart-specific notes:
/// - Tree hashing runs in worker isolates through [boundedMapStream]; the
///   worker function is top-level and returns plain result objects.
/// - Path ordering reproduces Rust `Path` ordering (component-wise), which
///   decides the order of files and directory steps in the plan.
library;

import 'dart:collection';
import 'dart:io';
import 'dart:math' as math;
import 'dart:typed_data';

import 'package:crypto/crypto.dart' as crypto;
import 'package:path/path.dart' as p;

import '../contracts.dart';
import '../errors.dart';
import '../journal.dart' show validSha256;
import '../paths.dart' show canonicalizePath, newUuidV4;
import '../parallel.dart';
import '../scan_index.dart';

const String replicaPublishOperationId = 'files.replica_publish';
const int _maxEntries = 500000;
const int _maxRootFiles = 64;
const int _maxWarnings = 200;
const String _hashProfile = 'sha256-v1';
const int _maxIntervalMinutes = 1440;
const int _maxSettleSeconds = 3600;
const int _maxChangedFiles = 500000;
const int _maxChangedMib = 1000000;

/// RFI System discipline folders. Mirrors `rfi_system_sync::DISCIPLINES`
/// until that module is ported.
const List<String> _disciplines = ['CIV', 'MEC', 'ELE', 'PRC'];

const Set<String> _knownParamsKeys = {
  'root_a',
  'root_b',
  'root_c',
  'source_root',
  'backup_root',
  'root_files',
  'unattended',
  'interval_minutes',
  'settle_seconds',
  'max_changed_files',
  'max_changed_mib',
};

/// Port of `ReplicaPublishParams` (`deny_unknown_fields`, snake_case keys).
class ReplicaPublishParams {
  const ReplicaPublishParams({
    required this.rootA,
    required this.rootB,
    required this.rootC,
    required this.sourceRoot,
    required this.backupRoot,
    this.rootFiles = '',
    this.unattended = false,
    this.intervalMinutes = 15,
    this.settleSeconds = 30,
    this.maxChangedFiles = 250,
    this.maxChangedMib = 2048,
  });

  final String rootA;
  final String rootB;
  final String rootC;
  final String sourceRoot;
  final String backupRoot;
  final String rootFiles;
  final bool unattended;
  final int intervalMinutes;
  final int settleSeconds;
  final int maxChangedFiles;
  final int maxChangedMib;

  factory ReplicaPublishParams.fromJson(JsonObject json) {
    for (final key in json.keys) {
      if (!_knownParamsKeys.contains(key)) {
        throw FormatException('unknown field `$key`');
      }
    }
    return ReplicaPublishParams(
      rootA: _requiredString(json, 'root_a'),
      rootB: _requiredString(json, 'root_b'),
      rootC: _requiredString(json, 'root_c'),
      sourceRoot: _requiredString(json, 'source_root'),
      backupRoot: _requiredString(json, 'backup_root'),
      rootFiles: _optionalString(json, 'root_files', ''),
      unattended: _optionalBool(json, 'unattended', false),
      intervalMinutes: _optionalUnsigned(json, 'interval_minutes', 15),
      settleSeconds: _optionalUnsigned(json, 'settle_seconds', 30),
      maxChangedFiles: _optionalUnsigned(json, 'max_changed_files', 250),
      maxChangedMib: _optionalUnsigned(json, 'max_changed_mib', 2048),
    );
  }

  JsonObject toJson() => {
    'root_a': rootA,
    'root_b': rootB,
    'root_c': rootC,
    'source_root': sourceRoot,
    'backup_root': backupRoot,
    'root_files': rootFiles,
    'unattended': unattended,
    'interval_minutes': intervalMinutes,
    'settle_seconds': settleSeconds,
    'max_changed_files': maxChangedFiles,
    'max_changed_mib': maxChangedMib,
  };
}

String _requiredString(JsonObject json, String key) {
  final value = json[key];
  if (value is! String) {
    throw FormatException('missing or invalid field `$key`');
  }
  return value;
}

String _optionalString(JsonObject json, String key, String fallback) {
  if (!json.containsKey(key)) {
    return fallback;
  }
  return _requiredString(json, key);
}

bool _optionalBool(JsonObject json, String key, bool fallback) {
  if (!json.containsKey(key)) {
    return fallback;
  }
  final value = json[key];
  if (value is! bool) {
    throw FormatException('invalid field `$key`');
  }
  return value;
}

int _optionalUnsigned(JsonObject json, String key, int fallback) {
  if (!json.containsKey(key)) {
    return fallback;
  }
  final value = json[key];
  if (value is! int || value < 0) {
    throw FormatException('invalid unsigned field `$key`');
  }
  return value;
}

/// Port of `ReplicaStepMeta` (`camelCase`, `deny_unknown_fields`, all required).
class ReplicaStepMeta {
  const ReplicaStepMeta({
    required this.sourceSha256,
    required this.expectedTargetSha256,
    required this.backupPath,
    required this.relativePath,
    required this.targetName,
  });

  final String sourceSha256;
  final String expectedTargetSha256;
  final String backupPath;
  final String relativePath;
  final String targetName;

  static const Set<String> _keys = {
    'sourceSha256',
    'expectedTargetSha256',
    'backupPath',
    'relativePath',
    'targetName',
  };

  factory ReplicaStepMeta.fromJson(JsonObject json) {
    for (final key in json.keys) {
      if (!_keys.contains(key)) {
        throw FormatException('unknown field `$key`');
      }
    }
    return ReplicaStepMeta(
      sourceSha256: _requiredString(json, 'sourceSha256'),
      expectedTargetSha256: _requiredString(json, 'expectedTargetSha256'),
      backupPath: _requiredString(json, 'backupPath'),
      relativePath: _requiredString(json, 'relativePath'),
      targetName: _requiredString(json, 'targetName'),
    );
  }

  JsonObject toJson() => {
    'sourceSha256': sourceSha256,
    'expectedTargetSha256': expectedTargetSha256,
    'backupPath': backupPath,
    'relativePath': relativePath,
    'targetName': targetName,
  };
}

/// Port of `decode_step_meta`.
ReplicaStepMeta decodeReplicaStepMeta(Step step) {
  if (step.kind != StepKind.replicaPublish) {
    throw BackendError.validation('error.replica.approvedStepInvalid');
  }
  final raw = step.meta['replicaPublish'];
  if (raw is! Map) {
    throw BackendError.validation('error.replica.approvedStepInvalid');
  }
  try {
    return ReplicaStepMeta.fromJson(Map<String, Object?>.from(raw));
  } on Object {
    throw BackendError.validation('error.replica.approvedStepInvalid');
  }
}

/// Port of `FileSnapshot`.
class _FileSnapshot {
  _FileSnapshot({
    required this.path,
    required this.len,
    required this.modified,
  });

  final String path;
  String hash = '';
  final int len;
  final DateTime? modified;
}

/// Port of `TreeSnapshot`. Keys are relative paths ordered component-wise.
class _TreeSnapshot {
  final SplayTreeMap<String, _FileSnapshot> files = SplayTreeMap(
    _compareComponents,
  );
  final SplayTreeSet<String> directories = SplayTreeSet(_compareComponents);
}

/// Port of the `ScanIndexStats` accumulator used by `hash_snapshot`.
class _StatsAccumulator {
  int discovered = 0;
  int reused = 0;
  int analyzed = 0;
  int failed = 0;
  int priorInterruptedRuns = 0;
  bool parallelAnalysis = false;
  int analysisWorkers = 0;

  ScanIndexStats toStats() => ScanIndexStats(
    discovered: discovered,
    reused: reused,
    analyzed: analyzed,
    failed: failed,
    priorInterruptedRuns: priorInterruptedRuns,
    parallelAnalysis: parallelAnalysis,
    analysisWorkers: analysisWorkers,
  );
}

/// Input of one hash job. Sendable across isolates.
class _HashInput {
  const _HashInput({
    required this.path,
    required this.len,
    required this.modified,
  });

  final String path;
  final int len;
  final DateTime? modified;
}

/// Result of one hash job. Sendable across isolates.
class _HashOutcome {
  const _HashOutcome.ok(this.hash) : error = null;
  const _HashOutcome.failed(this.error) : hash = null;

  final String? hash;
  final String? error;
}

/// Port of `CachedHash`.
class _CachedHash {
  const _CachedHash(this.sha256);

  final String sha256;

  factory _CachedHash.fromJson(Object? json) {
    if (json is! Map || json['sha256'] is! String) {
      throw const FormatException('invalid cached hash');
    }
    return _CachedHash(json['sha256']! as String);
  }
}

/// Port of `preview`.
Future<Plan> previewReplicaPublish(
  ReplicaPublishParams params,
  void Function(ProgressEvent event) progress,
) => _previewInner(params, null, false, progress);

/// Port of `preview_with_index`.
Future<Plan> previewReplicaPublishWithIndex(
  ReplicaPublishParams params, {
  required String ownerId,
  required ScanIndex index,
  required void Function(ProgressEvent event) progress,
}) => _previewInner(params, (ownerId, index), false, progress);

/// Port of `preview_automatic_with_index`.
Future<Plan> previewAutomaticReplicaPublishWithIndex(
  ReplicaPublishParams params, {
  required String ownerId,
  required ScanIndex index,
  required void Function(ProgressEvent event) progress,
}) => _previewInner(params, (ownerId, index), true, progress);

Future<Plan> _previewInner(
  ReplicaPublishParams params,
  (String, ScanIndex)? cache,
  bool automatic,
  void Function(ProgressEvent event) progress,
) async {
  final roots = [
    _canonicalDirectory(params.rootA, 'error.replica.rootInvalid'),
    _canonicalDirectory(params.rootB, 'error.replica.rootInvalid'),
    _canonicalDirectory(params.rootC, 'error.replica.rootInvalid'),
  ];
  final sourceIndex = switch (params.sourceRoot) {
    'a' => 0,
    'b' => 1,
    'c' => 2,
    _ => throw BackendError.validation('error.replica.sourceInvalid'),
  };
  final sourceRoot = roots[sourceIndex];
  final backupRoot = _canonicalDirectory(
    params.backupRoot,
    'error.replica.backupInvalid',
  );
  _validateRootTopology(roots, backupRoot);
  final rootFiles = _parseRootFiles(params.rootFiles);
  if (!(params.intervalMinutes >= 1 &&
      params.intervalMinutes <= _maxIntervalMinutes)) {
    throw BackendError.validation('error.replica.intervalInvalid');
  }
  if (!(params.settleSeconds >= 5 &&
      params.settleSeconds <= _maxSettleSeconds)) {
    throw BackendError.validation('error.replica.settleInvalid');
  }
  if (!(params.maxChangedFiles >= 1 &&
          params.maxChangedFiles <= _maxChangedFiles) ||
      !(params.maxChangedMib >= 1 && params.maxChangedMib <= _maxChangedMib)) {
    throw BackendError.validation('error.replica.changeLimitInvalid');
  }

  progress(
    ProgressEvent(
      phase: 'replica_discovery',
      done: 0,
      total: 3,
      subject: sourceRoot,
    ),
  );
  final source = _snapshotTree(sourceRoot, rootFiles, requireDisciplines: true);
  if (automatic) {
    _ensureSourceSettled(source, params.settleSeconds);
  }
  progress(
    ProgressEvent(
      phase: 'replica_discovery',
      done: 1,
      total: 3,
      subject: sourceRoot,
    ),
  );
  final scanStats = _StatsAccumulator();
  await _hashSnapshot(
    sourceRoot,
    source,
    cache,
    scanStats,
    progress,
    'replica_source_hash',
  );

  final runId = newUuidV4().replaceAll('-', '');
  final steps = <Step>[];
  final warnings = <Warning>[];
  final rows = <Map<String, Object?>>[];
  var targetNumber = 0;
  var changedBytes = 0;
  for (var index = 0; index < roots.length; index++) {
    if (index == sourceIndex) {
      continue;
    }
    targetNumber++;
    final targetName = const ['A', 'B', 'C'][index];
    final targetRoot = roots[index];
    final target = _snapshotTree(
      targetRoot,
      rootFiles,
      requireDisciplines: false,
    );
    await _hashSnapshot(
      targetRoot,
      target,
      cache,
      scanStats,
      progress,
      'replica_target_hash',
    );
    progress(
      ProgressEvent(
        phase: 'replica_discovery',
        done: targetNumber + 1,
        total: 3,
        subject: targetRoot,
      ),
    );

    final replacementPaths = SplayTreeSet<String>(_compareComponents);
    for (final entry in source.files.entries) {
      final targetFile = target.files[entry.key];
      if (targetFile != null && targetFile.hash != entry.value.hash) {
        replacementPaths.add(entry.key);
      }
    }
    final backupRun = p.join(backupRoot, runId, 'target-$targetName');
    final directories = SplayTreeSet<String>(_compareComponents);
    for (final relative in source.directories) {
      final full = p.join(targetRoot, relative);
      if (!_pathExists(full)) {
        directories.add(full);
      }
    }
    for (final relative in replacementPaths) {
      final parent = p.dirname(relative);
      final leaf = parent == '.' ? backupRun : p.join(backupRun, parent);
      _collectMissingDirectories(backupRoot, leaf, directories);
    }
    for (final directory in directories) {
      steps.add(
        Step(
          kind: StepKind.mkdir,
          src: null,
          dst: directory,
          noteKey: 'note.replica.mkdir',
          meta: const {},
        ),
      );
    }

    for (final entry in source.files.entries) {
      final relative = entry.key;
      final sourceFile = entry.value;
      final destination = p.join(targetRoot, relative);
      final targetFile = target.files[relative];
      if (targetFile == null) {
        changedBytes = _saturatingAdd(changedBytes, sourceFile.len);
        steps.add(
          Step(
            kind: StepKind.copy,
            src: sourceFile.path,
            dst: destination,
            noteKey: 'note.replica.copy',
            meta: {'expectedSourceSha256': sourceFile.hash},
          ),
        );
        rows.add(_reviewRow(targetName, relative, 'copy', '', sourceFile.hash));
      } else if (targetFile.hash == sourceFile.hash) {
        // Identical content: nothing to publish.
      } else {
        changedBytes = _saturatingAdd(changedBytes, sourceFile.len);
        final stepMeta = ReplicaStepMeta(
          sourceSha256: sourceFile.hash,
          expectedTargetSha256: targetFile.hash,
          backupPath: p.join(backupRun, relative),
          relativePath: relative,
          targetName: targetName,
        );
        steps.add(
          Step(
            kind: StepKind.replicaPublish,
            src: sourceFile.path,
            dst: destination,
            noteKey: 'note.replica.replace',
            meta: {'replicaPublish': stepMeta.toJson()},
          ),
        );
        rows.add(
          _reviewRow(
            targetName,
            relative,
            'replace',
            targetFile.hash,
            sourceFile.hash,
          ),
        );
      }
    }
    var omittedExtraWarnings = 0;
    for (final relative in target.files.keys) {
      if (source.files.containsKey(relative)) {
        continue;
      }
      if (warnings.length < _maxWarnings) {
        warnings.add(
          Warning(
            key: 'warn.replica.targetExtra',
            subject: '$targetName: $relative',
          ),
        );
      } else {
        omittedExtraWarnings++;
      }
    }
    if (omittedExtraWarnings > 0) {
      warnings.add(
        Warning(
          key: 'warn.replica.targetExtraOverflow',
          subject: '$targetName: $omittedExtraWarnings',
        ),
      );
    }
  }

  if (steps.isEmpty) {
    warnings.add(const Warning(key: 'warn.replica.noChanges', subject: ''));
  }
  final changedFileCount = rows.length;
  if (automatic) {
    enforceAutomaticChangeLimit(changedFileCount, changedBytes, params);
  }
  final meta = <String, Object?>{
    'editor': {
      'kind': 'recordTable',
      'layout': 'compact',
      'titleKey': 'replica.editor.title',
      'rowCountKey': 'replica.editor.rowCount',
      'requiredFields': <Object?>[],
      'groupField': 'target',
      'selectionGroupFields': <Object?>[],
      'columns': [
        {
          'key': 'target',
          'labelKey': 'replica.column.target',
          'kind': 'readonly',
        },
        {
          'key': 'relativePath',
          'labelKey': 'replica.column.path',
          'kind': 'readonly',
        },
        {
          'key': 'action',
          'labelKey': 'replica.column.action',
          'kind': 'readonly',
          'display': 'localizedKey',
        },
        {
          'key': 'beforeHash',
          'labelKey': 'replica.column.beforeHash',
          'kind': 'readonly',
          'detail': true,
        },
        {
          'key': 'afterHash',
          'labelKey': 'replica.column.afterHash',
          'kind': 'readonly',
          'detail': true,
        },
      ],
      'rows': rows,
      'duplicateEnabled': false,
      'selectionEnabled': false,
      'bulkEnabled': false,
      'reorderable': false,
      'detailsEnabled': true,
      'detailsDefaultVisible': false,
      'copyEnabled': false,
    },
    'sourceRoot': sourceRoot,
    'backupRoot': backupRoot,
    'unattended': params.unattended,
    'intervalMinutes': params.intervalMinutes,
    'changedFileCount': changedFileCount,
    'changedBytes': changedBytes,
    if (cache != null) 'scanIndex': scanStats.toStats().toJson(),
    if (params.unattended) 'automationProfile': params.toJson(),
  };
  progress(
    const ProgressEvent(phase: 'preview_ready', done: 1, total: 1, subject: ''),
  );
  return Plan(
    planId: '',
    opId: replicaPublishOperationId,
    steps: steps,
    warnings: warnings,
    meta: meta,
  );
}

Map<String, Object?> _reviewRow(
  String target,
  String relative,
  String action,
  String beforeHash,
  String afterHash,
) => {
  '_rowId': newUuidV4(),
  'target': target,
  'relativePath': relative,
  'action': 'replica.action.$action',
  'beforeHash': beforeHash,
  'afterHash': afterHash,
};

String _canonicalDirectory(String path, String key) {
  final type = FileSystemEntity.typeSync(path, followLinks: false);
  if (type != FileSystemEntityType.directory) {
    throw BackendError.pathInvalid(key);
  }
  try {
    return canonicalizePath(path);
  } on Object {
    throw BackendError.pathInvalid(key);
  }
}

void _validateRootTopology(List<String> roots, String backup) {
  for (var index = 0; index < roots.length; index++) {
    final left = roots[index];
    for (var other = index + 1; other < roots.length; other++) {
      final right = roots[other];
      if (left == right ||
          _startsWith(left, right) ||
          _startsWith(right, left)) {
        throw BackendError.validation('error.replica.rootsOverlap');
      }
    }
    if (backup == left ||
        _startsWith(backup, left) ||
        _startsWith(left, backup)) {
      throw BackendError.validation('error.replica.backupOverlap');
    }
  }
}

/// Port of `Path::starts_with`: component-wise, equal paths included.
bool _startsWith(String path, String base) =>
    p.equals(path, base) || p.isWithin(base, path);

SplayTreeSet<String> _parseRootFiles(String value) {
  final result = SplayTreeSet<String>(_compareUtf8);
  for (final raw in value.split(RegExp(r'[\n;]'))) {
    final line = raw.trim();
    if (line.isEmpty) {
      continue;
    }
    if (_componentCount(line) != 1 || line == '.' || line == '..') {
      throw BackendError.validation('error.replica.rootFileInvalid');
    }
    result.add(line);
  }
  if (result.length > _maxRootFiles) {
    throw BackendError.validation('error.replica.tooManyRootFiles');
  }
  return result;
}

/// Port of `Path::components().count()` for a path string.
int _componentCount(String path) {
  final separators = Platform.isWindows ? RegExp(r'[/\\]') : RegExp('/');
  final segments = path.split(separators);
  var count = 0;
  if (path.startsWith('/') || (Platform.isWindows && path.startsWith(r'\'))) {
    count++;
  }
  for (var index = 0; index < segments.length; index++) {
    final segment = segments[index];
    if (segment.isEmpty) {
      continue;
    }
    if (segment == '.') {
      if (index == 0) {
        count++;
      }
      continue;
    }
    count++;
  }
  return count;
}

_TreeSnapshot _snapshotTree(
  String root,
  SplayTreeSet<String> rootFiles, {
  required bool requireDisciplines,
}) {
  final snapshot = _TreeSnapshot();
  final pending = <(String, String)>[];
  for (final discipline in _disciplines) {
    final path = p.join(root, discipline);
    final type = FileSystemEntity.typeSync(path, followLinks: false);
    if (type == FileSystemEntityType.directory) {
      snapshot.directories.add(discipline);
      pending.add((path, discipline));
    } else if (type == FileSystemEntityType.notFound && !requireDisciplines) {
      // Optional discipline absent on a target replica.
    } else {
      throw BackendError.validation('error.replica.disciplineMissing');
    }
  }
  var entryCount = 0;
  while (pending.isNotEmpty) {
    final (directory, relativeDirectory) = pending.removeLast();
    final List<FileSystemEntity> entries;
    try {
      entries = Directory(directory).listSync(followLinks: false);
    } on FileSystemException {
      throw BackendError.pathInvalid('error.replica.scanIncomplete');
    }
    for (final entry in entries) {
      entryCount++;
      if (entryCount > _maxEntries) {
        throw BackendError.validation('error.replica.entryLimit');
      }
      final type = FileSystemEntity.typeSync(entry.path, followLinks: false);
      if (type == FileSystemEntityType.link) {
        throw BackendError.validation('error.replica.symlinkRefused');
      }
      final relative = p.join(relativeDirectory, p.basename(entry.path));
      if (type == FileSystemEntityType.directory) {
        snapshot.directories.add(relative);
        pending.add((entry.path, relative));
      } else if (type == FileSystemEntityType.file) {
        _insertFile(snapshot.files, relative, entry.path);
      }
    }
  }
  for (final name in rootFiles) {
    final path = p.join(root, name);
    final type = FileSystemEntity.typeSync(path, followLinks: false);
    if (type == FileSystemEntityType.file) {
      _insertFile(snapshot.files, name, path);
    } else if (type == FileSystemEntityType.notFound && !requireDisciplines) {
      // Optional root file absent on a target replica.
    } else if (type == FileSystemEntityType.notFound) {
      throw BackendError.validation('error.replica.sourceRootFileMissing');
    } else {
      throw BackendError.validation('error.replica.rootFileInvalid');
    }
  }
  return snapshot;
}

void _insertFile(
  SplayTreeMap<String, _FileSnapshot> files,
  String relative,
  String path,
) {
  final stat = FileStat.statSync(path);
  if (stat.type == FileSystemEntityType.notFound) {
    throw BackendError.pathInvalid('error.replica.scanIncomplete');
  }
  if (files.containsKey(relative)) {
    throw BackendError.validation('error.replica.pathCollision');
  }
  files[relative] = _FileSnapshot(
    path: path,
    len: stat.size,
    modified: stat.modified,
  );
}

void _ensureSourceSettled(_TreeSnapshot snapshot, int settleSeconds) {
  final now = DateTime.now();
  final settled = snapshot.files.values.every((file) {
    final modified = file.modified;
    if (modified == null) {
      return false;
    }
    final age = now.difference(modified);
    return !age.isNegative && age.inSeconds >= settleSeconds;
  });
  if (!settled) {
    throw BackendError.validation('error.replica.sourceSettling');
  }
}

/// Port of `enforce_automatic_change_limit`.
void enforceAutomaticChangeLimit(
  int changedFileCount,
  int changedBytes,
  ReplicaPublishParams params,
) {
  if (changedFileCount > params.maxChangedFiles ||
      changedBytes > params.maxChangedMib * 1024 * 1024) {
    throw BackendError.validation('error.replica.changeLimitExceeded');
  }
}

/// Port of `hash_snapshot`: cached hashes are reused when the scan index
/// fingerprint still matches; the rest are hashed in parallel and validated
/// against the size and modification time captured at discovery.
Future<void> _hashSnapshot(
  String root,
  _TreeSnapshot snapshot,
  (String, ScanIndex)? cache,
  _StatsAccumulator aggregate,
  void Function(ProgressEvent event) progress,
  String phase,
) async {
  final items = snapshot.files.entries.toList();
  final treeTotal = items.length;
  aggregate.discovered += items.length;
  ScanRun? run;
  if (cache != null) {
    try {
      final (started, prior) = cache.$2.beginRun(
        cache.$1,
        replicaPublishOperationId,
        _hashProfile,
        root,
        forceRescan: false,
      );
      run = started;
      aggregate.priorInterruptedRuns += prior;
    } on ScanIndexException catch (error) {
      throw BackendError.requestFailed('replica index: $error');
    }
  }
  try {
    final pending = <(String, _FileSnapshot, FileFingerprint)>[];
    var treeReused = 0;
    for (final item in items) {
      final relative = item.key;
      final file = item.value;
      final FileFingerprint fingerprint;
      try {
        fingerprint = FileFingerprint.read(file.path);
      } on Object catch (error) {
        throw BackendError.validation('error.replica.fileUnstable: $error');
      }
      _CachedHash? cached;
      if (run != null) {
        try {
          cached = run.lookup<_CachedHash>(
            relative,
            fingerprint,
            _CachedHash.fromJson,
          );
        } on Object catch (error) {
          throw BackendError.requestFailed('replica index: $error');
        }
      }
      if (cached != null && validSha256(cached.sha256)) {
        snapshot.files[relative]!.hash = cached.sha256;
        aggregate.reused++;
        treeReused++;
        progress(
          ProgressEvent(
            phase: phase,
            done: treeReused,
            total: treeTotal,
            subject: relative,
          ),
        );
      } else {
        pending.add((relative, file, fingerprint));
      }
    }

    final inputs = [
      for (final (_, file, _) in pending)
        _HashInput(path: file.path, len: file.len, modified: file.modified),
    ];
    var completed = 0;
    final (_, results) =
        await boundedMapStream<_HashInput, _HashOutcome>(
          inputs,
          _hashInputFile,
          (itemIndex, outcome) {
            completed++;
            final (relative, _, fingerprint) = pending[itemIndex];
            if (run != null) {
              try {
                if (outcome.error == null) {
                  run.recordSuccess(relative, fingerprint, {
                    'sha256': outcome.hash,
                  });
                } else {
                  run.recordFailure(relative, outcome.error!);
                }
              } on Object catch (error) {
                throw BackendError.requestFailed('replica index: $error');
              }
            }
            progress(
              ProgressEvent(
                phase: phase,
                done: treeReused + completed,
                total: treeTotal,
                subject: relative,
              ),
            );
          },
        ).catchError((Object error) {
          if (error is BackendError) {
            throw error;
          }
          throw BackendError.requestFailed(error.toString());
        });

    for (var index = 0; index < results.length; index++) {
      final outcome = results[index];
      final relative = pending[index].$1;
      if (outcome.error != null) {
        aggregate.failed++;
        throw BackendError.validation(
          'error.replica.fileUnstable: ${outcome.error}',
        );
      }
      aggregate.analyzed++;
      snapshot.files[relative]!.hash = outcome.hash!;
    }
    if (pending.isNotEmpty) {
      aggregate.parallelAnalysis = true;
      aggregate.analysisWorkers = math.max(
        aggregate.analysisWorkers,
        workerCount(pending.length),
      );
    }
    if (run != null) {
      final reused = snapshot.files.values
          .where((file) => file.hash.isNotEmpty)
          .length;
      final runStats = ScanIndexStats(
        discovered: snapshot.files.length,
        reused: reused > pending.length ? reused - pending.length : 0,
        analyzed: pending.length,
        parallelAnalysis: pending.isNotEmpty,
        analysisWorkers: workerCount(pending.length),
      );
      try {
        run.complete(runStats);
      } on Object catch (error) {
        throw BackendError.requestFailed('replica index: $error');
      }
    }
  } finally {
    run?.abandon();
  }
}

/// Hashes one file on a worker isolate. Mirrors the Rust closure: hash, then
/// verify that size and modification time did not change during the scan.
_HashOutcome _hashInputFile(_HashInput input) {
  try {
    final hash = _sha256File(input.path);
    final stat = FileStat.statSync(input.path);
    if (stat.size != input.len || stat.modified != input.modified) {
      return const _HashOutcome.failed('file changed during scan');
    }
    return _HashOutcome.ok(hash);
  } on Object catch (error) {
    return _HashOutcome.failed(error.toString());
  }
}

/// Port of `fs_ops::file_sha256` (streams 1 MiB chunks, never follows links).
String _sha256File(String path) {
  if (FileSystemEntity.typeSync(path, followLinks: false) !=
      FileSystemEntityType.file) {
    throw FileSystemException('source is not a regular file', path);
  }
  final file = File(path).openSync();
  try {
    final sink = _DigestSink();
    final input = crypto.sha256.startChunkedConversion(sink);
    final buffer = Uint8List(1024 * 1024);
    while (true) {
      final count = file.readIntoSync(buffer);
      if (count == 0) {
        break;
      }
      input.add(Uint8List.sublistView(buffer, 0, count));
    }
    input.close();
    return sink.digest!.toString();
  } finally {
    file.closeSync();
  }
}

class _DigestSink implements Sink<crypto.Digest> {
  crypto.Digest? digest;

  @override
  void add(crypto.Digest data) => digest = data;

  @override
  void close() {}
}

void _collectMissingDirectories(String stop, String leaf, Set<String> result) {
  var current = leaf;
  while (!p.equals(current, stop)) {
    if (!_startsWith(current, stop)) {
      throw BackendError.pathEscape();
    }
    if (_pathExists(current)) {
      break;
    }
    result.add(current);
    final parent = p.dirname(current);
    if (parent == current) {
      throw BackendError.pathEscape();
    }
    current = parent;
  }
}

bool _pathExists(String path) =>
    FileSystemEntity.typeSync(path) != FileSystemEntityType.notFound;

int _saturatingAdd(int a, int b) {
  const maxValue = 0x7FFFFFFFFFFFFFFF;
  return a > maxValue - b ? maxValue : a + b;
}

/// Compares two relative or absolute paths component by component, as Rust's
/// `Path` `Ord` does. Components compare by code point.
int _compareComponents(String a, String b) {
  final left = _components(a);
  final right = _components(b);
  final shared = left.length < right.length ? left.length : right.length;
  for (var index = 0; index < shared; index++) {
    final comparison = _compareUtf8(left[index], right[index]);
    if (comparison != 0) {
      return comparison;
    }
  }
  return left.length.compareTo(right.length);
}

List<String> _components(String path) => [
  for (final part in p.split(path))
    if (part.isNotEmpty && part != '.') part,
];

/// Code-point string comparison, matching Rust `String` / `OsStr` ordering.
int _compareUtf8(String a, String b) {
  final left = a.runes.iterator;
  final right = b.runes.iterator;
  while (true) {
    final hasLeft = left.moveNext();
    final hasRight = right.moveNext();
    if (!hasLeft || !hasRight) {
      return (hasLeft ? 1 : 0) - (hasRight ? 1 : 0);
    }
    final comparison = left.current.compareTo(right.current);
    if (comparison != 0) {
      return comparison;
    }
  }
}
