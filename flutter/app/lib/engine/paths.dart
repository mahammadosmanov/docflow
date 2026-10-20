/// Port of src-tauri/src/paths.rs: path grants, scoped authorization and
/// symlink/reparse-aware revalidation.
///
/// The capability layer comes from `cap_fs.dart` (E2). The shared
/// `FilesystemObjectIdentity` and the persisted grant-root records come from
/// `journal.dart` (E3).
///
/// Dart has no Rust mutex poisoning or Drop, so locking is unnecessary on the
/// single isolate. `ProvisionalPathGrants` must be finished with `commit()` or
/// `revokeUnlessCommitted()` in a `finally` block.
library;

import 'dart:io';
import 'dart:math';

import 'package:path/path.dart' as p;

import 'cap_fs.dart' as cap;
import 'errors.dart';
import 'journal.dart' as journal;

/// Opens a capability root for a canonical directory with the given access.
typedef CapabilityRootOpener = cap.CapabilityRoot Function(
  String canonicalRoot,
  cap.DirectoryAccess access,
);

// ---- Caller identity and grant scope --------------------------------------

class CallerIdentity {
  const CallerIdentity(this.ownerId, this.sessionId);

  final String ownerId;
  final String sessionId;

  @override
  bool operator ==(Object other) =>
      other is CallerIdentity &&
      other.ownerId == ownerId &&
      other.sessionId == sessionId;

  @override
  int get hashCode => Object.hash(ownerId, sessionId);

  @override
  String toString() => 'CallerIdentity($ownerId, $sessionId)';
}

class GrantScope {
  const GrantScope({
    required this.caller,
    required this.requestId,
    required this.operationId,
  });

  factory GrantScope.create(
    String ownerId,
    String sessionId,
    String requestId,
    String operationId,
  ) => GrantScope(
    caller: CallerIdentity(ownerId, sessionId),
    requestId: requestId,
    operationId: operationId,
  );

  final CallerIdentity caller;
  final String requestId;
  final String operationId;

  @override
  bool operator ==(Object other) =>
      other is GrantScope &&
      other.caller == caller &&
      other.requestId == requestId &&
      other.operationId == operationId;

  @override
  int get hashCode => Object.hash(caller, requestId, operationId);
}

enum GrantAccess {
  read,
  write,
  readWrite;

  bool allows(PathIntent intent) => switch (this) {
    GrantAccess.readWrite => true,
    GrantAccess.read => intent == PathIntent.read,
    GrantAccess.write => intent == PathIntent.write,
  };
}

enum PathIntent { read, write }

class PathGrant {
  const PathGrant({
    required this.grantId,
    required this.root,
    required this.access,
    required this.scope,
    required this.createdAt,
  });

  final String grantId;
  final String root;
  final GrantAccess access;
  final GrantScope scope;
  final DateTime createdAt;

  @override
  bool operator ==(Object other) =>
      other is PathGrant &&
      other.grantId == grantId &&
      other.root == root &&
      other.access == access &&
      other.scope == scope &&
      other.createdAt == createdAt;

  @override
  int get hashCode => Object.hash(grantId, root, access, scope, createdAt);
}

/// Validated roster of grant roots used to admit a plan.
class ValidatedGrantRootRoster {
  ValidatedGrantRootRoster({
    required this.records,
    required this.roots,
    required this.grantIds,
  });

  final List<journal.GrantRootAccessRecord> records;
  final List<cap.CapabilityRoot> roots;
  final List<String> grantIds;

  bool matchesGrants(List<String> otherGrantIds) =>
      _sameStrings(grantIds, otherGrantIds);
}

class VerifiedPersistedSource {
  const VerifiedPersistedSource(this.path);

  final String path;

  @override
  bool operator ==(Object other) =>
      other is VerifiedPersistedSource && other.path == path;

  @override
  int get hashCode => path.hashCode;
}

class _StoredPathGrant {
  const _StoredPathGrant({
    required this.grant,
    required this.capability,
    required this.expectedReadIdentities,
  });

  final PathGrant grant;
  final cap.CapabilityRoot capability;
  final Map<String, journal.FilesystemObjectIdentity> expectedReadIdentities;
}

/// Provisional grants, revoked unless committed (Rust `Drop`).
class ProvisionalPathGrants {
  ProvisionalPathGrants._(this._store, this.grants, this.verifiedSources);

  final PathGrantStore _store;
  final List<PathGrant> grants;
  final List<VerifiedPersistedSource> verifiedSources;
  bool _committed = false;

  void commit() {
    _committed = true;
  }

  /// Equivalent of dropping an uncommitted provisional set.
  void revokeUnlessCommitted() {
    if (!_committed) {
      _store._revokeGrantIds(grants);
    }
  }
}

class PathGrantStore {
  PathGrantStore({required this.ttl, CapabilityRootOpener? openRoot})
    : openRoot = openRoot ?? cap.CapabilityRoot.openWithAccess;

  final Duration ttl;
  final CapabilityRootOpener openRoot;
  final Map<String, _StoredPathGrant> _grants = {};

  PathGrant issue(GrantScope scope, String selectedRoot, GrantAccess access) {
    _rejectLexicalEscape(selectedRoot);
    final root = canonicalizePathOrThrow(
      selectedRoot,
      () => BackendError.pathInvalid('selected path does not exist'),
    );
    if (!FileSystemEntity.isDirectorySync(root)) {
      throw BackendError.pathInvalid(
        'selected path grant root is not a directory',
      );
    }
    final directoryAccess = switch (access) {
      GrantAccess.read => cap.DirectoryAccess.readOnly,
      GrantAccess.write ||
      GrantAccess.readWrite => cap.DirectoryAccess.readWrite,
    };
    final capability = _tryOrThrow(
      () => openRoot(root, directoryAccess),
      () => BackendError.pathInvalid('selected path cannot be opened safely'),
    );
    final now = DateTime.now();
    _grants.removeWhere(
      (_, stored) => _expired(stored.grant.createdAt, now, ttl),
    );
    final grantId = _uniqueGrantId();
    final grant = PathGrant(
      grantId: grantId,
      root: root,
      access: access,
      scope: scope,
      createdAt: now,
    );
    _grants[grantId] = _StoredPathGrant(
      grant: grant,
      capability: capability,
      expectedReadIdentities: const {},
    );
    return grant;
  }

  /// Port of `issue_provisional`. Rolls back partial grants on failure.
  ProvisionalPathGrants issueProvisional(
    GrantScope scope,
    List<(String, GrantAccess)> requests,
  ) {
    if (requests.isEmpty || requests.length > 8) {
      throw BackendError.validation('provisional path grant count is invalid');
    }
    final issued = <PathGrant>[];
    for (final (root, access) in requests) {
      try {
        issued.add(issue(scope, root, access));
      } catch (_) {
        _revokeGrantIds(issued);
        rethrow;
      }
    }
    return ProvisionalPathGrants._(this, issued, const []);
  }

  /// Port of `issue_persisted_read_capabilities`.
  ProvisionalPathGrants issuePersistedReadCapabilities(
    GrantScope scope,
    List<journal.GrantRootIdentity> expectedRoots,
    List<(String, String, journal.FilesystemObjectIdentity)> expectedSources,
  ) {
    if (expectedRoots.isEmpty || expectedSources.isEmpty) {
      throw _persistedSourceReselectionRequired();
    }
    final prepared = <_PreparedRoot>[];
    final seenRoots = <String>{};
    for (final expected in expectedRoots) {
      final root = expected.root;
      _rejectLexicalEscape(root);
      final canonical = _tryOrThrow(
        () => canonicalizePath(root),
        _persistedSourceReselectionRequired,
      );
      if (canonical != root ||
          !seenRoots.add(canonical) ||
          !FileSystemEntity.isDirectorySync(canonical)) {
        throw _persistedSourceReselectionRequired();
      }
      final capability = _tryOrThrow(
        () => openRoot(canonical, cap.DirectoryAccess.readOnly),
        _persistedSourceReselectionRequired,
      );
      final identity = _tryOrThrow(
        capability.identity,
        _persistedSourceReselectionRequired,
      );
      if (identity != expected.identity) {
        throw _persistedSourceReselectionRequired();
      }
      prepared.add(_PreparedRoot(canonical, capability));
    }

    final seenSources = <String>{};
    final verifiedSources = <VerifiedPersistedSource>[];
    for (final (source, expectedCanonical, expectedIdentity)
        in expectedSources) {
      try {
        _rejectLexicalEscape(source);
        _rejectLexicalEscape(expectedCanonical);
      } on BackendError {
        throw _persistedSourceReselectionRequired();
      }
      if (!seenSources.add(source)) {
        throw _persistedSourceReselectionRequired();
      }
      final resolvedSource = _tryOrThrow(
        () => canonicalizePath(source),
        _persistedSourceReselectionRequired,
      );
      if (resolvedSource != expectedCanonical) {
        throw _persistedSourceReselectionRequired();
      }
      // max_by_key: on ties the last matching root wins.
      int? matchedIndex;
      var bestDepth = -1;
      for (var index = 0; index < prepared.length; index++) {
        final root = prepared[index].root;
        if (!_startsWith(expectedCanonical, root)) {
          continue;
        }
        final depth = _componentCount(root);
        if (depth >= bestDepth) {
          bestDepth = depth;
          matchedIndex = index;
        }
      }
      if (matchedIndex == null) {
        throw _persistedSourceReselectionRequired();
      }
      final matched = prepared[matchedIndex];
      final anchoredIdentity = _tryOrThrow(
        () => matched.capability.authorize(resolvedSource).anchor().identity(),
        _persistedSourceReselectionRequired,
      );
      if (anchoredIdentity != expectedIdentity) {
        throw _persistedSourceReselectionRequired();
      }
      if (matched.expectedReadIdentities.containsKey(expectedCanonical)) {
        throw _persistedSourceReselectionRequired();
      }
      matched.expectedReadIdentities[expectedCanonical] = expectedIdentity;
      verifiedSources.add(VerifiedPersistedSource(source));
    }

    final now = DateTime.now();
    _grants.removeWhere(
      (_, stored) => _expired(stored.grant.createdAt, now, ttl),
    );
    final issued = <PathGrant>[];
    for (final root in prepared) {
      final grantId = _uniqueGrantId();
      final grant = PathGrant(
        grantId: grantId,
        root: root.root,
        access: GrantAccess.read,
        scope: scope,
        createdAt: now,
      );
      _grants[grantId] = _StoredPathGrant(
        grant: grant,
        capability: root.capability,
        expectedReadIdentities: root.expectedReadIdentities,
      );
      issued.add(grant);
    }
    return ProvisionalPathGrants._(this, issued, verifiedSources);
  }

  /// Port of `authorize`.
  String authorize(
    String grantId,
    GrantScope scope,
    String candidate,
    PathIntent intent,
  ) {
    _rejectLexicalEscape(candidate);
    final now = DateTime.now();
    final stored = _lookupOrThrow(grantId, now);
    final grant = stored.grant;
    if (grant.scope != scope) {
      throw BackendError.pathGrantScopeMismatch();
    }
    if (!grant.access.allows(intent)) {
      throw BackendError.pathAccessDenied();
    }
    final resolved = _resolveExistingAncestor(candidate);
    if (!_startsWith(resolved, grant.root)) {
      throw BackendError.pathEscape();
    }
    if (intent == PathIntent.read && !_exists(resolved)) {
      throw BackendError.pathInvalid('source path does not exist');
    }
    if (stored.expectedReadIdentities.isNotEmpty) {
      final expectedIdentity = stored.expectedReadIdentities[resolved];
      if (expectedIdentity == null) {
        throw _persistedSourceReselectionRequired();
      }
      _tryOrThrow(
        () => stored.capability
            .authorize(resolved)
            .withExpectedIdentity(expectedIdentity)
            .anchor()
            .openRead(),
        _persistedSourceReselectionRequired,
      );
    }
    return resolved;
  }

  /// Port of `authorize_any`.
  String authorizeAny(
    List<String> grantIds,
    GrantScope scope,
    String candidate,
    PathIntent intent,
  ) {
    BackendError? lastError;
    for (final grantId in grantIds) {
      try {
        return authorize(grantId, scope, candidate, intent);
      } on BackendError catch (error) {
        lastError = error;
      }
    }
    throw lastError ?? BackendError.pathGrantUnavailable();
  }

  /// Port of `authorize_for_plan`.
  String authorizeForPlan(
    List<String> grantIds,
    CallerIdentity caller,
    String operationId,
    String candidate,
    PathIntent intent,
  ) {
    final now = DateTime.now();
    BackendError? lastError;
    for (final grantId in grantIds) {
      final grant = _grantOrNull(grantId, now);
      if (grant == null) {
        lastError = BackendError.pathGrantUnavailable();
        continue;
      }
      if (grant.scope.caller != caller ||
          grant.scope.operationId != operationId) {
        lastError = BackendError.pathGrantScopeMismatch();
        continue;
      }
      try {
        final path = authorize(grantId, grant.scope, candidate, intent);
        _revalidateUnderRoot(grant.root, path, intent);
        return path;
      } on BackendError catch (error) {
        lastError = error;
      }
    }
    throw lastError ?? BackendError.pathGrantUnavailable();
  }

  /// Port of `revalidate_for_plan`.
  String revalidateForPlan(
    List<String> grantIds,
    CallerIdentity caller,
    String operationId,
    String candidate,
    PathIntent intent,
  ) {
    _rejectLexicalEscape(candidate);
    final now = DateTime.now();
    BackendError? lastError;
    for (final grantId in grantIds) {
      final grant = _grantOrNull(grantId, now);
      if (grant == null) {
        lastError = BackendError.pathGrantUnavailable();
        continue;
      }
      if (grant.scope.caller != caller ||
          grant.scope.operationId != operationId) {
        lastError = BackendError.pathGrantScopeMismatch();
        continue;
      }
      if (!grant.access.allows(intent)) {
        lastError = BackendError.pathAccessDenied();
        continue;
      }
      try {
        _revalidateUnderRoot(grant.root, candidate, intent);
        return candidate;
      } on BackendError catch (error) {
        lastError = error;
      }
    }
    throw lastError ?? BackendError.pathGrantUnavailable();
  }

  /// Port of `roots_for_plan`.
  List<String> rootsForPlan(
    List<String> grantIds,
    CallerIdentity caller,
    String operationId,
  ) {
    final now = DateTime.now();
    final roots = <String>[];
    for (final grantId in grantIds) {
      final stored = _lookupOrThrow(grantId, now);
      if (stored.grant.scope.caller != caller ||
          stored.grant.scope.operationId != operationId) {
        throw BackendError.pathGrantScopeMismatch();
      }
      if (!roots.contains(stored.grant.root)) {
        roots.add(stored.grant.root);
      }
    }
    if (roots.isEmpty) {
      throw BackendError.pathGrantUnavailable();
    }
    return roots;
  }

  /// Port of `root_identities_for_plan`.
  List<journal.GrantRootIdentity> rootIdentitiesForPlan(
    List<String> grantIds,
    CallerIdentity caller,
    String operationId,
  ) {
    final now = DateTime.now();
    final roots = <journal.GrantRootIdentity>[];
    for (final grantId in grantIds) {
      final stored = _lookupOrThrow(grantId, now);
      if (stored.grant.scope.caller != caller ||
          stored.grant.scope.operationId != operationId) {
        throw BackendError.pathGrantScopeMismatch();
      }
      final root = stored.grant.root;
      if (roots.any((known) => known.root == root)) {
        continue;
      }
      roots.add(
        journal.GrantRootIdentity(
          root: root,
          identity: _tryOrThrow(
            stored.capability.identity,
            () => BackendError.pathInvalid(
              'granted root identity is unavailable',
            ),
          ),
        ),
      );
    }
    if (roots.isEmpty) {
      throw BackendError.pathGrantUnavailable();
    }
    return roots;
  }

  /// Port of `root_accesses_for_plan`.
  ValidatedGrantRootRoster rootAccessesForPlan(
    List<String> grantIds,
    CallerIdentity caller,
    String operationId,
  ) {
    if (grantIds.isEmpty || grantIds.length > 8) {
      throw BackendError.pathGrantUnavailable();
    }
    final now = DateTime.now();
    final seenGrants = <String>{};
    final records = <journal.GrantRootAccessRecord>[];
    final capabilities = <cap.CapabilityRoot>[];
    for (final grantId in grantIds) {
      if (!seenGrants.add(grantId)) {
        throw BackendError.pathGrantUnavailable();
      }
      final stored = _lookupOrThrow(grantId, now);
      if (stored.grant.scope.caller != caller ||
          stored.grant.scope.operationId != operationId) {
        throw BackendError.pathGrantScopeMismatch();
      }
      final access = switch (stored.grant.access) {
        GrantAccess.read => journal.PersistedRootAccess.readOnly,
        GrantAccess.write ||
        GrantAccess.readWrite => journal.PersistedRootAccess.readWrite,
      };
      final root = stored.grant.root;
      final identity = _tryOrThrow(
        stored.capability.identity,
        () => BackendError.pathInvalid('granted root identity is unavailable'),
      );
      final existing = records.indexWhere((record) => record.root == root);
      if (existing >= 0) {
        final record = records[existing];
        if (record.identity != identity) {
          throw BackendError.pathInvalid(
            'granted root identity changed during admission',
          );
        }
        if (access == journal.PersistedRootAccess.readWrite) {
          records[existing] = journal.GrantRootAccessRecord(
            root: record.root,
            identity: record.identity,
            access: journal.PersistedRootAccess.readWrite,
          );
          capabilities[existing] = stored.capability;
        }
      } else {
        records.add(
          journal.GrantRootAccessRecord(
            root: root,
            identity: identity,
            access: access,
          ),
        );
        capabilities.add(stored.capability);
      }
    }
    return ValidatedGrantRootRoster(
      records: records,
      roots: capabilities,
      grantIds: List.of(grantIds),
    );
  }

  /// Port of `reopen_recorded_roots`.
  ValidatedGrantRootRoster reopenRecordedRoots(
    List<journal.GrantRootAccessRecord> records,
    CallerIdentity caller,
    String operationId,
  ) {
    if (records.isEmpty || records.length > 8) {
      throw BackendError.pathGrantUnavailable();
    }
    final seen = <String>{};
    final roots = <cap.CapabilityRoot>[];
    for (final record in records) {
      final root = record.root;
      _rejectLexicalEscape(root);
      if (!seen.add(record.root) ||
          _tryOrThrow(
                () => canonicalizePath(root),
                BackendError.pathGrantUnavailable,
              ) !=
              root) {
        throw BackendError.pathGrantUnavailable();
      }
      final access = switch (record.access) {
        journal.PersistedRootAccess.readOnly => cap.DirectoryAccess.readOnly,
        journal.PersistedRootAccess.readWrite => cap.DirectoryAccess.readWrite,
      };
      final capability = _tryOrThrow(
        () => openRoot(root, access),
        BackendError.pathGrantUnavailable,
      );
      final identity = _tryOrThrow(
        capability.identity,
        BackendError.pathGrantUnavailable,
      );
      if (identity != record.identity) {
        throw BackendError.pathGrantUnavailable();
      }
      roots.add(capability);
    }
    return ValidatedGrantRootRoster(
      records: List.of(records),
      roots: roots,
      grantIds: const [],
    );
  }

  void revoke(CallerIdentity caller) {
    _grants.removeWhere((_, stored) => stored.grant.scope.caller == caller);
  }

  void _revokeGrantIds(List<PathGrant> grants) {
    final grantIds = {for (final grant in grants) grant.grantId};
    _grants.removeWhere((grantId, _) => grantIds.contains(grantId));
  }

  /// Test helper (`active_grant_count_for_testing`).
  int get activeGrantCountForTesting => _grants.length;

  /// Port of `authorize_capability_for_plan`.
  cap.AuthorizedPath authorizeCapabilityForPlan(
    List<String> grantIds,
    CallerIdentity caller,
    String operationId,
    String candidate,
    PathIntent intent,
  ) => _authorizeCapabilityForPlanInner(
    grantIds,
    caller,
    operationId,
    candidate,
    intent,
    allowRoot: false,
  );

  /// Port of `authorize_directory_capability_for_plan`.
  cap.AuthorizedPath authorizeDirectoryCapabilityForPlan(
    List<String> grantIds,
    CallerIdentity caller,
    String operationId,
    String candidate,
    PathIntent intent,
  ) => _authorizeCapabilityForPlanInner(
    grantIds,
    caller,
    operationId,
    candidate,
    intent,
    allowRoot: true,
  );

  /// Port of `authorize_capability_for_grant`.
  cap.AuthorizedPath authorizeCapabilityForGrant(
    PathGrant grant,
    String candidate,
    PathIntent intent,
  ) {
    _rejectLexicalEscape(candidate);
    final stored = _lookupOrThrow(grant.grantId, DateTime.now());
    if (stored.grant != grant) {
      throw BackendError.pathGrantScopeMismatch();
    }
    if (!grant.access.allows(intent)) {
      throw BackendError.pathAccessDenied();
    }
    final resolved = _resolveExistingAncestor(candidate);
    return _tryOrThrow(
      () => stored.capability.authorize(resolved),
      BackendError.pathEscape,
    );
  }

  cap.AuthorizedPath _authorizeCapabilityForPlanInner(
    List<String> grantIds,
    CallerIdentity caller,
    String operationId,
    String candidate,
    PathIntent intent, {
    required bool allowRoot,
  }) {
    _rejectLexicalEscape(candidate);
    final resolved = _resolveExistingAncestor(candidate);
    final now = DateTime.now();
    BackendError? lastError;
    for (final grantId in grantIds) {
      final stored = _storedOrNull(grantId, now);
      if (stored == null) {
        lastError = BackendError.pathGrantUnavailable();
        continue;
      }
      final grant = stored.grant;
      if (grant.scope.caller != caller ||
          grant.scope.operationId != operationId) {
        lastError = BackendError.pathGrantScopeMismatch();
        continue;
      }
      if (!grant.access.allows(intent)) {
        lastError = BackendError.pathAccessDenied();
        continue;
      }
      final cap.AuthorizedPath authorized;
      try {
        authorized = allowRoot
            ? stored.capability.authorizeDirectory(resolved)
            : stored.capability.authorize(resolved);
      } on Object {
        lastError = BackendError.pathEscape();
        continue;
      }
      if (stored.expectedReadIdentities.isEmpty) {
        return authorized;
      }
      final expectedIdentity = stored.expectedReadIdentities[resolved];
      if (expectedIdentity == null) {
        lastError = _persistedSourceReselectionRequired();
        continue;
      }
      return authorized.withExpectedIdentity(expectedIdentity);
    }
    throw lastError ?? BackendError.pathGrantUnavailable();
  }

  /// Returns the stored grant (dropped first if expired), or throws
  /// `path_grant_not_found_or_expired`.
  _StoredPathGrant _lookupOrThrow(String grantId, DateTime now) {
    final stored = _storedOrNull(grantId, now);
    if (stored == null) {
      throw BackendError.pathGrantUnavailable();
    }
    return stored;
  }

  _StoredPathGrant? _storedOrNull(String grantId, DateTime now) {
    final stored = _grants[grantId];
    if (stored != null && _expired(stored.grant.createdAt, now, ttl)) {
      _grants.remove(grantId);
      return null;
    }
    return stored;
  }

  PathGrant? _grantOrNull(String grantId, DateTime now) =>
      _storedOrNull(grantId, now)?.grant;

  String _uniqueGrantId() {
    while (true) {
      final candidate = newUuidV4();
      if (!_grants.containsKey(candidate)) {
        return candidate;
      }
    }
  }
}

class _PreparedRoot {
  _PreparedRoot(this.root, this.capability);

  final String root;
  final cap.CapabilityRoot capability;
  final Map<String, journal.FilesystemObjectIdentity> expectedReadIdentities =
      {};
}

// ---- Free functions ---------------------------------------------------------

BackendError _persistedSourceReselectionRequired() =>
    BackendError.validation('error.print.queueSourceReselectionRequired');

/// Port of `reject_lexical_escape`.
void _rejectLexicalEscape(String path) {
  if (!p.isAbsolute(path)) {
    throw BackendError.pathInvalid('path must be absolute');
  }
  if (p.split(path).contains('..')) {
    throw BackendError.pathTraversal();
  }
}

/// Port of `resolve_existing_ancestor`.
String _resolveExistingAncestor(String path) {
  var probe = path;
  final suffix = <String>[];
  while (true) {
    final type = _tryOrThrow(
      () => _typeNoFollow(probe),
      () => BackendError.pathInvalid('path metadata could not be inspected'),
    );
    if (type != FileSystemEntityType.notFound) {
      var resolved = _tryOrThrow(
        () => canonicalizePath(probe),
        () => BackendError.pathInvalid('path cannot be canonicalized'),
      );
      for (final component in suffix.reversed) {
        resolved = p.join(resolved, component);
      }
      return resolved;
    }
    final parent = p.dirname(probe);
    if (parent == probe) {
      throw BackendError.pathInvalid('path has no existing ancestor');
    }
    suffix.add(p.basename(probe));
    probe = parent;
  }
}

/// Port of `revalidate_under_root`.
void _revalidateUnderRoot(String root, String candidate, PathIntent intent) {
  if (!_startsWith(candidate, root)) {
    throw BackendError.pathEscape();
  }
  final relative = p.relative(candidate, from: root);
  var current = root;
  _inspectAncestor(current, root, mustBeDirectory: true);
  for (final component in _components(relative)) {
    current = p.join(current, component);
    final type = _tryOrThrow(
      () => _typeNoFollow(current),
      () => BackendError.pathInvalid('path metadata could not be inspected'),
    );
    if (type == FileSystemEntityType.notFound) {
      break;
    }
    if (_isLinkOrReparse(type)) {
      throw BackendError.pathEscape();
    }
    final canonical = _tryOrThrow(
      () => canonicalizePath(current),
      () => BackendError.pathInvalid('path cannot be canonicalized'),
    );
    if (!_startsWith(canonical, root)) {
      throw BackendError.pathEscape();
    }
  }
  if (intent == PathIntent.read && !_exists(candidate)) {
    throw BackendError.pathInvalid('source path does not exist');
  }
}

/// Port of `inspect_ancestor`.
void _inspectAncestor(
  String current,
  String root, {
  required bool mustBeDirectory,
}) {
  final type = _tryOrThrow(
    () => _typeNoFollow(current),
    () => BackendError.pathInvalid('authorized root is unavailable'),
  );
  if (type == FileSystemEntityType.notFound) {
    throw BackendError.pathInvalid('authorized root is unavailable');
  }
  if (_isLinkOrReparse(type) ||
      (mustBeDirectory && type != FileSystemEntityType.directory)) {
    throw BackendError.pathEscape();
  }
  final canonical = _tryOrThrow(
    () => canonicalizePath(current),
    () => BackendError.pathInvalid('authorized root cannot be canonicalized'),
  );
  if (canonical != root) {
    throw BackendError.pathEscape();
  }
}

/// Non-Windows: symbolic links only.
///
/// UNVERIFIED: Windows reparse-point detection (FILE_ATTRIBUTE_REPARSE_POINT)
/// needs native metadata that dart:io does not expose. On Windows only links
/// that dart:io reports as `FileSystemEntityType.link` are rejected; junctions
/// and other reparse points still need a native check.
bool _isLinkOrReparse(FileSystemEntityType type) =>
    type == FileSystemEntityType.link;

FileSystemEntityType _typeNoFollow(String path) =>
    FileSystemEntity.typeSync(path, followLinks: false);

bool _exists(String path) =>
    FileSystemEntity.typeSync(path, followLinks: true) !=
    FileSystemEntityType.notFound;

/// Port of `fs::canonicalize`: resolves symbolic links; throws if missing.
String canonicalizePath(String path) {
  final type = FileSystemEntity.typeSync(path, followLinks: true);
  if (type == FileSystemEntityType.directory) {
    return Directory(path).resolveSymbolicLinksSync();
  }
  if (type == FileSystemEntityType.file) {
    return File(path).resolveSymbolicLinksSync();
  }
  if (type == FileSystemEntityType.link) {
    return Link(path).resolveSymbolicLinksSync();
  }
  throw FileSystemException('path cannot be canonicalized', path);
}

/// [canonicalizePath] with a caller-chosen error on failure.
String canonicalizePathOrThrow(String path, BackendError Function() onError) =>
    _tryOrThrow(() => canonicalizePath(path), onError);

/// Runs [body]; any thrown error is replaced by the error from [onError].
T _tryOrThrow<T>(T Function() body, BackendError Function() onError) {
  try {
    return body();
  } on Object {
    throw onError();
  }
}

/// Component-wise `Path::starts_with`.
bool _startsWith(String path, String root) =>
    p.equals(path, root) || p.isWithin(root, path);

int _componentCount(String path) => p.split(p.normalize(path)).length;

List<String> _components(String relative) {
  if (relative.isEmpty || relative == '.') {
    return const [];
  }
  return [
    for (final component in p.split(relative))
      if (component != '.' && component.isNotEmpty) component,
  ];
}

/// Port of `expired`: `now.duration_since(created_at)` is Ok and >= ttl.
bool _expired(DateTime createdAt, DateTime now, Duration ttl) {
  final age = now.difference(createdAt);
  return !age.isNegative && age >= ttl;
}

bool _sameStrings(List<String> a, List<String> b) {
  if (a.length != b.length) {
    return false;
  }
  for (var index = 0; index < a.length; index++) {
    if (a[index] != b[index]) {
      return false;
    }
  }
  return true;
}

/// Port of `Uuid::new_v4().to_string()` using a cryptographic RNG.
String newUuidV4() {
  final random = Random.secure();
  final bytes = List<int>.generate(16, (_) => random.nextInt(256));
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  final hex = [
    for (final byte in bytes) byte.toRadixString(16).padLeft(2, '0'),
  ];
  return '${hex.sublist(0, 4).join()}-${hex.sublist(4, 6).join()}-'
      '${hex.sublist(6, 8).join()}-${hex.sublist(8, 10).join()}-'
      '${hex.sublist(10, 16).join()}';
}
