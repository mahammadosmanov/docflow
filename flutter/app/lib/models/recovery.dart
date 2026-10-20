import 'json.dart';
import 'print.dart';

/// Why a journal needs recovery (`RecoveryEntry.disposition`).
enum RecoveryDisposition {
  noEffect('no_effect'),
  outcomeUnknown('outcome_unknown');

  const RecoveryDisposition(this.wire);

  final String wire;

  static RecoveryDisposition fromWire(String value) =>
      RecoveryDisposition.values.firstWhere(
        (disposition) => disposition.wire == value,
        orElse: () => throw FormatException('Unknown disposition "$value"'),
      );
}

/// A journal that was interrupted and needs a decision (`RecoveryEntry`).
final class RecoveryEntry {
  const RecoveryEntry({
    required this.journalId,
    required this.disposition,
    this.restoreOriginalAvailable,
  });

  factory RecoveryEntry.fromJson(JsonMap json) => RecoveryEntry(
    journalId: jsonString(json, 'journalId'),
    disposition: RecoveryDisposition.fromWire(jsonString(json, 'disposition')),
    restoreOriginalAvailable: jsonBoolOrNull(json, 'restoreOriginalAvailable'),
  );

  final String journalId;
  final RecoveryDisposition disposition;
  final bool? restoreOriginalAvailable;

  JsonMap toJson() => withoutNulls({
    'journalId': journalId,
    'disposition': disposition.wire,
    'restoreOriginalAvailable': restoreOriginalAvailable,
  });
}

/// Result of `recovery/list` (`RecoveryPayload`).
final class RecoveryPayload {
  const RecoveryPayload({required this.journals, required this.printQueue});

  factory RecoveryPayload.fromJson(JsonMap json) => RecoveryPayload(
    journals: jsonObjects(json, 'journals', RecoveryEntry.fromJson),
    printQueue: json['printQueue'] == null
        ? null
        : PrintQueueSummary.fromJson(jsonObject(json, 'printQueue')),
  );

  final List<RecoveryEntry> journals;
  final PrintQueueSummary? printQueue;

  JsonMap toJson() => {
    'journals': [for (final journal in journals) journal.toJson()],
    'printQueue': printQueue?.toJson(),
  };
}
