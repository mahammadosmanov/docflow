import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:docflow/engine/cap_fs.dart';
import 'package:docflow/engine/errors.dart';
import 'package:docflow/engine/fs_ops.dart';
import 'package:docflow/engine/journal.dart' show FsErrorKind;

void main() {
  late Directory dir;
  setUp(() {
    dir = Directory.systemTemp.createTempSync('e2ops');
  });
  tearDown(() => dir.deleteSync(recursive: true));

  test('copyNoClobber copies bytes and refuses an existing destination', () {
    File('${dir.path}/src.bin').writeAsBytesSync([1, 2, 3]);
    copyNoClobber('${dir.path}/src.bin', '${dir.path}/dst.bin');
    expect(File('${dir.path}/dst.bin').readAsBytesSync(), [1, 2, 3]);
    File('${dir.path}/exists.bin').writeAsBytesSync([9]);
    expect(
      () => copyNoClobber('${dir.path}/src.bin', '${dir.path}/exists.bin'),
      throwsA(anything),
    );
    expect(File('${dir.path}/exists.bin').readAsBytesSync(), [9]);
  });

  test('writeNoClobberPath writes once and fileSha256 matches content', () {
    writeNoClobberPath('${dir.path}/w.txt', 'abc'.codeUnits);
    expect(
      fileSha256('${dir.path}/w.txt'),
      'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad',
    );
    expect(
      () => writeNoClobberPath('${dir.path}/w.txt', 'x'.codeUnits),
      throwsA(anything),
    );
    expect(File('${dir.path}/w.txt').readAsStringSync(), 'abc');
  });

  test('moveNoClobber moves a file when the destination is free', () {
    File('${dir.path}/m1.txt').writeAsStringSync('m');
    moveNoClobber('${dir.path}/m1.txt', '${dir.path}/m2.txt');
    expect(File('${dir.path}/m1.txt').existsSync(), isFalse);
    expect(File('${dir.path}/m2.txt').readAsStringSync(), 'm');
  });

  test('IoError maps errno to the journal error kind', () {
    final error = IoError.fromErrno(2);
    expect(error.kind, FsErrorKind.notFound);
    expect(error.osError, 2);
  });
  test('BackendError is unaffected by the port types', () {
    expect(BackendError.cancelled().code, isNotEmpty);
  });
}
