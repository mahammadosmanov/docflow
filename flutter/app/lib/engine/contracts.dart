/// Port of src-tauri/src/contracts.rs: protocol-1 wire types.
///
/// Every type exposes `fromJson` / `toJson` producing the same camelCase
/// keys, null-vs-omitted behaviour and default values as the serde derives:
/// - `Option` fields serialize as `null` when absent (unless marked skip).
/// - `ParamDef.remember` defaults to true and is omitted when true.
/// - `OperationDef.supportsPresets` defaults to false when missing.
/// - `ProgressEvent.subject` defaults to "" when missing.
library;

import 'path_selection.dart';

typedef JsonObject = Map<String, Object?>;

// ---- JSON helpers (strict on required keys, lenient on Option keys) -------

Object? _req(JsonObject map, String key) {
  if (!map.containsKey(key)) {
    throw FormatException('missing field `$key`');
  }
  return map[key];
}

String _str(JsonObject map, String key) => _req(map, key)! as String;

String? _optStr(JsonObject map, String key) => map[key] as String?;

int _int(JsonObject map, String key) => (_req(map, key)! as num).toInt();

double _dbl(JsonObject map, String key) => (_req(map, key)! as num).toDouble();

bool _bool(JsonObject map, String key) => _req(map, key)! as bool;

JsonObject _obj(JsonObject map, String key) =>
    Map<String, Object?>.from(_req(map, key)! as Map);

List<String> _strList(JsonObject map, String key) =>
    (_req(map, key)! as List).cast<String>().toList();

Map<String, String> _strMap(JsonObject map, String key) =>
    Map<String, String>.from(_req(map, key)! as Map);

JsonObject? _optObj(JsonObject map, String key) {
  final value = map[key];
  return value == null ? null : Map<String, Object?>.from(value as Map);
}

JsonObject _asObject(Object? value) => Map<String, Object?>.from(value! as Map);

// ---- RFI print find (request is deserialize-only; result is serialize-only) --

/// Port of `RfiPrintFindRequest` (`deny_unknown_fields`).
class RfiPrintFindRequest {
  const RfiPrintFindRequest({
    required this.root,
    required this.numbers,
    required this.firstPage,
    required this.sketch,
  });

  static const Set<String> _knownKeys = {
    'root',
    'numbers',
    'firstPage',
    'sketch',
  };

  final PathSelection root;
  final List<String> numbers;
  final bool firstPage;
  final bool sketch;

  factory RfiPrintFindRequest.fromJson(JsonObject json) {
    final unknown = json.keys.where((key) => !_knownKeys.contains(key));
    if (unknown.isNotEmpty) {
      throw FormatException('unknown field `${unknown.first}`');
    }
    return RfiPrintFindRequest(
      root: PathSelection.fromJson(_obj(json, 'root')),
      numbers: _strList(json, 'numbers'),
      firstPage: _bool(json, 'firstPage'),
      sketch: _bool(json, 'sketch'),
    );
  }
}

class RfiPrintCandidate {
  const RfiPrintCandidate({
    required this.requested,
    required this.rfiCode,
    required this.documentType,
    required this.displayPath,
    required this.selection,
    required this.selected,
  });

  final String requested;
  final String rfiCode;
  final String documentType;
  final String displayPath;
  final PathSelection selection;
  final bool selected;

  JsonObject toJson() => {
    'requested': requested,
    'rfiCode': rfiCode,
    'documentType': documentType,
    'displayPath': displayPath,
    'selection': selection.toJson(),
    'selected': selected,
  };
}

class RfiPrintResolution {
  const RfiPrintResolution({
    required this.requested,
    required this.rfiCodes,
    required this.missingFirstPage,
    required this.missingSketch,
    required this.ambiguous,
  });

  final String requested;
  final List<String> rfiCodes;
  final bool missingFirstPage;
  final bool missingSketch;
  final bool ambiguous;

  JsonObject toJson() => {
    'requested': requested,
    'rfiCodes': rfiCodes,
    'missingFirstPage': missingFirstPage,
    'missingSketch': missingSketch,
    'ambiguous': ambiguous,
  };
}

class RfiPrintFindResult {
  const RfiPrintFindResult({
    required this.candidates,
    required this.resolutions,
    required this.visited,
    required this.truncated,
  });

  final List<RfiPrintCandidate> candidates;
  final List<RfiPrintResolution> resolutions;
  final int visited;
  final bool truncated;

  JsonObject toJson() => {
    'candidates': [for (final candidate in candidates) candidate.toJson()],
    'resolutions': [for (final resolution in resolutions) resolution.toJson()],
    'visited': visited,
    'truncated': truncated,
  };
}

// ---- Operations, parameters, presets ------------------------------------

class OperationPreset {
  const OperationPreset({
    required this.id,
    required this.name,
    required this.values,
  });

  final String id;
  final String name;
  final JsonObject values;

  factory OperationPreset.fromJson(JsonObject json) => OperationPreset(
    id: _str(json, 'id'),
    name: _str(json, 'name'),
    values: _obj(json, 'values'),
  );

  JsonObject toJson() => {'id': id, 'name': name, 'values': values};

  @override
  bool operator ==(Object other) =>
      other is OperationPreset &&
      other.id == id &&
      other.name == name &&
      _mapEquals(other.values, values);

  @override
  int get hashCode => Object.hash(id, name);
}

/// Port of `ParamKind` (serde renames per variant).
enum ParamKind {
  folder('folder'),
  file('file'),
  files('files'),
  text('text'),
  bool('bool'),
  choice('choice'),
  int('int'),
  longText('longText'),
  secret('secret'),
  sheetCounts('sheetCounts');

  const ParamKind(this.wire);

  final String wire;

  static ParamKind fromWire(String value) => ParamKind.values.firstWhere(
    (kind) => kind.wire == value,
    orElse: () => throw FormatException('unknown param kind `$value`'),
  );
}

class ParamDef {
  const ParamDef({
    required this.name,
    required this.kind,
    required this.labelKey,
    required this.defaultValue,
    required this.required,
    required this.choices,
    required this.choiceLabels,
    required this.visibleWhen,
    this.helpKey,
    this.remember = true,
  });

  final String name;
  final ParamKind kind;
  final String labelKey;

  /// Serde field `default` (any JSON value, including null).
  final Object? defaultValue;
  final bool required;
  final List<String> choices;
  final Map<String, String> choiceLabels;
  final JsonObject visibleWhen;
  final String? helpKey;
  final bool remember;

  factory ParamDef.fromJson(JsonObject json) => ParamDef(
    name: _str(json, 'name'),
    kind: ParamKind.fromWire(_str(json, 'kind')),
    labelKey: _str(json, 'labelKey'),
    defaultValue: _req(json, 'default'),
    required: _bool(json, 'required'),
    choices: _strList(json, 'choices'),
    choiceLabels: _strMap(json, 'choiceLabels'),
    visibleWhen: _obj(json, 'visibleWhen'),
    helpKey: _optStr(json, 'helpKey'),
    remember: json.containsKey('remember') ? _bool(json, 'remember') : true,
  );

  JsonObject toJson() => {
    'name': name,
    'kind': kind.wire,
    'labelKey': labelKey,
    'default': defaultValue,
    'required': required,
    'choices': choices,
    'choiceLabels': choiceLabels,
    'visibleWhen': visibleWhen,
    'helpKey': helpKey,
    if (!remember) 'remember': false,
  };

  @override
  bool operator ==(Object other) =>
      other is ParamDef &&
      other.name == name &&
      other.kind == kind &&
      other.labelKey == labelKey &&
      _jsonEquals(other.defaultValue, defaultValue) &&
      other.required == required &&
      _listEquals(other.choices, choices) &&
      _mapEquals(other.choiceLabels, choiceLabels) &&
      _mapEquals(other.visibleWhen, visibleWhen) &&
      other.helpKey == helpKey &&
      other.remember == remember;

  @override
  int get hashCode => Object.hash(name, kind, labelKey, required, helpKey);
}

/// Port of `ParamSchema` (`#[serde(transparent)]` over a list of params).
class ParamSchema {
  const ParamSchema({required this.params});

  final List<ParamDef> params;

  factory ParamSchema.fromJson(List<Object?> json) => ParamSchema(
    params: [for (final item in json) ParamDef.fromJson(_asObject(item))],
  );

  List<Object?> toJson() => [for (final param in params) param.toJson()];

  @override
  bool operator ==(Object other) =>
      other is ParamSchema && _listEquals(other.params, params);

  @override
  int get hashCode => Object.hashAll(params);

  /// Port of `ParamSchema::validate`. Returns `error.param.required.<name>`
  /// keys for every missing required parameter, in schema order.
  List<String> validate(JsonObject values) {
    final missing = <String>[];
    for (final parameter in params) {
      final value = values[parameter.name];
      final isSheetCountsRequired =
          parameter.kind == ParamKind.sheetCounts && parameter.required;
      final isMissing = isSheetCountsRequired
          ? !(value is Map && value.values.any(_positiveProtocolInteger))
          : parameter.required && !_protocolTruthy(value);
      if (isMissing) {
        missing.add('error.param.required.${parameter.name}');
      }
    }
    return missing;
  }
}

/// Port of `positive_protocol_integer`.
bool _positiveProtocolInteger(Object? value) {
  if (value is bool) {
    return value;
  }
  if (value is int) {
    return value > 0;
  }
  return false;
}

/// Port of `protocol_truthy`.
bool _protocolTruthy(Object? value) {
  if (value == null) {
    return false;
  }
  if (value is bool) {
    return value;
  }
  if (value is num) {
    return value != 0;
  }
  if (value is String) {
    return value.isNotEmpty;
  }
  if (value is List) {
    return value.isNotEmpty;
  }
  if (value is Map) {
    return value.isNotEmpty;
  }
  return true;
}

class OperationDef {
  const OperationDef({
    required this.id,
    required this.titleKey,
    required this.descriptionKey,
    required this.schema,
    this.supportsPresets = false,
  });

  final String id;
  final String titleKey;
  final String descriptionKey;
  final ParamSchema schema;
  final bool supportsPresets;

  factory OperationDef.fromJson(JsonObject json) => OperationDef(
    id: _str(json, 'id'),
    titleKey: _str(json, 'titleKey'),
    descriptionKey: _str(json, 'descriptionKey'),
    schema: ParamSchema.fromJson(_req(json, 'schema')! as List<Object?>),
    supportsPresets:
        json.containsKey('supportsPresets') && _bool(json, 'supportsPresets'),
  );

  JsonObject toJson() => {
    'id': id,
    'titleKey': titleKey,
    'descriptionKey': descriptionKey,
    'schema': schema.toJson(),
    'supportsPresets': supportsPresets,
  };

  @override
  bool operator ==(Object other) =>
      other is OperationDef &&
      other.id == id &&
      other.titleKey == titleKey &&
      other.descriptionKey == descriptionKey &&
      other.schema == schema &&
      other.supportsPresets == supportsPresets;

  @override
  int get hashCode =>
      Object.hash(id, titleKey, descriptionKey, supportsPresets);
}

// ---- Preferences, auth, bootstrap ---------------------------------------

class AppPrefs {
  const AppPrefs({
    this.locale,
    this.theme = 'system',
    this.font = 'system',
    this.fontScale = 1.0,
    this.compact = false,
    this.startWithWindows = false,
    this.replicaSyncEnabled = false,
  });

  final String? locale;
  final String theme;
  final String font;
  final double fontScale;
  final bool compact;
  final bool startWithWindows;
  final bool replicaSyncEnabled;

  factory AppPrefs.fromJson(JsonObject json) => AppPrefs(
    locale: _optStr(json, 'locale'),
    theme: _str(json, 'theme'),
    font: _str(json, 'font'),
    fontScale: _dbl(json, 'fontScale'),
    compact: _bool(json, 'compact'),
    startWithWindows: _bool(json, 'startWithWindows'),
    replicaSyncEnabled: _bool(json, 'replicaSyncEnabled'),
  );

  JsonObject toJson() => {
    'locale': locale,
    'theme': theme,
    'font': font,
    'fontScale': fontScale,
    'compact': compact,
    'startWithWindows': startWithWindows,
    'replicaSyncEnabled': replicaSyncEnabled,
  };

  @override
  bool operator ==(Object other) =>
      other is AppPrefs &&
      other.locale == locale &&
      other.theme == theme &&
      other.font == font &&
      other.fontScale == fontScale &&
      other.compact == compact &&
      other.startWithWindows == startWithWindows &&
      other.replicaSyncEnabled == replicaSyncEnabled;

  @override
  int get hashCode => Object.hash(
    locale,
    theme,
    font,
    fontScale,
    compact,
    startWithWindows,
    replicaSyncEnabled,
  );
}

class AppUser {
  const AppUser({
    required this.id,
    required this.email,
    required this.username,
    required this.display,
    required this.allowed,
    required this.isAdmin,
    required this.createdAt,
  });

  final String id;
  final String email;
  final String? username;
  final String display;
  final bool allowed;
  final bool isAdmin;
  final String createdAt;

  factory AppUser.fromJson(JsonObject json) => AppUser(
    id: _str(json, 'id'),
    email: _str(json, 'email'),
    username: _optStr(json, 'username'),
    display: _str(json, 'display'),
    allowed: _bool(json, 'allowed'),
    isAdmin: _bool(json, 'isAdmin'),
    createdAt: _str(json, 'createdAt'),
  );

  JsonObject toJson() => {
    'id': id,
    'email': email,
    'username': username,
    'display': display,
    'allowed': allowed,
    'isAdmin': isAdmin,
    'createdAt': createdAt,
  };

  @override
  bool operator ==(Object other) =>
      other is AppUser &&
      other.id == id &&
      other.email == email &&
      other.username == username &&
      other.display == display &&
      other.allowed == allowed &&
      other.isAdmin == isAdmin &&
      other.createdAt == createdAt;

  @override
  int get hashCode =>
      Object.hash(id, email, username, display, allowed, isAdmin, createdAt);
}

class AuthSessionView {
  const AuthSessionView({
    required this.userId,
    required this.email,
    required this.expiresAt,
  });

  final String userId;
  final String email;
  final int expiresAt;

  factory AuthSessionView.fromJson(JsonObject json) => AuthSessionView(
    userId: _str(json, 'userId'),
    email: _str(json, 'email'),
    expiresAt: _int(json, 'expiresAt'),
  );

  JsonObject toJson() => {
    'userId': userId,
    'email': email,
    'expiresAt': expiresAt,
  };

  @override
  bool operator ==(Object other) =>
      other is AuthSessionView &&
      other.userId == userId &&
      other.email == email &&
      other.expiresAt == expiresAt;

  @override
  int get hashCode => Object.hash(userId, email, expiresAt);
}

class AuthState {
  const AuthState({
    required this.signedIn,
    required this.allowed,
    required this.isAdmin,
    required this.session,
    required this.user,
  });

  final bool signedIn;
  final bool allowed;
  final bool isAdmin;
  final AuthSessionView? session;
  final AppUser? user;

  factory AuthState.fromJson(JsonObject json) => AuthState(
    signedIn: _bool(json, 'signedIn'),
    allowed: _bool(json, 'allowed'),
    isAdmin: _bool(json, 'isAdmin'),
    session: _optObj(json, 'session') == null
        ? null
        : AuthSessionView.fromJson(_optObj(json, 'session')!),
    user: _optObj(json, 'user') == null
        ? null
        : AppUser.fromJson(_optObj(json, 'user')!),
  );

  JsonObject toJson() => {
    'signedIn': signedIn,
    'allowed': allowed,
    'isAdmin': isAdmin,
    'session': session?.toJson(),
    'user': user?.toJson(),
  };

  @override
  bool operator ==(Object other) =>
      other is AuthState &&
      other.signedIn == signedIn &&
      other.allowed == allowed &&
      other.isAdmin == isAdmin &&
      other.session == session &&
      other.user == user;

  @override
  int get hashCode => Object.hash(signedIn, allowed, isAdmin, session, user);
}

class Bootstrap {
  const Bootstrap({
    required this.protocol,
    required this.version,
    required this.auth,
    required this.prefs,
    required this.operations,
  });

  final int protocol;
  final String version;
  final AuthState auth;
  final AppPrefs prefs;
  final List<OperationDef> operations;

  factory Bootstrap.fromJson(JsonObject json) => Bootstrap(
    protocol: _int(json, 'protocol'),
    version: _str(json, 'version'),
    auth: AuthState.fromJson(_obj(json, 'auth')),
    prefs: AppPrefs.fromJson(_obj(json, 'prefs')),
    operations: [
      for (final item in _req(json, 'operations')! as List)
        OperationDef.fromJson(_asObject(item)),
    ],
  );

  JsonObject toJson() => {
    'protocol': protocol,
    'version': version,
    'auth': auth.toJson(),
    'prefs': prefs.toJson(),
    'operations': [for (final operation in operations) operation.toJson()],
  };
}

// ---- Progress, steps, plans, journal ------------------------------------

class ProgressEvent {
  const ProgressEvent({
    required this.phase,
    required this.done,
    required this.total,
    this.subject = '',
  });

  final String phase;
  final int done;
  final int total;
  final String subject;

  factory ProgressEvent.fromJson(JsonObject json) => ProgressEvent(
    phase: _str(json, 'phase'),
    done: _int(json, 'done'),
    total: _int(json, 'total'),
    subject: json.containsKey('subject') ? _str(json, 'subject') : '',
  );

  JsonObject toJson() => {
    'phase': phase,
    'done': done,
    'total': total,
    'subject': subject,
  };

  @override
  bool operator ==(Object other) =>
      other is ProgressEvent &&
      other.phase == phase &&
      other.done == done &&
      other.total == total &&
      other.subject == subject;

  @override
  int get hashCode => Object.hash(phase, done, total, subject);
}

/// Port of `StepKind` (`rename_all = "snake_case"`).
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
    orElse: () => throw FormatException('unknown step kind `$value`'),
  );
}

class Step {
  const Step({
    required this.kind,
    required this.src,
    required this.dst,
    required this.noteKey,
    required this.meta,
  });

  final StepKind kind;
  final String? src;
  final String? dst;
  final String? noteKey;
  final JsonObject meta;

  factory Step.fromJson(JsonObject json) => Step(
    kind: StepKind.fromWire(_str(json, 'kind')),
    src: _optStr(json, 'src'),
    dst: _optStr(json, 'dst'),
    noteKey: _optStr(json, 'noteKey'),
    meta: _obj(json, 'meta'),
  );

  JsonObject toJson() => {
    'kind': kind.wire,
    'src': src,
    'dst': dst,
    'noteKey': noteKey,
    'meta': meta,
  };

  @override
  bool operator ==(Object other) =>
      other is Step &&
      other.kind == kind &&
      other.src == src &&
      other.dst == dst &&
      other.noteKey == noteKey &&
      _mapEquals(other.meta, meta);

  @override
  int get hashCode => Object.hash(kind, src, dst, noteKey);
}

class Warning {
  const Warning({required this.key, required this.subject});

  final String key;
  final String subject;

  factory Warning.fromJson(JsonObject json) =>
      Warning(key: _str(json, 'key'), subject: _str(json, 'subject'));

  JsonObject toJson() => {'key': key, 'subject': subject};

  @override
  bool operator ==(Object other) =>
      other is Warning && other.key == key && other.subject == subject;

  @override
  int get hashCode => Object.hash(key, subject);
}

/// Port of `pub type Action = Step;`.
typedef Action = Step;

/// Port of `pub type Issue = Warning;`.
typedef Issue = Warning;

class Plan {
  const Plan({
    required this.planId,
    required this.opId,
    required this.steps,
    required this.warnings,
    required this.meta,
  });

  final String planId;
  final String opId;
  final List<Step> steps;
  final List<Warning> warnings;
  final JsonObject meta;

  factory Plan.fromJson(JsonObject json) => Plan(
    planId: _str(json, 'planId'),
    opId: _str(json, 'opId'),
    steps: [
      for (final item in _req(json, 'steps')! as List)
        Step.fromJson(_asObject(item)),
    ],
    warnings: [
      for (final item in _req(json, 'warnings')! as List)
        Warning.fromJson(_asObject(item)),
    ],
    meta: _obj(json, 'meta'),
  );

  JsonObject toJson() => {
    'planId': planId,
    'opId': opId,
    'steps': [for (final step in steps) step.toJson()],
    'warnings': [for (final warning in warnings) warning.toJson()],
    'meta': meta,
  };
}

class OperationResult {
  const OperationResult({
    required this.applied,
    required this.failed,
    required this.journalId,
    required this.errors,
    required this.meta,
    this.cloudSynced,
  });

  final int applied;
  final int failed;
  final String journalId;
  final List<Warning> errors;
  final JsonObject meta;

  /// Omitted from JSON when null (`skip_serializing_if = Option::is_none`).
  final bool? cloudSynced;

  factory OperationResult.fromJson(JsonObject json) => OperationResult(
    applied: _int(json, 'applied'),
    failed: _int(json, 'failed'),
    journalId: _str(json, 'journalId'),
    errors: [
      for (final item in _req(json, 'errors')! as List)
        Warning.fromJson(_asObject(item)),
    ],
    meta: _obj(json, 'meta'),
    cloudSynced: json['cloudSynced'] as bool?,
  );

  JsonObject toJson() => {
    'applied': applied,
    'failed': failed,
    'journalId': journalId,
    'errors': [for (final error in errors) error.toJson()],
    'meta': meta,
    if (cloudSynced != null) 'cloudSynced': cloudSynced,
  };
}

class JournalEntry {
  const JournalEntry({
    required this.journalId,
    required this.opId,
    required this.ts,
    required this.steps,
    this.pendingStep,
  });

  final String journalId;
  final String opId;
  final String ts;
  final List<Step> steps;

  /// Omitted from JSON when null.
  final Step? pendingStep;

  factory JournalEntry.fromJson(JsonObject json) => JournalEntry(
    journalId: _str(json, 'journalId'),
    opId: _str(json, 'opId'),
    ts: _str(json, 'ts'),
    steps: [
      for (final item in _req(json, 'steps')! as List)
        Step.fromJson(_asObject(item)),
    ],
    pendingStep: _optObj(json, 'pendingStep') == null
        ? null
        : Step.fromJson(_optObj(json, 'pendingStep')!),
  );

  JsonObject toJson() => {
    'journalId': journalId,
    'opId': opId,
    'ts': ts,
    'steps': [for (final step in steps) step.toJson()],
    if (pendingStep != null) 'pendingStep': pendingStep!.toJson(),
  };
}

class ReviewItem {
  const ReviewItem({
    required this.name,
    required this.path,
    required this.code,
    required this.confidence,
    required this.target,
    required this.targetExists,
  });

  final String name;
  final String path;
  final String code;
  final double confidence;
  final String? target;
  final bool targetExists;

  factory ReviewItem.fromJson(JsonObject json) => ReviewItem(
    name: _str(json, 'name'),
    path: _str(json, 'path'),
    code: _str(json, 'code'),
    confidence: _dbl(json, 'confidence'),
    target: _optStr(json, 'target'),
    targetExists: _bool(json, 'targetExists'),
  );

  JsonObject toJson() => {
    'name': name,
    'path': path,
    'code': code,
    'confidence': confidence,
    'target': target,
    'targetExists': targetExists,
  };
}

class HistoryRow {
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

  factory HistoryRow.fromJson(JsonObject json) => HistoryRow(
    when: _str(json, 'when'),
    opId: _str(json, 'opId'),
    moves: _int(json, 'moves'),
    failed: _int(json, 'failed'),
    journalId: _str(json, 'journalId'),
    steps: json['steps'] == null
        ? null
        : [
            for (final item in json['steps']! as List)
              Step.fromJson(_asObject(item)),
          ],
    cloudId: _optStr(json, 'cloudId'),
    undone: _bool(json, 'undone'),
    undoAvailable: _bool(json, 'undoAvailable'),
    fromCloud: _bool(json, 'fromCloud'),
    userEmail: _str(json, 'userEmail'),
  );

  JsonObject toJson() => {
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

class HistoryPayload {
  const HistoryPayload({required this.online, required this.rows});

  final bool online;
  final List<HistoryRow> rows;

  factory HistoryPayload.fromJson(JsonObject json) => HistoryPayload(
    online: _bool(json, 'online'),
    rows: [
      for (final item in _req(json, 'rows')! as List)
        HistoryRow.fromJson(_asObject(item)),
    ],
  );

  JsonObject toJson() => {
    'online': online,
    'rows': [for (final row in rows) row.toJson()],
  };
}

class PrintQueueSummary {
  const PrintQueueSummary({
    required this.queueId,
    required this.status,
    required this.createdAt,
    required this.updatedAt,
    required this.counts,
    required this.resumable,
    required this.fileCount,
  });

  final String queueId;
  final String status;
  final String createdAt;
  final String updatedAt;

  /// Rust `BTreeMap<String, u64>`: serialized with sorted keys.
  final Map<String, int> counts;
  final int resumable;
  final int fileCount;

  factory PrintQueueSummary.fromJson(JsonObject json) => PrintQueueSummary(
    queueId: _str(json, 'queueId'),
    status: _str(json, 'status'),
    createdAt: _str(json, 'createdAt'),
    updatedAt: _str(json, 'updatedAt'),
    counts: {
      for (final entry in _obj(json, 'counts').entries)
        entry.key: (entry.value! as num).toInt(),
    },
    resumable: _int(json, 'resumable'),
    fileCount: _int(json, 'fileCount'),
  );

  JsonObject toJson() {
    final sortedKeys = counts.keys.toList()..sort();
    return {
      'queueId': queueId,
      'status': status,
      'createdAt': createdAt,
      'updatedAt': updatedAt,
      'counts': {for (final key in sortedKeys) key: counts[key]},
      'resumable': resumable,
      'fileCount': fileCount,
    };
  }
}

// ---- Structural equality helpers ----------------------------------------

bool _listEquals<T>(List<T> a, List<T> b) {
  if (a.length != b.length) {
    return false;
  }
  for (var index = 0; index < a.length; index++) {
    if (a[index] != b[index]) {
      return false;
    }
  }
  return true;
}

bool _mapEquals(Map<Object?, Object?> a, Map<Object?, Object?> b) {
  if (a.length != b.length) {
    return false;
  }
  for (final key in a.keys) {
    if (!b.containsKey(key) || !_jsonEquals(a[key], b[key])) {
      return false;
    }
  }
  return true;
}

bool _jsonEquals(Object? a, Object? b) {
  if (identical(a, b)) {
    return true;
  }
  if (a is Map && b is Map) {
    return _mapEquals(a, b);
  }
  if (a is List && b is List) {
    if (a.length != b.length) {
      return false;
    }
    for (var index = 0; index < a.length; index++) {
      if (!_jsonEquals(a[index], b[index])) {
        return false;
      }
    }
    return true;
  }
  return a == b;
}
