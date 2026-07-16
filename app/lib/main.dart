import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';

import 'core/auth/auth_service.dart';
import 'core/cloud/cloud_history.dart' show supabaseKey, supabaseUrl;
import 'core/ipc/sidecar_client.dart';
import 'core/l10n/strings.dart';
import 'core/settings/app_prefs.dart';
import 'core/settings/settings_store.dart';
import 'ui/shell/auth_gate.dart';
import 'ui/theme/app_theme.dart';

// Dev fallback yollari (bu makine). Paketlenmis derlemede kullanilmaz —
// orada sidecar app exe'sinin yaninda bundle edilir. --dart-define ile ezilebilir.
const _devPython = String.fromEnvironment(
  'DOCFLOW_PY',
  defaultValue: '/Users/rohtolos/Desktop/mono_azfen/sidecar/.venv/bin/python',
);
const _devSidecarDir = String.fromEnvironment(
  'DOCFLOW_SIDECAR_DIR',
  defaultValue: '/Users/rohtolos/Desktop/mono_azfen/sidecar',
);

class _Launch {
  _Launch(this.executable, this.args, this.workingDir, [this.environment]);
  final String executable;
  final List<String> args;
  final String workingDir;
  final Map<String, String>? environment;
}

/// Sidecar nasil baslatilacak? Uc kademe: bundle -> env override -> dev fallback.
_Launch _resolveSidecar() {
  final env = Platform.environment;
  final sep = Platform.pathSeparator;
  final exeDir = File(Platform.resolvedExecutable).parent.path;

  // 1) Paketlenmis: app exe yaninda  sidecar/<exe>  ve  tesseract/<exe>
  final sideName = Platform.isWindows ? 'docflow_sidecar.exe' : 'docflow_sidecar';
  final bundled = '$exeDir${sep}sidecar$sep$sideName';
  if (File(bundled).existsSync()) {
    final tessName = Platform.isWindows ? 'tesseract.exe' : 'tesseract';
    final tess = '$exeDir${sep}tesseract$sep$tessName';
    final childEnv = <String, String>{};
    if (File(tess).existsSync()) childEnv['DOCFLOW_TESSERACT'] = tess;
    return _Launch(bundled, const [], '$exeDir${sep}sidecar', childEnv);
  }

  // 2) Env override (dev/CI)
  final py = env['DOCFLOW_PY'];
  final dir = env['DOCFLOW_SIDECAR_DIR'];
  if (py != null && dir != null) {
    return _Launch(py, const ['-m', 'docflow_sidecar'], dir);
  }

  // 3) Dev fallback (compile-time defaults)
  return _Launch(_devPython, const ['-m', 'docflow_sidecar'], _devSidecarDir);
}

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  final settings = await SettingsStore.load();
  final prefs = AppPrefs(settings);
  final auth = AuthService(url: supabaseUrl, key: supabaseKey, settings: settings);
  await auth.restore(); // diskteki oturumu geri yukle (varsa)
  final l = _resolveSidecar();
  final client = SidecarClient(
    executable: l.executable,
    args: l.args,
    workingDir: l.workingDir,
    environment: l.environment,
  );
  runApp(DocFlowApp(client: client, settings: settings, auth: auth, prefs: prefs));
}

class DocFlowApp extends StatelessWidget {
  const DocFlowApp({
    super.key,
    required this.client,
    required this.settings,
    required this.auth,
    required this.prefs,
  });

  final SidecarClient client;
  final SettingsStore settings;
  final AuthService auth;
  final AppPrefs prefs;

  @override
  Widget build(BuildContext context) {
    // prefs bir ChangeNotifier: dil/tema/font degisince tum agac yeniden kurulur.
    return ListenableBuilder(
      listenable: prefs,
      builder: (context, _) => MaterialApp(
        onGenerateTitle: (ctx) => Strings.of(ctx).t('app.title'),
        debugShowCheckedModeBanner: false,
        themeMode: prefs.themeMode,
        theme: AppTheme.build(
          brightness: Brightness.light,
          font: prefs.font,
          fontScale: prefs.fontScale,
          compact: prefs.compact,
        ),
        darkTheme: AppTheme.build(
          brightness: Brightness.dark,
          font: prefs.font,
          fontScale: prefs.fontScale,
          compact: prefs.compact,
        ),
        localizationsDelegates: const [
          StringsDelegate(),
          GlobalMaterialLocalizations.delegate,
          GlobalWidgetsLocalizations.delegate,
          GlobalCupertinoLocalizations.delegate,
        ],
        supportedLocales: Strings.supported,
        // Ayarlardan secilen dil sistem dilini EZER; null ise sistem dili.
        locale: prefs.locale,
        localeResolutionCallback: (device, supported) {
          if (device != null) {
            for (final l in supported) {
              if (l.languageCode == device.languageCode) return l;
            }
          }
          return const Locale('en');
        },
        home: AuthGate(
          auth: auth,
          client: client,
          settings: settings,
          prefs: prefs,
        ),
      ),
    );
  }
}
