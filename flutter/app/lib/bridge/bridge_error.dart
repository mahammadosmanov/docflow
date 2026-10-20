import '../models/json.dart';

/// Failure reported by the engine (`BridgeError` in bridge.ts). [code] is the
/// engine's stable error code when it has one; [message] is shown to the user
/// through `humanError`.
final class BridgeError implements Exception {
  const BridgeError({this.code, required this.message});

  /// Code used when the engine sends an error object without one.
  static const String defaultCode = 'backend_request_failed';

  final String? code;
  final String message;

  @override
  String toString() => message;
}

/// Converts anything thrown by an [Engine] into a [BridgeError], mirroring
/// `asError` in bridge.ts. Bridge errors pass through unchanged. An engine
/// error object with a `message` keeps its `code`, with a default when absent.
/// Anything else becomes a BridgeError carrying its text as the message.
BridgeError asBridgeError(Object error) {
  if (error is BridgeError) return error;
  if (error is Map) {
    final json = jsonMap(error);
    final message = json['message'];
    if (message is String) {
      final code = json['code'];
      return BridgeError(
        code: code is String ? code : BridgeError.defaultCode,
        message: message,
      );
    }
  }
  if (error is String) return BridgeError(message: error);
  return BridgeError(message: _messageOf(error));
}

String _messageOf(Object error) {
  final text = error.toString();
  const exceptionPrefix = 'Exception: ';
  return text.startsWith(exceptionPrefix)
      ? text.substring(exceptionPrefix.length)
      : text;
}
