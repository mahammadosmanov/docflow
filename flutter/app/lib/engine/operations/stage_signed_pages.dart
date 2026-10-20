/// Port of src-tauri/src/operations/stage_signed_pages.rs.
///
/// Preview and revise only. The preview finds signed-scan pages whose OCR
/// yields an exact canonical RFI code, pairs each with the exact archive PDF
/// of that name, and plans a no-clobber `PdfFirstPageReplace` into the review
/// root. Warning-review copies go under `_review/`. Apply is executed by the
/// filesystem authority, which is not ported yet.
///
/// Differences from the Rust module, all deliberate:
/// - OCR is `Future`-based (Tesseract runs as a child process). The Rust
///   `bounded_map_stream` pool becomes [_boundedAsync]: same worker count,
///   bounded concurrency, and completion-order callbacks on this isolate.
/// - Directory and archive discovery run on the event loop instead of worker
///   threads. Results, ordering and the reported worker counts are identical.
library;

import 'dart:async';
import 'dart:collection';
import 'dart:io';
import 'dart:math' as math;

import 'package:path/path.dart' as p;

import '../contracts.dart';
import '../errors.dart';
import '../fs_ops.dart' show fileSha256;
import '../ocr.dart' show renderPageThumbnailDataUrl;
import '../parallel.dart' show workerCount;
import '../pdf.dart' show PdfInspection, inspect;
import '../pdf_publish.dart' show sameFileIdentity, temporaryPdfNameMatches;
import '../rfi_code.dart'
    show
        RfiMatch,
        RfiMethod,
        normalizeCandidateNumber,
        normalizeFullCode,
        prefix;
import 'process_scans.dart'
    show OcrAdapter, OcrDocumentFailure, OcrEngineMissing, OcrFailure, OcrRead;

/// Port of `stage_signed_pages::OPERATION_ID`.
const String stageSignedOperationId = 'rfi.stage_signed_first_pages';

const double _minConfidence = 0.8;
const int _maxSignedPdfs = 10000;
const int _maxSignedPages = 20000;
const int _maxArchiveEntries = 250000;
const int _maxPageComparisons = 20;
const int _maxPagePreviewBytes = 12 * 1024 * 1024;
const List<String> _disciplines = ['CIV', 'MEC', 'ELE', 'PRC'];

/// Port of `StageSignedPagesParams`.
class StageSignedPagesParams {
  const StageSignedPagesParams({
    required this.signedScans,
    required this.archiveRoot,
    required this.reviewRoot,
    required this.scanSubfolders,
  });

  final String signedScans;
  final String archiveRoot;
  final String reviewRoot;
  final bool scanSubfolders;
}

/// Port of `Candidate`.
class _Candidate {
  const _Candidate({
    required this.path,
    required this.pageIndex,
    required this.confidence,
    required this.method,
  });

  final String path;
  final int pageIndex;
  final double confidence;
  final String method;
}

/// Port of `PreparedArchive`.
class _PreparedArchive {
  const _PreparedArchive({
    required this.archive,
    required this.relative,
    required this.steps,
  });

  final String archive;
  final String relative;
  final List<Step> steps;
}

/// Port of `WarningReviewReason`.
class _ReviewReason {
  const _ReviewReason({
    required this.key,
    required this.pageNumber,
    required this.code,
    required this.detail,
  });

  final String key;
  final int? pageNumber;
  final String? code;
  final String detail;

  @override
  bool operator ==(Object other) =>
      other is _ReviewReason &&
      other.key == key &&
      other.pageNumber == pageNumber &&
      other.code == code &&
      other.detail == detail;

  @override
  int get hashCode => Object.hash(key, pageNumber, code, detail);

  JsonObject toJson() => {
    'key': key,
    'pageNumber': pageNumber,
    'code': code,
    'detail': detail,
  };
}

/// One OCR result: a read or the failure that the Rust `Result` carried.
class _ReadResult {
  const _ReadResult.success(OcrRead this.read) : failure = null;
  const _ReadResult.failure(OcrFailure this.failure) : read = null;

  final OcrRead? read;
  final OcrFailure? failure;
}

/// One inspection result: facts, or the text of the failed inspection.
class _InspectionResult {
  const _InspectionResult.success(PdfInspection this.facts) : error = null;
  const _InspectionResult.failure(String this.error) : facts = null;

  final PdfInspection? facts;
  final String? error;
}

/// Port of `CachedPageOcr`: serves the reads computed by the parallel pass.
class _CachedPageOcr extends OcrAdapter {
  const _CachedPageOcr({
    required this.reads,
    required this.pagePreviews,
    required this.diagnosticsText,
  });

  final Map<(String, int), _ReadResult> reads;
  final bool pagePreviews;
  final String diagnosticsText;

  @override
  Future<OcrRead> read(String path) => readPage(path, 0);

  @override
  bool get supportsPageScan => true;

  @override
  bool get supportsPagePreviews => pagePreviews;

  @override
  Future<OcrRead> readPage(String path, int pageIndex) async {
    final cached = reads[(path, pageIndex)];
    if (cached == null) {
      throw const OcrDocumentFailure(
        'parallel page OCR cache entry is missing',
      );
    }
    final failure = cached.failure;
    if (failure != null) {
      throw failure;
    }
    return cached.read!;
  }

  @override
  String diagnostics() => diagnosticsText;
}

/// Port of `PdfRole`.
enum _PdfRole { archive, signedScan }

/// Port of `ArchiveIndexResult`.
class _ArchiveIndexResult {
  const _ArchiveIndexResult({
    required this.matches,
    required this.workerCount,
    required this.directoriesScanned,
    required this.entriesScanned,
  });

  final SplayTreeMap<String, List<String>> matches;
  final int workerCount;
  final int directoriesScanned;
  final int entriesScanned;
}

/// Port of `ArchiveDirectoryBatch`.
class _ArchiveDirectoryBatch {
  final List<String> directories = [];
  final List<(String, String)> matches = [];
  int entriesScanned = 0;
}

/// Port of `SignedDirectoryBatch`.
class _SignedDirectoryBatch {
  final List<String> directories = [];
  final List<String> files = [];
}

/// Mutable page-preview byte budget (`page_preview_bytes` in Rust).
class _PreviewBudget {
  int used = 0;
}

/// Port of `preview`. [progress] and [cancelled] mirror the Rust closures;
/// [cancelled] is consulted at the same points as the Rust code.
Future<Plan> previewStageSignedPages(
  StageSignedPagesParams params,
  OcrAdapter ocr, {
  void Function(ProgressEvent event)? progress,
  bool Function()? cancelled,
}) async {
  void emit(ProgressEvent event) => progress?.call(event);
  bool isCancelled() => cancelled?.call() ?? false;

  final signedRoot = _canonicalDirectory(params.signedScans, 'signed scans');
  final archiveRoot = _canonicalDirectory(params.archiveRoot, 'archive root');
  final reviewRoot = _canonicalDirectory(params.reviewRoot, 'review root');
  if (_overlaps(reviewRoot, archiveRoot) || _overlaps(reviewRoot, signedRoot)) {
    return Plan(
      planId: '',
      opId: stageSignedOperationId,
      steps: [],
      warnings: [_warning('error.rfi.reviewRootMustBeSeparate', '')],
      meta: _emptyMeta(),
    );
  }
  emit(
    ProgressEvent(
      phase: 'signed_discovery',
      done: 0,
      total: 0,
      subject: signedRoot,
    ),
  );
  final discovery = _signedPdfs(signedRoot, params.scanSubfolders, emit);
  final pdfs = discovery.$1;
  final discoveryWorkers = discovery.$2;
  emit(
    ProgressEvent(
      phase: 'signed_discovery',
      done: pdfs.length,
      total: pdfs.length,
      subject: signedRoot,
    ),
  );
  final plan = _emptyPlan();
  final warningReviews = SplayTreeMap<String, List<_ReviewReason>>(
    _comparePaths,
  );
  if (pdfs.isEmpty) {
    plan.warnings.add(_warning('warn.rfi.noSignedPdfs', signedRoot));
    return plan;
  }

  if (ocr.supportsParallelAnalysis) {
    if (isCancelled()) {
      throw BackendError.cancelled();
    }
    var inspected = 0;
    final (inspectionWorkers, inspections) =
        await _boundedAsync<String, _InspectionResult>(
          pdfs,
          (pdf) async => _inspectOutcome(pdf),
          (index, _) {
            inspected++;
            emit(
              ProgressEvent(
                phase: 'signed_inspection',
                done: inspected,
                total: pdfs.length,
                subject: _displayName(pdfs[index]),
              ),
            );
          },
        );
    final tasks = <(String, int)>[];
    for (var index = 0; index < pdfs.length; index++) {
      final facts = inspections[index].facts;
      if (facts != null && _fidelitySupported(facts, _PdfRole.signedScan)) {
        if (tasks.length + facts.pageCount > _maxSignedPages) {
          throw BackendError.validation(
            'signed scans exceed the $_maxSignedPages-page limit',
          );
        }
        for (var pageIndex = 0; pageIndex < facts.pageCount; pageIndex++) {
          tasks.add((pdfs[index], pageIndex));
        }
      }
    }
    var completed = 0;
    final (ocrWorkers, reads) = await _boundedAsync<(String, int), _ReadResult>(
      tasks,
      (task) => _readPageResult(ocr, task.$1, task.$2),
      (index, _) {
        completed++;
        final task = tasks[index];
        emit(
          ProgressEvent(
            phase: 'signed_ocr',
            done: completed,
            total: tasks.length,
            subject: _candidateDisplay(task.$1, task.$2),
          ),
        );
      },
    );
    if (isCancelled()) {
      throw BackendError.cancelled();
    }
    final cached = _CachedPageOcr(
      reads: {
        for (var index = 0; index < tasks.length; index++)
          tasks[index]: reads[index],
      },
      pagePreviews: ocr.supportsPagePreviews,
      diagnosticsText: ocr.diagnostics(),
    );
    final inner = await previewStageSignedPages(
      params,
      cached,
      progress: progress,
      cancelled: cancelled,
    );
    inner.meta['workerCount'] = math.max(
      math.max(ocrWorkers, inspectionWorkers),
      discoveryWorkers,
    );
    inner.meta['parallelAnalysis'] = true;
    return inner;
  }

  final groups = SplayTreeMap<String, List<_Candidate>>();
  var scannedPages = 0;
  for (var index = 0; index < pdfs.length; index++) {
    final pdf = pdfs[index];
    if (isCancelled()) {
      throw BackendError.cancelled();
    }
    emit(
      ProgressEvent(
        phase: 'signed_classification',
        done: index,
        total: pdfs.length,
        subject: _displayName(pdf),
      ),
    );
    final int pageCount;
    if (ocr.supportsPageScan) {
      final inspected = _inspectOutcome(pdf);
      final facts = inspected.facts;
      if (facts != null && _fidelitySupported(facts, _PdfRole.signedScan)) {
        pageCount = facts.pageCount;
      } else if (facts != null) {
        _pushWarningReview(
          plan,
          warningReviews,
          signedRoot,
          'warn.rfi.pdfFidelityUnsupported',
          pdf,
          null,
          null,
          _fidelityReason(facts, _PdfRole.signedScan),
        );
        continue;
      } else {
        _pushWarningReview(
          plan,
          warningReviews,
          signedRoot,
          'warn.rfi.pdfFidelityUnsupported',
          pdf,
          null,
          null,
          inspected.error!,
        );
        continue;
      }
    } else {
      pageCount = 1;
    }
    if (pageCount == 0) {
      _pushWarningReview(
        plan,
        warningReviews,
        signedRoot,
        'warn.rfi.pdfFidelityUnsupported',
        pdf,
        null,
        null,
        'PDF has no pages',
      );
      continue;
    }
    scannedPages += pageCount;
    if (scannedPages > _maxSignedPages) {
      throw BackendError.validation(
        'signed scans exceed the $_maxSignedPages-page limit',
      );
    }
    if (pageCount > 1) {
      plan.warnings.add(
        _warning('warn.rfi.signedMultiPage', '${_displayName(pdf)} — $pageCount'),
      );
    }
    for (var pageIndex = 0; pageIndex < pageCount; pageIndex++) {
      if (isCancelled()) {
        throw BackendError.cancelled();
      }
      emit(
        ProgressEvent(
          phase: 'signed_classification',
          done: index,
          total: pdfs.length,
          subject: _candidateDisplay(pdf, pageIndex),
        ),
      );
      final read = await _readPageResult(ocr, pdf, pageIndex);
      final failure = read.failure;
      if (failure is OcrEngineMissing) {
        return Plan(
          planId: '',
          opId: stageSignedOperationId,
          steps: [],
          warnings: [_warning('warn.no_ocr', ocr.diagnostics())],
          meta: _emptyMeta(),
        );
      }
      if (failure is OcrDocumentFailure) {
        _pushWarningReview(
          plan,
          warningReviews,
          signedRoot,
          'warn.rfi.signedUnreadable',
          pdf,
          pageIndex,
          null,
          failure.message,
        );
        continue;
      }
      final ocrRead = read.read!;
      final code = _canonicalStageCode(ocrRead.matched);
      if (code == null) {
        _pushWarningReview(
          plan,
          warningReviews,
          signedRoot,
          'warn.no_code',
          pdf,
          pageIndex,
          null,
          'No conflict-free exact canonical RFI code was recognized',
        );
        continue;
      }
      if (ocrRead.matched.confidence < _minConfidence) {
        final detail =
            'OCR score ${(ocrRead.matched.confidence * 100).toStringAsFixed(0)}% '
            'is below the 80% threshold';
        _pushWarningReview(
          plan,
          warningReviews,
          signedRoot,
          'warn.low_confidence',
          pdf,
          pageIndex,
          code,
          detail,
        );
        continue;
      }
      groups
          .putIfAbsent(code, () => [])
          .add(
            _Candidate(
              path: pdf,
              pageIndex: pageIndex,
              confidence: ocrRead.matched.confidence,
              method: ocrRead.source,
            ),
          );
    }
  }

  final unique = SplayTreeMap<String, _Candidate>();
  for (final entry in groups.entries) {
    final code = entry.key;
    final candidates = entry.value;
    if (candidates.length != 1) {
      final names = candidates
          .map((candidate) => _candidateDisplay(candidate.path, candidate.pageIndex))
          .join(', ');
      for (final candidate in candidates) {
        _recordWarningReview(
          warningReviews,
          candidate.path,
          candidate.pageIndex,
          code,
          'warn.rfi.signedDuplicate',
          'The same RFI code was recognized on more than one signed page',
        );
      }
      plan.warnings.add(
        _warning(
          'warn.rfi.signedDuplicate',
          _truncateChars('$code — $names — duplicate signed pages', 300),
        ),
      );
    } else {
      unique[code] = candidates.first;
    }
  }

  final archiveScan = _archiveIndex(
    archiveRoot,
    unique.keys.toSet(),
    emit,
    isCancelled,
  );
  plan.meta['archiveRecursive'] = true;
  plan.meta['archiveWorkerCount'] = archiveScan.workerCount;
  plan.meta['archiveDirectoriesScanned'] = archiveScan.directoriesScanned;
  plan.meta['archiveEntriesScanned'] = archiveScan.entriesScanned;
  final archiveMatches = archiveScan.matches;

  final editorRows = <Map<String, Object?>>[];
  final archiveChoices = <String, Object?>{};
  final archiveChoiceLabels = <String, Object?>{
    '': 'rfi.signed.archive.choose',
  };
  final candidateSteps = <String, Object?>{};
  final pageComparisons = <String, Object?>{};
  final previewBudget = _PreviewBudget();
  var pagePreviewOmitted = 0;
  var pagePreviewRowsAttempted = 0;
  final comparisonTotal = unique.length;
  var comparisonIndex = 0;
  for (final entry in unique.entries) {
    final code = entry.key;
    final candidate = entry.value;
    if (isCancelled()) {
      throw BackendError.cancelled();
    }
    emit(
      ProgressEvent(
        phase: 'comparison_preparation',
        done: comparisonIndex,
        total: comparisonTotal,
        subject: '${_candidateDisplay(candidate.path, candidate.pageIndex)} · $code',
      ),
    );
    comparisonIndex++;

    final matches = archiveMatches[code] ?? const <String>[];
    if (matches.isEmpty) {
      _pushWarningReview(
        plan,
        warningReviews,
        signedRoot,
        'warn.rfi.archiveMissing',
        candidate.path,
        candidate.pageIndex,
        code,
        'No exact archive PDF filename was found anywhere below the selected RFI System',
      );
      continue;
    }
    final inspected = _inspectOutcome(candidate.path);
    final PdfInspection signedFacts;
    final signedInspection = inspected.facts;
    if (signedInspection != null &&
        _fidelitySupported(signedInspection, _PdfRole.signedScan)) {
      signedFacts = signedInspection;
    } else if (signedInspection != null) {
      _pushWarningReview(
        plan,
        warningReviews,
        signedRoot,
        'warn.rfi.pdfFidelityUnsupported',
        candidate.path,
        candidate.pageIndex,
        code,
        _fidelityReason(signedInspection, _PdfRole.signedScan),
      );
      continue;
    } else {
      _pushWarningReview(
        plan,
        warningReviews,
        signedRoot,
        'warn.rfi.pdfFidelityUnsupported',
        candidate.path,
        candidate.pageIndex,
        code,
        inspected.error!,
      );
      continue;
    }
    if (signedFacts.pageCount == 0) {
      _pushWarningReview(
        plan,
        warningReviews,
        signedRoot,
        'warn.rfi.pdfFidelityUnsupported',
        candidate.path,
        candidate.pageIndex,
        code,
        'PDF has no pages',
      );
      continue;
    }
    final pageFacts = candidate.pageIndex < signedFacts.pages.length
        ? signedFacts.pages[candidate.pageIndex]
        : null;
    if (pageFacts == null || pageFacts.widgets > 0) {
      _pushWarningReview(
        plan,
        warningReviews,
        signedRoot,
        'warn.rfi.pdfFidelityUnsupported',
        candidate.path,
        candidate.pageIndex,
        code,
        'The signed page contains form widgets and requires manual review',
      );
      continue;
    }
    final String signedHash;
    try {
      signedHash = fileSha256(candidate.path);
    } on Object {
      throw BackendError.pathInvalid('signed PDF could not be hashed');
    }
    final rowId = 'signed-$code';
    final candidateWarningStart = plan.warnings.length;
    final prepared = <_PreparedArchive>[];
    for (final archivePdf in matches) {
      final candidatePlan = _prepareArchiveCandidate(
        code,
        rowId,
        candidate,
        archivePdf,
        signedFacts,
        signedHash,
        signedRoot,
        archiveRoot,
        reviewRoot,
        plan,
      );
      if (candidatePlan != null) {
        prepared.add(candidatePlan);
      }
    }
    if (prepared.isEmpty) {
      Warning? sourceWarning;
      for (final item in plan.warnings.skip(candidateWarningStart)) {
        if (item.key != 'warn.rfi.staleTempFound') {
          sourceWarning = item;
          break;
        }
      }
      if (sourceWarning != null) {
        _recordWarningReview(
          warningReviews,
          candidate.path,
          candidate.pageIndex,
          code,
          sourceWarning.key,
          'No archive candidate passed all fidelity, destination, and safety checks',
        );
      }
      continue;
    }
    if (signedFacts.pageCount > 1 && !ocr.supportsPageScan) {
      _pushWarningReview(
        plan,
        warningReviews,
        signedRoot,
        'warn.rfi.signedExtraPages',
        candidate.path,
        candidate.pageIndex,
        code,
        'Additional signed-scan pages could not be independently identified',
      );
    }
    final ambiguous = prepared.length > 1;
    if (ambiguous) {
      final names = prepared.map((item) => item.relative).join(', ');
      plan.warnings.add(
        _warning(
          'warn.rfi.archiveAmbiguous',
          _truncateChars(
            '${_candidateDisplay(candidate.path, candidate.pageIndex)} — $code — '
            '$names — choose the correct archive candidate',
            300,
          ),
        ),
      );
      _recordWarningReview(
        warningReviews,
        candidate.path,
        candidate.pageIndex,
        code,
        'warn.rfi.archiveAmbiguous',
        'More than one exact archive PDF exists; choose the correct candidate',
      );
    }
    var selected = '';
    final choices = <Object?>[];
    if (ambiguous) {
      choices.add('');
    }
    final rowCandidates = <String, Object?>{};
    final previewAllowed =
        ocr.supportsPagePreviews &&
        pagePreviewRowsAttempted < _maxPageComparisons;
    if (previewAllowed) {
      pagePreviewRowsAttempted++;
    }
    String? signedPreview;
    if (previewAllowed) {
      final value = await _thumbnailOrNull(candidate.path, candidate.pageIndex);
      if (value != null) {
        signedPreview = _acceptPagePreview(value, previewBudget);
      }
    }
    final archivePreviews = <String, Object?>{};
    for (var index = 0; index < prepared.length; index++) {
      final item = prepared[index];
      final token = '$rowId-candidate-${index + 1}';
      if (!ambiguous && index == 0) {
        selected = token;
      }
      choices.add(token);
      archiveChoiceLabels[token] = item.relative;
      if (previewAllowed) {
        final value = await _thumbnailOrNull(item.archive, 0);
        if (value != null) {
          final accepted = _acceptPagePreview(value, previewBudget);
          if (accepted != null) {
            archivePreviews[token] = accepted;
          }
        }
      }
      rowCandidates[token] = [for (final step in item.steps) step.toJson()];
    }
    if (!ambiguous) {
      _extendCandidateSteps(plan.steps, prepared[0].steps);
    }
    archiveChoices[code] = choices;
    candidateSteps[rowId] = rowCandidates;
    if (signedPreview != null || archivePreviews.isNotEmpty) {
      pageComparisons[rowId] = {
        'signed': signedPreview,
        'archives': archivePreviews,
      };
    } else {
      pagePreviewOmitted++;
    }
    editorRows.add({
      '_rowId': rowId,
      'enabled': true,
      'code': code,
      'confidence': candidate.confidence,
      'signedFile': _candidateDisplay(candidate.path, candidate.pageIndex),
      'archiveFile': selected,
    });
  }

  _addWarningReviewSteps(plan, warningReviews, signedRoot, reviewRoot);
  plan.meta['archiveCandidateSteps'] = candidateSteps;
  plan.meta['pageComparisons'] = pageComparisons;
  plan.meta['pagePreviewOmitted'] = pagePreviewOmitted;
  plan.meta['editor'] = <String, Object?>{
    'titleKey': 'rfi.signed.editor.title',
    'rowCountKey': 'rfi.signed.editor.rowCount',
    'groupField': 'code',
    'selectionEnabled': false,
    'bulkEnabled': false,
    'duplicateEnabled': false,
    'requiredFields': ['archiveFile'],
    'choicesByValue': {
      'archiveFile': {'dependsOn': 'code', 'values': archiveChoices},
    },
    'choiceLabels': {'archiveFile': archiveChoiceLabels},
    'columns': [
      {'key': 'enabled', 'labelKey': 'rfi.signed.column.enabled', 'kind': 'bool'},
      {'key': 'code', 'labelKey': 'rfi.signed.column.code', 'kind': 'readonly'},
      {
        'key': 'confidence',
        'labelKey': 'ocr.confidence',
        'kind': 'readonly',
        'display': 'percent',
        'width': 112,
      },
      {
        'key': 'signedFile',
        'labelKey': 'rfi.signed.column.scan',
        'kind': 'readonly',
      },
      {
        'key': 'archiveFile',
        'labelKey': 'rfi.signed.column.archive',
        'kind': 'choice',
        'bulk': false,
      },
    ],
    'rows': editorRows,
  };
  emit(
    ProgressEvent(
      phase: 'preview_ready',
      done: pdfs.length,
      total: pdfs.length,
      subject: '',
    ),
  );
  return plan;
}

/// Port of `revise`. Only the `enabled` flag and the offered archive choice of
/// each existing row may change. Paths are never recalculated.
Plan revise(Plan plan, JsonObject payload) {
  if (plan.opId != stageSignedOperationId) {
    throw BackendError.validation('signed-page plan identity is invalid');
  }
  final editor = plan.meta['editor'];
  final existingRows = editor is Map ? editor['rows'] : null;
  if (existingRows is! List<Object?>) {
    throw BackendError.validation('signed-page review rows are missing');
  }
  final suppliedRows = payload['rows'];
  if (suppliedRows is! List<Object?>) {
    throw BackendError.validation('signed-page review rows are invalid');
  }
  if (existingRows.length != suppliedRows.length) {
    throw BackendError.validation(
      'signed-page review rows cannot be added or removed',
    );
  }
  final existingIds = _rowIds(existingRows);
  final suppliedIds = _rowIds(suppliedRows);
  if (existingIds.length != existingRows.length ||
      !_setEquals(suppliedIds, existingIds)) {
    throw BackendError.validation('signed-page review row identity is invalid');
  }
  final cachedCandidates = plan.meta['archiveCandidateSteps'];
  if (cachedCandidates is! Map) {
    throw BackendError.validation(
      'signed-page archive candidates are missing',
    );
  }
  final steps = <Step>[
    for (final step in plan.steps)
      if (step.kind == StepKind.cleanupTemp ||
          step.meta['warningReview'] == true)
        step,
  ];
  final meta = _deepCopy(plan.meta) as Map<String, Object?>;
  final revisedRows = <Map<String, Object?>>[];
  for (final rowObject in existingRows) {
    final row = Map<String, Object?>.from(rowObject! as Map);
    final rowId = (row['_rowId'] as String?) ?? '';
    Map<String, Object?>? supplied;
    for (final candidate in suppliedRows) {
      if (candidate is Map && candidate['_rowId'] == rowId) {
        supplied = Map<String, Object?>.from(candidate);
        break;
      }
    }
    if (supplied == null) {
      throw BackendError.validation('signed-page review row identity is invalid');
    }
    final enabled = supplied['enabled'];
    if (enabled is! bool) {
      throw BackendError.validation('signed-page row state is invalid');
    }
    final selected = supplied['archiveFile'];
    if (selected is! String) {
      throw BackendError.validation('signed-page archive selection is invalid');
    }
    final choices = cachedCandidates[rowId];
    if (choices is! Map) {
      throw BackendError.validation('signed-page archive choices are invalid');
    }
    if (selected.isNotEmpty && !choices.containsKey(selected)) {
      throw BackendError.validation(
        'signed-page archive selection was not offered by preview',
      );
    }
    if (enabled) {
      if (selected.isEmpty) {
        throw BackendError.validation('signed-page archive selection is required');
      }
      final List<Step> selectedSteps;
      try {
        selectedSteps = [
          for (final item in choices[selected]! as List<Object?>)
            Step.fromJson(Map<String, Object?>.from(item! as Map)),
        ];
      } on Object {
        throw BackendError.validation('signed-page candidate plan is invalid');
      }
      if (selectedSteps.any(
        (step) =>
            !(step.kind == StepKind.mkdir ||
                step.kind == StepKind.pdfFirstPageReplace) ||
            (step.kind == StepKind.pdfFirstPageReplace &&
                step.meta['rowId'] != rowId),
      )) {
        throw BackendError.validation(
          'signed-page candidate plan contains an invalid step',
        );
      }
      _extendCandidateSteps(steps, selectedSteps);
    }
    row['enabled'] = enabled;
    row['archiveFile'] = selected;
    revisedRows.add(row);
  }
  (meta['editor']! as Map<String, Object?>)['rows'] = revisedRows;
  return Plan(
    planId: '',
    opId: plan.opId,
    steps: steps,
    warnings: List<Warning>.of(plan.warnings),
    meta: meta,
  );
}

Set<String> _rowIds(List<Object?> rows) => {
  for (final row in rows)
    if (row is Map && row['_rowId'] is String) row['_rowId']! as String,
};

bool _setEquals(Set<String> left, Set<String> right) =>
    left.length == right.length && left.containsAll(right);

Object? _deepCopy(Object? value) => switch (value) {
  Map() => {
    for (final entry in value.entries)
      entry.key as String: _deepCopy(entry.value),
  },
  List() => [for (final item in value) _deepCopy(item)],
  _ => value,
};

/// Port of `prepare_archive_candidate`. Returns null when the candidate is not
/// staged; the reason is recorded as a plan warning, as in Rust.
_PreparedArchive? _prepareArchiveCandidate(
  String code,
  String rowId,
  _Candidate candidate,
  String archivePdf,
  PdfInspection signedFacts,
  String signedHash,
  String signedRoot,
  String archiveRoot,
  String reviewRoot,
  Plan plan,
) {
  final relative = _relativeTo(archivePdf, archiveRoot);
  if (relative == null) {
    throw BackendError.pathInvalid('archive match escaped the selected root');
  }
  final destination = p.join(reviewRoot, relative);
  if (_exists(destination)) {
    final staleTemps = _staleHardlinkTemps(destination, reviewRoot);
    final destinationHash = staleTemps.isEmpty
        ? null
        : _tryFileSha256(destination);
    if (destinationHash != null) {
      for (final staleTemp in staleTemps) {
        plan.steps.add(
          Step(
            kind: StepKind.cleanupTemp,
            src: staleTemp,
            dst: destination,
            noteKey: 'note.rfi.cleanupStaleTemp',
            meta: {
              'reviewRoot': reviewRoot,
              'destinationSha256': destinationHash,
            },
          ),
        );
      }
      plan.warnings.add(_warning('warn.rfi.staleTempFound', relative));
    } else {
      plan.warnings.add(_warning('warn.rfi.reviewExists', relative));
    }
    return null;
  }
  if (_ancestorsUntil(destination, reviewRoot).any(_isSymlink)) {
    plan.warnings.add(_warning('warn.rfi.reviewPathBlocked', relative));
    return null;
  }
  final inspected = _inspectOutcome(archivePdf);
  final archiveInspection = inspected.facts;
  final PdfInspection archiveFacts;
  if (archiveInspection != null &&
      _fidelitySupported(archiveInspection, _PdfRole.archive)) {
    archiveFacts = archiveInspection;
  } else if (archiveInspection != null) {
    plan.warnings.add(
      _warning(
        'warn.rfi.pdfFidelityUnsupported',
        _truncateChars(
          '$code — ${_fidelityReason(archiveInspection, _PdfRole.archive)}',
          300,
        ),
      ),
    );
    return null;
  } else {
    plan.warnings.add(
      _warning(
        'warn.rfi.pdfFidelityUnsupported',
        _truncateChars('$code — ${inspected.error}', 300),
      ),
    );
    return null;
  }
  if (archiveFacts.pageCount == 0) {
    plan.warnings.add(
      _warning('warn.rfi.pdfFidelityUnsupported', '$code — PDF has no pages'),
    );
    return null;
  }
  if (archiveFacts.pages[0].widgets > 0) {
    plan.warnings.add(
      _warning(
        'warn.rfi.pdfFidelityUnsupported',
        '$code — page-one form widgets require manual review',
      ),
    );
    return null;
  }
  final String archiveHash;
  try {
    archiveHash = fileSha256(archivePdf);
  } on Object {
    throw BackendError.pathInvalid('archive PDF could not be hashed');
  }
  final steps = <Step>[];
  _addStagingDirectorySteps(
    p.dirname(destination),
    reviewRoot,
    <String>{},
    steps,
  );
  steps.add(
    Step(
      kind: StepKind.pdfFirstPageReplace,
      src: archivePdf,
      dst: destination,
      noteKey: 'note.rfi.signedFirstPageStaged',
      meta: {
        'code': code,
        'signedPdf': candidate.path,
        'signedRoot': signedRoot,
        'archiveRoot': archiveRoot,
        'reviewRoot': reviewRoot,
        'relativePath': relative,
        'confidence': candidate.confidence,
        'method': candidate.method,
        'archiveSha256': archiveHash,
        'signedSha256': signedHash,
        'archivePageCount': archiveFacts.pageCount,
        'signedPageCount': signedFacts.pageCount,
        'signedPageIndex': candidate.pageIndex,
        'rowId': rowId,
      },
    ),
  );
  return _PreparedArchive(
    archive: archivePdf,
    relative: relative,
    steps: steps,
  );
}

String? _acceptPagePreview(String value, _PreviewBudget budget) {
  final next = budget.used + value.length;
  if (next > _maxPagePreviewBytes) {
    return null;
  }
  budget.used = next;
  return value;
}

void _extendCandidateSteps(List<Step> target, List<Step> candidate) {
  for (final step in candidate) {
    if (step.kind == StepKind.mkdir &&
        target.any(
          (existing) =>
              existing.kind == StepKind.mkdir && existing.dst == step.dst,
        )) {
      continue;
    }
    target.add(step);
  }
}

/// Port of `push_warning_review`: records the reason and appends the
/// user-facing warning with its page and code context.
void _pushWarningReview(
  Plan plan,
  SplayTreeMap<String, List<_ReviewReason>> reviews,
  String signedRoot,
  String key,
  String source,
  int? pageIndex,
  String? code,
  String detail,
) {
  _recordWarningReview(reviews, source, pageIndex, code, key, detail);
  final context = <String>[_relativeOrSelf(source, signedRoot)];
  if (pageIndex != null) {
    context.add('page ${pageIndex + 1} (#${pageIndex + 1})');
  }
  if (code != null) {
    context.add(code);
  }
  if (detail.isNotEmpty) {
    context.add(detail);
  }
  plan.warnings.add(_warning(key, _truncateChars(context.join(' · '), 500)));
}

void _recordWarningReview(
  SplayTreeMap<String, List<_ReviewReason>> reviews,
  String source,
  int? pageIndex,
  String? code,
  String key,
  String detail,
) {
  final reason = _ReviewReason(
    key: key,
    pageNumber: pageIndex == null ? null : pageIndex + 1,
    code: code,
    detail: _truncateChars(detail, 300),
  );
  final reasons = reviews.putIfAbsent(source, () => []);
  if (!reasons.contains(reason)) {
    reasons.add(reason);
  }
}

/// Port of `add_warning_review_steps`.
void _addWarningReviewSteps(
  Plan plan,
  SplayTreeMap<String, List<_ReviewReason>> reviews,
  String signedRoot,
  String reviewRoot,
) {
  final warningRoot = p.join(reviewRoot, '_review');
  final items = <Object?>[];
  final plannedDirectories = <String>{};
  for (final entry in reviews.entries) {
    final source = entry.key;
    final reasons = entry.value;
    final relative = _relativeTo(source, signedRoot);
    if (relative == null) {
      throw BackendError.pathInvalid(
        'warning review source escaped signed scans',
      );
    }
    final destination = p.join(warningRoot, relative);
    final reasonValues = [for (final reason in reasons) reason.toJson()];
    var status = 'planned';
    if (_exists(destination)) {
      status = 'exists';
      plan.warnings.add(_warning('warn.rfi.warningReviewExists', relative));
    } else if (_reviewDestinationBlocked(destination, reviewRoot)) {
      status = 'blocked';
      plan.warnings.add(_warning('warn.rfi.warningReviewBlocked', relative));
    } else {
      final reviewSteps = <Step>[];
      _addStagingDirectorySteps(
        p.dirname(destination),
        reviewRoot,
        plannedDirectories,
        reviewSteps,
      );
      _extendCandidateSteps(plan.steps, [
        for (final step in reviewSteps)
          Step(
            kind: step.kind,
            src: step.src,
            dst: step.dst,
            noteKey: step.noteKey,
            meta: {...step.meta, 'warningReview': true},
          ),
      ]);
      final String sourceHash;
      try {
        sourceHash = fileSha256(source);
      } on Object {
        throw BackendError.pathInvalid(
          'warning review PDF could not be hashed',
        );
      }
      plan.steps.add(
        Step(
          kind: StepKind.copy,
          src: source,
          dst: destination,
          noteKey: 'note.rfi.signedWarningReviewCopy',
          meta: {
            'warningReview': true,
            'expectedSourceSha256': sourceHash,
            'reasons': reasonValues,
          },
        ),
      );
    }
    items.add({
      'sourceFile': relative,
      'destination': _relativeOrSelf(destination, reviewRoot),
      'status': status,
      'reasons': reasonValues,
    });
  }
  plan.meta['warningReviewItems'] = items;
}

bool _reviewDestinationBlocked(String destination, String reviewRoot) {
  for (final path in _ancestorsUntil(destination, reviewRoot).skip(1)) {
    final type = FileSystemEntity.typeSync(path, followLinks: false);
    if (type == FileSystemEntityType.notFound) {
      continue;
    }
    if (type != FileSystemEntityType.directory) {
      return true;
    }
  }
  return false;
}

Plan _emptyPlan() => Plan(
  planId: '',
  opId: stageSignedOperationId,
  steps: [],
  warnings: [],
  meta: _emptyMeta(),
);

JsonObject _emptyMeta() => {
  'archiveReadOnly': true,
  'preservesArchiveLayout': true,
  'usesOnlySignedFirstPage': true,
};

String _canonicalDirectory(String path, String label) {
  final String resolved;
  try {
    resolved = Directory(path).resolveSymbolicLinksSync();
  } on Object {
    throw BackendError.pathInvalid('$label does not exist');
  }
  if (!FileSystemEntity.isDirectorySync(resolved)) {
    throw BackendError.pathInvalid('$label is not a directory');
  }
  return resolved;
}

/// Component-wise `starts_with` in either direction.
bool _overlaps(String first, String second) =>
    _startsWith(first, second) || _startsWith(second, first);

/// Component-wise `Path::starts_with`: [path] equals [root] or is inside it.
bool _startsWith(String path, String root) =>
    path == root || p.isWithin(root, path);

bool _exists(String path) =>
    FileSystemEntity.typeSync(path) != FileSystemEntityType.notFound;

bool _isSymlink(String path) =>
    FileSystemEntity.typeSync(path, followLinks: false) ==
    FileSystemEntityType.link;

/// Port of `Path::ancestors`: the path itself, then each parent up to the
/// filesystem root.
List<String> _ancestors(String path) {
  final result = <String>[];
  var current = path;
  while (true) {
    result.add(current);
    final parent = p.dirname(current);
    if (parent == current) {
      break;
    }
    current = parent;
  }
  return result;
}

/// `ancestors().take_while(|path| *path != root)`.
Iterable<String> _ancestorsUntil(String path, String root) =>
    _ancestors(path).takeWhile((ancestor) => ancestor != root);

/// `strip_prefix`, or null when [path] is not strictly inside [root].
String? _relativeTo(String path, String root) {
  if (path == root) {
    return '';
  }
  if (!p.isWithin(root, path)) {
    return null;
  }
  return p.relative(path, from: root);
}

String _relativeOrSelf(String path, String root) =>
    _relativeTo(path, root) ?? path;

/// Port of `stale_hardlink_temps`.
List<String> _staleHardlinkTemps(String destination, String reviewRoot) {
  if (FileSystemEntity.typeSync(destination, followLinks: false) !=
      FileSystemEntityType.file) {
    return const [];
  }
  final String canonical;
  try {
    canonical = File(destination).resolveSymbolicLinksSync();
  } on Object {
    return const [];
  }
  if (!_startsWith(canonical, reviewRoot)) {
    return const [];
  }
  final parent = p.dirname(destination);
  final destinationName = p.basename(destination);
  final List<FileSystemEntity> entries;
  try {
    entries = Directory(parent).listSync(followLinks: false);
  } on Object {
    return const [];
  }
  final stale = <String>[];
  for (final entry in entries) {
    final name = p.basename(entry.path);
    if (!temporaryPdfNameMatches(name, destinationName)) {
      continue;
    }
    if (FileSystemEntity.typeSync(entry.path, followLinks: false) !=
        FileSystemEntityType.file) {
      continue;
    }
    if (!sameFileIdentity(destination, entry.path)) {
      continue;
    }
    final String resolved;
    try {
      resolved = File(entry.path).resolveSymbolicLinksSync();
    } on Object {
      continue;
    }
    if (_startsWith(resolved, reviewRoot)) {
      stale.add(entry.path);
    }
  }
  stale.sort(_comparePaths);
  return stale;
}

String? _tryFileSha256(String path) {
  try {
    return fileSha256(path);
  } on Object {
    return null;
  }
}

/// Port of `add_directory_steps` for staging: plans one `Mkdir` per missing
/// component below [reviewRoot], refusing any blocked component.
void _addStagingDirectorySteps(
  String parent,
  String reviewRoot,
  Set<String> planned,
  List<Step> steps,
) {
  final List<String> components;
  if (parent == reviewRoot) {
    components = const [];
  } else {
    final relative = _relativeTo(parent, reviewRoot);
    if (relative == null) {
      throw BackendError.pathEscape();
    }
    components = p.split(relative);
  }
  var current = reviewRoot;
  for (final component in components) {
    current = p.join(current, component);
    if (planned.contains(current)) {
      continue;
    }
    final type = FileSystemEntity.typeSync(current, followLinks: false);
    if (type == FileSystemEntityType.directory) {
      continue;
    }
    if (type != FileSystemEntityType.notFound) {
      throw BackendError.pathInvalid('review path is blocked');
    }
    steps.add(
      Step(
        kind: StepKind.mkdir,
        src: null,
        dst: current,
        noteKey: 'note.folder_created',
        meta: {'reviewRoot': reviewRoot},
      ),
    );
    planned.add(current);
  }
}

bool _fidelitySupported(PdfInspection facts, _PdfRole role) =>
    !facts.encrypted &&
    !facts.signed &&
    !facts.catalogEntryHashes.containsKey('StructTreeRoot') &&
    (role != _PdfRole.signedScan ||
        !facts.catalogEntryHashes.containsKey('OCProperties'));

String _fidelityReason(PdfInspection facts, _PdfRole role) {
  if (facts.encrypted) {
    return 'encrypted PDF';
  }
  if (facts.signed) {
    return 'digitally signed PDF';
  }
  if (facts.catalogEntryHashes.containsKey('StructTreeRoot')) {
    return 'tagged PDF page structure cannot be safely preserved';
  }
  if (role == _PdfRole.signedScan &&
      facts.catalogEntryHashes.containsKey('OCProperties')) {
    return 'signed page depends on document-level optional-content state';
  }
  return 'unsupported PDF fidelity case';
}

/// Port of `canonical_stage_code`: only strict and approved OCR-alias matches
/// may stage; repaired or fuzzy identities stay review-only.
String? _canonicalStageCode(RfiMatch matched) {
  if (matched.method != RfiMethod.strict &&
      matched.method != RfiMethod.ocrAlias) {
    return null;
  }
  return normalizeFullCode(matched.code);
}

/// Port of `canonical_archive_filename_code`.
String? _canonicalArchiveFilenameCode(String filename) {
  final afterPrefix = _stripPrefixCi(filename, 'RFI No. ');
  if (afterPrefix == null) {
    return null;
  }
  final body = _stripSuffixCi(afterPrefix, '.pdf');
  if (body == null) {
    return null;
  }
  final rfi = _stripPrefixCi(body, 'AZE-SUN-INT-QMS-RFI-');
  if (rfi == null) {
    return null;
  }
  final dash = rfi.indexOf('-');
  if (dash < 0) {
    return null;
  }
  final discipline = rfi.substring(0, dash);
  final number = rfi.substring(dash + 1);
  final upper = _asciiUpper(discipline);
  if (!_disciplines.contains(upper)) {
    return null;
  }
  final normalized = normalizeCandidateNumber(number);
  if (normalized == null) {
    return null;
  }
  return '$prefix-$upper-$normalized';
}

String? _stripPrefixCi(String value, String prefixText) {
  if (value.length < prefixText.length) {
    return null;
  }
  if (_asciiLower(value.substring(0, prefixText.length)) !=
      _asciiLower(prefixText)) {
    return null;
  }
  return value.substring(prefixText.length);
}

String? _stripSuffixCi(String value, String suffix) {
  final start = value.length - suffix.length;
  if (start < 0) {
    return null;
  }
  if (_asciiLower(value.substring(start)) != _asciiLower(suffix)) {
    return null;
  }
  return value.substring(0, start);
}

/// Port of `archive_index`: every exact archive PDF for the wanted codes, at
/// any depth below [root]. Symbolic links are never followed.
_ArchiveIndexResult _archiveIndex(
  String root,
  Set<String> codes,
  void Function(ProgressEvent event) emit,
  bool Function() isCancelled,
) {
  final wanted = {for (final code in codes) code: code};
  final matches = SplayTreeMap<String, List<String>>();
  for (final code in codes) {
    matches[code] = <String>[];
  }
  if (wanted.isEmpty) {
    return _ArchiveIndexResult(
      matches: matches,
      workerCount: 0,
      directoriesScanned: 0,
      entriesScanned: 0,
    );
  }
  var directories = <String>[root];
  var entriesScanned = 0;
  var directoriesScanned = 0;
  var workers = 1;
  while (directories.isNotEmpty) {
    if (isCancelled()) {
      throw BackendError.cancelled();
    }
    directories.sort(_comparePaths);
    workers = math.max(workers, workerCount(directories.length));
    directoriesScanned += directories.length;
    final next = <String>[];
    for (final directory in directories) {
      final batch = _scanArchiveDirectory(root, directory, wanted);
      entriesScanned += batch.entriesScanned;
      if (entriesScanned > _maxArchiveEntries) {
        throw BackendError.validation(
          'archive exceeds the $_maxArchiveEntries-entry limit',
        );
      }
      next.addAll(batch.directories);
      for (final (code, path) in batch.matches) {
        matches.putIfAbsent(code, () => []).add(path);
      }
    }
    next.sort(_comparePaths);
    emit(
      ProgressEvent(
        phase: 'archive_scan',
        done: directoriesScanned,
        total: directoriesScanned + next.length,
        subject: root,
      ),
    );
    directories = next;
  }
  for (final values in matches.values) {
    values.sort(_comparePaths);
  }
  return _ArchiveIndexResult(
    matches: matches,
    workerCount: workers,
    directoriesScanned: directoriesScanned,
    entriesScanned: entriesScanned,
  );
}

_ArchiveDirectoryBatch _scanArchiveDirectory(
  String root,
  String directory,
  Map<String, String> wanted,
) {
  final batch = _ArchiveDirectoryBatch();
  final List<FileSystemEntity> entries;
  try {
    entries = Directory(directory).listSync(followLinks: false);
  } on Object {
    throw BackendError.pathInvalid('archive directory could not be read');
  }
  for (final entry in entries) {
    batch.entriesScanned++;
    if (batch.entriesScanned > _maxArchiveEntries) {
      throw BackendError.validation(
        'archive exceeds the $_maxArchiveEntries-entry limit',
      );
    }
    if (entry is Directory) {
      batch.directories.add(entry.path);
      continue;
    }
    if (entry is! File) {
      continue;
    }
    final code = _canonicalArchiveFilenameCode(p.basename(entry.path));
    final canonical = code == null ? null : wanted[code];
    if (canonical == null) {
      continue;
    }
    final String resolved;
    try {
      resolved = File(entry.path).resolveSymbolicLinksSync();
    } on Object {
      throw BackendError.pathInvalid('archive PDF could not be resolved');
    }
    if (!_startsWith(resolved, root) || !FileSystemEntity.isFileSync(resolved)) {
      throw BackendError.pathEscape();
    }
    batch.matches.add((canonical, resolved));
  }
  batch.directories.sort(_comparePaths);
  batch.matches.sort((left, right) {
    final byCode = left.$1.compareTo(right.$1);
    return byCode != 0 ? byCode : _comparePaths(left.$2, right.$2);
  });
  return batch;
}

/// Port of `signed_pdfs`: sorted, de-duplicated by path, bounded.
(List<String>, int) _signedPdfs(
  String root,
  bool recursive,
  void Function(ProgressEvent event) emit,
) {
  final files = <String>[];
  var directories = <String>[root];
  var scanned = 0;
  var workers = 1;
  while (directories.isNotEmpty) {
    directories.sort(_comparePaths);
    final currentCount = directories.length;
    workers = math.max(workers, workerCount(directories.length));
    final next = <String>[];
    for (final directory in directories) {
      final batch = _scanSignedDirectory(directory, recursive);
      files.addAll(batch.files);
      next.addAll(batch.directories);
    }
    if (files.length > _maxSignedPdfs) {
      throw BackendError.validation(
        'signed scans exceed the $_maxSignedPdfs-PDF limit',
      );
    }
    scanned += currentCount;
    emit(
      ProgressEvent(
        phase: 'signed_discovery',
        done: scanned,
        total: scanned + next.length,
        subject: '${files.length} PDF',
      ),
    );
    directories = next;
  }
  files.sort((left, right) => left.compareTo(right));
  return (files, workers);
}

_SignedDirectoryBatch _scanSignedDirectory(String directory, bool recursive) {
  final List<FileSystemEntity> entries;
  try {
    entries = Directory(directory).listSync(followLinks: false);
  } on Object {
    throw BackendError.pathInvalid('signed scans could not be read');
  }
  final batch = _SignedDirectoryBatch();
  for (final entry in entries) {
    if (entry is Link) {
      continue;
    }
    final name = p.basename(entry.path);
    final hidden = name.startsWith('.');
    if (entry is File &&
        !hidden &&
        p.extension(entry.path).toLowerCase() == '.pdf') {
      batch.files.add(entry.path);
    } else if (recursive && entry is Directory) {
      batch.directories.add(entry.path);
    }
  }
  return batch;
}

_InspectionResult _inspectOutcome(String path) {
  try {
    return _InspectionResult.success(inspect(path));
  } on Object catch (error) {
    return _InspectionResult.failure(error.toString());
  }
}

Future<_ReadResult> _readPageResult(
  OcrAdapter ocr,
  String path,
  int pageIndex,
) async {
  try {
    return _ReadResult.success(await ocr.readPage(path, pageIndex));
  } on OcrFailure catch (failure) {
    return _ReadResult.failure(failure);
  }
}

Future<String?> _thumbnailOrNull(String path, int pageIndex) async {
  try {
    return await renderPageThumbnailDataUrl(path, pageIndex);
  } on Object {
    return null;
  }
}

/// Bounded async pool with the semantics of `bounded_map_stream`.
Future<(int, List<R>)> _boundedAsync<T, R>(
  List<T> items,
  Future<R> Function(T item) job,
  void Function(int index, R result) completed,
) async {
  final workers = workerCount(items.length);
  final results = List<R?>.filled(items.length, null);
  var next = 0;
  Future<void> worker() async {
    while (next < items.length) {
      final index = next++;
      final result = await job(items[index]);
      results[index] = result;
      completed(index, result);
    }
  }

  if (items.isNotEmpty) {
    await Future.wait([
      for (var i = 0; i < math.min(workers, items.length); i++) worker(),
    ]);
  }
  return (workers, [for (final result in results) result as R]);
}

/// Component-wise ordering, as `Ord` for `PathBuf` compares components.
int _comparePaths(String left, String right) {
  final a = p.split(left);
  final b = p.split(right);
  for (var index = 0; index < math.min(a.length, b.length); index++) {
    final order = a[index].compareTo(b[index]);
    if (order != 0) {
      return order;
    }
  }
  return a.length.compareTo(b.length);
}

String _asciiUpper(String value) => String.fromCharCodes(
  value.codeUnits.map((unit) => unit >= 0x61 && unit <= 0x7A ? unit - 32 : unit),
);

String _asciiLower(String value) => String.fromCharCodes(
  value.codeUnits.map((unit) => unit >= 0x41 && unit <= 0x5A ? unit + 32 : unit),
);

String _displayName(String path) => p.basename(path);

String _candidateDisplay(String path, int pageIndex) =>
    '${_displayName(path)} — #${pageIndex + 1}';

/// Port of `truncate`: the first [maxChars] characters, with no ellipsis.
String _truncateChars(String value, int maxChars) =>
    String.fromCharCodes(value.runes.take(maxChars));

Warning _warning(String key, String subject) =>
    Warning(key: key, subject: subject);
