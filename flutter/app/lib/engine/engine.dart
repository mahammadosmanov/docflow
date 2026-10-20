import '../bridge/bridge_error.dart';
import '../models/models.dart';

/// Receives progress notifications while a command runs (`ProgressEventHandler`).
typedef ProgressHandler = void Function(ProgressEvent event);

/// The single entry point from the UI into the engine. The Dart port of the
/// Rust `backend::frontend_call(command, params)` implements this class; the
/// UI never reaches files, networks, shells or printers except through it.
///
/// [call] receives a command name from `FrontendCommands` (see
/// lib/bridge/commands.dart) and JSON-ready [params]. It completes with the
/// JSON result, which the bridge decodes into the typed model. Failures are
/// thrown; the bridge converts them to `BridgeError`, keeping the engine's
/// `code` and message, so that `humanError` can localize them.
///
/// [onProgress] is called for each progress event the command emits, in order,
/// until the returned future completes.
abstract class Engine {
  const Engine();

  Future<Object?> call(
    String command,
    JsonMap params, {
    ProgressHandler? onProgress,
  });

  /// Starts engine resources. Defaults to a no-op for engines that initialize
  /// lazily behind [call].
  Future<void> start() async {}

  /// Releases engine resources. Defaults to a no-op.
  Future<void> stop() async {}
}

/// Engine used when the composition root does not supply one. Every command
/// fails with `engine_not_configured`, so the shell reports the missing engine
/// through its bootstrap failure view instead of guessing at results.
final class UnconfiguredEngine extends Engine {
  const UnconfiguredEngine();

  @override
  Future<Object?> call(
    String command,
    JsonMap params, {
    ProgressHandler? onProgress,
  }) => Future.error(
    const BridgeError(
      code: 'engine_not_configured',
      message: 'engine_not_configured: no engine is connected',
    ),
  );
}
