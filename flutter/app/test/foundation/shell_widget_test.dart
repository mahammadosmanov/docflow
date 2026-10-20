import 'package:docflow/app/app_frame.dart';
import 'package:docflow/app/doc_flow_app.dart';
import 'package:docflow/app/screen_registry.dart';
import 'package:docflow/app/routes.dart';
import 'package:docflow/engine/engine.dart';
import 'package:docflow/models/models.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// Replies to app/bootstrap with the given auth state, or fails when [error].
final class _BootEngine extends Engine {
  _BootEngine({
    this.signedIn = true,
    this.allowed = true,
    this.isAdmin = false,
    this.error,
  });

  final bool signedIn;
  final bool allowed;
  final bool isAdmin;
  final String? error;

  @override
  Future<Object?> call(
    String command,
    JsonMap params, {
    ProgressHandler? onProgress,
  }) async {
    if (error != null) throw {'code': 'boot_failed', 'message': error};
    expect(command, 'app/bootstrap');
    return {
      'protocol': 1,
      'version': '9.9.9',
      'auth': {
        'signedIn': signedIn,
        'allowed': allowed,
        'isAdmin': isAdmin,
        'session': signedIn
            ? {'userId': 'u1', 'email': 'ada@example.com', 'expiresAt': 1.9e9}
            : null,
        'user': signedIn
            ? {
                'id': 'u1',
                'email': 'ada@example.com',
                'username': 'ada',
                'display': 'Ada',
                'allowed': allowed,
                'isAdmin': isAdmin,
                'createdAt': '2026-01-01T00:00:00Z',
              }
            : null,
      },
      'prefs': {
        'locale': 'en',
        'theme': 'light',
        'font': 'system',
        'fontScale': 1,
        'compact': false,
        'startWithWindows': false,
        'replicaSyncEnabled': false,
      },
      'operations': <Object?>[],
    };
  }
}

ScreenRegistry _registry() {
  final registry = ScreenRegistry();
  for (final id in ScreenId.values) {
    registry.register(
      id,
      (context) => Center(child: Text('screen:${id.name}')),
    );
  }
  return registry;
}

Future<void> _pump(
  WidgetTester tester, {
  required Size size,
  required Engine engine,
}) async {
  tester.view.physicalSize = size;
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  await tester.pumpWidget(
    DocFlowApp(key: UniqueKey(), engine: engine, screens: _registry()),
  );
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('desktop width shows the rail with labels and the home screen', (
    tester,
  ) async {
    await _pump(tester, size: const Size(1280, 800), engine: _BootEngine());

    expect(tester.takeException(), isNull);
    expect(find.text('screen:home'), findsOneWidget);
    expect(find.text('History'), findsOneWidget);
    expect(find.text('Ada'), findsOneWidget);
    expect(find.byType(AppFrame), findsOneWidget);
  });

  testWidgets('tablet width hides the brand text but keeps the frame', (
    tester,
  ) async {
    await _pump(tester, size: const Size(760, 800), engine: _BootEngine());

    expect(tester.takeException(), isNull);
    expect(find.text('History'), findsNothing);
    expect(find.text('screen:home'), findsOneWidget);
  });

  testWidgets('phone width uses the bottom bar and no rail labels', (
    tester,
  ) async {
    await _pump(tester, size: const Size(390, 800), engine: _BootEngine());

    expect(tester.takeException(), isNull);
    expect(find.text('History'), findsNothing);
    expect(find.text('screen:home'), findsOneWidget);
  });

  testWidgets('admin entry appears only for administrators', (tester) async {
    await _pump(
      tester,
      size: const Size(1280, 800),
      engine: _BootEngine(isAdmin: true),
    );
    expect(find.text('Admin panel'), findsOneWidget);

    await _pump(tester, size: const Size(1280, 800), engine: _BootEngine());
    expect(find.text('Admin panel'), findsNothing);
  });

  testWidgets('signed-out users see the login screen without the frame', (
    tester,
  ) async {
    await _pump(
      tester,
      size: const Size(1280, 800),
      engine: _BootEngine(signedIn: false),
    );

    expect(find.text('screen:login'), findsOneWidget);
    expect(find.byType(AppFrame), findsNothing);
  });

  testWidgets('signed-in users awaiting approval see the pending screen', (
    tester,
  ) async {
    await _pump(
      tester,
      size: const Size(1280, 800),
      engine: _BootEngine(allowed: false),
    );

    expect(find.text('screen:pending'), findsOneWidget);
    expect(find.byType(AppFrame), findsNothing);
  });

  testWidgets('a failed bootstrap shows the retry view', (tester) async {
    await _pump(
      tester,
      size: const Size(1280, 800),
      engine: _BootEngine(error: 'engine unavailable'),
    );

    expect(find.text("DocFlow couldn't start"), findsOneWidget);
    expect(find.text('engine unavailable'), findsOneWidget);
    expect(find.text('Try again'), findsOneWidget);
  });

  testWidgets('the shell routes to a settings route inside the frame', (
    tester,
  ) async {
    final engine = _BootEngine();
    tester.view.physicalSize = const Size(1280, 800);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    await tester.pumpWidget(DocFlowApp(engine: engine, screens: _registry()));
    await tester.pumpAndSettle();

    await tester.tap(find.text('Settings'));
    await tester.pumpAndSettle();

    expect(find.text('screen:settings'), findsOneWidget);
    expect(find.text('screen:home'), findsNothing);
    expect(find.byType(AppFrame), findsOneWidget);
  });

  test('SettingsRoute keeps its back target', () {
    expect(
      const SettingsRoute(back: SettingsBack.login).back,
      SettingsBack.login,
    );
  });
}
