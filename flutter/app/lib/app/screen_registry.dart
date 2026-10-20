// The screen registration points. The shell never builds a screen itself: it
// asks this registry for the builder registered under a ScreenId.
//
// Screen owners add a function such as `registerHomeScreen(ScreenRegistry)` in
// their own files, and the composition root calls each one before runApp. A
// screen that is not registered fails loudly, so a missing screen is never
// shown as a blank or placeholder page.

import 'package:flutter/widgets.dart';

import '../bridge/bridge.dart';
import '../l10n/strings.dart';
import '../models/models.dart';
import 'routes.dart';

/// Everything a screen receives from the shell. Callbacks are the navigation
/// and state-update operations that App.tsx passed as props.
final class ScreenContext {
  const ScreenContext({
    required this.bridge,
    required this.strings,
    required this.bootstrap,
    required this.route,
    required this.navigate,
    required this.updateAuth,
    required this.updatePrefs,
  });

  final Bridge bridge;

  /// Translations for the active locale (the `s` prop).
  final Strings strings;

  final Bootstrap bootstrap;

  /// The current route. Screens read their arguments from it (for example an
  /// `OperationRoute` carries the operation to show).
  final AppRoute route;

  final void Function(AppRoute route) navigate;
  final void Function(AuthState auth) updateAuth;
  final void Function(AppPrefs prefs) updatePrefs;

  AuthState get auth => bootstrap.auth;

  AppPrefs get prefs => bootstrap.prefs;

  void goHome() => navigate(const HomeRoute());
}

/// Builds the widget for one screen.
typedef ScreenBuilder = Widget Function(ScreenContext context);

/// Maps [ScreenId] to its builder.
final class ScreenRegistry {
  ScreenRegistry();

  final Map<ScreenId, ScreenBuilder> _builders = {};

  /// Registers [builder] for [id]. Each screen has exactly one owner, so a
  /// second registration is a programming error.
  void register(ScreenId id, ScreenBuilder builder) {
    if (_builders.containsKey(id)) {
      throw StateError('Screen ${id.name} is already registered');
    }
    _builders[id] = builder;
  }

  bool isRegistered(ScreenId id) => _builders.containsKey(id);

  /// Builds the screen for [id]. Throws [StateError] when none is registered.
  Widget build(ScreenId id, ScreenContext context) {
    final builder = _builders[id];
    if (builder == null) {
      throw StateError('No screen registered for ${id.name}');
    }
    return builder(context);
  }
}
