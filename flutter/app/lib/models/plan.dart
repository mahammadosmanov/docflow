import 'json.dart';

/// Step kinds of a Plan (`Step.kind`). `wire` is the engine's snake_case name.
enum StepKind {
  mkdir('mkdir'),
  move('move'),
  copy('copy'),
  rename('rename'),
  write('write'),
  workbookUpdate('workbook_update'),
  restoreBackup('restore_backup'),
  print('print'),
  pdfFirstPageReplace('pdf_first_page_replace'),
  pdfPageRotate('pdf_page_rotate'),
  pdfPagesExtract('pdf_pages_extract'),
  replicaPublish('replica_publish'),
  cleanupTemp('cleanup_temp'),
  outlookReplyDraft('outlook_reply_draft');

  const StepKind(this.wire);

  final String wire;

  static StepKind fromWire(String value) => StepKind.values.firstWhere(
    (kind) => kind.wire == value,
    orElse: () => throw FormatException('Unknown StepKind "$value"'),
  );
}

/// One planned effect (`Step`). The Plan is immutable once built; apply runs
/// only this cached data.
final class Step {
  const Step({
    required this.kind,
    required this.src,
    required this.dst,
    required this.noteKey,
    required this.meta,
  });

  factory Step.fromJson(JsonMap json) => Step(
    kind: StepKind.fromWire(jsonString(json, 'kind')),
    src: jsonStringOrNull(json, 'src'),
    dst: jsonStringOrNull(json, 'dst'),
    noteKey: jsonStringOrNull(json, 'noteKey'),
    meta: jsonMap(json['meta'], 'meta'),
  );

  final StepKind kind;
  final String? src;
  final String? dst;
  final String? noteKey;
  final JsonMap meta;

  JsonMap toJson() => {
    'kind': kind.wire,
    'src': src,
    'dst': dst,
    'noteKey': noteKey,
    'meta': meta,
  };
}

/// A localizable warning attached to a Plan or result (`Warning`).
final class Warning {
  const Warning({required this.key, required this.subject});

  factory Warning.fromJson(JsonMap json) => Warning(
    key: jsonString(json, 'key'),
    subject: jsonString(json, 'subject'),
  );

  final String key;
  final String subject;

  JsonMap toJson() => {'key': key, 'subject': subject};
}

/// An immutable preview of an operation (`Plan`).
final class Plan {
  const Plan({
    required this.planId,
    required this.opId,
    required this.steps,
    required this.warnings,
    required this.meta,
  });

  factory Plan.fromJson(JsonMap json) => Plan(
    planId: jsonString(json, 'planId'),
    opId: jsonString(json, 'opId'),
    steps: jsonObjects(json, 'steps', Step.fromJson),
    warnings: jsonObjects(json, 'warnings', Warning.fromJson),
    meta: jsonMap(json['meta'], 'meta'),
  );

  final String planId;
  final String opId;
  final List<Step> steps;
  final List<Warning> warnings;
  final JsonMap meta;

  JsonMap toJson() => {
    'planId': planId,
    'opId': opId,
    'steps': [for (final step in steps) step.toJson()],
    'warnings': [for (final warning in warnings) warning.toJson()],
    'meta': meta,
  };
}

/// Outcome of applying a Plan (`OperationResult`).
final class OperationResult {
  const OperationResult({
    required this.applied,
    required this.failed,
    required this.journalId,
    required this.errors,
    this.meta,
    this.cloudSynced,
  });

  factory OperationResult.fromJson(JsonMap json) => OperationResult(
    applied: jsonInt(json, 'applied'),
    failed: jsonInt(json, 'failed'),
    journalId: jsonString(json, 'journalId'),
    errors: jsonObjects(json, 'errors', Warning.fromJson),
    meta: json['meta'] == null ? null : jsonMap(json['meta'], 'meta'),
    cloudSynced: jsonBoolOrNull(json, 'cloudSynced'),
  );

  final int applied;
  final int failed;
  final String journalId;
  final List<Warning> errors;
  final JsonMap? meta;
  final bool? cloudSynced;

  JsonMap toJson() => withoutNulls({
    'applied': applied,
    'failed': failed,
    'journalId': journalId,
    'errors': [for (final error in errors) error.toJson()],
    'meta': meta,
    'cloudSynced': cloudSynced,
  });
}
