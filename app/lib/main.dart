import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';

import 'core/ipc/sidecar_client.dart';
import 'core/l10n/strings.dart';
import 'core/settings/settings_store.dart';
import 'ui/shell/operation_list_screen.dart';

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
  final l = _resolveSidecar();
  final client = SidecarClient(
    executable: l.executable,
    args: l.args,
    workingDir: l.workingDir,
    environment: l.environment,
  );
  runApp(DocFlowApp(client: client, settings: settings));
}

class DocFlowApp extends StatelessWidget {
  const DocFlowApp({super.key, required this.client, required this.settings});

  final SidecarClient client;
  final SettingsStore settings;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      onGenerateTitle: (ctx) => Strings.of(ctx).t('app.title'),
      debugShowCheckedModeBanner: false,
      theme: ThemeData(
        useMaterial3: true,
        colorSchemeSeed: const Color(0xFF3B5BDB),
        visualDensity: VisualDensity.comfortable,
      ),
      localizationsDelegates: const [
        StringsDelegate(),
        GlobalMaterialLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
      ],
      supportedLocales: Strings.supported,
      localeResolutionCallback: (device, supported) {
        if (device != null) {
          for (final l in supported) {
            if (l.languageCode == device.languageCode) return l;
          }
        }
        return const Locale('en');
      },
      home: OperationListScreen(client: client, settings: settings),
    );
  }
}
