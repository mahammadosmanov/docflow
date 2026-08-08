import 'package:docflow/core/l10n/strings.dart';
import 'package:docflow/core/operations/models.dart';
import 'package:docflow/ui/forms/guided_session_panel.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';

class _TestStringsDelegate extends LocalizationsDelegate<Strings> {
  const _TestStringsDelegate();

  @override
  bool isSupported(Locale locale) => true;

  @override
  Future<Strings> load(Locale locale) => SynchronousFuture(
    Strings({
      'print.session.completed': 'Print queue completed',
      'print.session.paper': 'Paper',
      'print.session.copies': 'Copies',
      'print.session.instruction': 'Print this file',
      'print.session.paused': 'Queue paused',
      'action.printedNext': 'Printed — next',
      'action.stop': 'Stop',
      'action.continue': 'Continue',
      'action.restartAll': 'Restart all',
      'action.done': 'Done',
    }),
  );

  @override
  bool shouldReload(_TestStringsDelegate old) => false;
}

void main() {
  GuidedSession session(String status) => GuidedSession.fromJson({
    'sessionId': 'queue-1',
    'revision': 1,
    'opId': 'files.batch_print',
    'status': status,
    'current': status == 'completed' ? 2 : 1,
    'total': 2,
    'currentJob': status == 'completed'
        ? null
        : {'path': r'C:\Docs\second.pdf', 'paperSize': 'A3', 'copies': 2},
  });

  Future<void> pump(
    WidgetTester tester,
    GuidedSession value, {
    required VoidCallback onComplete,
    required VoidCallback onStop,
    required VoidCallback onContinue,
    required VoidCallback onRestart,
    required VoidCallback onDone,
    bool busy = false,
  }) async {
    await tester.pumpWidget(const SizedBox.shrink());
    await tester.pump();
    await tester.pumpWidget(
      MaterialApp(
        key: ValueKey(value.status),
        locale: const Locale('en'),
        supportedLocales: Strings.supported,
        localizationsDelegates: const [
          _TestStringsDelegate(),
          GlobalMaterialLocalizations.delegate,
          GlobalWidgetsLocalizations.delegate,
          GlobalCupertinoLocalizations.delegate,
        ],
        home: Scaffold(
          body: GuidedSessionPanel(
            session: value,
            busy: busy,
            onComplete: onComplete,
            onStop: onStop,
            onContinue: onContinue,
            onRestart: onRestart,
            onDone: onDone,
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
  }

  testWidgets('active guided session shows current instructions and stop', (
    tester,
  ) async {
    var complete = 0;
    var stopped = 0;
    await pump(
      tester,
      session('active'),
      onComplete: () => complete++,
      onStop: () => stopped++,
      onContinue: () {},
      onRestart: () {},
      onDone: () {},
    );

    expect(find.text('second.pdf'), findsOneWidget);
    expect(find.textContaining('A3'), findsOneWidget);
    expect(find.textContaining('2'), findsWidgets);
    await tester.tap(find.byKey(const ValueKey('guided-complete')));
    await tester.tap(find.byKey(const ValueKey('guided-stop')));
    expect(complete, 1);
    expect(stopped, 1);
  });

  testWidgets('paused session offers continue and restart-all', (tester) async {
    var continued = 0;
    var restarted = 0;
    await pump(
      tester,
      session('paused'),
      onComplete: () {},
      onStop: () {},
      onContinue: () => continued++,
      onRestart: () => restarted++,
      onDone: () {},
    );

    await tester.tap(find.byKey(const ValueKey('guided-continue')));
    await tester.tap(find.byKey(const ValueKey('guided-restart')));
    expect(continued, 1);
    expect(restarted, 1);
  });

  testWidgets('completed session offers done', (tester) async {
    var done = 0;
    await pump(
      tester,
      session('completed'),
      onComplete: () {},
      onStop: () {},
      onContinue: () {},
      onRestart: () {},
      onDone: () => done++,
    );

    await tester.tap(find.byKey(const ValueKey('guided-done')));
    expect(done, 1);
  });

  testWidgets('completed session disables done while busy', (tester) async {
    await pump(
      tester,
      session('completed'),
      onComplete: () {},
      onStop: () {},
      onContinue: () {},
      onRestart: () {},
      onDone: () {},
      busy: true,
    );

    final button = tester.widget<FilledButton>(
      find.byKey(const ValueKey('guided-done')),
    );
    expect(button.onPressed, isNull);
  });

  testWidgets('active actions wrap without overflow on a narrow window', (
    tester,
  ) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(280, 700);
    addTearDown(tester.view.resetDevicePixelRatio);
    addTearDown(tester.view.resetPhysicalSize);

    await pump(
      tester,
      session('active'),
      onComplete: () {},
      onStop: () {},
      onContinue: () {},
      onRestart: () {},
      onDone: () {},
    );

    expect(tester.takeException(), isNull);
  });
}
