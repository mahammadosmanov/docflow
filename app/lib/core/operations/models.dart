// Sidecar'dan gelen JSON'un Dart karsiliklari. Bu dosya operation'a ozel
// HICBIR SEY bilmez — sadece sozlesme sekilleri. UI bunlardan uretilir.

enum ParamKind { folder, file, files, text, bool, choice, int, unknown }

ParamKind _kindFrom(String s) {
  switch (s) {
    case 'folder':
      return ParamKind.folder;
    case 'file':
      return ParamKind.file;
    case 'files':
      return ParamKind.files;
    case 'text':
      return ParamKind.text;
    case 'bool':
      return ParamKind.bool;
    case 'choice':
      return ParamKind.choice;
    case 'int':
      return ParamKind.int;
    default:
      return ParamKind.unknown;
  }
}

class ParamDef {
  ParamDef({
    required this.name,
    required this.kind,
    required this.labelKey,
    required this.required,
    this.defaultValue,
    this.choices = const [],
    this.helpKey,
  });

  final String name;
  final ParamKind kind;
  final String labelKey;
  final bool required;
  final Object? defaultValue;
  final List<String> choices;
  final String? helpKey;

  factory ParamDef.fromJson(Map<String, dynamic> j) => ParamDef(
    name: j['name'] as String,
    kind: _kindFrom(j['kind'] as String),
    labelKey: j['labelKey'] as String,
    required: (j['required'] as bool?) ?? true,
    defaultValue: j['default'],
    choices: (j['choices'] as List?)?.cast<String>() ?? const [],
    helpKey: j['helpKey'] as String?,
  );
}

class OperationDef {
  OperationDef({
    required this.id,
    required this.titleKey,
    required this.descriptionKey,
    required this.params,
  });

  final String id;
  final String titleKey;
  final String descriptionKey;
  final List<ParamDef> params;

  factory OperationDef.fromJson(Map<String, dynamic> j) => OperationDef(
    id: j['id'] as String,
    titleKey: j['titleKey'] as String,
    descriptionKey: j['descriptionKey'] as String,
    params: (j['schema'] as List)
        .map((e) => ParamDef.fromJson(e as Map<String, dynamic>))
        .toList(),
  );
}

class PlanStep {
  PlanStep({
    required this.kind,
    this.src,
    this.dst,
    this.noteKey,
    this.meta = const {},
  });

  final String kind; // mkdir | move | copy | rename | write
  final String? src;
  final String? dst;
  final String? noteKey;
  final Map<String, dynamic> meta;

  double? get confidence => (meta['confidence'] as num?)?.toDouble();
  String? get code => meta['code'] as String?;

  factory PlanStep.fromJson(Map<String, dynamic> j) => PlanStep(
    kind: j['kind'] as String,
    src: j['src'] as String?,
    dst: j['dst'] as String?,
    noteKey: j['noteKey'] as String?,
    meta: (j['meta'] as Map?)?.cast<String, dynamic>() ?? const {},
  );
}

class PlanWarning {
  PlanWarning({required this.key, this.subject = ''});

  final String key;
  final String subject;

  factory PlanWarning.fromJson(Map<String, dynamic> j) => PlanWarning(
    key: j['key'] as String,
    subject: (j['subject'] as String?) ?? '',
  );
}

class Plan {
  Plan({
    required this.planId,
    required this.opId,
    required this.steps,
    required this.warnings,
    this.meta = const {},
  });

  final String? planId;
  final String opId;
  final List<PlanStep> steps;
  final List<PlanWarning> warnings;
  final Map<String, dynamic> meta;

  /// Uygulanabilir adim = dosyayi tasiyan/kopyalayan (mkdir tek basina degil).
  bool get hasActionableSteps => steps.any(
    (s) =>
        s.kind == 'move' ||
        s.kind == 'copy' ||
        s.kind == 'rename' ||
        s.kind == 'workbook_update' ||
        s.kind == 'print',
  );

  Map<String, dynamic>? get editor {
    final raw = meta['editor'];
    return raw is Map ? raw.cast<String, dynamic>() : null;
  }

  factory Plan.fromJson(Map<String, dynamic> j) => Plan(
    planId: j['planId'] as String?,
    opId: j['opId'] as String,
    steps: (j['steps'] as List? ?? [])
        .map((e) => PlanStep.fromJson(e as Map<String, dynamic>))
        .toList(),
    warnings: (j['warnings'] as List? ?? [])
        .map((e) => PlanWarning.fromJson(e as Map<String, dynamic>))
        .toList(),
    meta: (j['meta'] as Map?)?.cast<String, dynamic>() ?? const {},
  );
}

class ApplyResult {
  ApplyResult({
    required this.applied,
    required this.failed,
    required this.journalId,
    required this.errors,
  });

  final int applied;
  final int failed;
  final String journalId;
  final List<PlanWarning> errors;

  factory ApplyResult.fromJson(Map<String, dynamic> j) => ApplyResult(
    applied: (j['applied'] as num).toInt(),
    failed: (j['failed'] as num).toInt(),
    journalId: (j['journalId'] as String?) ?? '',
    errors: (j['errors'] as List? ?? [])
        .map((e) => PlanWarning.fromJson(e as Map<String, dynamic>))
        .toList(),
  );
}

class Progress {
  Progress({
    required this.phase,
    required this.done,
    required this.total,
    this.subject = '',
  });
  final String phase;
  final int done;
  final int total;
  final String subject;
  double get fraction => total == 0 ? 0 : done / total;
}

/// _review/ klasorundeki bir dosya (review_scan ciktisi).
class ReviewItem {
  ReviewItem({
    required this.name,
    required this.path,
    required this.code,
    required this.confidence,
    required this.target,
    required this.targetExists,
  });

  final String name;
  final String path;
  final String code; // bos = kod okunamadi -> "Put" devre disi
  final double confidence;
  final String? target;
  final bool targetExists;

  bool get hasCode => code.isNotEmpty;

  factory ReviewItem.fromJson(Map<String, dynamic> j) => ReviewItem(
    name: j['name'] as String,
    path: j['path'] as String,
    code: (j['code'] as String?) ?? '',
    confidence: (j['confidence'] as num?)?.toDouble() ?? 0.0,
    target: j['target'] as String?,
    targetExists: (j['targetExists'] as bool?) ?? false,
  );
}

/// Bir gecmis kaydi (journal ozeti). history_list ciktisi.
class HistoryEntry {
  HistoryEntry({
    required this.journalId,
    required this.opId,
    required this.ts,
    required this.total,
    required this.moves,
  });

  final String journalId;
  final String opId;
  final String ts; // ISO-8601 UTC
  final int total;
  final int moves;

  DateTime? get when => DateTime.tryParse(ts)?.toLocal();

  factory HistoryEntry.fromJson(Map<String, dynamic> j) => HistoryEntry(
    journalId: (j['journalId'] as String?) ?? '',
    opId: (j['opId'] as String?) ?? '',
    ts: (j['ts'] as String?) ?? '',
    total: (j['total'] as num?)?.toInt() ?? 0,
    moves: (j['moves'] as num?)?.toInt() ?? 0,
  );
}
