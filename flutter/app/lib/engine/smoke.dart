// Port of src-tauri/src/smoke.rs: `--package-smoke <output|-> [--skip-ocr]`.
//
// The bootstrap and settings round trip run in an isolated temporary directory.
// The OCR and fast-scan apply/undo steps need the OCR, process-scan, and journal
// modules. Those are not wired here, so without `--skip-ocr` the smoke reports
// the missing step and exits non-zero instead of claiming success.

import 'dart:convert';
import 'dart:io';

import 'package:path/path.dart' as p;

import 'backend.dart';
import 'contracts.dart';
import 'registry.dart';
import 'settings.dart';

const int _expectedOperationCount = 10;

/// Runs the package smoke. [arguments] excludes the executable name and starts
/// with `--package-smoke`. Returns the process exit status.
Future<int> runPackageSmoke(List<String> arguments) async {
  final tokens = arguments.skip(1).toList();
  if (tokens.isEmpty) {
    stderr.writeln('package smoke requires an output path or \'-\'');
    return 2;
  }
  final output = tokens.first;
  final skipOcr = tokens.contains('--skip-ocr');
  final report = await _run(skipOcr: skipOcr);
  final encoded = const JsonEncoder.withIndent('  ').convert(report);
  if (output == '-') {
    stdout.writeln(encoded);
  } else {
    try {
      _writeNew(output, utf8.encode(encoded));
    } on FileSystemException catch (error) {
      stderr.writeln('package smoke report write failed: ${error.message}');
      return 1;
    }
  }
  return report['ok'] == true ? 0 : 1;
}

Future<Map<String, Object?>> _run({required bool skipOcr}) async {
  final root = Directory.systemTemp.createTempSync('docflow-smoke');
  try {
    final bootstrapStart = Stopwatch()..start();
    final settingsStart = Stopwatch()..start();
    final settings = SettingsStore(p.join(root.path, 'settings.json'));
    settings.savePreferences(
      const AppPrefs(
        locale: 'az',
        theme: 'dark',
        font: 'sans',
        fontScale: 1.2,
        compact: true,
      ),
    );
    settings.saveParams(fastCoverOperationId, {
      'keep_original': true,
      'scan_mode': 'multi_rfi',
      'organize_into_rfi_folders': false,
      'correct_orientation': false,
      'nested': {'mustNotPersist': true},
    });
    if (settings.preferences().locale != 'az' ||
        settings.preferences().fontScale != 1.2) {
      throw StateError('settings round trip changed preferences');
    }
    if (settings.paramsFor(fastCoverOperationId).containsKey('nested')) {
      throw StateError('settings persisted a non-scalar operation value');
    }
    final settingsMillis = settingsStart.elapsedMilliseconds;
    final operations = operationRegistry(const RegistryFacts());
    if (operations.length != _expectedOperationCount) {
      throw StateError('bootstrap did not expose all ten operations');
    }
    final bootstrapMillis = bootstrapStart.elapsedMilliseconds;
    final report = <String, Object?>{
      'ok': true,
      'version': docflowVersion,
      'operationCount': operations.length,
      'bootstrapMillis': bootstrapMillis,
      'settingsMillis': settingsMillis,
      'firstOcrMillis': null,
      'operationMillis': null,
      'ocrCode': null,
      'ocrSource': null,
      'appliedSteps': null,
      'undoneSteps': null,
    };
    if (!skipOcr) {
      report
        ..['ok'] = false
        ..['error'] =
            'OCR and fast-scan smoke steps need the OCR, process-scan, and '
            'journal engine modules, which are not wired in this build; rerun '
            'with --skip-ocr to check bootstrap and settings only';
    }
    return report;
  } on Object catch (error) {
    return <String, Object?>{'ok': false, 'error': error.toString()};
  } finally {
    root.deleteSync(recursive: true);
  }
}

void _writeNew(String path, List<int> content) {
  final file = File(path);
  file.parent.createSync(recursive: true);
  final handle = file.openSync(mode: FileMode.writeOnlyAppend);
  try {
    if (handle.lengthSync() != 0) {
      throw const FileSystemException('output already exists');
    }
    handle
      ..writeFromSync(content)
      ..flushSync();
  } finally {
    handle.closeSync();
  }
}
