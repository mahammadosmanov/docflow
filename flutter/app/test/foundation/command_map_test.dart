import 'package:docflow/bridge/bridge.dart';
import 'package:flutter_test/flutter_test.dart';

/// `FRONTEND_METHODS` from frontend/src/lib/commands.ts, in the same order.
const List<String> _tsFrontendMethods = [
  'app/bootstrap',
  'auth/usernameAvailable',
  'auth/signIn',
  'auth/signUp',
  'auth/signOut',
  'auth/loadMe',
  'auth/setUsername',
  'auth/listUsers',
  'auth/setFlags',
  'settings/update',
  'settings/reset',
  'settings/replicaStatus',
  'settings/params',
  'settings/saveParams',
  'settings/presets',
  'settings/savePreset',
  'settings/deletePreset',
  'support/diagnostics',
  'support/copyDiagnostics',
  'engine/preview',
  'engine/apply',
  'engine/cancel',
  'engine/printQueueLatest',
  'engine/printQueueDiscard',
  'engine/printQueueResume',
  'engine/revise',
  'engine/copyTable',
  'engine/rfiReplyPreview',
  'engine/reviewScan',
  'engine/reviewPreview',
  'engine/undo',
  'history/list',
  'paths/select',
  'paths/reveal',
  'rfi/printFind',
  'recovery/list',
];

void main() {
  test('command names match FRONTEND_METHODS in commands.ts', () {
    expect([
      for (final command in FrontendCommands.all) command.name,
    ], _tsFrontendMethods);
  });

  test('command names are unique', () {
    final names = [for (final command in FrontendCommands.all) command.name];
    expect(names.toSet().length, names.length);
  });
}
