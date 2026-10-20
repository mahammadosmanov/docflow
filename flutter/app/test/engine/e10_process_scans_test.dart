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
import 'package:docflow/engine/naming.dart';
import 'package:docflow/engine/operations/process_scans.dart';
import 'package:docflow/engine/pdf.dart' as pdf;
import 'package:docflow/engine/rfi_code.dart';
import 'package:flutter_test/flutter_test.dart' hide find;
import 'package:path/path.dart' as p;

const String _fixtureRoot = '../../src-tauri/tests/fixtures';
const String _civ0064 = 'AZE-SUN-INT-QMS-RFI-CIV-0064';

const String _skipApply =
    'Apply/Undo is not exercised by this Preview-level port; needs the '
    'FilesystemAuthority apply and undo harness';
const String _notPortedYet =
    'not ported in this repair pass: orientation, parallel analysis and '
    'revision cases remain to be ported from src-tauri/tests/process_scans.rs';
const String _skipNative =
    'UNVERIFIED: macOS PDFKit / Windows PDF engine renderer is not ported to '
    'Dart; cannot run on this host';

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
void _writeMultiPagePdf(String path, int pageCount) {
  const catalog = '<< /Type /Catalog /Pages 2 0 R >>';
  final kids = [for (var i = 0; i < pageCount; i++) '${i + 3} 0 R'].join(' ');
  final objects = <String>[
    catalog,
    '<< /Type /Pages /Kids [$kids] /Count $pageCount >>',
  ];
  for (var page = 0; page < pageCount; page++) {
    objects.add(
      '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] '
      '/Resources << /Font << /F1 ${pageCount + 3} 0 R >> >> '
      '/Contents ${pageCount + 4 + page} 0 R >>',
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

/// Every read fails with an OCR engine that is not installed.
final class _MissingOcr extends OcrAdapter {
  const _MissingOcr();

  @override
  Future<OcrRead> read(String path) =>
      Future.error(const OcrEngineMissing());

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

    test('accepts only supported disciplines and does not repair discipline',
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
    });

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
      final fixture =
          jsonDecode(
                File(
                  p.join(_fixtureRoot, 'slice5_rfi_naming.json'),
                ).readAsStringSync(),
              )
              as Map<String, Object?>;

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
        expect(matched.confidence, (c['confidence']! as num).toDouble(),
            reason: '$name');
      }

      const root = 'fixture-root';
      for (final raw in fixture['namingCases']! as List<Object?>) {
        final c = raw! as Map<String, Object?>;
        final code = c['code']! as String;
        expect(filenameFor(code), c['filename']);
        expect(
          folderFor(root, code),
          p.join(root, c['folder']! as String),
        );
      }
    });
  });

  group('fast-cover Preview (fixture OCR, real PDFs)', () {
    test('uncertain pages between confirmed covers stay with the current RFI',
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
    });

    test('long continuation run is split only at the next confirmed cover',
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
      expect(extracted[0].meta['pageIndexes'], [for (var i = 0; i < 23; i++) i]);
      expect(extracted[1].meta['pageIndexes'], [23, 24]);
      expect(plan.warnings, isEmpty);
    });

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

    test('combined scan Preview splits ordered RFIs into discipline folders',
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

      expect(File(combined).existsSync(), isTrue,
          reason: 'Preview must not move the input');
      expect(_mkdir(destination).listSync(), isEmpty);
      final extracts = _extracts(plan);
      expect(extracts, hasLength(4));
      expect(extracts[0].meta['pageIndexes'], [0, 1]);
      expect(_hasPathComponent(extracts[0].dst!, 'CIV'), isTrue);
      expect(_hasPathComponent(extracts[1].dst!, 'MEC'), isTrue);
      expect(_hasPathComponent(extracts[2].dst!, 'ELE'), isTrue);
      expect(_hasPathComponent(extracts[3].dst!, 'PRC'), isTrue);
    });

    test('keep original copies low-confidence files into Review', () async {
      final dir = _tempDir('low-confidence');
      final source = _mkdir(p.join(dir.path, 'source')).path;
      final destination = _mkdir(p.join(dir.path, 'destination')).path;
      File(p.join(source, 'low-confidence.pdf')).writeAsBytesSync(
        ascii.encode('PDF'),
      );

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

    test('missing OCR engine returns one diagnostic and a zero-step plan',
        () async {
      final dir = _tempDir('no-ocr');
      final source = _mkdir(p.join(dir.path, 'source')).path;
      final destination = _mkdir(p.join(dir.path, 'destination')).path;
      File(p.join(source, 'a.pdf')).writeAsBytesSync(ascii.encode('%PDF stub'));
      File(p.join(source, 'b.PDF')).writeAsBytesSync(ascii.encode('%PDF stub'));

      final plan = await _preview(
        _params(source: source, destination: destination),
        const _MissingOcr(),
      );

      expect(plan.steps, isEmpty);
      expect(plan.warnings, hasLength(1));
      expect(plan.warnings.single.key, 'warn.no_ocr');
      expect(plan.warnings.single.subject, contains('tesseract: unavailable'));
      expect(File(p.join(source, 'a.pdf')).existsSync(), isTrue);
      expect(File(p.join(source, 'b.PDF')).existsSync(), isTrue);
      expect(_mkdir(destination).listSync(), isEmpty);
    });
  });

  group('skipped: not portable in this repair', () {
    test('combined scan Apply and Undo round trip', () {}, skip: _skipApply);
    test('Apply never clobbers a destination created after Preview', () {},
        skip: _skipApply);
    test('Apply cancellation is observed before the first mutation', () {},
        skip: _skipApply);
    test('Preview, Apply, journal and Undo keep duplicates and failures', () {},
        skip: _skipApply);
    test('review scan and put/delete decisions are inert Rust plans', () {},
        skip: 'review decision planning is not part of this Preview-level port');
    test('native image-only PDF runs through the platform renderer', () {},
        skip: _skipNative);
    test('native PDF adapter never trusts searchable text without page OCR',
        () {},
        skip: _skipNative);
    test('searchable-text decoys never become false RFI labels', () {},
        skip: _skipNative);
    test('operator process-scan PDF matches the expected exact RFI', () {},
        skip: 'needs the operator lab PDF and Tesseract; set env to run');
    test('confident orientation is added to the cached plan', () {},
        skip: _notPortedYet);
    test('uncertain orientation routes the file to Review unchanged', () {},
        skip: _notPortedYet);
    test('overflow rotation is canonical and idempotent', () {},
        skip: _notPortedYet);
    test('noncardinal metadata is routed to Review', () {}, skip: _notPortedYet);
    test('source rotation is remapped to the extracted page index', () {},
        skip: _notPortedYet);
    test('source replaced during parallel OCR analysis is rejected', () {},
        skip: _notPortedYet);
    test('preview uses bounded parallel analysis when supported', () {},
        skip: _notPortedYet);
    test('fast scan preview revision rederives target from valid code', () {},
        skip: _notPortedYet);
    test('fast scan revision rejects invalid codes, paths and collisions', () {},
        skip: _notPortedYet);
  });
}

bool _listEquals(Object? actual, List<int> expected) =>
    actual is List && actual.length == expected.length &&
    [for (var i = 0; i < expected.length; i++) actual[i] == expected[i]]
        .every((ok) => ok);
