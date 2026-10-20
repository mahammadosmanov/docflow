import 'json.dart';

/// Parameter widget kinds (`ParamKind` in types.ts). Wire names equal the Dart
/// names.
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
  sheetCounts;

  static ParamKind fromWire(String value) => ParamKind.values.firstWhere(
    (kind) => kind.name == value,
    orElse: () => throw FormatException('Unknown ParamKind "$value"'),
  );
}

/// One parameter of an operation form, generated from the engine's
/// `ParamSchema`. Forms are generic: never special-case an operation.
final class ParamDef {
  const ParamDef({
    required this.name,
    required this.kind,
    required this.labelKey,
    required this.defaultValue,
    required this.required,
    required this.choices,
    required this.choiceLabels,
    required this.visibleWhen,
    required this.helpKey,
    this.remember,
  });

  factory ParamDef.fromJson(JsonMap json) => ParamDef(
    name: jsonString(json, 'name'),
    kind: ParamKind.fromWire(jsonString(json, 'kind')),
    labelKey: jsonString(json, 'labelKey'),
    defaultValue: json['default'],
    required: jsonBool(json, 'required'),
    choices: jsonStrings(json, 'choices'),
    choiceLabels: {
      for (final entry in jsonMap(json['choiceLabels']).entries)
        entry.key: entry.value as String,
    },
    visibleWhen: jsonMap(json['visibleWhen']),
    helpKey: jsonStringOrNull(json, 'helpKey'),
    remember: jsonBoolOrNull(json, 'remember'),
  );

  final String name;
  final ParamKind kind;
  final String labelKey;

  /// JSON key `default`. Any JSON value, so kept as `Object?`.
  final Object? defaultValue;
  final bool required;
  final List<String> choices;
  final Map<String, String> choiceLabels;
  final JsonMap visibleWhen;
  final String? helpKey;
  final bool? remember;

  JsonMap toJson() => withoutNulls({
    'name': name,
    'kind': kind.name,
    'labelKey': labelKey,
    'required': required,
    'choices': choices,
    'choiceLabels': choiceLabels,
    'visibleWhen': visibleWhen,
    'helpKey': helpKey,
    'remember': remember,
  })..['default'] = defaultValue;
}

/// An operation the engine offers (`OperationDef`).
final class OperationDef {
  const OperationDef({
    required this.id,
    required this.titleKey,
    required this.descriptionKey,
    required this.schema,
    required this.supportsPresets,
  });

  factory OperationDef.fromJson(JsonMap json) => OperationDef(
    id: jsonString(json, 'id'),
    titleKey: jsonString(json, 'titleKey'),
    descriptionKey: jsonString(json, 'descriptionKey'),
    schema: jsonObjects(json, 'schema', ParamDef.fromJson),
    supportsPresets: jsonBool(json, 'supportsPresets'),
  );

  final String id;
  final String titleKey;
  final String descriptionKey;
  final List<ParamDef> schema;
  final bool supportsPresets;

  JsonMap toJson() => {
    'id': id,
    'titleKey': titleKey,
    'descriptionKey': descriptionKey,
    'schema': [for (final param in schema) param.toJson()],
    'supportsPresets': supportsPresets,
  };
}

/// A saved parameter set for an operation (`OperationPreset`).
final class OperationPreset {
  const OperationPreset({
    required this.id,
    required this.name,
    required this.values,
  });

  factory OperationPreset.fromJson(JsonMap json) => OperationPreset(
    id: jsonString(json, 'id'),
    name: jsonString(json, 'name'),
    values: jsonMap(json['values'], 'values'),
  );

  final String id;
  final String name;
  final JsonMap values;

  JsonMap toJson() => {'id': id, 'name': name, 'values': values};
}

/// Editor metadata (`EditorMetadata`): a free-form record with typed access to
/// the keys the editor understands.
final class EditorMetadata {
  const EditorMetadata(this.values);

  factory EditorMetadata.fromJson(JsonMap json) => EditorMetadata(json);

  final JsonMap values;

  bool? get reorderable => values['reorderable'] as bool?;

  JsonMap toJson() => values;
}
