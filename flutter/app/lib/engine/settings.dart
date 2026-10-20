// Port of src-tauri/src/settings.rs.
//
// Non-secret settings in `~/.docflow/settings.json`: preferences, per-operation
// parameter memory, operation presets, and the auth session metadata section.
// The file is written compactly with object keys in sorted order, as serde_json
// does without `preserve_order`, so an existing settings file keeps its format.
// Every mutation builds the next document, persists it atomically, and only
// then replaces the in-memory copy. A failed write leaves memory and disk as
// they were.

import 'dart:convert';
import 'dart:io';

import 'package:path/path.dart' as p;
import 'package:uuid/uuid.dart';

import 'auth.dart' show AuthSettings;
import 'contracts.dart';
import 'credentials.dart' show StoredSession;

const double _minFontScale = 0.8;
const double _maxFontScale = 1.6;
const List<String> _locales = ['tr', 'en', 'ru', 'az'];
const List<String> _themes = ['system', 'light', 'dark'];
const List<String> _fonts = ['system', 'sans', 'serif', 'mono'];
const int _maxOperationPresets = 20;
const int _maxPresetNameChars = 64;
const int _maxPrintPrinterChars = 512;
const int _maxParamStringChars = 4096;
const int _maxParamListItems = 1000;
const int _maxParamObjectEntries = 64;

final RegExp _uuidPattern = RegExp(
  r'^(urn:uuid:)?\{?[0-9a-fA-F]{8}-?[0-9a-fA-F]{4}-?[0-9a-fA-F]{4}-?'
  r'[0-9a-fA-F]{4}-?[0-9a-fA-F]{12}\}?$',
);

/// Failure of a settings file read or write (`io::Error` in the Rust store).
final class SettingsIoException implements Exception {
  const SettingsIoException(this.message, [this.cause]);

  final String message;
  final Object? cause;

  @override
  String toString() => 'SettingsIoException: $message';
}

enum PresetStoreErrorKind {
  invalidName,
  invalidValue,
  duplicateName,
  limitReached,
  missing,
  write,
}

/// Port of `PresetStoreError`.
final class PresetStoreError implements Exception {
  const PresetStoreError(this.kind, [this.cause]);

  final PresetStoreErrorKind kind;
  final Object? cause;

  @override
  String toString() => switch (kind) {
    PresetStoreErrorKind.invalidName => 'preset name is invalid',
    PresetStoreErrorKind.invalidValue => 'preset value is invalid',
    PresetStoreErrorKind.duplicateName => 'preset name already exists',
    PresetStoreErrorKind.limitReached => 'preset limit reached',
    PresetStoreErrorKind.missing => 'preset not found',
    PresetStoreErrorKind.write => 'settings write failed: $cause',
  };
}

/// Writes [data] to [path]. Throws on failure.
typedef SettingsWriter = void Function(String path, JsonObject data);

/// Path of the settings file: `$HOME/.docflow/settings.json`.
String defaultSettingsPath() {
  final env = Platform.environment;
  final home = env['HOME'] ?? env['USERPROFILE'] ?? Directory.systemTemp.path;
  return p.join(home, '.docflow', 'settings.json');
}

// ---- Canonical JSON -------------------------------------------------------

/// Returns [value] with every map rebuilt in sorted key order, recursively.
/// Matches the key order of serde_json's default `Map` (a BTreeMap).
Object? sortKeysDeep(Object? value) {
  if (value is Map) {
    final keys = value.keys.map((key) => key.toString()).toList()..sort();
    return <String, Object?>{
      for (final key in keys) key: sortKeysDeep(value[key]),
    };
  }
  if (value is List) {
    return [for (final item in value) sortKeysDeep(item)];
  }
  return value;
}

/// Compact JSON with sorted keys (`serde_json::to_vec`).
String canonicalJson(Object? value) => jsonEncode(sortKeysDeep(value));

/// Pretty JSON with two-space indent and sorted keys
/// (`serde_json::to_string_pretty`).
String canonicalJsonPretty(Object? value) =>
    JsonEncoder.withIndent('  ').convert(sortKeysDeep(value));

/// Deep copy of a JSON value, so a candidate document never aliases the
/// committed one.
Object? _deepCopy(Object? value) {
  if (value is Map) {
    return <String, Object?>{
      for (final entry in value.entries)
        entry.key.toString(): _deepCopy(entry.value),
    };
  }
  if (value is List) {
    return [for (final item in value) _deepCopy(item)];
  }
  return value;
}

/// Structural JSON equality (`serde_json::Value ==`).
bool jsonDeepEquals(Object? left, Object? right) {
  if (left is Map && right is Map) {
    if (left.length != right.length) {
      return false;
    }
    for (final key in left.keys) {
      if (!right.containsKey(key) || !jsonDeepEquals(left[key], right[key])) {
        return false;
      }
    }
    return true;
  }
  if (left is List && right is List) {
    if (left.length != right.length) {
      return false;
    }
    for (var index = 0; index < left.length; index++) {
      if (!jsonDeepEquals(left[index], right[index])) {
        return false;
      }
    }
    return true;
  }
  return left == right;
}

/// Multi-character full case folds (CaseFolding.txt, status F) that Dart's
/// `toLowerCase` does not expand. Covers the Latin and ligature cases that
/// operators type; other folds fall back to simple lowercase.
const Map<String, String> _fullCaseFolds = {
  '\u00DF': 'ss', // sharp s
  '\u1E9E': 'ss', // capital sharp s
  '\u0149': '\u02BCn', // n preceded by apostrophe
  '\uFB00': 'ff',
  '\uFB01': 'fi',
  '\uFB02': 'fl',
  '\uFB03': 'ffi',
  '\uFB04': 'ffl',
  '\uFB05': 'st',
  '\uFB06': 'st',
};

/// Unicode case folding used for preset duplicate detection (the Rust
/// `unicode_casefold` full fold). Full folds come from [_fullCaseFolds];
/// everything else uses simple lowercase.
String _presetNameKey(String name) {
  final buffer = StringBuffer();
  for (final rune in name.runes) {
    final character = String.fromCharCode(rune);
    buffer.write(_fullCaseFolds[character] ?? character.toLowerCase());
  }
  return buffer.toString();
}

int _runeCount(String value) => value.runes.length;

// ---- Store ----------------------------------------------------------------

/// Port of `SettingsStore`, implementing the auth settings seam.
final class SettingsStore implements AuthSettings {
  SettingsStore._(this.path, this._data, this._writer);

  /// Opens (and, when needed, scrubs) the settings file at [path]. Throws when
  /// the scrub cannot be persisted, so a secret never stays on disk silently.
  factory SettingsStore(String path, {SettingsWriter? writer}) {
    final effectiveWriter = writer ?? atomicWriteSettings;
    final data = _loadObject(path);
    if (_scrubLegacyOperationSecrets(data)) {
      effectiveWriter(path, data);
    }
    return SettingsStore._(path, data, effectiveWriter);
  }

  factory SettingsStore.atDefaultPath({SettingsWriter? writer}) =>
      SettingsStore(defaultSettingsPath(), writer: writer);

  final String path;
  final SettingsWriter _writer;
  JsonObject _data;

  /// Re-reads the file from disk, replacing the in-memory copy.
  void reload() {
    final data = _loadObject(path);
    if (_scrubLegacyOperationSecrets(data)) {
      _writer(path, data);
    }
    _data = data;
  }

  JsonObject paramsFor(String opId) {
    final params = _data['params'];
    if (params is! Map) {
      return <String, Object?>{};
    }
    final values = params[opId];
    return values is Map
        ? Map<String, Object?>.from(values)
        : <String, Object?>{};
  }

  void saveParams(String opId, JsonObject values) {
    final clean = <String, Object?>{
      for (final entry in values.entries)
        if (_isSafeParamValue(entry.value)) entry.key: entry.value,
    };
    final next = _deepCopy(_data) as JsonObject;
    final existing = next['params'];
    final params = existing is Map
        ? Map<String, Object?>.from(existing)
        : <String, Object?>{};
    params[opId] = clean;
    next['params'] = params;
    _persist(next);
  }

  /// Port of `presets_for`: returns the operation's presets sorted by name, and
  /// rewrites the stored list when sanitizing changed it.
  List<OperationPreset> presetsFor(OperationDef operation) {
    final presets = _sanitizedOperationPresets(_data, operation);
    final encoded = _encodePresets(presets);
    final sections = _data['operationPresets'];
    final existing = sections is Map ? sections[operation.id] : null;
    if (existing != null && !jsonDeepEquals(existing, encoded)) {
      final next = _deepCopy(_data) as JsonObject;
      final nextSections = _mapOrEmpty(next['operationPresets']);
      nextSections[operation.id] = encoded;
      next['operationPresets'] = nextSections;
      _persist(next);
    }
    final sorted = List<OperationPreset>.of(presets)
      ..sort((left, right) {
        final byName = _presetNameKey(
          left.name,
        ).compareTo(_presetNameKey(right.name));
        return byName != 0 ? byName : left.id.compareTo(right.id);
      });
    return sorted;
  }

  OperationPreset savePreset(
    OperationDef operation,
    String name,
    JsonObject values, {
    required bool replace,
  }) {
    final trimmed = name.trim();
    if (trimmed.isEmpty || _runeCount(trimmed) > _maxPresetNameChars) {
      throw const PresetStoreError(PresetStoreErrorKind.invalidName);
    }
    final validated = _validateNewPresetValues(operation, values);
    final next = _deepCopy(_data) as JsonObject;
    final sections = _mapOrEmpty(next['operationPresets']);
    final presets = _sanitizedOperationPresets(next, operation);
    final nameKey = _presetNameKey(trimmed);
    final duplicate = presets.indexWhere(
      (preset) => _presetNameKey(preset.name) == nameKey,
    );
    final OperationPreset preset;
    if (duplicate >= 0) {
      if (!replace) {
        throw const PresetStoreError(PresetStoreErrorKind.duplicateName);
      }
      preset = OperationPreset(
        id: presets[duplicate].id,
        name: trimmed,
        values: validated,
      );
      presets[duplicate] = preset;
    } else {
      if (presets.length >= _maxOperationPresets) {
        throw const PresetStoreError(PresetStoreErrorKind.limitReached);
      }
      preset = OperationPreset(
        id: const Uuid().v4(),
        name: trimmed,
        values: validated,
      );
      presets.add(preset);
    }
    sections[operation.id] = _encodePresets(presets);
    next['operationPresets'] = sections;
    _persist(next);
    return preset;
  }

  void deletePreset(OperationDef operation, String presetId) {
    final next = _deepCopy(_data) as JsonObject;
    final sections = _mapOrEmpty(next['operationPresets']);
    final presets = _sanitizedOperationPresets(next, operation);
    final index = presets.indexWhere((preset) => preset.id == presetId);
    if (index < 0) {
      throw const PresetStoreError(PresetStoreErrorKind.missing);
    }
    presets.removeAt(index);
    sections[operation.id] = _encodePresets(presets);
    next['operationPresets'] = sections;
    _persist(next);
  }

  @override
  JsonObject? section(String name) {
    final value = _data[name];
    return value is Map ? Map<String, Object?>.from(value) : null;
  }

  @override
  void saveSection(String name, JsonObject? value) {
    final next = _deepCopy(_data) as JsonObject;
    if (value != null) {
      next[name] = _deepCopy(value);
    } else {
      next.remove(name);
    }
    _persist(next);
  }

  @override
  void saveSessionMetadata(String userId, String email, int expiresAt) {
    final next = _deepCopy(_data) as JsonObject;
    final session = _mapOrEmpty(next['session']);
    session
      ..remove('accessToken')
      ..remove('refreshToken')
      ..['userId'] = userId
      ..['email'] = email
      ..['expiresAt'] = expiresAt;
    next['session'] = session;
    _persist(next);
  }

  @override
  bool stripLegacySessionTokens(StoredSession expected) {
    final current = _data['session'];
    if (current is! Map) {
      return false;
    }
    final tokensMatch =
        current['accessToken'] == expected.accessToken &&
        current['refreshToken'] == expected.refreshToken;
    if (!tokensMatch) {
      return false;
    }
    final next = _deepCopy(_data) as JsonObject;
    final session = Map<String, Object?>.from(current)
      ..remove('accessToken')
      ..remove('refreshToken');
    next['session'] = session;
    _persist(next);
    return true;
  }

  AppPrefs preferences() {
    final raw = section('prefs') ?? const <String, Object?>{};
    final locale = raw['locale'];
    final fontScale = raw['fontScale'];
    return _normalizePreferences(
      AppPrefs(
        locale: locale is String ? locale : null,
        theme: _stringOr(raw['theme'], 'system'),
        font: _stringOr(raw['font'], 'system'),
        fontScale: fontScale is num ? fontScale.toDouble() : 1.0,
        compact: raw['compact'] is bool && raw['compact'] as bool,
        startWithWindows:
            raw['startWithWindows'] is bool && raw['startWithWindows'] as bool,
        replicaSyncEnabled:
            raw['replicaSyncEnabled'] is bool &&
            raw['replicaSyncEnabled'] as bool,
      ),
    );
  }

  void savePreferences(AppPrefs preferences) {
    final normalized = _normalizePreferences(preferences);
    saveSection('prefs', normalized.toJson());
  }

  // ---- internals ---------------------------------------------------------

  void _persist(JsonObject next) {
    _writer(path, next);
    _data = next;
  }
}

String _stringOr(Object? value, String fallback) =>
    value is String ? value : fallback;

Map<String, Object?> _mapOrEmpty(Object? value) =>
    value is Map ? Map<String, Object?>.from(value) : <String, Object?>{};

AppPrefs _normalizePreferences(AppPrefs preferences) {
  final locale = preferences.locale;
  final theme = _themes.contains(preferences.theme)
      ? preferences.theme
      : 'system';
  final font = _fonts.contains(preferences.font) ? preferences.font : 'system';
  final fontScale = preferences.fontScale.isFinite
      ? preferences.fontScale.clamp(_minFontScale, _maxFontScale).toDouble()
      : 1.0;
  return AppPrefs(
    locale: locale != null && _locales.contains(locale) ? locale : null,
    theme: theme,
    font: font,
    fontScale: fontScale,
    compact: preferences.compact,
    startWithWindows: preferences.startWithWindows,
    replicaSyncEnabled: preferences.replicaSyncEnabled,
  );
}

Map<String, Object?> _loadObject(String path) {
  try {
    final file = File(path);
    if (!file.existsSync()) {
      return <String, Object?>{};
    }
    final decoded = jsonDecode(file.readAsStringSync());
    return decoded is Map ? Map<String, Object?>.from(decoded) : {};
  } on Object {
    return <String, Object?>{};
  }
}

/// Removes `gemini_api_key` from every stored operation section. Returns true
/// when anything was removed.
bool _scrubLegacyOperationSecrets(JsonObject data) {
  final params = data['params'];
  if (params is! Map) {
    return false;
  }
  var changed = false;
  for (final values in params.values) {
    if (values is Map && values.containsKey('gemini_api_key')) {
      values.remove('gemini_api_key');
      changed = true;
    }
  }
  return changed;
}

bool _isSafeParamValue(Object? value) {
  if (value == null || value is bool || value is num) {
    return true;
  }
  if (value is String) {
    return _runeCount(value) <= _maxParamStringChars;
  }
  if (value is List) {
    return value.length <= _maxParamListItems &&
        value.every(
          (item) => item is String && _runeCount(item) <= _maxParamStringChars,
        );
  }
  if (value is Map) {
    return value.length <= _maxParamObjectEntries &&
        value.values.every((item) => item is int && item >= 0);
  }
  return false;
}

bool _isUuid(String value) => _uuidPattern.hasMatch(value);

/// Decodes stored presets, dropping only malformed entries.
List<OperationPreset> _operationPresets(JsonObject data, String opId) {
  final sections = data['operationPresets'];
  final raw = sections is Map ? sections[opId] : null;
  if (raw is! List) {
    return <OperationPreset>[];
  }
  final presets = <OperationPreset>[];
  for (final entry in raw) {
    if (entry is! Map) {
      continue;
    }
    final id = entry['id'];
    final name = entry['name'];
    final values = entry['values'];
    if (id is! String || name is! String || values is! Map) {
      continue;
    }
    final trimmed = name.trim();
    if (name == trimmed &&
        trimmed.isNotEmpty &&
        _runeCount(trimmed) <= _maxPresetNameChars &&
        _isUuid(id)) {
      presets.add(
        OperationPreset(
          id: id,
          name: name,
          values: Map<String, Object?>.from(values),
        ),
      );
    }
  }
  return presets;
}

List<OperationPreset> _sanitizedOperationPresets(
  JsonObject data,
  OperationDef operation,
) => [
  for (final preset in _operationPresets(data, operation.id))
    OperationPreset(
      id: preset.id,
      name: preset.name,
      values: _sanitizeStoredPresetValues(operation, preset.values),
    ),
];

JsonObject _sanitizeStoredPresetValues(
  OperationDef operation,
  JsonObject values,
) {
  final result = <String, Object?>{};
  for (final entry in values.entries) {
    final parameter = _parameterNamed(operation, entry.key);
    if (parameter != null &&
        _storedPresetValueMatches(operation, parameter, entry.value)) {
      result[entry.key] = entry.value;
    }
  }
  return result;
}

ParamDef? _parameterNamed(OperationDef operation, String name) {
  for (final parameter in operation.schema.params) {
    if (parameter.name == name) {
      return parameter;
    }
  }
  return null;
}

bool _storedPresetValueMatches(
  OperationDef operation,
  ParamDef parameter,
  Object? value,
) {
  if (operation.id == 'files.batch_print' && parameter.name == 'printer') {
    if (value is! String) {
      return false;
    }
    return value.trim().isNotEmpty &&
        _runeCount(value) <= _maxPrintPrinterChars &&
        !value.runes.any(_isControl);
  }
  return _presetValueMatches(parameter, value, validateChoices: true);
}

bool _isControl(int rune) => rune < 0x20 || (rune >= 0x7f && rune <= 0x9f);

JsonObject _validateNewPresetValues(
  OperationDef operation,
  JsonObject values,
) {
  final filtered = <String, Object?>{};
  for (final entry in values.entries) {
    final parameter = _parameterNamed(operation, entry.key);
    if (parameter == null) {
      continue;
    }
    if (!(parameter.kind == ParamKind.bool ||
        parameter.kind == ParamKind.choice ||
        parameter.kind == ParamKind.int)) {
      continue;
    }
    if (!_presetValueMatches(parameter, entry.value, validateChoices: true)) {
      throw const PresetStoreError(PresetStoreErrorKind.invalidValue);
    }
    filtered[entry.key] = entry.value;
  }
  return filtered;
}

bool _presetValueMatches(
  ParamDef parameter,
  Object? value, {
  required bool validateChoices,
}) => switch (parameter.kind) {
  ParamKind.bool => value is bool,
  ParamKind.choice =>
    value is String &&
        (!validateChoices || parameter.choices.contains(value)),
  ParamKind.int => value is int,
  _ => false,
};

List<JsonObject> _encodePresets(List<OperationPreset> presets) => [
  for (final preset in presets) preset.toJson(),
];

/// Writes [data] atomically: a temporary file in the same directory is synced,
/// renamed over [path], and the directory is then synced.
void atomicWriteSettings(String path, JsonObject data) =>
    atomicWriteWithParentSync(path, data, _syncDirectory);

/// Testable core of [atomicWriteSettings]. If the parent sync fails after the
/// rename, the write still succeeds when the file reads back exactly as written.
void atomicWriteWithParentSync(
  String path,
  JsonObject data,
  void Function(String parent) syncParent,
) {
  final file = File(path);
  final parent = file.parent;
  try {
    parent.createSync(recursive: true);
  } on FileSystemException catch (error) {
    throw SettingsIoException('settings directory is unavailable', error);
  }
  final serialized = utf8.encode(canonicalJson(data));
  final temporary = File(
    p.join(parent.path, '.${p.basename(path)}.${const Uuid().v4()}.tmp'),
  );
  try {
    final handle = temporary.openSync(mode: FileMode.writeOnly);
    try {
      handle.writeFromSync(serialized);
      handle.flushSync();
    } finally {
      handle.closeSync();
    }
    temporary.renameSync(path);
  } on Object catch (error) {
    _deleteIfExists(temporary);
    throw SettingsIoException('settings could not be written', error);
  }

  try {
    syncParent(parent.path);
  } on Object {
    final persisted = _readBytesOrNull(path);
    if (persisted == null || !_bytesEqual(persisted, serialized)) {
      throw const SettingsIoException('settings directory sync failed');
    }
  }
}

void _syncDirectory(String parent) {
  if (Platform.isWindows) {
    // Directory handles cannot be fsynced through dart:io on Windows; the
    // rename itself is the durable step there. UNVERIFIED on Windows hosts.
    return;
  }
  final handle = File(parent).openSync();
  try {
    handle.flushSync();
  } finally {
    handle.closeSync();
  }
}

void _deleteIfExists(File file) {
  try {
    if (file.existsSync()) {
      file.deleteSync();
    }
  } on Object {
    // Cleanup failure leaves only the temporary name behind.
  }
}

List<int>? _readBytesOrNull(String path) {
  try {
    return File(path).readAsBytesSync();
  } on Object {
    return null;
  }
}

bool _bytesEqual(List<int> left, List<int> right) {
  if (left.length != right.length) {
    return false;
  }
  for (var index = 0; index < left.length; index++) {
    if (left[index] != right[index]) {
      return false;
    }
  }
  return true;
}
