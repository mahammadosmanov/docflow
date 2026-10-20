// Widget tests for the home and operation screens. They drive the real Bridge
// through a fake Engine, so the typed command map and models are exercised.

import 'package:docflow/app/routes.dart';
import 'package:docflow/app/screen_registry.dart';
import 'package:docflow/bridge/bridge.dart';
import 'package:docflow/engine/engine.dart';
import 'package:docflow/models/models.dart';
import 'package:docflow/ui/screens/home_screen.dart';
import 'package:docflow/ui/screens/operation_screen.dart';
import 'package:flutter/material.dart' hide IconButton, Switch;
import 'package:flutter_test/flutter_test.dart';

import '../widgets/harness.dart';

typedef Handler = Object? Function(JsonMap params);

final class FakeEngine extends Engine {
  FakeEngine(this.handlers);

  final Map<String, Handler> handlers;
  final List<(String, JsonMap)> calls = [];

  List<JsonMap> callsTo(String command) => [
    for (final call in calls)
      if (call.$1 == command) call.$2,
  ];

  @override
  Future<Object?> call(
    String command,
    JsonMap params, {
    ProgressHandler? onProgress,
  }) async {
    calls.add((command, params));
    final handler = handlers[command];
    if (handler == null) {
      throw StateError('Unexpected engine command $command');
    }
    return handler(params);
  }
}

OperationDef _operation(String id, String titleKey) => OperationDef(
  id: id,
  titleKey: titleKey,
  descriptionKey: '$titleKey.help',
  schema: const [],
  supportsPresets: false,
);

ScreenContext _context({
  required Bridge bridge,
  required AppRoute route,
  List<OperationDef> operations = const [],
  void Function(AppRoute route)? navigate,
  void Function(AuthState auth)? updateAuth,
}) {
  final bootstrap = Bootstrap.fromJson({
    'protocol': bootstrapProtocolVersion,
    'version': '2.0.0',
    'auth': AuthState.signedOut.toJson(),
    'prefs': AppPrefs.defaults.toJson(),
    'operations': [for (final operation in operations) operation.toJson()],
  });
  return ScreenContext(
    bridge: bridge,
    strings: testStrings(),
    bootstrap: bootstrap,
    route: route,
    navigate: navigate ?? (_) {},
    updateAuth: updateAuth ?? (_) {},
    updatePrefs: (_) {},
  );
}

Future<void> settle(WidgetTester tester) async {
  for (var i = 0; i < 8; i++) {
    await tester.pump(const Duration(milliseconds: 20));
  }
}

/// Stand-in stage views: the real PlanView and ResultView have their own
/// tests, so these only expose the callbacks the operation screen wires.
final class _FakeStages implements OperationStageViews {
  @override
  Widget planView(PlanStageProps props) => Column(
    children: [
      Text('plan:${props.plan.planId}'),
      TextButton(onPressed: props.onApply, child: const Text('apply-plan')),
      TextButton(onPressed: props.onCancel, child: const Text('cancel-plan')),
    ],
  );

  @override
  Widget resultView(ResultStageProps props) =>
      Text('result:${props.result.applied}');
}

JsonMap _plan(String planId, String opId) => {
  'planId': planId,
  'opId': opId,
  'steps': [
    {'kind': 'move', 'src': 'a', 'dst': 'b', 'noteKey': null, 'meta': {}},
  ],
  'warnings': <Object?>[],
  'meta': <String, Object?>{},
};

const _desktop = Size(1100, 800);

void main() {
  final s = testStrings();
  final sortOp = _operation('files.sort', 'op.files.sort');
  final renameOp = _operation('files.rename', 'op.files.rename');

  group('HomeScreen', () {
    testWidgets('lists operations, filters by search and opens one', (
      tester,
    ) async {
      final engine = FakeEngine({});
      final routes = <AppRoute>[];
      await pumpWidgetApp(
        tester,
        HomeScreen(
          screen: _context(
            bridge: Bridge(engine),
            route: const HomeRoute(),
            operations: [sortOp, renameOp],
            navigate: routes.add,
          ),
        ),
        size: _desktop,
      );
      await settle(tester);

      expect(find.text(s.t(sortOp.titleKey)), findsWidgets);
      expect(find.text(s.t(renameOp.titleKey)), findsWidgets);

      await tester.enterText(find.byType(TextField), 'rename');
      await settle(tester);
      expect(find.text(s.t(sortOp.titleKey)), findsNothing);
      expect(find.text(s.t(renameOp.titleKey)), findsWidgets);

      await tester.tap(find.text(s.t(renameOp.titleKey)).first);
      await settle(tester);
      expect(
        routes.whereType<OperationRoute>().single.operation.id,
        'files.rename',
      );
    });

    testWidgets('an unmatched search shows the empty state', (tester) async {
      await pumpWidgetApp(
        tester,
        HomeScreen(
          screen: _context(
            bridge: Bridge(FakeEngine({})),
            route: const HomeRoute(),
            operations: [sortOp],
          ),
        ),
        size: _desktop,
      );
      await settle(tester);
      await tester.enterText(find.byType(TextField), 'zzzz');
      await settle(tester);
      expect(
        find.text(s.t('ops.emptySearch', {'query': 'zzzz'})),
        findsOneWidget,
      );
    });

    testWidgets('sign out reports the signed-out state', (tester) async {
      final engine = FakeEngine({
        'auth/signOut': (_) => AuthState.signedOut.toJson(),
      });
      AuthState? reported;
      await pumpWidgetApp(
        tester,
        HomeScreen(
          screen: _context(
            bridge: Bridge(engine),
            route: const HomeRoute(),
            operations: [sortOp],
            updateAuth: (auth) => reported = auth,
          ),
        ),
        size: _desktop,
      );
      await settle(tester);
      await tester.tap(find.bySemanticsLabel(s.t('auth.signOut')).first);
      await settle(tester);
      expect(engine.callsTo('auth/signOut'), hasLength(1));
      expect(reported, isNotNull);
    });
  });

  group('OperationScreen', () {
    Future<FakeEngine> pumpOperation(
      WidgetTester tester, {
      Object? Function(JsonMap)? apply,
    }) async {
      final engine = FakeEngine({
        'settings/params': (_) => <String, Object?>{},
        'settings/saveParams': (_) => {'ok': true},
        'engine/preview': (params) => _plan('p-1', '${params['opId']}'),
        'engine/apply':
            apply ??
            (_) => {
              'applied': 1,
              'failed': 0,
              'journalId': 'j-1',
              'errors': <Object?>[],
            },
      });
      final route = OperationRoute(operation: sortOp);
      await pumpWidgetApp(
        tester,
        OperationScreen(
          screen: _context(
            bridge: Bridge(engine),
            route: route,
            operations: [sortOp],
          ),
          route: route,
          stages: _FakeStages(),
        ),
        size: _desktop,
      );
      await settle(tester);
      return engine;
    }

    testWidgets('preview then apply walks form, plan and result', (
      tester,
    ) async {
      final engine = await pumpOperation(tester);
      expect(engine.callsTo('settings/params').single['opId'], 'files.sort');

      await tester.tap(find.text(s.t('action.preview')).last);
      await settle(tester);
      expect(engine.callsTo('engine/preview').single['opId'], 'files.sort');
      expect(find.text('plan:p-1'), findsOneWidget);

      await tester.tap(find.text('apply-plan'));
      await settle(tester);
      final applied = engine.callsTo('engine/apply').single;
      expect(applied['planId'], 'p-1');
      expect(applied['opId'], 'files.sort');
      expect(find.text('result:1'), findsOneWidget);
    });

    testWidgets('cancel returns from the plan to the form', (tester) async {
      await pumpOperation(tester);
      await tester.tap(find.text(s.t('action.preview')).last);
      await settle(tester);
      await tester.tap(find.text('cancel-plan'));
      await settle(tester);
      expect(find.text('plan:p-1'), findsNothing);
      expect(find.text(s.t('action.preview')), findsWidgets);
    });

    testWidgets('an apply failure keeps the plan and shows the error', (
      tester,
    ) async {
      await pumpOperation(tester, apply: (_) => throw StateError('disk full'));
      await tester.tap(find.text(s.t('action.preview')).last);
      await settle(tester);
      await tester.tap(find.text('apply-plan'));
      await settle(tester);
      expect(find.text('plan:p-1'), findsOneWidget);
      expect(find.textContaining('result:'), findsNothing);
    });
  });
}
