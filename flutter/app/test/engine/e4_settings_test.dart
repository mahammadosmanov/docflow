// Port of the settings.rs test module.
import 'dart:convert';
import 'dart:io';

import 'package:docflow/engine/contracts.dart';
import 'package:docflow/engine/credentials.dart';
import 'package:docflow/engine/registry.dart';
import 'package:docflow/engine/settings.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

OperationDef _operation(String id) => operationRegistry(
  const RegistryFacts(
    printers: ['Office Printer'],
    defaultPrinter: 'Office Printer',
  ),
).firstWhere((operation) => operation.id == id);

Map<String, Object?> _map(Object? value) => jsonDecode(jsonEncode(value))
    as Map<String, Object?>;

Map<String, Object?> _values(String raw) => _map(jsonDecode(raw));

Directory _tempDir() => Directory.systemTemp.createTempSync('docflow-settings');

void main() {
  late Directory directory;

  setUp(() {
    directory = _tempDir();
  });

  tearDown(() {
    directory.deleteSync(recursive: true);
  });

  String settingsPath() => p.join(directory.path, 'settings.json');

  Map<String, Object?> readJson(String path) =>
      jsonDecode(File(path).readAsStringSync()) as Map<String, Object?>;

  test('operation presets round trip without losing unknown settings', () {
    final path = settingsPath();
    File(path).writeAsStringSync('{"custom":{"keep":true}}');
    final store = SettingsStore(path);

    final saved = store.savePreset(
      _operation('files.batch_print'),
      '  A4 duplex  ',
      _values(
        '{"printer":"Office Printer","paper":"A4","duplex":"long","copies":2}',
      ),
      replace: false,
    );

    expect(saved.name, 'A4 duplex');
    expect(
      RegExp(r'^[0-9a-f-]{36}$').hasMatch(saved.id),
      isTrue,
    );
    expect(store.presetsFor(_operation('files.batch_print')), [saved]);
    expect(
      SettingsStore(path).presetsFor(_operation('files.batch_print')),
      [saved],
    );
    final persisted = readJson(path);
    expect(persisted['custom'], {'keep': true});
  });

  test('operation presets reject duplicates and replace preserves id', () {
    final store = SettingsStore(settingsPath());
    final batch = _operation('files.batch_print');
    final first = store.savePreset(
      batch,
      'Office',
      _values('{"copies":1}'),
      replace: false,
    );

    expect(
      () => store.savePreset(
        batch,
        'office',
        _values('{"copies":2}'),
        replace: false,
      ),
      throwsA(
        isA<PresetStoreError>().having(
          (error) => error.kind,
          'kind',
          PresetStoreErrorKind.duplicateName,
        ),
      ),
    );

    final replaced = store.savePreset(
      batch,
      'office',
      _values('{"copies":2}'),
      replace: true,
    );
    expect(replaced.id, first.id);
    expect(replaced.values['copies'], 2);
    expect(store.presetsFor(batch), [replaced]);
  });

  test('operation preset duplicates use Unicode case folding', () {
    final store = SettingsStore(settingsPath());
    final batch = _operation('files.batch_print');
    store.savePreset(batch, 'Straße', _values('{"copies":1}'), replace: false);

    expect(
      () => store.savePreset(
        batch,
        'STRASSE',
        _values('{"copies":2}'),
        replace: false,
      ),
      throwsA(isA<PresetStoreError>()),
    );
  });

  test('operation presets enforce name and capacity limits', () {
    final store = SettingsStore(settingsPath());
    final batch = _operation('files.batch_print');
    for (final invalid in ['   ', 'x' * 65]) {
      expect(
        () => store.savePreset(batch, invalid, _values('{"copies":1}'), replace: false),
        throwsA(
          isA<PresetStoreError>().having(
            (error) => error.kind,
            'kind',
            PresetStoreErrorKind.invalidName,
          ),
        ),
      );
    }
    store.savePreset(
      batch,
      '\u{1F5A8}' * 64,
      _values('{"copies":1}'),
      replace: false,
    );
    for (var index = 1; index < 20; index++) {
      store.savePreset(
        batch,
        'Preset ${index.toString().padLeft(2, '0')}',
        _values('{"copies":1}'),
        replace: false,
      );
    }
    expect(
      () => store.savePreset(batch, 'Overflow', _values('{"copies":1}'), replace: false),
      throwsA(
        isA<PresetStoreError>().having(
          (error) => error.kind,
          'kind',
          PresetStoreErrorKind.limitReached,
        ),
      ),
    );
  });

  test('operation presets list deterministically and delete exact ids', () {
    final store = SettingsStore(settingsPath());
    final batch = _operation('files.batch_print');
    final zulu = store.savePreset(batch, 'zulu', _values('{"copies":1}'), replace: false);
    final alpha = store.savePreset(batch, 'Alpha', _values('{"copies":1}'), replace: false);
    store.savePreset(batch, 'bravo', _values('{"copies":1}'), replace: false);

    expect(
      store.presetsFor(batch).map((preset) => preset.name).toList(),
      ['Alpha', 'bravo', 'zulu'],
    );
    expect(
      () => store.deletePreset(_operation('rfi.process_scans_fast_covers'), alpha.id),
      throwsA(
        isA<PresetStoreError>().having(
          (error) => error.kind,
          'kind',
          PresetStoreErrorKind.missing,
        ),
      ),
    );
    expect(
      () => store.deletePreset(batch, 'stale-id'),
      throwsA(isA<PresetStoreError>()),
    );
    store.deletePreset(batch, zulu.id);
    expect(
      store.presetsFor(batch).map((preset) => preset.name).toList(),
      ['Alpha', 'bravo'],
    );
  });

  test('operation presets drop only malformed stored entries', () {
    final path = settingsPath();
    final validId = '0f6f1c3e-9a1b-4c2d-8e3f-112233445566';
    File(path).writeAsStringSync(
      jsonEncode({
        'operationPresets': {
          'files.batch_print': [
            {
              'id': validId,
              'name': 'Valid',
              'values': {
                'copies': 2,
                'files': {'selectionId': 'forbidden'},
                'orientation': 'diagonal',
                'printer': 'bad\nprinter',
              },
            },
            {
              'id': '7d8a2b1c-0000-4000-8000-000000000001',
              'name': 'Missing values',
            },
            {'id': 'not-a-uuid', 'name': 'Bad ID', 'values': {}},
            {
              'id': '7d8a2b1c-0000-4000-8000-000000000002',
              'name': '',
              'values': {},
            },
            42,
          ],
        },
        'custom': {'keep': true},
      }),
    );

    final presets = SettingsStore(
      path,
    ).presetsFor(_operation('files.batch_print'));
    expect(presets, hasLength(1));
    expect(presets[0].id, validId);
    expect(presets[0].name, 'Valid');
    expect(presets[0].values, {'copies': 2});

    final persisted = readJson(path);
    expect(
      (persisted['operationPresets'] as Map)['files.batch_print'],
      [
        {'id': validId, 'name': 'Valid', 'values': {'copies': 2}},
      ],
    );
    expect(persisted['custom'], {'keep': true});
  });

  test('operation presets scrub write failure preserves memory and disk', () {
    final path = settingsPath();
    final original = jsonEncode({
      'operationPresets': {
        'files.batch_print': [
          {
            'id': '0f6f1c3e-9a1b-4c2d-8e3f-112233445566',
            'name': 'Unsafe',
            'values': {
              'copies': 2,
              'files': {'selectionId': 'forbidden'},
            },
          },
        ],
      },
      'custom': {'keep': true},
    });
    File(path).writeAsStringSync(original);
    // The stored preset values are sanitized on read, so the scrub-on-read
    // rewrite is the step that fails here.
    expect(
      () => SettingsStore(
        path,
        writer: (_, _) => throw const FileSystemException('injected'),
      ).presetsFor(_operation('files.batch_print')),
      throwsA(isA<FileSystemException>()),
    );
    expect(File(path).readAsStringSync(), original);
  });

  test('operation presets keep a temporarily unavailable printer', () {
    final path = settingsPath();
    const presetId = '0f6f1c3e-9a1b-4c2d-8e3f-112233445566';
    File(path).writeAsStringSync(
      jsonEncode({
        'operationPresets': {
          'files.batch_print': [
            {
              'id': presetId,
              'name': 'Offline queue',
              'values': {
                'printer': 'Temporarily Disconnected Printer',
                'orientation': 'landscape',
              },
            },
          ],
        },
      }),
    );

    final presets = SettingsStore(
      path,
    ).presetsFor(_operation('files.batch_print'));
    expect(presets[0].values['printer'], 'Temporarily Disconnected Printer');
    final persisted = readJson(path);
    expect(
      ((persisted['operationPresets'] as Map)['files.batch_print'] as List)
          .first,
      containsPair(
        'values',
        containsPair('printer', 'Temporarily Disconnected Printer'),
      ),
    );
  });

  test('operation preset failed write rolls back memory and disk', () {
    final path = settingsPath();
    const original = '{"custom":{"keep":true}}';
    File(path).writeAsStringSync(original);
    final store = SettingsStore(
      path,
      writer: (_, _) => throw const FileSystemException('injected'),
    );

    expect(
      () => store.savePreset(
        _operation('files.batch_print'),
        'Office',
        _values('{"copies":2}'),
        replace: false,
      ),
      throwsA(isA<FileSystemException>()),
    );
    expect(store.presetsFor(_operation('files.batch_print')), isEmpty);
    expect(File(path).readAsStringSync(), original);
  });

  test('atomic write accepts verified parent sync failure after persist', () {
    final path = settingsPath();
    File(path).writeAsStringSync('{"before":true}');
    const next = <String, Object?>{
      'after': true,
      'nested': {'value': 2},
    };

    atomicWriteWithParentSync(path, next, (_) {
      throw const OSError('injected parent sync failure');
    });

    expect(File(path).readAsStringSync(), canonicalJson(next));
  });

  test('atomic write fails closed when parent sync failure readback mismatches', () {
    final path = settingsPath();
    File(path).writeAsStringSync('{"before":true}');
    const next = <String, Object?>{'after': true};

    expect(
      () => atomicWriteWithParentSync(path, next, (_) {
        File(path).writeAsStringSync('{"unexpected":true}');
        throw const OSError('injected parent sync failure');
      }),
      throwsA(isA<SettingsIoException>()),
    );
    expect(File(path).readAsStringSync(), isNot(canonicalJson(next)));
  });

  test('settings round trip is atomic, compatible and filters operation params', () {
    final path = settingsPath();
    File(path).writeAsStringSync(
      jsonEncode({
        'prefs': {'locale': 'az', 'fontScale': 1.25},
        'session': {
          'accessToken': 'legacy-access',
          'refreshToken': 'legacy-refresh',
        },
        'custom': [1, 2, 3],
        'params': {
          'other.operation': {'keep': 'value'},
        },
      }),
    );

    final store = SettingsStore(path);
    store.saveParams(
      'rfi.process_scans_fast_covers',
      _values('''{
        "scan_mode": "multi_rfi",
        "source": "/in",
        "dest": "/out",
        "keep_original": true,
        "nullable": null,
        "unsafe_list": [1, 2, 3],
        "unsafe_object": {"path": "/forged"}
      }'''),
    );

    final restored = SettingsStore(path);
    expect(
      restored.paramsFor('rfi.process_scans_fast_covers'),
      _values('''{
        "scan_mode": "multi_rfi",
        "source": "/in",
        "dest": "/out",
        "keep_original": true,
        "nullable": null
      }'''),
    );
    expect(restored.paramsFor('other.operation'), {'keep': 'value'});

    final encoded = File(path).readAsStringSync();
    final persisted = jsonDecode(encoded) as Map<String, Object?>;
    expect((persisted['prefs'] as Map)['locale'], 'az');
    expect((persisted['session'] as Map)['accessToken'], 'legacy-access');
    expect(persisted['custom'], [1, 2, 3]);
    expect(encoded.contains('unsafe_list'), isFalse);
    expect(encoded.contains('unsafe_object'), isFalse);
    expect(directory.listSync().length, 1);
  });

  test('settings file keeps sorted compact keys like serde_json', () {
    final path = settingsPath();
    final store = SettingsStore(path);
    store.saveSection('zeta', {'b': 1, 'a': 2});
    store.saveSection('alpha', {'y': true});
    expect(
      File(path).readAsStringSync(),
      '{"alpha":{"y":true},"zeta":{"a":2,"b":1}}',
    );
  });

  test('preferences validate clamp round trip and reset compatibly', () {
    final path = settingsPath();
    File(path).writeAsStringSync(
      jsonEncode({
        'prefs': {
          'locale': 42,
          'theme': 'neon',
          'font': 'comic',
          'fontScale': 'large',
          'compact': 'yes',
        },
      }),
    );
    final store = SettingsStore(path);
    expect(store.preferences(), const AppPrefs());

    store.savePreferences(
      const AppPrefs(
        locale: 'az',
        theme: 'dark',
        font: 'mono',
        fontScale: 9.0,
        compact: true,
        startWithWindows: true,
        replicaSyncEnabled: true,
      ),
    );
    expect(
      SettingsStore(path).preferences(),
      const AppPrefs(
        locale: 'az',
        theme: 'dark',
        font: 'mono',
        fontScale: 1.6,
        compact: true,
        startWithWindows: true,
        replicaSyncEnabled: true,
      ),
    );

    store.savePreferences(const AppPrefs());
    final reset = readJson(path);
    expect((reset['prefs'] as Map)['locale'], isNull);
  });

  test('legacy operation secrets are scrubbed from memory and disk on load', () {
    final path = settingsPath();
    File(path).writeAsStringSync(
      jsonEncode({
        'params': {
          'rfi.log_add': {
            'email_provider': 'gemini',
            'gemini_api_key': 'legacy-secret',
          },
        },
        'custom': {'keep': true},
      }),
    );

    final store = SettingsStore(path);
    expect(store.paramsFor('rfi.log_add'), {'email_provider': 'gemini'});
    final encoded = File(path).readAsStringSync();
    expect(encoded.contains('gemini_api_key'), isFalse);
    expect(encoded.contains('legacy-secret'), isFalse);
    expect(encoded.contains('"custom"'), isTrue);
  });

  test('startup surfaces an injected legacy secret scrub write failure', () {
    final path = settingsPath();
    final original = jsonEncode({
      'params': {
        'rfi.log_add': {
          'email_provider': 'gemini',
          'gemini_api_key': 'legacy-plaintext-secret',
        },
      },
      'custom': {'keep': true},
    });
    File(path).writeAsStringSync(original);

    expect(
      () => SettingsStore(
        path,
        writer: (_, _) => throw const FileSystemException('injected'),
      ),
      throwsA(isA<FileSystemException>()),
    );
    expect(File(path).readAsStringSync(), original);
  });

  test('session metadata strips tokens and legacy strip matches exactly', () {
    final path = settingsPath();
    File(path).writeAsStringSync(
      jsonEncode({
        'session': {'accessToken': 'a1', 'refreshToken': 'r1'},
      }),
    );
    final store = SettingsStore(path);
    expect(
      store.stripLegacySessionTokens(
        StoredSession('a1', 'other', 'user', 'u@example.com', 10),
      ),
      isFalse,
    );
    expect(
      store.stripLegacySessionTokens(
        StoredSession('a1', 'r1', 'user', 'u@example.com', 10),
      ),
      isTrue,
    );
    expect(store.section('session'), <String, Object?>{});

    store.saveSessionMetadata('user-1', 'u@example.com', 99);
    expect(store.section('session'), {
      'userId': 'user-1',
      'email': 'u@example.com',
      'expiresAt': 99,
    });
  });

  test('a corrupt settings file loads as empty and is replaced on write', () {
    final path = settingsPath();
    File(path).writeAsStringSync('{not json');
    final store = SettingsStore(path);
    expect(store.section('prefs'), isNull);
    store.saveSection('prefs', const AppPrefs().toJson());
    expect(readJson(path)['prefs'], isA<Map<String, Object?>>());
  });
}
