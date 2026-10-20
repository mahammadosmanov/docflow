import 'package:flutter/foundation.dart';

import '../bridge/bridge.dart';
import '../l10n/strings.dart';
import '../models/models.dart';
import 'routes.dart';

/// App-level state (`App` in App.tsx): the bootstrap payload, the current
/// route and the fatal bootstrap error. Views rebuild when it notifies.
final class AppController extends ChangeNotifier {
  AppController({required this.bridge});

  final Bridge bridge;

  Bootstrap? _bootstrap;
  String _fatal = '';
  AppRoute _route = const HomeRoute();

  /// The loaded bootstrap payload, or null until the first load succeeds.
  Bootstrap? get bootstrap => _bootstrap;

  /// The message of a failed bootstrap, or the empty string.
  String get fatal => _fatal;

  AppRoute get route => _route;

  /// Translations for the current preference. Before bootstrap, the system
  /// language (`makeStrings(null)` in App.tsx).
  Strings get strings => makeStrings(_bootstrap?.prefs.locale);

  /// Loads `app/bootstrap`. Clears a previous fatal error first.
  Future<void> load() async {
    _fatal = '';
    notifyListeners();
    try {
      _bootstrap = await bridge.call(FrontendCommands.appBootstrap);
    } catch (error) {
      _fatal = asBridgeError(error).message;
    }
    notifyListeners();
  }

  /// Replaces the auth state after sign-in, sign-out or a flag change.
  void updateAuth(AuthState auth) {
    final current = _bootstrap;
    if (current == null) return;
    _bootstrap = _copy(current, auth: auth);
    notifyListeners();
  }

  /// Replaces the preferences. Theme, font and locale follow immediately.
  void updatePrefs(AppPrefs prefs) {
    final current = _bootstrap;
    if (current == null) return;
    _bootstrap = _copy(current, prefs: prefs);
    notifyListeners();
  }

  void navigate(AppRoute route) {
    _route = route;
    notifyListeners();
  }

  static Bootstrap _copy(
    Bootstrap source, {
    AuthState? auth,
    AppPrefs? prefs,
  }) => Bootstrap(
    protocol: source.protocol,
    version: source.version,
    auth: auth ?? source.auth,
    prefs: prefs ?? source.prefs,
    operations: source.operations,
  );
}
