// Port of src-tauri/src/backend.rs: the closed method allowlist, authorization
// gate, request shapes, and the command router behind `frontend_call`.
//
// Commands whose engine module is not wired yet go through [NativeOperations]
// or [PlatformServices]. Their defaults fail with `engine_not_wired`, which the
// composition root must replace before those commands can run.

import 'dart:io';

import 'package:path/path.dart' as p;
import 'package:uuid/uuid.dart';

import 'auth.dart' as auth;
import 'cloud.dart';
import 'contracts.dart';
import 'diagnostics.dart';
import 'engine.dart' show ProgressHandler;
import 'errors.dart';
import 'path_selection.dart';
import 'paths.dart' show CallerIdentity;
import 'registry.dart';
import 'settings.dart';

/// Engine version reported by `app/bootstrap` and diagnostics. Must match the
/// root `VERSION` file.
const String docflowVersion = '1.20.14';

/// Every frontend-reachable method (`BackendMethod` in backend.rs). The wire
/// names are the exact strings in `frontend/src/lib/commands.ts`.
enum BackendMethod {
  appBootstrap('app/bootstrap'),
  authUsernameAvailable('auth/usernameAvailable'),
  authSignIn('auth/signIn'),
  authSignUp('auth/signUp'),
  authSignOut('auth/signOut'),
  authLoadMe('auth/loadMe'),
  authSetUsername('auth/setUsername'),
  authListUsers('auth/listUsers'),
  authSetFlags('auth/setFlags'),
  settingsUpdate('settings/update'),
  settingsReset('settings/reset'),
  settingsReplicaStatus('settings/replicaStatus'),
  settingsParams('settings/params'),
  settingsSaveParams('settings/saveParams'),
  settingsPresets('settings/presets'),
  settingsSavePreset('settings/savePreset'),
  settingsDeletePreset('settings/deletePreset'),
  supportDiagnostics('support/diagnostics'),
  supportCopyDiagnostics('support/copyDiagnostics'),
  enginePreview('engine/preview'),
  engineApply('engine/apply'),
  engineCancel('engine/cancel'),
  enginePrintQueueLatest('engine/printQueueLatest'),
  enginePrintQueueDiscard('engine/printQueueDiscard'),
  enginePrintQueueResume('engine/printQueueResume'),
  engineRevise('engine/revise'),
  engineCopyTable('engine/copyTable'),
  engineRfiReplyPreview('engine/rfiReplyPreview'),
  engineReviewScan('engine/reviewScan'),
  engineReviewPreview('engine/reviewPreview'),
  engineUndo('engine/undo'),
  historyList('history/list'),
  pathsSelect('paths/select'),
  pathsReveal('paths/reveal'),
  recoveryList('recovery/list'),
  rfiPrintFind('rfi/printFind');

  const BackendMethod(this.wire);

  /// The exact method name the frontend sends.
  final String wire;

  /// Exact-match lookup. Any other name is not a frontend method.
  static BackendMethod? fromWire(String name) {
    for (final method in values) {
      if (method.wire == name) {
        return method;
      }
    }
    return null;
  }

  static const Set<BackendMethod> _public = {
    appBootstrap,
    settingsUpdate,
    settingsReset,
    settingsReplicaStatus,
    supportDiagnostics,
    supportCopyDiagnostics,
    authSignIn,
    authSignUp,
    authSignOut,
    authUsernameAvailable,
    authLoadMe,
    authSetUsername,
    authListUsers,
    authSetFlags,
  };

  /// Port of `authorize`: public methods pass signed out; every other method
  /// needs a signed-in, allowed caller.
  void authorize({required bool signedIn, required bool allowed}) {
    if (_public.contains(this)) {
      return;
    }
    if (!signedIn) {
      throw BackendError.authRequired();
    }
    if (!allowed) {
      throw BackendError.permissionDenied();
    }
  }
}

/// Native dialog filter for a file parameter (`path_dialog_filter`).
typedef DialogFilter = ({String name, List<String> extensions});

DialogFilter? pathDialogFilter(String operationId, String parameterName) {
  const excel = (name: 'Excel (.xlsm, .xlsx)', extensions: ['xlsm', 'xlsx']);
  switch ((operationId, parameterName)) {
    case ('rfi.log_add', 'workbook') ||
        ('rfi.follow_up_sync', 'rfi_log_workbook' || 'follow_up_workbook') ||
        ('rfi.system_sync', 'rfi_log_workbook' || 'follow_up_workbook') ||
        ('rfi.complete_status_sync', 'rfi_log_workbook') ||
        ('rfi.generate_concrete_documents', 'rfi_log_workbook' || 'follow_up_workbook'):
      return excel;
    case ('rfi.generate_concrete_documents', 'template_010' || 'template_013'):
      return (name: 'Excel (.xlsx)', extensions: const ['xlsx']);
    case ('rfi.generate_concrete_documents', 'template_011' || 'template_014'):
      return (name: 'Word (.docx)', extensions: const ['docx']);
    default:
      return null;
  }
}

/// Port of `validate_apply_envelope`: apply accepts only an opaque plan
/// identity and an optional operation ID, never client-authored actions.
({String planId, String? opId}) validateApplyEnvelope(Object? params) {
  final object = _objectOrInvalid(params);
  if (object.keys.any((key) => key != 'planId' && key != 'opId')) {
    throw BackendError.invalidParams();
  }
  final planId = object['planId'];
  if (planId is! String || planId.isEmpty) {
    throw BackendError.invalidParams();
  }
  final opId = object['opId'];
  if (opId == null) {
    return (planId: planId, opId: null);
  }
  if (opId is! String || opId.isEmpty) {
    throw BackendError.invalidParams();
  }
  return (planId: planId, opId: opId);
}

// ---- Request shapes (serde `deny_unknown_fields`) --------------------------

Map<String, Object?> _objectOrInvalid(Object? params) {
  if (params is! Map) {
    throw BackendError.invalidParams();
  }
  return Map<String, Object?>.from(params);
}

/// Decodes a request object: unknown keys and wrong types are invalid params.
Map<String, Object?> _decodeFields(
  Object? params, {
  required Set<String> required,
  Set<String> optional = const {},
  Map<String, bool Function(Object?)> checks = const {},
}) {
  final object = _objectOrInvalid(params);
  final allowed = {...required, ...optional};
  if (object.keys.any((key) => !allowed.contains(key))) {
    throw BackendError.invalidParams();
  }
  for (final key in required) {
    if (!object.containsKey(key)) {
      throw BackendError.invalidParams();
    }
  }
  for (final entry in checks.entries) {
    final value = object[entry.key];
    if (value != null && !entry.value(value)) {
      throw BackendError.invalidParams();
    }
  }
  return object;
}

bool _isString(Object? value) => value is String;
bool _isBool(Object? value) => value is bool;
bool _isNum(Object? value) => value is num;
bool _isObject(Object? value) => value is Map;
bool _isStringList(Object? value) => value is List && value.every(_isString);

void _decodeEmpty(Object? params) {
  if (params is Map && params.isEmpty) {
    return;
  }
  throw BackendError.invalidParams();
}

String _nonEmptyOperationId(Object? value) {
  if (value is! String || value.isEmpty || value.runes.length > 128) {
    throw BackendError.validation('operation ID is invalid');
  }
  return value;
}

/// Decoded `settings/update` body. A missing key means "unchanged"; an explicit
/// null locale clears the locale.
final class SettingsUpdateRequest {
  const SettingsUpdateRequest({
    this.locale = _missing,
    this.theme,
    this.font,
    this.fontScale,
    this.compact,
    this.startWithWindows,
    this.replicaSyncEnabled,
  });

  static const Object _missing = Object();

  final Object? locale;
  final String? theme;
  final String? font;
  final double? fontScale;
  final bool? compact;
  final bool? startWithWindows;
  final bool? replicaSyncEnabled;

  bool get hasLocale => !identical(locale, _missing);

  factory SettingsUpdateRequest.decode(Object? params) {
    final object = _decodeFields(
      params,
      required: const {},
      optional: const {
        'locale',
        'theme',
        'font',
        'fontScale',
        'compact',
        'startWithWindows',
        'replicaSyncEnabled',
      },
      checks: {
        'locale': (value) => value is String,
        'theme': _isString,
        'font': _isString,
        'fontScale': _isNum,
        'compact': _isBool,
        'startWithWindows': _isBool,
        'replicaSyncEnabled': _isBool,
      },
    );
    final hasLocale = object.containsKey('locale');
    return SettingsUpdateRequest(
      locale: hasLocale ? object['locale'] : _missing,
      theme: object['theme'] as String?,
      font: object['font'] as String?,
      fontScale: (object['fontScale'] as num?)?.toDouble(),
      compact: object['compact'] as bool?,
      startWithWindows: object['startWithWindows'] as bool?,
      replicaSyncEnabled: object['replicaSyncEnabled'] as bool?,
    );
  }
}

/// Request shape of `settings/saveParams`.
final class SaveParamsRequest {
  const SaveParamsRequest({required this.opId, required this.values});

  final String opId;
  final Map<String, Object?> values;

  factory SaveParamsRequest.decode(Object? params) {
    final object = _decodeFields(
      params,
      required: const {'opId', 'values'},
      checks: {'opId': _isString, 'values': _isObject},
    );
    return SaveParamsRequest(
      opId: object['opId']! as String,
      values: Map<String, Object?>.from(object['values']! as Map),
    );
  }
}

/// Request shape of `settings/savePreset`.
final class SavePresetRequest {
  const SavePresetRequest({
    required this.opId,
    required this.name,
    required this.values,
    required this.replace,
  });

  final String opId;
  final String name;
  final Map<String, Object?> values;
  final bool replace;

  factory SavePresetRequest.decode(Object? params) {
    final object = _decodeFields(
      params,
      required: const {'opId', 'name', 'values'},
      optional: const {'replace'},
      checks: {
        'opId': _isString,
        'name': _isString,
        'values': _isObject,
        'replace': _isBool,
      },
    );
    return SavePresetRequest(
      opId: object['opId']! as String,
      name: object['name']! as String,
      values: Map<String, Object?>.from(object['values']! as Map),
      replace: (object['replace'] as bool?) ?? false,
    );
  }
}

/// Request shape of `settings/deletePreset`.
final class DeletePresetRequest {
  const DeletePresetRequest({required this.opId, required this.presetId});

  final String opId;
  final String presetId;

  factory DeletePresetRequest.decode(Object? params) {
    final object = _decodeFields(
      params,
      required: const {'opId', 'presetId'},
      checks: {'opId': _isString, 'presetId': _isString},
    );
    return DeletePresetRequest(
      opId: object['opId']! as String,
      presetId: object['presetId']! as String,
    );
  }
}

/// Request shape of `auth/setFlags`.
final class SetFlagsRequest {
  const SetFlagsRequest({required this.userId, this.allowed, this.isAdmin});

  final String userId;
  final bool? allowed;
  final bool? isAdmin;

  factory SetFlagsRequest.decode(Object? params) {
    final object = _decodeFields(
      params,
      required: const {'userId'},
      optional: const {'allowed', 'isAdmin'},
      checks: {
        'userId': _isString,
        'allowed': _isBool,
        'isAdmin': _isBool,
      },
    );
    return SetFlagsRequest(
      userId: object['userId']! as String,
      allowed: object['allowed'] as bool?,
      isAdmin: object['isAdmin'] as bool?,
    );
  }
}

/// Request shape of `paths/select`.
final class SelectPathsRequest {
  const SelectPathsRequest({
    required this.opId,
    required this.paramName,
    required this.droppedPaths,
  });

  final String opId;
  final String paramName;
  final List<String> droppedPaths;

  factory SelectPathsRequest.decode(Object? params) {
    final object = _decodeFields(
      params,
      required: const {'opId', 'paramName'},
      optional: const {'droppedPaths'},
      checks: {
        'opId': _isString,
        'paramName': _isString,
        'droppedPaths': _isStringList,
      },
    );
    return SelectPathsRequest(
      opId: object['opId']! as String,
      paramName: object['paramName']! as String,
      droppedPaths:
          (object['droppedPaths'] as List?)?.cast<String>() ?? const [],
    );
  }
}

/// Request shape of `auth/signIn`.
final class SignInRequest {
  const SignInRequest({required this.identifier, required this.password});

  final String identifier;
  final String password;

  factory SignInRequest.decode(Object? params) {
    final object = _decodeFields(
      params,
      required: const {'identifier', 'password'},
      checks: {'identifier': _isString, 'password': _isString},
    );
    return SignInRequest(
      identifier: object['identifier']! as String,
      password: object['password']! as String,
    );
  }
}

/// Request shape of `support/copyDiagnostics`.
final class CopyDiagnosticsRequest {
  const CopyDiagnosticsRequest({required this.content});

  final String content;

  factory CopyDiagnosticsRequest.decode(Object? params) {
    final object = _decodeFields(
      params,
      required: const {'content'},
      checks: {'content': _isString},
    );
    return CopyDiagnosticsRequest(content: object['content']! as String);
  }
}

/// Request shape of `engine/cancel`. Exactly one of planId or opId is used.
final class CancelRequest {
  const CancelRequest({this.planId, this.opId});

  final String? planId;
  final String? opId;

  factory CancelRequest.decode(Object? params) {
    final object = _objectOrInvalid(params);
    final planId = object['planId'];
    final opId = object['opId'];
    if (planId is String && planId.isNotEmpty) {
      return CancelRequest(planId: planId);
    }
    if (opId is String && opId.isNotEmpty) {
      return CancelRequest(opId: opId);
    }
    throw BackendError.invalidParams();
  }
}

/// Request shape of `engine/printQueueDiscard` and `engine/printQueueResume`.
String decodeQueueId(Object? params) {
  final object = _decodeFields(
    params,
    required: const {'queueId'},
    checks: {'queueId': _isString},
  );
  return object['queueId']! as String;
}

/// Request shape of `history/list`.
int decodeHistoryLimit(Object? params) {
  final object = _decodeFields(
    params,
    required: const {},
    optional: const {'limit'},
    checks: {'limit': (value) => value is int && value >= 0},
  );
  final limit = object['limit'];
  return limit is int ? limit : 50;
}

/// Request shape of `paths/reveal`.
String decodeRevealSelectionId(Object? params) {
  final object = _decodeFields(
    params,
    required: const {'selectionId'},
    checks: {'selectionId': _isString},
  );
  return object['selectionId']! as String;
}

/// Request shape of `engine/reviewScan`: a destination selection.
PathSelection decodeReviewScanDestination(Object? params) {
  final object = _decodeFields(
    params,
    required: const {'destination'},
    checks: {'destination': _isObject},
  );
  return _decodeSelection(object['destination']);
}

/// Decodes a `PathSelection` object: exactly selectionId, displayPath, name.
PathSelection _decodeSelection(Object? value) {
  if (value is! Map) {
    throw BackendError.invalidParams();
  }
  try {
    return PathSelection.fromJson(Map<String, Object?>.from(value));
  } on FormatException {
    throw BackendError.invalidParams();
  } on TypeError {
    throw BackendError.invalidParams();
  }
}

// ---- Ports ---------------------------------------------------------------

/// Printer inventory and print-queue state that depends on the platform print
/// adapter. Implemented by the print module (E11).
final class PrinterInventory {
  const PrinterInventory({required this.printers, this.defaultPrinter});

  final List<String> printers;
  final String? defaultPrinter;
}

/// Operations that run in native engine modules. [UnwiredNativeOperations]
/// is the default; each module owner supplies the real implementation.
abstract interface class NativeOperations {
  /// Printer facts for registry schemas. Empty when no inventory exists.
  RegistryFacts registryFacts();

  /// Printer diagnostics for the support bundle.
  PrinterDiagnostic printerDiagnostics(CallerIdentity? caller);

  Future<Object?> preview(
    CallerIdentity caller,
    String opId,
    JsonObject params,
    ProgressHandler? onProgress,
  );

  Future<({JsonObject result, String approvedOpId})> apply(
    CallerIdentity caller,
    String planId,
    String? opId,
    ProgressHandler? onProgress,
  );

  /// Records the applied result to cloud history. Sets `cloudSynced`.
  Future<void> syncResult(String approvedOpId, JsonObject result);

  bool cancelPlan(String planId);

  bool cancelOperation(String opId);

  Future<Object?> revise(CallerIdentity caller, JsonObject params);

  Future<Object?> copyTable(CallerIdentity caller, JsonObject params);

  Future<Object?> rfiReplyPreview(CallerIdentity caller, JsonObject params);

  /// Returns null when the journal holds an unsupported legacy action.
  Future<Object?> undo(CallerIdentity caller, JsonObject params);

  Future<Object?> reviewScan(CallerIdentity caller, JsonObject params);

  Future<Object?> reviewPreview(CallerIdentity caller, JsonObject params);

  Future<Object?> findRfiPrintFiles(CallerIdentity caller, JsonObject params);

  Future<Object?> printQueueLatest(CallerIdentity caller);

  Future<Object?> printQueueDiscard(CallerIdentity caller, String queueId);

  Future<Object?> printQueueResume(CallerIdentity caller, String queueId);

  Future<Object?> recovery(CallerIdentity caller);

  void revokeSessionAuthority(CallerIdentity caller);
}

/// OS services for dialogs, opening paths, clipboard, login item, and OCR.
abstract interface class PlatformServices {
  /// Shows a native picker. Returns the picked paths, empty when cancelled.
  Future<List<String>> pickPaths({
    required PathSelectionMode mode,
    DialogFilter? filter,
  });

  Future<void> openPath(String path);

  Future<void> writeClipboard(String content);

  void setStartWithWindows(bool enabled);

  /// Whether OCR is available and whether its language data is installed.
  ({bool available, bool languageData}) ocrSupport();
}

/// Path selection modes (`PathSelectionMode` in native.rs).
enum PathSelectionMode { folder, file, files }

/// Native seam default: each operation fails until its module is wired.
final class UnwiredNativeOperations implements NativeOperations {
  const UnwiredNativeOperations();

  Never _unwired(String name) => throw BackendError(
    'engine_not_wired',
    '$name is not wired in this engine build',
  );

  @override
  RegistryFacts registryFacts() => const RegistryFacts();

  @override
  PrinterDiagnostic printerDiagnostics(CallerIdentity? caller) =>
      const PrinterDiagnostic(
        inventoryStatus: 'unavailable',
        printerCount: 0,
        defaultPrinterAvailable: false,
      );

  @override
  Future<Object?> preview(
    CallerIdentity caller,
    String opId,
    JsonObject params,
    ProgressHandler? onProgress,
  ) async => _unwired('engine/preview');

  @override
  Future<({JsonObject result, String approvedOpId})> apply(
    CallerIdentity caller,
    String planId,
    String? opId,
    ProgressHandler? onProgress,
  ) async => _unwired('engine/apply');

  @override
  Future<void> syncResult(String approvedOpId, JsonObject result) async {
    result['cloudSynced'] = false;
  }

  @override
  bool cancelPlan(String planId) => false;

  @override
  bool cancelOperation(String opId) => false;

  @override
  Future<Object?> revise(CallerIdentity caller, JsonObject params) async =>
      _unwired('engine/revise');

  @override
  Future<Object?> copyTable(CallerIdentity caller, JsonObject params) async =>
      _unwired('engine/copyTable');

  @override
  Future<Object?> rfiReplyPreview(
    CallerIdentity caller,
    JsonObject params,
  ) async => _unwired('engine/rfiReplyPreview');

  @override
  Future<Object?> undo(CallerIdentity caller, JsonObject params) async =>
      _unwired('engine/undo');

  @override
  Future<Object?> reviewScan(CallerIdentity caller, JsonObject params) async =>
      _unwired('engine/reviewScan');

  @override
  Future<Object?> reviewPreview(
    CallerIdentity caller,
    JsonObject params,
  ) async => _unwired('engine/reviewPreview');

  @override
  Future<Object?> findRfiPrintFiles(
    CallerIdentity caller,
    JsonObject params,
  ) async => _unwired('rfi/printFind');

  @override
  Future<Object?> printQueueLatest(CallerIdentity caller) async =>
      _unwired('engine/printQueueLatest');

  @override
  Future<Object?> printQueueDiscard(
    CallerIdentity caller,
    String queueId,
  ) async => _unwired('engine/printQueueDiscard');

  @override
  Future<Object?> printQueueResume(
    CallerIdentity caller,
    String queueId,
  ) async => _unwired('engine/printQueueResume');

  @override
  Future<Object?> recovery(CallerIdentity caller) async =>
      _unwired('recovery/list');

  @override
  void revokeSessionAuthority(CallerIdentity caller) {}
}

/// Platform default: each service fails until the platform adapter is wired.
final class UnwiredPlatformServices implements PlatformServices {
  const UnwiredPlatformServices();

  @override
  Future<List<String>> pickPaths({
    required PathSelectionMode mode,
    DialogFilter? filter,
  }) => Future.error(
    BackendError.requestFailed('native path dialog is not wired'),
  );

  @override
  Future<void> openPath(String path) => Future.error(
    BackendError.requestFailed('selected path could not be opened'),
  );

  @override
  Future<void> writeClipboard(String content) => Future.error(
    BackendError.requestFailed('clipboard is not wired in this engine build'),
  );

  @override
  void setStartWithWindows(bool enabled) => throw BackendError.settingsWrite();

  @override
  ({bool available, bool languageData}) ocrSupport() =>
      (available: false, languageData: false);
}

/// Session-scoped caller identity (`authenticated_caller_identity`).
CallerIdentity _authenticatedCaller(auth.AuthState state, String sessionId) {
  final caller = _signedInCaller(state, sessionId);
  if (!state.allowed) {
    throw BackendError.permissionDenied();
  }
  return caller;
}

CallerIdentity _signedInCaller(auth.AuthState state, String sessionId) {
  if (!state.signedIn) {
    throw BackendError.authRequired();
  }
  final ownerId = state.session?.userId;
  if (ownerId == null || ownerId.isEmpty || ownerId == 'anonymous') {
    throw BackendError.authRequired();
  }
  if (sessionId.isEmpty) {
    throw BackendError.authRequired();
  }
  return CallerIdentity(ownerId, sessionId);
}

/// Port of `BackendRouter`. Routes each method and enforces the gate first.
final class BackendRouter {
  BackendRouter({
    required this.settings,
    required this.authService,
    required this.selections,
    required this.native,
    required this.platform,
    required this.history,
    required this.now,
  }) : _sessionId = const Uuid().v4();

  final SettingsStore settings;
  final auth.AuthService authService;
  final PathSelectionStore selections;
  final NativeOperations native;
  final PlatformServices platform;
  final CloudHistory? history;
  final int Function() now;
  final RecentErrorCodes recentErrorCodes = RecentErrorCodes();
  String _sessionId;

  Future<Object?> call(
    BackendMethod method,
    Object? params,
    ProgressHandler? onProgress,
  ) async {
    final state = authService.state();
    method.authorize(signedIn: state.signedIn, allowed: state.allowed);
    return switch (method) {
      BackendMethod.appBootstrap => _bootstrap(params),
      BackendMethod.settingsUpdate => _settingsUpdate(params),
      BackendMethod.settingsReset => _settingsReset(params),
      BackendMethod.settingsReplicaStatus => _replicaStatus(params),
      BackendMethod.settingsParams => _settingsParams(params),
      BackendMethod.settingsSaveParams => _saveParams(params),
      BackendMethod.settingsPresets => _presets(params),
      BackendMethod.settingsSavePreset => _savePreset(params),
      BackendMethod.settingsDeletePreset => _deletePreset(params),
      BackendMethod.supportDiagnostics => _supportDiagnostics(params),
      BackendMethod.supportCopyDiagnostics => _copyDiagnostics(params),
      BackendMethod.authSignIn => _signIn(params),
      BackendMethod.authSignUp => _signUp(params),
      BackendMethod.authSignOut => _signOut(params),
      BackendMethod.authUsernameAvailable => _usernameAvailable(params),
      BackendMethod.authLoadMe => _loadMe(params),
      BackendMethod.authSetUsername => _setUsername(params),
      BackendMethod.authListUsers => _listUsers(params),
      BackendMethod.authSetFlags => _setFlags(params),
      BackendMethod.enginePreview => _preview(params, onProgress),
      BackendMethod.engineApply => _apply(params, onProgress),
      BackendMethod.engineCancel => _cancel(params),
      BackendMethod.enginePrintQueueLatest => _queueLatest(params),
      BackendMethod.enginePrintQueueDiscard => _queueDiscard(params),
      BackendMethod.enginePrintQueueResume => _queueResume(params),
      BackendMethod.engineRevise => native.revise(_caller(), _map(params)),
      BackendMethod.engineCopyTable => native.copyTable(_caller(), _map(params)),
      BackendMethod.engineRfiReplyPreview =>
        native.rfiReplyPreview(_caller(), _map(params)),
      BackendMethod.engineReviewScan => native.reviewScan(_caller(), _map(params)),
      BackendMethod.engineReviewPreview =>
        native.reviewPreview(_caller(), _map(params)),
      BackendMethod.engineUndo => _undo(params),
      BackendMethod.historyList => _history(params),
      BackendMethod.pathsSelect => _selectPaths(params),
      BackendMethod.pathsReveal => _revealPath(params),
      BackendMethod.recoveryList => _recovery(params),
      BackendMethod.rfiPrintFind =>
        native.findRfiPrintFiles(_caller(), _map(params)),
    };
  }

  // Each handler below is one method's port. Helpers follow.

  CallerIdentity _caller() => _authenticatedCaller(authService.state(), _sessionId);

  CallerIdentity? _callerOrNull() {
    try {
      return _caller();
    } on BackendError {
      return null;
    }
  }

  JsonObject _map(Object? params) => _objectOrInvalid(params);

  Future<Object?> _bootstrap(Object? params) async {
    _decodeEmpty(params);
    final state = await authService.restore(now());
    final facts = native.registryFacts();
    return <String, Object?>{
      'protocol': 1,
      'version': docflowVersion,
      'auth': state.toJson(),
      'prefs': settings.preferences().toJson(),
      'operations': [
        for (final operation in operationRegistry(facts)) operation.toJson(),
      ],
    };
  }

  Future<Object?> _settingsUpdate(Object? params) async {
    final request = SettingsUpdateRequest.decode(params);
    final current = settings.preferences();
    final previousStartup = current.startWithWindows;
    var locale = current.locale;
    if (request.hasLocale) {
      locale = request.locale as String?;
    }
    final startWithWindows = request.startWithWindows;
    if (startWithWindows != null) {
      try {
        platform.setStartWithWindows(startWithWindows);
      } on BackendError {
        throw BackendError.settingsWrite();
      }
    }
    var replicaSyncEnabled = current.replicaSyncEnabled;
    if (request.replicaSyncEnabled != null) {
      if (request.replicaSyncEnabled! &&
          (settings.section('replicaAutomation')?['profile'] == null)) {
        throw BackendError.validation('error.replica.profileMissing');
      }
      replicaSyncEnabled = request.replicaSyncEnabled!;
    }
    final next = AppPrefs(
      locale: locale,
      theme: request.theme ?? current.theme,
      font: request.font ?? current.font,
      fontScale: request.fontScale ?? current.fontScale,
      compact: request.compact ?? current.compact,
      startWithWindows: startWithWindows ?? current.startWithWindows,
      replicaSyncEnabled: replicaSyncEnabled,
    );
    try {
      settings.savePreferences(next);
    } on Object {
      if (startWithWindows != null) {
        try {
          platform.setStartWithWindows(previousStartup);
        } on BackendError {
          // Best-effort restore, as the Rust `let _ =` does.
        }
      }
      throw BackendError.settingsWrite();
    }
    return settings.preferences().toJson();
  }

  Future<Object?> _settingsReset(Object? params) async {
    _decodeEmpty(params);
    final previous = settings.preferences();
    try {
      platform.setStartWithWindows(false);
    } on BackendError {
      throw BackendError.settingsWrite();
    }
    try {
      settings.savePreferences(const AppPrefs());
    } on Object {
      try {
        platform.setStartWithWindows(previous.startWithWindows);
      } on BackendError {
        // Best-effort restore.
      }
      throw BackendError.settingsWrite();
    }
    return settings.preferences().toJson();
  }

  Future<Object?> _replicaStatus(Object? params) async {
    _decodeEmpty(params);
    final preferences = settings.preferences();
    final section = settings.section('replicaAutomation');
    final configured = section?.containsKey('profile') ?? false;
    final status = !configured
        ? 'unconfigured'
        : !preferences.replicaSyncEnabled
        ? 'disabled'
        : (section?['status'] as String?) ?? 'idle';
    return <String, Object?>{
      'enabled': preferences.replicaSyncEnabled,
      'configured': configured,
      'status': status,
      'lastSuccessUnix': section?['lastSuccessUnix'] is int
          ? section!['lastSuccessUnix']
          : null,
      'lastErrorCode': section?['lastErrorCode'] as String?,
    };
  }

  Future<Object?> _settingsParams(Object? params) async {
    final object = _decodeFields(
      params,
      required: const {'opId'},
      checks: {'opId': _isString},
    );
    final opId = _nonEmptyOperationId(object['opId']);
    return _selectedParams(opId);
  }

  Future<Object?> _saveParams(Object? params) async {
    final request = SaveParamsRequest.decode(params);
    final opId = _nonEmptyOperationId(request.opId);
    final sanitized = _sanitizeSaveParams(opId, request.values);
    try {
      settings.saveParams(opId, sanitized);
    } on Object {
      throw BackendError.settingsWrite();
    }
    return <String, Object?>{'ok': true};
  }

  Future<Object?> _presets(Object? params) async {
    final object = _decodeFields(
      params,
      required: const {'opId'},
      checks: {'opId': _isString},
    );
    final opId = _nonEmptyOperationId(object['opId']);
    final operation = _presetOperation(opId);
    try {
      return [
        for (final preset in settings.presetsFor(operation)) preset.toJson(),
      ];
    } on PresetStoreError catch (error) {
      throw _presetError(error);
    }
  }

  Future<Object?> _savePreset(Object? params) async {
    final request = SavePresetRequest.decode(params);
    final opId = _nonEmptyOperationId(request.opId);
    final operation = _presetOperation(opId);
    try {
      return settings
          .savePreset(
            operation,
            request.name,
            request.values,
            replace: request.replace,
          )
          .toJson();
    } on PresetStoreError catch (error) {
      throw _presetError(error);
    }
  }

  Future<Object?> _deletePreset(Object? params) async {
    final request = DeletePresetRequest.decode(params);
    final opId = _nonEmptyOperationId(request.opId);
    final operation = _presetOperation(opId);
    try {
      settings.deletePreset(operation, request.presetId);
    } on PresetStoreError catch (error) {
      throw _presetError(error);
    }
    return <String, Object?>{'ok': true};
  }

  Future<Object?> _supportDiagnostics(Object? params) async {
    _decodeEmpty(params);
    final printer = native.printerDiagnostics(_callerOrNull());
    final ocr = platform.ocrSupport();
    final bundle = buildBundle(
      DiagnosticsInput(
        version: docflowVersion,
        platformOs: currentPlatformNames().os,
        platformArch: currentPlatformNames().arch,
        ocrAvailable: ocr.available,
        ocrLanguageDataAvailable: ocr.languageData,
        recentErrorCodes: recentErrorCodes.snapshot(),
        printer: printer,
      ),
    );
    return bundle.toJson();
  }

  Future<Object?> _copyDiagnostics(Object? params) async {
    final request = CopyDiagnosticsRequest.decode(params);
    validateBundleContent(request.content);
    await platform.writeClipboard(request.content);
    return <String, Object?>{'ok': true};
  }

  Future<Object?> _signIn(Object? params) async {
    final previous = _callerOrNull();
    if (previous != null) {
      _revokeSession(previous);
    }
    _sessionId = const Uuid().v4();
    final request = SignInRequest.decode(params);
    final state = await authService.signIn(
      request.identifier,
      request.password,
      now(),
    );
    return state.toJson();
  }

  Future<Object?> _signUp(Object? params) async {
    final object = _decodeFields(
      params,
      required: const {'email', 'password', 'username'},
      checks: {
        'email': _isString,
        'password': _isString,
        'username': _isString,
      },
    );
    await authService.signUp(
      object['email']! as String,
      object['password']! as String,
      object['username']! as String,
    );
    return <String, Object?>{'ok': true};
  }

  Future<Object?> _signOut(Object? params) async {
    _decodeEmpty(params);
    final caller = _callerOrNull();
    if (caller != null) {
      _revokeSession(caller);
    }
    _sessionId = const Uuid().v4();
    final state = await authService.signOut();
    return state.toJson();
  }

  Future<Object?> _usernameAvailable(Object? params) async {
    final object = _decodeFields(
      params,
      required: const {'username'},
      checks: {'username': _isString},
    );
    return authService.usernameAvailable(object['username']! as String);
  }

  Future<Object?> _loadMe(Object? params) async {
    _decodeEmpty(params);
    return (await authService.loadMe(now())).toJson();
  }

  Future<Object?> _setUsername(Object? params) async {
    final object = _decodeFields(
      params,
      required: const {'username'},
      checks: {'username': _isString},
    );
    return (await authService.setUsername(
      object['username']! as String,
      now(),
    )).toJson();
  }

  Future<Object?> _listUsers(Object? params) async {
    _decodeEmpty(params);
    return [
      for (final user in await authService.listUsers(now())) user.toJson(),
    ];
  }

  Future<Object?> _setFlags(Object? params) async {
    final request = SetFlagsRequest.decode(params);
    await authService.setFlags(
      request.userId,
      allowed: request.allowed,
      isAdmin: request.isAdmin,
      now: now(),
    );
    return <String, Object?>{'ok': true};
  }

  Future<Object?> _preview(Object? params, ProgressHandler? onProgress) async {
    final object = _objectOrInvalid(params);
    final opId = object['opId'];
    if (opId is! String) {
      throw BackendError.validation('unknown operation ID');
    }
    final known = operationRegistry(
      const RegistryFacts(),
    ).any((operation) => operation.id == opId);
    if (!known) {
      throw BackendError.validation('unknown operation ID');
    }
    return native.preview(_caller(), opId, object, onProgress);
  }

  Future<Object?> _apply(Object? params, ProgressHandler? onProgress) async {
    final envelope = validateApplyEnvelope(params);
    final outcome = await native.apply(
      _caller(),
      envelope.planId,
      envelope.opId,
      onProgress,
    );
    await native.syncResult(outcome.approvedOpId, outcome.result);
    return outcome.result;
  }

  Future<Object?> _cancel(Object? params) async {
    final request = CancelRequest.decode(params);
    if (request.planId != null) {
      return <String, Object?>{'accepted': native.cancelPlan(request.planId!)};
    }
    return <String, Object?>{
      'accepted': native.cancelOperation(request.opId!),
    };
  }

  Future<Object?> _queueLatest(Object? params) async {
    _decodeEmpty(params);
    return native.printQueueLatest(_caller());
  }

  Future<Object?> _queueDiscard(Object? params) async =>
      native.printQueueDiscard(_caller(), decodeQueueId(params));

  Future<Object?> _queueResume(Object? params) async =>
      native.printQueueResume(_caller(), decodeQueueId(params));

  Future<Object?> _undo(Object? params) async {
    final result = await native.undo(_caller(), _map(params));
    if (result == null) {
      throw BackendError.validation(
        'journal contains an unsupported legacy recovery action',
      );
    }
    return result;
  }

  Future<Object?> _history(Object? params) async {
    final limit = decodeHistoryLimit(params);
    final cloud = history;
    if (cloud == null) {
      throw const BackendError(
        'engine_not_wired',
        'history/list is not wired in this engine build',
      );
    }
    return (await cloud.list(limit, now())).toJson();
  }

  Future<Object?> _selectPaths(Object? params) async {
    final request = SelectPathsRequest.decode(params);
    final mode = _selectionMode(request.opId, request.paramName);
    final filter = pathDialogFilter(request.opId, request.paramName);
    final List<String> paths;
    if (request.droppedPaths.isNotEmpty) {
      paths = _validateDroppedPaths(request.droppedPaths, mode, filter);
    } else {
      paths = await platform.pickPaths(mode: mode, filter: filter);
    }
    final kind = mode == PathSelectionMode.folder
        ? SelectionKind.folder
        : SelectionKind.file;
    if (mode != PathSelectionMode.files && paths.length > 1) {
      throw BackendError.invalidParams();
    }
    final caller = _caller();
    return [
      for (final path in paths)
        selections
            .issue(caller, request.opId, request.paramName, path, kind)
            .toJson(),
    ];
  }

  Future<Object?> _revealPath(Object? params) async {
    final selectionId = decodeRevealSelectionId(params);
    final path = selections.resolveScope(
      selectionId,
      _caller(),
      SelectionScope.revealReviewItem,
    );
    try {
      await platform.openPath(path);
    } on BackendError {
      throw BackendError.requestFailed('selected path could not be opened');
    }
    return <String, Object?>{'ok': true};
  }

  Future<Object?> _recovery(Object? params) async {
    _decodeEmpty(params);
    return native.recovery(_caller());
  }

  // ---- helpers ----------------------------------------------------------

  void _revokeSession(CallerIdentity caller) {
    native.revokeSessionAuthority(caller);
    selections.revoke(caller);
  }

  BackendError _presetError(PresetStoreError error) => switch (error.kind) {
    PresetStoreErrorKind.invalidName => BackendError.validation(
      'error.preset.nameInvalid',
    ),
    PresetStoreErrorKind.invalidValue => BackendError.validation(
      'preset value is invalid',
    ),
    PresetStoreErrorKind.duplicateName => BackendError.validation(
      'error.preset.duplicate',
    ),
    PresetStoreErrorKind.limitReached => BackendError.validation(
      'error.preset.limit',
    ),
    PresetStoreErrorKind.missing => BackendError.validation(
      'error.preset.missing',
    ),
    PresetStoreErrorKind.write => BackendError.settingsWrite(),
  };

  OperationDef _presetOperation(String opId) {
    final operation = operationRegistry(
      native.registryFacts(),
    ).where((operation) => operation.id == opId).firstOrNull;
    if (operation == null) {
      throw BackendError.validation('unknown operation ID');
    }
    if (!operation.supportsPresets) {
      throw BackendError.validation('operation does not support presets');
    }
    return operation;
  }

  PathSelectionMode _selectionMode(String opId, String paramName) {
    final kind = _pathParameterKind(opId, paramName);
    return switch (kind) {
      'folder' => PathSelectionMode.folder,
      'file' => PathSelectionMode.file,
      'files' => PathSelectionMode.files,
      _ => throw BackendError.invalidParams(),
    };
  }

  /// Kind of a path parameter: `folder`, `file`, `files`, or null.
  String? _pathParameterKind(String opId, String paramName) {
    if ((opId == 'rfi.review' || opId == 'history.undo') &&
        paramName == 'destination') {
      return 'folder';
    }
    for (final operation in operationRegistry(const RegistryFacts())) {
      if (operation.id != opId) {
        continue;
      }
      for (final parameter in operation.schema.params) {
        if (parameter.name == paramName) {
          return parameter.kind.wire;
        }
      }
    }
    return null;
  }

  List<String> _validateDroppedPaths(
    List<String> dropped,
    PathSelectionMode mode,
    DialogFilter? filter,
  ) {
    if (dropped.length > 1000) {
      throw BackendError.invalidParams();
    }
    if (mode != PathSelectionMode.files && dropped.length > 1) {
      throw BackendError.invalidParams();
    }
    final resolved = <String>[];
    for (final path in dropped) {
      final type = FileSystemEntity.typeSync(path);
      if (type == FileSystemEntityType.notFound) {
        throw BackendError.pathInvalid('dropped path does not exist');
      }
      switch (mode) {
        case PathSelectionMode.folder:
          if (type != FileSystemEntityType.directory) {
            throw BackendError.pathInvalid('dropped path must be a folder');
          }
        case PathSelectionMode.file || PathSelectionMode.files:
          if (type != FileSystemEntityType.file) {
            throw BackendError.pathInvalid('dropped path must be a file');
          }
      }
      if (filter != null) {
        final extension = p.extension(path).replaceFirst('.', '').toLowerCase();
        if (!filter.extensions.any(
          (allowed) => allowed.toLowerCase() == extension,
        )) {
          throw BackendError.pathInvalid('dropped file type is not accepted');
        }
      }
      resolved.add(path);
    }
    return resolved;
  }

  JsonObject _selectedParams(String opId) {
    final operation = _operationFor(opId);
    final caller = _caller();
    final saved = settings.paramsFor(opId);
    final result = <String, Object?>{};
    for (final param in operation.schema.params) {
      if (!param.remember) {
        continue;
      }
      final raw = saved[param.name];
      if (raw == null) {
        continue;
      }
      switch (param.kind) {
        case ParamKind.secret || ParamKind.longText:
          continue;
        case ParamKind.folder || ParamKind.file:
          final kind = param.kind == ParamKind.folder
              ? SelectionKind.folder
              : SelectionKind.file;
          if (raw is String && _exists(raw)) {
            final selection = _tryIssue(caller, opId, param.name, raw, kind);
            if (selection != null) {
              result[param.name] = selection.toJson();
            }
          }
        case ParamKind.files:
          if (raw is List) {
            final selections = <Map<String, Object?>>[];
            for (final item in raw) {
              if (item is String && _exists(item)) {
                final selection = _tryIssue(
                  caller,
                  opId,
                  param.name,
                  item,
                  SelectionKind.file,
                );
                if (selection != null) {
                  selections.add(selection.toJson());
                }
              }
            }
            if (selections.isNotEmpty) {
              result[param.name] = selections;
            }
          }
        case ParamKind.sheetCounts:
          if (raw is Map && raw.values.every(_isCount)) {
            result[param.name] = Map<String, Object?>.from(raw);
          }
        case ParamKind.bool:
          if (raw is bool) {
            result[param.name] = raw;
          }
        case ParamKind.choice:
          if (raw is String &&
              (param.choices.isEmpty || param.choices.contains(raw))) {
            result[param.name] = raw;
          }
        case ParamKind.int:
          if (raw is num) {
            result[param.name] = raw;
          }
        case ParamKind.text:
          if (raw is String) {
            result[param.name] = raw;
          }
      }
    }
    return result;
  }

  JsonObject _sanitizeSaveParams(String opId, Map<String, Object?> input) {
    final operation = _operationFor(opId);
    final caller = _caller();
    final values = Map<String, Object?>.from(input);
    final sanitized = <String, Object?>{};
    for (final param in operation.schema.params) {
      if (!param.remember) {
        continue;
      }
      final raw = values.remove(param.name);
      if (raw == null) {
        continue;
      }
      switch (param.kind) {
        case ParamKind.secret || ParamKind.longText:
          continue;
        case ParamKind.folder || ParamKind.file:
          final kind = param.kind == ParamKind.folder
              ? SelectionKind.folder
              : SelectionKind.file;
          final selection = _tryDecodeSelection(raw);
          if (selection != null) {
            final path = _tryResolve(selection, caller, opId, param.name, kind);
            if (path != null) {
              sanitized[param.name] = path;
            } else {
              final previous = settings.paramsFor(opId)[param.name];
              if (previous is String &&
                  previous == selection.displayPath &&
                  _exists(previous)) {
                sanitized[param.name] = previous;
              }
            }
          } else if (raw is String && _exists(raw)) {
            final canonical = _canonical(raw);
            if (canonical != null) {
              sanitized[param.name] = canonical;
            }
          }
        case ParamKind.files:
          final decoded = _tryDecodeSelections(raw);
          if (decoded != null) {
            final paths = <String>[];
            final saved = settings.paramsFor(opId)[param.name];
            for (final selection in decoded) {
              final path = _tryResolve(
                selection,
                caller,
                opId,
                param.name,
                SelectionKind.file,
              );
              if (path != null) {
                paths.add(path);
              } else if (saved is List &&
                  saved.contains(selection.displayPath) &&
                  _exists(selection.displayPath)) {
                paths.add(selection.displayPath);
              }
            }
            if (paths.isNotEmpty) {
              sanitized[param.name] = paths;
            }
          } else if (raw is List) {
            final paths = <String>[];
            for (final item in raw) {
              if (item is String && _exists(item)) {
                final canonical = _canonical(item);
                if (canonical != null) {
                  paths.add(canonical);
                }
              }
            }
            if (paths.isNotEmpty) {
              sanitized[param.name] = paths;
            }
          }
        case ParamKind.sheetCounts:
          if (raw is Map && raw.values.every(_isCount)) {
            sanitized[param.name] = Map<String, Object?>.from(raw);
          }
        case ParamKind.bool:
          if (raw is bool) {
            sanitized[param.name] = raw;
          }
        case ParamKind.choice:
          if (raw is String &&
              (param.choices.isEmpty || param.choices.contains(raw))) {
            sanitized[param.name] = raw;
          }
        case ParamKind.int:
          if (raw is num) {
            sanitized[param.name] = raw;
          }
        case ParamKind.text:
          if (raw is String) {
            sanitized[param.name] = raw;
          }
      }
    }
    return sanitized;
  }

  OperationDef _operationFor(String opId) =>
      operationRegistry(native.registryFacts()).firstWhere(
        (operation) => operation.id == opId,
        orElse: () => throw BackendError.validation('unknown operation ID'),
      );

  PathSelection? _tryIssue(
    CallerIdentity caller,
    String opId,
    String name,
    String path,
    SelectionKind kind,
  ) {
    try {
      return selections.issue(caller, opId, name, path, kind);
    } on BackendError {
      return null;
    }
  }

  PathSelection? _tryDecodeSelection(Object? raw) {
    try {
      return _decodeSelection(raw);
    } on BackendError {
      return null;
    }
  }

  List<PathSelection>? _tryDecodeSelections(Object? raw) {
    if (raw is! List) {
      return null;
    }
    try {
      return [for (final item in raw) _decodeSelection(item)];
    } on BackendError {
      return null;
    }
  }

  String? _tryResolve(
    PathSelection selection,
    CallerIdentity caller,
    String opId,
    String name,
    SelectionKind kind,
  ) {
    try {
      return selections.resolve(selection, caller, opId, name, kind);
    } on BackendError {
      return null;
    }
  }

  bool _exists(String path) =>
      FileSystemEntity.typeSync(path) != FileSystemEntityType.notFound;

  String? _canonical(String path) {
    try {
      return File(path).resolveSymbolicLinksSync();
    } on FileSystemException {
      return null;
    }
  }

  bool _isCount(Object? value) => value is int && value >= 0;
}
