import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';

import 'core/auth/auth_service.dart';
import 'core/cloud/cloud_history.dart' show supabaseKey, supabaseUrl;
import 'core/ipc/sidecar_client.dart';
import 'core/ipc/sidecar_launch.dart';
import 'core/l10n/strings.dart';
import 'core/settings/app_prefs.dart';
import 'core/settings/settings_store.dart';
import 'ui/shell/auth_gate.dart';
import 'ui/theme/app_theme.dart';
import 'ui/version_badge.dart';

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

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  final settings = await SettingsStore.load();
  final prefs = AppPrefs(settings);
  final auth = AuthService(
    url: supabaseUrl,
    key: supabaseKey,
    settings: settings,
  );
  await auth.restore(); // diskteki oturumu geri yukle (varsa)
  final l = resolveSidecarLaunch(
    devPython: _devPython,
    devSidecarDir: _devSidecarDir,
  );
  final client = SidecarClient(
    executable: l.executable,
    args: l.args,
    workingDir: l.workingDir,
    environment: l.environment,
  );
  runApp(
    DocFlowApp(client: client, settings: settings, auth: auth, prefs: prefs),
  );
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
          compact: prefs.compact,
        ),
        darkTheme: AppTheme.build(
          brightness: Brightness.dark,
          font: prefs.font,
          compact: prefs.compact,
        ),
        // Yazi boyutu olcegi tema degil, MediaQuery isi (bkz. applyTextScale).
        builder: (context, child) {
          final content = AppTheme.applyTextScale(
            context,
            child,
            prefs.fontScale,
          );
          return Stack(
            fit: StackFit.expand,
            children: [
              content,
              const Positioned(left: 0, bottom: 0, child: AppVersionBadge()),
            ],
          );
        },
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
