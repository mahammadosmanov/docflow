/// JSON access helpers shared by the models. The wire format is the camelCase
/// JSON produced by the engine (see frontend/src/types.ts).
library;

typedef JsonMap = Map<String, Object?>;

JsonMap jsonMap(Object? value, [String where = 'value']) {
  if (value is Map<String, Object?>) return value;
  if (value is Map) {
    return value.map((key, entry) => MapEntry(key.toString(), entry));
  }
  throw FormatException('Expected object for $where, got ${_describe(value)}');
}

List<Object?> jsonList(Object? value, [String where = 'value']) {
  if (value is List) return value;
  throw FormatException('Expected array for $where, got ${_describe(value)}');
}

String jsonString(JsonMap map, String key) {
  final value = map[key];
  if (value is String) return value;
  throw FormatException('Expected string at "$key", got ${_describe(value)}');
}

String? jsonStringOrNull(JsonMap map, String key) {
  final value = map[key];
  if (value == null) return null;
  if (value is String) return value;
  throw FormatException('Expected string at "$key", got ${_describe(value)}');
}

bool jsonBool(JsonMap map, String key) {
  final value = map[key];
  if (value is bool) return value;
  throw FormatException('Expected bool at "$key", got ${_describe(value)}');
}

bool? jsonBoolOrNull(JsonMap map, String key) {
  final value = map[key];
  if (value == null) return null;
  if (value is bool) return value;
  throw FormatException('Expected bool at "$key", got ${_describe(value)}');
}

int jsonInt(JsonMap map, String key) {
  final value = map[key];
  if (value is num && value == value.truncate()) return value.toInt();
  throw FormatException('Expected integer at "$key", got ${_describe(value)}');
}

int? jsonIntOrNull(JsonMap map, String key) {
  if (map[key] == null) return null;
  return jsonInt(map, key);
}

double jsonDouble(JsonMap map, String key) {
  final value = map[key];
  if (value is num) return value.toDouble();
  throw FormatException('Expected number at "$key", got ${_describe(value)}');
}

double? jsonDoubleOrNull(JsonMap map, String key) {
  if (map[key] == null) return null;
  return jsonDouble(map, key);
}

JsonMap jsonObject(JsonMap map, String key) => jsonMap(map[key], key);

JsonMap? jsonObjectOrNull(JsonMap map, String key) {
  if (map[key] == null) return null;
  return jsonMap(map[key], key);
}

List<T> jsonObjects<T>(
  JsonMap map,
  String key,
  T Function(JsonMap json) fromJson,
) => [for (final item in jsonList(map[key], key)) fromJson(jsonMap(item, key))];

List<String> jsonStrings(JsonMap map, String key) => [
  for (final item in jsonList(map[key], key))
    if (item is String)
      item
    else
      throw FormatException('Expected string item in "$key"'),
];

Map<String, int> jsonIntMap(JsonMap map, String key) {
  final source = jsonMap(map[key], key);
  return {
    for (final entry in source.entries)
      entry.key: _asInt(entry.value, '$key.${entry.key}'),
  };
}

int _asInt(Object? value, String where) {
  if (value is num && value == value.truncate()) return value.toInt();
  throw FormatException(
    'Expected integer at "$where", got ${_describe(value)}',
  );
}

/// Reads a wire-coded enum value by its `wire` string.
T jsonEnum<T extends Enum>(
  JsonMap map,
  String key,
  List<T> values,
  String Function(T value) wireOf,
) {
  final raw = jsonString(map, key);
  for (final value in values) {
    if (wireOf(value) == raw) return value;
  }
  throw FormatException('Unknown value "$raw" at "$key"');
}

T? jsonEnumOrNull<T extends Enum>(
  JsonMap map,
  String key,
  List<T> values,
  String Function(T value) wireOf,
) {
  if (map[key] == null) return null;
  return jsonEnum(map, key, values, wireOf);
}

/// Drops entries whose value is null, so optional fields are omitted.
JsonMap withoutNulls(JsonMap map) => {
  for (final entry in map.entries)
    if (entry.value != null) entry.key: entry.value,
};

String _describe(Object? value) =>
    value == null ? 'null' : value.runtimeType.toString();
