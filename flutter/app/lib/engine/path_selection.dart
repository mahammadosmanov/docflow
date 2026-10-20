/// Port of src-tauri/src/path_selection.rs: opaque, caller-bound, exact-scoped
/// path selections issued from a native file/folder picker.
library;

import 'dart:io';

import 'package:path/path.dart' as p;

import 'errors.dart';
import 'paths.dart';

enum SelectionKind { folder, file }

/// Port of `SelectionScope`.
enum SelectionScope {
  reviewDestination,
  reviewItem,
  revealReviewItem,
  undoDestination,
}

extension SelectionScopeExact on SelectionScope {
  /// (operationId, parameterName, kind), as in `SelectionScope::exact`.
  (String, String, SelectionKind) get exact => switch (this) {
    SelectionScope.reviewDestination => (
      'rfi.review',
      'destination',
      SelectionKind.folder,
    ),
    SelectionScope.reviewItem || SelectionScope.revealReviewItem => (
      'rfi.review',
      'item',
      SelectionKind.file,
    ),
    SelectionScope.undoDestination => (
      'history.undo',
      'destination',
      SelectionKind.folder,
    ),
  };
}

/// Port of `PathSelection` (`deny_unknown_fields`, camelCase).
class PathSelection {
  const PathSelection({
    required this.selectionId,
    required this.displayPath,
    required this.name,
  });

  static const Set<String> _knownKeys = {'selectionId', 'displayPath', 'name'};

  final String selectionId;
  final String displayPath;
  final String name;

  factory PathSelection.fromJson(Map<String, Object?> json) {
    final unknown = json.keys.where((key) => !_knownKeys.contains(key));
    if (unknown.isNotEmpty) {
      throw FormatException('unknown field `${unknown.first}`');
    }
    return PathSelection(
      selectionId: json['selectionId']! as String,
      displayPath: json['displayPath']! as String,
      name: json['name']! as String,
    );
  }

  Map<String, Object?> toJson() => {
    'selectionId': selectionId,
    'displayPath': displayPath,
    'name': name,
  };

  @override
  bool operator ==(Object other) =>
      other is PathSelection &&
      other.selectionId == selectionId &&
      other.displayPath == displayPath &&
      other.name == name;

  @override
  int get hashCode => Object.hash(selectionId, displayPath, name);
}

class _StoredSelection {
  const _StoredSelection({
    required this.path,
    required this.caller,
    required this.operationId,
    required this.parameterName,
    required this.kind,
    required this.createdAt,
  });

  final String path;
  final CallerIdentity caller;
  final String operationId;
  final String parameterName;
  final SelectionKind kind;
  final DateTime createdAt;
}

class PathSelectionStore {
  PathSelectionStore({required this.ttl});

  final Duration ttl;
  final Map<String, _StoredSelection> _selections = {};

  PathSelection issue(
    CallerIdentity caller,
    String operationId,
    String parameterName,
    String path,
    SelectionKind kind,
  ) {
    _validateScope(operationId, parameterName);
    final canonical = canonicalizePathOrThrow(
      path,
      () => BackendError.pathInvalid('selected path does not exist'),
    );
    final FileSystemEntityType type;
    try {
      type = FileSystemEntity.typeSync(canonical, followLinks: false);
    } on Object {
      throw BackendError.pathInvalid('selected path cannot be inspected');
    }
    final kindMatches = switch (kind) {
      SelectionKind.folder => type == FileSystemEntityType.directory,
      SelectionKind.file => type == FileSystemEntityType.file,
    };
    if (!kindMatches) {
      throw BackendError.pathInvalid('selected path has the wrong kind');
    }

    final selectionId = newUuidV4();
    final now = DateTime.now();
    _selections.removeWhere((_, value) => _expired(value.createdAt, now, ttl));
    _selections[selectionId] = _StoredSelection(
      path: canonical,
      caller: caller,
      operationId: operationId,
      parameterName: parameterName,
      kind: kind,
      createdAt: now,
    );
    return PathSelection(
      selectionId: selectionId,
      displayPath: canonical,
      name: _fileNameOrPath(canonical),
    );
  }

  /// Port of `resolve`.
  String resolve(
    PathSelection reference,
    CallerIdentity caller,
    String operationId,
    String parameterName,
    SelectionKind kind,
  ) => resolveId(
    reference.selectionId,
    caller,
    operationId,
    parameterName,
    kind,
  );

  /// Port of `resolve_id`.
  String resolveId(
    String selectionId,
    CallerIdentity caller,
    String operationId,
    String parameterName,
    SelectionKind kind,
  ) {
    final selection = _resolveStored(selectionId, caller);
    if (selection.operationId != operationId ||
        selection.parameterName != parameterName ||
        selection.kind != kind) {
      throw BackendError.pathGrantScopeMismatch();
    }
    return selection.path;
  }

  /// Port of `resolve_scope`.
  String resolveScope(
    String selectionId,
    CallerIdentity caller,
    SelectionScope scope,
  ) {
    final (operationId, parameterName, kind) = scope.exact;
    return resolveId(selectionId, caller, operationId, parameterName, kind);
  }

  /// Port of `revoke`.
  void revoke(CallerIdentity caller) {
    _selections.removeWhere((_, selection) => selection.caller == caller);
  }

  /// Port of `revoke_selection`.
  void revokeSelection(String selectionId, CallerIdentity caller) {
    final selection = _selections[selectionId];
    if (selection != null && selection.caller == caller) {
      _selections.remove(selectionId);
    }
  }

  _StoredSelection _resolveStored(String selectionId, CallerIdentity caller) {
    final now = DateTime.now();
    final existing = _selections[selectionId];
    if (existing != null && _expired(existing.createdAt, now, ttl)) {
      _selections.remove(selectionId);
    }
    final selection = _selections[selectionId];
    if (selection == null) {
      throw BackendError.pathGrantUnavailable();
    }
    if (selection.caller != caller) {
      throw BackendError.pathGrantScopeMismatch();
    }
    return selection;
  }
}

void _validateScope(String operationId, String parameterName) {
  if (operationId.isEmpty ||
      _utf8Length(operationId) > 128 ||
      parameterName.isEmpty ||
      _utf8Length(parameterName) > 128) {
    throw BackendError.invalidParams();
  }
}

/// UTF-8 byte length, matching Rust `str::len`.
int _utf8Length(String value) {
  var total = 0;
  for (final rune in value.runes) {
    total += rune < 0x80
        ? 1
        : rune < 0x800
        ? 2
        : rune < 0x10000
        ? 3
        : 4;
  }
  return total;
}

/// Port of `file_name().and_then(to_str)` with the full-path fallback.
String _fileNameOrPath(String path) {
  final name = p.basename(path);
  if (p.dirname(path) == path || name.isEmpty || name == '..') {
    return path;
  }
  return name;
}

/// Port of `expired`: `now.duration_since(created_at)` is Ok and >= ttl.
bool _expired(DateTime createdAt, DateTime now, Duration ttl) {
  final age = now.difference(createdAt);
  return !age.isNegative && age >= ttl;
}
