import 'json.dart';

/// A path the user picked or dropped (`PathSelection`). The engine keeps the
/// real path behind `selectionId`; the UI only sees `displayPath`.
final class PathSelection {
  const PathSelection({
    required this.selectionId,
    required this.displayPath,
    required this.name,
  });

  factory PathSelection.fromJson(JsonMap json) => PathSelection(
    selectionId: jsonString(json, 'selectionId'),
    displayPath: jsonString(json, 'displayPath'),
    name: jsonString(json, 'name'),
  );

  final String selectionId;
  final String displayPath;
  final String name;

  JsonMap toJson() => {
    'selectionId': selectionId,
    'displayPath': displayPath,
    'name': name,
  };
}
