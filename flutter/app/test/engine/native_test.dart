import 'dart:io';

import 'package:docflow/engine/auth.dart' as auth;
import 'package:docflow/engine/errors.dart';
import 'package:docflow/engine/http.dart';
import 'package:docflow/engine/native.dart';
import 'package:docflow/engine/paths.dart';
import 'package:docflow/engine/path_selection.dart';
import 'package:docflow/engine/settings.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

import 'e5_support.dart';

void main() {
  late Directory directory;
  late NativeBackend native;
  const caller = CallerIdentity('owner', 'session');

  setUp(() {
    directory = Directory.systemTemp.createTempSync('docflow-native-');
    final settings = SettingsStore(p.join(directory.path, 'settings.json'));
    final config = auth.AuthConfig('https://example.invalid', 'key');
    final authService = auth.AuthService(
      settings,
      FixtureCredentials(),
      HttpClient(const HttpConfig()),
      config,
    );
    native = NativeBackend.production(
      selections: PathSelectionStore(ttl: const Duration(minutes: 30)),
      settings: settings,
      authService: authService,
      authConfig: config,
      appDataDir: directory.path,
    );
  });

  tearDown(() => directory.deleteSync(recursive: true));

  test('registry facts and cancel of unknown ids are safe no-ops', () {
    expect(native.registryFacts().printers, isA<List<String>>());
    expect(native.cancelPlan('missing'), isFalse);
    expect(native.cancelOperation('missing'), isFalse);
    expect(native.cloud, isNotNull);
  });

  test('preview rejects an unknown operation id before dispatch', () async {
    await expectLater(
      native.preview(caller, 'rfi.unknown', const {
        'opId': 'rfi.unknown',
        'values': <String, Object?>{},
      }, null),
      throwsA(isA<BackendError>().having((e) => e.code, 'code', 'validation')),
    );
  });

  test('preview fails closed on a selection it never issued', () async {
    await expectLater(
      native.preview(caller, 'files.batch_print', const {
        'opId': 'files.batch_print',
        'values': {
          'files': [
            {'selectionId': 'nope', 'displayPath': 'x', 'name': 'x'},
          ],
        },
      }, null),
      throwsA(isA<BackendError>()),
    );
  });

  test('preview of an operation outside the registry is rejected', () async {
    await expectLater(
      native.preview(caller, 'rfi.review', const {
        'opId': 'rfi.review',
        'values': <String, Object?>{},
      }, null),
      throwsA(isA<BackendError>().having((e) => e.code, 'code', 'validation')),
    );
  });

  test(
    'sync result marks cloudSynced false when the journal is missing',
    () async {
      final result = <String, Object?>{
        'applied': 1,
        'failed': 0,
        'journalId': 'no-such-journal',
      };
      await native.syncResult('files.batch_print', result);
      expect(result['cloudSynced'], isFalse);
    },
  );

  group('native previews and methods (error paths and empty inputs)', () {
    late Directory work;
    late PathSelectionStore store;
    late NativeBackend backend;
    const caller2 = CallerIdentity('owner', 'session');

    setUp(() {
      work = Directory.systemTemp.createTempSync('docflow-native-ops-');
      store = PathSelectionStore(ttl: const Duration(minutes: 30));
      final settings = SettingsStore(p.join(work.path, 'settings.json'));
      final config = auth.AuthConfig('https://example.invalid', 'key');
      backend = NativeBackend.production(
        selections: store,
        settings: settings,
        authService: auth.AuthService(
          settings,
          FixtureCredentials(),
          HttpClient(const HttpConfig()),
          config,
        ),
        authConfig: config,
        appDataDir: work.path,
      );
    });

    tearDown(() => work.deleteSync(recursive: true));

    String codeOf(Object error) => (error as BackendError).code;

    test('concrete documents reject an empty RFI selection', () async {
      await expectLater(
        backend.preview(caller2, 'rfi.generate_concrete_documents', {
          'opId': 'rfi.generate_concrete_documents',
          'values': <String, Object?>{},
        }, null),
        throwsA(
          isA<BackendError>().having(
            (e) => e.message,
            'message',
            contains('Enter RFI numbers'),
          ),
        ),
      );
    });

    test('process scans previews an empty folder into a cached plan', () async {
      final source = Directory(p.join(work.path, 'scans'))..createSync();
      final dest = Directory(p.join(work.path, 'out'))..createSync();
      final src = store.issue(
        caller2,
        'rfi.process_scans_fast_covers',
        'source',
        source.path,
        SelectionKind.folder,
      );
      final dst = store.issue(
        caller2,
        'rfi.process_scans_fast_covers',
        'dest',
        dest.path,
        SelectionKind.folder,
      );
      final plan = await backend.preview(
        caller2,
        'rfi.process_scans_fast_covers',
        {
          'opId': 'rfi.process_scans_fast_covers',
          'values': {
            'source': src.toJson(),
            'dest': dst.toJson(),
            'scan_mode': 'multi_rfi',
            'organize_into_rfi_folders': false,
            'keep_original': true,
            'correct_orientation': false,
          },
        },
        null,
      ) as Map<String, Object?>;
      expect(plan['opId'], 'rfi.process_scans_fast_covers');
      expect(plan['planId'], isNotEmpty);
    });

    test('rfi log preview rejects a missing workbook parameter', () async {
      await expectLater(
        backend.preview(caller2, 'rfi.log_add', {
          'opId': 'rfi.log_add',
          'values': <String, Object?>{},
        }, null),
        throwsA(isA<BackendError>()),
      );
    });

    test('find print files rejects a root that is not a selection', () async {
      await expectLater(
        backend.findRfiPrintFiles(caller2, const {
          'root': 'not-a-selection',
          'numbers': <String>[],
          'firstPage': true,
          'sketch': false,
        }),
        throwsA(isA<BackendError>()),
      );
    });

    test('review preview fails on an unknown destination selection', () async {
      await expectLater(
        backend.reviewPreview(caller2, const {
          'destination': {
            'selectionId': 'nope',
            'displayPath': 'x',
            'name': 'x',
          },
          'decisions': <Object?>[],
        }),
        throwsA(isA<BackendError>()),
      );
    });

    test('plan-bound methods reject an unknown plan id', () async {
      await expectLater(
        backend.copyTable(caller2, const {'planId': 'missing'}),
        throwsA(isA<BackendError>()),
      );
      await expectLater(
        backend.revise(caller2, const {
          'planId': 'missing',
          'rows': <Object?>[],
        }),
        throwsA(isA<BackendError>()),
      );
      await expectLater(
        backend.rfiReplyPreview(caller2, const {
          'planId': 'missing',
          'journalId': 'missing',
        }),
        throwsA(isA<BackendError>()),
      );
    });

    test('review scan rejects an unknown destination selection', () async {
      await expectLater(
        backend.reviewScan(caller2, const {
          'destination': {
            'selectionId': 'nope',
            'displayPath': 'x',
            'name': 'x',
          },
        }),
        throwsA(isA<BackendError>()),
      );
    });
  });
}
