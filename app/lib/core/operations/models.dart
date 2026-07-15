// Sidecar'dan gelen JSON'un Dart karsiliklari. Bu dosya operation'a ozel
// HICBIR SEY bilmez — sadece sozlesme sekilleri. UI bunlardan uretilir.

enum ParamKind { folder, file, text, bool, choice, int, unknown }

ParamKind _kindFrom(String s) {
  switch (s) {
    case 'folder':
      return ParamKind.folder;
    case 'file':
      return ParamKind.file;
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
  });

  final String? planId;
  final String opId;
  final List<PlanStep> steps;
  final List<PlanWarning> warnings;

  /// Uygulanabilir adim = dosyayi tasiyan/kopyalayan (mkdir tek basina degil).
  bool get hasActionableSteps =>
      steps.any((s) => s.kind == 'move' || s.kind == 'copy' || s.kind == 'rename');

  factory Plan.fromJson(Map<String, dynamic> j) => Plan(
        planId: j['planId'] as String?,
        opId: j['opId'] as String,
        steps: (j['steps'] as List? ?? [])
            .map((e) => PlanStep.fromJson(e as Map<String, dynamic>))
            .toList(),
        warnings: (j['warnings'] as List? ?? [])
            .map((e) => PlanWarning.fromJson(e as Map<String, dynamic>))
            .toList(),
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
  Progress({required this.phase, required this.done, required this.total});
  final String phase;
  final int done;
  final int total;
  double get fraction => total == 0 ? 0 : done / total;
}
