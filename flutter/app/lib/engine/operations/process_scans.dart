/// Port of src-tauri/src/operations/process_scans.rs (fast-cover Preview and
/// the Preview-revision path).
///
/// Dart differences from the Rust module, all deliberate:
/// - OCR, orientation and inspection are `Future`-based because Tesseract is
///   spawned as a child process. The Rust thread pool (`bounded_map_stream`)
///   becomes a bounded async pool in this isolate with the same worker count.
/// - `pdf::inspect`, `pdf::extraction_supported`, `pdf::MAX_PDF_BYTES` and the
///   orientation rules of `operations/pdf_orientation.rs` are not ported yet.
///   Minimal surfaces are declared below ([PdfInspector], [PdfInspection],
///   [OrientationDetection], [orientationTargetRotation]) so this module is
///   complete. When pdf.dart and pdf_orientation.dart land, move these
///   declarations there and import them.
library;

import 'dart:async';
import 'dart:collection';
import 'dart:io';
import 'dart:math' as math;

import 'package:crypto/crypto.dart' as crypto;
import 'package:path/path.dart' as p;
import 'package:uuid/uuid.dart';

import '../contracts.dart';
import '../errors.dart';
import '../naming.dart';
import '../parallel.dart' show workerCount;
import '../rfi_code.dart';

const double minConfidence = 0.8;
const String fastCoverOperationId = 'rfi.process_scans_fast_covers';
const String reviewDir = '_review';
const int maxScanPdfs = 10000;
const int maxScanPages = 20000;
const List<String> disciplineFolders = ['CIV', 'MEC', 'ELE', 'PRC'];

/// Mirrors `pdf::MAX_PDF_BYTES` until pdf.dart is ported.
const int maxPdfBytes = 512 * 1024 * 1024;

/// Mirrors `operations::pdf_orientation::MIN_ORIENTATION_CONFIDENCE`.
const double minOrientationConfidence = 10.0;

/// `(page index, absolute clockwise rotation, confidence)`.
typedef ApprovedRotation = (int, int, double);

// ---- OCR contract ---------------------------------------------------------

/// Port of `OcrFailure`.
sealed class OcrFailure implements Exception {
  const OcrFailure();

  String get message;
}

/// Port of `OcrFailure::EngineMissing`.
final class OcrEngineMissing extends OcrFailure {
  const OcrEngineMissing();

  @override
  String get message => 'OCR engine is missing';

  @override
  String toString() => 'OcrEngineMissing';
}

/// Port of `OcrFailure::Document(String)`.
final class OcrDocumentFailure extends OcrFailure {
  const OcrDocumentFailure(this.message);

  @override
  final String message;

  @override
  String toString() => 'OcrDocumentFailure: $message';
}

/// Port of `OcrRead`.
class OcrRead {
  const OcrRead({required this.matched, required this.source});

  final RfiMatch matched;
  final String source;

  @override
  bool operator ==(Object other) =>
      other is OcrRead && other.matched == matched && other.source == source;

  @override
  int get hashCode => Object.hash(matched, source);

  @override
  String toString() => 'OcrRead($matched, $source)';
}

/// Port of `operations::pdf_orientation::OrientationDetection`.
class OrientationDetection {
  const OrientationDetection({
    required this.rotateClockwise,
    required this.confidence,
    required this.method,
  });

  final int rotateClockwise;
  final double confidence;
  final String method;

  @override
  bool operator ==(Object other) =>
      other is OrientationDetection &&
      other.rotateClockwise == rotateClockwise &&
      other.confidence == confidence &&
      other.method == method;

  @override
  int get hashCode => Object.hash(rotateClockwise, confidence, method);
}

/// Carries the Rust `Err(String)` of orientation detection.
class OrientationError implements Exception {
  const OrientationError(this.message);

  final String message;

  @override
  String toString() => message;
}

/// Port of `pdf_orientation::OrientationDetector`.
///
/// The Rust trait's `diagnostics` is named [orientationDiagnostics] here
/// because [OcrAdapter.diagnostics] already owns that name on the same engine.
abstract interface class OrientationDetector {
  Future<OrientationDetection> detectPage(String path, int pageIndex);

  String orientationDiagnostics();
}

/// Port of `pdf_orientation::orientation_target_rotation`. Returns `null` when
/// the rendered page is already upright, otherwise the absolute target.
int? orientationTargetRotation(
  int currentRotation,
  OrientationDetection detection,
) {
  final current = currentRotation % 360;
  if (!const {0, 90, 180, 270}.contains(current)) {
    throw const OrientationError('PDF page rotation is not a right angle');
  }
  if (!detection.confidence.isFinite ||
      detection.confidence < minOrientationConfidence) {
    throw OrientationError(
      'orientation is uncertain (confidence ${detection.confidence.toStringAsFixed(1)})',
    );
  }
  if (!const {0, 90, 180, 270}.contains(detection.rotateClockwise)) {
    throw const OrientationError('orientation correction is not a right angle');
  }
  final target = (current + detection.rotateClockwise) % 360;
  return target != current ? target : null;
}

/// Port of the `OcrAdapter` trait. Default methods match the Rust defaults.
abstract class OcrAdapter {
  const OcrAdapter();

  /// Reads page 0 (or the whole document for single-page adapters).
  Future<OcrRead> read(String path);

  Future<OcrRead> readCoverPage(String path, int pageIndex) =>
      readPage(path, pageIndex);

  bool get supportsPageScan => false;

  /// Returns a cached inspection, or `null` to let the caller inspect.
  PdfInspectionOutcome? inspectPdf(String path) => null;

  bool get supportsPagePreviews => false;

  bool get supportsParallelAnalysis => false;

  bool get supportsOrientation => false;

  Future<OrientationDetection> detectOrientation(String path, int pageIndex) {
    throw const OrientationError('page orientation detection is unavailable');
  }

  Future<OcrRead> readPage(String path, int pageIndex) async {
    if (pageIndex == 0) {
      return read(path);
    }
    throw const OcrDocumentFailure('page-level OCR is unavailable');
  }

  String diagnostics();
}

// ---- PDF contract (minimal until pdf.dart is ported) ----------------------

/// Minimal port of the page facts from `pdf::PdfInspection` that Process Scans
/// reads.
class PdfPageFacts {
  const PdfPageFacts({required this.rotation});

  final int rotation;
}

/// Minimal port of `pdf::PdfInspection`. [extractionSupported] stands in for
/// `pdf::extraction_supported(facts)`, which the inspector evaluates.
class PdfInspection {
  const PdfInspection({
    required this.pageCount,
    required this.pages,
    required this.signed,
    required this.encrypted,
    required this.extractionSupported,
  });

  final int pageCount;
  final List<PdfPageFacts> pages;
  final bool signed;
  final bool encrypted;
  final bool extractionSupported;
}

/// Result of an inspection: facts, or the error text of a failed inspection.
class PdfInspectionOutcome {
  const PdfInspectionOutcome.ok(PdfInspection this.facts) : error = null;
  const PdfInspectionOutcome.failed(String this.error) : facts = null;

  final PdfInspection? facts;
  final String? error;
}

/// Port of `pdf::inspect`. Implementations throw on an unreadable PDF.
abstract interface class PdfInspector {
  PdfInspection inspect(String path);
}

// ---- Parameters -----------------------------------------------------------

/// Port of `ScanMode` (`rename_all = "snake_case"`).
enum ScanMode {
  multiRfi('multi_rfi'),
  singleRfi('single_rfi');

  const ScanMode(this.wire);

  final String wire;

  static ScanMode fromWire(String value) => ScanMode.values.firstWhere(
    (mode) => mode.wire == value,
    orElse: () => throw FormatException('unknown scan mode `$value`'),
  );
}

/// Port of `ProcessScansParams`.
class ProcessScansParams {
  const ProcessScansParams({
    required this.source,
    required this.destination,
    required this.organizeIntoRfiFolders,
    required this.keepOriginal,
    required this.correctOrientation,
    this.scanMode = ScanMode.multiRfi,
    this.trashRoot,
  });

  final String source;
  final String destination;
  final ScanMode scanMode;
  final bool organizeIntoRfiFolders;
  final bool keepOriginal;
  final bool correctOrientation;
  final String? trashRoot;
}

// ---- Internal records -----------------------------------------------------

/// Port of the Rust `Result` values that the OCR cache stores.
final class _Attempt<T> {
  const _Attempt.ok(T this.value) : error = null;
  const _Attempt.failed(Object this.error) : value = null;

  final T? value;
  final Object? error;
}

class _Candidate {
  _Candidate({
    required this.path,
    required this.pages,
    required this.sourceHash,
    required this.modified,
    required this.confidence,
    required this.source,
    required this.rotations,
  });

  final String path;
  final List<int>? pages;
  final String? sourceHash;
  final DateTime modified;
  final double confidence;
  final String source;
  final List<ApprovedRotation> rotations;
}

class _ReviewPages {
  const _ReviewPages({
    required this.path,
    required this.pages,
    required this.reason,
    required this.rotations,
  });

  final String path;
  final List<int> pages;
  final String reason;
  final List<ApprovedRotation> rotations;
}

class _PageGroup {
  _PageGroup({
    required this.code,
    required this.pages,
    required this.confidence,
    required this.source,
    required this.reason,
  });

  String? code;
  List<int> pages;
  double confidence;
  String source;
  String reason;
}

class _RfiGroup {
  _RfiGroup(this.code, this.files);

  final String code;
  final List<_Candidate> files;
}

/// Port of `CachedPageOcr`: replays a parallel analysis through the sequential
/// planner.
class _CachedPageOcr extends OcrAdapter {
  _CachedPageOcr({
    required this.reads,
    required this.inspections,
    required this.pageScan,
    required this.diagnosticsText,
    required this.orientations,
  });

  final Map<(String, int), _Attempt<OcrRead>> reads;
  final Map<String, PdfInspectionOutcome> inspections;
  final bool pageScan;
  final String diagnosticsText;
  final Map<(String, int), _Attempt<OrientationDetection>> orientations;

  @override
  Future<OcrRead> read(String path) => readPage(path, 0);

  @override
  bool get supportsPageScan => pageScan;

  @override
  PdfInspectionOutcome? inspectPdf(String path) => inspections[path];

  @override
  Future<OcrRead> readPage(String path, int pageIndex) async {
    final attempt = reads[(path, pageIndex)];
    if (attempt == null) {
      throw const OcrDocumentFailure(
        'parallel page OCR cache entry is missing',
      );
    }
    if (attempt.error case final OcrFailure failure) {
      throw failure;
    }
    return attempt.value!;
  }

  @override
  String diagnostics() => diagnosticsText;

  @override
  bool get supportsOrientation => orientations.isNotEmpty;

  @override
  Future<OrientationDetection> detectOrientation(
    String path,
    int pageIndex,
  ) async {
    final attempt = orientations[(path, pageIndex)];
    if (attempt == null) {
      throw const OrientationError('page orientation result is missing');
    }
    if (attempt.error case final OrientationError error) {
      throw error;
    }
    return attempt.value!;
  }
}

// ---- Preview --------------------------------------------------------------

/// Port of `preview_fast_covers`.
Future<Plan> previewFastCovers({
  required ProcessScansParams params,
  required OcrAdapter ocr,
  required PdfInspector inspector,
  void Function(ProgressEvent event)? progress,
  bool Function()? cancelled,
}) => _previewWithCoverMode(
  params: params,
  ocr: ocr,
  inspector: inspector,
  operationId: fastCoverOperationId,
  fastCoverMode: true,
  sourceSha256: null,
  progress: progress ?? (_) {},
  cancelled: cancelled ?? () => false,
);

Future<Plan> _previewWithCoverMode({
  required ProcessScansParams params,
  required OcrAdapter ocr,
  required PdfInspector inspector,
  required String operationId,
  required bool fastCoverMode,
  required Map<String, String>? sourceSha256,
  required void Function(ProgressEvent event) progress,
  required bool Function() cancelled,
}) async {
  final source = _canonicalDirectory(params.source, 'scan source');
  final destination = _canonicalDirectory(
    params.destination,
    'scan destination',
  );
  progress(
    ProgressEvent(phase: 'scan_discovery', done: 0, total: 0, subject: source),
  );
  final pdfs = _discoverPdfs(source);
  progress(
    ProgressEvent(
      phase: 'scan_discovery',
      done: pdfs.length,
      total: pdfs.length,
      subject: '',
    ),
  );

  if (ocr.supportsParallelAnalysis && pdfs.isNotEmpty) {
    if (cancelled()) {
      throw BackendError.cancelled();
    }
    final orientationSourceHashes = SplayTreeMap<String, String>();
    if (params.scanMode == ScanMode.multiRfi &&
        params.correctOrientation &&
        ocr.supportsOrientation) {
      for (final pdf in pdfs) {
        orientationSourceHashes[pdf] = await _hashPdf(pdf);
      }
    }

    var inspectionWorkers = 0;
    final inspections = <String, PdfInspectionOutcome>{};
    if (ocr.supportsPageScan) {
      var completed = 0;
      final (
        workers,
        results,
      ) = await _boundedAsync<String, PdfInspectionOutcome>(
        pdfs,
        (pdf) async => _inspect(inspector, pdf),
        (index, _) {
          completed += 1;
          progress(
            ProgressEvent(
              phase: 'scan_discovery',
              done: completed,
              total: pdfs.length,
              subject: _displayName(pdfs[index]),
            ),
          );
        },
      );
      inspectionWorkers = workers;
      for (var index = 0; index < pdfs.length; index++) {
        inspections[pdfs[index]] = results[index];
      }
    }

    final tasks = <(String, int)>[];
    final fastCoverPdfs = <String>{};
    var totalPages = 0;
    for (final pdf in pdfs) {
      final outcome = inspections[pdf];
      final pageCount = outcome?.facts?.pageCount ?? 1;
      totalPages += pageCount;
      if (totalPages > maxScanPages) {
        throw BackendError.validation(
          'scan source exceeds the $maxScanPages-page limit',
        );
      }
      if ((outcome == null || outcome.error == null) && pageCount > 0) {
        if (fastCoverMode &&
            params.scanMode == ScanMode.multiRfi &&
            pageCount > 1) {
          fastCoverPdfs.add(pdf);
        }
        final ocrPageCount = params.scanMode == ScanMode.singleRfi
            ? 1
            : pageCount;
        for (var page = 0; page < ocrPageCount; page++) {
          tasks.add((pdf, page));
        }
      }
    }

    var orientationWorkers = 0;
    final orientations = <(String, int), _Attempt<OrientationDetection>>{};
    if (params.scanMode == ScanMode.multiRfi &&
        params.correctOrientation &&
        ocr.supportsOrientation) {
      var completed = 0;
      final (
        workers,
        results,
      ) = await _boundedAsync<(String, int), _Attempt<OrientationDetection>>(
        tasks,
        (task) => _attempt<OrientationDetection, OrientationError>(
          () => ocr.detectOrientation(task.$1, task.$2),
        ),
        (index, _) {
          completed += 1;
          final (pdf, page) = tasks[index];
          progress(
            ProgressEvent(
              phase: 'scan_orientation',
              done: completed,
              total: tasks.length,
              subject: '${_displayName(pdf)} — #${page + 1}',
            ),
          );
        },
      );
      orientationWorkers = workers;
      for (var index = 0; index < tasks.length; index++) {
        orientations[tasks[index]] = results[index];
      }
    }

    var completed = 0;
    var ocrWorkers = 0;
    var ocrResults = <_Attempt<OcrRead>>[];
    if (tasks.isNotEmpty) {
      final (
        workers,
        results,
      ) = await _boundedAsync<(String, int), _Attempt<OcrRead>>(
        tasks,
        (task) => _attempt<OcrRead, OcrFailure>(() {
          final (pdf, page) = task;
          return fastCoverPdfs.contains(pdf)
              ? ocr.readCoverPage(pdf, page)
              : ocr.readPage(pdf, page);
        }),
        (index, _) {
          completed += 1;
          final (pdf, page) = tasks[index];
          progress(
            ProgressEvent(
              phase: 'scan_ocr',
              done: completed,
              total: tasks.length,
              subject: '${_displayName(pdf)} — #${page + 1}',
            ),
          );
        },
      );
      ocrWorkers = workers;
      ocrResults = results;
    }
    if (cancelled()) {
      throw BackendError.cancelled();
    }
    for (final entry in orientationSourceHashes.entries) {
      if (await _hashPdf(entry.key) != entry.value) {
        throw BackendError.pathInvalid(
          'scan source changed during orientation or OCR analysis: '
          '${_displayName(entry.key)}',
        );
      }
    }
    final cached = _CachedPageOcr(
      reads: {
        for (var index = 0; index < tasks.length; index++)
          tasks[index]: ocrResults[index],
      },
      inspections: inspections,
      pageScan: ocr.supportsPageScan,
      diagnosticsText: ocr.diagnostics(),
      orientations: orientations,
    );
    final plan = await _previewWithCoverMode(
      params: params,
      ocr: cached,
      inspector: inspector,
      operationId: operationId,
      fastCoverMode: fastCoverMode,
      sourceSha256: orientationSourceHashes,
      progress: progress,
      cancelled: cancelled,
    );
    plan.meta['workerCount'] = math.max(ocrWorkers, inspectionWorkers);
    plan.meta['parallelAnalysis'] = true;
    plan.meta['orientationWorkerCount'] = orientationWorkers;
    return plan;
  }

  final transferKind = params.keepOriginal ? StepKind.copy : StepKind.move;
  final requiresExistingRfiFolder =
      !fastCoverMode &&
      params.organizeIntoRfiFolders &&
      _destinationContainsExactRfiFolder(destination);
  final plan = _emptyPlan(operationId);
  final groups = <_RfiGroup>[];
  final failures = <(String, String)>[];
  final reviewPages = <_ReviewPages>[];
  final splitSources = SplayTreeMap<String, String>();
  var scannedPages = 0;

  for (var index = 0; index < pdfs.length; index++) {
    final pdf = pdfs[index];
    if (cancelled()) {
      throw BackendError.cancelled();
    }
    progress(
      ProgressEvent(
        phase: 'scan_planning',
        done: index,
        total: pdfs.length,
        subject: '',
      ),
    );
    PdfInspection? facts;
    if (ocr.supportsPageScan) {
      final outcome = ocr.inspectPdf(pdf) ?? _inspect(inspector, pdf);
      if (outcome.error != null) {
        failures.add((pdf, outcome.error!));
        continue;
      }
      facts = outcome.facts;
    }
    final pageCount = facts?.pageCount ?? 1;
    scannedPages += pageCount;
    if (scannedPages > maxScanPages) {
      throw BackendError.validation(
        'scan source exceeds the $maxScanPages-page limit',
      );
    }
    if (pageCount == 0) {
      failures.add((pdf, 'PDF has no pages'));
      continue;
    }

    var rotations = <ApprovedRotation>[];
    final shouldCorrectOrientation =
        params.scanMode == ScanMode.multiRfi && params.correctOrientation;
    String? orientationSourceHash;
    if (shouldCorrectOrientation) {
      orientationSourceHash = sourceSha256?[pdf] ?? await _hashPdf(pdf);
    }
    if (shouldCorrectOrientation) {
      try {
        final detected = <ApprovedRotation>[];
        for (var pageIndex = 0; pageIndex < pageCount; pageIndex++) {
          final orientation = await ocr.detectOrientation(pdf, pageIndex);
          final current = facts != null && pageIndex < facts.pages.length
              ? facts.pages[pageIndex].rotation
              : 0;
          final target = orientationTargetRotation(current, orientation);
          if (target != null) {
            detected.add((pageIndex, target, orientation.confidence));
          }
        }
        if (detected.isNotEmpty &&
            facts != null &&
            (facts.signed || facts.encrypted || !facts.extractionSupported)) {
          throw const OrientationError(
            'PDF features prevent safe orientation correction',
          );
        }
        rotations = detected;
        final expected = orientationSourceHash;
        if (expected != null && await _hashPdf(pdf) != expected) {
          throw BackendError.pathInvalid(
            'scan source changed during orientation analysis',
          );
        }
      } on OrientationError catch (error) {
        plan.warnings.add(
          Warning(
            key: 'warn.rfi.processScansOrientationReview',
            subject: '${_displayName(pdf)} — ${error.message}',
          ),
        );
        plan.steps.add(
          _step(
            transferKind,
            pdf,
            p.join(destination, reviewDir, _displayName(pdf)),
            'note.needs_review',
            {'reason': error.message},
          ),
        );
        continue;
      }
    }

    if (pageCount > 1 && params.scanMode == ScanMode.multiRfi) {
      if (facts == null || !facts.extractionSupported) {
        failures.add((pdf, 'PDF features prevent safe page separation'));
        continue;
      }
      final sourceHash = await _hashPdf(pdf);
      final expectedOrientationHash = orientationSourceHash;
      if (expectedOrientationHash != null &&
          expectedOrientationHash != sourceHash) {
        throw BackendError.pathInvalid('scan source changed during Preview');
      }
      final localGroups = <_PageGroup>[];
      _PageGroup? active;
      final seenCodes = <String>{};
      final reviewForPdf = <_ReviewPages>[];
      for (var pageIndex = 0; pageIndex < pageCount; pageIndex++) {
        if (cancelled()) {
          throw BackendError.cancelled();
        }
        final OcrRead read;
        try {
          read = fastCoverMode
              ? await ocr.readCoverPage(pdf, pageIndex)
              : await ocr.readPage(pdf, pageIndex);
        } on OcrEngineMissing {
          return Plan(
            planId: '',
            opId: operationId,
            steps: [],
            warnings: [Warning(key: 'warn.no_ocr', subject: ocr.diagnostics())],
            meta: {},
          );
        } on OcrDocumentFailure catch (error) {
          if (active != null) {
            active.pages.add(pageIndex);
          } else {
            active = _PageGroup(
              code: null,
              pages: [pageIndex],
              confidence: 0.0,
              source: '',
              reason: 'unreadable page: ${error.message}',
            );
          }
          continue;
        }

        // A strict, high-confidence RFI identity is the only group boundary.
        // Once a cover starts a group, every page stays with it until another
        // confirmed cover appears, even when continuation OCR is uncertain.
        if (fastCoverMode &&
            read.source.startsWith('ocr:fast-cover:uncertain')) {
          _flushPageGroup(active, localGroups);
          active = _PageGroup(
            code: null,
            pages: [pageIndex],
            confidence: read.matched.confidence,
            source: read.source,
            reason: 'possible RFI cover could not be confirmed',
          );
        } else if (_isConfirmedCover(read)) {
          final code = read.matched.code;
          final continues = active != null && active.code == code;
          if (continues) {
            active.pages.add(pageIndex);
            continue;
          }
          _flushPageGroup(active, localGroups);
          if (!seenCodes.add(code)) {
            active = _PageGroup(
              code: null,
              pages: [pageIndex],
              confidence: read.matched.confidence,
              source: read.source,
              reason: 'duplicate RFI page for $code',
            );
          } else {
            active = _PageGroup(
              code: code,
              pages: [pageIndex],
              confidence: read.matched.confidence,
              source: read.source,
              reason: '',
            );
          }
        } else if (active != null && active.code != null) {
          active.pages.add(pageIndex);
        } else if (read.matched.method == RfiMethod.miss &&
            !read.source.startsWith('ocr:ambiguous-or-unconfirmed:')) {
          if (active != null) {
            active.pages.add(pageIndex);
          } else {
            active = _PageGroup(
              code: null,
              pages: [pageIndex],
              confidence: 0.0,
              source: read.source,
              reason: 'page precedes the first exact RFI identity',
            );
          }
        } else {
          _flushPageGroup(active, localGroups);
          active = _PageGroup(
            code: null,
            pages: [pageIndex],
            confidence: read.matched.confidence,
            source: read.source,
            reason:
                'uncertain RFI evidence: ${_methodName(read.matched.method)}',
          );
        }
      }
      _flushPageGroup(active, localGroups);
      if (await _hashPdf(pdf) != sourceHash) {
        throw BackendError.pathInvalid('scan source changed during Preview');
      }
      splitSources[pdf] = sourceHash;
      for (final group in localGroups) {
        final groupCode = group.code;
        if (groupCode != null) {
          final pages = group.pages;
          final groupRotations = [
            for (final rotation in rotations)
              if (pages.contains(rotation.$1)) rotation,
          ];
          final candidate = _Candidate(
            path: pdf,
            pages: pages,
            sourceHash: sourceHash,
            modified: _modifiedTime(pdf),
            confidence: group.confidence,
            source: group.source,
            rotations: groupRotations,
          );
          final existing = groups.where((value) => value.code == groupCode);
          if (existing.isNotEmpty) {
            existing.first.files.add(candidate);
          } else {
            groups.add(_RfiGroup(groupCode, [candidate]));
          }
        } else {
          final groupRotations = [
            for (final rotation in rotations)
              if (group.pages.contains(rotation.$1)) rotation,
          ];
          plan.warnings.add(
            Warning(
              key: 'warn.no_code',
              subject: '${_displayName(pdf)} — ${_pageRange(group.pages)}',
            ),
          );
          reviewForPdf.add(
            _ReviewPages(
              path: pdf,
              pages: group.pages,
              reason: group.reason,
              rotations: groupRotations,
            ),
          );
        }
      }
      reviewPages.addAll(reviewForPdf);
      continue;
    }

    final OcrRead read;
    try {
      read = await ocr.readPage(pdf, 0);
    } on OcrEngineMissing {
      return Plan(
        planId: '',
        opId: operationId,
        steps: [],
        warnings: [Warning(key: 'warn.no_ocr', subject: ocr.diagnostics())],
        meta: {},
      );
    } on OcrDocumentFailure catch (error) {
      failures.add((pdf, error.message));
      continue;
    }
    if (!(read.matched.method == RfiMethod.strict ||
            read.matched.method == RfiMethod.ocrAlias) ||
        read.matched.confidence < minConfidence) {
      plan.warnings.add(
        Warning(
          key: read.matched.ok ? 'warn.low_confidence' : 'warn.no_code',
          subject: _displayName(pdf),
        ),
      );
      plan.steps.add(
        _step(
          transferKind,
          pdf,
          p.join(destination, reviewDir, _displayName(pdf)),
          'note.needs_review',
          {
            'confidence': read.matched.confidence,
            'method': _methodName(read.matched.method),
            'source': read.source,
          },
        ),
      );
      continue;
    }

    final sourceHash = rotations.isNotEmpty ? orientationSourceHash : null;
    if (sourceHash != null && await _hashPdf(pdf) != sourceHash) {
      throw BackendError.pathInvalid('scan source changed during Preview');
    }
    if (sourceHash != null) {
      splitSources[pdf] = sourceHash;
    }
    final candidate = _Candidate(
      path: pdf,
      pages: sourceHash != null ? [0] : null,
      sourceHash: sourceHash,
      modified: _modifiedTime(pdf),
      confidence: read.matched.confidence,
      source: read.source,
      rotations: rotations,
    );
    final code = read.matched.code;
    final existing = groups.where((value) => value.code == code);
    if (existing.isNotEmpty) {
      existing.first.files.add(candidate);
    } else {
      groups.add(_RfiGroup(code, [candidate]));
    }
  }

  if (pdfs.length > 1 && failures.length == pdfs.length) {
    return Plan(
      planId: '',
      opId: operationId,
      steps: [],
      warnings: [
        Warning(
          key: 'warn.ocr_broken',
          subject: '${failures[0].$2}  ||  ${ocr.diagnostics()}',
        ),
      ],
      meta: {},
    );
  }

  for (final (pdf, error) in failures) {
    final truncated = String.fromCharCodes(error.runes.take(200));
    plan.warnings.add(
      Warning(
        key: 'warn.unreadable',
        subject: '${_displayName(pdf)} — $truncated',
      ),
    );
    plan.steps.add(
      _step(
        transferKind,
        pdf,
        p.join(destination, reviewDir, _displayName(pdf)),
        'note.unreadable',
        {'error': error},
      ),
    );
  }

  for (final review in reviewPages) {
    final target = _reviewTarget(destination, review.path, review.pages);
    final candidate = _Candidate(
      path: review.path,
      pages: review.pages,
      sourceHash: splitSources[review.path],
      modified: _epoch,
      confidence: 0.0,
      source: '',
      rotations: review.rotations,
    );
    _pushCandidateOutput(plan, candidate, target, 'note.needs_review', {
      'reason': review.reason,
    }, params.keepOriginal);
  }

  for (final group in groups) {
    final code = group.code;
    final files = _stableSortedByModifiedDesc(group.files);
    final winner = files.removeAt(0);
    final (folder, target) = _processScanTargets(
      destination,
      code,
      params.organizeIntoRfiFolders,
    );
    if (_sameFile(winner.path, target)) {
      continue;
    }
    if (_pathExists(target)) {
      plan.warnings.add(
        Warning(key: 'warn.exists', subject: _displayName(winner.path)),
      );
      final reviewTarget = winner.pages == null
          ? p.join(destination, reviewDir, _displayName(winner.path))
          : _reviewTarget(destination, winner.path, winner.pages!);
      _pushCandidateOutput(plan, winner, reviewTarget, 'note.needs_review', {
        'code': code,
        'confidence': winner.confidence,
        'source': winner.source,
        'reason': 'target_exists',
        'target': target,
      }, params.keepOriginal);
    } else if (requiresExistingRfiFolder &&
        folder != null &&
        !FileSystemEntity.isDirectorySync(folder)) {
      plan.warnings.add(
        Warning(
          key: 'warn.unknown_rfi_folder',
          subject: '${_displayName(winner.path)} — $code',
        ),
      );
      final reviewTarget = winner.pages == null
          ? p.join(destination, reviewDir, _displayName(winner.path))
          : _reviewTarget(destination, winner.path, winner.pages!);
      _pushCandidateOutput(plan, winner, reviewTarget, 'note.needs_review', {
        'code': code,
        'confidence': winner.confidence,
        'source': winner.source,
        'reason': 'unknown_rfi_folder',
      }, params.keepOriginal);
    } else {
      final folderCreated =
          folder != null && !FileSystemEntity.isDirectorySync(folder);
      if (folder != null && folderCreated) {
        plan.steps.add(
          _step(StepKind.mkdir, null, folder, 'note.folder_created', {
            'code': code,
          }),
        );
      }
      _pushCandidateOutput(plan, winner, target, 'note.renamed', {
        'code': code,
        'confidence': winner.confidence,
        'source': winner.source,
        'intoFolder': params.organizeIntoRfiFolders,
        'folderCreated': folderCreated,
      }, params.keepOriginal);
    }

    for (final older in files) {
      plan.warnings.add(
        Warning(key: 'warn.duplicate', subject: _displayName(older.path)),
      );
      final reviewTarget = older.pages == null
          ? p.join(destination, reviewDir, _displayName(older.path))
          : _reviewTarget(destination, older.path, older.pages!);
      _pushCandidateOutput(plan, older, reviewTarget, 'note.needs_review', {
        'code': code,
        'confidence': older.confidence,
        'source': older.source,
        'reason': 'older_revision',
        'kept': _displayName(winner.path),
      }, params.keepOriginal);
    }
  }

  if (!params.keepOriginal && splitSources.isNotEmpty) {
    final trashRootParam = params.trashRoot;
    if (trashRootParam == null) {
      throw BackendError.pathInvalid(
        'recoverable trash is unavailable for batch scans',
      );
    }
    final trashRoot = _canonicalDirectory(trashRootParam, 'recoverable trash');
    for (final entry in splitSources.entries) {
      final trashName =
          'process-scans-${const Uuid().v4()}_${_displayName(entry.key)}';
      plan.steps.add(
        _step(
          StepKind.move,
          entry.key,
          p.join(trashRoot, trashName),
          'note.deleted',
          {'expectedSourceSha256': entry.value},
        ),
      );
    }
  }

  if (pdfs.isEmpty) {
    plan.warnings.add(Warning(key: 'warn.no_pdfs', subject: source));
  }
  _attachScanEditor(plan, destination, params.organizeIntoRfiFolders);
  progress(
    ProgressEvent(phase: 'preview_ready', done: 1, total: 1, subject: ''),
  );
  return plan;
}

void _attachScanEditor(Plan plan, String destination, bool organize) {
  final rows = <JsonObject>[];
  for (var index = 0; index < plan.steps.length; index++) {
    final step = plan.steps[index];
    if (step.noteKey != 'note.renamed') {
      continue;
    }
    final code = step.meta['code'];
    if (code is! String) {
      continue;
    }
    final target = step.dst;
    if (target == null) {
      continue;
    }
    final rowId = 'scan-output-$index';
    step.meta['scanEditRowId'] = rowId;
    final sourceFile = step.src == null ? '' : p.basename(step.src!);
    final evidenceSource = step.meta['source'];
    final evidenceKey =
        evidenceSource is String && evidenceSource.contains('CLV->CIV')
        ? 'rfi.fastScan.evidence.clvToCiv'
        : 'rfi.fastScan.evidence.confirmed';
    rows.add({
      '_rowId': rowId,
      'sourceFile': sourceFile,
      'evidenceKey': evidenceKey,
      'detectedCode': code,
      'code': code,
      'target': target,
    });
  }
  if (rows.isEmpty) {
    return;
  }
  plan.meta['scanEdit'] = {
    'destinationRoot': destination,
    'organizeIntoRfiFolders': organize,
  };
  plan.meta['editor'] = {
    'kind': 'rfiScan',
    'titleKey': 'rfi.fastScan.editor.title',
    'rowCountKey': 'rfi.fastScan.editor.rowCount',
    'groupField': '_rowId',
    'requiredFields': ['code'],
    'columns': [
      {
        'key': 'sourceFile',
        'labelKey': 'rfi.fastScan.column.sourceFile',
        'kind': 'readonly',
        'width': 170,
        'bulk': false,
      },
      {
        'key': 'evidenceKey',
        'labelKey': 'rfi.fastScan.column.evidence',
        'kind': 'readonly',
        'display': 'localizedKey',
        'width': 170,
        'bulk': false,
      },
      {
        'key': 'detectedCode',
        'labelKey': 'rfi.fastScan.column.detectedCode',
        'kind': 'readonly',
        'width': 270,
        'bulk': false,
      },
      {
        'key': 'code',
        'labelKey': 'rfi.fastScan.column.code',
        'kind': 'text',
        'width': 310,
        'bulk': false,
      },
      {
        'key': 'target',
        'labelKey': 'rfi.fastScan.column.target',
        'kind': 'readonly',
        'width': 360,
        'bulk': false,
      },
    ],
    'rows': rows,
  };
}

// ---- Revision -------------------------------------------------------------

const String _revisionInvalid = 'error.rfi.fastScan.revisionInvalid';

/// Port of `revise`: re-derives targets from the operator's corrected codes.
Plan revise(Plan plan, JsonObject payload) {
  if (plan.opId != fastCoverOperationId) {
    throw BackendError.validation(_revisionInvalid);
  }
  final editor = plan.meta['editor'];
  final originalRows = editor is Map ? editor['rows'] : null;
  if (originalRows is! List || originalRows.isEmpty) {
    throw BackendError.validation(_revisionInvalid);
  }
  final submittedRows = payload['rows'];
  if (submittedRows is! List) {
    throw BackendError.validation(_revisionInvalid);
  }
  if (originalRows.length != submittedRows.length) {
    throw BackendError.validation(_revisionInvalid);
  }
  final edit = plan.meta['scanEdit'];
  if (edit is! Map) {
    throw BackendError.validation(_revisionInvalid);
  }
  final destinationValue = edit['destinationRoot'];
  if (destinationValue is! String || !p.isAbsolute(destinationValue)) {
    throw BackendError.validation(_revisionInvalid);
  }
  final destination = destinationValue;
  final organizeValue = edit['organizeIntoRfiFolders'];
  if (organizeValue is! bool) {
    throw BackendError.validation(_revisionInvalid);
  }
  final organize = organizeValue;

  final originals = <String, JsonObject>{};
  for (final row in originalRows) {
    final rowMap = _asJsonObject(row);
    final rowId = rowMap['_rowId'];
    if (rowId is! String) {
      throw BackendError.validation(_revisionInvalid);
    }
    if (originals.containsKey(rowId)) {
      throw BackendError.validation(_revisionInvalid);
    }
    originals[rowId] = rowMap;
  }

  final seen = <String>{};
  final edits = <String, _RowEdit>{};
  final targets = <String>{};
  for (final submittedValue in submittedRows) {
    final submitted = _asJsonObject(submittedValue);
    final rowId = submitted['_rowId'];
    if (rowId is! String || !seen.add(rowId)) {
      throw BackendError.validation(_revisionInvalid);
    }
    final original = originals[rowId];
    if (original == null) {
      throw BackendError.validation(_revisionInvalid);
    }
    final rawCode = submitted['code'];
    if (rawCode is! String) {
      throw BackendError.validation('error.rfi.fastScan.codeInvalid');
    }
    final code = normalizeFullCode(rawCode);
    if (code == null) {
      throw BackendError.validation('error.rfi.fastScan.codeInvalid');
    }
    final expected = Map<String, Object?>.of(original)..['code'] = code;
    final unchangedExceptCode =
        submitted.length == expected.length &&
        expected.entries.every(
          (entry) =>
              entry.key == 'code' ||
              (submitted.containsKey(entry.key) &&
                  _jsonEquals(submitted[entry.key], entry.value)),
        );
    if (!unchangedExceptCode) {
      throw BackendError.validation(_revisionInvalid);
    }
    final (folder, target) = _processScanTargets(destination, code, organize);
    if (!p.isWithin(destination, target) || !targets.add(target)) {
      throw BackendError.validation('error.rfi.fastScan.targetExists');
    }
    if (folder == null) {
      throw BackendError.validation(_revisionInvalid);
    }
    _scanEditFolderExists(destination, folder);
    final FileSystemEntityType targetType;
    try {
      targetType = _linkType(target);
    } on FileSystemException {
      throw BackendError.pathInvalid(
        'scan output target changed during Preview revision',
      );
    }
    if (targetType != FileSystemEntityType.notFound) {
      throw BackendError.validation('error.rfi.fastScan.targetExists');
    }
    expected['target'] = target;
    edits[rowId] = _RowEdit(
      code: code,
      folder: folder,
      target: target,
      row: expected,
    );
  }
  if (seen.length != originals.length) {
    throw BackendError.validation(_revisionInvalid);
  }

  final steps = <Step>[];
  final createdFolders = <String>{};
  final revisedIds = <String>{};
  for (final originalStep in plan.steps) {
    if (originalStep.kind == StepKind.mkdir &&
        originalStep.noteKey == 'note.folder_created') {
      continue;
    }
    final rowId = originalStep.meta['scanEditRowId'];
    if (rowId is! String) {
      steps.add(originalStep);
      continue;
    }
    final edit = edits[rowId];
    if (edit == null) {
      throw BackendError.validation(_revisionInvalid);
    }
    if (!revisedIds.add(rowId)) {
      throw BackendError.validation(_revisionInvalid);
    }
    final folderCreated = !_directoryExists(edit.folder);
    if (folderCreated && createdFolders.add(edit.folder)) {
      steps.add(
        _step(StepKind.mkdir, null, edit.folder, 'note.folder_created', {
          'code': edit.code,
        }),
      );
    }
    final meta = _deepCopyMap(originalStep.meta)
      ..['code'] = edit.code
      ..['folderCreated'] = folderCreated;
    steps.add(
      Step(
        kind: originalStep.kind,
        src: originalStep.src,
        dst: edit.target,
        noteKey: originalStep.noteKey,
        meta: meta,
      ),
    );
  }
  if (revisedIds.length != edits.length) {
    throw BackendError.validation(_revisionInvalid);
  }

  final rows = <Object?>[];
  for (final originalRow in originalRows) {
    final rowId = _asJsonObject(originalRow)['_rowId'] as String;
    final edit = edits[rowId];
    if (edit == null) {
      throw BackendError.validation(_revisionInvalid);
    }
    rows.add(edit.row);
  }
  final meta = _deepCopyMap(plan.meta);
  (meta['editor'] as Map<String, Object?>)['rows'] = rows;
  return Plan(
    planId: '',
    opId: fastCoverOperationId,
    steps: steps,
    warnings: List<Warning>.of(plan.warnings),
    meta: meta,
  );
}

class _RowEdit {
  const _RowEdit({
    required this.code,
    required this.folder,
    required this.target,
    required this.row,
  });

  final String code;
  final String folder;
  final String target;
  final JsonObject row;
}

/// Port of `scan_edit_folder_exists`. Returns whether the folder exists and
/// rejects links, files and paths outside [destination].
bool _scanEditFolderExists(String destination, String folder) {
  if (folder == destination || !p.isWithin(destination, folder)) {
    throw BackendError.validation(_revisionInvalid);
  }
  final FileSystemEntityType rootType;
  try {
    rootType = _linkType(destination);
  } on FileSystemException {
    throw BackendError.pathInvalid(
      'scan destination changed during Preview revision',
    );
  }
  if (rootType != FileSystemEntityType.directory) {
    throw BackendError.pathInvalid(
      'scan destination changed during Preview revision',
    );
  }
  final relative = p.relative(folder, from: destination);
  var current = destination;
  for (final component in p.split(relative)) {
    current = p.join(current, component);
    final FileSystemEntityType type;
    try {
      type = _linkType(current);
    } on FileSystemException {
      throw BackendError.pathInvalid(
        'scan output folder changed during Preview revision',
      );
    }
    if (type == FileSystemEntityType.notFound) {
      return false;
    }
    if (type != FileSystemEntityType.directory) {
      throw BackendError.validation('error.rfi.fastScan.targetExists');
    }
  }
  return true;
}

/// Port of `directory_exists`.
bool _directoryExists(String folder) {
  final FileSystemEntityType type;
  try {
    type = _linkType(folder);
  } on FileSystemException {
    throw BackendError.pathInvalid(
      'scan output folder changed during Preview revision',
    );
  }
  if (type == FileSystemEntityType.notFound) {
    return false;
  }
  if (type != FileSystemEntityType.directory) {
    throw BackendError.validation('error.rfi.fastScan.targetExists');
  }
  return true;
}

// ---- Shared helpers -------------------------------------------------------

final DateTime _epoch = DateTime.fromMillisecondsSinceEpoch(0, isUtc: true);

/// Port of the bounded worker pool used for page analysis. Results are
/// returned in input order; [completed] runs in completion order.
Future<(int, List<R>)> _boundedAsync<T, R>(
  List<T> items,
  Future<R> Function(T item) task,
  void Function(int index, R result) completed,
) async {
  final workers = workerCount(items.length);
  if (items.isEmpty) {
    return (workers, <R>[]);
  }
  final results = List<R?>.filled(items.length, null);
  var next = 0;
  Object? callbackError;
  StackTrace? callbackStack;

  Future<void> worker() async {
    while (next < items.length) {
      final index = next;
      next += 1;
      final result = await task(items[index]);
      results[index] = result;
      if (callbackError == null) {
        try {
          completed(index, result);
        } on Object catch (error, stack) {
          callbackError ??= error;
          callbackStack ??= stack;
        }
      }
    }
  }

  await Future.wait([for (var index = 0; index < workers; index++) worker()]);
  if (callbackError != null) {
    Error.throwWithStackTrace(callbackError!, callbackStack!);
  }
  return (workers, [for (final result in results) result as R]);
}

Future<_Attempt<T>> _attempt<T, E extends Object>(
  FutureOr<T> Function() action,
) async {
  try {
    return _Attempt<T>.ok(await action());
  } on E catch (error) {
    return _Attempt<T>.failed(error);
  }
}

PdfInspectionOutcome _inspect(PdfInspector inspector, String path) {
  try {
    return PdfInspectionOutcome.ok(inspector.inspect(path));
  } on Object catch (error) {
    return PdfInspectionOutcome.failed(_errorText(error));
  }
}

String _errorText(Object error) => switch (error) {
  BackendError(:final message) => message,
  OcrFailure(:final message) => message,
  OrientationError(:final message) => message,
  _ => error.toString(),
};

List<String> _discoverPdfs(String source) {
  final pdfs = <String>[];
  try {
    for (final entity in Directory(source).listSync(followLinks: false)) {
      final name = p.basename(entity.path);
      if (name.startsWith('.') ||
          FileSystemEntity.typeSync(entity.path, followLinks: false) !=
              FileSystemEntityType.file ||
          _asciiLower(p.extension(name)) != '.pdf') {
        continue;
      }
      pdfs.add(entity.path);
      if (pdfs.length > maxScanPdfs) {
        break;
      }
    }
  } on FileSystemException {
    throw BackendError.pathInvalid('scan source could not be read');
  }
  if (pdfs.length > maxScanPdfs) {
    throw BackendError.validation(
      'scan source exceeds the $maxScanPdfs-PDF limit',
    );
  }
  pdfs.sort((left, right) => _displayName(left).compareTo(_displayName(right)));
  return pdfs;
}

String _asciiLower(String value) => String.fromCharCodes(
  value.codeUnits.map(
    (unit) => unit >= 0x41 && unit <= 0x5a ? unit + 32 : unit,
  ),
);

Future<String> _hashPdf(String path) async {
  final FileSystemEntityType type;
  try {
    type = FileSystemEntity.typeSync(path, followLinks: false);
  } on FileSystemException catch (error) {
    throw BackendError.pathInvalid(error.message);
  }
  if (type != FileSystemEntityType.file) {
    throw BackendError.pathInvalid('source PDF is not a bounded regular file');
  }
  final file = File(path);
  final int length;
  try {
    length = file.lengthSync();
  } on FileSystemException catch (error) {
    throw BackendError.pathInvalid(error.message);
  }
  if (length > maxPdfBytes) {
    throw BackendError.pathInvalid('source PDF is not a bounded regular file');
  }
  final digest = await crypto.sha256.bind(file.openRead()).first;
  return digest.toString();
}

String _canonicalDirectory(String path, String label) {
  final String resolved;
  try {
    resolved = Link(path).resolveSymbolicLinksSync();
  } on FileSystemException {
    throw BackendError.pathInvalid('$label does not exist');
  }
  if (!FileSystemEntity.isDirectorySync(resolved)) {
    throw BackendError.pathInvalid('$label is not a directory');
  }
  return resolved;
}

FileSystemEntityType _linkType(String path) =>
    FileSystemEntity.typeSync(path, followLinks: false);

bool _pathExists(String path) =>
    FileSystemEntity.typeSync(path) != FileSystemEntityType.notFound;

DateTime _modifiedTime(String path) {
  try {
    return File(path).lastModifiedSync();
  } on FileSystemException {
    return _epoch;
  }
}

bool _destinationContainsExactRfiFolder(String destination) =>
    _exactRfiFolderIn(destination) ||
    disciplineFolders.any(
      (discipline) => _exactRfiFolderIn(p.join(destination, discipline)),
    );

bool _exactRfiFolderIn(String directory) {
  final List<FileSystemEntity> entries;
  try {
    entries = Directory(directory).listSync(followLinks: false);
  } on FileSystemException {
    return false;
  }
  for (final entry in entries) {
    final name = p.basename(entry.path);
    if (FileSystemEntity.typeSync(entry.path, followLinks: false) !=
        FileSystemEntityType.directory) {
      continue;
    }
    final matched = find(name);
    if (matched.method == RfiMethod.strict && matched.code == name) {
      return true;
    }
  }
  return false;
}

void _flushPageGroup(_PageGroup? active, List<_PageGroup> groups) {
  if (active != null) {
    groups.add(active);
  }
}

bool _isConfirmedCover(OcrRead read) {
  final canonicalMatch = find(read.matched.code);
  return (read.matched.method == RfiMethod.strict ||
          read.matched.method == RfiMethod.ocrAlias) &&
      read.matched.confidence >= minConfidence &&
      canonicalMatch.code == read.matched.code &&
      canonicalMatch.method == RfiMethod.strict;
}

(String?, String) _processScanTargets(
  String destination,
  String code,
  bool organize,
) {
  final discipline = find(code).discipline;
  final disciplineRoot = disciplineFolders.contains(discipline)
      ? p.join(destination, discipline)
      : null;
  final root = disciplineRoot ?? destination;
  if (organize) {
    return (folderFor(root, code), targetFor(root, code));
  }
  return (disciplineRoot, flatTargetFor(root, code));
}

String _reviewTarget(String destination, String source, List<int> pages) {
  final stem = p.basenameWithoutExtension(source);
  final first = (pages.isEmpty ? 0 : pages.first) + 1;
  final last = (pages.isEmpty ? first - 1 : pages.last) + 1;
  return p.join(
    destination,
    reviewDir,
    '$stem - pages ${_pad4(first)}-${_pad4(last)}.pdf',
  );
}

String _pad4(int value) => value.toString().padLeft(4, '0');

String _pageRange(List<int> pages) => pages.isEmpty
    ? 'pages unavailable'
    : 'pages ${pages.first + 1}-${pages.last + 1}';

void _pushCandidateOutput(
  Plan plan,
  _Candidate candidate,
  String destination,
  String noteKey,
  JsonObject meta,
  bool keepOriginal,
) {
  final pages = candidate.pages;
  if (pages != null) {
    meta['pageIndexes'] = [for (final page in pages) page];
    final hash = candidate.sourceHash;
    if (hash == null) {
      throw StateError('split page candidate has a Preview source hash');
    }
    meta['sourceSha256'] = hash;
    if (candidate.rotations.isNotEmpty) {
      final rotations = <JsonObject>[];
      for (var outputPage = 0; outputPage < pages.length; outputPage++) {
        final sourcePage = pages[outputPage];
        for (final rotation in candidate.rotations) {
          if (rotation.$1 == sourcePage) {
            rotations.add({
              'pageIndex': outputPage,
              'targetRotation': rotation.$2,
              'confidence': rotation.$3,
            });
            break;
          }
        }
      }
      meta['rotations'] = rotations;
    }
    plan.steps.add(
      _step(
        StepKind.pdfPagesExtract,
        candidate.path,
        destination,
        noteKey,
        meta,
      ),
    );
  } else {
    plan.steps.add(
      _step(
        keepOriginal ? StepKind.copy : StepKind.move,
        candidate.path,
        destination,
        noteKey,
        meta,
      ),
    );
  }
}

List<_Candidate> _stableSortedByModifiedDesc(List<_Candidate> files) {
  final indexed = [
    for (var index = 0; index < files.length; index++) (index, files[index]),
  ];
  indexed.sort((left, right) {
    final byTime = right.$2.modified.compareTo(left.$2.modified);
    return byTime != 0 ? byTime : left.$1.compareTo(right.$1);
  });
  return [for (final entry in indexed) entry.$2];
}

Plan _emptyPlan(String operationId) => Plan(
  planId: '',
  opId: operationId,
  steps: <Step>[],
  warnings: <Warning>[],
  meta: <String, Object?>{},
);

Step _step(
  StepKind kind,
  String? source,
  String? destination,
  String noteKey,
  JsonObject meta,
) => Step(
  kind: kind,
  src: source,
  dst: destination,
  noteKey: noteKey,
  meta: meta,
);

String _displayName(String path) => p.basename(path);

String _methodName(RfiMethod method) => switch (method) {
  RfiMethod.strict => 'strict',
  RfiMethod.repaired => 'repaired',
  RfiMethod.ocrAlias => 'ocr_alias',
  RfiMethod.fuzzy => 'fuzzy',
  RfiMethod.miss => 'miss',
};

bool _sameFile(String left, String right) {
  if (!_pathExists(right)) {
    return false;
  }
  try {
    return Link(left).resolveSymbolicLinksSync() ==
        Link(right).resolveSymbolicLinksSync();
  } on FileSystemException {
    return false;
  }
}

JsonObject _asJsonObject(Object? value) {
  if (value is Map) {
    return Map<String, Object?>.of(value.cast<String, Object?>());
  }
  throw BackendError.validation(_revisionInvalid);
}

Map<String, Object?> _deepCopyMap(
  Map<String, Object?> source,
) => <String, Object?>{
  for (final entry in source.entries) entry.key: _deepCopyValue(entry.value),
};

Object? _deepCopyValue(Object? value) => switch (value) {
  Map() => _deepCopyMap(value.cast<String, Object?>()),
  List() => [for (final item in value) _deepCopyValue(item)],
  _ => value,
};

bool _jsonEquals(Object? left, Object? right) {
  if (left is Map && right is Map) {
    if (left.length != right.length) {
      return false;
    }
    for (final key in left.keys) {
      if (!right.containsKey(key) || !_jsonEquals(left[key], right[key])) {
        return false;
      }
    }
    return true;
  }
  if (left is List && right is List) {
    if (left.length != right.length) {
      return false;
    }
    for (var index = 0; index < left.length; index++) {
      if (!_jsonEquals(left[index], right[index])) {
        return false;
      }
    }
    return true;
  }
  return left == right;
}
