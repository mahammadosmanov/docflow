import 'package:docflow/app/app_controller.dart';
import 'package:docflow/app/routes.dart';
import 'package:docflow/app/screen_registry.dart';
import 'package:docflow/bridge/bridge.dart';
import 'package:docflow/engine/engine.dart';
import 'package:docflow/l10n/strings.dart';
import 'package:docflow/models/models.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

final class _ScriptedEngine extends Engine {
  _ScriptedEngine(this.onCall);

  final Future<Object?> Function(String command) onCall;

  @override
  Future<Object?> call(
    String command,
    JsonMap params, {
    ProgressHandler? onProgress,
  }) => onCall(command);
}

Map<String, Object?> _bootstrap({bool signedIn = true, bool allowed = true}) =>
    {
      'protocol': 1,
      'version': '1.0.0',
      'auth': {
        'signedIn': signedIn,
        'allowed': allowed,
        'isAdmin': false,
        'session': null,
        'user': null,
      },
      'prefs': {
        'locale': 'en',
        'theme': 'dark',
        'font': 'serif',
        'fontScale': 1.25,
        'compact': false,
        'startWithWindows': false,
        'replicaSyncEnabled': false,
      },
      'operations': <Object?>[],
    };

void main() {
  test('load stores the bootstrap payload', () async {
    final controller = AppController(
      bridge: Bridge(_ScriptedEngine((_) async => _bootstrap())),
    );

    await controller.load();

    expect(controller.fatal, isEmpty);
    expect(controller.bootstrap?.version, '1.0.0');
    expect(controller.strings.locale, AppLocale.en);
    expect(controller.route, isA<HomeRoute>());
  });

  test('a failed load keeps the message as the fatal state', () async {
    final controller = AppController(
      bridge: Bridge(
        _ScriptedEngine(
          (_) async => throw {'code': 'boom', 'message': 'engine unavailable'},
        ),
      ),
    );

    await controller.load();

    expect(controller.bootstrap, isNull);
    expect(controller.fatal, 'engine unavailable');
  });

  test(
    'updateAuth and updatePrefs replace only their part and notify',
    () async {
      final controller = AppController(
        bridge: Bridge(_ScriptedEngine((_) async => _bootstrap())),
      );
      await controller.load();
      var notifications = 0;
      controller.addListener(() => notifications++);

      controller.updatePrefs(
        const AppPrefs(
          locale: AppLocale.ru,
          theme: ThemePreference.light,
          font: FontPreference.mono,
          fontScale: 1,
          compact: true,
          startWithWindows: false,
          replicaSyncEnabled: false,
        ),
      );
      controller.updateAuth(AuthState.signedOut);

      expect(notifications, 2);
      expect(controller.bootstrap?.prefs.locale, AppLocale.ru);
      expect(controller.bootstrap?.auth.signedIn, isFalse);
      expect(controller.bootstrap?.version, '1.0.0');
    },
  );

  test('navigate changes the route', () {
    final controller = AppController(
      bridge: Bridge(const UnconfiguredEngine()),
    );
    controller.navigate(const HistoryRoute());
    expect(controller.route.screen, ScreenId.history);
  });

  test('registry rejects duplicates and unregistered screens', () {
    final registry = ScreenRegistry();
    expect(registry.isRegistered(ScreenId.home), isFalse);
    registry.register(ScreenId.home, (_) => const SizedBox());
    expect(registry.isRegistered(ScreenId.home), isTrue);
    expect(
      () => registry.register(ScreenId.home, (_) => const SizedBox()),
      throwsStateError,
    );
    expect(
      () => registry.build(ScreenId.admin, _emptyContext()),
      throwsStateError,
    );
  });
}

ScreenContext _emptyContext() => ScreenContext(
  bridge: Bridge(const UnconfiguredEngine()),
  strings: makeStrings(AppLocale.en),
  bootstrap: Bootstrap.fromJson(_bootstrap()),
  route: const HomeRoute(),
  navigate: (_) {},
  updateAuth: (_) {},
  updatePrefs: (_) {},
);
