/// Port of src-tauri/src/journal.rs.
///
library;

/// Journals are the durable record of every effect. Wire JSON is exact to the
/// Rust serde shape (camelCase keys, snake_case enum values, omitted optional
/// and empty fields, struct field order). Only a validated local journal may
/// authorize undo.
///
/// Notes on the Dart port:
/// * Dart has no `Drop`; permits are released with explicit `release()` calls.
/// * Anchored (`openat`-style) filesystem access belongs to the capability
///   layer (`cap_fs`). This module reaches it through the narrow ports declared
///   below ([CapabilityRootPort], [AuthorizedPathPort], [AnchoredPathPort]).
/// * Serde's `BTreeMap` ordering of `serde_json::Map` is reproduced for the
///   `meta` objects: keys are emitted sorted, at every depth.
/// * Platform calls that Dart does not expose (`flock`, `fsync` on a directory,
///   `link`, `stat` inode numbers) use `dart:ffi` against libc on POSIX.
///   Windows paths are marked `// UNVERIFIED:`.
import 'dart:collection';
import 'dart:convert';
import 'dart:ffi';
import 'dart:io';
import 'dart:typed_data';

import 'package:crypto/crypto.dart' as crypto;
import 'package:ffi/ffi.dart';
import 'package:path/path.dart' as p;
import 'package:uuid/uuid.dart';

import 'contracts.dart';
import 'errors.dart';
import 'paths.dart' show CallerIdentity;
import 'plan_store.dart' show ClaimedS5Apply, PlanBinding;
import 'progress.dart' show GlobalMutationPermit;

const int maxJournalBytes = 8 * 1024 * 1024;
const int maxJournalSteps = 100000;
const int maxCleanupReviewEvidencePerJob = 16;
const int maxRetainedArtifactsPerJournal = 8;
const int maxRecoveryLocatorScalars = 512;
const int maxS5RetainedFiles = 256;
const int maxS5RetainedBytes = 8 * 1024 * 1024 * 1024;
const int maxJournalRootEntriesScanned = 4096;
const int maxPendingS5Records = 256;
const int maxInvalidS5Notices = 64;
const int maxInvalidS5NoticeBytes = 4096;
const int maxRecoveryEntries = 256;

const Uuid _uuid = Uuid();

// ---------------------------------------------------------------------------
// Shared helpers (exported for the other engine modules)
// ---------------------------------------------------------------------------

/// Lower-case hex SHA-256 of [bytes].
String sha256Hex(List<int> bytes) => crypto.sha256.convert(bytes).toString();

/// Returns true when both lists hold the same strings in the same order.
bool sameStrings(List<String> left, List<String> right) {
  if (left.length != right.length) {
    return false;
  }
  for (var index = 0; index < left.length; index++) {
    if (left[index] != right[index]) {
      return false;
    }
  }
  return true;
}

/// Serde's `serde_json::Map` is a `BTreeMap` (no `preserve_order` feature), so
/// every JSON object is emitted with sorted keys at every depth.
Object? sortedJsonValue(Object? value) {
  if (value is Map) {
    final keys = value.keys.map((key) => key.toString()).toList()..sort();
    return <String, Object?>{
      for (final key in keys) key: sortedJsonValue(value[key]),
    };
  }
  if (value is List) {
    return [for (final item in value) sortedJsonValue(item)];
  }
  return value;
}

/// Sorts only the `meta` objects (the `JsonObject` fields); every other struct
/// keeps its declaration order, as in the Rust serializer.
Object? normalizeMetaJson(Object? value) {
  if (value is Map) {
    return <String, Object?>{
      for (final entry in value.entries)
        entry.key.toString(): entry.key == 'meta' && entry.value is Map
            ? sortedJsonValue(entry.value)
            : normalizeMetaJson(entry.value),
    };
  }
  if (value is List) {
    return [for (final item in value) normalizeMetaJson(item)];
  }
  return value;
}

/// Bytes hashed for plan digests: the struct-order JSON with `meta` sorted.
List<int> digestJsonBytes(Object? value) =>
    utf8.encode(jsonEncode(normalizeMetaJson(value)));

/// Fully key-sorted compact JSON bytes (`serde_json::to_vec` of a `Value`).
List<int> canonicalJsonBytes(Object? value) =>
    utf8.encode(jsonEncode(sortedJsonValue(value)));

/// Port of the journal's `valid_sha256`: exactly 64 lower-case hex digits.
bool validSha256(String value) =>
    value.length == 64 &&
    value.codeUnits.every(
      (unit) =>
          (unit >= 0x30 && unit <= 0x39) || (unit >= 0x61 && unit <= 0x66),
    );

/// Byte length of a string as Rust `str::len` reports it.
int _byteLength(String value) => utf8.encode(value).length;

bool _hasControl(String value) => value.runes.any(
  (rune) => rune < 0x20 || (rune >= 0x7f && rune <= 0x9f),
);

/// Overflow-checked addition for non-negative sizes (`u64::checked_add`).
int? _checkedAdd(int left, int right) {
  if (left < 0 || right < 0) {
    return null;
  }
  if (left > 0x7fffffffffffffff - right) {
    return null;
  }
  return left + right;
}

// ---------------------------------------------------------------------------
// Errors that come from filesystem ports
// ---------------------------------------------------------------------------

/// Kinds of `std::io::ErrorKind` the undo logic branches on.
enum FsErrorKind {
  notFound,
  alreadyExists,
  permissionDenied,
  directoryNotEmpty,
  invalidData,
  invalidInput,
  unsupported,
  other,
}

/// The Dart counterpart of `io::Error`.
class FsIoException implements Exception {
  const FsIoException(this.kind, this.message);

  final FsErrorKind kind;
  final String message;

  @override
  String toString() => message;
}

BackendError journalIo(String context, Object error) =>
    BackendError.journalInvalid('$context: $error');

// ---------------------------------------------------------------------------
// Capability ports (implemented by the cap_fs port)
// ---------------------------------------------------------------------------

/// A pinned grant-root directory capability (`cap_fs::CapabilityRoot`).
abstract interface class CapabilityRootPort {
  /// The canonical absolute path of the root.
  String get canonical;

  /// Object identity of the pinned root directory.
  FilesystemObjectIdentity identity();

  /// Resolves [absolutePath] under this root. Throws [FsIoException] or
  /// [BackendError] when the path is outside the root.
  AuthorizedPathPort authorize(String absolutePath);
}

/// A path that was authorized by a root but not yet anchored.
abstract interface class AuthorizedPathPort {
  AnchoredPathPort anchor();
}

/// A path anchored to its pinned parent directory (`cap_fs::AnchoredPath`).
abstract interface class AnchoredPathPort {
  String get absolute;

  /// SHA-256 of the file contents, lower-case hex.
  String sha256();

  /// Whether the path can be opened for reading.
  bool canOpenRead();

  void removeFile();

  void removeFileDurable();

  void removeDirDurable();

  /// Removes the file only if its SHA-256 equals [expectedSha256].
  void removeVerified(String expectedSha256);

  /// Moves this file to [destination] without replacing an existing entry.
  void moveNoClobberTo(AnchoredPathPort destination);

  /// Copies [source] to this (new) path without replacing an existing entry.
  void copyNoClobberFrom(AnchoredPathPort source);

  /// Atomically replaces this file's content with [source]'s.
  void replaceWith(AnchoredPathPort source);

  /// Overwrites this file from [source] and verifies the applied hash.
  void overwriteVerifiedFrom(AnchoredPathPort source, String appliedSha256);

  /// A not-yet-existing sibling path with the given [label] in its name.
  AnchoredPathPort temporarySibling(String label);
}

/// The validated grant-root roster of one caller (`ValidatedGrantRootRoster`).
abstract interface class GrantRootRoster {
  List<GrantRootAccessRecord> get records;

  List<CapabilityRootPort> get roots;

  /// True when the roster was built from exactly these grant ids.
  bool matchesGrants(List<String> grantIds);
}

/// The second permit of an S5 mutation (`InterprocessMutationPermit`).
class InterprocessMutationPermit {
  InterprocessMutationPermit._(this._release);

  final void Function() _release;
  bool _released = false;

  void release() {
    if (_released) {
      return;
    }
    _released = true;
    _release();
  }
}

/// Process permit plus interprocess permit held together for an S5 mutation.
class S5PermitBundle {
  S5PermitBundle(this._process, this._interprocess);

  final GlobalMutationPermit _process;
  final InterprocessMutationPermit _interprocess;

  void release() {
    _interprocess.release();
    _process.release();
  }
}

// ---------------------------------------------------------------------------
// Wire enums
// ---------------------------------------------------------------------------

T _enumFromWire<T extends Enum>(
  List<T> values,
  String Function(T) wireOf,
  Object? wire,
) {
  for (final value in values) {
    if (wireOf(value) == wire) {
      return value;
    }
  }
  throw FormatException('unknown wire value: $wire');
}

enum JournalState {
  applying('applying'),
  committed('committed'),
  partialCommitted('partial_committed'),
  noEffectRetryable('no_effect_retryable'),
  cancelledClean('cancelled_clean'),
  cancelledPartial('cancelled_partial'),
  recoveryRequired('recovery_required'),
  undone('undone'),
  undoPartial('undo_partial');

  const JournalState(this.wire);

  final String wire;

  static JournalState fromWire(Object? wire) =>
      _enumFromWire(values, (value) => value.wire, wire);
}

enum PersistedRootAccess {
  readOnly('read_only'),
  readWrite('read_write');

  const PersistedRootAccess(this.wire);

  final String wire;

  static PersistedRootAccess fromWire(Object? wire) =>
      _enumFromWire(values, (value) => value.wire, wire);
}

enum RetainedPublicationPurpose {
  apply('apply'),
  undo('undo');

  const RetainedPublicationPurpose(this.wire);

  final String wire;

  static RetainedPublicationPurpose fromWire(Object? wire) =>
      _enumFromWire(values, (value) => value.wire, wire);
}

enum RetainedPublicationPhase {
  prepared('prepared'),
  displacing('displacing'),
  retained('retained'),
  creatingCanonical('creating_canonical'),
  canonicalSecured('canonical_secured'),
  canonicalAuxiliaryReady('canonical_auxiliary_ready'),
  writingCanonical('writing_canonical'),
  verifyingCanonical('verifying_canonical'),
  applied('applied'),
  restoringOriginal('restoring_original'),
  recoveryRequired('recovery_required');

  const RetainedPublicationPhase(this.wire);

  final String wire;

  static RetainedPublicationPhase fromWire(Object? wire) =>
      _enumFromWire(values, (value) => value.wire, wire);
}

enum RetainedArtifactRole {
  backup('backup', 'backup'),
  privateOutput('private_output', 'privateOutput'),
  displacedOriginal('displaced_original', 'displacedOriginal'),
  displacedApplied('displaced_applied', 'displacedApplied');

  const RetainedArtifactRole(this.wire, this.projectionKey);

  final String wire;
  final String projectionKey;

  static RetainedArtifactRole fromWire(Object? wire) =>
      _enumFromWire(values, (value) => value.wire, wire);
}

enum FollowUpJournalPhase {
  artifactParentReady('artifact_parent_ready'),
  privateOutputReady('private_output_ready'),
  backupReady('backup_ready'),
  publicationActive('publication_active');

  const FollowUpJournalPhase(this.wire);

  final String wire;

  static FollowUpJournalPhase fromWire(Object? wire) =>
      _enumFromWire(values, (value) => value.wire, wire);
}

enum UndoRecoveryPhase {
  prepared('prepared'),
  quarantined('quarantined'),
  restoring('restoring'),
  publishing('publishing'),
  deleting('deleting');

  const UndoRecoveryPhase(this.wire);

  final String wire;

  static UndoRecoveryPhase fromWire(Object? wire) =>
      _enumFromWire(values, (value) => value.wire, wire);
}

enum RecoveryDisposition {
  noEffect('no_effect'),
  outcomeUnknown('outcome_unknown');

  const RecoveryDisposition(this.wire);

  final String wire;
}

// ---------------------------------------------------------------------------
// Object identity
// ---------------------------------------------------------------------------

/// `FilesystemObjectIdentity`: serde internally tagged on `kind`.
sealed class FilesystemObjectIdentity {
  const FilesystemObjectIdentity();

  Map<String, Object?> toJson();

  static FilesystemObjectIdentity fromJson(Object? value) {
    if (value is! Map) {
      throw const FormatException('identity must be an object');
    }
    switch (value['kind']) {
      case 'unix':
        return UnixObjectIdentity(
          device: _intField(value, 'device'),
          inode: _intField(value, 'inode'),
        );
      case 'windows':
        return WindowsObjectIdentity(
          volumeSerial: _intField(value, 'volume_serial'),
          fileIndex: _intField(value, 'file_index'),
        );
      default:
        throw FormatException('unknown identity kind: ${value['kind']}');
    }
  }
}

class UnixObjectIdentity extends FilesystemObjectIdentity {
  const UnixObjectIdentity({required this.device, required this.inode});

  final int device;
  final int inode;

  @override
  Map<String, Object?> toJson() => {
    'kind': 'unix',
    'device': device,
    'inode': inode,
  };

  @override
  bool operator ==(Object other) =>
      other is UnixObjectIdentity &&
      other.device == device &&
      other.inode == inode;

  @override
  int get hashCode => Object.hash('unix', device, inode);

  @override
  String toString() => 'UnixObjectIdentity($device:$inode)';
}

class WindowsObjectIdentity extends FilesystemObjectIdentity {
  const WindowsObjectIdentity({
    required this.volumeSerial,
    required this.fileIndex,
  });

  final int volumeSerial;
  final int fileIndex;

  @override
  Map<String, Object?> toJson() => {
    'kind': 'windows',
    'volume_serial': volumeSerial,
    'file_index': fileIndex,
  };

  @override
  bool operator ==(Object other) =>
      other is WindowsObjectIdentity &&
      other.volumeSerial == volumeSerial &&
      other.fileIndex == fileIndex;

  @override
  int get hashCode => Object.hash('windows', volumeSerial, fileIndex);

  @override
  String toString() => 'WindowsObjectIdentity($volumeSerial:$fileIndex)';
}

int _intField(Map<dynamic, dynamic> json, String key) {
  final value = json[key];
  if (value is! int) {
    throw FormatException('$key must be an integer');
  }
  return value;
}

String _stringField(Map<dynamic, dynamic> json, String key) {
  final value = json[key];
  if (value is! String) {
    throw FormatException('$key must be a string');
  }
  return value;
}

String? _optionalString(Map<dynamic, dynamic> json, String key) {
  final value = json[key];
  if (value == null) {
    return null;
  }
  if (value is! String) {
    throw FormatException('$key must be a string');
  }
  return value;
}

int? _optionalInt(Map<dynamic, dynamic> json, String key) {
  final value = json[key];
  if (value == null) {
    return null;
  }
  if (value is! int) {
    throw FormatException('$key must be an integer');
  }
  return value;
}

int _intOrZero(Map<dynamic, dynamic> json, String key) =>
    _optionalInt(json, key) ?? 0;

JsonObject _objectField(Map<dynamic, dynamic> json, String key) {
  final value = json[key];
  if (value is! Map) {
    throw FormatException('$key must be an object');
  }
  return value.cast<String, Object?>();
}

/// Casts one element of a wire array to a JSON object.
JsonObject _asJsonObject(Object? value) {
  if (value is! Map) {
    throw const FormatException('expected an object');
  }
  return value.cast<String, Object?>();
}

FilesystemObjectIdentity? _optionalIdentity(
  Map<dynamic, dynamic> json,
  String key,
) {
  final value = json[key];
  return value == null ? null : FilesystemObjectIdentity.fromJson(value);
}

/// Reads `kind` of an identity as serde does (unknown fields are ignored).
void _denyUnknownFields(Map<dynamic, dynamic> json, Set<String> allowed) {
  for (final key in json.keys) {
    if (!allowed.contains(key)) {
      throw FormatException('unknown field: $key');
    }
  }
}

/// Reads a path identity (device and inode on POSIX, volume serial and file
/// index on Windows) without following the caller's handle semantics.
///
/// This is the Dart counterpart of `cap_fs::file_identity` for paths.
FilesystemObjectIdentity readFilesystemObjectIdentity(String path) =>
    _ObjectIdentityReader.read(path);

// ---------------------------------------------------------------------------
// Grant roots, S5 authority and the publication state machine
// ---------------------------------------------------------------------------

class GrantRootIdentity {
  const GrantRootIdentity({required this.root, required this.identity});

  final String root;
  final FilesystemObjectIdentity identity;

  Map<String, Object?> toJson() => {
    'root': root,
    'identity': identity.toJson(),
  };

  factory GrantRootIdentity.fromJson(Map<dynamic, dynamic> json) =>
      GrantRootIdentity(
        root: _stringField(json, 'root'),
        identity: FilesystemObjectIdentity.fromJson(json['identity']),
      );

  @override
  bool operator ==(Object other) =>
      other is GrantRootIdentity &&
      other.root == root &&
      other.identity == identity;

  @override
  int get hashCode => Object.hash(root, identity);
}

class GrantRootAccessRecord {
  const GrantRootAccessRecord({
    required this.root,
    required this.identity,
    required this.access,
  });

  final String root;
  final FilesystemObjectIdentity identity;
  final PersistedRootAccess access;

  Map<String, Object?> toJson() => {
    'root': root,
    'identity': identity.toJson(),
    'access': access.wire,
  };

  factory GrantRootAccessRecord.fromJson(Map<dynamic, dynamic> json) {
    _denyUnknownFields(json, {'root', 'identity', 'access'});
    return GrantRootAccessRecord(
      root: _stringField(json, 'root'),
      identity: FilesystemObjectIdentity.fromJson(json['identity']),
      access: PersistedRootAccess.fromWire(json['access']),
    );
  }

  @override
  bool operator ==(Object other) =>
      other is GrantRootAccessRecord &&
      other.root == root &&
      other.identity == identity &&
      other.access == access;

  @override
  int get hashCode => Object.hash(root, identity, access);

  @override
  String toString() => 'GrantRootAccessRecord($root, $access)';
}

class S5AuthorityRecord {
  const S5AuthorityRecord({
    required this.initialStep,
    required this.sourcePath,
    required this.sourceIdentity,
    required this.targetPath,
    required this.targetIdentity,
    required this.approvedUpdatesSha256,
  });

  final Step initialStep;
  final String sourcePath;
  final FilesystemObjectIdentity sourceIdentity;
  final String targetPath;
  final FilesystemObjectIdentity targetIdentity;
  final String approvedUpdatesSha256;

  Map<String, Object?> toJson() => {
    'initialStep': initialStep.toJson(),
    'sourcePath': sourcePath,
    'sourceIdentity': sourceIdentity.toJson(),
    'targetPath': targetPath,
    'targetIdentity': targetIdentity.toJson(),
    'approvedUpdatesSha256': approvedUpdatesSha256,
  };

  factory S5AuthorityRecord.fromJson(Map<dynamic, dynamic> json) {
    _denyUnknownFields(json, {
      'initialStep',
      'sourcePath',
      'sourceIdentity',
      'targetPath',
      'targetIdentity',
      'approvedUpdatesSha256',
    });
    return S5AuthorityRecord(
      initialStep: Step.fromJson(_objectField(json, 'initialStep')),
      sourcePath: _stringField(json, 'sourcePath'),
      sourceIdentity: FilesystemObjectIdentity.fromJson(json['sourceIdentity']),
      targetPath: _stringField(json, 'targetPath'),
      targetIdentity: FilesystemObjectIdentity.fromJson(json['targetIdentity']),
      approvedUpdatesSha256: _stringField(json, 'approvedUpdatesSha256'),
    );
  }
}

class PendingArtifactParent {
  const PendingArtifactParent({required this.path, required this.locator});

  final String path;
  final String locator;

  Map<String, Object?> toJson() => {'path': path, 'locator': locator};

  factory PendingArtifactParent.fromJson(Map<dynamic, dynamic> json) {
    _denyUnknownFields(json, {'path', 'locator'});
    return PendingArtifactParent(
      path: _stringField(json, 'path'),
      locator: _stringField(json, 'locator'),
    );
  }
}

class CreatedArtifactParent {
  const CreatedArtifactParent({
    required this.path,
    required this.locator,
    required this.identity,
  });

  final String path;
  final String locator;
  final FilesystemObjectIdentity identity;

  Map<String, Object?> toJson() => {
    'path': path,
    'locator': locator,
    'identity': identity.toJson(),
  };

  factory CreatedArtifactParent.fromJson(Map<dynamic, dynamic> json) {
    _denyUnknownFields(json, {'path', 'locator', 'identity'});
    return CreatedArtifactParent(
      path: _stringField(json, 'path'),
      locator: _stringField(json, 'locator'),
      identity: FilesystemObjectIdentity.fromJson(json['identity']),
    );
  }
}

/// `PendingArtifactCreationState` in serde's externally tagged form: the
/// strings `"intent"` / `"created_identity_unknown"`, or `{"created": identity}`.
sealed class PendingArtifactCreationState {
  const PendingArtifactCreationState();

  Object toJson();

  static PendingArtifactCreationState fromJson(Object? value) {
    if (value == 'intent') {
      return const PendingArtifactCreatedIntent();
    }
    if (value == 'created_identity_unknown') {
      return const PendingArtifactCreatedIdentityUnknown();
    }
    if (value is Map && value.length == 1 && value.containsKey('created')) {
      return PendingArtifactCreated(
        FilesystemObjectIdentity.fromJson(value['created']),
      );
    }
    throw const FormatException('invalid creation state');
  }
}

class PendingArtifactCreatedIntent extends PendingArtifactCreationState {
  const PendingArtifactCreatedIntent();

  @override
  Object toJson() => 'intent';

  @override
  bool operator ==(Object other) => other is PendingArtifactCreatedIntent;

  @override
  int get hashCode => 1;
}

class PendingArtifactCreated extends PendingArtifactCreationState {
  const PendingArtifactCreated(this.identity);

  final FilesystemObjectIdentity identity;

  @override
  Object toJson() => {'created': identity.toJson()};

  @override
  bool operator ==(Object other) =>
      other is PendingArtifactCreated && other.identity == identity;

  @override
  int get hashCode => identity.hashCode;
}

class PendingArtifactCreatedIdentityUnknown extends PendingArtifactCreationState {
  const PendingArtifactCreatedIdentityUnknown();

  @override
  Object toJson() => 'created_identity_unknown';

  @override
  bool operator ==(Object other) =>
      other is PendingArtifactCreatedIdentityUnknown;

  @override
  int get hashCode => 2;
}

class PendingRetainedArtifact {
  const PendingRetainedArtifact({
    required this.role,
    required this.path,
    required this.locator,
    required this.creationState,
  });

  final RetainedArtifactRole role;
  final String path;
  final String locator;
  final PendingArtifactCreationState creationState;

  Map<String, Object?> toJson() => {
    'role': role.wire,
    'path': path,
    'locator': locator,
    'creationState': creationState.toJson(),
  };

  factory PendingRetainedArtifact.fromJson(Map<dynamic, dynamic> json) {
    _denyUnknownFields(json, {'role', 'path', 'locator', 'creationState'});
    return PendingRetainedArtifact(
      role: RetainedArtifactRole.fromWire(json['role']),
      path: _stringField(json, 'path'),
      locator: _stringField(json, 'locator'),
      creationState: PendingArtifactCreationState.fromJson(json['creationState']),
    );
  }

  @override
  bool operator ==(Object other) =>
      other is PendingRetainedArtifact &&
      other.role == role &&
      other.path == path &&
      other.locator == locator &&
      other.creationState == creationState;

  @override
  int get hashCode => Object.hash(role, path, locator, creationState);
}

class RetainedArtifactEvidence {
  const RetainedArtifactEvidence({
    required this.role,
    required this.path,
    required this.identity,
    required this.sha256,
    required this.securitySha256,
    required this.defaultStreamBytes,
    required this.auxiliaryChargeBytes,
    required this.logicalChargeBytes,
    required this.locator,
  });

  final RetainedArtifactRole role;
  final String path;
  final FilesystemObjectIdentity identity;
  final String sha256;
  final String securitySha256;
  final int defaultStreamBytes;
  final int auxiliaryChargeBytes;
  final int logicalChargeBytes;
  final String locator;

  Map<String, Object?> toJson() => {
    'role': role.wire,
    'path': path,
    'identity': identity.toJson(),
    'sha256': sha256,
    'securitySha256': securitySha256,
    'defaultStreamBytes': defaultStreamBytes,
    'auxiliaryChargeBytes': auxiliaryChargeBytes,
    'logicalChargeBytes': logicalChargeBytes,
    'locator': locator,
  };

  factory RetainedArtifactEvidence.fromJson(Map<dynamic, dynamic> json) {
    _denyUnknownFields(json, {
      'role',
      'path',
      'identity',
      'sha256',
      'securitySha256',
      'defaultStreamBytes',
      'auxiliaryChargeBytes',
      'logicalChargeBytes',
      'locator',
    });
    return RetainedArtifactEvidence(
      role: RetainedArtifactRole.fromWire(json['role']),
      path: _stringField(json, 'path'),
      identity: FilesystemObjectIdentity.fromJson(json['identity']),
      sha256: _stringField(json, 'sha256'),
      securitySha256: _stringField(json, 'securitySha256'),
      defaultStreamBytes: _intField(json, 'defaultStreamBytes'),
      auxiliaryChargeBytes: _intField(json, 'auxiliaryChargeBytes'),
      logicalChargeBytes: _intField(json, 'logicalChargeBytes'),
      locator: _stringField(json, 'locator'),
    );
  }

  @override
  bool operator ==(Object other) =>
      other is RetainedArtifactEvidence &&
      other.role == role &&
      other.path == path &&
      other.identity == identity &&
      other.sha256 == sha256 &&
      other.securitySha256 == securitySha256 &&
      other.defaultStreamBytes == defaultStreamBytes &&
      other.auxiliaryChargeBytes == auxiliaryChargeBytes &&
      other.logicalChargeBytes == logicalChargeBytes &&
      other.locator == locator;

  @override
  int get hashCode => Object.hash(
    role,
    path,
    identity,
    sha256,
    securitySha256,
    defaultStreamBytes,
    auxiliaryChargeBytes,
    logicalChargeBytes,
    locator,
  );
}

class RetainedPublication {
  RetainedPublication({
    required this.purpose,
    required this.canonical,
    required this.retained,
    required this.backup,
    required this.publicationSource,
    required this.expectedCanonicalIdentity,
    required this.expectedCanonicalSha256,
    required this.expectedBackupIdentity,
    required this.expectedBackupSha256,
    required this.publicationSourceIdentity,
    required this.publicationSha256,
    required this.expectedCanonicalSecuritySha256,
    required this.expectedBackupSecuritySha256,
    required this.publicationSecuritySha256,
    required this.expectedCanonicalDefaultStreamBytes,
    required this.expectedCanonicalAuxiliaryChargeBytes,
    required this.expectedCanonicalLogicalChargeBytes,
    required this.destinationAccessSha256,
    required this.destinationAuxiliarySha256,
    required this.destinationAuxiliaryChargeBytes,
    required this.phase,
    this.retainedIdentity,
    this.retainedSha256,
    this.retainedSecuritySha256,
    this.createdCanonicalIdentity,
  });

  final RetainedPublicationPurpose purpose;
  final String canonical;
  final String retained;
  final String backup;
  final String publicationSource;
  final FilesystemObjectIdentity expectedCanonicalIdentity;
  final String expectedCanonicalSha256;
  final FilesystemObjectIdentity expectedBackupIdentity;
  final String expectedBackupSha256;
  final FilesystemObjectIdentity publicationSourceIdentity;
  final String publicationSha256;
  final String expectedCanonicalSecuritySha256;
  final String expectedBackupSecuritySha256;
  final String publicationSecuritySha256;
  final int expectedCanonicalDefaultStreamBytes;
  final int expectedCanonicalAuxiliaryChargeBytes;
  final int expectedCanonicalLogicalChargeBytes;
  final String destinationAccessSha256;
  final String destinationAuxiliarySha256;
  final int destinationAuxiliaryChargeBytes;
  FilesystemObjectIdentity? retainedIdentity;
  String? retainedSha256;
  String? retainedSecuritySha256;
  FilesystemObjectIdentity? createdCanonicalIdentity;
  RetainedPublicationPhase phase;

  Map<String, Object?> toJson() => {
    'purpose': purpose.wire,
    'canonical': canonical,
    'retained': retained,
    'backup': backup,
    'publicationSource': publicationSource,
    'expectedCanonicalIdentity': expectedCanonicalIdentity.toJson(),
    'expectedCanonicalSha256': expectedCanonicalSha256,
    'expectedBackupIdentity': expectedBackupIdentity.toJson(),
    'expectedBackupSha256': expectedBackupSha256,
    'publicationSourceIdentity': publicationSourceIdentity.toJson(),
    'publicationSha256': publicationSha256,
    'expectedCanonicalSecuritySha256': expectedCanonicalSecuritySha256,
    'expectedBackupSecuritySha256': expectedBackupSecuritySha256,
    'publicationSecuritySha256': publicationSecuritySha256,
    'expectedCanonicalDefaultStreamBytes': expectedCanonicalDefaultStreamBytes,
    'expectedCanonicalAuxiliaryChargeBytes':
        expectedCanonicalAuxiliaryChargeBytes,
    'expectedCanonicalLogicalChargeBytes': expectedCanonicalLogicalChargeBytes,
    'destinationAccessSha256': destinationAccessSha256,
    'destinationAuxiliarySha256': destinationAuxiliarySha256,
    'destinationAuxiliaryChargeBytes': destinationAuxiliaryChargeBytes,
    'retainedIdentity': retainedIdentity?.toJson(),
    'retainedSha256': retainedSha256,
    'retainedSecuritySha256': retainedSecuritySha256,
    'createdCanonicalIdentity': createdCanonicalIdentity?.toJson(),
    'phase': phase.wire,
  };

  factory RetainedPublication.fromJson(Map<dynamic, dynamic> json) {
    _denyUnknownFields(json, {
      'purpose',
      'canonical',
      'retained',
      'backup',
      'publicationSource',
      'expectedCanonicalIdentity',
      'expectedCanonicalSha256',
      'expectedBackupIdentity',
      'expectedBackupSha256',
      'publicationSourceIdentity',
      'publicationSha256',
      'expectedCanonicalSecuritySha256',
      'expectedBackupSecuritySha256',
      'publicationSecuritySha256',
      'expectedCanonicalDefaultStreamBytes',
      'expectedCanonicalAuxiliaryChargeBytes',
      'expectedCanonicalLogicalChargeBytes',
      'destinationAccessSha256',
      'destinationAuxiliarySha256',
      'destinationAuxiliaryChargeBytes',
      'retainedIdentity',
      'retainedSha256',
      'retainedSecuritySha256',
      'createdCanonicalIdentity',
      'phase',
    });
    return RetainedPublication(
      purpose: RetainedPublicationPurpose.fromWire(json['purpose']),
      canonical: _stringField(json, 'canonical'),
      retained: _stringField(json, 'retained'),
      backup: _stringField(json, 'backup'),
      publicationSource: _stringField(json, 'publicationSource'),
      expectedCanonicalIdentity: FilesystemObjectIdentity.fromJson(
        json['expectedCanonicalIdentity'],
      ),
      expectedCanonicalSha256: _stringField(json, 'expectedCanonicalSha256'),
      expectedBackupIdentity: FilesystemObjectIdentity.fromJson(
        json['expectedBackupIdentity'],
      ),
      expectedBackupSha256: _stringField(json, 'expectedBackupSha256'),
      publicationSourceIdentity: FilesystemObjectIdentity.fromJson(
        json['publicationSourceIdentity'],
      ),
      publicationSha256: _stringField(json, 'publicationSha256'),
      expectedCanonicalSecuritySha256: _stringField(
        json,
        'expectedCanonicalSecuritySha256',
      ),
      expectedBackupSecuritySha256: _stringField(
        json,
        'expectedBackupSecuritySha256',
      ),
      publicationSecuritySha256: _stringField(json, 'publicationSecuritySha256'),
      expectedCanonicalDefaultStreamBytes: _intField(
        json,
        'expectedCanonicalDefaultStreamBytes',
      ),
      expectedCanonicalAuxiliaryChargeBytes: _intField(
        json,
        'expectedCanonicalAuxiliaryChargeBytes',
      ),
      expectedCanonicalLogicalChargeBytes: _intField(
        json,
        'expectedCanonicalLogicalChargeBytes',
      ),
      destinationAccessSha256: _stringField(json, 'destinationAccessSha256'),
      destinationAuxiliarySha256: _stringField(
        json,
        'destinationAuxiliarySha256',
      ),
      destinationAuxiliaryChargeBytes: _intField(
        json,
        'destinationAuxiliaryChargeBytes',
      ),
      retainedIdentity: _optionalIdentity(json, 'retainedIdentity'),
      retainedSha256: _optionalString(json, 'retainedSha256'),
      retainedSecuritySha256: _optionalString(json, 'retainedSecuritySha256'),
      createdCanonicalIdentity: _optionalIdentity(
        json,
        'createdCanonicalIdentity',
      ),
      phase: RetainedPublicationPhase.fromWire(json['phase']),
    );
  }
}

class RecoveryNotice {
  const RecoveryNotice({required this.warningKey, required this.locator});

  final String warningKey;
  final String locator;

  Map<String, Object?> toJson() => {
    'warningKey': warningKey,
    'locator': locator,
  };

  factory RecoveryNotice.fromJson(Map<dynamic, dynamic> json) {
    _denyUnknownFields(json, {'warningKey', 'locator'});
    return RecoveryNotice(
      warningKey: _stringField(json, 'warningKey'),
      locator: _stringField(json, 'locator'),
    );
  }

  /// Port of `RecoveryNotice::from_warning`.
  factory RecoveryNotice.fromWarning(Warning warning) =>
      RecoveryNotice(warningKey: warning.key, locator: warning.subject);

  @override
  bool operator ==(Object other) =>
      other is RecoveryNotice &&
      other.warningKey == warningKey &&
      other.locator == locator;

  @override
  int get hashCode => Object.hash(warningKey, locator);
}

class UndoRecovery {
  const UndoRecovery({
    required this.index,
    required this.original,
    required this.quarantine,
    required this.phase,
    this.target,
  });

  final int index;
  final String original;
  final String quarantine;
  final String? target;
  final UndoRecoveryPhase phase;

  Map<String, Object?> toJson() => {
    'index': index,
    'original': original,
    'quarantine': quarantine,
    if (target != null) 'target': target,
    'phase': phase.wire,
  };

  factory UndoRecovery.fromJson(Map<dynamic, dynamic> json) => UndoRecovery(
    index: _intField(json, 'index'),
    original: _stringField(json, 'original'),
    quarantine: _stringField(json, 'quarantine'),
    target: _optionalString(json, 'target'),
    phase: UndoRecoveryPhase.fromWire(json['phase']),
  );

  UndoRecovery withPhase(UndoRecoveryPhase next) => UndoRecovery(
    index: index,
    original: original,
    quarantine: quarantine,
    target: target,
    phase: next,
  );

  @override
  bool operator ==(Object other) =>
      other is UndoRecovery &&
      other.index == index &&
      other.original == original &&
      other.quarantine == quarantine &&
      other.target == target &&
      other.phase == phase;

  @override
  int get hashCode => Object.hash(index, original, quarantine, target, phase);
}

class CleanupReviewEntry {
  const CleanupReviewEntry({required this.warningKey, required this.locator});

  final String warningKey;
  final String locator;

  Map<String, Object?> toJson() => {
    'warningKey': warningKey,
    'locator': locator,
  };

  factory CleanupReviewEntry.fromJson(Map<dynamic, dynamic> json) =>
      CleanupReviewEntry(
        warningKey: _stringField(json, 'warningKey'),
        locator: _stringField(json, 'locator'),
      );

  @override
  bool operator ==(Object other) =>
      other is CleanupReviewEntry &&
      other.warningKey == warningKey &&
      other.locator == locator;

  @override
  int get hashCode => Object.hash(warningKey, locator);
}

/// `CleanupReviewEvidence`: serialized as one object when it holds a single
/// entry and as an array otherwise. Deserialization dedupes by locator.
class CleanupReviewEvidence {
  CleanupReviewEvidence._(this._entries);

  /// Port of `CleanupReviewEvidence::default()`.
  CleanupReviewEvidence.empty() : _entries = <CleanupReviewEntry>[];

  /// Port of `CleanupReviewEvidence::single`.
  factory CleanupReviewEvidence.single(String warningKey, String locator) =>
      CleanupReviewEvidence._([
        CleanupReviewEntry(warningKey: warningKey, locator: locator),
      ]);

  final List<CleanupReviewEntry> _entries;

  /// Port of `append`: returns false for a duplicate locator, throws when the
  /// per-job limit is reached.
  bool append(String warningKey, String locator) {
    if (_entries.any((entry) => entry.locator == locator)) {
      return false;
    }
    if (_entries.length >= maxCleanupReviewEvidencePerJob) {
      throw StateError('cleanup review evidence limit exceeded');
    }
    _entries.add(CleanupReviewEntry(warningKey: warningKey, locator: locator));
    return true;
  }

  Iterable<CleanupReviewEntry> get entries => _entries;

  /// Port of the `Deref` target: the last entry.
  CleanupReviewEntry get last {
    if (_entries.isEmpty) {
      throw StateError('cleanup-review evidence is never empty');
    }
    return _entries.last;
  }

  Object toJson() {
    if (_entries.length == 1) {
      return _entries.single.toJson();
    }
    return [for (final entry in _entries) entry.toJson()];
  }

  factory CleanupReviewEvidence.fromJson(Object? value) {
    final List<CleanupReviewEntry> entries;
    if (value is Map) {
      entries = [CleanupReviewEntry.fromJson(value)];
    } else if (value is List) {
      entries = [
        for (final item in value) CleanupReviewEntry.fromJson(item as Map),
      ];
    } else {
      throw const FormatException('cleanup review evidence is invalid');
    }
    if (entries.isEmpty) {
      throw const FormatException('cleanup review evidence must not be empty');
    }
    final deduplicated = <CleanupReviewEntry>[];
    for (final entry in entries) {
      if (deduplicated.every((known) => known.locator != entry.locator)) {
        deduplicated.add(entry);
      }
    }
    if (deduplicated.length > maxCleanupReviewEvidencePerJob) {
      throw FormatException(
        'cleanup review evidence limit is $maxCleanupReviewEvidencePerJob '
        'distinct locators',
      );
    }
    return CleanupReviewEvidence._(deduplicated);
  }
}

class JournalCleanupReview {
  const JournalCleanupReview({
    required this.jobId,
    required this.warningKey,
    required this.locator,
  });

  final String jobId;
  final String warningKey;
  final String locator;

  Map<String, Object?> toJson() => {
    'jobId': jobId,
    'warningKey': warningKey,
    'locator': locator,
  };

  factory JournalCleanupReview.fromJson(Map<dynamic, dynamic> json) =>
      JournalCleanupReview(
        jobId: _stringField(json, 'jobId'),
        warningKey: _stringField(json, 'warningKey'),
        locator: _stringField(json, 'locator'),
      );
}

/// Port of `JournalRecord`. Field order and optionality follow the Rust struct;
/// [toJson] omits exactly the fields that `skip_serializing_if` omits.
class JournalRecord {
  JournalRecord({
    required this.journalId,
    required this.opId,
    required this.ts,
    required this.steps,
    this.pendingStep,
    this.state,
    this.ownerId,
    this.sessionId,
    this.planDigest,
    List<String>? grantRoots,
    List<GrantRootIdentity>? grantRootIdentities,
    List<int>? undoneSteps,
    this.pendingUndo,
    this.undoRecovery,
    List<JournalCleanupReview>? cleanupReviews,
    this.s5Authority,
    this.pendingPublication,
    this.pendingArtifactParent,
    this.createdArtifactParent,
    this.pendingRetainedArtifact,
    List<RetainedArtifactEvidence>? retainedEvidence,
    this.recoveryNotice,
    this.followUpPhase,
    List<GrantRootAccessRecord>? grantRootAccesses,
    this.revision = 0,
  }) : grantRoots = grantRoots ?? <String>[],
       grantRootIdentities = grantRootIdentities ?? <GrantRootIdentity>[],
       undoneSteps = undoneSteps ?? <int>[],
       cleanupReviews = cleanupReviews ?? <JournalCleanupReview>[],
       retainedEvidence = retainedEvidence ?? <RetainedArtifactEvidence>[],
       grantRootAccesses = grantRootAccesses ?? <GrantRootAccessRecord>[];

  /// Port of `JournalRecord::new`: owner, session and digest are always set.
  factory JournalRecord.create({
    required String journalId,
    required String opId,
    required String ts,
    required String ownerId,
    required String sessionId,
    required String planDigest,
  }) => JournalRecord(
    journalId: journalId,
    opId: opId,
    ts: ts,
    steps: <Step>[],
    state: null,
    ownerId: ownerId,
    sessionId: sessionId,
    planDigest: planDigest,
  );

  /// Port of `JournalRecord::fresh`: new uuid and RFC 3339 timestamp (micros).
  factory JournalRecord.fresh({
    required String opId,
    required String ownerId,
    required String sessionId,
    required String planDigest,
  }) => JournalRecord.create(
    journalId: _uuid.v4(),
    opId: opId,
    ts: _rfc3339Micros(DateTime.now()),
    ownerId: ownerId,
    sessionId: sessionId,
    planDigest: planDigest,
  );

  String journalId;
  String opId;
  String ts;
  List<Step> steps;
  Step? pendingStep;
  JournalState? state;
  String? ownerId;
  String? sessionId;
  String? planDigest;
  List<String> grantRoots;
  List<GrantRootIdentity> grantRootIdentities;
  List<int> undoneSteps;
  int? pendingUndo;
  UndoRecovery? undoRecovery;
  List<JournalCleanupReview> cleanupReviews;
  S5AuthorityRecord? s5Authority;
  RetainedPublication? pendingPublication;
  PendingArtifactParent? pendingArtifactParent;
  CreatedArtifactParent? createdArtifactParent;
  PendingRetainedArtifact? pendingRetainedArtifact;
  List<RetainedArtifactEvidence> retainedEvidence;
  RecoveryNotice? recoveryNotice;
  FollowUpJournalPhase? followUpPhase;
  List<GrantRootAccessRecord> grantRootAccesses;
  int revision;

  /// Wire JSON in Rust field order with the same omissions.
  Map<String, Object?> toJson() => {
    'journalId': journalId,
    'opId': opId,
    'ts': ts,
    'steps': [for (final step in steps) step.toJson()],
    if (pendingStep != null) 'pendingStep': pendingStep!.toJson(),
    if (state != null) 'state': state!.wire,
    if (ownerId != null) 'ownerId': ownerId,
    if (sessionId != null) 'sessionId': sessionId,
    if (planDigest != null) 'planDigest': planDigest,
    if (grantRoots.isNotEmpty) 'grantRoots': grantRoots,
    if (grantRootIdentities.isNotEmpty)
      'grantRootIdentities': [
        for (final identity in grantRootIdentities) identity.toJson(),
      ],
    if (undoneSteps.isNotEmpty) 'undoneSteps': undoneSteps,
    if (pendingUndo != null) 'pendingUndo': pendingUndo,
    if (undoRecovery != null) 'undoRecovery': undoRecovery!.toJson(),
    if (cleanupReviews.isNotEmpty)
      'cleanupReviews': [
        for (final review in cleanupReviews) review.toJson(),
      ],
    if (s5Authority != null) 's5Authority': s5Authority!.toJson(),
    if (pendingPublication != null)
      'pendingPublication': pendingPublication!.toJson(),
    if (pendingArtifactParent != null)
      'pendingArtifactParent': pendingArtifactParent!.toJson(),
    if (createdArtifactParent != null)
      'createdArtifactParent': createdArtifactParent!.toJson(),
    if (pendingRetainedArtifact != null)
      'pendingRetainedArtifact': pendingRetainedArtifact!.toJson(),
    if (retainedEvidence.isNotEmpty)
      'retainedEvidence': [
        for (final evidence in retainedEvidence) evidence.toJson(),
      ],
    if (recoveryNotice != null) 'recoveryNotice': recoveryNotice!.toJson(),
    if (followUpPhase != null) 'followUpPhase': followUpPhase!.wire,
    if (grantRootAccesses.isNotEmpty)
      'grantRootAccesses': [
        for (final access in grantRootAccesses) access.toJson(),
      ],
    if (revision != 0) 'revision': revision,
  };

  factory JournalRecord.fromJson(Map<dynamic, dynamic> json) {
    final rawSteps = json['steps'];
    if (rawSteps is! List) {
      throw const FormatException('steps must be an array');
    }
    final rawState = json['state'];
    return JournalRecord(
      journalId: _stringField(json, 'journalId'),
      opId: _stringField(json, 'opId'),
      ts: _stringField(json, 'ts'),
      steps: [for (final step in rawSteps) Step.fromJson(_asJsonObject(step))],
      pendingStep: json['pendingStep'] == null
          ? null
          : Step.fromJson(_objectField(json, 'pendingStep')),
      state: rawState == null ? null : JournalState.fromWire(rawState),
      ownerId: _optionalString(json, 'ownerId'),
      sessionId: _optionalString(json, 'sessionId'),
      planDigest: _optionalString(json, 'planDigest'),
      grantRoots: _stringList(json['grantRoots']),
      grantRootIdentities: [
        for (final item in _listOrEmpty(json['grantRootIdentities']))
          GrantRootIdentity.fromJson(item as Map),
      ],
      undoneSteps: [
        for (final item in _listOrEmpty(json['undoneSteps'])) item as int,
      ],
      pendingUndo: _optionalInt(json, 'pendingUndo'),
      undoRecovery: json['undoRecovery'] == null
          ? null
          : UndoRecovery.fromJson(_objectField(json, 'undoRecovery')),
      cleanupReviews: [
        for (final item in _listOrEmpty(json['cleanupReviews']))
          JournalCleanupReview.fromJson(item as Map),
      ],
      s5Authority: json['s5Authority'] == null
          ? null
          : S5AuthorityRecord.fromJson(_objectField(json, 's5Authority')),
      pendingPublication: json['pendingPublication'] == null
          ? null
          : RetainedPublication.fromJson(
              _objectField(json, 'pendingPublication'),
            ),
      pendingArtifactParent: json['pendingArtifactParent'] == null
          ? null
          : PendingArtifactParent.fromJson(
              _objectField(json, 'pendingArtifactParent'),
            ),
      createdArtifactParent: json['createdArtifactParent'] == null
          ? null
          : CreatedArtifactParent.fromJson(
              _objectField(json, 'createdArtifactParent'),
            ),
      pendingRetainedArtifact: json['pendingRetainedArtifact'] == null
          ? null
          : PendingRetainedArtifact.fromJson(
              _objectField(json, 'pendingRetainedArtifact'),
            ),
      retainedEvidence: [
        for (final item in _listOrEmpty(json['retainedEvidence']))
          RetainedArtifactEvidence.fromJson(item as Map),
      ],
      recoveryNotice: json['recoveryNotice'] == null
          ? null
          : RecoveryNotice.fromJson(_objectField(json, 'recoveryNotice')),
      followUpPhase: json['followUpPhase'] == null
          ? null
          : FollowUpJournalPhase.fromWire(json['followUpPhase']),
      grantRootAccesses: [
        for (final item in _listOrEmpty(json['grantRootAccesses']))
          GrantRootAccessRecord.fromJson(item as Map),
      ],
      revision: _intOrZero(json, 'revision'),
    );
  }

  /// Deep copy through the wire form, so candidates never alias the record.
  JournalRecord copy() => JournalRecord.fromJson(
    jsonDecode(jsonEncode(toJson())) as Map<String, dynamic>,
  );
}

List<Object?> _listOrEmpty(Object? value) {
  if (value == null) {
    return const <Object?>[];
  }
  if (value is! List) {
    throw const FormatException('expected an array');
  }
  return value;
}

List<String> _stringList(Object? value) => [
  for (final item in _listOrEmpty(value)) item as String,
];

/// `to_rfc3339_opts(SecondsFormat::Micros, use_z = false)`.
String _rfc3339Micros(DateTime moment) {
  final utc = moment.toUtc();
  String pad(int value, int width) => value.toString().padLeft(width, '0');
  final micros = pad(utc.millisecond * 1000 + utc.microsecond, 6);
  return '${pad(utc.year, 4)}-${pad(utc.month, 2)}-${pad(utc.day, 2)}'
      'T${pad(utc.hour, 2)}:${pad(utc.minute, 2)}:${pad(utc.second, 2)}'
      '.$micros+00:00';
}

/// Port of `RecoveryEntry`. Optional fields are omitted when absent, and
/// [restoreOriginalAvailable] is always emitted.
class RecoveryEntry {
  const RecoveryEntry({
    required this.journalId,
    required this.disposition,
    this.warningKey,
    this.locator,
    this.restoreOriginalAvailable = false,
  });

  final String journalId;
  final RecoveryDisposition disposition;
  final String? warningKey;
  final String? locator;
  final bool restoreOriginalAvailable;

  Map<String, Object?> toJson() => {
    'journalId': journalId,
    'disposition': disposition.wire,
    if (warningKey != null) 'warningKey': warningKey,
    if (locator != null) 'locator': locator,
    'restoreOriginalAvailable': restoreOriginalAvailable,
  };

  @override
  bool operator ==(Object other) =>
      other is RecoveryEntry &&
      other.journalId == journalId &&
      other.disposition == disposition &&
      other.warningKey == warningKey &&
      other.locator == locator &&
      other.restoreOriginalAvailable == restoreOriginalAvailable;

  @override
  int get hashCode => Object.hash(
    journalId,
    disposition,
    warningKey,
    locator,
    restoreOriginalAvailable,
  );

  @override
  String toString() => 'RecoveryEntry(${jsonEncode(toJson())})';
}

// ---------------------------------------------------------------------------
// S5 leases and validated records
// ---------------------------------------------------------------------------

enum S5UndoMode { committedApply, restoreFailedApply }

/// An undo that passed the eligibility inspection but has not been admitted
/// against its replayed grant roots yet.
class PendingS5Undo {
  PendingS5Undo._(this._record, this._permits, this.mode);

  final JournalRecord _record;
  final S5PermitBundle _permits;
  final S5UndoMode mode;

  List<GrantRootAccessRecord> get recordedRootAccesses =>
      _record.grantRootAccesses;
}

sealed class _S5Lease {
  const _S5Lease();

  void release();
}

class _ApplyLease extends _S5Lease {
  const _ApplyLease(this.claim);

  final ClaimedS5Apply claim;

  @override
  void release() => claim.release();
}

class _UndoLease extends _S5Lease {
  const _UndoLease(this.permits, this.roots, this.mode);

  final S5PermitBundle permits;
  final GrantRootRoster roots;
  final S5UndoMode mode;

  @override
  void release() => permits.release();
}

class _RecoveryLease extends _S5Lease {
  const _RecoveryLease(this.roots);

  final GrantRootRoster roots;

  @override
  void release() {}
}

class _TestLease extends _S5Lease {
  const _TestLease(this.permits);

  final S5PermitBundle permits;

  @override
  void release() => permits.release();
}

/// Port of `RetainedLocators`: a non-empty, sorted, de-duplicated list.
class RetainedLocators {
  RetainedLocators._(this._values);

  final List<String> _values;

  String get first => _values.first;

  List<String> get asList => List.unmodifiable(_values);
}

/// Port of `S5Transition`. Each variant is one atomic, validated change.
sealed class S5Transition {
  const S5Transition();
}

class SetPendingArtifactParent extends S5Transition {
  const SetPendingArtifactParent(this.pending);

  final PendingArtifactParent pending;
}

/// Outcome of creating the artifact parent directory.
sealed class ArtifactParentCreateOutcome {
  const ArtifactParentCreateOutcome();
}

class ArtifactParentCreated extends ArtifactParentCreateOutcome {
  const ArtifactParentCreated(this.identity);

  final FilesystemObjectIdentity identity;
}

class ArtifactParentAlreadyExists extends ArtifactParentCreateOutcome {
  const ArtifactParentAlreadyExists();
}

class ArtifactParentCreatedIdentityUnknown extends ArtifactParentCreateOutcome {
  const ArtifactParentCreatedIdentityUnknown();
}

class ResolveArtifactParent extends S5Transition {
  const ResolveArtifactParent(this.outcome);

  final ArtifactParentCreateOutcome outcome;
}

class SetPendingRetainedArtifact extends S5Transition {
  const SetPendingRetainedArtifact(this.pending);

  final PendingRetainedArtifact pending;
}

class MarkPendingRetainedCreated extends S5Transition {
  const MarkPendingRetainedCreated(this.identity);

  final FilesystemObjectIdentity identity;
}

class MarkPendingRetainedIdentityUnknown extends S5Transition {
  const MarkPendingRetainedIdentityUnknown();
}

class FinalizeRetainedArtifact extends S5Transition {
  const FinalizeRetainedArtifact({required this.evidence, required this.phase});

  final RetainedArtifactEvidence evidence;
  final FollowUpJournalPhase phase;
}

class InitializePublication extends S5Transition {
  const InitializePublication(this.publication);

  final RetainedPublication publication;
}

class RecordRetainedPublicationEvidence extends S5Transition {
  const RecordRetainedPublicationEvidence(this.evidence);

  final RetainedArtifactEvidence evidence;
}

class RecordCanonicalSecured extends S5Transition {
  const RecordCanonicalSecured({
    required this.identity,
    required this.descriptorSha256,
  });

  final FilesystemObjectIdentity identity;
  final String descriptorSha256;
}

class RecordCanonicalAuxiliaryReady extends S5Transition {
  const RecordCanonicalAuxiliaryReady({
    required this.auxiliarySha256,
    required this.auxiliaryChargeBytes,
  });

  final String auxiliarySha256;
  final int auxiliaryChargeBytes;
}

class MarkCanonicalBytesStarted extends S5Transition {
  const MarkCanonicalBytesStarted();
}

class MarkCanonicalVerifying extends S5Transition {
  const MarkCanonicalVerifying();
}

class FinalizeApplySuccess extends S5Transition {
  const FinalizeApplySuccess({
    required this.completedStep,
    required this.evidence,
  });

  final Step completedStep;
  final List<RetainedArtifactEvidence> evidence;
}

class FinalizeUndoSuccess extends S5Transition {
  const FinalizeUndoSuccess({
    required this.completedStep,
    required this.evidence,
  });

  final Step completedStep;
  final List<RetainedArtifactEvidence> evidence;
}

class BeginFailedApplyRestore extends S5Transition {
  const BeginFailedApplyRestore();
}

class FinalizeFailedApplyRestore extends S5Transition {
  const FinalizeFailedApplyRestore();
}

enum S5RecoveryTerminal {
  completedApply,
  completedUndo,
  completedRestore,
  failedPartial,
  outcomeUnknown,
}

class FinalizeRecovery extends S5Transition {
  const FinalizeRecovery({
    required this.terminal,
    required this.notice,
    required this.evidence,
  });

  final S5RecoveryTerminal terminal;
  final RecoveryNotice? notice;
  final List<RetainedArtifactEvidence> evidence;
}

class PrepareFailedPartial extends S5Transition {
  const PrepareFailedPartial(this.notice);

  final RecoveryNotice notice;
}

class PrepareOutcomeUnknown extends S5Transition {
  const PrepareOutcomeUnknown(this.notice);

  final RecoveryNotice notice;
}

/// A journal record whose S5 authority was validated and whose mutation lease
/// (apply, undo, recovery or test) is held. Every transition goes through
/// [applyTransition], which validates the whole candidate before it commits.
class ValidatedS5Record {
  ValidatedS5Record._(this._record, this._lease);

  /// Port of the `cfg(test)` constructor: validates [record] and holds
  /// [permits] as the lease.
  factory ValidatedS5Record.forTest(
    JournalRecord record,
    S5PermitBundle permits,
  ) {
    _validateRecord(record);
    return ValidatedS5Record._(record, _TestLease(permits));
  }

  JournalRecord _record;
  final _S5Lease _lease;

  /// Releases the permits and roster lease held by this token.
  void release() => _lease.release();

  bool get _applyLease => _lease is _ApplyLease;

  bool get _undoLease => _lease is _UndoLease;

  S5UndoMode? get _undoModeOrNull {
    final lease = _lease;
    return lease is _UndoLease ? lease.mode : null;
  }

  bool get _recoveryLease => _lease is _RecoveryLease;

  void applyTransition(S5Transition transition) {
    final applyLease = _applyLease;
    final undoLease = _undoLease;
    final undoMode = _undoModeOrNull;
    final recoveryLease = _recoveryLease;
    final candidate = _record.copy();
    switch (transition) {
      case SetPendingArtifactParent(:final pending):
        if (candidate.pendingArtifactParent != null ||
            candidate.createdArtifactParent != null) {
          throw BackendError.journalInvalid(
            'S5 artifact parent transition is invalid',
          );
        }
        candidate.pendingArtifactParent = pending;
      case ResolveArtifactParent(:final outcome):
        final pending = candidate.pendingArtifactParent;
        if (pending == null) {
          throw BackendError.journalInvalid(
            'S5 artifact parent intent is missing',
          );
        }
        candidate.pendingArtifactParent = null;
        switch (outcome) {
          case ArtifactParentCreated(:final identity):
            candidate.createdArtifactParent = CreatedArtifactParent(
              path: pending.path,
              locator: pending.locator,
              identity: identity,
            );
          case ArtifactParentAlreadyExists():
            break;
          case ArtifactParentCreatedIdentityUnknown():
            candidate.pendingArtifactParent = pending;
            candidate.recoveryNotice = RecoveryNotice(
              warningKey: 'error.rfi.followUp.recoveryRequired',
              locator: candidate.pendingArtifactParent!.locator,
            );
            candidate.state = JournalState.recoveryRequired;
        }
        candidate.followUpPhase = FollowUpJournalPhase.artifactParentReady;
      case SetPendingRetainedArtifact(:final pending):
        if (candidate.pendingRetainedArtifact != null) {
          throw BackendError.journalInvalid(
            'S5 retained artifact transition is invalid',
          );
        }
        candidate.pendingRetainedArtifact = pending;
      case MarkPendingRetainedCreated(:final identity):
        final pending = candidate.pendingRetainedArtifact;
        if (pending == null) {
          throw BackendError.journalInvalid(
            'S5 retained artifact intent is missing',
          );
        }
        if (pending.creationState is! PendingArtifactCreatedIntent) {
          throw BackendError.journalInvalid(
            'S5 retained artifact creation state is invalid',
          );
        }
        candidate.pendingRetainedArtifact = PendingRetainedArtifact(
          role: pending.role,
          path: pending.path,
          locator: pending.locator,
          creationState: PendingArtifactCreated(identity),
        );
      case MarkPendingRetainedIdentityUnknown():
        final pending = candidate.pendingRetainedArtifact;
        if (pending == null) {
          throw BackendError.journalInvalid(
            'S5 retained artifact intent is missing',
          );
        }
        candidate.pendingRetainedArtifact = PendingRetainedArtifact(
          role: pending.role,
          path: pending.path,
          locator: pending.locator,
          creationState: const PendingArtifactCreatedIdentityUnknown(),
        );
      case FinalizeRetainedArtifact(:final evidence, :final phase):
        final pending = candidate.pendingRetainedArtifact;
        candidate.pendingRetainedArtifact = null;
        if (pending == null) {
          throw BackendError.journalInvalid(
            'S5 retained artifact state is missing',
          );
        }
        if (pending.role != evidence.role ||
            pending.path != evidence.path ||
            pending.locator != evidence.locator ||
            pending.creationState is! PendingArtifactCreated) {
          throw BackendError.journalInvalid(
            'S5 retained artifact evidence does not match its intent',
          );
        }
        _upsertEvidence(candidate.retainedEvidence, evidence);
        candidate.followUpPhase = phase;
      case InitializePublication(:final publication):
        if (candidate.pendingPublication != null ||
            publication.phase != RetainedPublicationPhase.prepared ||
            (publication.purpose == RetainedPublicationPurpose.apply &&
                !applyLease) ||
            (publication.purpose == RetainedPublicationPurpose.undo &&
                !undoLease)) {
          throw BackendError.journalInvalid(
            'S5 publication initialization is invalid',
          );
        }
        if (publication.purpose == RetainedPublicationPurpose.undo) {
          if (undoMode != S5UndoMode.committedApply) {
            throw BackendError.journalInvalid(
              'S5 retained undo publication requires committed undo authority',
            );
          }
          if (candidate.steps.isEmpty) {
            throw BackendError.journalInvalid(
              'S5 undo step authority is missing',
            );
          }
          final index = candidate.steps.length - 1;
          if (candidate.undoneSteps.contains(index) ||
              candidate.pendingUndo != null) {
            throw BackendError.journalInvalid(
              'S5 undo publication state is invalid',
            );
          }
          candidate.pendingUndo = index;
        }
        candidate.pendingPublication = publication;
        candidate.followUpPhase = FollowUpJournalPhase.publicationActive;
      case RecordRetainedPublicationEvidence(:final evidence):
        final publication = _requirePublication(candidate);
        if (evidence.path != publication.retained) {
          throw BackendError.journalInvalid(
            'S5 retained publication evidence path is invalid',
          );
        }
        publication.retainedIdentity = evidence.identity;
        publication.retainedSha256 = evidence.sha256;
        publication.retainedSecuritySha256 = evidence.securitySha256;
        publication.phase = RetainedPublicationPhase.retained;
        _upsertEvidence(candidate.retainedEvidence, evidence);
      case RecordCanonicalSecured(:final identity, :final descriptorSha256):
        if (!validSha256(descriptorSha256)) {
          throw BackendError.journalInvalid(
            'S5 descriptor predicate is invalid',
          );
        }
        final publication = _requirePublication(candidate);
        publication.createdCanonicalIdentity = identity;
        publication.phase = RetainedPublicationPhase.canonicalSecured;
      case RecordCanonicalAuxiliaryReady(
        :final auxiliarySha256,
        :final auxiliaryChargeBytes,
      ):
        final publication = _requirePublication(candidate);
        if (publication.phase != RetainedPublicationPhase.canonicalSecured ||
            publication.destinationAuxiliarySha256 != auxiliarySha256 ||
            publication.destinationAuxiliaryChargeBytes !=
                auxiliaryChargeBytes) {
          throw BackendError.journalInvalid(
            'S5 canonical auxiliary predicate is invalid',
          );
        }
        publication.phase = RetainedPublicationPhase.canonicalAuxiliaryReady;
      case MarkCanonicalBytesStarted():
        _requirePublication(candidate).phase =
            RetainedPublicationPhase.writingCanonical;
      case MarkCanonicalVerifying():
        _requirePublication(candidate).phase =
            RetainedPublicationPhase.verifyingCanonical;
      case FinalizeApplySuccess(:final completedStep, :final evidence):
        if (!applyLease) {
          throw BackendError.journalInvalid(
            'S5 apply completion requires apply authority',
          );
        }
        for (final item in evidence) {
          _upsertEvidence(candidate.retainedEvidence, item);
        }
        candidate.steps.add(completedStep);
        candidate.pendingPublication = null;
        candidate.pendingArtifactParent = null;
        candidate.pendingRetainedArtifact = null;
        candidate.followUpPhase = null;
        candidate.recoveryNotice = null;
        candidate.pendingUndo = null;
        candidate.state = JournalState.committed;
      case FinalizeUndoSuccess(:final completedStep, :final evidence):
        if (!undoLease || undoMode != S5UndoMode.committedApply) {
          throw BackendError.journalInvalid(
            'S5 undo completion requires undo authority',
          );
        }
        for (final item in evidence) {
          _upsertEvidence(candidate.retainedEvidence, item);
        }
        final index = candidate.pendingUndo;
        if (index == null) {
          throw BackendError.journalInvalid(
            'S5 pending undo authority is missing',
          );
        }
        if (index >= candidate.steps.length ||
            !_stepsEqual(candidate.steps[index], completedStep) ||
            candidate.undoneSteps.contains(index)) {
          throw BackendError.journalInvalid('S5 completed undo step is invalid');
        }
        candidate.undoneSteps.add(index);
        candidate.pendingPublication = null;
        candidate.pendingArtifactParent = null;
        candidate.pendingRetainedArtifact = null;
        candidate.followUpPhase = null;
        candidate.recoveryNotice = null;
        candidate.pendingUndo = null;
        candidate.state = JournalState.undone;
      case BeginFailedApplyRestore():
        final publication = candidate.pendingPublication;
        final firstAdmission =
            candidate.state == JournalState.recoveryRequired &&
            candidate.recoveryNotice != null &&
            publication != null &&
            publication.phase == RetainedPublicationPhase.recoveryRequired;
        final repeatedAdmission =
            candidate.state == JournalState.applying &&
            candidate.recoveryNotice == null &&
            publication != null &&
            publication.phase == RetainedPublicationPhase.restoringOriginal;
        final retryAfterUnknown =
            candidate.state == JournalState.recoveryRequired &&
            candidate.recoveryNotice != null &&
            publication != null &&
            publication.phase == RetainedPublicationPhase.restoringOriginal;
        final displacedOriginalPending =
            publication != null &&
            publication.purpose == RetainedPublicationPurpose.apply &&
            (publication.phase == RetainedPublicationPhase.recoveryRequired ||
                publication.phase ==
                    RetainedPublicationPhase.restoringOriginal) &&
            candidate.pendingRetainedArtifact != null &&
            candidate.pendingRetainedArtifact!.role ==
                RetainedArtifactRole.displacedOriginal &&
            candidate.pendingRetainedArtifact!.path == publication.retained;
        if (undoMode != S5UndoMode.restoreFailedApply ||
            (!firstAdmission && !repeatedAdmission && !retryAfterUnknown) ||
            !displacedOriginalPending ||
            candidate.steps.isNotEmpty ||
            candidate.pendingUndo != null ||
            candidate.pendingStep != null ||
            candidate.undoneSteps.isNotEmpty) {
          throw BackendError.journalInvalid(
            'S5 failed apply restore admission is invalid',
          );
        }
        // displacedOriginalPending (checked above) guarantees a publication.
        final active = publication;
        if (active.purpose != RetainedPublicationPurpose.apply ||
            (active.phase != RetainedPublicationPhase.recoveryRequired &&
                active.phase != RetainedPublicationPhase.restoringOriginal)) {
          throw BackendError.journalInvalid(
            'S5 failed apply restore publication is invalid',
          );
        }
        active.phase = RetainedPublicationPhase.restoringOriginal;
        candidate.recoveryNotice = null;
        candidate.state = JournalState.applying;
      case FinalizeFailedApplyRestore():
        final publication = candidate.pendingPublication;
        final pendingRetained = candidate.pendingRetainedArtifact;
        if (undoMode != S5UndoMode.restoreFailedApply ||
            candidate.state != JournalState.applying ||
            candidate.steps.isNotEmpty ||
            candidate.pendingStep != null ||
            candidate.pendingUndo != null ||
            candidate.undoneSteps.isNotEmpty ||
            candidate.recoveryNotice != null ||
            publication == null ||
            publication.purpose != RetainedPublicationPurpose.apply ||
            publication.phase != RetainedPublicationPhase.restoringOriginal ||
            pendingRetained == null ||
            pendingRetained.role != RetainedArtifactRole.displacedOriginal ||
            pendingRetained.path != publication.retained) {
          throw BackendError.journalInvalid(
            'S5 failed apply restore completion is invalid',
          );
        }
        candidate.retainedEvidence.removeWhere(
          (evidence) => evidence.role == RetainedArtifactRole.displacedOriginal,
        );
        candidate.pendingPublication = null;
        candidate.pendingArtifactParent = null;
        candidate.pendingRetainedArtifact = null;
        candidate.followUpPhase = null;
        candidate.recoveryNotice = null;
        candidate.state = JournalState.undone;
      case FinalizeRecovery(
        :final terminal,
        :final notice,
        :final evidence,
      ):
        if (!recoveryLease) {
          throw BackendError.journalInvalid(
            'S5 recovery completion requires recovery authority',
          );
        }
        for (final item in evidence) {
          _upsertEvidence(candidate.retainedEvidence, item);
        }
        final publication = candidate.pendingPublication;
        switch (terminal) {
          case S5RecoveryTerminal.completedApply:
            if (publication == null ||
                publication.purpose != RetainedPublicationPurpose.apply) {
              throw BackendError.journalInvalid(
                'S5 recovered apply publication is missing',
              );
            }
            if (candidate.steps.isEmpty) {
              candidate.steps.add(_recoveredRestoreStep(publication));
            }
            candidate.state = JournalState.committed;
            candidate.recoveryNotice = null;
          case S5RecoveryTerminal.completedUndo:
            if (publication == null ||
                publication.purpose != RetainedPublicationPurpose.undo) {
              throw BackendError.journalInvalid(
                'S5 recovered undo publication is missing',
              );
            }
            final index = candidate.pendingUndo;
            if (index == null) {
              throw BackendError.journalInvalid(
                'S5 recovered undo index is missing',
              );
            }
            if (!candidate.undoneSteps.contains(index)) {
              candidate.undoneSteps.add(index);
            }
            candidate.state = JournalState.undone;
            candidate.recoveryNotice = null;
          case S5RecoveryTerminal.completedRestore:
            if (publication == null ||
                publication.purpose != RetainedPublicationPurpose.apply ||
                publication.phase !=
                    RetainedPublicationPhase.restoringOriginal) {
              throw BackendError.journalInvalid(
                'S5 restored apply publication is missing',
              );
            }
            final pendingRetained = candidate.pendingRetainedArtifact;
            final stateOk =
                (candidate.state == JournalState.applying &&
                    candidate.recoveryNotice == null) ||
                (candidate.state == JournalState.recoveryRequired &&
                    candidate.recoveryNotice != null);
            if (candidate.steps.isNotEmpty ||
                candidate.pendingStep != null ||
                candidate.pendingUndo != null ||
                candidate.undoneSteps.isNotEmpty ||
                !stateOk ||
                pendingRetained == null ||
                pendingRetained.role !=
                    RetainedArtifactRole.displacedOriginal ||
                pendingRetained.path != publication.retained) {
              throw BackendError.journalInvalid(
                'S5 completed restore state is invalid',
              );
            }
            candidate.retainedEvidence.removeWhere(
              (evidence) =>
                  evidence.role == RetainedArtifactRole.displacedOriginal,
            );
            candidate.state = JournalState.undone;
            candidate.recoveryNotice = null;
          case S5RecoveryTerminal.failedPartial:
            candidate.state = JournalState.partialCommitted;
            candidate.recoveryNotice =
                notice ??
                (throw BackendError.journalInvalid(
                  'S5 partial recovery notice is missing',
                ));
          case S5RecoveryTerminal.outcomeUnknown:
            candidate.state = JournalState.recoveryRequired;
            candidate.recoveryNotice =
                notice ??
                (throw BackendError.journalInvalid(
                  'S5 unknown recovery notice is missing',
                ));
        }
        candidate.pendingPublication = null;
        candidate.pendingArtifactParent = null;
        candidate.pendingRetainedArtifact = null;
        candidate.followUpPhase = null;
        candidate.pendingUndo = null;
      case PrepareFailedPartial(:final notice):
        candidate.recoveryNotice = notice;
        candidate.pendingPublication = null;
        candidate.pendingArtifactParent = null;
        candidate.pendingRetainedArtifact = null;
        candidate.followUpPhase = null;
        candidate.state = JournalState.partialCommitted;
      case PrepareOutcomeUnknown(:final notice):
        final publication = candidate.pendingPublication;
        if (publication != null &&
            publication.phase != RetainedPublicationPhase.restoringOriginal) {
          publication.phase = RetainedPublicationPhase.recoveryRequired;
        }
        candidate.recoveryNotice = notice;
        candidate.state = JournalState.recoveryRequired;
    }
    _validateRecord(candidate);
    _record = candidate;
  }

  String get journalId => _record.journalId;

  Step get approvedStep => _record.s5Authority!.initialStep;

  List<Step> get completedSteps => _record.steps;

  Plan? get plan {
    final lease = _lease;
    return lease is _ApplyLease ? lease.claim.plan : null;
  }

  PlanBinding? get binding {
    final lease = _lease;
    return lease is _ApplyLease ? lease.claim.binding : null;
  }

  GrantRootRoster? get roots {
    final lease = _lease;
    return switch (lease) {
      _ApplyLease(:final claim) => claim.roots,
      _UndoLease(:final roots) => roots,
      _RecoveryLease(:final roots) => roots,
      _TestLease() => null,
    };
  }

  S5UndoMode? get undoMode => _undoModeOrNull;

  int get revision => _record.revision;

  FollowUpJournalPhase? get phase => _record.followUpPhase;

  RetainedPublication? get pendingPublication => _record.pendingPublication;

  PendingArtifactParent? get pendingArtifactParent =>
      _record.pendingArtifactParent;

  PendingRetainedArtifact? get pendingRetainedArtifact =>
      _record.pendingRetainedArtifact;

  List<RetainedArtifactEvidence> get retainedEvidence => _record.retainedEvidence;

  RetainedArtifactEvidence? retainedEvidenceFor(RetainedArtifactRole role) {
    for (final evidence in _record.retainedEvidence) {
      if (evidence.role == role) {
        return evidence;
      }
    }
    return null;
  }

  /// Port of `retained_locators`: every retained locator, sorted and unique,
  /// validated against the journal id.
  RetainedLocators retainedLocators() {
    final locators = <String>[
      for (final evidence in _record.retainedEvidence) evidence.locator,
      if (_record.pendingArtifactParent != null)
        _record.pendingArtifactParent!.locator,
      if (_record.createdArtifactParent != null)
        _record.createdArtifactParent!.locator,
      if (_record.pendingRetainedArtifact != null)
        _record.pendingRetainedArtifact!.locator,
    ]..sort();
    final unique = <String>[];
    for (final locator in locators) {
      if (unique.isEmpty || unique.last != locator) {
        unique.add(locator);
      }
    }
    if (unique.isEmpty ||
        unique.length > maxRetainedArtifactsPerJournal + 2 ||
        unique.any(
          (locator) => !_validRecoveryLocator(_record.journalId, locator),
        )) {
      throw BackendError.journalInvalid(
        'S5 retained locator projection is invalid',
      );
    }
    return RetainedLocators._(unique);
  }

  void markPublicationPhase(
    RetainedPublicationPhase expected,
    RetainedPublicationPhase next,
  ) {
    final candidate = _record.copy();
    final publication = _requirePublication(candidate);
    if (publication.phase != expected) {
      throw BackendError.journalInvalid(
        'S5 publication phase transition is invalid',
      );
    }
    publication.phase = next;
    _validateRecord(candidate);
    _record = candidate;
  }

  void prepareFailedPartial(RecoveryNotice notice) =>
      applyTransition(PrepareFailedPartial(notice));

  void prepareOutcomeUnknown(RecoveryNotice notice) =>
      applyTransition(PrepareOutcomeUnknown(notice));

  void prepareNoEffect() {
    final candidate = _record.copy();
    if (candidate.retainedEvidence.isNotEmpty ||
        candidate.createdArtifactParent != null ||
        candidate.pendingPublication != null) {
      throw BackendError.journalInvalid(
        'S5 no-effect classification conflicts with durable effects',
      );
    }
    candidate.pendingArtifactParent = null;
    candidate.pendingRetainedArtifact = null;
    candidate.followUpPhase = null;
    candidate.recoveryNotice = null;
    candidate.state = JournalState.noEffectRetryable;
    _validateRecord(candidate);
    _record = candidate;
  }

  void prepareCancelled() {
    final candidate = _record.copy();
    final partial =
        candidate.retainedEvidence.isNotEmpty ||
        candidate.createdArtifactParent != null;
    candidate.pendingArtifactParent = null;
    candidate.pendingRetainedArtifact = null;
    candidate.followUpPhase = null;
    candidate.recoveryNotice = null;
    candidate.state = partial
        ? JournalState.cancelledPartial
        : JournalState.cancelledClean;
    _validateRecord(candidate);
    _record = candidate;
  }

  /// The journal record as it stands. Test and recovery code reads this to
  /// inspect the committed state.
  JournalRecord get record => _record;

  RetainedPublication _requirePublication(JournalRecord candidate) {
    final publication = candidate.pendingPublication;
    if (publication == null) {
      throw BackendError.journalInvalid('S5 publication is not active');
    }
    return publication;
  }
}

bool _stepsEqual(Step left, Step right) =>
    jsonEncode(normalizeMetaJson(left.toJson())) ==
    jsonEncode(normalizeMetaJson(right.toJson()));

void _upsertEvidence(
  List<RetainedArtifactEvidence> evidence,
  RetainedArtifactEvidence item,
) {
  for (final existing in evidence) {
    if (existing.role == item.role) {
      if (existing == item) {
        return;
      }
      throw BackendError.journalInvalid(
        'S5 retained evidence role conflicts with existing evidence',
      );
    }
  }
  if (evidence.length >= maxRetainedArtifactsPerJournal) {
    throw BackendError.journalInvalid(
      'S5 retained evidence limit was exceeded',
    );
  }
  evidence.add(item);
}

Step _recoveredRestoreStep(RetainedPublication publication) {
  final created = publication.createdCanonicalIdentity;
  if (created == null) {
    throw BackendError.journalInvalid('S5 created identity is missing');
  }
  return Step(
    kind: StepKind.restoreBackup,
    src: publication.backup,
    dst: publication.canonical,
    noteKey: 'note.rfi.followUp.workbookUpdate',
    meta: <String, Object?>{
      'appliedSha256': publication.publicationSha256,
      'createdCanonicalIdentity': created.toJson(),
      'originalSha256': publication.expectedCanonicalSha256,
    },
  );
}

// ---------------------------------------------------------------------------
// The journal store
// ---------------------------------------------------------------------------

sealed class _PendingS5Entry {
  const _PendingS5Entry();
}

class _ValidPendingS5 extends _PendingS5Entry {
  const _ValidPendingS5(this.record);

  final JournalRecord record;
}

class _LegacyPendingS5 extends _PendingS5Entry {
  const _LegacyPendingS5(this.entry);

  final RecoveryEntry entry;
}

/// Signature of the retained-publication reconciler (the Dart counterpart of
/// `leaf_transaction::reconcile_retained_publication`).
typedef RetainedPublicationReconciler =
    RecoveryEntry? Function(JournalStore store, ValidatedS5Record record);

/// Port of `S5RecoverySession`: iterates pending S5 journals, reconciling each
/// one under its own permit bundle.
class S5RecoverySession {
  S5RecoverySession._(this._store, this._permits, this._entries, this._reconcile);

  final JournalStore _store;
  final S5PermitBundle _permits;
  final ListQueue<_PendingS5Entry> _entries;
  final RetainedPublicationReconciler _reconcile;

  bool get hasNext => _entries.isNotEmpty;

  /// Releases the S5 permits once recovery is finished.
  void release() => _permits.release();

  /// Root accesses required by the next entry, or null when none are needed.
  List<GrantRootAccessRecord>? get nextRootAccesses {
    if (_entries.isEmpty) {
      return null;
    }
    final entry = _entries.first;
    if (entry is _ValidPendingS5) {
      final record = entry.record;
      final restoring =
          record.pendingPublication != null &&
          record.pendingPublication!.phase ==
              RetainedPublicationPhase.restoringOriginal;
      if (record.recoveryNotice == null || restoring) {
        return record.grantRootAccesses;
      }
    }
    return null;
  }

  /// Recovers the next entry. [roots] is required for valid S5 journals and
  /// must be null for legacy entries.
  RecoveryEntry? recoverNext(
    GrantRootRoster? roots,
    CallerIdentity caller,
  ) {
    if (_entries.isEmpty) {
      return null;
    }
    final entry = _entries.removeFirst();
    if (entry is _LegacyPendingS5) {
      if (roots != null) {
        throw BackendError.journalInvalid(
          'legacy recovery cannot consume S5 root authority',
        );
      }
      return entry.entry;
    }
    final record = (entry as _ValidPendingS5).record;
    if (record.ownerId != caller.ownerId) {
      throw BackendError.journalOwnerMismatch();
    }
    final restoringOriginal =
        record.pendingPublication != null &&
        record.pendingPublication!.phase ==
            RetainedPublicationPhase.restoringOriginal;
    final notice = record.recoveryNotice;
    if (notice != null && !restoringOriginal) {
      return _recoveryEntry(record, notice);
    }
    if (roots == null) {
      throw BackendError.journalInvalid('S5 recovery root authority is missing');
    }
    if (!_rostersEqual(roots.records, record.grantRootAccesses)) {
      throw BackendError.journalInvalid('S5 recovery root authority changed');
    }
    final token = ValidatedS5Record._(record, _RecoveryLease(roots));
    if (token.pendingPublication != null) {
      return _reconcile(_store, token);
    }
    final knownEffect =
        token._record.retainedEvidence.isNotEmpty ||
        token._record.createdArtifactParent != null ||
        (token._record.pendingRetainedArtifact != null &&
            token._record.pendingRetainedArtifact!.creationState
                is PendingArtifactCreated);
    final String locator;
    try {
      locator = token.retainedLocators().first;
    } on BackendError {
      final fallback = 'journal:${token._record.journalId};root:0;leaf:recovery';
      return _finishRecovery(token, fallback, knownEffect);
    }
    return _finishRecovery(token, locator, knownEffect);
  }

  RecoveryEntry _finishRecovery(
    ValidatedS5Record token,
    String locator,
    bool knownEffect,
  ) {
    final notice = RecoveryNotice(
      warningKey: 'error.rfi.followUp.recoveryRequired',
      locator: locator,
    );
    final terminal = knownEffect
        ? S5RecoveryTerminal.failedPartial
        : S5RecoveryTerminal.outcomeUnknown;
    final expected = token.revision;
    token.applyTransition(
      FinalizeRecovery(
        terminal: terminal,
        notice: notice,
        evidence: const [],
      ),
    );
    _store.writeS5Cas(token, expected);
    return _recoveryEntry(token._record, notice);
  }
}

bool _rostersEqual(
  List<GrantRootAccessRecord> left,
  List<GrantRootAccessRecord> right,
) {
  if (left.length != right.length) {
    return false;
  }
  for (var index = 0; index < left.length; index++) {
    if (left[index] != right[index]) {
      return false;
    }
  }
  return true;
}

bool _sameS5Provenance(JournalRecord left, JournalRecord right) =>
    left.journalId == right.journalId &&
    left.opId == right.opId &&
    left.ts == right.ts &&
    left.ownerId == right.ownerId &&
    left.sessionId == right.sessionId &&
    left.planDigest == right.planDigest &&
    sameStrings(left.grantRoots, right.grantRoots) &&
    _identitiesEqual(left.grantRootIdentities, right.grantRootIdentities) &&
    _rostersEqual(left.grantRootAccesses, right.grantRootAccesses) &&
    _s5AuthorityEqual(left.s5Authority, right.s5Authority);

bool _identitiesEqual(List<GrantRootIdentity> left, List<GrantRootIdentity> right) {
  if (left.length != right.length) {
    return false;
  }
  for (var index = 0; index < left.length; index++) {
    if (left[index] != right[index]) {
      return false;
    }
  }
  return true;
}

bool _s5AuthorityEqual(S5AuthorityRecord? left, S5AuthorityRecord? right) {
  if (left == null || right == null) {
    return left == right;
  }
  return _stepsEqual(left.initialStep, right.initialStep) &&
      left.sourcePath == right.sourcePath &&
      left.sourceIdentity == right.sourceIdentity &&
      left.targetPath == right.targetPath &&
      left.targetIdentity == right.targetIdentity &&
      left.approvedUpdatesSha256 == right.approvedUpdatesSha256;
}

/// Durable journal storage. Records are JSON files named
/// `<ts with ':' replaced>_<first 8 chars of journalId>.json`.
class JournalStore {
  JournalStore(this.root);

  final String root;

  /// Acquires the interprocess S5 mutation lock. Throws
  /// `operation_in_progress` when another holder has it.
  InterprocessMutationPermit tryAcquireMutationLock() {
    try {
      Directory(root).createSync(recursive: true);
    } on FileSystemException catch (error) {
      throw journalIo('journal directory could not be created', error);
    }
    final lockPath = p.join(root, '.docflow-s5-mutation.lock');
    final handle = _MutationLockFile.acquire(lockPath);
    return InterprocessMutationPermit._(handle.release);
  }

  S5PermitBundle bindS5Permits(
    GlobalMutationPermit process,
    InterprocessMutationPermit interprocess,
  ) => S5PermitBundle(process, interprocess);

  /// Port of `begin_s5`: builds the first S5 record from a claimed apply and
  /// commits it at revision 1. The claim is released on failure.
  ValidatedS5Record beginS5(ClaimedS5Apply claim, CallerIdentity caller) {
    try {
      final plan = claim.plan;
      if (claim.caller.ownerId != caller.ownerId ||
          claim.caller.sessionId != caller.sessionId ||
          plan.steps.length != 1 ||
          claim.createdAt.isAfter(DateTime.now())) {
        throw BackendError.planUnavailable();
      }
      final step = plan.steps[0];
      final sourcePath = step.meta['rfiLogWorkbook'];
      if (sourcePath is! String) {
        throw BackendError.journalInvalid('S5 source authority is missing');
      }
      final sourceIdentity = _identityFromMeta(
        step.meta,
        'rfiLogFingerprint',
        'S5 source identity',
      );
      final targetPath = step.src;
      if (targetPath == null) {
        throw BackendError.journalInvalid('S5 target authority is missing');
      }
      final targetIdentity = _identityFromMeta(
        step.meta,
        'followUpFingerprint',
        'S5 target identity',
      );
      if (!step.meta.containsKey('updates')) {
        throw BackendError.journalInvalid('S5 approved updates are missing');
      }
      final approvedUpdatesSha256 = sha256Hex(
        canonicalJsonBytes(step.meta['updates']),
      );
      final roots = claim.roots.records;
      final record = JournalRecord.fresh(
        opId: plan.opId,
        ownerId: claim.binding.ownerId,
        sessionId: claim.binding.sessionId,
        planDigest: claim.digest,
      );
      record.state = JournalState.applying;
      record.grantRoots = [for (final root in roots) root.root];
      record.grantRootIdentities = [
        for (final root in roots)
          GrantRootIdentity(root: root.root, identity: root.identity),
      ];
      record.grantRootAccesses = List.of(roots);
      record.s5Authority = S5AuthorityRecord(
        initialStep: step,
        sourcePath: sourcePath,
        sourceIdentity: sourceIdentity,
        targetPath: targetPath,
        targetIdentity: targetIdentity,
        approvedUpdatesSha256: approvedUpdatesSha256,
      );
      final validated = ValidatedS5Record._(record, _ApplyLease(claim));
      writeS5Cas(validated, 0);
      return validated;
    } catch (_) {
      claim.release();
      rethrow;
    }
  }

  /// Port of `write` for non-S5 records.
  void write(JournalRecord record) {
    _validateRecord(record);
    if (_hasS5Discriminator(record)) {
      throw BackendError.journalInvalid(
        'S5 journals require revision-CAS authority',
      );
    }
    final permit = tryAcquireMutationLock();
    try {
      Directory(root).createSync(recursive: true);
      final target = _pathFor(record);
      if (File(target).existsSync()) {
        final current = _readFile(target);
        if (current.journalId != record.journalId) {
          throw BackendError.journalInvalid(
            'journal filename collision was detected',
          );
        }
        if (_hasS5Discriminator(current)) {
          throw BackendError.journalInvalid(
            'S5 journal authority cannot be downgraded',
          );
        }
      }
      _writeRecordDurable(record);
    } finally {
      permit.release();
    }
  }

  void _writeRecordDurable(JournalRecord record) {
    try {
      Directory(root).createSync(recursive: true);
    } on FileSystemException catch (error) {
      throw journalIo('journal directory could not be created', error);
    }
    final target = _pathFor(record);
    final temporary = p.join(root, '.docflow-journal-${_uuid.v4()}.tmp');
    try {
      final file = File(temporary);
      if (file.existsSync()) {
        throw BackendError.journalInvalid(
          'journal temporary file could not be created',
        );
      }
      final encoded = const JsonEncoder.withIndent(
        '  ',
      ).convert(normalizeMetaJson(record.toJson()));
      final handle = file.openSync(mode: FileMode.writeOnly);
      try {
        handle.writeFromSync(utf8.encode(encoded));
        handle.flushSync();
      } on FileSystemException catch (error) {
        throw journalIo('journal could not be flushed', error);
      } finally {
        handle.closeSync();
      }
      if (File(target).existsSync()) {
        _atomicReplace(temporary, target);
      } else {
        _publishNoClobber(temporary, target);
      }
      _syncDirectory(root);
    } catch (_) {
      try {
        final leftover = File(temporary);
        if (leftover.existsSync()) {
          leftover.deleteSync();
        }
      } on FileSystemException {
        // Best effort, as in the Rust path.
      }
      rethrow;
    }
  }

  /// Port of `read`: rejects S5 journals, which need targeted admission.
  JournalRecord read(String journalId) {
    final record = _readRaw(journalId);
    if (_hasS5Discriminator(record)) {
      throw BackendError.journalInvalid(
        'S5 journal requires targeted validated admission',
      );
    }
    return record;
  }

  String inspectOperation(String journalId) => _readRaw(journalId).opId;

  /// Port of `inspect_s5_for_undo`.
  PendingS5Undo inspectS5ForUndo(
    String journalId,
    CallerIdentity caller,
    S5PermitBundle permits,
  ) {
    try {
      final record = _readRaw(journalId);
      final common =
          record.opId == 'rfi.follow_up_sync' &&
          record.ownerId == caller.ownerId &&
          record.pendingStep == null &&
          record.pendingUndo == null &&
          record.undoneSteps.isEmpty;
      final committedApply =
          common &&
          record.state == JournalState.committed &&
          record.steps.length == 1 &&
          record.steps[0].kind == StepKind.restoreBackup &&
          record.pendingPublication == null &&
          record.recoveryNotice == null;
      final failedApplyRestoreState =
          record.steps.isEmpty &&
          ((record.state == JournalState.recoveryRequired &&
                  record.recoveryNotice != null) ||
              (record.state == JournalState.applying &&
                  record.recoveryNotice == null));
      final publication = record.pendingPublication;
      final restoreFailedApply =
          common &&
          failedApplyRestoreState &&
          publication != null &&
          publication.purpose == RetainedPublicationPurpose.apply &&
          (publication.phase == RetainedPublicationPhase.recoveryRequired ||
              publication.phase == RetainedPublicationPhase.restoringOriginal) &&
          record.pendingRetainedArtifact != null &&
          record.pendingRetainedArtifact!.role ==
              RetainedArtifactRole.displacedOriginal &&
          record.pendingRetainedArtifact!.path == publication.retained &&
          (publication.phase != RetainedPublicationPhase.recoveryRequired ||
              record.state == JournalState.recoveryRequired) &&
          (publication.phase != RetainedPublicationPhase.restoringOriginal ||
              record.state == JournalState.applying ||
              record.state == JournalState.recoveryRequired);
      if (!committedApply && !restoreFailedApply) {
        throw BackendError.journalInvalid(
          'S5 journal is not eligible for exact undo',
        );
      }
      _validateS5Fields(record);
      return PendingS5Undo._(
        record,
        permits,
        committedApply ? S5UndoMode.committedApply : S5UndoMode.restoreFailedApply,
      );
    } catch (_) {
      permits.release();
      rethrow;
    }
  }

  /// Port of `admit_s5_undo`.
  ValidatedS5Record admitS5Undo(PendingS5Undo pending, GrantRootRoster roots) {
    try {
      if (!_rostersEqual(roots.records, pending._record.grantRootAccesses)) {
        throw BackendError.journalInvalid(
          'S5 replayed root authority does not match the journal',
        );
      }
      _validateRecord(pending._record);
      return ValidatedS5Record._(
        pending._record,
        _UndoLease(pending._permits, roots, pending.mode),
      );
    } catch (_) {
      pending._permits.release();
      rethrow;
    }
  }

  /// Port of `begin_s5_recovery`. [reconcile] is the retained-publication
  /// reconciler from the capability/leaf layer.
  S5RecoverySession beginS5Recovery(
    S5PermitBundle permits, {
    required RetainedPublicationReconciler reconcile,
  }) {
    try {
      final entries = _scanPendingRecoveryEntries();
      return S5RecoverySession._(this, permits, entries, reconcile);
    } catch (_) {
      permits.release();
      rethrow;
    }
  }

  ListQueue<_PendingS5Entry> _scanPendingRecoveryEntries() {
    final directory = Directory(root);
    if (!directory.existsSync()) {
      return ListQueue<_PendingS5Entry>();
    }
    final List<FileSystemEntity> paths;
    try {
      paths = directory.listSync(followLinks: false);
    } on FileSystemException catch (error) {
      throw journalIo('journal directory entry could not be read', error);
    }
    if (paths.length > maxJournalRootEntriesScanned) {
      throw BackendError.journalInvalid(
        'journal recovery root entry limit was exceeded',
      );
    }
    paths.sort((left, right) => p.basename(left.path).compareTo(
      p.basename(right.path),
    ));
    final entries = ListQueue<_PendingS5Entry>();
    var pendingS5 = 0;
    var invalidNotices = 0;
    for (final entry in paths) {
      final path = entry.path;
      final filename = p.basename(path);
      if (_invalidS5NoticeName(filename)) {
        invalidNotices += 1;
        final type = FileSystemEntity.typeSync(path, followLinks: false);
        final size = type == FileSystemEntityType.file
            ? File(path).lengthSync()
            : 0;
        if (invalidNotices > maxInvalidS5Notices ||
            type != FileSystemEntityType.file ||
            size > maxInvalidS5NoticeBytes) {
          throw BackendError.journalInvalid(
            'invalid S5 notice bound was exceeded',
          );
        }
        continue;
      }
      if (p.extension(path) != '.json') {
        continue;
      }
      final record = _readFile(path);
      if (_hasS5Discriminator(record)) {
        if (!_s5RecordNeedsRecovery(record)) {
          continue;
        }
        pendingS5 += 1;
        if (pendingS5 > maxPendingS5Records) {
          throw BackendError.journalInvalid(
            'pending S5 journal limit was exceeded',
          );
        }
        entries.add(_ValidPendingS5(record));
      } else {
        final legacy = _legacyRecoveryEntry(record);
        if (legacy != null) {
          entries.add(_LegacyPendingS5(legacy));
        }
      }
      if (entries.length > maxRecoveryEntries) {
        throw BackendError.journalInvalid(
          'recovery response limit was exceeded',
        );
      }
    }
    return entries;
  }

  /// Test helper port of `read_s5_for_test`: raw read, no S5 rejection.
  JournalRecord readS5ForTest(String journalId) => _readRaw(journalId);

  JournalRecord _readRaw(String journalId) {
    _validateJournalId(journalId);
    final List<FileSystemEntity> entries;
    try {
      entries = Directory(root).listSync(followLinks: false);
    } on FileSystemException catch (error) {
      throw journalIo('journal directory could not be read', error);
    }
    final prefix = journalId.substring(0, journalId.length.clamp(0, 8));
    final matching = <JournalRecord>[];
    for (final entry in entries) {
      final path = entry.path;
      if (p.extension(path) != '.json') {
        continue;
      }
      final filename = p.basename(path);
      if (!filename.endsWith('_$prefix.json')) {
        continue;
      }
      final record = _readFile(path);
      if (record.journalId == journalId) {
        matching.add(record);
      }
    }
    switch (matching.length) {
      case 1:
        return matching.single;
      case 0:
        throw BackendError.journalInvalid('journal was not found');
      default:
        throw BackendError.journalInvalid(
          'duplicate local journal identities were found',
        );
    }
  }

  /// Port of `write_s5_cas`: a revision compare-and-swap. The first write uses
  /// expected revision 0 and creates the file; later writes must match the
  /// on-disk revision and provenance.
  void writeS5Cas(ValidatedS5Record record, int expectedRevision) {
    if (record._record.revision != expectedRevision) {
      throw BackendError.journalInvalid('S5 journal revision token is stale');
    }
    final nextRevision = expectedRevision + 1;
    final candidate = record._record.copy();
    candidate.revision = nextRevision;
    _validateRecord(candidate);
    final target = _pathFor(candidate);
    if (expectedRevision == 0) {
      if (File(target).existsSync()) {
        throw BackendError.journalInvalid(
          'S5 journal initial revision already exists',
        );
      }
    } else {
      final current = _readFile(target);
      if (!_hasS5Discriminator(current) ||
          current.revision != expectedRevision ||
          !_sameS5Provenance(current, candidate)) {
        throw BackendError.journalInvalid(
          'S5 journal revision or provenance changed',
        );
      }
    }
    _writeRecordDurable(candidate);
    record._record = candidate;
  }

  /// Port of `recover_pending`: legacy (non-S5) recovery entries, sorted by
  /// journal id.
  List<RecoveryEntry> recoverPending() {
    final directory = Directory(root);
    if (!directory.existsSync()) {
      return <RecoveryEntry>[];
    }
    final List<FileSystemEntity> entries;
    try {
      entries = directory.listSync(followLinks: false);
    } on FileSystemException catch (error) {
      throw journalIo('journal directory entry could not be read', error);
    }
    final recovery = <RecoveryEntry>[];
    for (final entry in entries) {
      if (p.extension(entry.path) != '.json') {
        continue;
      }
      final record = _readFile(entry.path);
      final legacy = _legacyRecoveryEntry(record);
      if (legacy != null) {
        recovery.add(legacy);
      }
    }
    recovery.sort((left, right) => left.journalId.compareTo(right.journalId));
    return recovery;
  }

  String _pathFor(JournalRecord record) {
    final timestamp = record.ts.replaceAll(':', '-');
    final prefix = record.journalId.substring(
      0,
      record.journalId.length.clamp(0, 8),
    );
    return p.join(root, '${timestamp}_$prefix.json');
  }
}

/// Port of `pending_disposition`.
RecoveryDisposition _pendingDisposition(Step step) {
  final srcExists = step.src != null && _pathExists(step.src!);
  final dstExists = step.dst != null && _pathExists(step.dst!);
  switch (step.kind) {
    case StepKind.mkdir when !dstExists:
      return RecoveryDisposition.noEffect;
    case StepKind.copy when !dstExists:
      return RecoveryDisposition.noEffect;
    case StepKind.move || StepKind.rename
        when srcExists && !dstExists:
      return RecoveryDisposition.noEffect;
    default:
      return RecoveryDisposition.outcomeUnknown;
  }
}

/// `Path::exists`: follows symlinks and is false for dangling links.
bool _pathExists(String path) =>
    FileSystemEntity.typeSync(path) != FileSystemEntityType.notFound;

RecoveryEntry? _legacyRecoveryEntry(JournalRecord record) {
  final RecoveryDisposition disposition;
  if (record.pendingStep != null) {
    disposition = _pendingDisposition(record.pendingStep!);
  } else if (record.pendingUndo != null || record.undoRecovery != null) {
    disposition = RecoveryDisposition.outcomeUnknown;
  } else {
    return null;
  }
  return RecoveryEntry(
    journalId: record.journalId,
    disposition: disposition,
  );
}

bool _s5RecordNeedsRecovery(JournalRecord record) =>
    record.pendingPublication != null ||
    record.pendingArtifactParent != null ||
    record.pendingRetainedArtifact != null ||
    record.followUpPhase != null ||
    record.recoveryNotice != null ||
    record.state == JournalState.applying ||
    record.state == JournalState.recoveryRequired;

bool _invalidS5NoticeName(String filename) {
  const prefix = '.invalid-s5-';
  const suffix = '.json';
  if (!filename.startsWith(prefix) || !filename.endsWith(suffix)) {
    return false;
  }
  if (filename.length < prefix.length + suffix.length) {
    return false;
  }
  final hash = filename.substring(
    prefix.length,
    filename.length - suffix.length,
  );
  return validSha256(hash);
}

RecoveryEntry _recoveryEntry(JournalRecord record, RecoveryNotice notice) =>
    RecoveryEntry(
      journalId: record.journalId,
      disposition: RecoveryDisposition.outcomeUnknown,
      warningKey: notice.warningKey,
      locator: notice.locator,
      restoreOriginalAvailable: _failedApplyRestoreCandidate(record),
    );

bool _failedApplyRestoreCandidate(JournalRecord record) {
  final publication = record.pendingPublication;
  if (publication == null) {
    return false;
  }
  final stateReady =
      (record.state == JournalState.recoveryRequired &&
          record.recoveryNotice != null) ||
      (record.state == JournalState.applying && record.recoveryNotice == null);
  final pending = record.pendingRetainedArtifact;
  return stateReady &&
      record.opId == 'rfi.follow_up_sync' &&
      record.steps.isEmpty &&
      record.pendingStep == null &&
      record.pendingUndo == null &&
      record.undoneSteps.isEmpty &&
      publication.purpose == RetainedPublicationPurpose.apply &&
      (publication.phase == RetainedPublicationPhase.recoveryRequired ||
          publication.phase == RetainedPublicationPhase.restoringOriginal) &&
      pending != null &&
      pending.role == RetainedArtifactRole.displacedOriginal &&
      pending.path == publication.retained;
}

FilesystemObjectIdentity _identityFromMeta(
  Map<String, Object?> meta,
  String fingerprintKey,
  String label,
) {
  final fingerprint = meta[fingerprintKey];
  final identity = fingerprint is Map ? fingerprint['identity'] : null;
  if (identity == null) {
    throw BackendError.journalInvalid('$label is missing');
  }
  try {
    return FilesystemObjectIdentity.fromJson(identity);
  } on FormatException {
    throw BackendError.journalInvalid('$label is invalid');
  }
}

bool _hasS5Discriminator(JournalRecord record) =>
    record.opId == 'rfi.follow_up_sync' ||
    record.s5Authority != null ||
    record.pendingPublication != null ||
    record.pendingArtifactParent != null ||
    record.createdArtifactParent != null ||
    record.pendingRetainedArtifact != null ||
    record.retainedEvidence.isNotEmpty ||
    record.recoveryNotice != null ||
    record.followUpPhase != null ||
    record.grantRootAccesses.isNotEmpty ||
    record.revision != 0;

void _validateRecord(JournalRecord record) {
  _validateJournalId(record.journalId);
  if (record.ts.isEmpty ||
      _byteLength(record.ts) > 128 ||
      _hasControl(record.ts) ||
      record.ts.contains('/') ||
      record.ts.contains('\\')) {
    throw BackendError.journalInvalid('journal timestamp is invalid');
  }
  if (record.opId.isEmpty || _byteLength(record.opId) > 128) {
    throw BackendError.journalInvalid('journal operation identity is invalid');
  }
  if (record.steps.length > maxJournalSteps) {
    throw BackendError.journalInvalid('journal contains too many steps');
  }
  if (record.undoneSteps.any((index) => index >= record.steps.length)) {
    throw BackendError.journalInvalid('journal undo progress is invalid');
  }
  final recovery = record.undoRecovery;
  if (recovery != null &&
      (record.pendingUndo != recovery.index ||
          recovery.index >= record.steps.length)) {
    throw BackendError.journalInvalid('journal undo recovery metadata is invalid');
  }
  if (_hasS5Discriminator(record)) {
    _validateS5Fields(record);
  }
}

void _validateS5Fields(JournalRecord record) {
  Never invalid() =>
      throw BackendError.journalInvalid('S5 journal transaction metadata is invalid');

  if (record.opId != 'rfi.follow_up_sync') {
    throw BackendError.journalInvalid('S5 operation identity is invalid');
  }
  if ((record.ownerId ?? '').isEmpty ||
      (record.sessionId ?? '').isEmpty ||
      record.planDigest == null ||
      !validSha256(record.planDigest!)) {
    throw BackendError.journalInvalid('S5 immutable provenance is invalid');
  }
  if (record.grantRoots.isEmpty ||
      record.grantRoots.length != record.grantRootIdentities.length ||
      record.grantRoots.length != record.grantRootAccesses.length ||
      record.grantRoots.length > 8) {
    throw BackendError.journalInvalid('S5 grant root roster is invalid');
  }
  if (record.s5Authority == null ||
      record.pendingStep != null ||
      record.undoRecovery != null) {
    throw BackendError.journalInvalid('S5 authority state is invalid');
  }

  final seenRoots = <String>{};
  for (final access in record.grantRootAccesses) {
    if (!seenRoots.add(access.root) ||
        !p.isAbsolute(access.root) ||
        !record.grantRoots.contains(access.root) ||
        !record.grantRootIdentities.any(
          (identity) =>
              identity.root == access.root &&
              identity.identity == access.identity,
        )) {
      throw BackendError.journalInvalid('S5 grant root access parity is invalid');
    }
  }

  final authority = record.s5Authority;
  if (authority == null) {
    invalid();
  }
  if (authority.initialStep.kind != StepKind.workbookUpdate ||
      authority.initialStep.src != authority.targetPath ||
      authority.initialStep.dst == null) {
    throw BackendError.journalInvalid('S5 initial step authority is invalid');
  }
  if (!validSha256(authority.approvedUpdatesSha256)) {
    throw BackendError.journalInvalid('S5 approved update digest is invalid');
  }
  if (!_pathInRecordedRoots(authority.sourcePath, record.grantRoots) ||
      !_pathInRecordedRoots(authority.targetPath, record.grantRoots)) {
    throw BackendError.journalInvalid('S5 workbook path authority is invalid');
  }
  if (!_pathHasAccess(
    authority.targetPath,
    record.grantRootAccesses,
    PersistedRootAccess.readWrite,
  )) {
    throw BackendError.journalInvalid('S5 target write authority is invalid');
  }
  final targetParent = _parentOf(authority.targetPath);
  if (targetParent == null) {
    invalid();
  }
  final backupPath = authority.initialStep.dst;
  if (backupPath == null) {
    invalid();
  }
  if (_parentOf(backupPath) != targetParent ||
      !_pathInRecordedRoots(backupPath, record.grantRoots)) {
    throw BackendError.journalInvalid('S5 backup authority is invalid');
  }

  if (record.retainedEvidence.length > maxRetainedArtifactsPerJournal) {
    invalid();
  }
  final roles = <RetainedArtifactRole>{};
  final paths = <String>{};
  final locators = <String>{};
  for (final evidence in record.retainedEvidence) {
    final logical = _checkedAdd(
      evidence.defaultStreamBytes,
      evidence.auxiliaryChargeBytes,
    );
    if (!roles.add(evidence.role) ||
        !paths.add(evidence.path) ||
        !locators.add(evidence.locator) ||
        !validSha256(evidence.sha256) ||
        !validSha256(evidence.securitySha256) ||
        logical != evidence.logicalChargeBytes ||
        !_validRecoveryLocator(record.journalId, evidence.locator) ||
        !_pathInRecordedRoots(evidence.path, record.grantRoots)) {
      invalid();
    }
  }

  final artifactPaths = <(String, String)>[
    if (record.pendingArtifactParent != null)
      (record.pendingArtifactParent!.path, record.pendingArtifactParent!.locator),
    if (record.createdArtifactParent != null)
      (record.createdArtifactParent!.path, record.createdArtifactParent!.locator),
    if (record.pendingRetainedArtifact != null)
      (record.pendingRetainedArtifact!.path, record.pendingRetainedArtifact!.locator),
  ];
  for (final (path, locator) in artifactPaths) {
    if (!_pathInRecordedRoots(path, record.grantRoots) ||
        !_validRecoveryLocator(record.journalId, locator)) {
      invalid();
    }
  }
  if (record.pendingArtifactParent != null &&
      record.createdArtifactParent != null) {
    invalid();
  }
  final notice = record.recoveryNotice;
  if (notice != null &&
      (notice.warningKey.isEmpty ||
          _byteLength(notice.warningKey) > 256 ||
          !_validRecoveryLocator(record.journalId, notice.locator))) {
    invalid();
  }

  final publication = record.pendingPublication;
  if (publication != null) {
    final hashes = [
      publication.expectedCanonicalSha256,
      publication.expectedBackupSha256,
      publication.publicationSha256,
      publication.expectedCanonicalSecuritySha256,
      publication.expectedBackupSecuritySha256,
      publication.publicationSecuritySha256,
      publication.destinationAccessSha256,
      publication.destinationAuxiliarySha256,
    ];
    final retainedSha = publication.retainedSha256;
    final retainedSecurity = publication.retainedSecuritySha256;
    final expectedLogical = _checkedAdd(
      publication.expectedCanonicalDefaultStreamBytes,
      publication.expectedCanonicalAuxiliaryChargeBytes,
    );
    if (hashes.any((hash) => !validSha256(hash)) ||
        expectedLogical != publication.expectedCanonicalLogicalChargeBytes ||
        _parentOf(publication.canonical) != _parentOf(publication.retained) ||
        _parentOf(publication.canonical) != _parentOf(publication.backup) ||
        [
          publication.canonical,
          publication.retained,
          publication.backup,
          publication.publicationSource,
        ].any((path) => !_pathInRecordedRoots(path, record.grantRoots)) ||
        (publication.retainedIdentity == null) != (retainedSha == null) ||
        (publication.retainedIdentity == null) != (retainedSecurity == null) ||
        (retainedSha != null && !validSha256(retainedSha)) ||
        (retainedSecurity != null && !validSha256(retainedSecurity))) {
      invalid();
    }
  }
}

bool _validRecoveryLocator(String journalId, String locator) =>
    locator.runes.length <= maxRecoveryLocatorScalars &&
    !_hasControl(locator) &&
    locator.startsWith('journal:$journalId;root:') &&
    locator.contains(';leaf:') &&
    !locator.contains('/') &&
    !locator.contains('\\');

bool _pathInRecordedRoots(String path, List<String> roots) {
  if (!p.isAbsolute(path) || _hasParentComponent(path)) {
    return false;
  }
  return roots.any((root) => _startsWithComponents(path, root));
}

bool _pathHasAccess(
  String path,
  List<GrantRootAccessRecord> roots,
  PersistedRootAccess required,
) => roots.any(
  (root) =>
      _startsWithComponents(path, root.root) &&
      (required == PersistedRootAccess.readOnly ||
          root.access == PersistedRootAccess.readWrite),
);

bool _hasParentComponent(String path) =>
    p.split(path).any((component) => component == '..');

/// `Path::starts_with`: component-wise prefix test.
bool _startsWithComponents(String path, String root) {
  final pathParts = p.split(p.normalize(path));
  final rootParts = p.split(p.normalize(root));
  if (rootParts.length > pathParts.length) {
    return false;
  }
  for (var index = 0; index < rootParts.length; index++) {
    if (pathParts[index] != rootParts[index]) {
      return false;
    }
  }
  return true;
}

/// `Path::parent`: null for roots and for paths without a parent.
String? _parentOf(String path) {
  final parent = p.dirname(path);
  if (parent == path) {
    return null;
  }
  return parent;
}

void _validateJournalId(String journalId) {
  if (journalId.isEmpty ||
      _byteLength(journalId) > 128 ||
      !journalId.codeUnits.every(
        (unit) =>
            (unit >= 0x30 && unit <= 0x39) ||
            (unit >= 0x41 && unit <= 0x5a) ||
            (unit >= 0x61 && unit <= 0x7a) ||
            unit == 0x2d ||
            unit == 0x5f,
      )) {
    throw BackendError.journalInvalid('journal identity is invalid');
  }
}

JournalRecord _readFile(String path) {
  final FileSystemEntityType type;
  final int length;
  try {
    type = FileSystemEntity.typeSync(path, followLinks: false);
    length = type == FileSystemEntityType.file ? File(path).lengthSync() : 0;
  } on FileSystemException catch (error) {
    throw journalIo('journal metadata could not be read', error);
  }
  if (type != FileSystemEntityType.file || length > maxJournalBytes) {
    throw BackendError.journalInvalid('journal file is invalid');
  }
  final List<int> bytes;
  try {
    bytes = File(path).readAsBytesSync();
  } on FileSystemException catch (error) {
    throw journalIo('journal could not be read', error);
  }
  final JournalRecord record;
  try {
    record = JournalRecord.fromJson(
      jsonDecode(utf8.decode(bytes)) as Map<dynamic, dynamic>,
    );
  } on Object {
    throw BackendError.journalInvalid('journal JSON is invalid');
  }
  _validateRecord(record);
  return record;
}

/// Port of `publish_no_clobber`: the target must not exist.
void _publishNoClobber(String temporary, String target) {
  if (Platform.isWindows) {
    // UNVERIFIED: Windows hard-link publication (CreateHardLinkW) is replaced
    // by an existence check followed by rename, which is not race-free there.
    if (File(target).existsSync()) {
      throw BackendError.journalInvalid(
        'journal could not be published: target already exists',
      );
    }
    try {
      File(temporary).renameSync(target);
    } on FileSystemException catch (error) {
      throw journalIo('journal could not be published', error);
    }
    return;
  }
  try {
    _Posix.link(temporary, target);
  } on FileSystemException catch (error) {
    throw journalIo('journal could not be published', error);
  }
  try {
    File(temporary).deleteSync();
  } on FileSystemException catch (error) {
    throw journalIo('journal temporary file could not be removed', error);
  }
}

/// Port of `atomic_replace`. POSIX `rename` replaces atomically.
void _atomicReplace(String temporary, String target) {
  if (!Platform.isWindows) {
    try {
      File(temporary).renameSync(target);
    } on FileSystemException catch (error) {
      throw journalIo('journal could not be atomically replaced', error);
    }
    return;
  }
  // UNVERIFIED: Windows MoveFileExW(MOVEFILE_REPLACE_EXISTING |
  // MOVEFILE_WRITE_THROUGH) with the 20 x 100 ms retry on access-denied (5),
  // sharing violation (32) and lock violation (33). Dart `rename` is used here
  // and retried with the same policy; not exercised on Linux.
  const maxAttempts = 20;
  for (var attempt = 0; ; attempt++) {
    try {
      File(temporary).renameSync(target);
      return;
    } on FileSystemException catch (error) {
      final code = error.osError?.errorCode;
      final transient = code == 5 || code == 32 || code == 33;
      if (!transient || attempt + 1 >= maxAttempts) {
        throw journalIo('journal could not be atomically replaced', error);
      }
      sleep(const Duration(milliseconds: 100));
    }
  }
}

/// Port of `sync_directory`: fsync of the journal directory on POSIX.
void _syncDirectory(String path) {
  if (Platform.isWindows) {
    return;
  }
  try {
    _Posix.fsyncDirectory(path);
  } on FileSystemException catch (error) {
    throw journalIo('journal directory could not be synchronized', error);
  }
}

// ---------------------------------------------------------------------------
// POSIX FFI: flock, link, fsync, stat, chmod, errno
// ---------------------------------------------------------------------------

typedef _OpenNative = Int32 Function(Pointer<Utf8>, Int32);
typedef _OpenDart = int Function(Pointer<Utf8>, int);
typedef _FlockNative = Int32 Function(Int32, Int32);
typedef _FlockDart = int Function(int, int);
typedef _CloseNative = Int32 Function(Int32);
typedef _CloseDart = int Function(int);
typedef _FsyncNative = Int32 Function(Int32);
typedef _FsyncDart = int Function(int);
typedef _LinkNative = Int32 Function(Pointer<Utf8>, Pointer<Utf8>);
typedef _LinkDart = int Function(Pointer<Utf8>, Pointer<Utf8>);
typedef _ChmodNative = Int32 Function(Pointer<Utf8>, Uint32);
typedef _ChmodDart = int Function(Pointer<Utf8>, int);
typedef _StatNative = Int32 Function(Pointer<Utf8>, Pointer<Uint8>);
typedef _StatDart = int Function(Pointer<Utf8>, Pointer<Uint8>);
typedef _ErrnoNative = Pointer<Int32> Function();

/// Linux and macOS constants. Only the values used by this module are listed.
class _PosixConstants {
  const _PosixConstants({
    required this.oNofollow,
    required this.oCloexec,
    required this.ewouldblock,
  });

  final int oNofollow;
  final int oCloexec;
  final int ewouldblock;

  static _PosixConstants get current => Platform.isMacOS
      ? const _PosixConstants(
          oNofollow: 0x100,
          oCloexec: 0x1000000,
          ewouldblock: 35,
        )
      : const _PosixConstants(
          oNofollow: 0x20000,
          oCloexec: 0x80000,
          ewouldblock: 11,
        );
}

const int _oRdonly = 0;
const int _oRdwr = 2;
const int _lockEx = 2;
const int _lockNb = 4;
const int _lockUn = 8;

class _Posix {
  _Posix._();

  static DynamicLibrary get _library => DynamicLibrary.process();

  static final _OpenDart _open = _library.lookupFunction<_OpenNative, _OpenDart>(
    'open',
  );
  static final _FlockDart _flock = _library
      .lookupFunction<_FlockNative, _FlockDart>('flock');
  static final _CloseDart _close = _library
      .lookupFunction<_CloseNative, _CloseDart>('close');
  static final _FsyncDart _fsync = _library
      .lookupFunction<_FsyncNative, _FsyncDart>('fsync');
  static final _LinkDart _link = _library
      .lookupFunction<_LinkNative, _LinkDart>('link');
  static final _ChmodDart _chmod = _library
      .lookupFunction<_ChmodNative, _ChmodDart>('chmod');
  static final _StatDart _stat = _library.lookupFunction<_StatNative, _StatDart>(
    Platform.isMacOS ? r'stat$INODE64' : 'stat',
  );
  static final _ErrnoNative _errnoLocation = _library
      .lookupFunction<_ErrnoNative, _ErrnoNative>(
        Platform.isMacOS ? '__error' : '__errno_location',
      );

  static int get errno => _errnoLocation().value;

  /// `link(temporary, target)`: fails with EEXIST when the target exists.
  static void link(String temporary, String target) {
    final from = temporary.toNativeUtf8();
    final to = target.toNativeUtf8();
    try {
      if (_link(from, to) != 0) {
        throw FileSystemException(
          'hard link failed',
          target,
          OSError('errno $errno', errno),
        );
      }
    } finally {
      calloc.free(from);
      calloc.free(to);
    }
  }

  /// fsync of a directory descriptor.
  static void fsyncDirectory(String path) {
    final native = path.toNativeUtf8();
    try {
      final fd = _open(native, _oRdonly);
      if (fd < 0) {
        throw FileSystemException(
          'directory could not be opened',
          path,
          OSError('errno $errno', errno),
        );
      }
      try {
        if (_fsync(fd) != 0) {
          throw FileSystemException(
            'directory fsync failed',
            path,
            OSError('errno $errno', errno),
          );
        }
      } finally {
        _close(fd);
      }
    } finally {
      calloc.free(native);
    }
  }

  /// Opens an existing file with `O_NOFOLLOW | O_CLOEXEC` (no `O_CREAT`, so
  /// the variadic mode argument is never needed).
  static int openExisting(String path) {
    final native = path.toNativeUtf8();
    try {
      final fd = _open(
        native,
        _oRdwr | _PosixConstants.current.oNofollow | _PosixConstants.current.oCloexec,
      );
      if (fd < 0) {
        throw FileSystemException(
          'S5 mutation lock could not be opened',
          path,
          OSError('errno $errno', errno),
        );
      }
      return fd;
    } finally {
      calloc.free(native);
    }
  }

  /// Non-blocking exclusive `flock`. Returns 0 on success, otherwise errno.
  static int lockExclusiveNonBlocking(int fd) {
    if (_flock(fd, _lockEx | _lockNb) == 0) {
      return 0;
    }
    return errno == 0 ? -1 : errno;
  }

  static int get ewouldblock => _PosixConstants.current.ewouldblock;

  static void close(int fd) => _close(fd);

  static void unlock(int fd) {
    _flock(fd, _lockUn);
    _close(fd);
  }

  static void chmodOwnerOnly(String path) {
    final native = path.toNativeUtf8();
    try {
      _chmod(native, 0x180); // 0600
    } finally {
      calloc.free(native);
    }
  }

  /// `stat(2)` for the (device, inode) pair, using the platform struct layout.
  static (int, int) statDeviceInode(String path) {
    final native = path.toNativeUtf8();
    final buffer = calloc<Uint8>(256);
    try {
      if (_stat(native, buffer) != 0) {
        throw FileSystemException(
          'stat failed',
          path,
          OSError('errno $errno', errno),
        );
      }
      final view = ByteData.sublistView(buffer.asTypedList(256));
      if (Platform.isMacOS) {
        return (view.getInt32(0, Endian.little), view.getUint64(8, Endian.little));
      }
      return (view.getUint64(0, Endian.little), view.getUint64(8, Endian.little));
    } finally {
      calloc.free(buffer);
      calloc.free(native);
    }
  }
}

/// An acquired exclusive file lock. [release] unlocks and closes.
class _MutationLockFile {
  _MutationLockFile._(this._release);

  final void Function() _release;

  void release() => _release();

  /// Acquires the lock or throws `operation_in_progress` on contention.
  static _MutationLockFile acquire(String path) {
    if (Platform.isWindows) {
      return _acquireWindows(path);
    }
    final file = File(path);
    try {
      if (!file.existsSync()) {
        file.createSync();
        _Posix.chmodOwnerOnly(path);
      }
    } on FileSystemException catch (error) {
      throw journalIo('S5 mutation lock could not be opened', error);
    }
    final int fd;
    try {
      fd = _Posix.openExisting(path);
    } on FileSystemException catch (error) {
      throw journalIo('S5 mutation lock could not be opened', error);
    }
    final code = _Posix.lockExclusiveNonBlocking(fd);
    if (code == 0) {
      return _MutationLockFile._(() => _Posix.unlock(fd));
    }
    _Posix.close(fd);
    if (code == _Posix.ewouldblock) {
      throw BackendError.operationInProgress();
    }
    throw journalIo('S5 mutation lock could not be acquired', 'errno $code');
  }

  // UNVERIFIED: Windows exclusive lock through RandomAccessFile.lockSync. The
  // Rust implementation opens with FILE_SHARE_* and uses LockFileEx with
  // LOCKFILE_FAIL_IMMEDIATELY; contention is recognized by codes 32, 33 and 158.
  static _MutationLockFile _acquireWindows(String path) {
    final raf = File(path).openSync(mode: FileMode.append);
    try {
      raf.lockSync(FileLock.exclusive, 0, 1);
    } on FileSystemException catch (error) {
      raf.closeSync();
      final code = error.osError?.errorCode;
      if (code == 32 || code == 33 || code == 158) {
        throw BackendError.operationInProgress();
      }
      throw journalIo('S5 mutation lock could not be acquired', error);
    }
    return _MutationLockFile._(() {
      raf.unlockSync(0, 1);
      raf.closeSync();
    });
  }
}

/// Identity reader for [readFilesystemObjectIdentity].
class _ObjectIdentityReader {
  static FilesystemObjectIdentity read(String path) {
    if (Platform.isWindows) {
      return _readWindows(path);
    }
    final (device, inode) = _Posix.statDeviceInode(path);
    return UnixObjectIdentity(device: device, inode: inode);
  }

  // UNVERIFIED: Windows CreateFileW + GetFileInformationByHandle through
  // dart:ffi (kernel32). Not exercised on Linux. Offsets follow
  // BY_HANDLE_FILE_INFORMATION: dwVolumeSerialNumber at 28, nFileIndexHigh at
  // 44 and nFileIndexLow at 48.
  static FilesystemObjectIdentity _readWindows(String path) {
    final kernel32 = DynamicLibrary.open('kernel32.dll');
    final createFile = kernel32.lookupFunction<
      IntPtr Function(
        Pointer<Utf16>,
        Uint32,
        Uint32,
        Pointer<Void>,
        Uint32,
        Uint32,
        IntPtr,
      ),
      int Function(Pointer<Utf16>, int, int, Pointer<Void>, int, int, int)
    >('CreateFileW');
    final getInfo = kernel32.lookupFunction<
      Int32 Function(IntPtr, Pointer<Uint8>),
      int Function(int, Pointer<Uint8>)
    >('GetFileInformationByHandle');
    final closeHandle = kernel32.lookupFunction<
      Int32 Function(IntPtr),
      int Function(int)
    >('CloseHandle');
    const fileReadAttributes = 0x80;
    const shareReadWriteDelete = 0x7;
    const openExistingDisposition = 3;
    const backupSemantics = 0x02000000;
    const invalidHandleValue = -1;
    final native = path.toNativeUtf16();
    final info = calloc<Uint8>(52);
    try {
      final handle = createFile(
        native,
        fileReadAttributes,
        shareReadWriteDelete,
        nullptr,
        openExistingDisposition,
        backupSemantics,
        0,
      );
      if (handle == invalidHandleValue) {
        throw FileSystemException('CreateFileW failed', path);
      }
      try {
        if (getInfo(handle, info) == 0) {
          throw FileSystemException(
            'GetFileInformationByHandle failed',
            path,
          );
        }
        final view = ByteData.sublistView(info.asTypedList(52));
        final volumeSerial = view.getUint32(28, Endian.little);
        final fileIndex =
            (view.getUint32(44, Endian.little) << 32) |
            view.getUint32(48, Endian.little);
        return WindowsObjectIdentity(
          volumeSerial: volumeSerial,
          fileIndex: fileIndex,
        );
      } finally {
        closeHandle(handle);
      }
    } finally {
      calloc.free(native);
      calloc.free(info);
    }
  }
}
