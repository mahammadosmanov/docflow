// Port of src-tauri/tests/process_scans.rs (Preview-level cases) and the
// Process Scans inline behaviour it covers.
//
// Scope of this file:
// - RFI matching and the shared RFI/naming oracle run against rfi_code.dart
//   and naming.dart with no OCR engine.
// - Fast-cover Preview runs against process_scans.dart with fixture OCR
//   adapters and real multi-page PDFs written by [_writeMultiPagePdf].
// - Apply, Undo, native PDFKit / Windows renderer and review-decision cases
//   are skipped with a reason. They are not faked here.
@TestOn('vm')
library;

import 'dart:convert';
import 'dart:io';

import 'package:docflow/engine/contracts.dart';
import 'package:docflow/engine/errors.dart';
import 'package:docflow/engine/fs_ops.dart';
import 'package:docflow/engine/naming.dart';
import 'package:docflow/engine/ocr.dart';
import 'package:docflow/engine/paths.dart';
import 'package:docflow/engine/plan_store.dart';
import 'package:docflow/engine/production_ports.dart';
import 'package:docflow/engine/rfi_system_sync.dart' show RfiSystemSync;
import 'package:docflow/engine/review.dart' as review_lib;
import 'package:docflow/engine/undo.dart';
import 'package:docflow/engine/operations/process_scans.dart';
import 'package:docflow/engine/pdf.dart' as pdf;
import 'package:docflow/engine/rfi_code.dart';
import 'package:flutter_test/flutter_test.dart' hide find;
import 'package:path/path.dart' as p;

const String _fixtureRoot = 'test/fixtures';
const String _civ0064 = 'AZE-SUN-INT-QMS-RFI-CIV-0064';

const String _processOp = 'rfi.process_scans_fast_covers';

FilesystemAuthority _processAuthority(String journalDir) => FilesystemAuthority(
  journalDir: journalDir,
  ttl: const Duration(minutes: 5),
  ooxml: const ProductionFollowUpOoxml(),
  rfiLog: const ProductionRfiLogWorkbook(),
  rfiSystemSync: const RfiSystemSync(),
  pdf: const ProductionPdfEngine(),
  replicaPublish: const ProductionReplicaPublish(),
  undoJournal: undoJournal,
);

/// Issues read-write grants for [dirs] and caches [plan] bound to them.
Plan _cacheProcess(
  FilesystemAuthority authority,
  Plan plan,
  List<String> dirs,
) {
  final scope = GrantScope.create(
    'owner-1',
    'session-1',
    'batch-1',
    _processOp,
  );
  final ids = [
    for (final dir in dirs)
      authority.issueGrant(scope, dir, GrantAccess.readWrite).grantId,
  ];
  return authority.cachePlan(
    BackendOrigin.rust,
    PlanBinding(
      ownerId: 'owner-1',
      sessionId: 'session-1',
      operationId: _processOp,
      grantIds: ids,
    ),
    plan,
  );
}

const CallerIdentity _owner = CallerIdentity('owner-1', 'session-1');

// ---- Harness --------------------------------------------------------------

Directory _tempDir(String name) {
  final dir = Directory.systemTemp.createTempSync('e10-$name-');
  addTearDown(() {
    if (dir.existsSync()) {
      dir.deleteSync(recursive: true);
    }
  });
  return dir;
}

Directory _mkdir(String path) => Directory(path)..createSync(recursive: true);

/// Writes a text-only PDF with [pageCount] pages. Page `n` contains `PAGE n`.
/// Objects are ASCII, so string lengths equal byte offsets.
void _writeMultiPagePdf(
  String path,
  int pageCount, {
  int? pageRotate,
  int? inheritedRotate,
}) {
  const catalog = '<< /Type /Catalog /Pages 2 0 R >>';
  final kids = [for (var i = 0; i < pageCount; i++) '${i + 3} 0 R'].join(' ');
  final objects = <String>[
    catalog,
    '<< /Type /Pages /Kids [$kids] /Count $pageCount'
        '${inheritedRotate == null ? '' : ' /Rotate $inheritedRotate'} >>',
  ];
  for (var page = 0; page < pageCount; page++) {
    objects.add(
      '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] '
      '/Resources << /Font << /F1 ${pageCount + 3} 0 R >> >> '
      '/Contents ${pageCount + 4 + page} 0 R'
      '${pageRotate == null ? '' : ' /Rotate $pageRotate'} >>',
    );
  }
  objects.add('<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>');
  for (var page = 0; page < pageCount; page++) {
    final stream = 'BT /F1 18 Tf 36 700 Td (PAGE $page) Tj ET\n';
    objects.add('<< /Length ${stream.length} >>\nstream\n${stream}endstream');
  }
  final buffer = StringBuffer('%PDF-1.4\n');
  final offsets = <int>[];
  for (var index = 0; index < objects.length; index++) {
    offsets.add(buffer.length);
    buffer.write('${index + 1} 0 obj\n${objects[index]}\nendobj\n');
  }
  final xref = buffer.length;
  buffer.write('xref\n0 ${objects.length + 1}\n0000000000 65535 f \n');
  for (final offset in offsets) {
    buffer.write('${offset.toString().padLeft(10, '0')} 00000 n \n');
  }
  buffer.write(
    'trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\n'
    'startxref\n$xref\n%%EOF\n',
  );
  File(path).writeAsBytesSync(ascii.encode(buffer.toString()));
}

/// Tesseract executable for real-OCR cases, or null when unavailable.
final String? _tesseractPath = () {
  final configured = Platform.environment['DOCFLOW_TESSERACT'];
  for (final candidate in [?configured, '/opt/homebrew/bin/tesseract']) {
    if (File(candidate).existsSync()) {
      return candidate;
    }
  }
  return null;
}();

/// Writes a one-page searchable PDF whose text layer is [text].
void _writeTextPdf(String path, String text) {
  final stream = 'BT /F1 24 Tf 36 700 Td ($text) Tj ET\n';
  final objects = <String>[
    '<< /Type /Catalog /Pages 2 0 R >>',
    '<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] '
        '/Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>',
    '<< /Length ${stream.length} >>\nstream\n${stream}endstream',
    '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>',
  ];
  final buffer = StringBuffer('%PDF-1.4\n');
  final offsets = <int>[];
  for (var index = 0; index < objects.length; index++) {
    offsets.add(buffer.length);
    buffer.write('${index + 1} 0 obj\n${objects[index]}\nendobj\n');
  }
  final xref = buffer.length;
  buffer.write('xref\n0 ${objects.length + 1}\n0000000000 65535 f \n');
  for (final offset in offsets) {
    buffer.write('${offset.toString().padLeft(10, '0')} 00000 n \n');
  }
  buffer.write(
    'trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\n'
    'startxref\n$xref\n%%EOF\n',
  );
  File(path).writeAsBytesSync(ascii.encode(buffer.toString()));
}

/// Maps pdf.dart's inspection onto the Process Scans inspection contract.
final class _PdfInspectorBridge implements PdfInspector {
  const _PdfInspectorBridge();

  @override
  PdfInspection inspect(String path) {
    final facts = pdf.inspect(path);
    return PdfInspection(
      pageCount: facts.pageCount,
      pages: [
        for (final page in facts.pages) PdfPageFacts(rotation: page.rotation),
      ],
      signed: facts.signed,
      encrypted: facts.encrypted,
      extractionSupported: pdf.extractionSupported(facts),
    );
  }
}

ProcessScansParams _params({
  required String source,
  required String destination,
  bool keepOriginal = false,
  bool correctOrientation = false,
  String? trashRoot,
}) => ProcessScansParams(
  source: source,
  destination: destination,
  organizeIntoRfiFolders: true,
  keepOriginal: keepOriginal,
  correctOrientation: correctOrientation,
  trashRoot: trashRoot,
);

Future<Plan> _preview(ProcessScansParams params, OcrAdapter ocr) =>
    previewFastCovers(
      params: params,
      ocr: ocr,
      inspector: const _PdfInspectorBridge(),
    );

List<Step> _extracts(Plan plan) =>
    plan.steps.where((s) => s.kind == StepKind.pdfPagesExtract).toList();

List<Map<String, Object?>> _rotations(Step step) =>
    (step.meta['rotations']! as List).cast<Map<String, Object?>>();

Map<String, Object?> _rotation(Step step, int index) => _rotations(step)[index];

bool _hasPathComponent(String path, String expected) =>
    path.replaceAll(r'\', '/').split('/').contains(expected);

// ---- OCR fixtures ---------------------------------------------------------

/// Reads each page from a fixture list. Marker strings select the shapes the
/// Rust `PageFixtureOcr` produces.
final class _PageFixtureOcr extends OcrAdapter {
  const _PageFixtureOcr(this.pages);

  final List<String> pages;

  @override
  Future<OcrRead> read(String path) => readPage(path, 0);

  @override
  bool get supportsPageScan => true;

  @override
  Future<OcrRead> readPage(String path, int pageIndex) async {
    if (pageIndex >= pages.length) {
      throw const OcrDocumentFailure('fixture page index out of range');
    }
    final text = pages[pageIndex];
    if (text == '[unreadable]') {
      throw const OcrDocumentFailure('fixture page is unreadable');
    }
    return switch (text) {
      '[ambiguous]' => OcrRead(
        matched: find(''),
        source: 'ocr:ambiguous-or-unconfirmed:test',
      ),
      '[low-confidence]' => OcrRead(
        matched: const RfiMatch(
          code: 'AZE-SUN-INT-QMS-RFI-CIV-0065',
          discipline: 'CIV',
          number: '0065',
          method: RfiMethod.strict,
          confidence: 0.79,
        ),
        source: 'page-fixture',
      ),
      '[repaired]' => OcrRead(
        matched: const RfiMatch(
          code: 'AZE-SUN-INT-QMS-RFI-CIV-0066',
          discipline: 'CIV',
          number: '0066',
          method: RfiMethod.repaired,
          confidence: 0.9,
        ),
        source: 'page-fixture',
      ),
      _ => OcrRead(matched: find(text), source: 'page-fixture'),
    };
  }

  @override
  String diagnostics() => 'page fixture OCR';
}

/// File-aware fixture for Rust-style Preview tests: `broken.pdf` is
/// unreadable and every other file reads as CIV-0064.
final class _FileFixtureOcr extends OcrAdapter {
  const _FileFixtureOcr();

  @override
  Future<OcrRead> read(String path) => readPage(path, 0);

  @override
  bool get supportsPageScan => true;

  @override
  bool get supportsParallelAnalysis => true;

  @override
  Future<OcrRead> readPage(String path, int pageIndex) async {
    if (p.basename(path) == 'broken.pdf') {
      throw const OcrDocumentFailure('cannot open broken document');
    }
    return OcrRead(
      matched: find('AZE-SUN-INT-QMS-RFI-CIV-0064'),
      source: 'fixture-text-layer',
    );
  }

  @override
  String diagnostics() => 'file fixture OCR';
}

/// Orientation fixture mirroring Rust `OrientationFixtureOcr`: every read
/// matches CIV-0064, and detection returns the configured clockwise rotation.
final class _OrientationOcr extends OcrAdapter implements OrientationDetector {
  const _OrientationOcr({
    required this.uncertain,
    required this.rotations,
    this.coverPage = 0,
  });

  final bool uncertain;
  final List<int> rotations;
  final int coverPage;

  @override
  Future<OcrRead> read(String path) async => OcrRead(
    matched: find('AZE-SUN-INT-QMS-RFI-CIV-0064'),
    source: 'orientation-fixture',
  );

  @override
  bool get supportsPageScan => true;

  @override
  Future<OcrRead> readPage(String path, int pageIndex) async =>
      pageIndex == coverPage
      ? read(path)
      : OcrRead(matched: find(''), source: 'orientation-fixture-continuation');

  @override
  bool get supportsOrientation => true;

  @override
  Future<OrientationDetection> detectPage(String path, int pageIndex) async {
    if (uncertain) {
      throw const OrientationError('fixture orientation is uncertain');
    }
    return OrientationDetection(
      rotateClockwise: pageIndex < rotations.length ? rotations[pageIndex] : 90,
      confidence: 15.0,
      method: 'fixture',
    );
  }

  @override
  Future<OrientationDetection> detectOrientation(String path, int pageIndex) =>
      detectPage(path, pageIndex);

  @override
  String orientationDiagnostics() => 'orientation fixture OCR';

  @override
  String diagnostics() => 'orientation fixture OCR';
}

/// Replaces [target] with [bytes] during the first OCR read, like the Rust
/// `ReplacingOrientationOcr` fixture.
final class _ReplacingOcr extends OcrAdapter {
  _ReplacingOcr(this.target, this.bytes);

  final String target;
  final List<int> bytes;
  bool _replaced = false;

  @override
  Future<OcrRead> read(String path) async {
    if (!_replaced) {
      _replaced = true;
      File(target).writeAsBytesSync(bytes);
    }
    return OcrRead(
      matched: find('AZE-SUN-INT-QMS-RFI-CIV-0064'),
      source: 'replacing-fixture',
    );
  }

  @override
  bool get supportsPageScan => true;

  @override
  bool get supportsParallelAnalysis => true;

  @override
  bool get supportsOrientation => true;

  @override
  Future<OcrRead> readPage(String path, int pageIndex) => read(path);

  @override
  Future<OrientationDetection> detectOrientation(
    String path,
    int pageIndex,
  ) async => const OrientationDetection(
    rotateClockwise: 0,
    confidence: 15.0,
    method: 'fixture',
  );

  @override
  String diagnostics() => 'replacing fixture OCR';
}

/// Review fixture: every scanned file reads as CIV-0064.
final class _ReviewFixtureOcr implements review_lib.ReviewOcrAdapter {
  const _ReviewFixtureOcr();

  @override
  Future<review_lib.ReviewOcrRead> read(String path) async =>
      review_lib.ReviewOcrRead(
        matched: find(_civ0064),
        source: 'review-fixture',
      );
}

/// Every read fails with an OCR engine that is not installed.
final class _MissingOcr extends OcrAdapter {
  const _MissingOcr();

  @override
  Future<OcrRead> read(String path) => Future.error(const OcrEngineMissing());

  @override
  String diagnostics() => 'tesseract: unavailable';
}

/// Reads one fixed match for every file, as the Rust `LowConfidenceOcr` does.
final class _FixedOcr extends OcrAdapter {
  const _FixedOcr(this.matched, this.source);

  final RfiMatch matched;
  final String source;

  @override
  Future<OcrRead> read(String path) async =>
      OcrRead(matched: matched, source: source);

  @override
  String diagnostics() => 'fixture OCR available';
}

/// Fails with a document error for `broken.pdf`, as the Rust `FixtureOcr`.
final class _BrokenDocumentOcr extends OcrAdapter {
  const _BrokenDocumentOcr();

  @override
  Future<OcrRead> read(String path) async {
    if (p.basename(path) == 'broken.pdf') {
      throw const OcrDocumentFailure('cannot open broken document');
    }
    return OcrRead(matched: find(_civ0064), source: 'fixture-text-layer');
  }

  @override
  String diagnostics() => 'fixture OCR available';
}

// ---- Tests ----------------------------------------------------------------

void main() {
  group('RFI matching (no OCR engine)', () {
    test('preserves NFC uppercase and the strict anchor', () {
      final strict = find(
        'REQUEST FOR INSPECTION 18.02.2026 REI N0 '
        'AZE-SUN-INT-QMS-ITP-MEC-0001 ... '
        'AZE-SUN-INT-ENG-STR-DRG-0901 ... '
        'aze-sun-int-qms-rfi-mec-0107',
      );
      expect(strict.code, 'AZE-SUN-INT-QMS-RFI-MEC-0107');
      expect(strict.method, RfiMethod.strict);
      expect(strict.confidence, 1.0);

      for (final damaged in const [
        'AZE-SUN-INT-QMS-RFI-MEC-O1O7',
        'AZE-SUN-INT-QMS-RFI-MEC-Ol07',
        '4ZE-SUN-1NT-QMS-RFI-MEC-0107',
      ]) {
        final repaired = find(damaged);
        expect(repaired.code, 'AZE-SUN-INT-QMS-RFI-MEC-0107', reason: damaged);
        expect(repaired.method, RfiMethod.repaired, reason: damaged);
      }
      final twoLetterDiscipline = find('AZE-SUN-INT-QMS-RFI-CV-0064');
      expect(
        twoLetterDiscipline.method,
        RfiMethod.miss,
        reason: 'two-letter CV must not be corrected',
      );
      expect(twoLetterDiscipline.ok, isFalse);
      final fuzzy = find('AZE-SUN-INT-QMX-RFI-MEC-0107');
      expect(fuzzy.method, RfiMethod.fuzzy);
      expect(fuzzy.confidence, 0.9);
      expect(find('AZE-SUN-INT-QMS-ITP-MEC-0001').ok, isFalse);
      expect(find('AZE-SUN-INT-ENG-STR-DRG-0901 S-205').ok, isFalse);
      expect(canonical('İNT'), 'İNT');
    });

    test(
      'accepts only supported disciplines and does not repair discipline',
      () {
        for (final discipline in const ['CIV', 'MEC', 'ELE', 'PRC']) {
          final code = 'AZE-SUN-INT-QMS-RFI-$discipline-1234';
          expect(find(code).method, RfiMethod.strict, reason: code);
          expect(exactCodes(code), contains(code));
        }
        for (final discipline in const [
          'ARC',
          'INS',
          'PIP',
          'STR',
          'TEL',
          'CV',
          'CLV',
        ]) {
          final code = 'AZE-SUN-INT-QMS-RFI-$discipline-1234';
          expect(find(code).method, RfiMethod.miss, reason: code);
          expect(exactCodes(code), isEmpty, reason: code);
        }
      },
    );

    test('recovers a number wrapped after the final anchor hyphen', () {
      final wrapped = find('RFI No: AZE-SUN-INT-QMS-RFI-CIV-\n0064');
      final blankLine = find('RFI No: AZE-SUN-INT-QMS-RFI-CIV-\r\n \r\n0064');

      expect(wrapped.code, _civ0064);
      expect(wrapped.method, RfiMethod.strict);
      expect(wrapped.confidence, 1.0);
      expect(blankLine.code, wrapped.code);
      expect(
        find('AZE-SUN-INT-QMS-RFI-CIV-\nRemarks 0064').ok,
        isFalse,
        reason: 'unrelated text must not be joined into a false RFI label',
      );
      expect(
        find('AZE-SUN-INT-QMS-RFI-CIV-\n\n\n0064').ok,
        isFalse,
        reason: 'wrapped recovery must not cross more than one blank line',
      );
      for (final malformed in const [
        'AZE-SUN-INT-QMS-RFI-CIV-00645',
        'AZE-SUN-INT-QMS-RFI-CIV-٠٠٦٤',
        'AZE-SUN-INT-QMS-RFI-CIV-0064٥',
        'AZE-SUN-INT-QMS-RFI-CIV-\n00645',
      ]) {
        expect(
          find(malformed).ok,
          isFalse,
          reason: 'RFI numbers must be exactly four ASCII digits: $malformed',
        );
      }
      expect(
        find('AZE-SUN-INT-QMS-RFI-CIV-0064REQUEST').code,
        _civ0064,
        reason: 'OCR-concatenated trailing letters remain valid',
      );
      expect(
        find('AZE-SUN-INT-QMS-RFI-CIV-00645 AZE-SUN-INT-QMS-RFI-MEC-0107').code,
        'AZE-SUN-INT-QMS-RFI-MEC-0107',
        reason: 'a malformed candidate must not hide a later valid code',
      );
    });

    test('accepts bounded line wraps between exact code segments', () {
      for (final wrapped in const [
        'AZE-SUN-INT-QMS-RFI-\nMEC-0765',
        'AZE-SUN-INT-QMS-RFI-MEC\n-0765',
        'AZE-SUN-INT-QMS-\r\nRFI-MEC-0765',
        'AZE - SUN-INT-QMS-RFI-MEC-0765',
      ]) {
        final matched = find(wrapped);
        expect(matched.code, 'AZE-SUN-INT-QMS-RFI-MEC-0765', reason: wrapped);
        expect(matched.method, RfiMethod.strict, reason: wrapped);
      }
      expect(
        find('AZE-SUN-INT-QMS-\n\nRFI-\nMEC-0765').ok,
        isFalse,
        reason: 'exact recovery must not join more than two line breaks',
      );
    });

    test('canonical and find expose the single normalized value', () {
      final (normalized, matched) = canonicalAndFind(
        'aze-sun-int-qms-rfi-civ-0064',
      );
      expect(normalized, _civ0064);
      expect(matched.code, normalized);
      expect(matched.method, RfiMethod.strict);
    });

    test('shared RFI and naming fixture matches the compatibility oracle', () {
      final fixture = jsonDecode(
        File(p.join(_fixtureRoot, 'slice5_rfi_naming.json')).readAsStringSync(),
      ) as Map<String, Object?>;

      const methodNames = {
        RfiMethod.strict: 'strict',
        RfiMethod.repaired: 'repaired',
        RfiMethod.ocrAlias: 'ocr_alias',
        RfiMethod.fuzzy: 'fuzzy',
        RfiMethod.miss: 'miss',
      };
      for (final raw in fixture['rfiCases']! as List<Object?>) {
        final c = raw! as Map<String, Object?>;
        final name = c['name'];
        final matched = find(c['input']! as String);
        expect(matched.code, c['code'], reason: '$name');
        expect(matched.discipline, c['discipline'], reason: '$name');
        expect(matched.number, c['number'], reason: '$name');
        expect(methodNames[matched.method], c['method'], reason: '$name');
        expect(
          matched.confidence,
          (c['confidence']! as num).toDouble(),
          reason: '$name',
        );
      }

      const root = 'fixture-root';
      for (final raw in fixture['namingCases']! as List<Object?>) {
        final c = raw! as Map<String, Object?>;
        final code = c['code']! as String;
        expect(filenameFor(code), c['filename']);
        expect(folderFor(root, code), p.join(root, c['folder']! as String));
      }
    });
  });

  group('fast-cover Preview (fixture OCR, real PDFs)', () {
    test(
      'uncertain pages between confirmed covers stay with the current RFI',
      () async {
        final dir = _tempDir('uncertain');
        final source = _mkdir(p.join(dir.path, 'source')).path;
        final destination = _mkdir(p.join(dir.path, 'destination')).path;
        _writeMultiPagePdf(p.join(source, 'uncertain.pdf'), 4);

        final plan = await _preview(
          _params(source: source, destination: destination, keepOriginal: true),
          _PageFixtureOcr([
            _civ0064,
            '[ambiguous]',
            '',
            'AZE-SUN-INT-QMS-RFI-MEC-0010',
          ]),
        );

        final extracted = _extracts(plan);
        expect(extracted, hasLength(2));
        expect(extracted[0].meta['pageIndexes'], [0, 1, 2]);
        expect(extracted[1].meta['pageIndexes'], [3]);
        expect(plan.warnings, isEmpty);
        for (final step in extracted) {
          expect(step.dst, isNot(contains('_review')));
        }
      },
    );

    test(
      'long continuation run is split only at the next confirmed cover',
      () async {
        final dir = _tempDir('long-run');
        final source = _mkdir(p.join(dir.path, 'source')).path;
        final destination = _mkdir(p.join(dir.path, 'destination')).path;
        _writeMultiPagePdf(p.join(source, 'batch.pdf'), 25);

        final pages = <String>[
          _civ0064,
          for (var index = 0; index < 22; index++)
            switch (index % 3) {
              0 => '[ambiguous]',
              1 => '[low-confidence]',
              _ => '[repaired]',
            },
          'AZE-SUN-INT-QMS-RFI-MEC-0010',
          '[unreadable]',
        ];

        final plan = await _preview(
          _params(source: source, destination: destination, keepOriginal: true),
          _PageFixtureOcr(pages),
        );

        final extracted = _extracts(plan);
        expect(extracted, hasLength(2));
        expect(extracted[0].meta['pageIndexes'], [
          for (var i = 0; i < 23; i++) i,
        ]);
        expect(extracted[1].meta['pageIndexes'], [23, 24]);
        expect(plan.warnings, isEmpty);
      },
    );

    test('pages before the first confirmed cover remain in Review', () async {
      final dir = _tempDir('leading');
      final source = _mkdir(p.join(dir.path, 'source')).path;
      final destination = _mkdir(p.join(dir.path, 'destination')).path;
      _writeMultiPagePdf(p.join(source, 'batch.pdf'), 3);

      final plan = await _preview(
        _params(source: source, destination: destination, keepOriginal: true),
        _PageFixtureOcr(['[ambiguous]', '', 'AZE-SUN-INT-QMS-RFI-MEC-0010']),
      );

      final extracted = _extracts(plan);
      expect(extracted, hasLength(2));
      final cover = extracted.singleWhere(
        (s) => _listEquals(s.meta['pageIndexes'], [2]),
      );
      final leadingReview = extracted.singleWhere(
        (s) => _listEquals(s.meta['pageIndexes'], [0, 1]),
      );
      expect(cover.dst, isNot(contains('_review')));
      expect(leadingReview.dst, contains('_review'));
    });

    test(
      'combined scan Preview splits ordered RFIs into discipline folders',
      () async {
        final dir = _tempDir('combined');
        final source = _mkdir(p.join(dir.path, 'source')).path;
        final destination = _mkdir(p.join(dir.path, 'destination')).path;
        final trash = _mkdir(p.join(dir.path, 'app-data', 'trash')).path;
        final combined = p.join(source, 'batch.pdf');
        _writeMultiPagePdf(combined, 5);

        final plan = await _preview(
          _params(source: source, destination: destination, trashRoot: trash),
          _PageFixtureOcr([
            _civ0064,
            '',
            'AZE-SUN-INT-QMS-RFI-MEC-0072',
            'AZE-SUN-INT-QMS-RFI-ELE-0080',
            'AZE-SUN-INT-QMS-RFI-PRC-0091',
          ]),
        );

        expect(
          File(combined).existsSync(),
          isTrue,
          reason: 'Preview must not move the input',
        );
        expect(_mkdir(destination).listSync(), isEmpty);
        final extracts = _extracts(plan);
        expect(extracts, hasLength(4));
        expect(extracts[0].meta['pageIndexes'], [0, 1]);
        expect(_hasPathComponent(extracts[0].dst!, 'CIV'), isTrue);
        expect(_hasPathComponent(extracts[1].dst!, 'MEC'), isTrue);
        expect(_hasPathComponent(extracts[2].dst!, 'ELE'), isTrue);
        expect(_hasPathComponent(extracts[3].dst!, 'PRC'), isTrue);
      },
    );

    test('keep original copies low-confidence files into Review', () async {
      final dir = _tempDir('low-confidence');
      final source = _mkdir(p.join(dir.path, 'source')).path;
      final destination = _mkdir(p.join(dir.path, 'destination')).path;
      File(p.join(source, 'low-confidence.pdf'))
          .writeAsBytesSync(ascii.encode('PDF'));

      final plan = await _preview(
        _params(source: source, destination: destination, keepOriginal: true),
        const _FixedOcr(
          RfiMatch(
            code: _civ0064,
            discipline: 'CIV',
            number: '0064',
            method: RfiMethod.fuzzy,
            confidence: 0.79,
          ),
          'fixture-low-confidence',
        ),
      );

      expect(plan.warnings.first.key, 'warn.low_confidence');
      expect(plan.steps, hasLength(1));
      expect(plan.steps.single.kind, StepKind.copy);
      expect(plan.steps.single.meta['confidence'], 0.79);
      expect(plan.steps.single.meta['source'], 'fixture-low-confidence');
    });

    test('keep original copies OCR document errors into Review', () async {
      final dir = _tempDir('broken');
      final source = _mkdir(p.join(dir.path, 'source')).path;
      final destination = _mkdir(p.join(dir.path, 'destination')).path;
      File(p.join(source, 'broken.pdf')).writeAsBytesSync(ascii.encode('PDF'));

      final plan = await _preview(
        _params(source: source, destination: destination, keepOriginal: true),
        const _BrokenDocumentOcr(),
      );

      expect(plan.warnings.first.key, 'warn.unreadable');
      expect(plan.steps, hasLength(1));
      expect(plan.steps.single.kind, StepKind.copy);
    });

    test(
      'missing OCR engine returns one diagnostic and a zero-step plan',
      () async {
        final dir = _tempDir('no-ocr');
        final source = _mkdir(p.join(dir.path, 'source')).path;
        final destination = _mkdir(p.join(dir.path, 'destination')).path;
        File(p.join(source, 'a.pdf'))
            .writeAsBytesSync(ascii.encode('%PDF stub'));
        File(p.join(source, 'b.PDF'))
            .writeAsBytesSync(ascii.encode('%PDF stub'));

        final plan = await _preview(
          _params(source: source, destination: destination),
          const _MissingOcr(),
        );

        expect(plan.steps, isEmpty);
        expect(plan.warnings, hasLength(1));
        expect(plan.warnings.single.key, 'warn.no_ocr');
        expect(
          plan.warnings.single.subject,
          contains('tesseract: unavailable'),
        );
        expect(File(p.join(source, 'a.pdf')).existsSync(), isTrue);
        expect(File(p.join(source, 'b.PDF')).existsSync(), isTrue);
        expect(_mkdir(destination).listSync(), isEmpty);
      },
    );
  });

  group('apply and undo (FilesystemAuthority)', () {
    test(
      'combined scan splits RFIs into discipline folders and undoes',
      () async {
        final root = _tempDir('combined');
        final source = _mkdir(p.join(root.path, 'source'));
        final destination = _mkdir(p.join(root.path, 'destination'));
        final trash = _mkdir(p.join(root.path, 'app-data', 'trash'));
        final journal = _mkdir(p.join(root.path, 'journal'));
        final combined = p.join(source.path, 'batch.pdf');
        _writeMultiPagePdf(combined, 5);
        final codes = [
          'AZE-SUN-INT-QMS-RFI-CIV-0064',
          '',
          'AZE-SUN-INT-QMS-RFI-MEC-0072',
          'AZE-SUN-INT-QMS-RFI-ELE-0080',
          'AZE-SUN-INT-QMS-RFI-PRC-0091',
        ];
        final plan = await _preview(
          _params(
            source: source.path,
            destination: destination.path,
            trashRoot: trash.path,
          ),
          _PageFixtureOcr(codes),
        );
        expect(File(combined).existsSync(), isTrue, reason: 'preview is inert');
        expect(destination.listSync(), isEmpty);
        final extracts = _extracts(plan);
        expect(extracts, hasLength(4));
        expect(extracts[0].meta['pageIndexes'], [0, 1]);
        expect(_hasPathComponent(extracts[0].dst!, 'CIV'), isTrue);
        expect(_hasPathComponent(extracts[1].dst!, 'MEC'), isTrue);
        expect(_hasPathComponent(extracts[2].dst!, 'ELE'), isTrue);
        expect(_hasPathComponent(extracts[3].dst!, 'PRC'), isTrue);

        final authority = _processAuthority(journal.path);
        final cached = _cacheProcess(authority, plan, [
          source.path,
          destination.path,
          trash.path,
        ]);
        final applied = authority.apply(cached.planId, _owner);
        expect(applied.failed, 0, reason: '${applied.errors}');
        expect(File(combined).existsSync(), isFalse);
        expect(trash.listSync(), hasLength(1));

        final trashed = trash.listSync().single.path;
        final sourceFacts = pdf.inspect(trashed);
        const expected = [
          ('CIV', 'AZE-SUN-INT-QMS-RFI-CIV-0064', [0, 1]),
          ('MEC', 'AZE-SUN-INT-QMS-RFI-MEC-0072', [2]),
          ('ELE', 'AZE-SUN-INT-QMS-RFI-ELE-0080', [3]),
          ('PRC', 'AZE-SUN-INT-QMS-RFI-PRC-0091', [4]),
        ];
        for (final (discipline, code, indexes) in expected) {
          final output = p.join(
            destination.path,
            discipline,
            code,
            filenameFor(code),
          );
          final facts = pdf.inspect(output);
          expect(facts.pageCount, indexes.length);
          for (var i = 0; i < indexes.length; i++) {
            expect(
              facts.pages[i].contentHash,
              sourceFacts.pages[indexes[i]].contentHash,
            );
          }
        }

        final undone = authority.undo(applied.journalId, _owner);
        expect(undone.failed, 0, reason: '${undone.errors}');
        expect(File(combined).existsSync(), isTrue);
        expect(trash.listSync(), isEmpty);
        expect(destination.listSync(), isEmpty);
      },
    );

    test('apply cancellation is observed before the first mutation', () async {
      final root = _tempDir('cancel');
      final source = _mkdir(p.join(root.path, 'source'));
      final destination = _mkdir(p.join(root.path, 'destination'));
      final scan = p.join(source.path, 'scan.pdf');
      File(scan).writeAsBytesSync(ascii.encode('SOURCE'));
      final plan = await _preview(
        _params(source: source.path, destination: destination.path),
        const _PageFixtureOcr(['']),
      );
      final authority = _processAuthority(p.join(root.path, 'journal'));
      final cached = _cacheProcess(authority, plan, [
        source.path,
        destination.path,
      ]);
      final result = authority.applyWithProgress(cached.planId, _owner, (
        event,
      ) {
        if (event.phase == 'apply' && event.done == 0) {
          expect(authority.cancel(cached.planId), isTrue);
        }
      });
      expect(result.applied, 0);
      expect(result.failed, 0);
      expect(result.meta['effectState'], 'cancelled_clean');
      expect(File(scan).existsSync(), isTrue);
      expect(destination.listSync(), isEmpty);
    });

    test(
      'preview, apply, journal and undo keep duplicates and failures',
      () async {
        final root = _tempDir('dupes');
        final source = _mkdir(p.join(root.path, 'source'));
        final destination = _mkdir(p.join(root.path, 'destination'));
        final journal = p.join(root.path, 'journal');
        _writeMultiPagePdf(p.join(source.path, 'older.pdf'), 1);
        final olderBytes = File(p.join(source.path, 'older.pdf'))
            .readAsBytesSync();
        await Future<void>.delayed(const Duration(milliseconds: 20));
        _writeMultiPagePdf(p.join(source.path, 'newest.pdf'), 1);
        final newestBytes = File(p.join(source.path, 'newest.pdf'))
            .readAsBytesSync();
        File(p.join(source.path, 'broken.pdf'))
            .writeAsBytesSync(ascii.encode('BROKEN'));

        final plan = await previewFastCovers(
          params: ProcessScansParams(
            source: source.path,
            destination: destination.path,
            organizeIntoRfiFolders: true,
            keepOriginal: false,
            correctOrientation: false,
          ),
          ocr: const _FileFixtureOcr(),
          inspector: const _PdfInspectorBridge(),
        );
        expect(plan.warnings.map((w) => w.key).toList(), [
          'warn.unreadable',
          'warn.duplicate',
        ]);
        expect(File(p.join(source.path, 'newest.pdf')).existsSync(), isTrue);
        expect(
          Directory(p.join(destination.path, '_review')).existsSync(),
          isFalse,
        );

        final authority = _processAuthority(journal);
        final cached = _cacheProcess(authority, plan, [
          source.path,
          destination.path,
        ]);
        final applied = authority.apply(cached.planId, _owner);
        expect(applied.applied, plan.steps.length);
        expect(applied.failed, 0, reason: '${applied.errors}');
        final code = 'AZE-SUN-INT-QMS-RFI-CIV-0064';
        expect(
          File(p.join(destination.path, 'CIV', code, filenameFor(code)))
              .readAsBytesSync(),
          newestBytes,
        );
        expect(
          File(p.join(destination.path, '_review', 'older.pdf'))
              .readAsBytesSync(),
          olderBytes,
        );
        expect(
          File(p.join(destination.path, '_review', 'broken.pdf'))
              .readAsBytesSync(),
          ascii.encode('BROKEN'),
        );

        final undone = authority.undo(applied.journalId, _owner);
        expect(undone.failed, 0, reason: '${undone.errors}');
        expect(
          File(p.join(source.path, 'older.pdf')).readAsBytesSync(),
          olderBytes,
        );
        expect(
          File(p.join(source.path, 'newest.pdf')).readAsBytesSync(),
          newestBytes,
        );
        expect(
          File(p.join(source.path, 'broken.pdf')).readAsBytesSync(),
          ascii.encode('BROKEN'),
        );
        expect(destination.listSync(), isEmpty);
      },
    );

    test('apply never clobbers a destination created after Preview', () async {
      final root = _tempDir('clobber');
      final source = _mkdir(p.join(root.path, 'source'));
      final destination = _mkdir(p.join(root.path, 'destination'));
      final scan = p.join(source.path, 'scan.pdf');
      _writeMultiPagePdf(scan, 1);
      final sourceBytes = File(scan).readAsBytesSync();
      final plan = await previewFastCovers(
        params: ProcessScansParams(
          source: source.path,
          destination: destination.path,
          organizeIntoRfiFolders: false,
          keepOriginal: false,
          correctOrientation: false,
        ),
        ocr: const _PageFixtureOcr(['AZE-SUN-INT-QMS-RFI-CIV-0064']),
        inspector: const _PdfInspectorBridge(),
      );
      final target = p.join(
        destination.path,
        'CIV',
        filenameFor('AZE-SUN-INT-QMS-RFI-CIV-0064'),
      );
      _mkdir(p.dirname(target));
      File(target).writeAsBytesSync(ascii.encode('USER DATA'));

      final authority = _processAuthority(p.join(root.path, 'journal'));
      final cached = _cacheProcess(authority, plan, [
        source.path,
        destination.path,
      ]);
      final result = authority.apply(cached.planId, _owner);
      expect(
        result.applied,
        1,
        reason: '${result.errors} ${plan.steps.map((s) => s.kind).toList()}',
      );
      expect(result.failed, 1);
      expect(File(target).readAsBytesSync(), ascii.encode('USER DATA'));
      expect(File(scan).readAsBytesSync(), sourceBytes);
    });
  });

  group('orientation (fixture detector, real PDFs)', () {
    test('confident orientation is added to the cached plan', () async {
      final root = _tempDir('orient-confident');
      final source = _mkdir(p.join(root.path, 'source'));
      final destination = _mkdir(p.join(root.path, 'destination'));
      _writeMultiPagePdf(p.join(source.path, 'batch.pdf'), 1);
      final plan = await previewFastCovers(
        params: ProcessScansParams(
          source: source.path,
          destination: destination.path,
          organizeIntoRfiFolders: true,
          keepOriginal: true,
          correctOrientation: true,
        ),
        ocr: const _OrientationOcr(uncertain: false, rotations: [90]),
        inspector: const _PdfInspectorBridge(),
      );
      final extraction = _extracts(plan).single;
      expect(_rotation(extraction, 0)['pageIndex'], 0);
      expect(_rotation(extraction, 0)['targetRotation'], 90);
      expect(_rotation(extraction, 0)['confidence'], 15.0);
      expect(extraction.meta['sourceSha256'], isA<String>());
    });

    test('uncertain orientation routes the file to Review unchanged', () async {
      final root = _tempDir('orient-uncertain');
      final source = _mkdir(p.join(root.path, 'source'));
      final destination = _mkdir(p.join(root.path, 'destination'));
      final pdfPath = p.join(source.path, 'batch.pdf');
      _writeMultiPagePdf(pdfPath, 1);
      final before = File(pdfPath).readAsBytesSync();
      final plan = await previewFastCovers(
        params: ProcessScansParams(
          source: source.path,
          destination: destination.path,
          organizeIntoRfiFolders: true,
          keepOriginal: true,
          correctOrientation: true,
        ),
        ocr: const _OrientationOcr(uncertain: true, rotations: [90]),
        inspector: const _PdfInspectorBridge(),
      );
      expect(File(pdfPath).readAsBytesSync(), before);
      expect(
        plan.steps.any(
          (s) =>
              s.kind == StepKind.copy &&
              s.dst != null &&
              s.dst!.replaceAll(r'\', '/').endsWith('_review/batch.pdf'),
        ),
        isTrue,
      );
      expect(_extracts(plan), isEmpty);
    });

    test(
      'inherited negative rotation is normalized and keeps page indices',
      () async {
        final root = _tempDir('orient-inherited');
        final source = _mkdir(p.join(root.path, 'source'));
        final destination = _mkdir(p.join(root.path, 'destination'));
        _writeMultiPagePdf(
          p.join(source.path, 'batch.pdf'),
          2,
          inheritedRotate: -90,
        );
        final plan = await previewFastCovers(
          params: ProcessScansParams(
            source: source.path,
            destination: destination.path,
            organizeIntoRfiFolders: true,
            keepOriginal: true,
            correctOrientation: true,
          ),
          ocr: const _OrientationOcr(uncertain: false, rotations: [90, 270]),
          inspector: const _PdfInspectorBridge(),
        );
        final rotations = _rotations(_extracts(plan).single);
        expect(rotations, hasLength(2));
        expect(rotations[0]['pageIndex'], 0);
        expect(rotations[0]['targetRotation'], 0);
        expect(rotations[1]['pageIndex'], 1);
        expect(rotations[1]['targetRotation'], 180);
      },
    );

    test('noncardinal metadata is routed to Review', () async {
      final root = _tempDir('orient-noncardinal');
      final source = _mkdir(p.join(root.path, 'source'));
      final destination = _mkdir(p.join(root.path, 'destination'));
      _writeMultiPagePdf(p.join(source.path, 'batch.pdf'), 1, pageRotate: 45);
      final plan = await previewFastCovers(
        params: ProcessScansParams(
          source: source.path,
          destination: destination.path,
          organizeIntoRfiFolders: true,
          keepOriginal: true,
          correctOrientation: true,
        ),
        ocr: const _OrientationOcr(uncertain: false, rotations: [0]),
        inspector: const _PdfInspectorBridge(),
      );
      expect(
        plan.steps.any(
          (s) =>
              s.kind == StepKind.copy &&
              s.dst != null &&
              s.dst!.replaceAll(r'\', '/').endsWith('_review/batch.pdf'),
        ),
        isTrue,
      );
      expect(_extracts(plan), isEmpty);
    });

    test('source rotation is remapped to the extracted page index', () async {
      final root = _tempDir('orient-remap');
      final source = _mkdir(p.join(root.path, 'source'));
      final destination = _mkdir(p.join(root.path, 'destination'));
      final journal = p.join(root.path, 'journal');
      final pdfPath = p.join(source.path, 'batch.pdf');
      _writeMultiPagePdf(pdfPath, 5);
      final originalBytes = File(pdfPath).readAsBytesSync();
      final plan = await previewFastCovers(
        params: ProcessScansParams(
          source: source.path,
          destination: destination.path,
          organizeIntoRfiFolders: true,
          keepOriginal: true,
          correctOrientation: true,
        ),
        ocr: const _OrientationOcr(
          uncertain: false,
          rotations: [0, 0, 0, 90, 0],
          coverPage: 3,
        ),
        inspector: const _PdfInspectorBridge(),
      );
      final extraction = plan.steps.firstWhere(
        (s) =>
            s.kind == StepKind.pdfPagesExtract &&
            (s.meta['pageIndexes'] as List).join(',') == '3,4',
      );
      expect(_rotation(extraction, 0)['pageIndex'], 0);
      expect(_rotation(extraction, 0)['targetRotation'], 90);
      final output = extraction.dst!;

      final authority = _processAuthority(journal);
      final cached = _cacheProcess(authority, plan, [
        source.path,
        destination.path,
      ]);
      final applied = authority.apply(cached.planId, _owner);
      expect(applied.failed, 0, reason: '${applied.errors}');
      expect(File(pdfPath).readAsBytesSync(), originalBytes);
      final facts = pdf.inspect(output);
      expect(facts.pageCount, 2);
      expect(facts.pages[0].rotation, 90);
      expect(facts.pages[1].rotation, 0);

      final undone = authority.undo(applied.journalId, _owner);
      expect(undone.failed, 0, reason: '${undone.errors}');
      expect(File(output).existsSync(), isFalse);
      expect(File(pdfPath).existsSync(), isTrue);
    });

    test('overflow rotation is canonical and idempotent', () async {
      final root = _tempDir('orient-overflow');
      final source = _mkdir(p.join(root.path, 'source'));
      final destination = _mkdir(p.join(root.path, 'destination'));
      final pdfPath = p.join(source.path, 'batch.pdf');
      _writeMultiPagePdf(pdfPath, 1, pageRotate: 450);
      final before = File(pdfPath).readAsBytesSync();
      final canonical = File(pdfPath).resolveSymbolicLinksSync();
      final plan = await previewFastCovers(
        params: ProcessScansParams(
          source: source.path,
          destination: destination.path,
          organizeIntoRfiFolders: true,
          keepOriginal: true,
          correctOrientation: true,
        ),
        ocr: const _OrientationOcr(uncertain: false, rotations: [0]),
        inspector: const _PdfInspectorBridge(),
      );
      expect(File(pdfPath).readAsBytesSync(), before);
      expect(
        plan.steps.any(
          (s) =>
              (s.kind == StepKind.copy || s.kind == StepKind.move) &&
              s.src == canonical,
        ),
        isTrue,
      );
      expect(_extracts(plan), isEmpty);
    });
  });

  group('parallel analysis (fixture OCR)', () {
    test('source replaced during parallel OCR analysis is rejected', () async {
      final root = _tempDir('parallel-replace');
      final source = _mkdir(p.join(root.path, 'source'));
      final destination = _mkdir(p.join(root.path, 'destination'));
      final pdfPath = p.join(source.path, 'batch.pdf');
      _writeMultiPagePdf(pdfPath, 2);
      final replacement = p.join(root.path, 'replacement.pdf');
      _writeMultiPagePdf(replacement, 2, pageRotate: 0);
      final originalHash = File(pdfPath).readAsBytesSync();
      final replacementBytes = File(replacement).readAsBytesSync();
      await expectLater(
        previewFastCovers(
          params: ProcessScansParams(
            source: source.path,
            destination: destination.path,
            organizeIntoRfiFolders: true,
            keepOriginal: true,
            correctOrientation: true,
          ),
          ocr: _ReplacingOcr(pdfPath, replacementBytes),
          inspector: const _PdfInspectorBridge(),
        ),
        throwsA(
          isA<BackendError>().having((e) => e.code, 'code', 'path_invalid'),
        ),
      );
      expect(File(pdfPath).readAsBytesSync(), isNot(originalHash));
      expect(pdf.inspect(pdfPath).pageCount, 2);
    });

    test('preview uses bounded parallel analysis when supported', () async {
      final root = _tempDir('parallel-bounded');
      final source = _mkdir(p.join(root.path, 'source'));
      final destination = _mkdir(p.join(root.path, 'destination'));
      _writeMultiPagePdf(p.join(source.path, 'first.pdf'), 1);
      _writeMultiPagePdf(p.join(source.path, 'second.pdf'), 1, pageRotate: 0);
      final progress = <String>[];
      final plan = await previewFastCovers(
        params: ProcessScansParams(
          source: source.path,
          destination: destination.path,
          organizeIntoRfiFolders: true,
          keepOriginal: true,
          correctOrientation: false,
        ),
        ocr: const _FileFixtureOcr(),
        inspector: const _PdfInspectorBridge(),
        progress: (event) => progress.add(event.phase),
      );
      expect(plan.meta['parallelAnalysis'], isTrue);
      expect(plan.meta['workerCount'], greaterThanOrEqualTo(1));
      expect(plan.steps.where((s) => s.kind == StepKind.copy), hasLength(2));
      expect(progress, contains('scan_discovery'));
      expect(progress, contains('scan_ocr'));
      expect(progress, contains('scan_planning'));
      expect(progress.last, 'preview_ready');
    });
  });

  group('native OCR adapter (searchable text and real Tesseract)', () {
    test('searchable text alone never authorizes an RFI label', () async {
      final root = _tempDir('native-searchable');
      final pdfPath = p.join(root.path, 'searchable.pdf');
      _writeTextPdf(
        pdfPath,
        'REQUEST FOR INSPECTION AZE-SUN-INT-QMS-RFI-CIV-0064 searchable text layer',
      );
      final adapter = NativeOcr.withTesseract('/path/that/must/not/be-needed');
      await expectLater(
        adapter.read(pdfPath),
        throwsA(isA<OcrEngineMissing>()),
      );
      await expectLater(
        adapter.readCoverPage(pdfPath, 0),
        throwsA(isA<OcrEngineMissing>()),
      );
    });

    test('searchable-text decoys never become false RFI labels', () async {
      final root = _tempDir('native-decoys');
      final pdfPath = p.join(root.path, 'decoys.pdf');
      _writeTextPdf(
        pdfPath,
        'AZE-SUN-INT-QMS-ITP-MEC-0001 AZE-SUN-INT-ENG-STR-DRG-0901 decoy text only',
      );
      await expectLater(
        NativeOcr.withTesseract('/missing/tesseract').read(pdfPath),
        throwsA(isA<OcrEngineMissing>()),
      );
    });

    test(
      'image-only PDF runs through the platform renderer and real Tesseract',
      () async {
        final root = _tempDir('native-real-ocr');
        final searchable = p.join(root.path, 'source.pdf');
        final raster = p.join(root.path, 'first-page.png');
        final imageOnly = p.join(root.path, 'image-only.pdf');
        _writeTextPdf(
          searchable,
          'AZE-SUN-INT-QMS-RFI-CIV-0064 REQUEST FOR INSPECTION NATIVE OCR FIXTURE',
        );
        await renderFirstPagePng(searchable, raster, 300);
        imagePdfFromPng(raster, imageOnly);
        final read = await NativeOcr.withTesseract(_tesseractPath!)
            .read(imageOnly);
        expect(read.matched.code, 'AZE-SUN-INT-QMS-RFI-CIV-0064');
        expect(read.matched.method, RfiMethod.strict);
        expect(read.source, startsWith('ocr:consensus:'));
      },
      skip: _tesseractPath == null
          ? 'real OCR fixture skipped: local Tesseract is unavailable'
          : (Platform.isMacOS ? false : 'macOS renderer only'),
    );
  });

  group('fast-scan Preview revision', () {
    Future<(Directory, Directory, Plan)> _singleRfiPlan() async {
      final root = _tempDir('revision');
      final source = _mkdir(p.join(root.path, 'source'));
      final destination = _mkdir(p.join(root.path, 'destination'));
      final trash = _mkdir(p.join(root.path, 'trash'));
      _writeMultiPagePdf(p.join(source.path, 'scan.pdf'), 2);
      final plan = await previewFastCovers(
        params: ProcessScansParams(
          source: source.path,
          destination: destination.path,
          organizeIntoRfiFolders: true,
          keepOriginal: true,
          scanMode: ScanMode.singleRfi,
          correctOrientation: false,
          trashRoot: trash.path,
        ),
        ocr: const _PageFixtureOcr([
          'AZE-SUN-INT-QMS-RFI-CIV-0064',
          'AZE-SUN-INT-QMS-RFI-CIV-0064',
        ]),
        inspector: const _PdfInspectorBridge(),
      );
      return (source, destination, plan);
    }

    List<Map<String, Object?>> rowsOf(Plan plan) =>
        ((plan.meta['editor']! as Map)['rows']! as List)
            .cast<Map<String, Object?>>()
            .map((row) => Map<String, Object?>.of(row))
            .toList();

    test('preview revision rederives target from a valid code', () async {
      final (source, destination, plan) = await _singleRfiPlan();
      final original = plan.steps.firstWhere(
        (s) => s.noteKey == 'note.renamed',
      );
      final rows = rowsOf(plan);
      rows[0]['code'] = 'AZE-SUN-INT-QMS-RFI-MEC-0192';

      final revised = revise(plan, {'rows': rows});
      final output = revised.steps.firstWhere(
        (s) => s.noteKey == 'note.renamed',
      );
      expect(
        p.isWithin(destination.resolveSymbolicLinksSync(), output.dst!),
        isTrue,
      );
      expect(_hasPathComponent(output.dst!, 'MEC'), isTrue);
      expect(output.src, original.src);
      expect(output.meta['code'], 'AZE-SUN-INT-QMS-RFI-MEC-0192');
      expect(
        (((revised.meta['editor']! as Map)['rows']! as List).first
            as Map)['target'],
        output.dst,
      );
      expect(source.existsSync(), isTrue);
    });

    test(
      'revision rejects invalid codes, paths and target collisions',
      () async {
        final (source, destination, plan) = await _singleRfiPlan();
        final originalRows = rowsOf(plan);

        final invalidRows = rowsOf(plan);
        invalidRows[0]['code'] = 'AZE-SUN-INT-QMS-RFI-CLV-0192';
        expect(
          () => revise(plan, {'rows': invalidRows}),
          throwsA(
            isA<BackendError>().having(
              (e) => e.message,
              'message',
              'error.rfi.fastScan.codeInvalid',
            ),
          ),
        );

        final tamperRows = rowsOf(plan);
        tamperRows[0]['target'] = '/outside/attacker.pdf';
        expect(
          () => revise(plan, {'rows': tamperRows}),
          throwsA(
            isA<BackendError>().having(
              (e) => e.message,
              'message',
              'error.rfi.fastScan.revisionInvalid',
            ),
          ),
        );

        const code = 'AZE-SUN-INT-QMS-RFI-MEC-0192';
        final occupied = p.join(
          destination.resolveSymbolicLinksSync(),
          'MEC',
          code,
          filenameFor(code),
        );
        _mkdir(p.dirname(occupied));
        File(occupied).writeAsBytesSync(ascii.encode('pre-existing user file'));
        final collisionRows = originalRows;
        collisionRows[0]['code'] = code;
        expect(
          () => revise(plan, {'rows': collisionRows}),
          throwsA(
            isA<BackendError>().having(
              (e) => e.message,
              'message',
              'error.rfi.fastScan.targetExists',
            ),
          ),
        );
        expect(
          File(occupied).readAsBytesSync(),
          ascii.encode('pre-existing user file'),
        );
        expect(File(p.join(source.path, 'scan.pdf')).existsSync(), isTrue);
      },
    );
  });

  group('review decisions (inert plans, apply and undo)', () {
    test(
      'review scan and put/delete decisions are inert canonical plans',
      () async {
        final root = _tempDir('review-decisions');
        final destination = _mkdir(p.join(root.path, 'destination'));
        final review = _mkdir(p.join(destination.path, '_review'));
        final trash = _mkdir(p.join(root.path, 'app-data', 'trash'));
        final journal = p.join(root.path, 'journal');
        final putSource = p.join(review.path, 'put.pdf');
        final deleteSource = p.join(review.path, 'delete.pdf');
        File(putSource).writeAsBytesSync(ascii.encode('PUT'));
        File(deleteSource).writeAsBytesSync(ascii.encode('DELETE'));

        final rows = await review_lib.scan(
          destination.path,
          const _ReviewFixtureOcr(),
        );
        expect(rows, hasLength(2));
        expect(rows.every((r) => r.code == _civ0064), isTrue);

        final plan = review_lib.preview(destination.path, trash.path, [
          review_lib.ReviewDecision(
            path: putSource,
            action: review_lib.ReviewAction.put,
            code: 'aze-sun-int-qms-rfi-civ-0064',
          ),
          review_lib.ReviewDecision(
            path: deleteSource,
            action: review_lib.ReviewAction.delete,
            code: '',
          ),
        ], '20260825T020304000005');
        expect(plan.opId, 'rfi.review');
        expect(plan.steps.first.kind, StepKind.mkdir);
        expect(
          plan.steps.any(
            (s) => (s.dst ?? '').endsWith('RFI No. $_civ0064.pdf'),
          ),
          isTrue,
        );
        expect(plan.steps.any((s) => s.meta['action'] == 'delete'), isTrue);
        expect(
          File(putSource).existsSync() && File(deleteSource).existsSync(),
          isTrue,
          reason: 'preview is inert',
        );

        final authority = _processAuthority(journal);
        final scope = GrantScope.create(
          'owner-1',
          'session-1',
          'review-1',
          'rfi.review',
        );
        final ids = [
          authority
              .issueGrant(scope, destination.path, GrantAccess.readWrite)
              .grantId,
          authority
              .issueGrant(
                scope,
                p.join(root.path, 'app-data'),
                GrantAccess.readWrite,
              )
              .grantId,
        ];
        final cached = authority.cachePlan(
          BackendOrigin.rust,
          PlanBinding(
            ownerId: 'owner-1',
            sessionId: 'session-1',
            operationId: 'rfi.review',
            grantIds: ids,
          ),
          plan,
        );
        final applied = authority.apply(cached.planId, _owner);
        expect(applied.applied, 3);
        expect(applied.failed, 0, reason: '${applied.errors}');
        expect(
          File(putSource).existsSync() || File(deleteSource).existsSync(),
          isFalse,
        );
        expect(
          File(p.join(destination.path, _civ0064, 'RFI No. $_civ0064.pdf'))
              .existsSync(),
          isTrue,
        );
        expect(trash.listSync(), hasLength(1));

        final undone = authority.undo(applied.journalId, _owner);
        expect(undone.failed, 0, reason: '${undone.errors}');
        expect(
          File(putSource).existsSync() && File(deleteSource).existsSync(),
          isTrue,
        );
        expect(trash.listSync(), isEmpty);
        expect(
          Directory(p.join(destination.path, _civ0064)).existsSync(),
          isFalse,
        );
      },
    );
  });

  group('skipped: not portable in this repair', () {
    test(
      'operator process-scan PDF matches the expected exact RFI',
      () async {
        final path = Platform.environment['DOCFLOW_OPERATOR_PROCESS_SCAN_PDF']!;
        final expected =
            Platform.environment['DOCFLOW_OPERATOR_PROCESS_SCAN_RFI']!;
        final tesseract = Platform.environment['DOCFLOW_OPERATOR_TESSERACT'];
        final ocr = tesseract == null
            ? NativeOcr.discover()
            : NativeOcr.withTesseract(tesseract);
        final read = await ocr.read(path);
        expect(read.matched.code, expected, reason: read.source);
      },
      skip:
          Platform.environment['DOCFLOW_OPERATOR_PROCESS_SCAN_PDF'] == null ||
              Platform.environment['DOCFLOW_OPERATOR_PROCESS_SCAN_RFI'] == null
          ? 'requires an operator-supplied Process Scans PDF (set DOCFLOW_OPERATOR_PROCESS_SCAN_PDF and DOCFLOW_OPERATOR_PROCESS_SCAN_RFI)'
          : false,
    );
  });
}

bool _listEquals(Object? actual, List<int> expected) =>
    actual is List &&
    actual.length == expected.length &&
    [for (var i = 0; i < expected.length; i++) actual[i] == expected[i]]
        .every((ok) => ok);
