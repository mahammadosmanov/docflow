import 'json.dart';
import 'plan.dart';

/// One journal entry in the history list (`HistoryRow`).
final class HistoryRow {
  const HistoryRow({
    required this.when,
    required this.opId,
    required this.moves,
    required this.failed,
    required this.journalId,
    required this.steps,
    required this.cloudId,
    required this.undone,
    required this.undoAvailable,
    required this.fromCloud,
    required this.userEmail,
  });

  factory HistoryRow.fromJson(JsonMap json) => HistoryRow(
    when: jsonString(json, 'when'),
    opId: jsonString(json, 'opId'),
    moves: jsonInt(json, 'moves'),
    failed: jsonInt(json, 'failed'),
    journalId: jsonString(json, 'journalId'),
    steps: json['steps'] == null
        ? null
        : jsonObjects(json, 'steps', Step.fromJson),
    cloudId: jsonStringOrNull(json, 'cloudId'),
    undone: jsonBool(json, 'undone'),
    undoAvailable: jsonBool(json, 'undoAvailable'),
    fromCloud: jsonBool(json, 'fromCloud'),
    userEmail: jsonString(json, 'userEmail'),
  );

  final String when;
  final String opId;
  final int moves;
  final int failed;
  final String journalId;
  final List<Step>? steps;
  final String? cloudId;
  final bool undone;
  final bool undoAvailable;
  final bool fromCloud;
  final String userEmail;

  JsonMap toJson() => {
    'when': when,
    'opId': opId,
    'moves': moves,
    'failed': failed,
    'journalId': journalId,
    'steps': steps == null ? null : [for (final step in steps!) step.toJson()],
    'cloudId': cloudId,
    'undone': undone,
    'undoAvailable': undoAvailable,
    'fromCloud': fromCloud,
    'userEmail': userEmail,
  };
}

/// Result of `history/list` (`HistoryPayload`).
final class HistoryPayload {
  const HistoryPayload({required this.online, required this.rows});

  factory HistoryPayload.fromJson(JsonMap json) => HistoryPayload(
    online: jsonBool(json, 'online'),
    rows: jsonObjects(json, 'rows', HistoryRow.fromJson),
  );

  final bool online;
  final List<HistoryRow> rows;

  JsonMap toJson() => {
    'online': online,
    'rows': [for (final row in rows) row.toJson()],
  };
}
