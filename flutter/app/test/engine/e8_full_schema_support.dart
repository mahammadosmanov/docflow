/// Full-schema builders shared by the E8 native parity tests. Port of the
/// `full_schema_*` and `pcu_*` helpers in src-tauri/tests/rfi_follow_up_native.rs.
library;

import 'dart:typed_data';

import 'package:docflow/engine/contracts.dart';

import 'e8_support.dart';

export 'e8_support.dart';

/// Port of `full_schema_source(civ_rows)`.
Uint8List fullSchemaSource(List<RowSpec> civRows) {
  final header = fullSchemaHeader();
  final civ = worksheet([header, ...civRows]);
  final mec = worksheet([header]);
  return sourcePackage(civ, mec);
}

RowSpec fullSchemaHeader() => row(9, [
  inline('A9', 'RFI No.'),
  inline('D9', 'ITP No:'),
  inline('E9', 'Item No.'),
  inline('F9', 'Tag No'),
  inline('G9', 'Area'),
  inline('N9', 'Inspection Location'),
  inline('T9', 'Inspection Status'),
  inline('V9', 'Remarks'),
]);

/// Port of `full_schema_record(row, suffix, item, tag, remarks)`.
RowSpec fullSchemaRecord(
  int rowNumber,
  String suffix,
  String item,
  String tag,
  String remarks,
) => fullSchemaRecordWithStatus(
  rowNumber,
  suffix,
  'AZE-SUN-INT-QMS-ITP-CIV-0002',
  item,
  tag,
  'Completed',
  remarks,
);

/// Port of `full_schema_record_with_status`.
RowSpec fullSchemaRecordWithStatus(
  int rowNumber,
  String suffix,
  String itp,
  String item,
  String tag,
  String status,
  String remarks,
) => row(rowNumber, [
  inline('A$rowNumber', 'AZE-SUN-INT-QMS-RFI-CIV-$suffix'),
  inline('D$rowNumber', itp),
  inline('E$rowNumber', item),
  inline('F$rowNumber', tag),
  inline('G$rowNumber', 'Zone 1'),
  inline('N$rowNumber', 'N1'),
  inline('T$rowNumber', status),
  inline('V$rowNumber', remarks),
]);

/// Port of `full_schema_mec_record(row, suffix, item, tag, area, location, status)`.
RowSpec fullSchemaMecRecord(
  int rowNumber,
  String suffix,
  String item,
  String tag,
  String area,
  String location,
  String status,
) => fullSchemaMecRecordWithItp(rowNumber, suffix, (
  'AZE-SUN-INT-QMS-ITP-MEC-0001',
  item,
  tag,
  area,
  location,
  status,
));

/// Port of `full_schema_mec_record_with_itp(row, suffix, fields)`.
RowSpec fullSchemaMecRecordWithItp(
  int rowNumber,
  String suffix,
  (String, String, String, String, String, String) fields,
) {
  final (itp, item, tag, area, location, status) = fields;
  return row(rowNumber, [
    inline('A$rowNumber', 'AZE-SUN-INT-QMS-RFI-MEC-$suffix'),
    inline('D$rowNumber', itp),
    inline('E$rowNumber', item),
    inline('F$rowNumber', tag),
    inline('G$rowNumber', area),
    inline('N$rowNumber', location),
    inline('T$rowNumber', status),
    blank('V$rowNumber'),
  ]);
}

/// Port of `full_schema_mec_header()`.
RowSpec fullSchemaMecHeaderRow() => fullSchemaHeader();

/// Port of `pcu_final_record(row, remarks)`.
RowSpec pcuFinalRecord(int rowNumber, String remarks) => row(rowNumber, [
  inline('A$rowNumber', 'AZE-SUN-INT-QMS-RFI-CIV-1001'),
  inline('D$rowNumber', 'AZE-SUN-INT-QMS-ITP-CIV-0005'),
  inline('E$rowNumber', 'CIV-2.3.6'),
  inline('F$rowNumber', 'PCS-01'),
  inline('G$rowNumber', 'Zone 1'),
  inline('N$rowNumber', 'N1'),
  inline('T$rowNumber', 'Completed'),
  inline('V$rowNumber', remarks),
]);

/// Port of `pcu_target(final_rfi, lifecycle_status, worley_status)`.
SheetSpec pcuTarget(String finalRfi, String lifecycleStatus, String worleyStatus) =>
    worksheet([
      row(3, [
        inline('P3', 'Final acceptance of PCU foundation (3.6)'),
        inline('R3', 'Signed by Worley'),
      ]),
      row(4, [inline('P4', 'RFI No.'), inline('Q4', 'STATUS')]),
      row(5, [
        inline('B5', 'N1'),
        inline('C5', 'PCS-01'),
        inline('P5', finalRfi),
        inline('Q5', lifecycleStatus),
        inline('R5', worleyStatus),
      ]),
    ]);

/// Rows of an editor projection as maps.
List<Map<String, Object?>> rowsOf(JsonObject review) =>
    (review['rows']! as List).cast<Map<String, Object?>>();

/// Nested map accessor for JSON objects.
Map<String, Object?> obj(Object? value) => (value! as Map).cast<String, Object?>();

/// Filters rows by a predicate.
Iterable<Map<String, Object?>> whereRows(
  JsonObject review,
  bool Function(Map<String, Object?>) test,
) => rowsOf(review).where(test);

const String readyDisposition = 'rfi.followUp.disposition.ready';
const String manualDisposition = 'rfi.followUp.disposition.manualReview';
const String alreadySyncedDisposition = 'rfi.followUp.disposition.alreadySynced';

/// MEC record whose Remarks cell is filled (Rust `.replace(&blank("V.."), ..)`).
RowSpec mecRecordWithRemarks(
  int rowNumber,
  String suffix,
  String item,
  String tag,
  String area,
  String location,
  String status,
  String remarks,
) {
  final base = fullSchemaMecRecord(
    rowNumber,
    suffix,
    item,
    tag,
    area,
    location,
    status,
  );
  return RowSpec(rowNumber, [
    ...base.cells.where((cell) => cell.reference != 'V$rowNumber'),
    inline('V$rowNumber', remarks),
  ]);
}

RowSpec mecTargetRow(int number, String rfi, String tag, String area) => row(
  number,
  [inline('B$number', rfi), inline('C$number', tag), inline('D$number', area), blank('N$number'), blank('O$number')],
);

RowSpec mecTargetHeader() => row(1, [inline('N1', 'RFI No.'), inline('O1', 'STATUS')]);
