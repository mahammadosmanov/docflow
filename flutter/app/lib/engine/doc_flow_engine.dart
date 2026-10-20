// The Dart `Engine` for DocFlow: routes every frontend command through the
// closed `BackendRouter` (backend.rs `frontend_call`). Unknown command names are
// rejected before any work runs.

import 'auth.dart' as auth;
import 'backend.dart';
import 'engine.dart';
import 'errors.dart';
import '../models/json.dart' show JsonMap;

/// Engine that serves `frontend_call` for one app session.
final class DocFlowEngine extends Engine {
  DocFlowEngine({required this.router});

  final BackendRouter router;

  /// Recent error codes, newest first, as reported by diagnostics.
  List<String> get recentErrorCodes => router.recentErrorCodes.snapshot();

  @override
  Future<Object?> call(
    String command,
    JsonMap params, {
    ProgressHandler? onProgress,
  }) async {
    final method = BackendMethod.fromWire(command);
    if (method == null) {
      throw BackendError.protocol('unknown frontend method');
    }
    try {
      return await router.call(method, params, onProgress);
    } on BackendError catch (error) {
      router.recentErrorCodes.record(error.code);
      rethrow;
    } on auth.BackendError catch (error) {
      final converted = BackendError(error.code, error.message);
      router.recentErrorCodes.record(converted.code);
      throw converted;
    } on Object {
      const failure = BackendError(
        'backend_request_failed',
        'native backend task failed',
      );
      router.recentErrorCodes.record(failure.code);
      throw failure;
    }
  }
}
