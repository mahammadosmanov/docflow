// Port of the smoke.rs bootstrap test. The OCR step is not wired, so it is
// covered by the skip flag only.
import 'dart:convert';
import 'dart:io';

import 'package:docflow/engine/smoke.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

void main() {
  test('package smoke bootstrap and settings are isolated', () async {
    final directory = Directory.systemTemp.createTempSync('docflow-smoke-test');
    addTearDown(() => directory.deleteSync(recursive: true));
    final output = p.join(directory.path, 'report.json');

    final status = await runPackageSmoke([
      '--package-smoke',
      output,
      '--skip-ocr',
    ]);

    expect(status, 0);
    final report = jsonDecode(File(output).readAsStringSync()) as Map;
    expect(report['ok'], true);
    expect(report['operationCount'], 10);
    expect(report['ocrCode'], isNull);
  });

  test('package smoke without skip-ocr reports the unwired OCR step', () async {
    final directory = Directory.systemTemp.createTempSync('docflow-smoke-ocr');
    addTearDown(() => directory.deleteSync(recursive: true));
    final output = p.join(directory.path, 'report.json');

    final status = await runPackageSmoke(['--package-smoke', output]);

    expect(status, 1);
    final report = jsonDecode(File(output).readAsStringSync()) as Map;
    expect(report['ok'], false);
    expect(report['error'] as String, contains('not wired'));
  });

  test('package smoke requires an output argument', () async {
    expect(await runPackageSmoke(['--package-smoke']), 2);
  });
}
