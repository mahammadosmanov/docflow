import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:docflow/engine/cap_fs.dart';
import 'package:docflow/engine/journal.dart';
import 'package:docflow/engine/pdf_publish.dart';

void main() {
  test('publish retries transient access errors without overwriting', () {
    final dir = Directory.systemTemp.createTempSync('e2pdf');
    addTearDown(() => dir.deleteSync(recursive: true));
    final temp = '${dir.path}/.review.pdf.docflow-test.tmp';
    final dest = '${dir.path}/review.pdf';
    File(temp).writeAsBytesSync([1, 2, 3]);
    var calls = 0;
    publishPdfNoClobberWith(
      temporary: temp,
      destination: dest,
      attempts: 4,
      link: (source, target) {
        calls++;
        if (calls < 3) {
          throw IoError.fromErrno(13);
        }
        File(source).copySync(target);
      },
      wait: (_) {},
    );
    expect(calls, 3);
    expect(File(dest).readAsBytesSync(), [1, 2, 3]);
    expect(File(temp).existsSync(), isFalse);
  });

  test('publish falls back to exclusive copy after persistent access errors', () {
    final dir = Directory.systemTemp.createTempSync('e2pdf');
    addTearDown(() => dir.deleteSync(recursive: true));
    final temp = '${dir.path}/.review.pdf.docflow-test.tmp';
    final dest = '${dir.path}/review.pdf';
    File(temp).writeAsBytesSync([9, 8]);
    var calls = 0;
    publishPdfNoClobberWith(
      temporary: temp,
      destination: dest,
      attempts: 3,
      link: (_, _) {
        calls++;
        throw IoError.fromErrno(13);
      },
      wait: (_) {},
    );
    expect(calls, 3);
    expect(File(dest).readAsBytesSync(), [9, 8]);
    expect(File(temp).existsSync(), isFalse);
  });

  test('publish never overwrites an existing destination', () {
    final dir = Directory.systemTemp.createTempSync('e2pdf');
    addTearDown(() => dir.deleteSync(recursive: true));
    final temp = '${dir.path}/.review.pdf.docflow-test.tmp';
    final dest = '${dir.path}/review.pdf';
    File(temp).writeAsBytesSync([1]);
    File(dest).writeAsBytesSync([7, 7]);
    expect(
      () => publishPdfNoClobberWith(
        temporary: temp,
        destination: dest,
        attempts: 3,
        link: (s, t) {
          if (File(t).existsSync()) {
            throw const FsIoException(FsErrorKind.alreadyExists, 'exists');
          }
          File(s).copySync(t);
        },
        wait: (_) {},
      ),
      throwsA(isA<FsIoException>()),
    );
    expect(File(dest).readAsBytesSync(), [7, 7]);
    expect(File(temp).readAsBytesSync(), [1]);
  });

  test('temporary PDF name contract matches only its destination', () {
    expect(
      temporaryPdfNameMatches('.review.pdf.docflow-abc.tmp', 'review.pdf'),
      isTrue,
    );
    expect(
      temporaryPdfNameMatches('.other.pdf.docflow-abc.tmp', 'review.pdf'),
      isFalse,
    );
  });
}
