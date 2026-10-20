// Port of the inline tests in src-tauri/src/scan_index.rs.
import 'dart:io';

import 'package:docflow/engine/scan_index.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

String _decodeValue(Object? json) => (json as Map<String, Object?>)['value']! as String;

void main() {
  test('completed items survive restart and changed files miss', () {
    final fixture = Directory.systemTemp.createTempSync('e3-scan-');
    addTearDown(() => fixture.deleteSync(recursive: true));
    final db = p.join(fixture.path, 'scan-index.sqlite3');
    final root = p.join(fixture.path, 'root');
    Directory(p.join(root, 'MEC')).createSync(recursive: true);
    final file = File(p.join(root, 'MEC', 'item.pdf'))..writeAsBytesSync([1, 2, 3]);
    final fingerprint = FileFingerprint.read(file.path);
    {
      final index = ScanIndex.open(db);
      final (run, prior) = index.beginRun(
        'owner',
        'operation',
        'profile-v1',
        root,
        forceRescan: false,
      );
      expect(prior, 0);
      run.recordSuccess('MEC/item.pdf', fingerprint, {'value': 'ok'});
      run.complete(const ScanIndexStats(discovered: 1, analyzed: 1));
      index.close();
    }

    final index = ScanIndex.open(db);
    final (run, _) = index.beginRun(
      'owner',
      'operation',
      'profile-v1',
      root,
      forceRescan: false,
    );
    expect(run.lookup('MEC/item.pdf', fingerprint, _decodeValue), 'ok');

    file.writeAsBytesSync([9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9]);
    final changed = FileFingerprint.read(file.path);
    expect(run.lookup('MEC/item.pdf', changed, _decodeValue), isNull);
    index.close();
  });

  test('a new run marks an abandoned run interrupted', () {
    final fixture = Directory.systemTemp.createTempSync('e3-scan-');
    addTearDown(() => fixture.deleteSync(recursive: true));
    final index = ScanIndex.open(p.join(fixture.path, 'scan-index.sqlite3'));
    final root = fixture.path;
    final (abandoned, prior) = index.beginRun(
      'owner',
      'operation',
      'profile-v1',
      root,
      forceRescan: false,
    );
    expect(prior, 0);
    abandoned.abandon();
    final (_, replacementPrior) = index.beginRun(
      'owner',
      'operation',
      'profile-v1',
      root,
      forceRescan: false,
    );
    expect(replacementPrior, 1);
    index.close();
  });
}
