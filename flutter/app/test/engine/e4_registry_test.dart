// Port of registry.rs tests and the protocol1 operation oracle replay.
import 'dart:convert';
import 'dart:io';

import 'package:docflow/engine/contracts.dart';
import 'package:docflow/engine/registry.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

const String _fixtures = 'test/fixtures';

Object? _loadFixture(String name) =>
    jsonDecode(File(p.join(_fixtures, name)).readAsStringSync());

OperationDef _find(List<OperationDef> operations, String id) =>
    operations.firstWhere((operation) => operation.id == id);

void main() {
  test('registry exposes only the Fast RFI Scan among process scans', () {
    final operations = operationRegistry(const RegistryFacts());
    final scanIds = operations
        .where((operation) => operation.id.startsWith('rfi.process_scans'))
        .map((operation) => operation.id)
        .toList();
    expect(scanIds, ['rfi.process_scans_fast_covers']);
  });

  test('fast scan mode defaults to multi RFI and offers single RFI', () {
    final scan = _find(
      operationRegistry(const RegistryFacts()),
      fastCoverOperationId,
    );
    final mode = scan.schema.params.firstWhere(
      (parameter) => parameter.name == 'scan_mode',
    );
    expect(mode.kind, ParamKind.choice);
    expect(mode.defaultValue, 'multi_rfi');
    expect(mode.choices, ['multi_rfi', 'single_rfi']);
    final orientation = scan.schema.params.firstWhere(
      (parameter) => parameter.name == 'correct_orientation',
    );
    expect(orientation.visibleWhen['scan_mode'], 'multi_rfi');
  });

  test('param schema validation matches the frozen required value rules', () {
    final operations = operationRegistry(const RegistryFacts());
    final rfiLog = operations[1];
    expect(rfiLog.id, 'rfi.log_add');
    expect(
      rfiLog.schema.validate({
        'workbook': '/tmp/RFI Log.xlsx',
        'email_provider': 'offline',
        'sheet_counts': {'CIV': 0, 'MEC': 0, 'ELE': 0, 'PRC': 0},
      }),
      ['error.param.required.sheet_counts'],
    );
  });

  test(
    'concrete documents require log and output but not follow-up by default',
    () {
      final operations = operationRegistry(const RegistryFacts());
      expect(operations.length, 10);
      final operation = _find(operations, 'rfi.generate_concrete_documents');
      expect(operation.schema.validate(const {}), [
        'error.param.required.rfi_log_workbook',
        'error.param.required.output_directory',
      ]);
      final followUp = operation.schema.params.firstWhere(
        (parameter) => parameter.name == 'follow_up_workbook',
      );
      expect(followUp.required, isFalse);
    },
  );

  test('batch print is the only preset-capable operation and forgets files', () {
    final operations = operationRegistry(
      const RegistryFacts(
        printers: ['Office Printer'],
        defaultPrinter: 'Office Printer',
      ),
    );
    final presets = operations
        .where((operation) => operation.supportsPresets)
        .map((operation) => operation.id)
        .toList();
    expect(presets, ['files.batch_print']);
    final print = _find(operations, 'files.batch_print');
    final files = print.schema.params.firstWhere(
      (parameter) => parameter.name == 'files',
    );
    expect(files.remember, isFalse);
    final printer = print.schema.params.firstWhere(
      (parameter) => parameter.name == 'printer',
    );
    expect(printer.defaultValue, 'Office Printer');
    expect(printer.choices, ['Office Printer']);
  });

  test('printer default falls back to the first installed printer', () {
    final operations = operationRegistry(
      const RegistryFacts(printers: ['A', 'B']),
    );
    final printer = _find(operations, 'files.batch_print').schema.params
        .firstWhere((parameter) => parameter.name == 'printer');
    expect(printer.defaultValue, 'A');
    final none = _find(
      operationRegistry(const RegistryFacts()),
      'files.batch_print',
    ).schema.params.firstWhere((parameter) => parameter.name == 'printer');
    expect(none.defaultValue, isNull);
  });

  test('log workbook default comes from the registry facts', () {
    final operations = operationRegistry(
      const RegistryFacts(defaultWorkbook: '/home/a/RFI Log.xlsx'),
    );
    final workbook = _find(operations, 'rfi.log_add').schema.params.firstWhere(
      (parameter) => parameter.name == 'workbook',
    );
    expect(workbook.defaultValue, '/home/a/RFI Log.xlsx');
  });

  // protocol1_operations.json is an additive snapshot that predates the
  // transient batch-print parameters and rfi.generate_concrete_documents. No
  // Rust test reads it, so the registry.rs source stays authoritative here. The
  // snapshot must still agree with every parameter it records.
  test('registry agrees with every parameter in the protocol1 snapshot', () {
    final snapshot = _loadFixture('protocol1_operations.json')! as List;
    // The snapshot was recorded on a host with no printers and no workbook.
    final operations = operationRegistry(const RegistryFacts());
    for (final raw in snapshot) {
      final expected = raw! as Map<String, Object?>;
      final id = expected['id']! as String;
      final operation = _find(operations, id);
      expect(operation.toJson()['titleKey'], expected['titleKey'], reason: id);
      expect(
        operation.toJson()['descriptionKey'],
        expected['descriptionKey'],
        reason: id,
      );
      final recorded = (expected['schema']! as List)
          .cast<Map<String, Object?>>();
      final registryParams = operation.toJson()['schema']! as List;
      for (final param in recorded) {
        final name = param['name'];
        final match = registryParams
            .cast<Map<String, Object?>>()
            .firstWhere((item) => item['name'] == name);
        expect(match, param, reason: '$id.$name');
      }
      final recordedNames = recorded.map((param) => param['name']).toSet();
      for (final param in registryParams.cast<Map<String, Object?>>()) {
        if (recordedNames.contains(param['name'])) {
          continue;
        }
        expect(
          param['remember'],
          false,
          reason: '$id.${param['name']} is not in the snapshot and must be transient',
        );
      }
    }
  });
}
