// Root widget (`App` in main.tsx). It owns the bridge and the controller, and
// applies the appearance preferences to the Material theme.

import 'package:flutter/material.dart';

import '../bridge/bridge.dart';
import '../engine/engine.dart';
import '../l10n/app_localizations.dart';
import '../models/models.dart';
import '../theme/app_theme.dart';
import 'app_controller.dart';
import 'app_shell.dart';
import 'screen_registry.dart';

final class DocFlowApp extends StatefulWidget {
  const DocFlowApp({super.key, required this.screens, this.engine});

  /// The engine the bridge talks to. The composition root supplies it. When it
  /// is null, bootstrap fails with `engine_not_configured`.
  final Engine? engine;

  /// Screen builders, registered by the screen owners before this widget is
  /// built.
  final ScreenRegistry screens;

  @override
  State<DocFlowApp> createState() => _DocFlowAppState();
}

final class _DocFlowAppState extends State<DocFlowApp> {
  late final AppController _controller;

  @override
  void initState() {
    super.initState();
    _controller = AppController(
      bridge: Bridge(widget.engine ?? const UnconfiguredEngine()),
    );
    _controller.load();
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: _controller,
    builder: (context, _) {
      final prefs = _controller.bootstrap?.prefs;
      final font = prefs?.font ?? FontPreference.system;
      final fontScale = prefs?.fontScale ?? 1;
      final compact = prefs?.compact ?? false;
      final strings = _controller.strings;
      return MaterialApp(
        title: 'DocFlow',
        debugShowCheckedModeBanner: false,
        themeMode: themeModeFor(prefs?.theme ?? ThemePreference.system),
        theme: buildAppTheme(
          brightness: Brightness.light,
          font: font,
          fontScale: fontScale,
          compact: compact,
        ),
        darkTheme: buildAppTheme(
          brightness: Brightness.dark,
          font: font,
          fontScale: fontScale,
          compact: compact,
        ),
        locale: Locale(strings.locale.name),
        supportedLocales: AppLocalizations.supportedLocales,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        home: AppShell(controller: _controller, screens: widget.screens),
      );
    },
  );
}
