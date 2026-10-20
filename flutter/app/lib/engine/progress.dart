/// Port of src-tauri/src/progress.rs: global mutation lock, cancellation
/// tokens and the per-request cancellation registry.
///
/// Dart has no Drop, so the Rust RAII permit is modelled as an explicit
/// [GlobalMutationPermit.release] (idempotent) plus the [withPermit] helper,
/// which releases in a `finally`.
library;

import 'dart:async';

import 'errors.dart';

/// Shared boolean cell, the Dart stand-in for `Arc<AtomicBool>`.
class _Flag {
  bool value = false;
}

class GlobalMutationLock {
  GlobalMutationLock() : _held = _Flag();

  GlobalMutationLock._sharing(this._held);

  final _Flag _held;

  /// Mirrors `#[derive(Clone)]`: the clone shares the same held flag.
  GlobalMutationLock clone() => GlobalMutationLock._sharing(_held);

  GlobalMutationPermit tryAcquire() {
    if (_held.value) {
      throw BackendError.operationInProgress();
    }
    _held.value = true;
    return GlobalMutationPermit._(_held);
  }

  /// Acquires, runs [body], and releases even when [body] throws.
  Future<T> withPermit<T>(Future<T> Function() body) async {
    final permit = tryAcquire();
    try {
      return await body();
    } finally {
      permit.release();
    }
  }
}

class GlobalMutationPermit {
  GlobalMutationPermit._(this._held);

  final _Flag _held;
  bool _released = false;

  void release() {
    if (_released) {
      return;
    }
    _released = true;
    _held.value = false;
  }
}

class CancellationToken {
  CancellationToken._(this._cancelled);

  final _Flag _cancelled;

  void checkpoint() {
    if (isCancelled) {
      throw BackendError.cancelled();
    }
  }

  bool get isCancelled => _cancelled.value;

  /// Test-only identity check (`#[cfg(test)] same_state`).
  bool sameState(CancellationToken other) =>
      identical(_cancelled, other._cancelled);
}

class CancellationRegistry {
  final Map<String, _Flag> _requests = {};

  CancellationToken register(String requestId) {
    final cancelled = _requests.putIfAbsent(requestId, _Flag.new);
    return CancellationToken._(cancelled);
  }

  bool cancel(String requestId) {
    final cancelled = _requests[requestId];
    if (cancelled == null) {
      return false;
    }
    cancelled.value = true;
    return true;
  }

  void finish(String requestId) {
    _requests.remove(requestId);
  }

  void revokeAll() {
    for (final cancelled in _requests.values) {
      cancelled.value = true;
    }
    _requests.clear();
  }
}
