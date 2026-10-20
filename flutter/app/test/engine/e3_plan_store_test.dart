// Port of the inline tests in src-tauri/src/plan_store.rs (first group).
// Concurrency tests are sequential: the Dart engine runs in one isolate.
import 'package:docflow/engine/contracts.dart';
import 'package:docflow/engine/errors.dart';
import 'package:docflow/engine/paths.dart';
import 'package:docflow/engine/plan_store.dart';
import 'package:docflow/engine/progress.dart';
import 'package:flutter_test/flutter_test.dart';

Plan _plan({String planId = 'client-authored-id'}) => Plan(
  planId: planId,
  opId: 'rfi.process_scans',
  steps: [
    Step(
      kind: StepKind.move,
      src: '/in/scan.pdf',
      dst: '/out/RFI.pdf',
      noteKey: 'note.renamed',
      meta: <String, Object?>{},
    ),
  ],
  warnings: const [],
  meta: <String, Object?>{},
);

final _uuidPattern = RegExp(
  r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$',
);

void main() {
  test(
    'plans are opaque, one-shot values with provenance and digest',
    () {
      final store = PlanStore(const Duration(seconds: 300));
      final before = DateTime.now();
      final review = store.insert(BackendOrigin.rust, _plan());
      final planId = review.planId;

      expect(planId, isNot('client-authored-id'));
      expect(_uuidPattern.hasMatch(planId), isTrue);
      final snapshot = store.snapshot(planId)!;
      expect(snapshot.origin, BackendOrigin.rust);
      expect(snapshot.state, PlanState.available);
      expect(snapshot.digest.length, 64);
      expect(snapshot.createdAt.isBefore(before), isFalse);

      final claimed = store.claim(planId);
      expect(claimed.origin, BackendOrigin.rust);
      expect(claimed.plan.steps.first.dst, '/out/RFI.pdf');
      expect(claimed.digest, snapshot.digest);

      final reused = _captureError(() => store.claim(planId));
      final forged = _captureError(
        () => store.claim('00000000-0000-4000-8000-000000000000'),
      );
      expect(reused.code, 'plan_not_found_or_expired');
      expect(forged, reused);
    },
  );

  test('expired plans are pruned and rejected before claiming', () {
    final store = PlanStore(Duration.zero);
    final review = store.insert(BackendOrigin.rust, _plan());

    expect(store.snapshot(review.planId), isNull);
    expect(
      _captureError(() => store.claim(review.planId)).code,
      'plan_not_found_or_expired',
    );
  });

  test('concurrent apply rejection does not consume the waiting plan', () {
    final store = PlanStore(const Duration(seconds: 300));
    final mutationLock = GlobalMutationLock();
    final first = store.insert(BackendOrigin.rust, _plan());
    final waiting = store.insert(BackendOrigin.rust, _plan());

    final active = store.claimForApply(first.planId, mutationLock);
    expect(
      _captureError(
        () => store.claimForApply(waiting.planId, mutationLock),
      ).code,
      'operation_in_progress',
    );
    expect(store.snapshot(waiting.planId)!.state, PlanState.available);

    active.release();
    final retried = store.claimForApply(waiting.planId, mutationLock);
    retried.release();
  });

  test('insertBoundReserved assigns the exact reserved identifier', () {
    final store = PlanStore(const Duration(seconds: 300));
    const binding = PlanBinding(
      ownerId: 'owner',
      sessionId: 'session',
      operationId: 'rfi.process_scans',
      grantIds: ['grant-a'],
    );
    final reservation = store.reservePlanId();
    final stored = store.insertBoundReserved(
      BackendOrigin.rust,
      binding,
      reservation,
      reservation.assignToEmpty(_plan(planId: '')),
    );
    expect(stored.planId, reservation.value);
    expect(store.activePlanCountForTesting, 1);
    expect(store.snapshot(reservation.value)!.binding, binding);
  });

  test('revoke removes only plans bound to the caller', () {
    final store = PlanStore(const Duration(seconds: 300));
    const mine = PlanBinding(
      ownerId: 'owner',
      sessionId: 'session',
      operationId: 'rfi.process_scans',
      grantIds: [],
    );
    final bound = store.insertBound(BackendOrigin.rust, mine, _plan());
    final other = store.insert(BackendOrigin.rust, _plan());

    store.revoke(const CallerIdentity('owner', 'session'));

    expect(store.snapshot(bound.planId), isNull);
    expect(store.snapshot(other.planId), isNotNull);
  });
}

BackendError _captureError(Object? Function() body) {
  try {
    body();
  } on BackendError catch (error) {
    return error;
  }
  fail('expected a BackendError');
}
