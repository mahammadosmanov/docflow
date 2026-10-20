// Port of src-tauri/tests/pdf_orientation.rs.
//
// Apply and undo tests are skipped: they need the Dart FilesystemAuthority,
// which is not ported yet. The operator tests are skipped because they need
// operator-supplied documents and a real Tesseract install, as in Rust
// (where they are `#[ignore]`).
library;

import 'dart:io';
import 'dart:typed_data';

import 'package:docflow/engine/contracts.dart';
import 'package:docflow/engine/fs_ops.dart';
import 'package:docflow/engine/paths.dart';
import 'package:docflow/engine/plan_store.dart';
import 'package:docflow/engine/production_ports.dart';
import 'package:docflow/engine/rfi_system_sync.dart';
import 'package:docflow/engine/undo.dart';
import 'package:docflow/engine/errors.dart';
import 'package:docflow/engine/operations/pdf_orientation.dart';
import 'package:docflow/engine/pdf.dart' show inspect;
import 'package:docflow/engine/scan_index.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

import 'e9_pdf_fixture.dart';

const String _operatorSkip =
    'operator-supplied documents and Tesseract; ignored in Rust as well';

OrientationDetection _detection(int rotateClockwise, double confidence) =>
    OrientationDetection(
      rotateClockwise: rotateClockwise,
      confidence: confidence,
      method: 'fake',
    );

class _FakeDetector implements OrientationDetector {
  _FakeDetector([this.pages = const {}]);

  final Map<int, OrientationDetection> pages;

  @override
  Future<OrientationDetection> detectPage(String path, int pageIndex) async {
    final detection = pages[pageIndex];
    if (detection == null) {
      throw const OrientationError('missing fake result');
    }
    return detection;
  }

  @override
  String orientationDiagnostics() => 'fake orientation detector';
}

class _CountingDetector implements OrientationDetector {
  _CountingDetector(this.inner);

  final _FakeDetector inner;
  int calls = 0;

  @override
  Future<OrientationDetection> detectPage(String path, int pageIndex) {
    calls++;
    return inner.detectPage(path, pageIndex);
  }

  @override
  String orientationDiagnostics() => inner.orientationDiagnostics();
}

class _SameFingerprintReplacementDetector implements OrientationDetector {
  bool replaced = false;

  @override
  Future<OrientationDetection> detectPage(String path, int pageIndex) async {
    if (!replaced) {
      replaced = true;
      replacePdfTextPreservingFingerprint(path);
    }
    return _detection(90, 18.0);
  }

  @override
  String orientationDiagnostics() => 'same-fingerprint replacement fixture';
}

/// Port of `replace_pdf_text_preserving_fingerprint`: same byte length, same
/// modification time, so the fingerprint is unchanged but the content is not.
void replacePdfTextPreservingFingerprint(String path) {
  final file = File(path);
  final modified = file.lastModifiedSync();
  final bytes = Uint8List.fromList(file.readAsBytesSync());
  final before = 'ARCHIVE OLD FIRST PAGE'.codeUnits;
  final after = 'ARCHIVE NEW FIRST PAGE'.codeUnits;
  final start = _indexOf(bytes, before);
  if (start < 0) {
    throw StateError('replacement sentinel is missing');
  }
  bytes.setRange(start, start + before.length, after);
  file.writeAsBytesSync(bytes);
  file.setLastModifiedSync(modified);
}

int _indexOf(Uint8List bytes, List<int> needle) {
  for (var start = 0; start + needle.length <= bytes.length; start++) {
    var matched = true;
    for (var offset = 0; offset < needle.length; offset++) {
      if (bytes[start + offset] != needle[offset]) {
        matched = false;
        break;
      }
    }
    if (matched) {
      return start;
    }
  }
  return -1;
}

Object? _scanStat(Plan plan, String key) =>
    (plan.meta['scanIndex']! as Map<String, Object?>)[key];

FilesystemAuthority _orientationAuthority(String journalDir) =>
    FilesystemAuthority(
      journalDir: journalDir,
      ttl: const Duration(minutes: 5),
      ooxml: const ProductionFollowUpOoxml(),
      rfiLog: const ProductionRfiLogWorkbook(),
      rfiSystemSync: const RfiSystemSync(),
      pdf: const ProductionPdfEngine(),
      replicaPublish: const ProductionReplicaPublish(),
      undoJournal: undoJournal,
    );

Plan _cacheOrientation(
  FilesystemAuthority authority,
  Plan plan,
  String source,
  String review,
) {
  const opId = 'rfi.normalize_pdf_orientation';
  final scope = GrantScope.create('owner-1', 'session-1', 'preview-1', opId);
  final sourceGrant = authority.issueGrant(scope, source, GrantAccess.read);
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
      grantIds: [sourceGrant.grantId, reviewGrant.grantId],
    ),
    plan,
  );
}

void main() {
  late Directory fixture;

  setUp(() {
    fixture = Directory.systemTemp.createTempSync('e9_orientation');
  });

  tearDown(() {
    fixture.deleteSync(recursive: true);
  });

  String at(String name) => p.join(fixture.path, name);

  Directory mkdirs(String path) => Directory(path)..createSync(recursive: true);

  test(
    'orientation targets canonicalize metadata and reject unsafe inputs',
    () {
      const cases = <(int, int, int?)>[
        (-90, 90, 0),
        (0, 0, null),
        (0, 90, 90),
        (0, 180, 180),
        (0, 270, 270),
        (450, 270, 0),
        (810, 270, 0),
        (90, 90, 180),
        (90, 270, 0),
        (270, 90, 0),
        (180, 90, 270),
      ];
      for (final (current, correction, expected) in cases) {
        expect(
          orientationTargetRotation(current, _detection(correction, 10.0)),
          expected,
          reason: 'current $current correction $correction',
        );
      }
      expect(orientationTargetRotation(450, _detection(0, 10.0)), isNull);
      expect(
        () => orientationTargetRotation(45, _detection(90, 20.0)),
        throwsA(isA<OrientationError>()),
      );
      expect(
        () => orientationTargetRotation(0, _detection(45, 20.0)),
        throwsA(isA<OrientationError>()),
      );
      expect(
        () => orientationTargetRotation(0, _detection(90, 9.9)),
        throwsA(isA<OrientationError>()),
      );
      expect(
        () => orientationTargetRotation(0, _detection(90, double.nan)),
        throwsA(isA<OrientationError>()),
      );
    },
  );

  test(
    'preview rejects same-fingerprint content change during detection',
    () async {
      final source = mkdirs(at('system')).path;
      final review = mkdirs(at('review')).path;
      final pdf = p.join(source, 'CIV/RFI-0001/document.pdf');
      Directory(p.dirname(pdf)).createSync(recursive: true);
      writeRichArchive(pdf);
      final originalFingerprint = FileFingerprint.read(pdf);
      final originalHash = fileSha256(pdf);

      await expectLater(
        previewOrientation(
          PdfOrientationParams(rfiSystemRoot: source, reviewRoot: review),
          _SameFingerprintReplacementDetector(),
        ),
        throwsA(isA<BackendError>()),
      );

      expect(FileFingerprint.read(pdf), originalFingerprint);
      expect(fileSha256(pdf), isNot(originalHash));
      expect(inspect(pdf).pageCount, 3);
    },
  );

  test('preview scans only discipline trees and proposes confident rotations', () async {
    final source = mkdirs(at('system')).path;
    final review = mkdirs(at('review')).path;
    final pdf = p.join(source, '01_RFI/MEC/RFI-0001/document.pdf');
    final ignored = p.join(source, 'Certificates/not-an-rfi.pdf');
    Directory(p.dirname(pdf)).createSync(recursive: true);
    Directory(p.dirname(ignored)).createSync(recursive: true);
    writeRichArchive(pdf);
    writeRichArchive(ignored);
    final detector = _FakeDetector({
      0: _detection(90, 18.0),
      1: _detection(0, 22.0),
      2: _detection(180, 15.0),
    });

    final plan = await previewOrientation(
      PdfOrientationParams(rfiSystemRoot: source, reviewRoot: review),
      detector,
    );

    final rotate = plan.steps.firstWhere(
      (step) => step.kind == StepKind.pdfPageRotate,
    );
    expect(
      rotate.src,
      '${Directory(source).resolveSymbolicLinksSync()}/01_RFI/MEC/RFI-0001/document.pdf',
    );
    expect(
      rotate.dst,
      '${Directory(review).resolveSymbolicLinksSync()}/01_RFI/MEC/RFI-0001/document.pdf',
    );
    final rotations = rotate.meta['rotations']! as List<Object?>;
    expect(rotations, hasLength(2));
    expect((rotations[0]! as Map)['pageIndex'], 0);
    expect((rotations[0]! as Map)['targetRotation'], 90);
    expect((rotations[1]! as Map)['pageIndex'], 2);
    expect((rotations[1]! as Map)['targetRotation'], 180);
    expect(plan.meta['verifiedPages'], 3);
    expect(plan.meta['rotatedPages'], 2);
    expect(plan.warnings, isEmpty);
  });

  test('unchanged orientation PDF is reused from the durable index', () async {
    final source = mkdirs(at('system')).path;
    final review = mkdirs(at('review')).path;
    final pdf = p.join(source, 'MEC/RFI-0001/document.pdf');
    Directory(p.dirname(pdf)).createSync(recursive: true);
    writeRichArchive(pdf);
    final secondPdf = p.join(source, 'PRC/RFI-0002/document.pdf');
    Directory(p.dirname(secondPdf)).createSync(recursive: true);
    writeRichArchive(secondPdf);
    final detector = _CountingDetector(
      _FakeDetector({
        0: _detection(90, 18.0),
        1: _detection(0, 22.0),
        2: _detection(180, 15.0),
      }),
    );
    final scanIndex = ScanIndex.open(at('scan-index.sqlite3'));
    addTearDown(scanIndex.close);
    final params = PdfOrientationParams(
      rfiSystemRoot: source,
      reviewRoot: review,
    );

    final first = await previewOrientation(
      params,
      detector,
      scanIndex: scanIndex,
      ownerId: 'owner',
    );
    expect(_scanStat(first, 'analyzed'), 2);
    final expectedWorkers = (Platform.numberOfProcessors - 1).clamp(1, 2);
    expect(first.meta['inspectionWorkers'], expectedWorkers);
    expect(detector.calls, 6);

    final secondProgress = <ProgressEvent>[];
    final second = await previewOrientation(
      params,
      detector,
      progress: secondProgress.add,
      scanIndex: scanIndex,
      ownerId: 'owner',
    );
    expect(_scanStat(second, 'reused'), 2);
    expect(_scanStat(second, 'analyzed'), 0);
    expect(detector.calls, 6);
    final inspectionEvents = secondProgress
        .where((event) => event.phase == 'orientation_inspection')
        .toList();
    expect(inspectionEvents.first.done, 0);
    expect(inspectionEvents.last.done, 2);
    expect(
      secondProgress
          .where((event) => event.phase == 'orientation_analysis')
          .length,
      lessThanOrEqualTo(2),
    );
    expect(_stepsJson(first), _stepsJson(second));

    final originalFingerprint = FileFingerprint.read(pdf);
    final originalHash = fileSha256(pdf);
    replacePdfTextPreservingFingerprint(pdf);
    expect(FileFingerprint.read(pdf), originalFingerprint);
    expect(fileSha256(pdf), isNot(originalHash));
    final afterReplacement = await previewOrientation(
      params,
      detector,
      scanIndex: scanIndex,
      ownerId: 'owner',
    );
    expect(_scanStat(afterReplacement, 'reused'), 1);
    expect(_scanStat(afterReplacement, 'analyzed'), 1);
    expect(detector.calls, 9);

    final forced = await previewOrientation(
      params,
      detector,
      scanIndex: scanIndex,
      ownerId: 'owner',
      forceRescan: true,
    );
    expect(_scanStat(forced, 'reused'), 0);
    expect(_scanStat(forced, 'analyzed'), 2);
    expect(detector.calls, 15);
  });

  test('cached and new PDFs keep deterministic review order', () async {
    final source = mkdirs(at('system')).path;
    final review = mkdirs(at('review')).path;
    final mecPdf = p.join(source, 'MEC/RFI-0001/document.pdf');
    final prcPdf = p.join(source, 'PRC/RFI-0002/document.pdf');
    Directory(p.dirname(mecPdf)).createSync(recursive: true);
    mkdirs(review);
    writeRichArchive(mecPdf);
    final detector = _CountingDetector(
      _FakeDetector({
        0: _detection(90, 18.0),
        1: _detection(0, 22.0),
        2: _detection(0, 15.0),
      }),
    );
    final scanIndex = ScanIndex.open(at('scan-index.sqlite3'));
    addTearDown(scanIndex.close);
    final params = PdfOrientationParams(
      rfiSystemRoot: source,
      reviewRoot: review,
    );
    await previewOrientation(
      params,
      detector,
      scanIndex: scanIndex,
      ownerId: 'owner',
    );

    Directory(p.dirname(prcPdf)).createSync(recursive: true);
    writeRichArchive(prcPdf);
    final mixed = await previewOrientation(
      params,
      detector,
      scanIndex: scanIndex,
      ownerId: 'owner',
    );
    expect(_scanStat(mixed, 'reused'), 1);
    expect(_scanStat(mixed, 'analyzed'), 1);
    expect(detector.calls, 6);
    final sources = [
      for (final step in mixed.steps)
        if (step.kind == StepKind.pdfPageRotate) step.src,
    ];
    expect(sources, [
      '${Directory(source).resolveSymbolicLinksSync()}/MEC/RFI-0001/document.pdf',
      '${Directory(source).resolveSymbolicLinksSync()}/PRC/RFI-0002/document.pdf',
    ]);
  });

  test('one uncertain page fails the whole PDF closed', () async {
    final source = mkdirs(at('system')).path;
    final review = mkdirs(at('review')).path;
    final pdf = p.join(source, 'CIV/RFI-0002/document.pdf');
    Directory(p.dirname(pdf)).createSync(recursive: true);
    writeRichArchive(pdf);
    final detector = _FakeDetector({
      0: _detection(90, 18.0),
      1: _detection(0, 4.0),
      2: _detection(0, 15.0),
    });

    final plan = await previewOrientation(
      PdfOrientationParams(rfiSystemRoot: source, reviewRoot: review),
      detector,
    );

    expect(
      plan.steps.where((step) => step.kind == StepKind.pdfPageRotate),
      isEmpty,
    );
    expect(plan.meta['manualReviewFiles'], 1);
    expect(plan.warnings.first.key, 'warn.rfi.orientationNeedsReview');
  });

  test(
    'orientation discovery never follows PDF or directory symlinks',
    () async {
      final source = mkdirs(p.join(fixture.path, 'system', 'CIV')).path;
      final review = mkdirs(at('review')).path;
      final outside = mkdirs(at('outside')).path;
      writeRichArchive(p.join(outside, 'outside.pdf'));
      Link(p.join(source, 'linked-directory')).createSync(outside);
      Link(p.join(source, 'linked-file.pdf'))
          .createSync(p.join(outside, 'outside.pdf'));

      final plan = await previewOrientation(
        PdfOrientationParams(
          rfiSystemRoot: p.join(fixture.path, 'system'),
          reviewRoot: review,
        ),
        _FakeDetector(),
      );

      expect(plan.steps, isEmpty);
      expect(plan.warnings.first.key, 'warn.rfi.orientationNoPdfs');
    },
  );

  test('review root overlapping the source is rejected', () async {
    final source = mkdirs(at('system')).path;
    final overlapping = mkdirs(p.join(source, 'review')).path;
    final pdf = p.join(source, 'CIV/RFI-0003/document.pdf');
    Directory(p.dirname(pdf)).createSync(recursive: true);
    writeRichArchive(pdf);

    final plan = await previewOrientation(
      PdfOrientationParams(rfiSystemRoot: source, reviewRoot: overlapping),
      _FakeDetector(),
    );

    expect(plan.steps, isEmpty);
    expect(
      plan.warnings.first.key,
      'error.rfi.orientationReviewRootMustBeSeparate',
    );
  });

  test(
    'apply preserves source, publishes no-clobber, and undoes an exact copy',
    () async {
      final source = at('system');
      final review = at('review');
      final pdf = p.join(source, 'MEC', 'RFI-0003', 'document.pdf');
      mkdirs(p.dirname(pdf));
      mkdirs(review);
      writeRichArchive(pdf);
      final sourceHash = fileSha256(pdf);
      final plan = await previewOrientation(
        PdfOrientationParams(rfiSystemRoot: source, reviewRoot: review),
        _FakeDetector({
          0: _detection(90, 18.0),
          1: _detection(0, 22.0),
          2: _detection(0, 15.0),
        }),
      );
      final authority = _orientationAuthority(at('journal'));
      final cached = _cacheOrientation(authority, plan, source, review);
      const caller = CallerIdentity('owner-1', 'session-1');

      final result = authority.apply(cached.planId, caller);
      final output = p.join(review, 'MEC', 'RFI-0003', 'document.pdf');
      expect((result.applied, result.failed), (3, 0));
      expect(fileSha256(pdf), sourceHash);
      expect(inspect(output).pages[0].rotation, 90);
      final journal = authority.journals.read(result.journalId);
      expect(journal.steps.any((step) => step.kind == StepKind.copy), isTrue);

      final undone = authority.undo(result.journalId, caller);
      expect(undone.failed, 0);
      expect(File(output).existsSync(), isFalse);
    },
  );

  test('apply rejects source drift without creating a review PDF', () async {
    final source = at('system');
    final review = at('review');
    final pdf = p.join(source, 'ELE', 'RFI-0004', 'document.pdf');
    mkdirs(p.dirname(pdf));
    mkdirs(review);
    writeRichArchive(pdf);
    final plan = await previewOrientation(
      PdfOrientationParams(rfiSystemRoot: source, reviewRoot: review),
      _FakeDetector({
        0: _detection(90, 18.0),
        1: _detection(0, 22.0),
        2: _detection(0, 15.0),
      }),
    );
    final authority = _orientationAuthority(at('journal'));
    final cached = _cacheOrientation(authority, plan, source, review);
    File(pdf).writeAsBytesSync([
      ...File(pdf).readAsBytesSync(),
      ...'\n% operator edit'.codeUnits,
    ], flush: true);

    final result = authority.apply(
      cached.planId,
      const CallerIdentity('owner-1', 'session-1'),
    );

    expect(result.failed, 1);
    expect(
      File(p.join(review, 'ELE', 'RFI-0004', 'document.pdf')).existsSync(),
      isFalse,
    );
    expect(Directory(p.join(review, 'ELE')).existsSync(), isFalse);
  });

  test(
    'operator orientation pages match visual review',
    () {},
    skip: _operatorSkip,
  );

  test(
    'operator orientation directory preview reaches the expected PDF',
    () {},
    skip: _operatorSkip,
  );

  test(
    'operator uncertain orientation page is reported without rotating',
    () {},
    skip: _operatorSkip,
  );
}

/// Serialized steps, for the same-plan comparison the Rust test performs with
/// `assert_eq!(first.steps, second.steps)`.
String _stepsJson(Plan plan) =>
    [for (final step in plan.steps) step.toJson()].toString();
