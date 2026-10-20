// Operation catalog helpers from frontend/src/App.tsx: the Home category of an
// operation, its icon, the preset value filter, and the copy sheets a plan
// offers. HomeScreen and OperationScreen share them.

import 'package:flutter/widgets.dart';

import '../../models/models.dart';
import '../../theme/app_icons.dart';

/// Home filter categories (`OperationCategory`). The names are the suffixes of
/// the `ops.category.*` messages.
enum OperationCategory { all, scans, excel, mail, system }

/// `getOperationCategory` in App.tsx.
OperationCategory operationCategoryOf(String id) => switch (id) {
  'rfi.process_scans_fast_covers' ||
  'rfi.normalize_pdf_orientation' ||
  'rfi.stage_signed_first_pages' => OperationCategory.scans,
  'rfi.log_add' ||
  'rfi.follow_up_sync' ||
  'rfi.complete_status_sync' ||
  'rfi.system_sync' => OperationCategory.excel,
  'rfi.group_files' => OperationCategory.mail,
  _ => OperationCategory.system,
};

/// `getOperationIcon` in App.tsx.
IconData operationIconOf(String id) => switch (id) {
  'files.batch_print' => AppIcons.printer,
  'rfi.group_files' => AppIcons.folderOpen,
  'rfi.log_add' => AppIcons.tableProperties,
  'rfi.follow_up_sync' => AppIcons.refreshCw,
  'rfi.complete_status_sync' => AppIcons.checkCircle,
  'rfi.stage_signed_first_pages' => AppIcons.badgeCheck,
  'rfi.system_sync' => AppIcons.shield,
  'rfi.normalize_pdf_orientation' => AppIcons.rotateCcw,
  'rfi.process_scans_fast_covers' => AppIcons.fileCheck,
  _ => AppIcons.fileSearch,
};

/// The operation the Home screen promotes above the list.
const String primaryScanOperationId = 'rfi.process_scans_fast_covers';

/// The parameters a preset may hold: safe bool, choice and int settings that
/// are not marked `remember: false` (`presetCapableValues` in App.tsx).
Map<String, Object?> presetCapableValues(
  OperationDef operation,
  Map<String, Object?> values,
) {
  final allowed = {
    for (final param in operation.schema)
      if (param.remember != false &&
          (param.kind == ParamKind.bool ||
              param.kind == ParamKind.choice ||
              param.kind == ParamKind.int))
        param.name,
  };
  return {
    for (final entry in values.entries)
      if (allowed.contains(entry.key)) entry.key: entry.value,
  };
}

/// The sheet names a plan can copy as tables (`copySheetsFromPlan`).
List<String> copySheetsFromPlan(Plan? plan) {
  final editor = plan?.meta['editor'];
  if (editor is! Map) return const [];
  final sources = editor['copySources'];
  if (sources is! Map) return const [];
  return [for (final key in sources.keys) key.toString()];
}

/// True when the plan editor allows copying tables (`copyEnabled`).
bool planCopyEnabled(Plan? plan) {
  final editor = plan?.meta['editor'];
  return editor is Map && editor['copyEnabled'] == true;
}
