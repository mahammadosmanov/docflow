// Port of the backend.rs and smoke.rs test modules, plus engine routing checks.
import 'dart:io';

import 'package:docflow/engine/backend.dart';
import 'package:docflow/engine/errors.dart';
import 'package:docflow/engine/lib.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

const String _commandsTs = '../../frontend/src/lib/commands.ts';

Matcher _errorCode(String code) => isA<BackendError>().having(
  (error) => error.code,
  'code',
  code,
);

void main() {
  test('authorization gate matches the public and protected method sets', () {
    const publicMethods = {
      BackendMethod.appBootstrap,
      BackendMethod.settingsUpdate,
      BackendMethod.settingsReset,
      BackendMethod.settingsReplicaStatus,
      BackendMethod.supportDiagnostics,
      BackendMethod.supportCopyDiagnostics,
      BackendMethod.authSignIn,
      BackendMethod.authSignUp,
      BackendMethod.authSignOut,
      BackendMethod.authUsernameAvailable,
      BackendMethod.authLoadMe,
      BackendMethod.authSetUsername,
      BackendMethod.authListUsers,
      BackendMethod.authSetFlags,
    };
    for (final method in BackendMethod.values) {
      if (publicMethods.contains(method)) {
        method.authorize(signedIn: false, allowed: false);
        continue;
      }
      expect(
        () => method.authorize(signedIn: false, allowed: false),
        throwsA(_errorCode('auth_required')),
        reason: method.wire,
      );
      expect(
        () => method.authorize(signedIn: true, allowed: false),
        throwsA(_errorCode('permission_denied')),
        reason: method.wire,
      );
      method.authorize(signedIn: true, allowed: true);
    }
  });

  test('method allowlist is exact and rejects internal or legacy names', () {
    for (final method in BackendMethod.values) {
      expect(BackendMethod.fromWire(method.wire), method);
    }
    for (final rejected in [
      'shell/exec',
      'system/ping',
      'system/diagnostics',
      'auth/state',
      'settings/get',
      'engine/listOperations',
      'engine/previewTable',
      'internal/rfiLogGeminiContext',
      'support/readiness',
      'workItems/list',
      'support/sendDiagnostics',
    ]) {
      expect(BackendMethod.fromWire(rejected), isNull, reason: rejected);
    }
  });

  test('Dart and TypeScript frontend method rosters match exactly', () {
    final source = File(_commandsTs).readAsStringSync();
    final roster = source
        .split('// BEGIN RUST-VERIFIED FRONTEND METHODS')[1]
        .split('// END RUST-VERIFIED FRONTEND METHODS')[0];
    final typescript =
        RegExp(r'"([^"]+)"')
            .allMatches(roster)
            .map((match) => match.group(1)!)
            .toList()
          ..sort();
    final dart = BackendMethod.values.map((method) => method.wire).toList()
      ..sort();
    expect(dart, typescript);
    expect(dart.length, 36);
  });

  test('apply envelope accepts identities and rejects executable fields', () {
    final ok = validateApplyEnvelope({'planId': 'opaque', 'opId': 'rfi.log_add'});
    expect(ok.planId, 'opaque');
    expect(ok.opId, 'rfi.log_add');
    expect(validateApplyEnvelope({'planId': 'opaque'}).opId, isNull);
    expect(
      () => validateApplyEnvelope({
        'planId': 'opaque',
        'steps': [
          {'kind': 'move', 'src': '/forged', 'dst': '/victim'},
        ],
      }),
      throwsA(_errorCode('backend_invalid_params')),
    );
    expect(
      () => validateApplyEnvelope({'planId': ''}),
      throwsA(_errorCode('backend_invalid_params')),
    );
  });

  test('RFI workbook picker offers macro-enabled and standard Excel files', () {
    DialogFilter filter(String op, String param) =>
        pathDialogFilter(op, param)!;
    expect(filter('rfi.log_add', 'workbook').extensions, ['xlsm', 'xlsx']);
    expect(
      filter('rfi.follow_up_sync', 'rfi_log_workbook').name,
      'Excel (.xlsm, .xlsx)',
    );
    expect(filter('rfi.system_sync', 'follow_up_workbook').extensions, [
      'xlsm',
      'xlsx',
    ]);
    expect(
      filter('rfi.generate_concrete_documents', 'template_011').extensions,
      ['docx'],
    );
    expect(pathDialogFilter('rfi.process_scans', 'source'), isNull);
  });

  test('each request shape rejects unknown keys and wrong types', () {
    expect(
      () => CancelRequest.decode({'planId': 3}),
      throwsA(_errorCode('backend_invalid_params')),
    );
    expect(
      () => decodeHistoryLimit({'limit': 'many'}),
      throwsA(_errorCode('backend_invalid_params')),
    );
    expect(
      () => SelectPathsRequest.decode({
        'opId': 'x',
        'paramName': 'y',
        'extra': 1,
      }),
      throwsA(_errorCode('backend_invalid_params')),
    );
    expect(decodeHistoryLimit({}), 50);
    expect(
      SavePresetRequest.decode({
        'opId': 'files.batch_print',
        'name': 'A',
        'values': <String, Object?>{},
      }).replace,
      isFalse,
    );
  });

  test('unwired native operations fail with engine_not_wired', () async {
    const native = UnwiredNativeOperations();
    expect(
      native.syncResult('files.batch_print', <String, Object?>{}),
      completes,
    );
    expect(native.cancelPlan('plan'), isFalse);
    expect(native.printerDiagnostics(null).inventoryStatus, 'unavailable');
  });

  group('DocFlowEngine routing', () {
    late Directory directory;

    setUp(() {
      directory = Directory.systemTemp.createTempSync('docflow-engine');
    });

    tearDown(() {
      directory.deleteSync(recursive: true);
    });

    test('bootstrap reports protocol 1 and the ten operations', () async {
      final engine = buildDocFlowEngine(
        settingsPath: p.join(directory.path, 'settings.json'),
      );
      final result = (await engine.call('app/bootstrap', const {}))!
          as Map<String, Object?>;
      expect(result['protocol'], 1);
      expect(result['version'], docflowVersion);
      expect((result['operations'] as List).length, 10);
      expect((result['auth'] as Map)['signedIn'], false);
    });

    test('unknown command names are rejected before any work runs', () async {
      final engine = buildDocFlowEngine(
        settingsPath: p.join(directory.path, 'settings.json'),
      );
      await expectLater(
        engine.call('shell/exec', const {}),
        throwsA(_errorCode('backend_protocol_error')),
      );
    });

    test('protected commands require sign-in before routing', () async {
      final engine = buildDocFlowEngine(
        settingsPath: p.join(directory.path, 'settings.json'),
      );
      await expectLater(
        engine.call('engine/preview', {'opId': 'rfi.group_files'}),
        throwsA(_errorCode('auth_required')),
      );
      await expectLater(
        engine.call('history/list', const {}),
        throwsA(_errorCode('auth_required')),
      );
    });

    test('settings update persists preferences and validates keys', () async {
      final path = p.join(directory.path, 'settings.json');
      final engine = buildDocFlowEngine(settingsPath: path);
      final updated = (await engine.call('settings/update', {
        'locale': 'az',
        'fontScale': 9,
      }))! as Map<String, Object?>;
      expect(updated['locale'], 'az');
      expect(updated['fontScale'], 1.6);
      final reread = (await buildDocFlowEngine(
        settingsPath: path,
      ).call('settings/replicaStatus', const {}))! as Map<String, Object?>;
      expect(reread['configured'], false);
      expect(reread['status'], 'unconfigured');

      await expectLater(
        engine.call('settings/update', {'theme': 'dark', 'unknown': 1}),
        throwsA(_errorCode('backend_invalid_params')),
      );
      await expectLater(
        engine.call('settings/update', {'replicaSyncEnabled': true}),
        throwsA(_errorCode('validation')),
      );
    });

    test('engine cancel and apply are gated before envelope validation', () async {
      final engine = buildDocFlowEngine(
        settingsPath: p.join(directory.path, 'settings.json'),
      );
      // Protected methods fail on the sign-in gate before their params are read.
      await expectLater(
        engine.call('engine/cancel', const {}),
        throwsA(_errorCode('auth_required')),
      );
      await expectLater(
        engine.call('engine/apply', {'planId': 'x', 'extra': true}),
        throwsA(_errorCode('auth_required')),
      );
    });

    test('copy diagnostics validates content and diagnostics record errors', () async {
      final engine = buildDocFlowEngine(
        settingsPath: p.join(directory.path, 'settings.json'),
      );
      await expectLater(
        engine.call('support/copyDiagnostics', {'content': '{}'}),
        throwsA(_errorCode('validation')),
      );
      final bundle = (await engine.call('support/diagnostics', const {}))!
          as Map<String, Object?>;
      expect(bundle['fileName'], 'docflow-support-diagnostics-v1.json');
      expect(bundle['content'], contains('"privacy"'));
      expect(
        (bundle['content']! as String).contains('validation'),
        isTrue,
      );
    });
  });
}
