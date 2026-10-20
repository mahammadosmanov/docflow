// Port of src-tauri/tests/signed_pages.rs (preview, revise and oracle tests)
// and of the preview portions of the Rust tests that also apply.
//
// The PDF-level tests in signed_pages.rs (incremental rotation, first-page
// replacement, indirect /Length, object streams, fail-closed inputs) are
// ported in e9_pdf_test.dart. Apply, journal and undo steps are skipped
// because the Dart FilesystemAuthority is not ported yet; the operator test
// needs private documents, as in Rust.
library;

import 'dart:convert';
import 'dart:io';

import 'package:docflow/engine/contracts.dart';
import 'package:docflow/engine/fs_ops.dart';
import 'package:docflow/engine/journal.dart';
import 'package:docflow/engine/paths.dart';
import 'package:docflow/engine/plan_store.dart';
import 'package:docflow/engine/production_ports.dart';
import 'package:docflow/engine/rfi_system_sync.dart' show RfiSystemSync;
import 'package:docflow/engine/undo.dart';
import 'package:docflow/engine/errors.dart';
import 'package:docflow/engine/naming.dart' show filenameFor;
import 'package:docflow/engine/operations/process_scans.dart'
    show OcrAdapter, OcrDocumentFailure, OcrRead;
import 'package:docflow/engine/operations/stage_signed_pages.dart';
import 'package:docflow/engine/rfi_code.dart' as rfi;
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

import 'e9_pdf_fixture.dart';

const String _operatorSkip =
    'needs private Ventoy RFI documents and Tesseract; ignored in Rust as well';

const String _code = fixtureCode;
const String _secondCode = 'AZE-SUN-INT-QMS-RFI-MEC-0249';

rfi.RfiMatch _withMethod(rfi.RfiMatch match, rfi.RfiMethod method) =>
    rfi.RfiMatch(
      code: match.code,
      discipline: match.discipline,
      number: match.number,
      method: method,
      confidence: match.confidence,
    );

/// Port of `FixtureOcr`: a text-layer read keyed by file name.
class _FixtureOcr extends OcrAdapter {
  _FixtureOcr(this.codes);

  factory _FixtureOcr.one(List<String> names) =>
      _FixtureOcr({for (final name in names) name: _code});

  final Map<String, String> codes;

  @override
  Future<OcrRead> read(String path) async {
    final name = p.basename(path);
    if (name == 'malformed.pdf') {
      throw const OcrDocumentFailure('malformed PDF');
    }
    return OcrRead(
      matched: rfi.find(codes[name] ?? ''),
      source: 'fixture-text-layer',
    );
  }

  @override
  String diagnostics() => 'fixture OCR';
}

/// Port of `PageFixtureOcr`: per-page codes, page scan supported.
class _PageFixtureOcr extends OcrAdapter {
  _PageFixtureOcr(this.codes) : previews = false;

  final Map<(String, int), String> codes;
  final bool previews;

  @override
  Future<OcrRead> read(String path) => readPage(path, 0);

  @override
  bool get supportsPageScan => true;

  @override
  bool get supportsPagePreviews => previews;

  @override
  Future<OcrRead> readPage(String path, int pageIndex) async {
    final code = codes[(p.basename(path), pageIndex)] ?? '';
    return OcrRead(matched: rfi.find(code), source: 'fixture-page-$pageIndex');
  }

  @override
  String diagnostics() => 'fixture page OCR';
}

/// Port of `ParallelPageFixtureOcr`: same reads, parallel analysis enabled.
class _ParallelPageFixtureOcr extends _PageFixtureOcr {
  _ParallelPageFixtureOcr(super.codes);

  @override
  bool get supportsParallelAnalysis => true;

  @override
  String diagnostics() => 'parallel fixture page OCR';
}

/// Port of `AliasFixtureOcr`: an approved OCR-alias identity.
class _AliasFixtureOcr extends OcrAdapter {
  _AliasFixtureOcr(this.code);

  final String code;

  @override
  Future<OcrRead> read(String path) async => OcrRead(
    matched: _withMethod(rfi.find(code), rfi.RfiMethod.ocrAlias),
    source: 'fixture-consensus-alias',
  );

  @override
  String diagnostics() => 'fixture alias OCR';
}

/// Port of `RepairedFixtureOcr`: a repaired identity, which must stay in
/// review.
class _RepairedFixtureOcr extends OcrAdapter {
  @override
  Future<OcrRead> read(String path) async => OcrRead(
    matched: _withMethod(rfi.find(_code), rfi.RfiMethod.repaired),
    source: 'fixture-repaired',
  );

  @override
  String diagnostics() => 'fixture repaired OCR';
}

StageSignedPagesParams _params(
  String signed,
  String archive,
  String review, {
  required bool recursive,
}) => StageSignedPagesParams(
  signedScans: signed,
  archiveRoot: archive,
  reviewRoot: review,
  scanSubfolders: recursive,
);

List<String> _keys(Plan plan) => [for (final w in plan.warnings) w.key];

List<Step> _replacements(Plan plan) => [
  for (final step in plan.steps)
    if (step.kind == StepKind.pdfFirstPageReplace) step,
];

Map<String, Object?> _editor(Plan plan) =>
    plan.meta['editor']! as Map<String, Object?>;

List<Map<String, Object?>> _rows(Plan plan) => [
  for (final row in _editor(plan)['rows']! as List<Object?>)
    row! as Map<String, Object?>,
];

Map<String, Object?> _copyRow(Map<String, Object?> row) =>
    Map<String, Object?>.from(row);

FilesystemAuthority _stageAuthority(String journalDir) => FilesystemAuthority(
  journalDir: journalDir,
  ttl: const Duration(minutes: 5),
  ooxml: const ProductionFollowUpOoxml(),
  rfiLog: const ProductionRfiLogWorkbook(),
  rfiSystemSync: const RfiSystemSync(),
  pdf: const ProductionPdfEngine(),
  replicaPublish: const ProductionReplicaPublish(),
  undoJournal: undoJournal,
);

Plan _cacheStage(
  FilesystemAuthority authority,
  Plan plan,
  String signed,
  String archive,
  String review,
) {
  const opId = 'rfi.stage_signed_first_pages';
  final scope = GrantScope.create('owner-1', 'session-1', 'preview-1', opId);
  final signedGrant = authority.issueGrant(scope, signed, GrantAccess.read);
  final archiveGrant = authority.issueGrant(scope, archive, GrantAccess.read);
  final reviewGrant = authority.issueGrant(
    scope,
    review,
    GrantAccess.readWrite,
  );
  return authority.cachePlan(
    BackendOrigin.rust,
    PlanBinding(
      ownerId: 'owner-1',
      sessionId: 'session-1',
      operationId: opId,
      grantIds: [
        signedGrant.grantId,
        archiveGrant.grantId,
        reviewGrant.grantId,
      ],
    ),
    plan,
  );
}

void main() {
  late Directory fixture;

  setUp(() {
    fixture = Directory.systemTemp.createTempSync('e9_stage');
  });

  tearDown(() {
    fixture.deleteSync(recursive: true);
  });

  String at(String name) => p.join(fixture.path, name);

  /// Port of `roots`: signed, archive and review directories.
  (String, String, String) roots() {
    final signed = at('signed');
    final archive = at('archive');
    final review = at('review');
    for (final root in [signed, archive, review]) {
      Directory(root).createSync(recursive: true);
    }
    return (signed, archive, review);
  }

  void writeAt(String path, void Function(String) writer) {
    Directory(p.dirname(path)).createSync(recursive: true);
    writer(path);
  }

  test(
    'one multi-page scan creates one staging row per exact RFI page',
    () async {
      final (signedRoot, archiveRoot, reviewRoot) = roots();
      writeRichArchive(p.join(archiveRoot, 'CIV', filenameFor(_code)));
      writeRichArchive(p.join(archiveRoot, 'MEC', filenameFor(_secondCode)));
      writeSigned(p.join(signedRoot, 'combined.pdf'), extraPage: true);
      final ocr = _PageFixtureOcr({
        ('combined.pdf', 0): _code,
        ('combined.pdf', 1): _secondCode,
      });

      final progress = <ProgressEvent>[];
      final plan = await previewStageSignedPages(
        _params(signedRoot, archiveRoot, reviewRoot, recursive: false),
        ocr,
        progress: progress.add,
      );

      final replacements = _replacements(plan);
      expect(replacements, hasLength(2));
      expect(replacements[0].meta['signedPageIndex'], 0);
      expect(replacements[1].meta['signedPageIndex'], 1);
      final rows = _rows(plan);
      expect(rows, hasLength(2));
      expect(rows[0]['confidence'], 1.0);
      expect(
        (_editor(plan)['columns']! as List<Object?>).any((column) {
          final map = column! as Map<String, Object?>;
          return map['key'] == 'confidence' && map['display'] == 'percent';
        }),
        isTrue,
      );
      expect(plan.meta['pageComparisons'], isEmpty);
      expect(_keys(plan), contains('warn.rfi.signedMultiPage'));
      expect(progress.last.phase, 'preview_ready');
      // Apply (four effects, then journal) is skipped: see _applySkip.
    },
  );

  test('staged apply revalidates hashes, never clobbers, and undoes created output', () async {
    final (signedRoot, archiveRoot, reviewRoot) = roots();
    final signed = p.join(signedRoot, 'signed.pdf');
    final archive = p.join(archiveRoot, 'Package', 'Civil', filenameFor(_code));
    writeSigned(signed, extraPage: false);
    writeRichArchive(archive);
    final archiveHash = fileSha256(archive);
    final signedHash = fileSha256(signed);
    final plan = await previewStageSignedPages(
      _params(signedRoot, archiveRoot, reviewRoot, recursive: false),
      _FixtureOcr.one(['signed.pdf']),
    );
    final authority = _stageAuthority(at('journal'));
    final cached = _cacheStage(
      authority,
      plan,
      signedRoot,
      archiveRoot,
      reviewRoot,
    );
    const caller = CallerIdentity('owner-1', 'session-1');

    final result = authority.apply(cached.planId, caller);
    final output = p.join(reviewRoot, 'Package', 'Civil', filenameFor(_code));
    expect(result.failed, 0);
    expect(result.meta['effectState'], 'committed');
    expect(File(output).existsSync(), isTrue);
    expect(fileSha256(archive), archiveHash);
    expect(fileSha256(signed), signedHash);
    final canonicalOutput = p.join(
      Directory(reviewRoot).resolveSymbolicLinksSync(),
      'Package',
      'Civil',
      filenameFor(_code),
    );
    final journal = authority.journals.read(result.journalId);
    expect(
      journal.steps.any(
        (step) => step.kind == StepKind.copy && step.dst == canonicalOutput,
      ),
      isTrue,
    );

    final undone = authority.undo(result.journalId, caller);
    expect(undone.failed, 0);
    expect(File(output).existsSync(), isFalse);
    expect(Directory(p.join(reviewRoot, 'Package')).existsSync(), isFalse);

    final secondPlan = await previewStageSignedPages(
      _params(signedRoot, archiveRoot, reviewRoot, recursive: false),
      _FixtureOcr.one(['signed.pdf']),
    );
    final second = _cacheStage(
      authority,
      secondPlan,
      signedRoot,
      archiveRoot,
      reviewRoot,
    );
    Directory(p.dirname(output)).createSync(recursive: true);
    File(output).writeAsBytesSync('USER RACE WINNER'.codeUnits);
    final collision = authority.apply(second.planId, caller);
    expect(collision.failed, 1);
    expect(File(output).readAsStringSync(), 'USER RACE WINNER');
  });

  test('combined scan uses bounded parallel page OCR when supported', () async {
    final (signedRoot, archiveRoot, reviewRoot) = roots();
    writeSigned(p.join(signedRoot, 'combined.pdf'), extraPage: true);
    writeRichArchive(p.join(archiveRoot, 'CIV', filenameFor(_code)));
    writeRichArchive(p.join(archiveRoot, 'MEC', filenameFor(_secondCode)));
    final ocr = _ParallelPageFixtureOcr({
      ('combined.pdf', 0): _code,
      ('combined.pdf', 1): _secondCode,
    });

    final progress = <ProgressEvent>[];
    final plan = await previewStageSignedPages(
      _params(signedRoot, archiveRoot, reviewRoot, recursive: false),
      ocr,
      progress: progress.add,
    );

    expect(plan.meta['parallelAnalysis'], true);
    expect(plan.meta['workerCount'], greaterThanOrEqualTo(1));
    expect(_replacements(plan), hasLength(2));
    expect(progress.any((event) => event.phase == 'signed_ocr'), isTrue);
    expect(progress.any((event) => event.phase == 'archive_scan'), isTrue);
    expect(progress.last.phase, 'preview_ready');
  });

  test(
    'archive discovery scans every depth with bounded parallel evidence',
    () async {
      final (signedRoot, archiveRoot, reviewRoot) = roots();
      writeSigned(
        p.join(signedRoot, 'incoming/day-1/signed.pdf'),
        extraPage: false,
      );
      for (final branch in ['CIV', 'MEC', 'ELE', 'PRC']) {
        Directory(p.join(archiveRoot, branch, 'Area/Package'))
            .createSync(recursive: true);
      }
      writeRichArchive(
        p.join(archiveRoot, 'CIV/Area/Package/RFI folder', filenameFor(_code)),
      );

      final plan = await previewStageSignedPages(
        _params(signedRoot, archiveRoot, reviewRoot, recursive: true),
        _FixtureOcr.one(['signed.pdf']),
      );

      expect(_replacements(plan), isNotEmpty);
      expect(plan.meta['archiveRecursive'], true);
      expect(plan.meta['archiveDirectoriesScanned'], greaterThanOrEqualTo(10));
      expect(plan.meta['archiveEntriesScanned'], greaterThanOrEqualTo(10));
      final workers = plan.meta['archiveWorkerCount']! as int;
      expect(workers, inInclusiveRange(1, 4));
    },
  );

  test(
    'unreadable and duplicate combined-scan pages fail closed with page labels',
    () async {
      final (signedRoot, archiveRoot, reviewRoot) = roots();
      writeSigned(p.join(signedRoot, 'combined.pdf'), extraPage: true);
      writeRichArchive(p.join(archiveRoot, 'CIV', filenameFor(_code)));

      final unreadable = _PageFixtureOcr({('combined.pdf', 0): _code});
      final first = await previewStageSignedPages(
        _params(signedRoot, archiveRoot, reviewRoot, recursive: false),
        unreadable,
      );
      expect(_replacements(first), hasLength(1));
      expect(
        first.warnings.any(
          (w) =>
              w.key == 'warn.no_code' &&
              w.subject.contains('combined.pdf') &&
              w.subject.contains('#2'),
        ),
        isTrue,
      );

      final duplicate = _PageFixtureOcr({
        ('combined.pdf', 0): _code,
        ('combined.pdf', 1): _code,
      });
      final second = await previewStageSignedPages(
        _params(signedRoot, archiveRoot, reviewRoot, recursive: false),
        duplicate,
      );
      expect(_replacements(second), isEmpty);
      expect(
        second.warnings.any(
          (w) =>
              w.key == 'warn.rfi.signedDuplicate' &&
              w.subject.contains('#1') &&
              w.subject.contains('#2'),
        ),
        isTrue,
      );
    },
  );

  test(
    'combined-scan preview reports pages and cancels between them',
    () async {
      final (signedRoot, archiveRoot, reviewRoot) = roots();
      writeSigned(p.join(signedRoot, 'combined.pdf'), extraPage: true);
      final ocr = _PageFixtureOcr({
        ('combined.pdf', 0): _code,
        ('combined.pdf', 1): _secondCode,
      });
      var cancellationChecks = 0;
      final subjects = <String>[];

      await expectLater(
        previewStageSignedPages(
          _params(signedRoot, archiveRoot, reviewRoot, recursive: false),
          ocr,
          progress: (event) => subjects.add(event.subject),
          cancelled: () {
            cancellationChecks++;
            return cancellationChecks >= 3;
          },
        ),
        throwsA(
          isA<BackendError>().having(
            (error) => error.code,
            'code',
            'operation_cancelled',
          ),
        ),
      );

      expect(subjects.any((subject) => subject.contains('#1')), isTrue);
      expect(subjects.any((subject) => subject.contains('#2')), isFalse);
      expect(Directory(reviewRoot).listSync(), isEmpty);
    },
  );

  test('archive lookup ignores nonstandard names even when the path contains the RFI code', () async {
    final (signedRoot, archiveRoot, reviewRoot) = roots();
    writeSigned(p.join(signedRoot, 'signed.pdf'), extraPage: false);
    writeRichArchive(
      p.join(archiveRoot, 'Package/Civil', 'Archived $_code approved copy.pdf'),
    );
    writeRichArchive(p.join(archiveRoot, _code, 'approved.pdf'));
    writeRichArchive(p.join(archiveRoot, _code, 'Sketch.pdf'));
    writeAt(
      p.setExtension(p.join(archiveRoot, filenameFor(_code)), '.docx'),
      (path) => File(path).writeAsBytesSync('not an archive PDF'.codeUnits),
    );

    final plan = await previewStageSignedPages(
      _params(signedRoot, archiveRoot, reviewRoot, recursive: false),
      _FixtureOcr.one(['signed.pdf']),
    );

    expect(_replacements(plan), isEmpty);
    expect(plan.warnings, hasLength(1));
    expect(plan.warnings.first.key, 'warn.rfi.archiveMissing');
    final reviewItems = plan.meta['warningReviewItems']! as List<Object?>;
    final reasons = ((reviewItems.first! as Map)['reasons']! as List<Object?>);
    expect((reasons.first! as Map)['code'], _code);
  });

  test('approved OCR alias is accepted and short archive numbers match canonically', () async {
    expect(rfi.normalizeCandidateNumber('64'), '0064');
    expect(rfi.normalizeCandidateNumber('0064'), '0064');
    expect(rfi.normalizeCandidateNumber('10064'), isNull);
    for (final archiveNumber in ['64', '0064']) {
      final (signedRoot, archiveRoot, reviewRoot) = roots();
      writeSigned(p.join(signedRoot, 'signed.pdf'), extraPage: false);
      writeRichArchive(
        p.join(
          archiveRoot,
          'Package/Civil',
          'RFI No. AZE-SUN-INT-QMS-RFI-CIV-$archiveNumber.pdf',
        ),
      );

      final plan = await previewStageSignedPages(
        _params(signedRoot, archiveRoot, reviewRoot, recursive: false),
        _AliasFixtureOcr(_code),
      );

      expect(
        _replacements(plan),
        isNotEmpty,
        reason: 'archive number $archiveNumber should resolve the RFI',
      );
      expect(
        _rows(plan).first['code'],
        _code,
        reason: 'persisted code is canonical',
      );
      fixture.deleteSync(recursive: true);
      fixture = Directory.systemTemp.createTempSync('e9_stage');
    }
  });

  test('repaired RFI identity remains review-only', () async {
    final (signedRoot, archiveRoot, reviewRoot) = roots();
    writeSigned(p.join(signedRoot, 'signed.pdf'), extraPage: false);
    writeRichArchive(p.join(archiveRoot, 'Package/Civil', filenameFor(_code)));

    final plan = await previewStageSignedPages(
      _params(signedRoot, archiveRoot, reviewRoot, recursive: false),
      _RepairedFixtureOcr(),
    );

    expect(_replacements(plan), isEmpty);
    expect(_keys(plan), contains('warn.no_code'));
    final reviewItems = plan.meta['warningReviewItems']! as List<Object?>;
    final reasons = ((reviewItems.first! as Map)['reasons']! as List<Object?>);
    expect((reasons.first! as Map)['key'], 'warn.no_code');
  });

  test(
    'archive ambiguity requires an allowlisted RFI-No PDF selection',
    () async {
      final (signedRoot, archiveRoot, reviewRoot) = roots();
      writeSigned(p.join(signedRoot, 'signed.pdf'), extraPage: false);
      final first = p.join('Other', filenameFor(_code));
      final second = p.join('Package', 'Civil', filenameFor(_code));
      writeRichArchive(p.join(archiveRoot, first));
      writeRichArchive(p.join(archiveRoot, second));

      final plan = await previewStageSignedPages(
        _params(signedRoot, archiveRoot, reviewRoot, recursive: false),
        _FixtureOcr.one(['signed.pdf']),
      );

      expect(_replacements(plan), isEmpty);
      expect(plan.warnings, hasLength(1));
      expect(plan.warnings.first.key, 'warn.rfi.archiveAmbiguous');
      final rows = _rows(plan);
      expect(rows, hasLength(1));
      expect(rows.first['enabled'], true);
      expect(rows.first['code'], _code);
      expect(rows.first['archiveFile'], '');
      expect(_editor(plan)['requiredFields'], ['archiveFile']);
      final choices =
          (_editor(plan)['choicesByValue']! as Map)['archiveFile']! as Map;
      expect((choices['values']! as Map)[_code], [
        '',
        'signed-$_code-candidate-1',
        'signed-$_code-candidate-2',
      ]);
      final labels =
          (_editor(plan)['choiceLabels']! as Map)['archiveFile']! as Map;
      expect(labels['signed-$_code-candidate-1'], first);
      expect(labels['signed-$_code-candidate-2'], second);

      final selected = [for (final row in rows) _copyRow(row)];
      selected.first['archiveFile'] = 'signed-$_code-candidate-2';
      final revised = revise(plan, {'rows': selected});
      final replacements = _replacements(revised);
      expect(replacements, hasLength(1));
      final canonicalArchiveRoot = Directory(archiveRoot)
          .resolveSymbolicLinksSync();
      final canonicalReviewRoot = Directory(reviewRoot)
          .resolveSymbolicLinksSync();
      expect(replacements.first.src, p.join(canonicalArchiveRoot, second));
      expect(replacements.first.dst, p.join(canonicalReviewRoot, second));

      final missing = [for (final row in rows) _copyRow(row)];
      missing.first['archiveFile'] = '';
      expect(
        () => revise(plan, {'rows': missing}),
        throwsA(isA<BackendError>()),
      );

      final forged = [for (final row in rows) _copyRow(row)];
      forged.first['archiveFile'] =
          'Forged/RFI No. AZE-SUN-INT-QMS-RFI-CIV-0064.pdf';
      expect(
        () => revise(plan, {'rows': forged}),
        throwsA(isA<BackendError>()),
      );

      final disabled = [for (final row in rows) _copyRow(row)];
      disabled.first['enabled'] = false;
      final skipped = revise(plan, {'rows': disabled});
      expect(_replacements(skipped), isEmpty);
    },
  );

  test('archive apply after a changed selection fails closed', () async {
    final (signedRoot, archiveRoot, reviewRoot) = roots();
    final signed = p.join(signedRoot, 'signed.pdf');
    final archive = p.join(archiveRoot, 'Package', 'Civil', filenameFor(_code));
    writeSigned(signed, extraPage: false);
    writeRichArchive(archive);
    final plan = await previewStageSignedPages(
      _params(signedRoot, archiveRoot, reviewRoot, recursive: false),
      _FixtureOcr.one(['signed.pdf']),
    );
    final authority = _stageAuthority(at('journal'));
    final cached = _cacheStage(
      authority,
      plan,
      signedRoot,
      archiveRoot,
      reviewRoot,
    );
    File(archive).writeAsBytesSync('ARCHIVE CHANGED AFTER SELECTION'.codeUnits);

    final result = authority.apply(
      cached.planId,
      const CallerIdentity('owner-1', 'session-1'),
    );
    expect(result.failed, 1);
    expect(
      File(p.join(reviewRoot, 'Package', 'Civil', filenameFor(_code)))
          .existsSync(),
      isFalse,
    );
  });

  test(
    'archive lookup rejects numeric tails, suffixes, and non-PDF files',
    () async {
      final (signedRoot, archiveRoot, reviewRoot) = roots();
      writeSigned(p.join(signedRoot, 'signed.pdf'), extraPage: false);
      for (final name in [
        'RFI No. ${_code}5.pdf',
        'RFI No. $_code approved.pdf',
        'RFI No. $_code٥.pdf',
        'RFI No. $_code.txt',
      ]) {
        writeRichArchive(p.join(archiveRoot, 'Package/Civil', name));
      }

      final plan = await previewStageSignedPages(
        _params(signedRoot, archiveRoot, reviewRoot, recursive: false),
        _FixtureOcr.one(['signed.pdf']),
      );

      expect(_replacements(plan), isEmpty);
      expect(plan.warnings, hasLength(1));
      expect(plan.warnings.first.key, 'warn.rfi.archiveMissing');
    },
  );

  test(
    'preview preserves scan subfolders and surfaces PDF fidelity warnings',
    () async {
      final (signedRoot, archiveRoot, reviewRoot) = roots();
      final nestedSigned = p.join(signedRoot, 'incoming/day-1/signed.pdf');
      final archive = p.join(
        archiveRoot,
        'Package/Civil',
        'rfi no. ${_code.toLowerCase()}.PdF',
      );
      writeSigned(nestedSigned, extraPage: true);
      writeRichArchive(archive);

      final shallow = await previewStageSignedPages(
        _params(signedRoot, archiveRoot, reviewRoot, recursive: false),
        _FixtureOcr.one(['signed.pdf']),
      );
      expect(shallow.steps, isEmpty);
      expect(shallow.warnings.first.key, 'warn.rfi.noSignedPdfs');

      final recursive = await previewStageSignedPages(
        _params(signedRoot, archiveRoot, reviewRoot, recursive: true),
        _FixtureOcr.one(['signed.pdf']),
      );
      expect(_replacements(recursive), hasLength(1));
      expect(_keys(recursive), contains('warn.rfi.signedExtraPages'));

      writeSignedDocument(archive);
      final unsupported = await previewStageSignedPages(
        _params(signedRoot, archiveRoot, reviewRoot, recursive: true),
        _FixtureOcr.one(['signed.pdf']),
      );
      expect(_replacements(unsupported), isEmpty);
      expect(_keys(unsupported), contains('warn.rfi.pdfFidelityUnsupported'));

      writeTaggedDocument(archive);
      final tagged = await previewStageSignedPages(
        _params(signedRoot, archiveRoot, reviewRoot, recursive: true),
        _FixtureOcr.one(['signed.pdf']),
      );
      expect(_replacements(tagged), isEmpty);
      expect(_keys(tagged), contains('warn.rfi.pdfFidelityUnsupported'));
    },
  );

  test('fuzzy prefixes never authorize signed-page staging', () async {
    final (signedRoot, archiveRoot, reviewRoot) = roots();
    writeSigned(p.join(signedRoot, 'fuzzy.pdf'), extraPage: false);
    writeRichArchive(p.join(archiveRoot, 'Package/Civil', filenameFor(_code)));

    final plan = await previewStageSignedPages(
      _params(signedRoot, archiveRoot, reviewRoot, recursive: false),
      _FixtureOcr({'fuzzy.pdf': 'AZE-SUN-INT-QMX-RFI-CIV-0064'}),
    );

    final reviewCopy = plan.steps.firstWhere(
      (step) => step.kind == StepKind.copy,
    );
    final canonicalReviewRoot = Directory(reviewRoot)
        .resolveSymbolicLinksSync();
    expect(reviewCopy.dst, p.join(canonicalReviewRoot, '_review', 'fuzzy.pdf'));
    expect(reviewCopy.meta['warningReview'], true);
    expect(reviewCopy.meta['expectedSourceSha256'], isA<String>());
    expect(plan.warnings.first.key, 'warn.no_code');
    expect(plan.warnings.first.subject, contains('fuzzy.pdf'));
    expect(plan.warnings.first.subject, contains('page 1'));
    final items = plan.meta['warningReviewItems']! as List<Object?>;
    expect((items.first! as Map)['status'], 'planned');
    expect(((items.first! as Map)['reasons']! as List).first, isA<Map>());
    expect(plan.meta['archiveWorkerCount'], 0);
    expect(plan.meta['archiveDirectoriesScanned'], 0);
    expect(plan.meta['archiveEntriesScanned'], 0);
  });

  test(
    'warning-review copy is planned with the previewed source hash',
    () async {
      final (signedRoot, archiveRoot, reviewRoot) = roots();
      writeSigned(p.join(signedRoot, 'unmatched.pdf'), extraPage: false);

      final plan = await previewStageSignedPages(
        _params(signedRoot, archiveRoot, reviewRoot, recursive: false),
        _FixtureOcr({}),
      );

      final copy = plan.steps.firstWhere((step) => step.kind == StepKind.copy);
      expect(copy.src, endsWith('unmatched.pdf'));
      expect(copy.meta['expectedSourceSha256'], isA<String>());
    },
  );

  test(
    'warning-review copy revalidates the previewed source hash on apply',
    () async {
      final (signedRoot, archiveRoot, reviewRoot) = roots();
      final source = p.join(signedRoot, 'unmatched.pdf');
      writeSigned(source, extraPage: false);
      final plan = await previewStageSignedPages(
        _params(signedRoot, archiveRoot, reviewRoot, recursive: false),
        _FixtureOcr({}),
      );
      final authority = _stageAuthority(at('journal'));
      final cached = _cacheStage(
        authority,
        plan,
        signedRoot,
        archiveRoot,
        reviewRoot,
      );
      File(source).writeAsBytesSync('changed after preview'.codeUnits);

      final result = authority.apply(
        cached.planId,
        const CallerIdentity('owner-1', 'session-1'),
      );
      expect(result.failed, 1);
      expect(
        File(p.join(reviewRoot, '_review', 'unmatched.pdf')).existsSync(),
        isFalse,
      );
    },
  );

  test(
    'recursive discovery never follows signed or archive directory symlinks',
    () async {
      final signedFixture = at('signed-links');
      Directory(signedFixture).createSync();
      final signedRoot = p.join(signedFixture, 'signed');
      final archiveRoot = p.join(signedFixture, 'archive');
      final reviewRoot = p.join(signedFixture, 'review');
      for (final root in [signedRoot, archiveRoot, reviewRoot]) {
        Directory(root).createSync(recursive: true);
      }
      final outsideSigned = p.join(signedFixture, 'outside-signed');
      writeSigned(p.join(outsideSigned, 'signed.pdf'), extraPage: false);
      Link(p.join(signedRoot, 'linked-scans')).createSync(outsideSigned);
      writeRichArchive(
        p.join(archiveRoot, 'Package/Civil', filenameFor(_code)),
      );

      final signedPlan = await previewStageSignedPages(
        _params(signedRoot, archiveRoot, reviewRoot, recursive: true),
        _FixtureOcr.one(['signed.pdf']),
      );
      expect(signedPlan.steps, isEmpty);
      expect(signedPlan.warnings.first.key, 'warn.rfi.noSignedPdfs');

      final archiveSignedRoot = at('signed-2');
      final archiveReview = at('review-2');
      final archiveSource = at('archive-2');
      for (final root in [archiveSignedRoot, archiveReview, archiveSource]) {
        Directory(root).createSync(recursive: true);
      }
      writeSigned(p.join(archiveSignedRoot, 'signed.pdf'), extraPage: false);
      final outsideArchive = at('outside-archive');
      writeRichArchive(p.join(outsideArchive, filenameFor(_code)));
      Link(p.join(archiveSource, 'linked-package')).createSync(outsideArchive);

      final archivePlan = await previewStageSignedPages(
        _params(
          archiveSignedRoot,
          archiveSource,
          archiveReview,
          recursive: false,
        ),
        _FixtureOcr.one(['signed.pdf']),
      );
      expect(_replacements(archivePlan), isEmpty);
      expect(archivePlan.warnings.first.key, 'warn.rfi.archiveMissing');
    },
  );

  test('archive lookup never follows direct PDF symlinks', () async {
    final (signedRoot, archiveRoot, reviewRoot) = roots();
    writeSigned(p.join(signedRoot, 'signed.pdf'), extraPage: false);
    final outsideArchive = at('outside-archive.pdf');
    writeRichArchive(outsideArchive);
    Link(p.join(archiveRoot, filenameFor(_code))).createSync(outsideArchive);

    final plan = await previewStageSignedPages(
      _params(signedRoot, archiveRoot, reviewRoot, recursive: false),
      _FixtureOcr.one(['signed.pdf']),
    );

    expect(_replacements(plan), isEmpty);
    expect(plan.warnings, hasLength(1));
    expect(plan.warnings.first.key, 'warn.rfi.archiveMissing');
  });

  test('revise can only disable existing staging rows and never recalculates paths', () async {
    final (signedRoot, archiveRoot, reviewRoot) = roots();
    writeSigned(p.join(signedRoot, 'signed.pdf'), extraPage: false);
    writeRichArchive(p.join(archiveRoot, 'Package/Civil', filenameFor(_code)));
    final plan = await previewStageSignedPages(
      _params(signedRoot, archiveRoot, reviewRoot, recursive: false),
      _FixtureOcr.one(['signed.pdf']),
    );
    final rows = [for (final row in _rows(plan)) _copyRow(row)];
    rows.first['enabled'] = false;

    final revised = revise(plan, {'rows': rows});
    expect(_replacements(revised), isEmpty);
    expect(_replacements(plan), isNotEmpty);

    final forged = [
      {'_rowId': 'forged', 'enabled': true},
    ];
    expect(() => revise(plan, {'rows': forged}), throwsA(isA<BackendError>()));
  });

  test('review root overlap is rejected without any plan authority', () async {
    final (signedRoot, archiveRoot, _) = roots();
    writeSigned(p.join(signedRoot, 'signed.pdf'), extraPage: false);
    writeRichArchive(p.join(archiveRoot, 'Package/Civil', filenameFor(_code)));
    final overlapping = at('archive/review');
    Directory(overlapping).createSync();

    final plan = await previewStageSignedPages(
      _params(signedRoot, archiveRoot, overlapping, recursive: false),
      _FixtureOcr.one(['signed.pdf']),
    );

    expect(plan.steps, isEmpty);
    expect(plan.warnings.first.key, 'error.rfi.reviewRootMustBeSeparate');
  });

  test('preview matches the frozen compatibility oracle for duplicates, misses, ambiguity, and no-clobber', () async {
    final oracle = jsonDecode(
      File('../../src-tauri/tests/fixtures/slice7_signed_pages_oracle.json')
          .readAsStringSync(),
    ) as Map<String, Object?>;
    for (final raw in oracle['cases']! as List<Object?>) {
      final tc = raw! as Map<String, Object?>;
      final name = tc['name']! as String;
      final (signedRoot, archiveRoot, reviewRoot) = roots();
      final signedNames = [
        for (final value in tc['signed']! as List<Object?>) value! as String,
      ];
      for (final signedName in signedNames) {
        writeSigned(p.join(signedRoot, signedName), extraPage: false);
      }
      final archives = [
        for (final value in tc['archives']! as List<Object?>) value! as String,
      ];
      for (final relative in archives) {
        writeRichArchive(p.join(archiveRoot, relative, filenameFor(_code)));
      }
      if (tc['reviewExists'] == true) {
        final output = p.join(reviewRoot, archives.first, filenameFor(_code));
        Directory(p.dirname(output)).createSync(recursive: true);
        File(output).writeAsStringSync('USER FILE');
      }

      final plan = await previewStageSignedPages(
        _params(signedRoot, archiveRoot, reviewRoot, recursive: false),
        _FixtureOcr.one(signedNames),
      );

      expect(_replacements(plan), hasLength(tc['steps']! as int), reason: name);
      expect(_keys(plan), [
        for (final value in tc['warnings']! as List<Object?>) value! as String,
      ], reason: name);
      for (final signedName in signedNames) {
        expect(File(p.join(signedRoot, signedName)).existsSync(), isTrue);
      }
      fixture.deleteSync(recursive: true);
      fixture = Directory.systemTemp.createTempSync('e9_stage');
    }
  });

  test('completed publish stale hardlink is reviewed and cleaned without touching output', () async {
    final (signedRoot, archiveRoot, reviewRoot) = roots();
    final signed = p.join(signedRoot, 'signed.pdf');
    final archive = p.join(archiveRoot, 'Package', 'Civil', filenameFor(_code));
    final output = p.join(reviewRoot, 'Package', 'Civil', filenameFor(_code));
    writeSigned(signed, extraPage: false);
    writeRichArchive(archive);
    writeRichArchive(output);
    final outputHash = fileSha256(output);
    final stale = p.join(
      p.dirname(output),
      '.${p.basename(output)}.docflow-leftover.tmp',
    );
    // Hard link (not symlink): same inode as the published output.
    final linked = Process.runSync('ln', [output, stale]);
    expect(linked.exitCode, 0, reason: '${linked.stderr}');

    final plan = await previewStageSignedPages(
      _params(signedRoot, archiveRoot, reviewRoot, recursive: false),
      _FixtureOcr.one(['signed.pdf']),
    );
    expect(plan.steps.map((s) => s.kind).toList(), [StepKind.cleanupTemp]);
    expect(plan.warnings.map((w) => w.key).toList(), [
      'warn.rfi.staleTempFound',
    ]);

    final authority = _stageAuthority(at('journal'));
    final cached = _cacheStage(
      authority,
      plan,
      signedRoot,
      archiveRoot,
      reviewRoot,
    );
    final result = authority.apply(
      cached.planId,
      const CallerIdentity('owner-1', 'session-1'),
    );
    expect(result.applied, 1);
    expect(result.failed, 0);
    expect(File(stale).existsSync(), isFalse);
    expect(fileSha256(output), outputHash);
    expect(authority.journals.read(result.journalId).steps, isEmpty);
  });

  test(
    'same bytes in an unrelated temp copy never authorize deletion',
    () async {
      final (signedRoot, archiveRoot, reviewRoot) = roots();
      final signed = p.join(signedRoot, 'signed.pdf');
      final archive = p.join(
        archiveRoot,
        'Package',
        'Civil',
        filenameFor(_code),
      );
      final output = p.join(reviewRoot, 'Package', 'Civil', filenameFor(_code));
      writeSigned(signed, extraPage: false);
      writeRichArchive(archive);
      writeRichArchive(output);
      final outputBytes = File(output).readAsBytesSync();
      final unrelated = p.join(
        p.dirname(output),
        '.${p.basename(output)}.docflow-unrelated.tmp',
      );
      File(unrelated).writeAsBytesSync(outputBytes);

      final plan = await previewStageSignedPages(
        _params(signedRoot, archiveRoot, reviewRoot, recursive: false),
        _FixtureOcr.one(['signed.pdf']),
      );
      expect(
        plan.steps.every((s) => s.kind != StepKind.pdfFirstPageReplace),
        isTrue,
      );
      expect(plan.warnings.first.key, 'warn.rfi.reviewExists');
      expect(File(output).readAsBytesSync(), outputBytes);
      expect(File(unrelated).existsSync(), isTrue);
    },
  );

  test('stage cancellation and pending journal recovery are explicit and non-mutating', () async {
    final (signedRoot, archiveRoot, reviewRoot) = roots();
    final signed = p.join(signedRoot, 'signed.pdf');
    final archive = p.join(archiveRoot, 'Package', 'Civil', filenameFor(_code));
    writeSigned(signed, extraPage: false);
    writeRichArchive(archive);
    final plan = await previewStageSignedPages(
      _params(signedRoot, archiveRoot, reviewRoot, recursive: false),
      _FixtureOcr.one(['signed.pdf']),
    );
    final journalRoot = at('journal');
    final authority = _stageAuthority(journalRoot);
    final cached = _cacheStage(
      authority,
      plan,
      signedRoot,
      archiveRoot,
      reviewRoot,
    );
    const caller = CallerIdentity('owner-1', 'session-1');
    final cancelled = authority.applyWithProgress(cached.planId, caller, (
      event,
    ) {
      if (event.done == 0) {
        expect(authority.cancel(cached.planId), isTrue);
      }
    });
    expect(cancelled.meta['effectState'], 'cancelled_clean');
    expect(Directory(reviewRoot).listSync(), isEmpty);

    final output = p.join(reviewRoot, 'Package', 'Civil', filenameFor(_code));
    final replacement = plan.steps.firstWhere(
      (s) => s.kind == StepKind.pdfFirstPageReplace,
    );
    final store = JournalStore(journalRoot);
    final pending = JournalRecord.create(
      journalId: 'slice7-pending',
      opId: 'rfi.stage_signed_first_pages',
      ts: '2026-08-25T00-00-00Z',
      ownerId: 'owner-1',
      sessionId: 'session-1',
      planDigest: 'digest',
    );
    pending.pendingStep = replacement;
    pending.state = JournalState.applying;
    store.write(pending);
    Directory(p.dirname(output)).createSync(recursive: true);
    File(output).writeAsBytesSync('UNCERTAIN PUBLISHED OUTPUT'.codeUnits);
    final recovery = store.recoverPending();
    expect(
      recovery.any(
        (e) =>
            e.journalId == 'slice7-pending' &&
            e.disposition == RecoveryDisposition.outcomeUnknown,
      ),
      isTrue,
    );
    expect(File(output).readAsStringSync(), 'UNCERTAIN PUBLISHED OUTPUT');
  });

  test(
    'changed approved inputs fail without publishing or recalculating',
    () async {
      final (signedRoot, archiveRoot, reviewRoot) = roots();
      final signed = p.join(signedRoot, 'signed.pdf');
      writeSigned(signed, extraPage: false);
      writeRichArchive(
        p.join(archiveRoot, 'Package', 'Civil', filenameFor(_code)),
      );
      final plan = await previewStageSignedPages(
        _params(signedRoot, archiveRoot, reviewRoot, recursive: false),
        _FixtureOcr.one(['signed.pdf']),
      );
      final authority = _stageAuthority(at('journal'));
      final cached = _cacheStage(
        authority,
        plan,
        signedRoot,
        archiveRoot,
        reviewRoot,
      );
      writeSigned(signed, extraPage: true);

      final result = authority.apply(
        cached.planId,
        const CallerIdentity('owner-1', 'session-1'),
      );
      expect(result.failed, 1);
      expect(
        File(p.join(reviewRoot, 'Package', 'Civil', filenameFor(_code)))
            .existsSync(),
        isFalse,
      );
    },
  );

  test(
    'operator Ventoy signed pages preview matches nine real RFI files',
    () {},
    skip: _operatorSkip,
  );
}
