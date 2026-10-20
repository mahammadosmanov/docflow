// Port of the inline tests in src-tauri/src/progress.rs.
import 'package:flutter_test/flutter_test.dart';
import 'package:docflow/engine/errors.dart';
import 'package:docflow/engine/progress.dart';

void main() {
  test('request cancellation is observable at stage and pre-effect checkpoints', () {
    final registry = CancellationRegistry();
    final stageToken = registry.register('request-1');
    // Rust `stage_token.clone()`: Dart tokens share the same cancellation flag.
    final preEffectToken = stageToken;
    expect(preEffectToken.sameState(stageToken), isTrue);

    stageToken.checkpoint();
    expect(registry.cancel('request-1'), isTrue);
    expect(
      () => stageToken.checkpoint(),
      throwsA(
        isA<BackendError>().having(
          (error) => error.code,
          'code',
          'operation_cancelled',
        ),
      ),
    );
    expect(
      () => preEffectToken.checkpoint(),
      throwsA(
        isA<BackendError>().having(
          (error) => error.code,
          'code',
          'operation_cancelled',
        ),
      ),
    );

    registry.finish('request-1');
    expect(registry.cancel('request-1'), isFalse);
    expect(registry.cancel('unknown-request'), isFalse);
  });

  test(
    'global mutation lock rejects concurrent apply and releases on release',
    () {
      final firstLock = GlobalMutationLock();
      final secondLock = firstLock.clone();
      final permit = firstLock.tryAcquire();

      expect(
        () => secondLock.tryAcquire(),
        throwsA(
          isA<BackendError>().having(
            (error) => error.code,
            'code',
            'operation_in_progress',
          ),
        ),
      );

      permit.release();
      permit.release();
      expect(() => secondLock.tryAcquire().release(), returnsNormally);
    },
  );

  test('withPermit releases the lock even when the body throws', () async {
    final lock = GlobalMutationLock();
    await expectLater(
      lock.withPermit<void>(() async => throw StateError('body failed')),
      throwsA(isA<StateError>()),
    );
    expect(() => lock.tryAcquire().release(), returnsNormally);
  });
}
