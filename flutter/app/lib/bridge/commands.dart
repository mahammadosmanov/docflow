// Typed command map: the Dart counterpart of frontend/src/lib/commands.ts.
//
// Each command pairs a wire name with the params record it accepts and the
// model it returns. `FRONTEND_METHODS` in commands.ts lists the same names;
// test/foundation/command_map_test.dart keeps the two lists equal.

import '../models/models.dart';

/// Decodes one engine response value into a typed result.
typedef Decoder<R> = R Function(Object? json);

/// Marker for commands without params (`undefined` in commands.ts).
final class NoParams {
  const NoParams();
}

/// Successful acknowledgement (`Ok` in commands.ts: `{ ok: true }`).
final class CommandOk {
  const CommandOk();
}

/// One frontend command: the engine method name plus typed encode and decode.
final class FrontendCommand<P, R> {
  const FrontendCommand._(this.name, this._encode, this.decode);

  /// A command with no params. Encodes to an empty object, as in bridge.ts.
  factory FrontendCommand.noParams(String name, Decoder<R> decode) =>
      FrontendCommand._(name, (_) => const <String, Object?>{}, decode);

  /// A command with params. A missing `params` argument is an error, since the
  /// TypeScript signature requires them.
  factory FrontendCommand.withParams(
    String name,
    Map<String, Object?> Function(P params) encode,
    Decoder<R> decode,
  ) => FrontendCommand._(name, (P? params) {
    if (params == null) {
      throw ArgumentError.notNull('params for $name');
    }
    return encode(params);
  }, decode);

  final String name;
  final Map<String, Object?> Function(P? params) _encode;
  final Decoder<R> decode;

  Map<String, Object?> encode(P? params) => _encode(params);
}

Decoder<R> _object<R>(R Function(Map<String, Object?> json) fromJson) =>
    (json) => fromJson(_mapOf(json));

Decoder<List<R>> _list<R>(R Function(Map<String, Object?> json) fromJson) =>
    (json) => [for (final item in _listOf(json)) fromJson(_mapOf(item))];

Decoder<R?> _nullableObject<R>(
  R Function(Map<String, Object?> json) fromJson,
) =>
    (json) => json == null ? null : fromJson(_mapOf(json));

Decoder<bool> _boolean() => (json) {
  if (json is bool) return json;
  throw FormatException('Expected boolean result, got ${json.runtimeType}');
};

Decoder<Map<String, Object?>> _map() => _mapOf;

Decoder<CommandOk> _ok() => (json) {
  if (json is Map && json['ok'] == true) return const CommandOk();
  throw const FormatException('Expected {"ok": true}');
};

Map<String, Object?> _mapOf(Object? json) {
  if (json is Map<String, Object?>) return json;
  if (json is Map) {
    return json.map((key, value) => MapEntry(key.toString(), value));
  }
  throw FormatException('Expected object result, got ${json.runtimeType}');
}

List<Object?> _listOf(Object? json) {
  if (json is List) return json;
  throw FormatException('Expected array result, got ${json.runtimeType}');
}

Map<String, Object?> _withoutNulls(Map<String, Object?> map) => {
  for (final entry in map.entries)
    if (entry.value != null) entry.key: entry.value,
};

/// Every frontend command, in the order of `FRONTEND_METHODS`.
abstract final class FrontendCommands {
  FrontendCommands._();

  static final appBootstrap = FrontendCommand<NoParams, Bootstrap>.noParams(
    'app/bootstrap',
    _object(Bootstrap.fromJson),
  );

  static final authUsernameAvailable =
      FrontendCommand<({String username}), bool>.withParams(
        'auth/usernameAvailable',
        (p) => {'username': p.username},
        _boolean(),
      );

  static final authSignIn =
      FrontendCommand<
        ({String identifier, String password}),
        AuthState
      >.withParams(
        'auth/signIn',
        (p) => {'identifier': p.identifier, 'password': p.password},
        _object(AuthState.fromJson),
      );

  static final authSignUp =
      FrontendCommand<
        ({String email, String password, String username}),
        CommandOk
      >.withParams(
        'auth/signUp',
        (p) => {
          'email': p.email,
          'password': p.password,
          'username': p.username,
        },
        _ok(),
      );

  static final authSignOut = FrontendCommand<NoParams, AuthState>.noParams(
    'auth/signOut',
    _object(AuthState.fromJson),
  );

  static final authLoadMe = FrontendCommand<NoParams, AuthState>.noParams(
    'auth/loadMe',
    _object(AuthState.fromJson),
  );

  static final authSetUsername =
      FrontendCommand<({String username}), AuthState>.withParams(
        'auth/setUsername',
        (p) => {'username': p.username},
        _object(AuthState.fromJson),
      );

  static final authListUsers =
      FrontendCommand<NoParams, List<AppUser>>.noParams(
        'auth/listUsers',
        _list(AppUser.fromJson),
      );

  static final authSetFlags =
      FrontendCommand<
        ({String userId, bool? allowed, bool? isAdmin}),
        CommandOk
      >.withParams(
        'auth/setFlags',
        (p) => _withoutNulls({
          'userId': p.userId,
          'allowed': p.allowed,
          'isAdmin': p.isAdmin,
        }),
        _ok(),
      );

  static final settingsUpdate =
      FrontendCommand<AppPrefsPatch, AppPrefs>.withParams(
        'settings/update',
        (p) => p.toJson(),
        _object(AppPrefs.fromJson),
      );

  static final settingsReset = FrontendCommand<NoParams, AppPrefs>.noParams(
    'settings/reset',
    _object(AppPrefs.fromJson),
  );

  static final settingsReplicaStatus =
      FrontendCommand<NoParams, ReplicaAutomationStatus>.noParams(
        'settings/replicaStatus',
        _object(ReplicaAutomationStatus.fromJson),
      );

  static final settingsParams =
      FrontendCommand<({String opId}), Map<String, Object?>>.withParams(
        'settings/params',
        (p) => {'opId': p.opId},
        _map(),
      );

  static final settingsSaveParams =
      FrontendCommand<
        ({String opId, Map<String, Object?> values}),
        CommandOk
      >.withParams(
        'settings/saveParams',
        (p) => {'opId': p.opId, 'values': p.values},
        _ok(),
      );

  static final settingsPresets =
      FrontendCommand<({String opId}), List<OperationPreset>>.withParams(
        'settings/presets',
        (p) => {'opId': p.opId},
        _list(OperationPreset.fromJson),
      );

  static final settingsSavePreset =
      FrontendCommand<
        ({
          String opId,
          String name,
          Map<String, Object?> values,
          bool? replace,
        }),
        OperationPreset
      >.withParams(
        'settings/savePreset',
        (p) => _withoutNulls({
          'opId': p.opId,
          'name': p.name,
          'values': p.values,
          'replace': p.replace,
        }),
        _object(OperationPreset.fromJson),
      );

  static final settingsDeletePreset =
      FrontendCommand<({String opId, String presetId}), CommandOk>.withParams(
        'settings/deletePreset',
        (p) => {'opId': p.opId, 'presetId': p.presetId},
        _ok(),
      );

  static final supportDiagnostics =
      FrontendCommand<NoParams, SupportBundlePreview>.noParams(
        'support/diagnostics',
        _object(SupportBundlePreview.fromJson),
      );

  static final supportCopyDiagnostics =
      FrontendCommand<({String content}), CommandOk>.withParams(
        'support/copyDiagnostics',
        (p) => {'content': p.content},
        _ok(),
      );

  static final enginePreview =
      FrontendCommand<
        ({String opId, Map<String, Object?> values}),
        Plan
      >.withParams(
        'engine/preview',
        (p) => {'opId': p.opId, 'values': p.values},
        _object(Plan.fromJson),
      );

  static final engineApply =
      FrontendCommand<
        ({String opId, String planId}),
        OperationResult
      >.withParams(
        'engine/apply',
        (p) => {'opId': p.opId, 'planId': p.planId},
        _object(OperationResult.fromJson),
      );

  static final engineCancel =
      FrontendCommand<
        ({String? planId, String? opId}),
        ({bool accepted})
      >.withParams(
        'engine/cancel',
        (p) => _withoutNulls({'planId': p.planId, 'opId': p.opId}),
        (json) => (accepted: _mapOf(json)['accepted'] as bool),
      );

  static final enginePrintQueueLatest =
      FrontendCommand<NoParams, PrintQueueSummary?>.noParams(
        'engine/printQueueLatest',
        _nullableObject(PrintQueueSummary.fromJson),
      );

  static final enginePrintQueueDiscard =
      FrontendCommand<({String queueId}), PrintQueueSummary>.withParams(
        'engine/printQueueDiscard',
        (p) => {'queueId': p.queueId},
        _object(PrintQueueSummary.fromJson),
      );

  static final enginePrintQueueResume =
      FrontendCommand<({String queueId}), Plan>.withParams(
        'engine/printQueueResume',
        (p) => {'queueId': p.queueId},
        _object(Plan.fromJson),
      );

  static final engineRevise =
      FrontendCommand<
        ({String planId, List<Map<String, Object?>> rows}),
        Plan
      >.withParams(
        'engine/revise',
        (p) => {'planId': p.planId, 'rows': p.rows},
        _object(Plan.fromJson),
      );

  static final engineCopyTable =
      FrontendCommand<
        ({String planId, String? sheet}),
        ({int count})
      >.withParams(
        'engine/copyTable',
        (p) => _withoutNulls({'planId': p.planId, 'sheet': p.sheet}),
        (json) => (count: _mapOf(json)['count'] as int),
      );

  static final engineRfiReplyPreview =
      FrontendCommand<({String planId, String journalId}), Plan>.withParams(
        'engine/rfiReplyPreview',
        (p) => {'planId': p.planId, 'journalId': p.journalId},
        _object(Plan.fromJson),
      );

  static final engineReviewScan =
      FrontendCommand<
        ({PathSelection destination}),
        List<ReviewItem>
      >.withParams(
        'engine/reviewScan',
        (p) => {'destination': p.destination.toJson()},
        _list(ReviewItem.fromJson),
      );

  static final engineReviewPreview =
      FrontendCommand<
        ({PathSelection destination, List<ReviewDecision> decisions}),
        Plan
      >.withParams(
        'engine/reviewPreview',
        (p) => {
          'destination': p.destination.toJson(),
          'decisions': [for (final decision in p.decisions) decision.toJson()],
        },
        _object(Plan.fromJson),
      );

  static final engineUndo =
      FrontendCommand<
        ({String journalId, String? cloudId, PathSelection? destination}),
        OperationResult
      >.withParams(
        'engine/undo',
        (p) => _withoutNulls({
          'journalId': p.journalId,
          'cloudId': p.cloudId,
          'destination': p.destination?.toJson(),
        }),
        _object(OperationResult.fromJson),
      );

  static final historyList =
      FrontendCommand<({int? limit}), HistoryPayload>.withParams(
        'history/list',
        (p) => _withoutNulls({'limit': p.limit}),
        _object(HistoryPayload.fromJson),
      );

  static final pathsSelect =
      FrontendCommand<
        ({String opId, String paramName, List<String>? droppedPaths}),
        List<PathSelection>
      >.withParams(
        'paths/select',
        (p) => _withoutNulls({
          'opId': p.opId,
          'paramName': p.paramName,
          'droppedPaths': p.droppedPaths,
        }),
        _list(PathSelection.fromJson),
      );

  static final pathsReveal =
      FrontendCommand<({String selectionId}), CommandOk>.withParams(
        'paths/reveal',
        (p) => {'selectionId': p.selectionId},
        _ok(),
      );

  static final recoveryList =
      FrontendCommand<NoParams, RecoveryPayload>.noParams(
        'recovery/list',
        _object(RecoveryPayload.fromJson),
      );

  static final rfiPrintFind =
      FrontendCommand<
        ({
          PathSelection root,
          List<String> numbers,
          bool firstPage,
          bool sketch,
        }),
        RfiPrintFindResult
      >.withParams(
        'rfi/printFind',
        (p) => {
          'root': p.root.toJson(),
          'numbers': p.numbers,
          'firstPage': p.firstPage,
          'sketch': p.sketch,
        },
        _object(RfiPrintFindResult.fromJson),
      );

  /// All commands, in `FRONTEND_METHODS` order.
  static final List<FrontendCommand<dynamic, dynamic>> all = [
    appBootstrap,
    authUsernameAvailable,
    authSignIn,
    authSignUp,
    authSignOut,
    authLoadMe,
    authSetUsername,
    authListUsers,
    authSetFlags,
    settingsUpdate,
    settingsReset,
    settingsReplicaStatus,
    settingsParams,
    settingsSaveParams,
    settingsPresets,
    settingsSavePreset,
    settingsDeletePreset,
    supportDiagnostics,
    supportCopyDiagnostics,
    enginePreview,
    engineApply,
    engineCancel,
    enginePrintQueueLatest,
    enginePrintQueueDiscard,
    enginePrintQueueResume,
    engineRevise,
    engineCopyTable,
    engineRfiReplyPreview,
    engineReviewScan,
    engineReviewPreview,
    engineUndo,
    historyList,
    pathsSelect,
    pathsReveal,
    rfiPrintFind,
    recoveryList,
  ];
}
