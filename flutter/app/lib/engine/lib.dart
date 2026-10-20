// Composition root for the engine (lib.rs bootstrapping). Builds a DocFlowEngine
// from the platform services and stores. Modules that are not wired yet use the
// unwired defaults in backend.dart; see the report for the list.

import 'dart:io';

import 'auth.dart' as auth;
import 'backend.dart';
import 'credentials.dart';
import 'doc_flow_engine.dart';
import 'http.dart';
import 'native.dart';
import 'path_selection.dart';
import 'settings.dart';
import 'smoke.dart' show runPackageSmoke;

export 'backend.dart' show docflowVersion;
export 'doc_flow_engine.dart' show DocFlowEngine;

/// Supabase project endpoint and publishable key (native.rs). The publishable
/// key is public by design; the service-role key never ships in a client.
const String supabaseUrl = 'https://ndpkgbfmshceffwhpqoz.supabase.co';
const String supabaseKey = 'sb_publishable_sK0uEH4t-Hw7LVBpnEaMog_z_2MQwxq';

/// Path selections expire after 30 minutes (`PathSelectionStore::new`).
const Duration _selectionTtl = Duration(minutes: 30);

/// Builds the engine. [settingsPath] defaults to `~/.docflow/settings.json`.
/// [native] defaults to the production [NativeBackend] (sharing one
/// [PathSelectionStore] and the app data directory); its cloud history is
/// wired to the router when no [native] is passed. [platform] defaults to the
/// unwired seam.
DocFlowEngine buildDocFlowEngine({
  String? settingsPath,
  NativeOperations? native,
  PlatformServices platform = const UnwiredPlatformServices(),
}) {
  final resolvedSettingsPath = settingsPath ?? defaultSettingsPath();
  final settings = SettingsStore(resolvedSettingsPath);
  final credentials = SessionCredentialStore(platformSecureStore());
  final http = HttpClient(const HttpConfig());
  final config = auth.AuthConfig(supabaseUrl, supabaseKey);
  final authService = auth.AuthService(settings, credentials, http, config);
  final selections = PathSelectionStore(ttl: _selectionTtl);
  final backend = native == null
      ? NativeBackend.production(
          selections: selections,
          settings: settings,
          authService: authService,
          authConfig: config,
          appDataDir: File(resolvedSettingsPath).parent.path,
        )
      : null;
  return DocFlowEngine(
    router: BackendRouter(
      settings: settings,
      authService: authService,
      selections: selections,
      native: native ?? backend!,
      platform: platform,
      history: backend?.cloud,
      now: () => DateTime.now().millisecondsSinceEpoch ~/ 1000,
    ),
  );
}

/// Runs `--package-smoke <output|-> [--skip-ocr]` when present. Returns the exit
/// status, or null when the arguments are not a smoke request.
Future<int?> maybeRunSmoke(List<String> arguments) async {
  if (arguments.isEmpty || arguments.first != '--package-smoke') {
    return null;
  }
  return runPackageSmoke(arguments);
}

/// Directory used for the app data files (settings, journal, trash).
Directory appDataDirectory() => File(defaultSettingsPath()).parent;
