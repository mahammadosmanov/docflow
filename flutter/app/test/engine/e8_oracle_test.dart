// Port of the compatibility-oracle tests in src-tauri/tests/rfi_follow_up_native.rs
// (preview_matches_the_frozen_follow_up_compatibility_oracle and
// compatibility_oracle_matches_exact_rows). The oracle is replayed from the
// Rust fixture at src-tauri/tests/fixtures/rfi_follow_up_compatibility_oracle.json.
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';

import 'e8_full_schema_support.dart';

const String _oraclePath =
    'test/fixtures/rfi_follow_up_compatibility_oracle.json';

Map<String, Object?> _loadOracle() =>
    jsonDecode(File(_oraclePath).readAsStringSync()) as Map<String, Object?>;

Uint8List compatibilitySource() {
  const entries = <(int, String, String?)>[
    (10, '0001', 'Completed'),
    (11, '0002', 'Ongoing'),
    (12, '0003', 'Completed'),
    (13, '0004', null),
    (14, '0005', 'Cancelled'),
    (15, '0006', 'Completed'),
    (16, '0007', 'Completed'),
    (18, '0009', 'Completed'),
    (19, '0010', 'Completed'),
    (20, '0011', 'Completed'),
    (21, '0012', 'Completed'),
    (22, '0013', 'Completed'),
    (23, '0014', 'Completed'),
    (24, '0015', 'Completed'),
    (25, '0016', 'Completed'),
    (26, '0017', 'Completed'),
  ];
  final civRows = <RowSpec>[
    for (final (physicalRow, suffix, status) in entries)
      row(physicalRow, [
        inline('A$physicalRow', 'AZE-SUN-INT-QMS-RFI-CIV-$suffix'),
        status == null
            ? blank('T$physicalRow')
            : inline('T$physicalRow', status),
      ]),
    row(17, [
      inline('A17', 'AZE-SUN-INT-QMS-RFI-CIV-0008'),
      formula('T17', 'LOOKUP()', 'Completed'),
    ]),
  ]..sort((a, b) => a.number.compareTo(b.number));
  return sourcePackage(
    worksheet(civRows, const ['T18:U18']),
    worksheet([
      row(10, [
        inline('A10', 'AZE-SUN-INT-QMS-RFI-CIV-0006'),
        inline('U10', 'Completed'),
      ]),
      row(11, [
        inline('A11', 'AZE-SUN-INT-QMS-RFI-CIV-0007'),
        inline('U11', 'Ongoing'),
      ]),
    ]),
  );
}

Uint8List compatibilityTarget() {
  String rfi(String suffix) => 'AZE-SUN-INT-QMS-RFI-CIV-$suffix';
  return targetWithRows([
    row(2, [inline('A2', rfi('0001')), blank('B2')]),
    row(3, [inline('A3', rfi('0002')), blank('B3')]),
    row(4, [inline('A4', rfi('0003')), inline('B4', 'Completed')]),
    row(5, [inline('A5', rfi('0004')), blank('B5')]),
    row(6, [inline('A6', rfi('0005')), blank('B6')]),
    row(7, [inline('A7', rfi('0006')), blank('B7')]),
    row(8, [inline('A8', rfi('0007')), blank('B8')]),
    row(9, [inline('A9', rfi('0008')), blank('B9')]),
    row(10, [inline('A10', rfi('0009')), blank('B10')]),
    row(11, [inline('A11', rfi('9999')), blank('B11')]),
    row(12, [
      inline('A12', 'prefix AZE-SUN-INT-QMS-RFI-CIV-0001'),
      blank('B12'),
    ]),
    row(13, [formula('A13', 'LOOKUP()', rfi('0017')), blank('B13')]),
    row(14, [inline('A14', rfi('0011')), blank('B14')]),
    row(15, [inline('A15', rfi('0014')), blank('B15')]),
    row(16, [inline('A16', rfi('0014')), blank('B16')]),
    row(17, [inline('A17', rfi('0012'))]),
    row(18, [
      inline('A18', rfi('0013')),
      formula('B18', 'LOOKUP()', 'Completed'),
    ]),
    row(19, [inline('A19', rfi('0016')), blank('B19')]),
    row(20, [inline('A20', rfi('0010')), inline('B20', 'Pending')]),
    row(21, [blank('A21'), inline('B21', 'Completed')]),
    row(22, [inline('A22', rfi('0015')), inline('B22', 'In Progress')]),
  ], const ['A14:C14', 'B19:C19']);
}

Uint8List noSelectionTarget() => targetWithRows([
  row(2, [inline('A2', 'CIV-0003'), inline('B2', 'Not Signed')]),
], const []);

void main() {
  test('preview matches the frozen Follow Up compatibility oracle', () {
    final oracle = _loadOracle();
    final plan = previewBytes(compatibilitySource(), compatibilityTarget());

    expect(
      publicOracleProjection(plan),
      equals(oracle['defaultReview']),
      reason: 'public Follow Up preview diverged from the frozen compatibility oracle',
    );

    final noSelection = previewBytes(compatibilitySource(), noSelectionTarget());
    expect(noSelection.steps, isEmpty);
    expect(
      [for (final warning in noSelection.warnings) warning.toJson()],
      equals(oracle['noSelectionWarnings']),
    );
  });

  test('compatibility oracle matches exact rows', () {
    final oracle = _loadOracle();
    final review = classifyBytes(compatibilitySource(), compatibilityTarget());
    final actual = oracleProjection(review);
    final defaultReview = obj(oracle['defaultReview']);
    expect(actual['rows'], equals(defaultReview['rows']));
    expect(actual['warnings'], equals(defaultReview['warnings']));

    final noSelection = classifyBytes(compatibilitySource(), noSelectionTarget());
    expect(noSelection['warnings'], equals(oracle['noSelectionWarnings']));
    expect(noSelection['editor'] as Map, containsPair('requiredFields', ['enabled']));
  });
}
