import 'json.dart';

/// A progress notification streamed while a command runs (`ProgressEvent`).
final class ProgressEvent {
  const ProgressEvent({
    required this.phase,
    required this.done,
    required this.total,
    this.subject,
  });

  factory ProgressEvent.fromJson(JsonMap json) => ProgressEvent(
    phase: jsonString(json, 'phase'),
    done: jsonInt(json, 'done'),
    total: jsonInt(json, 'total'),
    subject: jsonStringOrNull(json, 'subject'),
  );

  final String phase;
  final int done;
  final int total;
  final String? subject;

  JsonMap toJson() => withoutNulls({
    'phase': phase,
    'done': done,
    'total': total,
    'subject': subject,
  });
}
