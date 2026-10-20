// Tests for src-tauri/src/parallel.rs behaviour on Dart isolates.
import 'package:flutter_test/flutter_test.dart';
import 'package:docflow/engine/parallel.dart';

int _double(int value) => value * 2;

int _failOnSeven(int value) {
  if (value == 7) {
    throw StateError('boom');
  }
  return value;
}

void main() {
  test(
    'worker count is clamped to the analysis pool bounds and item count',
    () {
      expect(workerCount(0), inInclusiveRange(1, maxAnalysisWorkers));
      expect(workerCount(1), 1);
      expect(workerCount(1000), inInclusiveRange(1, maxAnalysisWorkers));
    },
  );

  test('bounded map returns results in input order', () async {
    final items = List<int>.generate(101, (index) => index);
    final (workers, results) = await boundedMap<int, int>(items, _double);
    expect(workers, inInclusiveRange(1, maxAnalysisWorkers));
    expect(results, [for (final item in items) item * 2]);
  });

  test('bounded map handles an empty input', () async {
    final (_, results) = await boundedMap<int, int>(const [], _double);
    expect(results, isEmpty);
  });

  test(
    'bounded map stream reports every index once and returns ordered results',
    () async {
      final items = List<int>.generate(40, (index) => index);
      final seen = <int>[];
      final (_, results) = await boundedMapStream<int, int>(items, _double, (
        index,
        result,
      ) {
        expect(result, index * 2);
        seen.add(index);
      });
      expect(seen.toSet(), {for (final item in items) item});
      expect(seen.length, items.length);
      expect(results, [for (final item in items) item * 2]);
    },
  );

  test(
    'a failing completion callback rethrows after the work drains',
    () async {
      final items = List<int>.generate(20, (index) => index);
      await expectLater(
        boundedMapStream<int, int>(items, _double, (index, _) {
          if (index == 3) {
            throw StateError('callback failed');
          }
        }),
        throwsA(isA<StateError>()),
      );
    },
  );

  test('a panicking map surfaces the analysis pool panic message', () async {
    await expectLater(
      boundedMap<int, int>(
        List<int>.generate(10, (index) => index),
        _failOnSeven,
      ),
      throwsA(
        isA<StateError>().having(
          (error) => error.message,
          'message',
          'analysis worker pool panicked',
        ),
      ),
    );
  });
}
