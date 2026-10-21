// Port of naming behaviour exercised by src-tauri/src/naming.rs and the
// slice5 naming oracle in process_scans.rs.
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;
import 'package:docflow/engine/naming.dart';

void main() {
  test('RFI filenames, folders and targets follow the frozen layout', () {
    const code = 'AZE-SUN-INT-QMS-RFI-CIV-0064';
    expect(filenameFor(code), 'RFI No. AZE-SUN-INT-QMS-RFI-CIV-0064.pdf');
    expect(folderFor('/out', code), p.join('/out', code));
    expect(
      targetFor('/out', code),
      p.join('/out', code, 'RFI No. AZE-SUN-INT-QMS-RFI-CIV-0064.pdf'),
    );
    expect(
      flatTargetFor('/out', code),
      p.join('/out', 'RFI No. AZE-SUN-INT-QMS-RFI-CIV-0064.pdf'),
    );
  });

  test('slice5 naming cases match the shared oracle fixture', () {
    final fixture = jsonDecode(
      File('test/fixtures/slice5_rfi_naming.json')
          .readAsStringSync(),
    ) as Map<String, Object?>;
    const root = 'fixture-root';
    for (final raw in fixture['namingCases']! as List) {
      final tc = raw! as Map<String, Object?>;
      final code = tc['code']! as String;
      expect(filenameFor(code), tc['filename']);
      expect(folderFor(root, code), p.join(root, tc['folder']! as String));
    }
  });
}
