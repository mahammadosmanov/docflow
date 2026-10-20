/// Port of src-tauri/src/parallel.rs using Dart isolates instead of rayon.
///
/// Work is partitioned round-robin over `workerCount` short-lived isolates.
/// Results are returned in input order. Each item result is also reported
/// to the caller in completion order, matching `bounded_map_stream`.
///
/// Constraints inherited from Dart isolate messaging: [map] and the item /
/// result values must be sendable (top-level/static functions, or closures
/// that capture only sendable values).
library;

import 'dart:async';
import 'dart:io';
import 'dart:isolate';
import 'dart:math' as math;

const int maxAnalysisWorkers = 4;

/// Port of `worker_count`: available processors minus one, clamped to
/// 1..=4, and never more than the item count.
int workerCount(int itemCount) {
  final cpus = Platform.numberOfProcessors;
  final workers = (cpus - 1).clamp(1, maxAnalysisWorkers);
  return math.min(workers, math.max(itemCount, 1));
}

/// Port of `bounded_map`. Returns `(workers, results)` in input order.
Future<(int, List<R>)> boundedMap<T, R>(
  List<T> items,
  R Function(T item) map,
) async {
  final (workers, results) = await boundedMapStream<T, R>(
    items,
    map,
    (_, _) {},
  );
  return (workers, results);
}

/// Port of `bounded_map_stream`.
///
/// [completed] is invoked on the calling isolate once per item, in completion
/// order, with the item index and its result. If it throws, no further
/// callbacks run, but the remaining work is still drained (as the Rust
/// version does when the worker joins) and the first error is rethrown.
Future<(int, List<R>)> boundedMapStream<T, R>(
  List<T> items,
  R Function(T item) map,
  void Function(int index, R result) completed,
) async {
  final workers = workerCount(items.length);
  if (items.isEmpty) {
    return (workers, <R>[]);
  }

  final ordered = List<R?>.filled(items.length, null);
  final events = ReceivePort();
  final errors = ReceivePort();
  final finished = Completer<void>();
  // One partition per worker that has at least one item.
  final expected = math.min(workers, items.length);
  var doneWorkers = 0;
  var panicked = false;
  Object? completedError;
  StackTrace? completedStack;

  void markWorkerDone() {
    doneWorkers++;
    if (doneWorkers == expected && !finished.isCompleted) {
      finished.complete();
    }
  }

  events.listen((message) {
    final event = message as List<Object?>;
    switch (event[0]) {
      case 'item':
        final index = event[1]! as int;
        final result = event[2] as R;
        ordered[index] = result;
        if (completedError == null) {
          try {
            completed(index, result);
          } catch (error, stack) {
            completedError = error;
            completedStack = stack;
          }
        }
      case 'panic':
        panicked = true;
      case 'done':
        markWorkerDone();
    }
  });
  errors.listen((message) {
    // Uncaught isolate error: the worker never reached its 'done' message.
    panicked = true;
    markWorkerDone();
  });

  try {
    for (var worker = 0; worker < workers; worker++) {
      final indices = <int>[
        for (var index = worker; index < items.length; index += workers) index,
      ];
      if (indices.isEmpty) {
        continue;
      }
      await Isolate.spawn<_WorkerArgs<T, R>>(
        _runWorker<T, R>,
        _WorkerArgs<T, R>(
          events.sendPort,
          [for (final index in indices) index],
          [for (final index in indices) items[index]],
          map,
        ),
        onError: errors.sendPort,
      );
    }
    await finished.future;
  } finally {
    events.close();
    errors.close();
  }

  if (completedError != null) {
    Error.throwWithStackTrace(completedError!, completedStack!);
  }
  if (panicked) {
    throw StateError('analysis worker pool panicked');
  }
  final results = <R>[];
  for (final result in ordered) {
    if (result == null) {
      throw StateError('analysis worker did not return a result');
    }
    results.add(result);
  }
  return (workers, results);
}

class _WorkerArgs<T, R> {
  _WorkerArgs(this.port, this.indices, this.items, this.map);

  final SendPort port;
  final List<int> indices;
  final List<T> items;
  final R Function(T item) map;
}

void _runWorker<T, R>(_WorkerArgs<T, R> args) {
  try {
    for (var position = 0; position < args.items.length; position++) {
      args.port.send(<Object?>[
        'item',
        args.indices[position],
        args.map(args.items[position]),
      ]);
    }
  } catch (_) {
    args.port.send(<Object?>['panic']);
  } finally {
    args.port.send(<Object?>['done']);
  }
}
