// Chooses what the window shows (the body of `App` in App.tsx): the splash, the
// bootstrap failure, the auth screens, or the workspace routes inside AppFrame.

import 'package:flutter/widgets.dart';

import 'app_controller.dart';
import 'app_frame.dart';
import 'routes.dart';
import 'screen_registry.dart';
import 'splash.dart';

final class AppShell extends StatelessWidget {
  const AppShell({super.key, required this.controller, required this.screens});

  final AppController controller;
  final ScreenRegistry screens;

  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: controller,
    builder: (context, _) => _resolve(),
  );

  Widget _resolve() {
    final bootstrap = controller.bootstrap;
    if (bootstrap == null) {
      if (controller.fatal.isEmpty) return const Splash();
      return BootstrapFailure(
        strings: controller.strings,
        message: controller.fatal,
        onRetry: () => controller.load(),
      );
    }

    final strings = controller.strings;
    final auth = bootstrap.auth;
    final route = controller.route;

    ScreenContext contextFor(AppRoute current) => ScreenContext(
      bridge: controller.bridge,
      strings: strings,
      bootstrap: bootstrap,
      route: current,
      navigate: controller.navigate,
      updateAuth: controller.updateAuth,
      updatePrefs: controller.updatePrefs,
    );

    AppFrame frame(ScreenId screen, Widget child) => AppFrame(
      bridge: controller.bridge,
      routeName: screen,
      isAdmin: auth.isAdmin,
      user: auth.user,
      strings: strings,
      onNavigate: controller.navigate,
      child: child,
    );

    if (route is SettingsRoute) {
      final settings = screens.build(ScreenId.settings, contextFor(route));
      return auth.signedIn && auth.allowed
          ? frame(ScreenId.settings, settings)
          : settings;
    }
    if (!auth.signedIn) {
      return screens.build(ScreenId.login, contextFor(route));
    }
    if (!auth.allowed) {
      return screens.build(ScreenId.pending, contextFor(route));
    }

    final page = screens.build(route.screen, contextFor(route));
    final keyed = switch (route) {
      OperationRoute(:final operation) => KeyedSubtree(
        key: ValueKey('operation:${operation.id}'),
        child: page,
      ),
      _ => page,
    };
    return frame(route.screen, keyed);
  }
}
