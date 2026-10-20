// Port of src-tauri/tests/rfi_system_sync_native.rs plus the in-module
// `candidate_normalization_tests` of src-tauri/src/rfi_system_sync.rs.
//
// Rust gates the signature-rendering tests on macOS/Windows, where its native
// PDF renderer exists. The Dart PDFKit / Windows renderer bridges are not
// ported (ocr.dart, UNVERIFIED), so those tests inject [_PdftoppmRenderer], a
// test renderer over poppler's `pdftoppm`, and are skipped when it is absent.
// Tests that Rust marks #[ignore] (operator-supplied files) are ported with
// the same skip reason.
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:archive/archive.dart';
import 'package:docflow/engine/contracts.dart';
import 'package:docflow/engine/errors.dart';
import 'package:docflow/engine/fs_ops.dart';
import 'package:docflow/engine/ocr.dart';
import 'package:docflow/engine/pdf_tool_renderer.dart';
import 'package:docflow/engine/ooxml.dart';
import 'package:docflow/engine/paths.dart';
import 'package:docflow/engine/plan_store.dart';
import 'package:docflow/engine/rfi_follow_up.dart' show FollowUpOoxml;
import 'package:docflow/engine/rfi_system_sync.dart' as rfi_system_sync;
import 'package:docflow/engine/rfi_system_sync.dart'
    show
        exactCanonicalRfi,
        openSyncWorkbookFile,
        previewCompleteStatusSync,
        previewSystemSync,
        scanSystemRootIndexed;
import 'package:docflow/engine/scan_index.dart';
import 'package:docflow/engine/undo.dart' show undoJournal;
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

const String _main =
    'http://schemas.openxmlformats.org/spreadsheetml/2006/main';
const String _officeRel =
    'http://schemas.openxmlformats.org/officeDocument/2006/relationships';
const String _packageRel =
    'http://schemas.openxmlformats.org/package/2006/relationships';

// ---- Workbook fixtures ------------------------------------------------------

Uint8List _package(List<(String, String)> sheets) {
  final archive = Archive();
  void add(String name, List<int> bytes) {
    final file = ArchiveFile(name, bytes.length, bytes)..compress = false;
    archive.addFile(file);
  }

  final workbookSheets = StringBuffer();
  final relationships = StringBuffer();
  for (var index = 0; index < sheets.length; index++) {
    final (name, worksheet) = sheets[index];
    final ordinal = index + 1;
    workbookSheets.write(
      '<sheet name="$name" sheetId="$ordinal" r:id="rId$ordinal"/>',
    );
    relationships.write(
      '<Relationship Id="rId$ordinal" Type="$_officeRel/worksheet" Target="worksheets/sheet$ordinal.xml"/>',
    );
    add('xl/worksheets/sheet$ordinal.xml', utf8.encode(worksheet));
  }
  relationships.write(
    '<Relationship Id="rIdCalc" Type="$_officeRel/calcChain" Target="calcChain.xml"/>',
  );
  final packageRelationships =
      '<Relationships xmlns="$_packageRel"><Relationship Id="rIdWorkbook" Type="$_officeRel/officeDocument" Target="xl/workbook.xml"/></Relationships>';
  final workbook =
      '<workbook xmlns="$_main" xmlns:r="$_officeRel"><sheets>$workbookSheets</sheets></workbook>';
  final rels =
      '<Relationships xmlns="$_packageRel">$relationships</Relationships>';
  final calcChain =
      '<calcChain xmlns="$_main"><c r="T10" i="1"/><c r="U10" i="2"/></calcChain>';
  for (final (name, content) in [
    ('[Content_Types].xml', 'content-types'),
    ('_rels/.rels', packageRelationships),
    ('xl/workbook.xml', workbook),
    ('xl/_rels/workbook.xml.rels', rels),
    ('xl/calcChain.xml', calcChain),
  ]) {
    add(name, utf8.encode(content));
  }
  return Uint8List.fromList(ZipEncoder().encode(archive)!);
}

String _inline(String reference, String value) =>
    '<c r="$reference" t="inlineStr"><is><t>$value</t></is></c>';

String _formula(String reference, String attributes, String value) =>
    '<c r="$reference"><f $attributes>$value</f><v/></c>';

String _row(int number, List<String> cells) =>
    '<row r="$number">${cells.join()}</row>';

String _worksheet(List<String> rows) =>
    '<worksheet xmlns="$_main"><sheetData>${rows.join()}</sheetData></worksheet>';

// ---- PDF fixtures -----------------------------------------------------------

void _writePdfWithSignature(String path, bool signed) {
  if (signed) {
    _writePdfAt(path, 470, 195, 'E.Mammadov Signed Worley');
  } else {
    _writePdfAt(path, 100, 700, 'RFI Inspection Details');
  }
}

void _writePdfAt(String path, int x, int y, String text) {
  final stream = 'BT /F1 18 Tf $x $y Td ($text) Tj ET\n';
  final objects = [
    '<< /Type /Catalog /Pages 2 0 R >>',
    '<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>',
    '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>',
    '<< /Length ${stream.length} >>\nstream\n${stream}endstream',
  ];
  final bytes = BytesBuilder()..add(utf8.encode('%PDF-1.4\n'));
  final offsets = <int>[];
  for (var index = 0; index < objects.length; index++) {
    offsets.add(bytes.length);
    bytes.add(utf8.encode('${index + 1} 0 obj\n${objects[index]}\nendobj\n'));
  }
  final xref = bytes.length;
  bytes.add(
    utf8.encode('xref\n0 ${objects.length + 1}\n0000000000 65535 f \n'),
  );
  for (final offset in offsets) {
    bytes.add(utf8.encode('${offset.toString().padLeft(10, '0')} 00000 n \n'));
  }
  bytes.add(
    utf8.encode(
      'trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n$xref\n%%EOF\n',
    ),
  );
  File(path).writeAsBytesSync(bytes.takeBytes());
}

// ---- Test renderer and authority wiring ----------------------------------

String? _findPdftoppm() {
  for (final directory in [
    ...(Platform.environment['PATH'] ?? '').split(':'),
    '/opt/homebrew/bin',
    '/usr/local/bin',
    '/usr/bin',
  ]) {
    if (directory.isEmpty) continue;
    final candidate = p.join(directory, 'pdftoppm');
    if (File(candidate).existsSync()) {
      return candidate;
    }
  }
  return null;
}

final String? _pdftoppm = Platform.isWindows ? null : _findPdftoppm();
final Object _renderSkip = _pdftoppm == null
    ? 'requires a PDF page renderer (pdftoppm); the native PDFKit/Windows '
          'bridges are not ported'
    : false;

/// Test-only [PdfPageRenderer] over poppler's `pdftoppm`.
class _PdftoppmRenderer implements PdfPageRenderer {
  const _PdftoppmRenderer();

  @override
  Future<PdfPageSession> openSession(String path) async =>
      _PdftoppmSession(path);
}

class _PdftoppmSession implements PdfPageSession {
  _PdftoppmSession(this.input);

  final String input;

  @override
  Future<String> pageText(int pageIndex) async => '';

  @override
  Future<void> renderPagePng(String output, int dpi, int pageIndex) async {
    final prefix = output.endsWith('.png')
        ? output.substring(0, output.length - 4)
        : output;
    final page = '${pageIndex + 1}';
    final result = await Process.run(_pdftoppm!, [
      '-png',
      '-r',
      '$dpi',
      '-f',
      page,
      '-l',
      page,
      '-singlefile',
      input,
      prefix,
    ]);
    if (result.exitCode != 0) {
      throw StateError('pdftoppm failed: ${result.stderr}');
    }
    if ('$prefix.png' != output) {
      File('$prefix.png').renameSync(output);
    }
  }

  @override
  Future<void> close() async {}
}

/// Production poppler renderer (lib/engine/pdf_tool_renderer.dart) when present.
final PdfPageRenderer _renderer = PdfToolPageRenderer.discover() ?? const _PdftoppmRenderer();

/// Ports that the RFI system sync apply path never reaches.
class _UnusedPorts
    implements RfiLogWorkbookPort, PdfEngine, ReplicaPublishPort {
  const _UnusedPorts();

  @override
  dynamic noSuchMethod(Invocation invocation) => throw UnsupportedError(
    'not used by the RFI system sync apply path: ${invocation.memberName}',
  );
}

class _UnusedFollowUpOoxml implements FollowUpOoxml {
  const _UnusedFollowUpOoxml();

  @override
  dynamic noSuchMethod(Invocation invocation) => throw UnsupportedError(
    'not used by the RFI system sync apply path: ${invocation.memberName}',
  );
}

FilesystemAuthority _authority(String journalDir) => FilesystemAuthority(
  journalDir: journalDir,
  ttl: const Duration(seconds: 300),
  ooxml: const _UnusedFollowUpOoxml(),
  rfiLog: const _UnusedPorts(),
  rfiSystemSync: const rfi_system_sync.RfiSystemSync(),
  pdf: const _UnusedPorts(),
  replicaPublish: const _UnusedPorts(),
  undoJournal: undoJournal,
);

Plan _cache(
  FilesystemAuthority authority,
  CallerIdentity caller,
  String requestId,
  String operation,
  String root,
  Plan plan,
) {
  final grant = authority.issueGrant(
    GrantScope(caller: caller, requestId: requestId, operationId: operation),
    root,
    GrantAccess.readWrite,
  );
  return authority.cachePlan(
    BackendOrigin.rust,
    PlanBinding(
      ownerId: caller.ownerId,
      sessionId: caller.sessionId,
      operationId: operation,
      grantIds: [grant.grantId],
    ),
    plan,
  );
}

WorkbookData _readWorkbookAt(String path) =>
    readWorkbook(File(path).readAsBytesSync());

List<Map<String, Object?>> _rows(Plan plan) => [
  for (final row in (plan.meta['editor']! as Map)['rows']! as List)
    (row as Map).cast<String, Object?>(),
];

Directory _tempDir() => Directory.systemTemp.createTempSync('df-e7-sync-');

void main() {
  // ---- src/rfi_system_sync.rs: candidate_normalization_tests -------------

  test('sync_identity_accepts_only_anchored_supported_rfi_suffixes', () {
    expect(
      exactCanonicalRfi('AZE-SUN-INT-QMS-RFI-CIV-901'),
      'AZE-SUN-INT-QMS-RFI-CIV-0901',
    );
    expect(exactCanonicalRfi('CIV-9'), 'AZE-SUN-INT-QMS-RFI-CIV-0009');
    expect(exactCanonicalRfi('ITP-CIV-901'), isNull);
    expect(exactCanonicalRfi('2026-09-01'), isNull);
    expect(exactCanonicalRfi('AZE-SUN-INT-QMS-RFI-CIV-09010'), isNull);
  });

  // ---- tests/rfi_system_sync_native.rs -----------------------------------

  test('signature_outside_others_is_not_worley_evidence', () async {
    final fixture = _tempDir();
    addTearDown(() => fixture.deleteSync(recursive: true));
    final system = p.join(fixture.path, 'RFI_SYSTEM');
    final folder = p.join(system, 'CIV/AZE-SUN-INT-QMS-RFI-CIV-0102');
    Directory(folder).createSync(recursive: true);
    _writePdfAt(
      p.join(folder, 'RFI No. AZE-SUN-INT-QMS-RFI-CIV-0102.pdf'),
      460,
      100,
      'Unrelated lower-page signature',
    );

    final (records, _) = await scanSystemRootIndexed(
      system,
      null,
      'owner',
      false,
      (_) {},
      renderer: _renderer,
    );
    expect(records, hasLength(1));
    expect(
      records[0].signedByWorley,
      isFalse,
      reason: records[0].signatureReason,
    );
  }, skip: _renderSkip);

  test('system_scan_requires_the_exact_canonical_rfi_pdf', () async {
    final fixture = _tempDir();
    addTearDown(() => fixture.deleteSync(recursive: true));
    final system = p.join(fixture.path, 'RFI_SYSTEM');
    final missing = p.join(system, 'CIV/AZE-SUN-INT-QMS-RFI-CIV-0101');
    final exact = p.join(system, 'MEC/AZE-SUN-INT-QMS-RFI-MEC-0700');
    Directory(missing).createSync(recursive: true);
    Directory(exact).createSync(recursive: true);
    _writePdfWithSignature(p.join(missing, 'inspection.pdf'), true);
    _writePdfWithSignature(
      p.join(missing, 'RFI No. AZE-SUN-INT-QMS-RFI-CIV-9999.pdf'),
      true,
    );
    _writePdfWithSignature(
      p.join(exact, 'rfi no. aze-sun-int-qms-rfi-mec-0700.PDF'),
      true,
    );

    final (records, _) = await scanSystemRootIndexed(
      system,
      null,
      'owner',
      false,
      (_) {},
      renderer: _renderer,
    );
    expect(records, hasLength(2));
    final missingRecord = records.firstWhere(
      (record) => record.canonicalRfi.endsWith('CIV-0101'),
    );
    expect(missingRecord.pdfPath, isNull);
    expect(missingRecord.signedByWorley, isFalse);
    expect(
      missingRecord.signatureReason,
      'Exact RFI No. PDF not found in folder',
    );
    final exactRecord = records.firstWhere(
      (record) => record.canonicalRfi.endsWith('MEC-0700'),
    );
    expect(exactRecord.pdfPath, isNotNull);
    expect(
      exactRecord.signedByWorley,
      isTrue,
      reason: exactRecord.signatureReason,
    );
  }, skip: _renderSkip);

  test('operator_worley_examples_use_only_the_others_signature_cell', () async {
    final signed = Platform.environment['DOCFLOW_SIGNED_WORLEY_PDF']!;
    final unsigned = Platform.environment['DOCFLOW_UNSIGNED_WORLEY_PDF']!;
    final expectedUnsignedRfi =
        Platform.environment['DOCFLOW_UNSIGNED_WORLEY_RFI'] ?? 'MEC-0737';
    final fixture = _tempDir();
    addTearDown(() => fixture.deleteSync(recursive: true));
    final system = p.join(fixture.path, 'RFI_SYSTEM');
    final signedFolder = p.join(system, 'CIV/AZE-SUN-INT-QMS-RFI-CIV-1041');
    final unsignedFolder = p.join(
      system,
      'MEC',
      'AZE-SUN-INT-QMS-RFI-$expectedUnsignedRfi',
    );
    Directory(signedFolder).createSync(recursive: true);
    Directory(unsignedFolder).createSync(recursive: true);
    File(signed).copySync(
      p.join(signedFolder, 'RFI No. AZE-SUN-INT-QMS-RFI-CIV-1041.pdf'),
    );
    File(unsigned).copySync(
      p.join(
        unsignedFolder,
        'RFI No. AZE-SUN-INT-QMS-RFI-$expectedUnsignedRfi.pdf',
      ),
    );
    final (records, _) = await scanSystemRootIndexed(
      system,
      null,
      'owner',
      false,
      (_) {},
      renderer: _renderer,
    );
    final signedRecord = records.firstWhere(
      (record) => record.canonicalRfi.endsWith('CIV-1041'),
    );
    final unsignedRecord = records.firstWhere(
      (record) => record.canonicalRfi.endsWith(expectedUnsignedRfi),
    );
    expect(
      signedRecord.signedByWorley,
      isTrue,
      reason: signedRecord.signatureReason,
    );
    expect(
      unsignedRecord.signedByWorley,
      isFalse,
      reason: unsignedRecord.signatureReason,
    );
  }, skip: 'requires operator-supplied signed and unsigned RFI PDFs');

  test('unchanged_system_pdf_is_reused_from_durable_index', () async {
    final fixture = _tempDir();
    addTearDown(() => fixture.deleteSync(recursive: true));
    final system = p.join(fixture.path, 'RFI_SYSTEM');
    final folder = p.join(system, 'MEC/AZE-SUN-INT-QMS-RFI-MEC-0700');
    Directory(folder).createSync(recursive: true);
    _writePdfWithSignature(
      p.join(folder, 'RFI No. AZE-SUN-INT-QMS-RFI-MEC-0700.pdf'),
      true,
    );
    final index = ScanIndex.open(p.join(fixture.path, 'scan-index.sqlite3'));
    addTearDown(index.close);
    final progress = <ProgressEvent>[];
    final (first, firstStats) = await scanSystemRootIndexed(
      system,
      index,
      'owner',
      false,
      progress.add,
      renderer: _renderer,
    );
    expect((firstStats.analyzed, firstStats.reused), (1, 0));
    final (second, secondStats) = await scanSystemRootIndexed(
      system,
      index,
      'owner',
      false,
      (_) {},
      renderer: _renderer,
    );
    expect((secondStats.analyzed, secondStats.reused), (0, 1));
    expect(first, second);
    expect(firstStats.parallelAnalysis, isTrue);
    expect(firstStats.analysisWorkers, greaterThanOrEqualTo(1));
    expect(progress.any((event) => event.phase == 'system_discovery'), isTrue);
    expect(
      progress.any((event) => event.phase == 'system_signature_analysis'),
      isTrue,
    );
  }, skip: _renderSkip);

  test(
    'completion_status_preview_recurses_and_patches_only_exact_safe_log_cells',
    () async {
      final fixture = _tempDir();
      addTearDown(() => fixture.deleteSync(recursive: true));
      final fixturePath = fixture.resolveSymbolicLinksSync();
      final system = p.join(fixturePath, 'RFI_SYSTEM');
      final nested = p.join(system, 'MEC/Area-1/deeper');
      Directory(nested).createSync(recursive: true);
      File(p.join(nested, 'RFI No. AZE-SUN-INT-QMS-RFI-MEC-0700.pdf'))
          .writeAsStringSync('exact-name evidence');
      File(p.join(nested, 'RFI No. AZE-SUN-INT-QMS-RFI-MEC-07001.pdf'))
          .writeAsStringSync('numeric suffix must not match');

      final statusSheet = _worksheet([
        _row(9, [
          _inline('A9', 'RFI No.'),
          _inline('P9', 'Inspection Status'),
          _inline('Q9', 'KEEP'),
        ]),
        _row(10, [
          _inline('A10', 'AZE-SUN-INT-QMS-RFI-MEC-0700'),
          _inline('P10', 'Ongoing'),
          _formula('Q10', '', '1+1'),
        ]),
      ]);
      final emptyStatusSheet = _worksheet([
        _row(9, [_inline('A9', 'RFI No.'), _inline('P9', 'Inspection Status')]),
      ]);
      final workbook = _package([
        ('CIV', emptyStatusSheet),
        ('MEC', statusSheet),
        ('ELE', emptyStatusSheet),
        ('PRC', emptyStatusSheet),
      ]);
      final logPath = p.join(fixturePath, 'RFI Log.xlsx');
      File(logPath).writeAsBytesSync(workbook);
      final progress = <ProgressEvent>[];
      final plan = await previewCompleteStatusSync(
        system,
        logPath,
        openSyncWorkbookFile(logPath),
        progress.add,
        () => false,
      );

      expect(plan.opId, rfi_system_sync.completeStatusOperationId);
      expect(plan.steps, hasLength(1));
      expect(_rows(plan), hasLength(1));
      expect(_rows(plan)[0]['currentStatus'], 'Ongoing');
      expect(_rows(plan)[0]['proposedStatus'], 'Completed');
      expect(
        progress.any((event) => event.phase == 'completion_pdf_discovery'),
        isTrue,
      );

      final disabledRows = _rows(plan)
          .map((row) => Map<String, Object?>.of(row))
          .toList();
      disabledRows[0]['enabled'] = false;
      final disabled = rfi_system_sync.revise(plan, disabledRows);
      expect(disabled.steps, isEmpty);
      expect(
        disabled.warnings.any(
          (warning) => warning.key == 'warn.rfi.completeStatus.noRowsSelected',
        ),
        isTrue,
      );

      final output = p.join(fixture.path, 'patched.xlsx');
      rfi_system_sync.executeSyncStep(plan.steps[0], logPath, output);
      final patched = _readWorkbookAt(output);
      expect(patched.sheet('MEC')!.value(10, 16), 'Completed');
      expect(patched.sheet('MEC')!.cell(10, 17)!.formula, '1+1');

      final authority = _authority(p.join(fixturePath, 'completion-journal'));
      const caller = CallerIdentity('completion-owner', 'completion-session');
      final cached = _cache(
        authority,
        caller,
        'completion-request',
        rfi_system_sync.completeStatusOperationId,
        fixturePath,
        plan,
      );
      final applied = authority.applyWithProgress(
        cached.planId,
        caller,
        (_) {},
      );
      expect((applied.applied, applied.failed), (1, 0));
      expect(_readWorkbookAt(logPath).sheet('MEC')!.value(10, 16), 'Completed');
      authority.undo(applied.journalId, caller);
      expect(_readWorkbookAt(logPath).sheet('MEC')!.value(10, 16), 'Ongoing');
    },
  );

  test(
    'completion_status_preview_refuses_terminal_or_formula_controlled_cells',
    () async {
      final fixture = _tempDir();
      addTearDown(() => fixture.deleteSync(recursive: true));
      final system = p.join(fixture.path, 'RFI_SYSTEM');
      Directory(p.join(system, 'deep')).createSync(recursive: true);
      for (final code in [
        'AZE-SUN-INT-QMS-RFI-CIV-0103',
        'AZE-SUN-INT-QMS-RFI-MEC-0700',
      ]) {
        File(p.join(system, 'deep', 'RFI No. $code.pdf'))
            .writeAsStringSync('exact-name evidence');
      }
      final civ = _worksheet([
        _row(9, [_inline('A9', 'RFI No.'), _inline('P9', 'Inspection Status')]),
        _row(10, [
          _inline('A10', 'AZE-SUN-INT-QMS-RFI-CIV-0103'),
          _formula('P10', '', '1+1'),
        ]),
      ]);
      final mec = _worksheet([
        _row(9, [_inline('A9', 'RFI No.'), _inline('P9', 'Inspection Status')]),
        _row(10, [
          _inline('A10', 'AZE-SUN-INT-QMS-RFI-MEC-0700'),
          _inline('P10', 'Cancelled'),
        ]),
      ]);
      final empty = _worksheet([
        _row(9, [_inline('A9', 'RFI No.'), _inline('P9', 'Inspection Status')]),
      ]);
      final logPath = p.join(fixture.path, 'RFI Log.xlsx');
      File(logPath).writeAsBytesSync(
        _package([('CIV', civ), ('MEC', mec), ('ELE', empty), ('PRC', empty)]),
      );
      final plan = await previewCompleteStatusSync(
        system,
        logPath,
        openSyncWorkbookFile(logPath),
        (_) {},
        () => false,
      );
      expect(plan.steps, isEmpty);
      final rows = _rows(plan);
      expect(rows, hasLength(2));
      expect(rows.every((row) => row['enabled'] == false), isTrue);
      expect(
        rows.every(
          (row) =>
              row['disposition'] ==
              'rfi.completeStatus.disposition.manualReview',
        ),
        isTrue,
      );
    },
  );

  test('test_system_sync_full_preview_apply_and_undo', () async {
    final fixture = _tempDir();
    addTearDown(() => fixture.deleteSync(recursive: true));
    final fixturePath = fixture.resolveSymbolicLinksSync();
    final systemRoot = p.join(fixturePath, 'RFI_SYSTEM');
    final civDir = p.join(systemRoot, 'CIV');
    final mecDir = p.join(systemRoot, 'MEC');
    Directory(civDir).createSync(recursive: true);
    Directory(mecDir).createSync(recursive: true);

    void rfi(String root, String code, bool signed) {
      final dir = p.join(root, 'AZE-SUN-INT-QMS-RFI-$code');
      Directory(dir).createSync(recursive: true);
      _writePdfWithSignature(
        p.join(dir, 'RFI No. AZE-SUN-INT-QMS-RFI-$code.pdf'),
        signed,
      );
    }

    // 1. CIV-0103 signed
    rfi(civDir, 'CIV-0103', true);
    // 2. CIV-0104 unsigned
    rfi(civDir, 'CIV-0104', false);
    // 3. CIV-0105 unsigned must not revoke a prior signed status.
    rfi(civDir, 'CIV-0105', false);
    // 4. CIV-0106 unsigned has a source log row but no prepared signature row.
    rfi(civDir, 'CIV-0106', false);
    // 5. MEC-0700 signed
    rfi(mecDir, 'MEC-0700', true);

    // Mirror the production workbook schema: REMARKS is V/W, while T/U is a
    // shared-formula Folder Link column. The rc.8 regression wrote into T/U.
    final civSheet = _worksheet([
      _row(9, [
        _inline('A9', 'RFI No.'),
        _inline('T9', 'Folder Link2'),
        _inline('V9', 'REMARKS'),
      ]),
      _row(10, [
        _inline('A10', 'AZE-SUN-INT-QMS-RFI-CIV-0103'),
        _formula('T10', 't="shared" si="1" ref="T10:T11"', 'HYPERLINK(A10)'),
        _inline('V10', ''),
      ]),
      _row(11, [
        _inline('A11', 'AZE-SUN-INT-QMS-RFI-CIV-0104'),
        _formula('T11', 't="shared" si="1"', ''),
        _inline('V11', ''),
      ]),
      _row(12, [
        _inline('A12', 'AZE-SUN-INT-QMS-RFI-CIV-0105'),
        _inline('V12', ''),
      ]),
      _row(13, [
        _inline('A13', 'AZE-SUN-INT-QMS-RFI-CIV-0106'),
        _inline('V13', ''),
      ]),
    ]);
    final mecSheet = _worksheet([
      _row(9, [
        _inline('A9', 'RFI No.'),
        _inline('U9', 'Folder Link'),
        _inline('W9', 'REMARKS'),
      ]),
      _row(10, [
        _inline('A10', 'AZE-SUN-INT-QMS-RFI-MEC-0700'),
        _formula('U10', 't="shared" si="2" ref="U10:U11"', 'HYPERLINK(A10)'),
        _inline('W10', 'existing note'),
      ]),
      _row(11, [_formula('U11', 't="shared" si="2"', ''), _inline('W11', '')]),
    ]);
    final eleSheet = _worksheet([
      _row(9, [_inline('A9', 'RFI No.'), _inline('V9', 'REMARKS')]),
    ]);
    final prcSheet = _worksheet([
      _row(9, [_inline('A9', 'RFI No.'), _inline('W9', 'REMARKS')]),
    ]);
    final rfiLogBytes = _package([
      ('CIV', civSheet),
      ('MEC', mecSheet),
      ('ELE', eleSheet),
      ('PRC', prcSheet),
    ]);
    final rfiLogPath = p.join(fixturePath, 'RFI Log.xlsx');
    File(rfiLogPath).writeAsBytesSync(rfiLogBytes);
    final originalCalcChain = readMember(rfiLogBytes, 'xl/calcChain.xml');

    // Mirror the current Follow Up control tables, not the retired Final-ITP
    // layout.
    final fuCiv = _worksheet([
      _row(4, [_inline('C4', 'RFI No.'), _inline('D4', 'STATUS')]),
      _row(5, [_inline('C5', 'CIV-0103'), _inline('D5', 'Not Signed')]),
      _row(6, [_inline('C6', 'CIV-0104'), _inline('D6', '')]),
      _row(7, [_inline('C7', 'CIV-0105'), _inline('D7', 'Signed')]),
    ]);
    final fuMec = _worksheet([
      _row(3, [_inline('S3', 'RFI No.'), _inline('T3', 'Worley Status')]),
      _row(4, [_inline('S4', 'MEC-0700'), _inline('T4', 'Not Signed')]),
    ]);
    final followUpBytes = _package([
      ('Signed By Worley CIV', fuCiv),
      ('Signed By Worley MEC', fuMec),
    ]);
    final followUpPath = p.join(fixturePath, 'Follow Up.xlsx');
    File(followUpPath).writeAsBytesSync(followUpBytes);

    var plan = await previewSystemSync(
      systemRoot,
      rfiLogPath,
      followUpPath,
      openSyncWorkbookFile(rfiLogPath),
      openSyncWorkbookFile(followUpPath),
      renderer: _renderer,
    );
    final missingTarget = _rows(plan)
        .firstWhere((row) => row['rfiNo'] == 'CIV-0106');
    expect(
      missingTarget['disposition'],
      'rfi.systemSync.disposition.manualReview',
    );
    expect(
      missingTarget['followUpAction'],
      'Manual review: no existing Follow Up control row',
    );
    expect(missingTarget['selectable'], false);

    expect(plan.opId, rfi_system_sync.operationId);
    expect(
      plan.steps,
      hasLength(2),
      reason: 'Expected 2 steps: RFI Log update and Follow Up update',
    );
    final revisedRows = _rows(plan)
        .map((row) => Map<String, Object?>.of(row))
        .toList();
    revisedRows[2]['enabled'] = false;
    final revised = rfi_system_sync.revise(plan, revisedRows);
    for (final step in revised.steps) {
      final updates =
          (step.meta['syncStep']! as Map)['updates']! as Map<String, Object?>;
      for (final sheet in updates.values) {
        for (final update in (sheet! as Map).values) {
          expect((update as Map)['rowId'], isNot('sync-row-3'));
        }
      }
    }

    // Any post-preview change must fail before either workbook or any backup
    // is touched.
    final staleAuthority = _authority(p.join(fixturePath, 'stale-journal'));
    const staleCaller = CallerIdentity('stale-owner', 'stale-session');
    final staleCached = _cache(
      staleAuthority,
      staleCaller,
      'stale-request',
      rfi_system_sync.operationId,
      fixturePath,
      plan,
    );
    File(rfiLogPath)
        .writeAsBytesSync(utf8.encode('stale'), mode: FileMode.append);
    expect(
      () => staleAuthority.applyWithProgress(
        staleCached.planId,
        staleCaller,
        (_) {},
      ),
      throwsA(
        isA<BackendError>().having(
          (error) => error.message,
          'message',
          'error.rfi.systemSync.workbookChanged',
        ),
      ),
    );
    for (final step in plan.steps) {
      expect(File(step.dst!).existsSync(), isFalse);
    }
    File(rfiLogPath).writeAsBytesSync(rfiLogBytes);

    plan = await previewSystemSync(
      systemRoot,
      rfiLogPath,
      followUpPath,
      openSyncWorkbookFile(rfiLogPath),
      openSyncWorkbookFile(followUpPath),
      renderer: _renderer,
    );

    // Setup FilesystemAuthority for Apply & Undo
    final authority = _authority(p.join(fixturePath, 'journal'));
    const caller = CallerIdentity('test-owner', 'test-session');
    final cached = _cache(
      authority,
      caller,
      'test-req-1',
      rfi_system_sync.operationId,
      fixturePath,
      plan,
    );

    // APPLY
    final result = authority.applyWithProgress(cached.planId, caller, (_) {});
    expect(result.applied, 2);
    expect(result.failed, 0);

    // Verify only the discovered REMARKS cells changed. Folder Link shared
    // formulas and calcChain must remain intact.
    final updatedRfiLog = _readWorkbookAt(rfiLogPath);
    final civ = updatedRfiLog.sheet('CIV')!;
    expect(civ.value(10, 22), 'Signed by Worley');
    expect(
      civ.value(11, 22),
      '',
      reason: 'Unsigned RFI should remain unchanged',
    );
    expect(civ.value(12, 22), '');
    expect(civ.value(13, 22), '');
    expect(civ.cell(10, 20)!.formula, isNotNull);
    expect(civ.cell(11, 20)!.formula, isNotNull);

    final mec = updatedRfiLog.sheet('MEC')!;
    expect(mec.value(10, 23), 'existing note\nSigned by Worley');
    expect(mec.cell(10, 21)!.formula, isNotNull);
    expect(mec.cell(11, 21)!.formula, isNotNull);
    expect(
      readMember(File(rfiLogPath).readAsBytesSync(), 'xl/calcChain.xml'),
      originalCalcChain,
    );

    // Verify Follow Up was updated
    final updatedFu = _readWorkbookAt(followUpPath);
    final fuCivSheet = updatedFu.sheet('Signed By Worley CIV')!;
    expect(fuCivSheet.value(5, 4), 'Signed');
    expect(fuCivSheet.value(6, 4), 'Not Signed');
    expect(fuCivSheet.value(7, 4), 'Signed');
    expect(updatedFu.sheet('Signed By Worley MEC')!.value(4, 20), 'Signed');

    // UNDO
    final undoResult = authority.undo(result.journalId, caller);
    expect(undoResult.failed, 0);

    // Undo owns restoration of both workbook files; verify their original
    // bytes.
    final restoredRfiLog = File(rfiLogPath).readAsBytesSync();
    final restoredFu = File(followUpPath).readAsBytesSync();
    expect(restoredRfiLog, rfiLogBytes);
    expect(restoredFu, followUpBytes);
    final checkLog = readWorkbook(restoredRfiLog);
    expect(checkLog.sheet('CIV')!.value(10, 22), '');
    expect(checkLog.sheet('MEC')!.value(10, 23), 'existing note');
    final checkFu = readWorkbook(restoredFu);
    expect(checkFu.sheet('Signed By Worley CIV')!.value(6, 4), '');
    expect(checkFu.sheet('Signed By Worley CIV')!.value(7, 4), 'Signed');
    expect(checkFu.sheet('Signed By Worley MEC')!.value(4, 20), 'Not Signed');
  }, skip: _renderSkip);

  test(
    'current_workbook_templates_accept_only_closed_guarded_deltas',
    () async {
      final rfiLogSource = Platform.environment['DOCFLOW_CURRENT_RFI_LOG']!;
      final followUpSource = Platform.environment['DOCFLOW_CURRENT_FOLLOW_UP']!;
      final fixture = _tempDir();
      addTearDown(() => fixture.deleteSync(recursive: true));
      final root = p.join(fixture.path, 'RFI_SYSTEM');
      for (final rfi in ['MEC-0001', 'MEC-0361']) {
        final folder = p.join(root, 'MEC', 'AZE-SUN-INT-QMS-RFI-$rfi');
        Directory(folder).createSync(recursive: true);
        _writePdfWithSignature(
          p.join(folder, 'RFI No. AZE-SUN-INT-QMS-RFI-$rfi.pdf'),
          true,
        );
      }
      final rfiLog = p.join(fixture.path, 'RFI Log.xlsm');
      final followUp = p.join(fixture.path, 'Inspection Status Follow Up.xlsm');
      final originalRfiLog = File(rfiLogSource).readAsBytesSync();
      final originalFollowUp = File(followUpSource).readAsBytesSync();
      File(rfiLogSource).copySync(rfiLog);
      File(followUpSource).copySync(followUp);
      final plan = await previewSystemSync(
        root,
        rfiLog,
        followUp,
        openSyncWorkbookFile(rfiLog),
        openSyncWorkbookFile(followUp),
        renderer: _renderer,
      );
      final rows = _rows(plan);
      expect(
        rows,
        hasLength(2),
        reason: 'both discovered RFIs must remain visible',
      );
      if (plan.steps.isEmpty) {
        expect(
          rows.every(
            (row) =>
                row['disposition'] ==
                    'rfi.systemSync.disposition.alreadySynced' ||
                (row['disposition'] ==
                        'rfi.systemSync.disposition.manualReview' &&
                    row['selectable'] == false &&
                    row['enabled'] == false &&
                    row['followUpAction'] ==
                        'Manual review: no existing Follow Up control row'),
          ),
          isTrue,
          reason: 'a zero-step plan must represent every RFI as synchronized or as a concrete non-selectable review',
        );
        expect(
          rows.any(
                (row) =>
                    row['disposition'] ==
                    'rfi.systemSync.disposition.manualReview',
              ) ||
              rows.every(
                (row) =>
                    row['disposition'] ==
                    'rfi.systemSync.disposition.alreadySynced',
              ),
          isTrue,
          reason: 'a zero-step plan must not disguise unresolved RFIs as already synchronized',
        );
        expect(
          plan.warnings.any(
            (warning) => warning.key == 'warn.rfi.systemSync.noChangesNeeded',
          ),
          isTrue,
        );
      } else {
        expect(
          rows.any(
            (row) => row['disposition'] == 'rfi.systemSync.disposition.ready',
          ),
          isTrue,
          reason: 'a non-empty plan requires at least one ready RFI',
        );
        expect(plan.warnings, isEmpty);
        for (var index = 0; index < plan.steps.length; index++) {
          final step = plan.steps[index];
          rfi_system_sync.executeSyncStep(
            step,
            step.src!,
            p.join(fixture.path, 'guarded-output-$index.xlsm'),
          );
        }
      }
      expect(File(rfiLogSource).readAsBytesSync(), originalRfiLog);
      expect(File(followUpSource).readAsBytesSync(), originalFollowUp);
    },
    skip: 'requires operator-supplied current production workbook copies',
  );

  test(
    'current_rfi_log_accepts_only_closed_completion_status_deltas',
    () async {
      final source = Platform.environment['DOCFLOW_CURRENT_RFI_LOG']!;
      final fixture = _tempDir();
      addTearDown(() => fixture.deleteSync(recursive: true));
      final root = p.join(fixture.path, 'RFI_SYSTEM/deep/archive');
      Directory(root).createSync(recursive: true);
      for (final rfi in ['MEC-0001', 'MEC-0361']) {
        File(p.join(root, 'RFI No. AZE-SUN-INT-QMS-RFI-$rfi.pdf'))
            .writeAsStringSync('exact-name evidence');
      }
      final log = p.join(fixture.path, 'RFI Log.xlsm');
      File(source).copySync(log);
      final plan = await previewCompleteStatusSync(
        p.join(fixture.path, 'RFI_SYSTEM'),
        log,
        openSyncWorkbookFile(log),
        (_) {},
        () => false,
      );
      expect(_rows(plan), hasLength(2));
      for (var index = 0; index < plan.steps.length; index++) {
        rfi_system_sync.executeSyncStep(
          plan.steps[index],
          log,
          p.join(fixture.path, 'completion-output-$index.xlsm'),
        );
      }
    },
    skip: 'requires an operator-supplied current production RFI Log copy',
  );

  test(
    'operator_ventoy_system_sync_uses_real_signatures_and_workbooks',
    () async {
      final systemRoot =
          Platform.environment['DOCFLOW_VENTOY_RFI_SYSTEM_ROOT']!;
      final fixture = _tempDir();
      addTearDown(() => fixture.deleteSync(recursive: true));
      final rfiLog = p.join(fixture.path, 'RFI Log.xlsm');
      final followUp = p.join(fixture.path, 'Inspection Status Follow Up.xlsm');
      File(Platform.environment['DOCFLOW_CURRENT_RFI_LOG']!).copySync(rfiLog);
      File(Platform.environment['DOCFLOW_CURRENT_FOLLOW_UP']!)
          .copySync(followUp);
      final originalLog = File(rfiLog).readAsBytesSync();
      final originalFollowUp = File(followUp).readAsBytesSync();
      final plan = await previewSystemSync(
        systemRoot,
        rfiLog,
        followUp,
        openSyncWorkbookFile(rfiLog),
        openSyncWorkbookFile(followUp),
        renderer: _renderer,
      );
      final rows = _rows(plan);
      expect(rows, hasLength(4));
      expect(plan.steps, isEmpty);
      expect(
        plan.warnings.any(
          (warning) => warning.key == 'warn.rfi.systemSync.noChangesNeeded',
        ),
        isTrue,
      );
      Map<String, Object?> rowFor(String suffix) => rows.firstWhere(
        (row) =>
            row['canonicalRfi'] is String &&
            (row['canonicalRfi']! as String).endsWith(suffix),
      );
      expect(rowFor('ELE-0032')['worleyStatus'], 'Signed by Worley');
      for (final suffix in ['CIV-1316', 'MEC-0767', 'PRC-0074']) {
        expect(
          rowFor(suffix)['worleyStatus'],
          'Unsigned',
          reason: '$suffix has a visually blank OTHERS signature cell',
        );
      }
      for (var index = 0; index < plan.steps.length; index++) {
        final step = plan.steps[index];
        final output = p.join(fixture.path, 'Ventoy-sync-$index.xlsm');
        rfi_system_sync.executeSyncStep(step, step.src!, output);
        expect(File(output).existsSync(), isTrue);
      }
      expect(File(rfiLog).readAsBytesSync(), originalLog);
      expect(File(followUp).readAsBytesSync(), originalFollowUp);
    },
    skip:
        'set DOCFLOW_VENTOY_RFI_SYSTEM_ROOT, DOCFLOW_CURRENT_RFI_LOG, and '
        'DOCFLOW_CURRENT_FOLLOW_UP to private Ventoy copies',
  );
}
