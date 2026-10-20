// Replays the shared protocol oracles from src-tauri/tests/fixtures:
// - slice5_rfi_naming.json (rfi_code + naming, as in process_scans.rs)
// - protocol1_operations.json (contracts round trip)
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:docflow/engine/contracts.dart';
import 'package:docflow/engine/naming.dart';
import 'package:docflow/engine/rfi_code.dart' as rfi;
import 'package:path/path.dart' as p;

const String _fixtures = '../../src-tauri/tests/fixtures';

Object? _loadFixture(String name) =>
    jsonDecode(File(p.join(_fixtures, name)).readAsStringSync());

/// Wire names used by the Rust oracle (`RfiMethod` mapping in process_scans.rs).
String _methodWire(rfi.RfiMethod method) => switch (method) {
  rfi.RfiMethod.strict => 'strict',
  rfi.RfiMethod.repaired => 'repaired',
  rfi.RfiMethod.ocrAlias => 'ocr_alias',
  rfi.RfiMethod.fuzzy => 'fuzzy',
  rfi.RfiMethod.miss => 'miss',
};

void main() {
  test('slice5 shared RFI and naming fixture matches the frozen oracle', () {
    final fixture =
        _loadFixture('slice5_rfi_naming.json')! as Map<String, Object?>;
    for (final raw in fixture['rfiCases']! as List) {
      final tc = raw! as Map<String, Object?>;
      final name = tc['name']! as String;
      final matched = rfi.find(tc['input']! as String);
      expect(matched.code, tc['code'], reason: name);
      expect(matched.discipline, tc['discipline'], reason: name);
      expect(matched.number, tc['number'], reason: name);
      expect(_methodWire(matched.method), tc['method'], reason: name);
      expect(
        matched.confidence,
        (tc['confidence']! as num).toDouble(),
        reason: name,
      );
    }

    const root = 'fixture-root';
    for (final raw in fixture['namingCases']! as List) {
      final tc = raw! as Map<String, Object?>;
      final code = tc['code']! as String;
      expect(filenameFor(code), tc['filename']);
      expect(folderFor(root, code), p.join(root, tc['folder']! as String));
    }
  });

  test(
    'protocol1 operation snapshot round-trips through contracts unchanged',
    () {
      final operations = _loadFixture('protocol1_operations.json')! as List;
      expect(operations, isNotEmpty);
      for (final raw in operations) {
        final json = raw! as Map<String, Object?>;
        final parsed = OperationDef.fromJson(json);
        expect(parsed.toJson(), json, reason: json['id'] as String?);
      }
    },
  );
}
