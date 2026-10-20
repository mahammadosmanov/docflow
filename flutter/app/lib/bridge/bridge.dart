import '../engine/engine.dart';
import '../models/models.dart';
import 'bridge_error.dart';
import 'commands.dart';

export 'bridge_error.dart';
export 'commands.dart';

/// Typed client for the engine (`Bridge` / `TauriBridge` in bridge.ts).
///
/// [call] takes a command from [FrontendCommands], encodes its params, runs it
/// on the [Engine] and decodes the result. The React bridge sent progress over
/// a Tauri channel; here it is the [ProgressHandler] argument.
final class Bridge {
  Bridge(this._engine);

  final Engine _engine;

  Future<void> start() => _engine.start();

  Future<void> stop() => _engine.stop();

  Future<R> call<P, R>(
    FrontendCommand<P, R> command, {
    P? params,
    ProgressHandler? onProgress,
  }) async {
    final JsonMap payload = command.encode(params);
    final Object? raw;
    try {
      raw = await _engine.call(command.name, payload, onProgress: onProgress);
    } catch (error) {
      throw asBridgeError(error);
    }
    return command.decode(raw);
  }
}
