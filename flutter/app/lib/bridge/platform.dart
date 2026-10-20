// Native-platform helpers (frontend/src/lib/platform.ts). The engine owns every
// file dialog and reveal action; these functions only ask it through the bridge.

import '../models/models.dart';
import 'bridge.dart';

Future<List<PathSelection>> _selectPaths(
  Bridge bridge,
  String opId,
  String paramName, {
  List<String>? droppedPaths,
}) => bridge.call(
  FrontendCommands.pathsSelect,
  params: (opId: opId, paramName: paramName, droppedPaths: droppedPaths),
);

/// Opens the engine folder picker for [paramName]; `null` when cancelled.
Future<PathSelection?> chooseFolder(
  Bridge bridge,
  String opId,
  String paramName,
) async {
  final selections = await _selectPaths(bridge, opId, paramName);
  return selections.isEmpty ? null : selections.first;
}

/// Opens the engine file picker for [paramName]; `null` when cancelled.
Future<PathSelection?> chooseFile(
  Bridge bridge,
  String opId,
  String paramName,
) async {
  final selections = await _selectPaths(bridge, opId, paramName);
  return selections.isEmpty ? null : selections.first;
}

/// Opens the engine multi-file picker for [paramName].
Future<List<PathSelection>> chooseFiles(
  Bridge bridge,
  String opId,
  String paramName,
) => _selectPaths(bridge, opId, paramName);

/// Resolves paths dropped onto the window for [paramName].
Future<List<PathSelection>> dropPaths(
  Bridge bridge,
  String opId,
  String paramName,
  List<String> paths,
) => _selectPaths(bridge, opId, paramName, droppedPaths: paths);

/// Reveals a selection in the platform file manager.
Future<void> revealPath(Bridge bridge, String selectionId) async {
  await bridge.call(
    FrontendCommands.pathsReveal,
    params: (selectionId: selectionId),
  );
}

Future<List<OperationPreset>> listOperationPresets(
  Bridge bridge,
  String opId,
) => bridge.call(FrontendCommands.settingsPresets, params: (opId: opId));

Future<OperationPreset> saveOperationPreset(
  Bridge bridge, {
  required String opId,
  required String name,
  required Map<String, Object?> values,
  bool? replace,
}) => bridge.call(
  FrontendCommands.settingsSavePreset,
  params: (opId: opId, name: name, values: values, replace: replace),
);

Future<void> deleteOperationPreset(
  Bridge bridge,
  String opId,
  String presetId,
) async {
  await bridge.call(
    FrontendCommands.settingsDeletePreset,
    params: (opId: opId, presetId: presetId),
  );
}
