// Widget tests for the account and settings screens: login, pending, admin
// and settings. Each test drives the screen through a fake engine and checks
// the command it sent, and what the screen shows after the reply.

import 'dart:io';

import 'package:docflow/app/routes.dart';
import 'package:docflow/app/screen_registry.dart';
import 'package:docflow/bridge/bridge.dart';
import 'package:docflow/engine/engine.dart';
import 'package:docflow/l10n/strings.dart';
import 'package:docflow/models/auth.dart';
import 'package:docflow/models/bootstrap.dart';
import 'package:docflow/models/json.dart';
import 'package:docflow/models/prefs.dart';
import 'package:docflow/models/support.dart';
import 'package:docflow/ui/screens/admin_screen.dart';
import 'package:docflow/ui/screens/diagnostics_saver.dart';
import 'package:docflow/ui/screens/login_screen.dart';
import 'package:docflow/ui/screens/pending_screen.dart';
import 'package:docflow/ui/screens/settings_screen.dart';
import 'package:docflow/ui/widgets/widgets.dart';
import 'package:flutter/material.dart' hide IconButton, Switch;
import 'package:flutter_test/flutter_test.dart';

import '../widgets/harness.dart';

/// Answers each command from [replies]. A reply may throw an engine error
/// object, as the Rust engine does.
final class FakeEngine extends Engine {
  FakeEngine(this.replies);

  final Map<String, Object? Function(JsonMap params)> replies;
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
    final reply = replies[command];
    if (reply == null) throw StateError('No reply for $command');
    return reply(params);
  }
}

/// Records every diagnostics bundle it is asked to save.
final class RecordingSaver implements DiagnosticsSaver {
  final List<SupportBundlePreview> saved = [];

  @override
  Future<String> save(SupportBundlePreview bundle) async {
    saved.add(bundle);
    return '/downloads/${bundle.fileName}';
  }
}

JsonMap authJson({
  bool signedIn = true,
  bool allowed = true,
  bool isAdmin = false,
}) => {
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
};

JsonMap prefsJson({String? locale = 'en', String theme = 'light'}) => {
  'locale': locale,
  'theme': theme,
  'font': 'system',
  'fontScale': 1,
  'compact': false,
  'startWithWindows': false,
  'replicaSyncEnabled': false,
};

JsonMap replicaJson({String status = 'idle', String? lastErrorCode}) => {
  'enabled': false,
  'configured': false,
  'status': status,
  'lastSuccessUnix': null,
  'lastErrorCode': lastErrorCode,
};

JsonMap userJson(String id, String display, {bool allowed = true}) => {
  'id': id,
  'email': '${display.toLowerCase()}@example.com',
  'username': display.toLowerCase(),
  'display': display,
  'allowed': allowed,
  'isAdmin': false,
  'createdAt': '2026-01-01T00:00:00Z',
};

JsonMap diagnosticsJson() => {
  'fileName': 'docflow-support.json',
  'mediaType': 'application/json',
  'byteLength': 27,
  'content': '{"app":"docflow","ok":true}',
};

final JsonMap okJson = {'ok': true};

Strings get s => testStrings();

Future<void> pumpLogin(
  WidgetTester tester,
  FakeEngine engine, {
  List<AuthState>? signedInStates,
  VoidCallback? onSettings,
}) => pumpWidgetApp(
  tester,
  LoginScreen(
    bridge: Bridge(engine),
    s: s,
    onAuth: (auth) => signedInStates?.add(auth),
    onSettings: onSettings ?? () {},
  ),
);

Finder submitButton(String label) => find.widgetWithText(DfButton, label);

DfButton button(WidgetTester tester, String label) =>
    tester.widget<DfButton>(submitButton(label));

void main() {
  group('LoginScreen', () {
    testWidgets('sign-in needs an email with @ and a password of 6+', (
      tester,
    ) async {
      await pumpLogin(tester, FakeEngine({}));

      expect(button(tester, s.t('auth.signIn')).onPressed, isNull);

      await tester.enterText(find.byType(TextField).at(0), 'ada@example.com');
      await tester.enterText(find.byType(TextField).at(1), '12345');
      await tester.pump();
      expect(button(tester, s.t('auth.signIn')).onPressed, isNull);

      await tester.enterText(find.byType(TextField).at(1), '123456');
      await tester.pump();
      expect(button(tester, s.t('auth.signIn')).onPressed, isNotNull);
    });

    testWidgets('signs in with the trimmed email and passes the state up', (
      tester,
    ) async {
      final engine = FakeEngine({'auth/signIn': (_) => authJson()});
      final states = <AuthState>[];
      await pumpLogin(tester, engine, signedInStates: states);

      await tester.enterText(
        find.byType(TextField).at(0),
        '  ada@example.com ',
      );
      await tester.enterText(find.byType(TextField).at(1), 'secret1');
      await tester.pump();
      await tester.tap(submitButton(s.t('auth.signIn')));
      await tester.pumpAndSettle();

      expect(engine.callsTo('auth/signIn'), [
        {'identifier': 'ada@example.com', 'password': 'secret1'},
      ]);
      expect(states, hasLength(1));
      expect(states.single.signedIn, isTrue);
      expect(states.single.user?.display, 'Ada');
    });

    testWidgets('shows the humanised engine error when sign-in fails', (
      tester,
    ) async {
      final engine = FakeEngine({
        'auth/signIn': (_) => throw {
          'code': 'invalid_credentials',
          'message': 'invalid_credentials',
        },
      });
      await pumpLogin(tester, engine);

      await tester.enterText(find.byType(TextField).at(0), 'ada@example.com');
      await tester.enterText(find.byType(TextField).at(1), 'wrong-pass');
      await tester.pump();
      await tester.tap(submitButton(s.t('auth.signIn')));
      await tester.pumpAndSettle();

      expect(find.text(s.t('auth.err.credentials')), findsOneWidget);
      expect(button(tester, s.t('auth.signIn')).onPressed, isNotNull);
    });

    testWidgets('toggles password visibility', (tester) async {
      await pumpLogin(tester, FakeEngine({}));

      expect(
        tester.widget<TextField>(find.byType(TextField).at(1)).obscureText,
        isTrue,
      );
      await tester.tap(find.byTooltip(s.t('auth.show')));
      await tester.pump();
      expect(
        tester.widget<TextField>(find.byType(TextField).at(1)).obscureText,
        isFalse,
      );
      await tester.tap(find.byTooltip(s.t('auth.hide')));
      await tester.pump();
      expect(
        tester.widget<TextField>(find.byType(TextField).at(1)).obscureText,
        isTrue,
      );
    });

    testWidgets(
      'sign-up checks the username after the pause and blocks a taken name',
      (tester) async {
        final engine = FakeEngine({'auth/usernameAvailable': (_) => false});
        await pumpLogin(tester, engine);

        await tester.tap(find.text(s.t('auth.signUp')));
        await tester.pump();
        await tester.enterText(find.byType(TextField).at(0), 'ada@example.com');
        await tester.enterText(find.byType(TextField).at(1), 'ada');
        await tester.enterText(find.byType(TextField).at(2), 'secret1');
        await tester.pump();

        // Nothing is sent before the 450ms pause ends.
        expect(engine.callsTo('auth/usernameAvailable'), isEmpty);
        await tester.pump(const Duration(milliseconds: 500));
        await tester.pumpAndSettle();

        expect(engine.callsTo('auth/usernameAvailable'), [
          {'username': 'ada'},
        ]);
        expect(find.text(s.t('auth.err.usernameTaken')), findsOneWidget);
        expect(button(tester, s.t('auth.signUp')).onPressed, isNull);
      },
    );

    testWidgets('sign-up sends the trimmed fields and confirms the account', (
      tester,
    ) async {
      final engine = FakeEngine({
        'auth/usernameAvailable': (_) => true,
        'auth/signUp': (_) => okJson,
      });
      await pumpLogin(tester, engine);

      await tester.tap(find.text(s.t('auth.signUp')));
      await tester.pump();
      await tester.enterText(find.byType(TextField).at(0), ' ada@example.com ');
      await tester.enterText(find.byType(TextField).at(1), 'ada_1');
      await tester.enterText(find.byType(TextField).at(2), 'secret1');
      await tester.pump(const Duration(milliseconds: 500));
      await tester.pumpAndSettle();

      await tester.tap(submitButton(s.t('auth.signUp')));
      await tester.pumpAndSettle();

      expect(engine.callsTo('auth/signUp'), [
        {
          'email': 'ada@example.com',
          'username': 'ada_1',
          'password': 'secret1',
        },
      ]);
      expect(find.text(s.t('auth.signupOk')), findsOneWidget);
    });

    testWidgets('the settings gear opens settings from the login page', (
      tester,
    ) async {
      var opened = 0;
      await pumpLogin(tester, FakeEngine({}), onSettings: () => opened++);

      await tester.tap(find.byTooltip(s.t('prefs.title')));
      await tester.pump();
      expect(opened, 1);
    });

    testWidgets(
      'registerLoginScreen routes the gear to settings with back=login',
      (tester) async {
        final registry = ScreenRegistry();
        registerLoginScreen(registry);
        final navigated = <AppRoute>[];
        final engine = FakeEngine({});
        await pumpWidgetApp(
          tester,
          registry.build(
            ScreenId.login,
            ScreenContext(
              bridge: Bridge(engine),
              strings: s,
              bootstrap: Bootstrap.fromJson({
                'protocol': 1,
                'version': '9.9.9',
                'auth': authJson(signedIn: false),
                'prefs': prefsJson(),
                'operations': <Object?>[],
              }),
              route: const HomeRoute(),
              navigate: navigated.add,
              updateAuth: (_) {},
              updatePrefs: (_) {},
            ),
          ),
        );

        await tester.tap(find.byTooltip(s.t('prefs.title')));
        await tester.pump();
        expect(navigated.single, isA<SettingsRoute>());
        expect((navigated.single as SettingsRoute).back, SettingsBack.login);
      },
    );
  });

  group('PendingScreen', () {
    AuthState pendingAuth() => AuthState.fromJson(authJson(allowed: false));

    testWidgets('shows the account email and retry reloads the account', (
      tester,
    ) async {
      final engine = FakeEngine({'auth/loadMe': (_) => authJson()});
      final states = <AuthState>[];
      await pumpWidgetApp(
        tester,
        PendingScreen(
          bridge: Bridge(engine),
          auth: pendingAuth(),
          s: s,
          onAuth: states.add,
          onSettings: () {},
        ),
      );

      expect(find.text(s.t('auth.pending')), findsOneWidget);
      expect(find.textContaining('ada@example.com'), findsOneWidget);

      await tester.tap(find.widgetWithText(DfButton, s.t('action.retry')));
      await tester.pumpAndSettle();
      expect(engine.callsTo('auth/loadMe'), hasLength(1));
      expect(states.single.allowed, isTrue);
    });

    testWidgets('sign out calls auth/signOut and passes the state up', (
      tester,
    ) async {
      final engine = FakeEngine({
        'auth/signOut': (_) => AuthState.signedOut.toJson(),
      });
      final states = <AuthState>[];
      await pumpWidgetApp(
        tester,
        PendingScreen(
          bridge: Bridge(engine),
          auth: pendingAuth(),
          s: s,
          onAuth: states.add,
          onSettings: () {},
        ),
      );

      await tester.tap(find.byTooltip(s.t('auth.signOut')));
      await tester.pumpAndSettle();
      expect(engine.callsTo('auth/signOut'), hasLength(1));
      expect(states.single.signedIn, isFalse);
    });

    testWidgets('a failed retry shows the engine error', (tester) async {
      final engine = FakeEngine({
        'auth/loadMe': (_) => throw {'code': 'x', 'message': 'error.network'},
      });
      await pumpWidgetApp(
        tester,
        PendingScreen(
          bridge: Bridge(engine),
          auth: pendingAuth(),
          s: s,
          onAuth: (_) {},
          onSettings: () {},
        ),
      );

      await tester.tap(find.widgetWithText(DfButton, s.t('action.retry')));
      await tester.pumpAndSettle();
      expect(find.text(s.t('error.network')), findsOneWidget);
    });
  });

  group('AdminScreen', () {
    Future<FakeEngine> pumpAdmin(
      WidgetTester tester, {
      List<JsonMap>? users,
    }) async {
      var current =
          users ??
          [userJson('u1', 'Ada'), userJson('u2', 'Grace', allowed: false)];
      final engine = FakeEngine({
        'auth/listUsers': (_) => current,
        'auth/setFlags': (_) {
          current = [
            userJson('u1', 'Ada'),
            userJson('u2', 'Grace', allowed: true),
          ];
          return okJson;
        },
      });
      await pumpWidgetApp(
        tester,
        AdminScreen(
          bridge: Bridge(engine),
          auth: AuthState.fromJson(authJson(isAdmin: true)),
          s: s,
          onBack: () {},
        ),
      );
      await tester.pumpAndSettle();
      return engine;
    }

    testWidgets('lists accounts, marks you and locks your own switches', (
      tester,
    ) async {
      await pumpAdmin(tester);

      expect(find.text('Ada'), findsOneWidget);
      expect(find.text('Grace'), findsOneWidget);
      expect(find.text(s.t('admin.you')), findsOneWidget);
      expect(find.textContaining(s.t('admin.noAccess')), findsOneWidget);

      final switches = tester.widgetList<Switch>(find.byType(Switch)).toList();
      expect(switches, hasLength(4));
      // Ada (self): both locked. Grace: both enabled.
      expect(switches.map((sw) => sw.disabled), [true, true, false, false]);
    });

    testWidgets('switching access saves the flag and reloads the list', (
      tester,
    ) async {
      final engine = await pumpAdmin(tester);

      await tester.tap(find.byType(Switch).at(2));
      await tester.pumpAndSettle();

      expect(engine.callsTo('auth/setFlags'), [
        {'userId': 'u2', 'allowed': true},
      ]);
      expect(engine.callsTo('auth/listUsers'), hasLength(2));
    });

    testWidgets('shows the empty state when there are no accounts', (
      tester,
    ) async {
      await pumpAdmin(tester, users: const []);
      expect(find.text(s.t('admin.empty')), findsOneWidget);
    });
  });

  group('SettingsScreen', () {
    Future<(FakeEngine, List<AppPrefs>, List<AuthState>)> pumpSettings(
      WidgetTester tester, {
      AuthState? auth,
      JsonMap? prefs,
      DiagnosticsSaver? saver,
      Map<String, Object? Function(JsonMap)> extra = const {},
    }) async {
      final prefsOut = <AppPrefs>[];
      final authOut = <AuthState>[];
      final engine = FakeEngine({
        'settings/replicaStatus': (_) => replicaJson(),
        'settings/update': (params) => {...prefs ?? prefsJson(), ...params},
        'settings/reset': (_) => prefsJson(locale: null),
        'auth/setUsername': (_) => authJson(),
        'support/diagnostics': (_) => diagnosticsJson(),
        'support/copyDiagnostics': (_) => okJson,
        ...extra,
      });
      await pumpWidgetApp(
        tester,
        SettingsScreen(
          bridge: Bridge(engine),
          auth: auth ?? AuthState.fromJson(authJson()),
          prefs: AppPrefs.fromJson(prefs ?? prefsJson()),
          s: s,
          onPrefs: prefsOut.add,
          onAuth: authOut.add,
          onBack: () {},
          diagnosticsSaver: saver ?? RecordingSaver(),
        ),
        size: const Size(1000, 2400),
      );
      await tester.pumpAndSettle();
      return (engine, prefsOut, authOut);
    }

    testWidgets('choosing a language sends the locale, system clears it', (
      tester,
    ) async {
      final (engine, prefsOut, _) = await pumpSettings(tester);

      await tester.tap(find.text(s.t('prefs.lang.tr')));
      await tester.pumpAndSettle();
      expect(engine.callsTo('settings/update').last, {'locale': 'tr'});
      expect(prefsOut.last.locale, AppLocale.tr);

      await tester.tap(find.text(s.t('prefs.lang.system')));
      await tester.pumpAndSettle();
      expect(
        engine.callsTo('settings/update').last,
        containsPair('locale', null),
      );
    });

    testWidgets('choosing a theme and font sends the preference', (
      tester,
    ) async {
      final (engine, _, _) = await pumpSettings(tester);

      await tester.tap(find.text(s.t('prefs.theme.dark')));
      await tester.pumpAndSettle();
      expect(engine.callsTo('settings/update').last, {'theme': 'dark'});

      await tester.tap(find.text(s.t('prefs.font.mono')));
      await tester.pumpAndSettle();
      expect(engine.callsTo('settings/update').last, {'font': 'mono'});
    });

    testWidgets('the compact switch and replica switch send their flags', (
      tester,
    ) async {
      final (engine, _, _) = await pumpSettings(tester);

      // Tap the label, which toggles like the React <label> did.
      await tester.tap(find.text(s.t('prefs.density.compact')));
      await tester.pumpAndSettle();
      expect(engine.callsTo('settings/update').last, {'compact': true});

      await tester.tap(find.text(s.t('prefs.replicaSyncEnabled')));
      await tester.pumpAndSettle();
      expect(engine.callsTo('settings/update').last, {
        'replicaSyncEnabled': true,
      });
    });

    testWidgets('shows the replica status and re-reads it every 5 seconds', (
      tester,
    ) async {
      var status = 'idle';
      final (engine, _, _) = await pumpSettings(
        tester,
        extra: {'settings/replicaStatus': (_) => replicaJson(status: status)},
      );
      expect(find.text(s.t('prefs.replica.status.idle')), findsOneWidget);
      final before = engine.callsTo('settings/replicaStatus').length;

      status = 'paused';
      await tester.pump(replicaStatusInterval);
      await tester.pumpAndSettle();
      expect(
        engine.callsTo('settings/replicaStatus').length,
        greaterThan(before),
      );
      expect(find.text(s.t('prefs.replica.status.paused')), findsOneWidget);
    });

    testWidgets(
      'the account group saves a valid username and shows the state',
      (tester) async {
        final (engine, _, authOut) = await pumpSettings(tester);

        final save = find.widgetWithText(DfButton, s.t('action.save'));
        // The seeded username "ada" matches the pattern, so save is enabled.
        expect(tester.widget<DfButton>(save).onPressed, isNotNull);

        await tester.enterText(find.byType(TextField).first, 'x');
        await tester.pump();
        expect(tester.widget<DfButton>(save).onPressed, isNull);

        await tester.enterText(find.byType(TextField).first, 'ada_lovelace');
        await tester.pump();
        expect(tester.widget<DfButton>(save).onPressed, isNotNull);

        await tester.tap(save);
        await tester.pumpAndSettle();
        expect(engine.callsTo('auth/setUsername'), [
          {'username': 'ada_lovelace'},
        ]);
        expect(authOut, hasLength(1));
        expect(find.text(s.t('prefs.account.saved')), findsOneWidget);
      },
    );

    testWidgets('the account group is hidden when signed out', (tester) async {
      await pumpSettings(
        tester,
        auth: AuthState.fromJson(authJson(signedIn: false)),
      );
      expect(find.text(s.t('prefs.account')), findsNothing);
      expect(find.text(s.t('prefs.language')), findsOneWidget);
    });

    testWidgets('preview shows the diagnostics, copy sends the content', (
      tester,
    ) async {
      final (engine, _, _) = await pumpSettings(tester);

      expect(find.text(s.t('prefs.support.copy')), findsNothing);
      await tester.tap(
        find.widgetWithText(DfButton, s.t('prefs.support.preview')),
      );
      await tester.pumpAndSettle();

      expect(engine.callsTo('support/diagnostics'), hasLength(1));
      expect(
        tester.widget<SelectableText>(find.byType(SelectableText)).data,
        '{"app":"docflow","ok":true}',
      );

      await tester.tap(
        find.widgetWithText(DfButton, s.t('prefs.support.copy')),
      );
      await tester.pumpAndSettle();
      expect(engine.callsTo('support/copyDiagnostics'), [
        {'content': '{"app":"docflow","ok":true}'},
      ]);
      expect(find.text(s.t('prefs.support.copied')), findsOneWidget);
    });

    testWidgets('download hands the bundle to the saver and reports the path', (
      tester,
    ) async {
      final saver = RecordingSaver();
      await pumpSettings(tester, saver: saver);

      await tester.tap(
        find.widgetWithText(DfButton, s.t('prefs.support.preview')),
      );
      await tester.pumpAndSettle();
      await tester.tap(
        find.widgetWithText(DfButton, s.t('prefs.support.download')),
      );
      await tester.pumpAndSettle();

      expect(saver.saved.single.fileName, 'docflow-support.json');
      expect(
        find.textContaining(s.t('prefs.support.downloaded')),
        findsOneWidget,
      );
      expect(
        find.textContaining('/downloads/docflow-support.json'),
        findsOneWidget,
      );
    });

    testWidgets('reset sends settings/reset and passes the defaults up', (
      tester,
    ) async {
      final (engine, prefsOut, _) = await pumpSettings(tester);

      await tester.tap(find.widgetWithText(DfButton, s.t('prefs.reset')));
      await tester.pumpAndSettle();
      expect(engine.callsTo('settings/reset'), hasLength(1));
      expect(prefsOut.last.locale, isNull);
    });

    testWidgets('the font size slider sends the chosen scale', (tester) async {
      final (engine, _, _) = await pumpSettings(tester);

      expect(find.text('100%'), findsOneWidget);
      final slider = find.byType(Slider);
      final rect = tester.getRect(slider);
      await tester.tapAt(Offset(rect.left + rect.width * 0.75, rect.center.dy));
      await tester.pumpAndSettle();

      final sent = engine.callsTo('settings/update').last['fontScale'];
      expect(sent, isA<double>());
      expect((sent as double) > 1.0, isTrue);
    });
  });

  group('FileDiagnosticsSaver', () {
    test('never overwrites: a taken name gets a numbered suffix', () async {
      final dir = Directory('${Directory.current.path}/.dart_tool/saver_test')
        ..createSync(recursive: true);
      addTearDown(() => dir.deleteSync(recursive: true));
      const saver = FileDiagnosticsSaver();
      final bundle = SupportBundlePreview.fromJson({
        'fileName': 'support.json',
        'mediaType': 'application/json',
        'byteLength': 2,
        'content': '{}',
      });
      final bound = FileDiagnosticsSaver(directory: dir.path);
      expect(saver, isNotNull);

      final first = await bound.save(bundle);
      final second = await bound.save(bundle);

      expect(first, endsWith('support.json'));
      expect(second, endsWith('support (1).json'));
      expect(File(first).readAsStringSync(), '{}');
    });
  });
}
