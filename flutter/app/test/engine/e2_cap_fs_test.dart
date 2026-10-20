import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:docflow/engine/cap_fs.dart';
import 'package:docflow/engine/journal.dart' show sha256Hex;

void main() {
  late Directory dir;
  setUp(() {
    dir = Directory.systemTemp.createTempSync('e2cap');
  });
  tearDown(() => dir.deleteSync(recursive: true));

  test('create-new secured file never clobbers an existing name', () {
    final root = CapabilityRoot.open(dir.path);
    File('${dir.path}/a.txt').writeAsStringSync('keep');
    final anchor = root.authorize('${dir.path}/a.txt').anchor();
    expect(() => anchor.createNew(), throwsA(isA<IoError>()));
    expect(File('${dir.path}/a.txt').readAsStringSync(), 'keep');
  });

  test('writeNoClobber creates new content and sha256 matches', () {
    final root = CapabilityRoot.open(dir.path);
    final anchor = root.authorize('${dir.path}/b.txt').anchor();
    anchor.writeNoClobber('hello'.codeUnits);
    expect(File('${dir.path}/b.txt').readAsStringSync(), 'hello');
    expect(
      anchor.sha256(),
      sha256Hex('hello'.codeUnits),
    );
  });

  test('traversal and parent components are rejected before any effect', () {
    final root = CapabilityRoot.open(dir.path);
    expect(
      () => root.authorize('${dir.path}/child/../../outside/secret.txt'),
      throwsA(anything),
    );
  });

  test('symlink leaf is not followed for reads', () {
    File('${dir.path}/target.txt').writeAsStringSync('secret');
    Link('${dir.path}/link.txt').createSync('${dir.path}/target.txt');
    final root = CapabilityRoot.open(dir.path);
    final anchor = root.authorize('${dir.path}/link.txt').anchor();
    expect(anchor.canOpenRead(), isFalse);
  });

  test('retain then restore moves the canonical name without clobbering', () {
    final root = CapabilityRoot.open(dir.path);
    File('${dir.path}/c.txt').writeAsStringSync('orig');
    final canonical = root.authorize('${dir.path}/c.txt').anchor();
    final retained = root.authorize('${dir.path}/c.retained.txt').anchor();
    canonical.retainCurrentNameNoClobber(retained);
    expect(File('${dir.path}/c.txt').existsSync(), isFalse);
    expect(File('${dir.path}/c.retained.txt').readAsStringSync(), 'orig');
    retained.restoreOriginalNameNoClobber(canonical);
    expect(File('${dir.path}/c.txt').readAsStringSync(), 'orig');
  });

  test('moveNoClobber refuses to replace an existing destination', () {
    final root = CapabilityRoot.open(dir.path);
    File('${dir.path}/s.txt').writeAsStringSync('src');
    File('${dir.path}/d.txt').writeAsStringSync('dst');
    final source = root.authorize('${dir.path}/s.txt').anchor();
    final dest = root.authorize('${dir.path}/d.txt').anchor();
    expect(() => source.moveNoClobberTo(dest), throwsA(isA<IoError>()));
    expect(File('${dir.path}/d.txt').readAsStringSync(), 'dst');
    expect(File('${dir.path}/s.txt').readAsStringSync(), 'src');
  });
}
