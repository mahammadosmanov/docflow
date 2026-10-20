/// Port of src-tauri/src/plan_store.rs.
///
/// Plans are opaque, immutable, one-shot values with backend provenance. Apply
/// only ever claims a cached plan; it never recalculates it.
///
/// Dart has no `Drop`, so the Rust RAII permits are released explicitly with
/// `release()` (idempotent). Every claim that fails releases the permit it was
/// handed, matching Rust's drop-on-error behaviour.
///
/// The Rust `Mutex` is not needed: a Dart isolate runs one event at a time, so
/// each method below is atomic with respect to the store.
library;

import 'dart:convert';

import 'package:uuid/uuid.dart';

import 'contracts.dart';
import 'errors.dart';
import 'journal.dart';
import 'paths.dart';
import 'progress.dart';

const Uuid _uuid = Uuid();

final RegExp _canonicalUuid = RegExp(
  r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$',
);

enum BackendOrigin { rust }

enum PlanState { available, claimed }

class PlanBinding {
  const PlanBinding({
    required this.ownerId,
    required this.sessionId,
    required this.operationId,
    required this.grantIds,
  });

  final String ownerId;
  final String sessionId;
  final String operationId;
  final List<String> grantIds;

  @override
  bool operator ==(Object other) =>
      other is PlanBinding &&
      other.ownerId == ownerId &&
      other.sessionId == sessionId &&
      other.operationId == operationId &&
      sameStrings(other.grantIds, grantIds);

  @override
  int get hashCode =>
      Object.hash(ownerId, sessionId, operationId, Object.hashAll(grantIds));

  @override
  String toString() =>
      'PlanBinding(ownerId: $ownerId, sessionId: $sessionId, '
      'operationId: $operationId, grantIds: $grantIds)';
}

class _StoredPlan {
  _StoredPlan({
    required this.plan,
    required this.backendPlanId,
    required this.origin,
    required this.binding,
    required this.digest,
    required this.createdAt,
    required this.state,
  });

  Plan plan;
  final String backendPlanId;
  final BackendOrigin origin;
  final PlanBinding? binding;
  String digest;
  final DateTime createdAt;
  PlanState state;
}

class PlanRecordSnapshot {
  const PlanRecordSnapshot({
    required this.origin,
    required this.digest,
    required this.createdAt,
    required this.state,
    required this.binding,
  });

  final BackendOrigin origin;
  final String digest;
  final DateTime createdAt;
  final PlanState state;
  final PlanBinding? binding;

  @override
  bool operator ==(Object other) =>
      other is PlanRecordSnapshot &&
      other.origin == origin &&
      other.digest == digest &&
      other.createdAt == createdAt &&
      other.state == state &&
      other.binding == binding;

  @override
  int get hashCode =>
      Object.hash(origin, digest, createdAt, state, binding);
}

class ClaimedPlan {
  const ClaimedPlan({
    required this.plan,
    required this.backendPlanId,
    required this.origin,
    required this.binding,
    required this.digest,
    required this.createdAt,
  });

  final Plan plan;
  final String backendPlanId;
  final BackendOrigin origin;
  final PlanBinding? binding;
  final String digest;
  final DateTime createdAt;
}

/// Port of `ApplyClaim`: a claimed plan plus the global mutation permit that
/// makes the apply exclusive. [release] must run when the apply finishes.
class ApplyClaim {
  ApplyClaim(this.claimed, this._mutation);

  final ClaimedPlan claimed;
  final GlobalMutationPermit? _mutation;

  void release() => _mutation?.release();
}

/// Port of `ClaimedS5Apply`: the S5 (`rfi.follow_up_sync`) claim carries the
/// process and interprocess permits and the validated grant-root roster.
class ClaimedS5Apply {
  ClaimedS5Apply._(
    this._claimed,
    this._caller,
    this._permits,
    this._roots,
  );

  final ClaimedPlan _claimed;
  final CallerIdentity _caller;
  final S5PermitBundle _permits;
  final GrantRootRoster _roots;

  Plan get plan => _claimed.plan;

  PlanBinding get binding {
    final value = _claimed.binding;
    if (value == null) {
      throw StateError('S5 claims are always bound');
    }
    return value;
  }

  String get digest => _claimed.digest;

  DateTime get createdAt => _claimed.createdAt;

  CallerIdentity get caller => _caller;

  GrantRootRoster get roots => _roots;

  /// Releases the process and interprocess permits held by this claim.
  void release() => _permits.release();
}

/// Port of `ReservedPlanId`: an identifier handed out before the plan exists.
class ReservedPlanId {
  ReservedPlanId._(this.value);

  final String value;

  /// Assigns the reserved identifier to a plan whose id is still empty.
  Plan assignToEmpty(Plan plan) {
    if (plan.planId.isNotEmpty) {
      throw BackendError.validation('reserved plan ID requires an empty plan ID');
    }
    return _withPlanId(plan, value);
  }
}

class PlanStore {
  PlanStore(this.ttl);

  final Duration ttl;
  final Map<String, _StoredPlan> _plans = {};

  ReservedPlanId reservePlanId() => ReservedPlanId._(_uuid.v4());

  /// Port of `insert_bound_reserved`.
  Plan insertBoundReserved(
    BackendOrigin origin,
    PlanBinding binding,
    ReservedPlanId reservation,
    Plan plan,
  ) {
    if (plan.opId != binding.operationId) {
      throw BackendError.validation('plan operation does not match its binding');
    }
    if (plan.planId != reservation.value ||
        !_canonicalUuid.hasMatch(plan.planId)) {
      throw BackendError.validation('reserved plan ID does not match the plan');
    }
    final digest = planDigest(plan);
    final now = DateTime.now();
    if (_plans.containsKey(plan.planId)) {
      throw BackendError.validation('reserved plan ID is occupied');
    }
    _plans[plan.planId] = _StoredPlan(
      plan: _clonePlan(plan),
      backendPlanId: '',
      origin: origin,
      binding: binding,
      digest: digest,
      createdAt: now,
      state: PlanState.available,
    );
    return _clonePlan(plan);
  }

  /// Unbound insertion. Rust exposes this only under `cfg(test)`; it is kept
  /// public here for the ported unit tests and for diagnostics.
  Plan insert(BackendOrigin origin, Plan plan) =>
      _insertInner(origin, null, plan);

  /// Port of `insert_bound`.
  Plan insertBound(BackendOrigin origin, PlanBinding binding, Plan plan) {
    if (plan.opId != binding.operationId) {
      throw BackendError.validation('plan operation does not match its binding');
    }
    return _insertInner(origin, binding, plan);
  }

  Plan _insertInner(BackendOrigin origin, PlanBinding? binding, Plan plan) {
    final now = DateTime.now();
    _plans.removeWhere((_, stored) => _expired(stored.createdAt, now, ttl));

    String planId;
    do {
      planId = _uuid.v4();
    } while (_plans.containsKey(planId));
    final backendPlanId = plan.planId;
    final stamped = _withPlanId(plan, planId);
    final digest = planDigest(stamped);
    _plans[planId] = _StoredPlan(
      plan: _clonePlan(stamped),
      backendPlanId: backendPlanId,
      origin: origin,
      binding: binding,
      digest: digest,
      createdAt: now,
      state: PlanState.available,
    );
    return _clonePlan(stamped);
  }

  PlanRecordSnapshot? snapshot(String planId) {
    final now = DateTime.now();
    final stored = _plans[planId];
    if (stored != null && _expired(stored.createdAt, now, ttl)) {
      _plans.remove(planId);
      return null;
    }
    if (stored == null) {
      return null;
    }
    return PlanRecordSnapshot(
      origin: stored.origin,
      digest: stored.digest,
      createdAt: stored.createdAt,
      state: stored.state,
      binding: stored.binding,
    );
  }

  void revoke(CallerIdentity caller) {
    _plans.removeWhere((_, stored) {
      final binding = stored.binding;
      return binding != null &&
          binding.ownerId == caller.ownerId &&
          binding.sessionId == caller.sessionId;
    });
  }

  /// Test helper port of `claim_for_apply` (unbound claim under the lock).
  ApplyClaim claimForApply(String planId, GlobalMutationLock mutationLock) {
    final mutation = mutationLock.tryAcquire();
    try {
      return ApplyClaim(_claimChecked(planId, null), mutation);
    } catch (_) {
      mutation.release();
      rethrow;
    }
  }

  ApplyClaim claimForApplyBound(
    String planId,
    CallerIdentity caller,
    GlobalMutationLock mutationLock,
  ) {
    final mutation = mutationLock.tryAcquire();
    try {
      return ApplyClaim(_claimBound(planId, caller), mutation);
    } catch (_) {
      mutation.release();
      rethrow;
    }
  }

  ApplyClaim claimForApplyBoundUnderPermit(
    String planId,
    CallerIdentity caller,
    GlobalMutationPermit mutation,
  ) {
    try {
      return ApplyClaim(_claimBound(planId, caller), mutation);
    } catch (_) {
      mutation.release();
      rethrow;
    }
  }

  /// Returns the operation id of an available, bound, unchanged plan without
  /// claiming it.
  String inspectAvailableBoundOperation(String planId, CallerIdentity caller) {
    final now = DateTime.now();
    final stored = _plans[planId];
    if (stored == null) {
      throw BackendError.planUnavailable();
    }
    if (_expired(stored.createdAt, now, ttl) ||
        stored.state != PlanState.available) {
      throw BackendError.planUnavailable();
    }
    final binding = stored.binding;
    if (binding == null) {
      throw BackendError.planUnavailable();
    }
    if (binding.ownerId != caller.ownerId) {
      throw BackendError.planOwnerMismatch();
    }
    if (binding.sessionId != caller.sessionId) {
      throw BackendError.planSessionMismatch();
    }
    if (planDigest(stored.plan) != stored.digest) {
      throw BackendError.planUnavailable();
    }
    return stored.plan.opId;
  }

  /// Port of `claim_s5_for_apply_bound_under_permit`. The permits and roster are
  /// consumed on success; on failure the permits are released and the stored
  /// plan is left untouched.
  ClaimedS5Apply claimS5ForApplyBoundUnderPermit(
    String planId,
    CallerIdentity caller,
    S5PermitBundle permits,
    GrantRootRoster roots,
  ) {
    try {
      final now = DateTime.now();
      final stored = _plans[planId];
      if (stored == null) {
        throw BackendError.planUnavailable();
      }
      if (_expired(stored.createdAt, now, ttl) ||
          stored.state != PlanState.available ||
          stored.plan.opId != 'rfi.follow_up_sync' ||
          planDigest(stored.plan) != stored.digest) {
        throw BackendError.planUnavailable();
      }
      final binding = stored.binding;
      if (binding == null) {
        throw BackendError.planUnavailable();
      }
      if (binding.ownerId != caller.ownerId) {
        throw BackendError.planOwnerMismatch();
      }
      if (binding.sessionId != caller.sessionId) {
        throw BackendError.planSessionMismatch();
      }
      if (binding.operationId != stored.plan.opId ||
          !roots.matchesGrants(binding.grantIds)) {
        throw BackendError.planUnavailable();
      }
      stored.state = PlanState.claimed;
      final claimed = ClaimedPlan(
        plan: _clonePlan(stored.plan),
        backendPlanId: stored.backendPlanId,
        origin: stored.origin,
        binding: stored.binding,
        digest: stored.digest,
        createdAt: stored.createdAt,
      );
      return ClaimedS5Apply._(claimed, caller, permits, roots);
    } catch (_) {
      permits.release();
      rethrow;
    }
  }

  ClaimedPlan resolveBound(String planId, CallerIdentity caller) {
    final now = DateTime.now();
    final stored = _plans[planId];
    if (stored != null && _expired(stored.createdAt, now, ttl)) {
      _plans.remove(planId);
      throw BackendError.planUnavailable();
    }
    if (stored == null) {
      throw BackendError.planUnavailable();
    }
    final binding = stored.binding;
    if (binding == null) {
      throw BackendError.planUnavailable();
    }
    if (binding.ownerId != caller.ownerId) {
      throw BackendError.planOwnerMismatch();
    }
    if (binding.sessionId != caller.sessionId) {
      throw BackendError.planSessionMismatch();
    }
    if (planDigest(stored.plan) != stored.digest) {
      throw BackendError.planUnavailable();
    }
    return ClaimedPlan(
      plan: _clonePlan(stored.plan),
      backendPlanId: stored.backendPlanId,
      origin: stored.origin,
      binding: stored.binding,
      digest: stored.digest,
      createdAt: stored.createdAt,
    );
  }

  /// Replaces an available plan's content while keeping its id, binding,
  /// provenance and creation time. The expected digest must match exactly.
  Plan replaceBoundAvailable(
    String planId,
    CallerIdentity caller,
    String expectedDigest,
    Plan replacement,
  ) {
    final now = DateTime.now();
    final stored = _plans[planId];
    if (stored == null) {
      throw BackendError.planUnavailable();
    }
    if (_expired(stored.createdAt, now, ttl)) {
      throw BackendError.planUnavailable();
    }
    final binding = stored.binding;
    if (binding == null) {
      throw BackendError.planUnavailable();
    }
    if (binding.ownerId != caller.ownerId) {
      throw BackendError.planOwnerMismatch();
    }
    if (binding.sessionId != caller.sessionId) {
      throw BackendError.planSessionMismatch();
    }
    if (stored.state != PlanState.available ||
        stored.digest != expectedDigest ||
        planDigest(stored.plan) != expectedDigest) {
      throw BackendError.planUnavailable();
    }
    if (replacement.opId != stored.plan.opId ||
        replacement.opId != binding.operationId) {
      throw BackendError.validation(
        'replacement plan operation does not match its binding',
      );
    }
    final revised = _withPlanId(replacement, planId);
    stored.plan = _clonePlan(revised);
    stored.digest = planDigest(revised);
    return _clonePlan(revised);
  }

  /// Number of plans currently cached (port of `active_plan_count_for_testing`).
  int get activePlanCountForTesting => _plans.length;

  /// Unbound claim (test helper port of `claim`).
  ClaimedPlan claim(String planId) => _claimChecked(planId, null);

  ClaimedPlan _claimBound(String planId, CallerIdentity caller) =>
      _claimChecked(planId, caller);

  ClaimedPlan _claimChecked(String planId, CallerIdentity? caller) {
    final now = DateTime.now();
    final stored = _plans[planId];
    if (stored != null && _expired(stored.createdAt, now, ttl)) {
      _plans.remove(planId);
      throw BackendError.planUnavailable();
    }
    if (stored == null) {
      throw BackendError.planUnavailable();
    }
    if (caller != null) {
      final binding = stored.binding;
      if (binding == null) {
        throw BackendError.planUnavailable();
      }
      if (binding.ownerId != caller.ownerId) {
        throw BackendError.planOwnerMismatch();
      }
      if (binding.sessionId != caller.sessionId) {
        throw BackendError.planSessionMismatch();
      }
    }
    if (stored.state != PlanState.available ||
        planDigest(stored.plan) != stored.digest) {
      throw BackendError.planUnavailable();
    }
    stored.state = PlanState.claimed;
    return ClaimedPlan(
      plan: _clonePlan(stored.plan),
      backendPlanId: stored.backendPlanId,
      origin: stored.origin,
      binding: stored.binding,
      digest: stored.digest,
      createdAt: stored.createdAt,
    );
  }
}

bool _expired(DateTime createdAt, DateTime now, Duration ttl) {
  final age = now.difference(createdAt);
  // Rust: `now.duration_since(created_at).is_ok_and(|age| age >= ttl)`. A
  // creation time in the future is never expired.
  return !age.isNegative && age >= ttl;
}

/// SHA-256 over the canonical JSON of the plan, lower-case hex.
String planDigest(Plan plan) => sha256Hex(canonicalJsonBytes(plan.toJson()));

Plan _withPlanId(Plan plan, String planId) => Plan(
  planId: planId,
  opId: plan.opId,
  steps: plan.steps,
  warnings: plan.warnings,
  meta: plan.meta,
);

/// Deep value copy with the wire representation, so stored plans cannot be
/// mutated through references held by callers.
Plan _clonePlan(Plan plan) =>
    Plan.fromJson(jsonDecode(jsonEncode(plan.toJson())) as Map<String, dynamic>);
