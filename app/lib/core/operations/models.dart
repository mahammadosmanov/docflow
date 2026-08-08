// Sidecar'dan gelen JSON'un Dart karsiliklari. Bu dosya operation'a ozel
// HICBIR SEY bilmez — sadece sozlesme sekilleri. UI bunlardan uretilir.

enum ParamKind {
  folder,
  file,
  files,
  text,
  bool,
  choice,
  int,
  longText,
  secret,
  sheetCounts,
  unknown,
}

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
    case 'longText':
      return ParamKind.longText;
    case 'secret':
      return ParamKind.secret;
    case 'sheetCounts':
      return ParamKind.sheetCounts;
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
    this.choiceLabels = const {},
    this.visibleWhen = const {},
    this.helpKey,
    this.itemChoices = const [],
    this.itemChoiceLabels = const {},
    this.itemDefault,
  });

  final String name;
  final ParamKind kind;
  final String labelKey;
  final bool required;
  final Object? defaultValue;
  final List<String> choices;
  final Map<String, String> choiceLabels;
  final Map<String, dynamic> visibleWhen;
  final String? helpKey;
  final List<String> itemChoices;
  final Map<String, String> itemChoiceLabels;
  final String? itemDefault;

  factory ParamDef.fromJson(Map<String, dynamic> j) => ParamDef(
    name: j['name'] as String,
    kind: _kindFrom(j['kind'] as String),
    labelKey: j['labelKey'] as String,
    required: (j['required'] as bool?) ?? true,
    defaultValue: j['default'],
    choices: (j['choices'] as List?)?.cast<String>() ?? const [],
    choiceLabels:
        (j['choiceLabels'] as Map?)?.map(
          (key, value) => MapEntry(key.toString(), value.toString()),
        ) ??
        const {},
    visibleWhen:
        (j['visibleWhen'] as Map?)?.cast<String, dynamic>() ?? const {},
    helpKey: j['helpKey'] as String?,
    itemChoices: (j['itemChoices'] as List?)?.cast<String>() ?? const [],
    itemChoiceLabels:
        (j['itemChoiceLabels'] as Map?)?.map(
          (key, value) => MapEntry(key.toString(), value.toString()),
        ) ??
        const {},
    itemDefault: j['itemDefault'] as String?,
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
    this.meta = const {},
  });

  final int applied;
  final int failed;
  final String journalId;
  final List<PlanWarning> errors;
  final Map<String, dynamic> meta;

  GuidedSession? get guidedSession {
    final raw = meta['guidedSession'];
    return raw is Map
        ? GuidedSession.fromJson(raw.cast<String, dynamic>())
        : null;
  }

  factory ApplyResult.fromJson(Map<String, dynamic> j) => ApplyResult(
    applied: (j['applied'] as num).toInt(),
    failed: (j['failed'] as num).toInt(),
    journalId: (j['journalId'] as String?) ?? '',
    errors: (j['errors'] as List? ?? [])
        .map((e) => PlanWarning.fromJson(e as Map<String, dynamic>))
        .toList(),
    meta: (j['meta'] as Map?)?.cast<String, dynamic>() ?? const {},
  );
}

class GuidedSession {
  GuidedSession({
    required this.sessionId,
    required this.revision,
    required this.opId,
    required this.status,
    required this.current,
    required this.total,
    required this.currentJob,
    required this.lastError,
  });

  final String sessionId;
  final int revision;
  final String opId;
  final String status;
  final int current;
  final int total;
  final Map<String, dynamic>? currentJob;
  final String lastError;

  bool get isPaused => status == 'paused';
  bool get isComplete => status == 'completed';
  int get currentNumber => isComplete ? total : current + 1;

  factory GuidedSession.fromJson(Map<String, dynamic> j) {
    final rawSessionId = j['sessionId'];
    final rawOpId = j['opId'];
    final rawRevision = j['revision'];
    final rawStatus = j['status'];
    final rawCurrent = j['current'];
    final rawTotal = j['total'];
    final rawLastError = j['lastError'];
    final sessionId = rawSessionId is String ? rawSessionId : '';
    final opId = rawOpId is String ? rawOpId : '';
    final revision = rawRevision is int ? rawRevision : -1;
    final status = rawStatus is String ? rawStatus : '';
    final current = rawCurrent is int ? rawCurrent : -1;
    final total = rawTotal is int ? rawTotal : -1;
    final currentJob = j['currentJob'] is Map
        ? (j['currentJob'] as Map).cast<String, dynamic>()
        : null;
    final currentPath = currentJob?['path'];
    final currentPaper = currentJob?['paperSize'];
    final absoluteCurrentPath =
        currentPath is String &&
        (currentPath.startsWith('/') ||
            currentPath.startsWith(r'\\') ||
            RegExp(r'^[A-Za-z]:[\\/]').hasMatch(currentPath));
    final validCurrentJob =
        currentJob != null &&
        absoluteCurrentPath &&
        currentPaper is String &&
        const {
          'A0',
          'A1',
          'A2',
          'A3',
          'A4',
          'A5',
          'LETTER',
          'LEGAL',
        }.contains(currentPaper) &&
        currentJob['copies'] is int &&
        (currentJob['copies'] as int) >= 1 &&
        (currentJob['copies'] as int) <= 99;
    if (sessionId.isEmpty ||
        opId != 'files.batch_print' ||
        revision < 0 ||
        (rawLastError != null && rawLastError is! String) ||
        rawCurrent is! int ||
        rawTotal is! int ||
        !const {'active', 'paused', 'completed'}.contains(status) ||
        current < 0 ||
        total < 0 ||
        current > total ||
        (status == 'completed' && current != total) ||
        (status == 'completed' && currentJob != null) ||
        (status != 'completed' &&
            (total == 0 || current >= total || !validCurrentJob))) {
      throw const FormatException('invalid guided session');
    }
    return GuidedSession(
      sessionId: sessionId,
      revision: revision,
      opId: opId,
      status: status,
      current: current,
      total: total,
      currentJob: currentJob,
      lastError: rawLastError as String? ?? '',
    );
  }
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
