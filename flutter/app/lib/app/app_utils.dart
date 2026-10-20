// Helpers from frontend/src/App.tsx shared by the screens.

import '../models/models.dart';

/// Usernames: 3 to 24 characters of letters, digits, dot, dash or underscore.
final RegExp usernamePattern = RegExp(r'^[a-zA-Z0-9._-]{3,24}$');

/// Reads a [PathSelection] from an untyped value (`isPathSelection` in App.tsx).
/// Returns null unless the value has string `selectionId` and `displayPath`.
PathSelection? asPathSelection(Object? value) {
  if (value is! Map) return null;
  final selectionId = value['selectionId'];
  final displayPath = value['displayPath'];
  if (selectionId is! String || displayPath is! String) return null;
  final name = value['name'];
  return PathSelection(
    selectionId: selectionId,
    displayPath: displayPath,
    name: name is String ? name : '',
  );
}

final RegExp _rfiSeparators = RegExp(r'[\s,;]+');

/// Splits a free-text list of RFI numbers on whitespace, commas and semicolons.
/// Empty entries are dropped and duplicates keep their first position.
List<String> parseRfiInputs(Object? value) {
  if (value is! String) return const [];
  return {
    for (final item in value.split(_rfiSeparators))
      if (item.trim().isNotEmpty) item.trim(),
  }.toList();
}
