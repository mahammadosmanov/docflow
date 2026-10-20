import 'package:docflow/bridge/bridge.dart';
import 'package:docflow/engine/engine.dart';
import 'package:docflow/models/models.dart';
import 'package:flutter_test/flutter_test.dart';

/// Records calls and replies with canned responses or errors.
final class FakeEngine extends Engine {
  FakeEngine(this.respond);

  final Future<Object?> Function(
    String command,
    JsonMap params,
    ProgressHandler? onProgress,
  )
  respond;

  final List<(String, JsonMap)> calls = [];

  @override
  Future<Object?> call(
    String command,
    JsonMap params, {
    ProgressHandler? onProgress,
  }) {
    calls.add((command, params));
    return respond(command, params, onProgress);
  }
}

const Map<String, Object?> _bootstrapJson = {
  'protocol': 1,
  'version': '1.0.0',
  'auth': {
    'signedIn': false,
    'allowed': false,
    'isAdmin': false,
    'session': null,
    'user': null,
  },
  'prefs': {
    'locale': null,
    'theme': 'system',
    'font': 'system',
    'fontScale': 1,
    'compact': false,
    'startWithWindows': false,
    'replicaSyncEnabled': false,
  },
  'operations': [],
};

void main() {
  test('encodes params as the engine request and decodes the result', () async {
    final engine = FakeEngine((command, params, _) async {
      expect(command, 'auth/signIn');
      return {
        'signedIn': true,
        'allowed': true,
        'isAdmin': false,
        'session': null,
        'user': null,
      };
    });
    final bridge = Bridge(engine);

    final state = await bridge.call(
      FrontendCommands.authSignIn,
      params: (identifier: 'ada', password: 'secret'),
    );

    expect(state.signedIn, isTrue);
    expect(engine.calls.single.$2, {'identifier': 'ada', 'password': 'secret'});
  });

  test('no-param commands send an empty object', () async {
    final engine = FakeEngine((_, _, _) async => _bootstrapJson);
    final bridge = Bridge(engine);

    final bootstrap = await bridge.call(FrontendCommands.appBootstrap);

    expect(engine.calls.single.$1, 'app/bootstrap');
    expect(engine.calls.single.$2, isEmpty);
    expect(bootstrap.version, '1.0.0');
    expect(bootstrap.prefs.locale, isNull);
  });

  test('forwards progress events to the handler', () async {
    final engine = FakeEngine((_, _, onProgress) async {
      onProgress?.call(
        const ProgressEvent(phase: 'scan', done: 1, total: 3, subject: 'a.pdf'),
      );
      onProgress?.call(const ProgressEvent(phase: 'scan', done: 3, total: 3));
      return {'applied': 3, 'failed': 0, 'journalId': 'j1', 'errors': []};
    });
    final bridge = Bridge(engine);
    final events = <ProgressEvent>[];

    final result = await bridge.call(
      FrontendCommands.engineApply,
      params: (opId: 'op', planId: 'p1'),
      onProgress: events.add,
    );

    expect(result.applied, 3);
    expect(events.map((e) => e.done), [1, 3]);
    expect(events.first.subject, 'a.pdf');
  });

  test('a command with required params rejects a missing argument', () {
    final bridge = Bridge(FakeEngine((_, _, _) async => null));
    expect(
      () => bridge.call(FrontendCommands.authSignIn),
      throwsA(isA<ArgumentError>()),
    );
  });

  test('engine error objects keep their code and message', () async {
    final bridge = Bridge(
      FakeEngine(
        (_, _, _) async => throw {
          'code': 'invalid_credentials',
          'message': 'Invalid login credentials',
        },
      ),
    );

    await expectLater(
      bridge.call(
        FrontendCommands.authSignIn,
        params: (identifier: 'a', password: 'b'),
      ),
      throwsA(
        isA<BridgeError>()
            .having((e) => e.code, 'code', 'invalid_credentials')
            .having((e) => e.message, 'message', 'Invalid login credentials'),
      ),
    );
  });

  test('engine error objects without a code use the default code', () {
    final error = asBridgeError({'message': 'disk full'});
    expect(error.code, BridgeError.defaultCode);
    expect(error.message, 'disk full');
  });

  test('plain text and other failures become message-only BridgeErrors', () {
    expect(asBridgeError('boom').message, 'boom');
    expect(asBridgeError('boom').code, isNull);
    expect(asBridgeError(StateError('bad')).message, 'Bad state: bad');
    final existing = BridgeError(code: 'x', message: 'y');
    expect(identical(asBridgeError(existing), existing), isTrue);
  });

  test('an engine that is not configured fails with a coded error', () async {
    final bridge = Bridge(const UnconfiguredEngine());
    await expectLater(
      bridge.call(FrontendCommands.appBootstrap),
      throwsA(
        isA<BridgeError>().having(
          (e) => e.code,
          'code',
          'engine_not_configured',
        ),
      ),
    );
  });

  test('a malformed result is reported as a FormatException', () async {
    final bridge = Bridge(FakeEngine((_, _, _) async => {'ok': false}));
    await expectLater(
      bridge.call(
        FrontendCommands.supportCopyDiagnostics,
        params: (content: 'x'),
      ),
      throwsFormatException,
    );
  });
}
