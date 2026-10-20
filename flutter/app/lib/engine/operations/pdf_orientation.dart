/// Port of src-tauri/src/operations/pdf_orientation.rs.
///
/// Preview-only operation: it detects page orientation per PDF, proposes
/// absolute clockwise rotations for confident pages, and routes every
/// uncertain PDF to manual review. Nothing is rotated in place. Apply (a
/// review-root copy with `PdfPageRotate`) is executed by the filesystem
/// authority, which is not ported yet.
///
/// Differences from the Rust module, all deliberate:
/// - Page detection is `Future`-based (Tesseract runs as a child process), so
///   the Rust `bounded_map_stream` pool becomes [_boundedAsync]: the same worker
///   count, bounded concurrency, and completion-order callbacks, on this
///   isolate's event loop.
/// - The orientation types and [orientationTargetRotation] are declared in
///   `process_scans.dart` until that module is migrated. They are re-exported
///   here so callers see the same surface as the Rust `pdf_orientation` module.
library;

import 'dart:async';
import 'dart:collection';
import 'dart:io';
import 'dart:math' as math;

import 'package:path/path.dart' as p;

import '../contracts.dart';
import '../errors.dart';
import '../fs_ops.dart' show fileSha256;
import '../parallel.dart' show workerCount;
import '../pdf.dart' show PdfInspection, inspect;
import '../scan_index.dart';
import 'process_scans.dart'
    show
        OrientationDetection,
        OrientationDetector,
        OrientationError,
        minOrientationConfidence,
        orientationTargetRotation;

export 'process_scans.dart'
    show
        OrientationDetection,
        OrientationDetector,
        OrientationError,
        minOrientationConfidence,
        orientationTargetRotation;

/// Port of `pdf_orientation::OPERATION_ID`.
const String orientationOperationId = 'rfi.normalize_pdf_orientation';

const List<String> _disciplines = ['CIV', 'MEC', 'ELE', 'PRC'];
const int _maxDirectoryEntries = 500000;
const int _maxPdfs = 50000;
const int _maxPages = 250000;
const String _orientationScanProfile = 'content-orientation-cascade-v5';

/// Port of `PdfOrientationParams`.
class PdfOrientationParams {
  const PdfOrientationParams({
    required this.rfiSystemRoot,
    required this.reviewRoot,
  });

  final String rfiSystemRoot;
  final String reviewRoot;
}

/// Port of `PageTask`.
class _PageTask {
  const _PageTask(this.path, this.pageIndex, this.currentRotation);

  final String path;
  final int pageIndex;
  final int currentRotation;
}

/// Port of `PdfWork`.
class _PdfWork {
  _PdfWork({
    required this.path,
    required this.relative,
    required this.pageCount,
    required this.sourceHash,
    required this.cached,
  });

  final String path;
  final String relative;
  final int pageCount;
  String? sourceHash;
  final bool cached;
}

/// Port of `Result<OrientationDetection, String>`.
final class _Outcome {
  const _Outcome.success(OrientationDetection this.detection) : error = null;
  const _Outcome.failure(String this.error) : detection = null;

  final OrientationDetection? detection;
  final String? error;

  bool get isOk => error == null;
}

/// Port of `CachedPageOrientation` (`deny_unknown_fields`).
class _CachedPage {
  const _CachedPage({
    required this.currentRotation,
    required this.detection,
    required this.error,
  });

  final int currentRotation;
  final OrientationDetection? detection;
  final String? error;
}

/// Port of `CachedPdfOrientation` (`deny_unknown_fields`).
class _CachedPdf {
  const _CachedPdf({
    required this.pageCount,
    required this.sourceSha256,
    required this.pages,
    required this.unsupported,
  });

  final int pageCount;
  final String? sourceSha256;
  final List<_CachedPage> pages;
  final String? unsupported;
}

/// Mutable counters that the Rust code keeps on `ScanIndexStats`.
class _Counters {
  int reused = 0;
  int analyzed = 0;
  int failed = 0;
}

/// Port of `preview` and `preview_indexed`. Without a [scanIndex] the run is
/// not persisted, matching `preview`.
Future<Plan> previewOrientation(
  PdfOrientationParams params,
  OrientationDetector detector, {
  void Function(ProgressEvent event)? progress,
  bool Function()? cancelled,
  ScanIndex? scanIndex,
  String ownerId = 'local',
  bool forceRescan = false,
}) async {
  void emit(ProgressEvent event) => progress?.call(event);
  bool isCancelled() => cancelled?.call() ?? false;

  final sourceRoot = _canonicalDirectory(
    params.rfiSystemRoot,
    'RFI system root',
  );
  final reviewRoot = _canonicalDirectory(
    params.reviewRoot,
    'orientation review root',
  );
  if (_overlaps(sourceRoot, reviewRoot)) {
    return Plan(
      planId: '',
      opId: orientationOperationId,
      steps: [],
      warnings: [_warning('error.rfi.orientationReviewRootMustBeSeparate', '')],
      meta: _emptyMeta(),
    );
  }

  emit(
    ProgressEvent(
      phase: 'orientation_discovery',
      done: 0,
      total: 0,
      subject: sourceRoot,
    ),
  );
  final discovery = _collectDisciplinePdfs(sourceRoot, emit, isCancelled);
  final pdfs = discovery.$1;
  final discoveryWorkers = discovery.$2;
  emit(
    ProgressEvent(
      phase: 'orientation_discovery',
      done: pdfs.length,
      total: pdfs.length,
      subject: '',
    ),
  );
  final plan = _emptyPlan();
  if (pdfs.isEmpty) {
    plan.warnings.add(_warning('warn.rfi.orientationNoPdfs', sourceRoot));
    return plan;
  }

  final counters = _Counters();
  ScanRun? run;
  var priorInterruptedRuns = 0;
  if (scanIndex != null) {
    try {
      final (started, prior) = scanIndex.beginRun(
        ownerId,
        orientationOperationId,
        _orientationScanProfile,
        sourceRoot,
        forceRescan: forceRescan,
      );
      run = started;
      priorInterruptedRuns = prior;
    } on Object catch (error) {
      throw BackendError.requestFailed('scan index unavailable: $error');
    }
  }

  try {
    final work = <_PdfWork>[];
    final tasks = <_PageTask>[];
    final initialFingerprints = <String, FileFingerprint>{};
    final initialHashes = <String, String>{};
    final byFile = SplayTreeMap<String, List<(_PageTask, _Outcome)>>(
      _comparePaths,
    );
    final totalPdfs = pdfs.length;
    var inspectedPdfs = 0;
    emit(
      ProgressEvent(
        phase: 'orientation_inspection',
        done: 0,
        total: totalPdfs,
        subject: '',
      ),
    );
    final inspectionWorkers = math.min(workerCount(pdfs.length), 2);

    // Keep at most two large parsed PDFs in flight, as the Rust batch does.
    for (var start = 0; start < pdfs.length; start += 2) {
      if (isCancelled()) {
        throw BackendError.cancelled();
      }
      final batch = pdfs.sublist(start, math.min(start + 2, pdfs.length));
      for (final path in batch) {
        if (isCancelled()) {
          throw BackendError.cancelled();
        }
        final relative = _relativeTo(path, sourceRoot);
        if (relative == null) {
          throw BackendError.pathInvalid('PDF escaped the selected RFI root');
        }
        final FileFingerprint fingerprint;
        try {
          fingerprint = FileFingerprint.read(path);
        } on Object catch (error) {
          throw BackendError.pathInvalid(
            'orientation fingerprint failed: $error',
          );
        }
        final String sourceSha256;
        try {
          sourceSha256 = fileSha256(path);
        } on Object catch (error) {
          throw BackendError.pathInvalid(
            'orientation source hash failed: $error',
          );
        }
        _CachedPdf? cached;
        if (!forceRescan && run != null) {
          try {
            cached = run.lookup<_CachedPdf>(
              relative,
              fingerprint,
              _cachedPdfFromJson,
            );
          } on Object catch (error) {
            throw BackendError.requestFailed('scan index read failed: $error');
          }
        }
        if (cached != null && cached.sourceSha256 != sourceSha256) {
          cached = null;
        }
        PdfInspection? facts;
        String? inspectionError;
        if (cached == null) {
          try {
            facts = inspect(path);
          } on Object catch (error) {
            inspectionError = error.toString();
          }
        }

        inspectedPdfs++;
        emit(
          ProgressEvent(
            phase: 'orientation_inspection',
            done: inspectedPdfs,
            total: totalPdfs,
            subject: relative,
          ),
        );
        if (cached != null) {
          counters.reused++;
          final unsupported = cached.unsupported;
          if (unsupported != null) {
            plan.warnings.add(
              _warning(
                'warn.rfi.orientationUnsupported',
                '$relative — $unsupported',
              ),
            );
            continue;
          }
          if (cached.pageCount == 0 ||
              cached.pages.length != cached.pageCount) {
            throw BackendError.requestFailed(
              'cached orientation result is structurally invalid',
            );
          }
          final cachedResults = <(_PageTask, _Outcome)>[];
          for (var pageIndex = 0; pageIndex < cached.pages.length; pageIndex++) {
            final page = cached.pages[pageIndex];
            final _Outcome result;
            if (page.detection != null && page.error == null) {
              result = _Outcome.success(page.detection!);
            } else if (page.detection == null && page.error != null) {
              result = _Outcome.failure(page.error!);
            } else {
              throw BackendError.requestFailed(
                'cached orientation page is structurally invalid',
              );
            }
            var currentRotation = page.currentRotation;
            var finalResult = result;
            try {
              currentRotation = _normalizeRotation(page.currentRotation);
            } on BackendError catch (error) {
              finalResult = _Outcome.failure(error.message);
            }
            cachedResults.add((
              _PageTask(path, pageIndex, currentRotation),
              finalResult,
            ));
          }
          byFile[path] = cachedResults;
          work.add(
            _PdfWork(
              path: path,
              relative: relative,
              pageCount: cached.pageCount,
              sourceHash: sourceSha256,
              cached: true,
            ),
          );
          continue;
        }

        if (inspectionError != null) {
          _checkpointFailure(run, relative, inspectionError);
          counters.failed++;
          plan.warnings.add(
            _warning(
              'warn.rfi.orientationUnsupported',
              '$relative — $inspectionError',
            ),
          );
          continue;
        }
        final inspected = facts!;
        if (inspected.encrypted) {
          _recordUnsupported(
            run,
            relative,
            fingerprint,
            sourceSha256,
            'encrypted PDF',
            counters,
            plan,
          );
          continue;
        }
        if (inspected.signed) {
          _recordUnsupported(
            run,
            relative,
            fingerprint,
            sourceSha256,
            'digitally signed PDF',
            counters,
            plan,
          );
          continue;
        }
        if (inspected.pageCount == 0) {
          _recordUnsupported(
            run,
            relative,
            fingerprint,
            sourceSha256,
            'PDF has no pages',
            counters,
            plan,
          );
          continue;
        }
        final List<int> currentRotations;
        try {
          currentRotations = [
            for (final page in inspected.pages)
              _normalizeRotation(page.rotation),
          ];
        } on BackendError catch (error) {
          _recordUnsupported(
            run,
            relative,
            fingerprint,
            sourceSha256,
            error.message,
            counters,
            plan,
          );
          continue;
        }
        if (tasks.length + inspected.pageCount > _maxPages) {
          throw BackendError.validation(
            'orientation scan exceeds the $_maxPages-page limit',
          );
        }
        for (var pageIndex = 0; pageIndex < currentRotations.length; pageIndex++) {
          tasks.add(
            _PageTask(path, pageIndex, currentRotations[pageIndex]),
          );
        }
        initialHashes[path] = sourceSha256;
        initialFingerprints[path] = fingerprint;
        work.add(
          _PdfWork(
            path: path,
            relative: relative,
            pageCount: inspected.pageCount,
            sourceHash: null,
            cached: false,
          ),
        );
      }
    }

    final cachedPages = byFile.values.fold<int>(
      0,
      (sum, results) => sum + results.length,
    );
    final totalPages = cachedPages + tasks.length;
    var flatIndex = 0;
    for (final results in byFile.values) {
      for (final (task, _) in results) {
        final done = flatIndex + 1;
        if (done == 1 || done % 128 == 0 || done == cachedPages) {
          emit(
            ProgressEvent(
              phase: 'orientation_analysis',
              done: done,
              total: totalPages,
              subject: '${task.path} · ${task.pageIndex + 1}',
            ),
          );
        }
        flatIndex++;
      }
    }
    var completedPages = cachedPages;
    final taskRotations = <(String, int), int>{
      for (final task in tasks)
        (task.path, task.pageIndex): task.currentRotation,
    };
    final remaining = <String, int>{
      for (final item in work)
        if (!item.cached) item.path: item.pageCount,
    };
    final checkpointPages = <String, List<_Outcome?>>{
      for (final item in work)
        if (!item.cached)
          item.path: List<_Outcome?>.filled(item.pageCount, null),
    };
    final sourceHashes = <String, String?>{};

    final (detectionWorkers, results) = await _boundedAsync<_PageTask, _Outcome>(
      tasks,
      (task) => _detectPage(detector, task),
      (index, result) {
        completedPages++;
        final task = tasks[index];
        emit(
          ProgressEvent(
            phase: 'orientation_analysis',
            done: completedPages,
            total: totalPages,
            subject: '${task.path} · ${task.pageIndex + 1}',
          ),
        );
        final pages = checkpointPages[task.path];
        if (pages == null) {
          throw BackendError.requestFailed(
            'orientation checkpoint has no file bucket',
          );
        }
        pages[task.pageIndex] = result;
        final left = remaining[task.path];
        if (left == null) {
          throw BackendError.requestFailed(
            'orientation checkpoint has no remaining count',
          );
        }
        remaining[task.path] = math.max(0, left - 1);
        if (remaining[task.path] != 0) {
          return;
        }

        final initial = initialFingerprints[task.path];
        if (initial == null) {
          throw BackendError.requestFailed(
            'orientation checkpoint has no initial fingerprint',
          );
        }
        final relative = _relativeTo(task.path, sourceRoot);
        final FileFingerprint current;
        try {
          current = FileFingerprint.read(task.path);
        } on Object catch (error) {
          throw BackendError.requestFailed(error.toString());
        }
        if (current != initial) {
          _checkpointFailure(
            run,
            relative,
            'source changed during orientation scan',
          );
          counters.failed++;
          throw BackendError.requestFailed(
            'source PDF changed during orientation scan',
          );
        }
        final initialHash = initialHashes[task.path];
        if (initialHash == null) {
          throw BackendError.requestFailed(
            'orientation checkpoint has no initial content hash',
          );
        }
        final String currentHash;
        try {
          currentHash = fileSha256(task.path);
        } on Object catch (error) {
          throw BackendError.requestFailed(
            'source PDF could not be hashed: $error',
          );
        }
        if (currentHash != initialHash) {
          _checkpointFailure(
            run,
            relative,
            'source content changed during orientation scan',
          );
          counters.failed++;
          throw BackendError.requestFailed(
            'source PDF content changed during orientation scan',
          );
        }

        final pageRecords = <_CachedPage>[];
        for (var pageIndex = 0; pageIndex < pages.length; pageIndex++) {
          final outcome = pages[pageIndex];
          if (outcome == null) {
            throw BackendError.requestFailed(
              'orientation checkpoint page is incomplete',
            );
          }
          final currentRotation = taskRotations[(task.path, pageIndex)];
          if (currentRotation == null) {
            throw BackendError.requestFailed(
              'orientation checkpoint page metadata is missing',
            );
          }
          pageRecords.add(
            outcome.isOk
                ? _CachedPage(
                    currentRotation: currentRotation,
                    detection: outcome.detection,
                    error: null,
                  )
                : _CachedPage(
                    currentRotation: currentRotation,
                    detection: null,
                    error: outcome.error,
                  ),
          );
        }
        final hasDetectionError = pageRecords.any((page) => page.error != null);
        final actionable =
            !hasDetectionError &&
            pageRecords.every(
              (page) =>
                  page.detection != null &&
                  _target(page.currentRotation, page.detection!).error == null,
            ) &&
            pageRecords.any(
              (page) =>
                  page.detection != null &&
                  _target(page.currentRotation, page.detection!).target != null,
            );
        if (run != null && relative != null) {
          if (hasDetectionError) {
            _checkpointFailure(run, relative, 'one or more page detectors failed');
          } else {
            try {
              run.recordSuccess(
                relative,
                initial,
                _cachedPdfToJson(
                  _CachedPdf(
                    pageCount: pages.length,
                    sourceSha256: initialHash,
                    pages: pageRecords,
                    unsupported: null,
                  ),
                ),
              );
            } on Object catch (error) {
              throw BackendError.requestFailed(error.toString());
            }
          }
        }
        sourceHashes[task.path] = actionable ? initialHash : null;
        if (hasDetectionError) {
          counters.failed++;
        } else {
          counters.analyzed++;
        }
      },
    );

    for (var index = 0; index < tasks.length; index++) {
      byFile
          .putIfAbsent(tasks[index].path, () => [])
          .add((tasks[index], results[index]));
    }
    for (final item in work) {
      if (!item.cached) {
        item.sourceHash = sourceHashes[item.path];
      }
    }

    final createdDirectories = <String>{};
    var verifiedPages = 0;
    var rotatedPages = 0;
    var actionableFiles = 0;
    var manualReviewFiles = 0;
    for (final item in work) {
      if (item.cached) {
        final expected = item.sourceHash;
        if (expected == null) {
          throw BackendError.requestFailed(
            'cached orientation result has no source hash',
          );
        }
        final String current;
        try {
          current = fileSha256(item.path);
        } on Object catch (error) {
          throw BackendError.pathInvalid(error.toString());
        }
        if (current != expected) {
          throw BackendError.pathInvalid(
            'source PDF content changed during orientation scan',
          );
        }
      }
      final pageResults = byFile.remove(item.path) ?? const [];
      final approved = <Map<String, Object?>>[];
      final uncertain = <String>[];
      for (final (task, result) in pageResults) {
        if (!result.isOk) {
          uncertain.add('page ${task.pageIndex + 1} ${result.error}');
          continue;
        }
        final detection = result.detection!;
        final target = _target(task.currentRotation, detection);
        if (target.error != null) {
          uncertain.add('page ${task.pageIndex + 1} ${target.error}');
          continue;
        }
        verifiedPages++;
        if (target.target != null) {
          approved.add({
            'pageIndex': task.pageIndex,
            'fromRotation': task.currentRotation,
            'targetRotation': target.target,
            'rotateClockwise': detection.rotateClockwise,
            'confidence': detection.confidence,
            'method': detection.method,
          });
        }
      }
      if (uncertain.isNotEmpty) {
        manualReviewFiles++;
        plan.warnings.add(
          _warning(
            'warn.rfi.orientationNeedsReview',
            _truncate(
              '${item.relative} — ${uncertain.join(', ')}',
              360,
            ),
          ),
        );
        continue;
      }
      if (approved.isEmpty) {
        continue;
      }
      final destination = p.join(reviewRoot, item.relative);
      if (_exists(destination)) {
        plan.warnings.add(
          _warning('warn.rfi.orientationReviewExists', item.relative),
        );
        continue;
      }
      _addDirectorySteps(
        p.dirname(destination),
        reviewRoot,
        createdDirectories,
        plan.steps,
      );
      final sourceHash = item.sourceHash;
      if (sourceHash == null) {
        throw BackendError.requestFailed(
          'orientation scan did not pin a source hash',
        );
      }
      rotatedPages += approved.length;
      actionableFiles++;
      plan.steps.add(
        Step(
          kind: StepKind.pdfPageRotate,
          src: item.path,
          dst: destination,
          noteKey: 'note.rfi.pdfOrientationNormalized',
          meta: {
            'sourceSha256': sourceHash,
            'sourceRoot': sourceRoot,
            'reviewRoot': reviewRoot,
            'relativePath': item.relative,
            'pageCount': item.pageCount,
            'rotations': approved,
          },
        ),
      );
    }

    final statsRecord = ScanIndexStats(
      discovered: pdfs.length,
      reused: counters.reused,
      analyzed: counters.analyzed,
      failed: counters.failed,
      priorInterruptedRuns: priorInterruptedRuns,
      forceRescan: forceRescan,
      parallelAnalysis: tasks.isNotEmpty,
      analysisWorkers: detectionWorkers,
    );
    plan.meta.addAll({
      'workerCount': math.max(
        math.max(detectionWorkers, discoveryWorkers),
        inspectionWorkers,
      ),
      'inspectionWorkers': inspectionWorkers,
      'verifiedPages': verifiedPages,
      'rotatedPages': rotatedPages,
      'actionableFiles': actionableFiles,
      'manualReviewFiles': manualReviewFiles,
      'detectorDiagnostics': detector.orientationDiagnostics(),
      'scanIndex': statsRecord.toJson(),
    });
    if (run != null) {
      try {
        run.complete(statsRecord);
      } on Object catch (error) {
        throw BackendError.requestFailed(
          'scan run completion failed: $error',
        );
      }
    }
    emit(
      const ProgressEvent(
        phase: 'preview_ready',
        done: 1,
        total: 1,
        subject: '',
      ),
    );
    return plan;
  } finally {
    run?.abandon();
  }
}

/// Port of `record_unsupported`.
void _recordUnsupported(
  ScanRun? run,
  String relative,
  FileFingerprint fingerprint,
  String sourceSha256,
  String reason,
  _Counters counters,
  Plan plan,
) {
  if (run != null) {
    try {
      run.recordSuccess(
        relative,
        fingerprint,
        _cachedPdfToJson(
          _CachedPdf(
            pageCount: 0,
            sourceSha256: sourceSha256,
            pages: const [],
            unsupported: reason,
          ),
        ),
      );
    } on Object catch (error) {
      throw BackendError.requestFailed('scan checkpoint failed: $error');
    }
  }
  counters.analyzed++;
  plan.warnings.add(
    _warning('warn.rfi.orientationUnsupported', '$relative — $reason'),
  );
}

/// Records a per-file failure in the durable index when one is open. A failed
/// checkpoint write is reported as a request failure, as in Rust.
void _checkpointFailure(ScanRun? run, String? relative, String message) {
  if (run == null || relative == null) {
    return;
  }
  try {
    run.recordFailure(relative, message);
  } on Object catch (error) {
    throw BackendError.requestFailed('scan checkpoint failed: $error');
  }
}

/// Port of `orientation_target_rotation` as a total function: `target` is null
/// for an upright page and `error` carries the Rust `Err(String)`.
({int? target, String? error}) _target(
  int currentRotation,
  OrientationDetection detection,
) {
  try {
    return (
      target: orientationTargetRotation(currentRotation, detection),
      error: null,
    );
  } on OrientationError catch (error) {
    return (target: null, error: error.message);
  }
}

/// Port of `detect_page` in the bounded pool: detector failures become a
/// per-page `Err(String)`, never a whole-run failure.
Future<_Outcome> _detectPage(
  OrientationDetector detector,
  _PageTask task,
) async {
  try {
    return _Outcome.success(
      await detector.detectPage(task.path, task.pageIndex),
    );
  } on OrientationError catch (error) {
    return _Outcome.failure(error.message);
  } on Object catch (error) {
    return _Outcome.failure(error.toString());
  }
}

/// Bounded async pool with the semantics of `bounded_map_stream`: at most
/// [workerCount] jobs in flight, results in input order, and [completed]
/// invoked once per item in completion order. A throwing [completed] or job
/// stops only its own worker; the first error is rethrown after the pool
/// drains.
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

/// Port of `collect_discipline_pdfs`. Only PDFs under a discipline folder
/// (CIV, MEC, ELE, PRC) are collected; symbolic links are never followed.
(List<String>, int) _collectDisciplinePdfs(
  String root,
  void Function(ProgressEvent) emit,
  bool Function() isCancelled,
) {
  var directories = <(String, bool)>[
    (root, _isDiscipline(p.basename(root))),
  ];
  final pdfs = SplayTreeSet<String>(_comparePaths);
  var visited = 0;
  var scannedDirectories = 0;
  var workers = 1;
  while (directories.isNotEmpty) {
    if (isCancelled()) {
      throw BackendError.cancelled();
    }
    directories.sort((left, right) {
      final byPath = _comparePaths(left.$1, right.$1);
      if (byPath != 0) {
        return byPath;
      }
      return (left.$2 ? 1 : 0).compareTo(right.$2 ? 1 : 0);
    });
    final currentCount = directories.length;
    workers = math.max(workers, workerCount(directories.length));
    final next = <(String, bool)>[];
    for (final (directory, inDiscipline) in directories) {
      final batch = _scanOrientationDirectory(directory, inDiscipline);
      visited += batch.entries;
      if (visited > _maxDirectoryEntries) {
        throw BackendError.validation(
          'RFI system is too large to scan safely',
        );
      }
      pdfs.addAll(batch.pdfs);
      next.addAll(batch.directories);
    }
    if (pdfs.length > _maxPdfs) {
      throw BackendError.validation(
        'orientation scan exceeds the $_maxPdfs-PDF limit',
      );
    }
    scannedDirectories += currentCount;
    emit(
      ProgressEvent(
        phase: 'orientation_discovery',
        done: scannedDirectories,
        total: scannedDirectories + next.length,
        subject: '${pdfs.length} PDF',
      ),
    );
    directories = next;
  }
  return (pdfs.toList(), workers);
}

class _OrientationDirectoryBatch {
  final List<(String, bool)> directories = [];
  final List<String> pdfs = [];
  int entries = 0;
}

/// Port of `scan_orientation_directory`.
_OrientationDirectoryBatch _scanOrientationDirectory(
  String directory,
  bool inDiscipline,
) {
  final batch = _OrientationDirectoryBatch();
  final List<FileSystemEntity> entries;
  try {
    entries = Directory(directory).listSync(followLinks: false);
  } on Object {
    throw BackendError.pathInvalid('RFI system folder could not be read');
  }
  for (final entry in entries) {
    batch.entries++;
    if (entry is Link) {
      continue;
    }
    if (entry is Directory) {
      final childIsDiscipline =
          inDiscipline || _isDiscipline(p.basename(entry.path));
      batch.directories.add((entry.path, childIsDiscipline));
    } else if (entry is File &&
        inDiscipline &&
        p.extension(entry.path).toLowerCase() == '.pdf') {
      batch.pdfs.add(entry.path);
    }
  }
  return batch;
}

/// Port of `add_directory_steps` for review copies: recursive, stops at the
/// first existing ancestor, and plans one `Mkdir` per missing directory.
void _addDirectorySteps(
  String directory,
  String root,
  Set<String> created,
  List<Step> steps,
) {
  if (directory == root || _exists(directory)) {
    return;
  }
  final parent = p.dirname(directory);
  if (parent == directory) {
    throw BackendError.pathInvalid('review directory has no parent');
  }
  _addDirectorySteps(parent, root, created, steps);
  if (created.add(directory)) {
    steps.add(
      Step(
        kind: StepKind.mkdir,
        src: null,
        dst: directory,
        noteKey: 'note.create_folder',
        meta: {},
      ),
    );
  }
}

Plan _emptyPlan() => Plan(
  planId: '',
  opId: orientationOperationId,
  steps: [],
  warnings: [],
  meta: _emptyMeta(),
);

JsonObject _emptyMeta() => {
  'sourceReadOnly': true,
  'preservesDirectoryLayout': true,
  'confidenceThreshold': minOrientationConfidence,
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
bool _overlaps(String left, String right) =>
    left == right || p.isWithin(right, left) || p.isWithin(left, right);

/// Port of `normalize_rotation`.
int _normalizeRotation(int rotation) {
  final normalized = rotation % 360;
  if (const {0, 90, 180, 270}.contains(normalized)) {
    return normalized;
  }
  throw BackendError.validation('PDF page rotation is not a right angle');
}

bool _exists(String path) =>
    FileSystemEntity.typeSync(path) != FileSystemEntityType.notFound;

bool _isDiscipline(String name) => _disciplines.contains(_asciiUpper(name));

/// `strip_prefix` for a path under [root]; null when [path] is not inside it.
String? _relativeTo(String path, String root) {
  if (!p.isWithin(root, path)) {
    return null;
  }
  return p.relative(path, from: root);
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

/// Port of Rust `truncate`: at most [maxChars] characters, then an ellipsis
/// when anything was cut.
String _truncate(String value, int maxChars) {
  final runes = value.runes.toList();
  final taken = String.fromCharCodes(runes.take(maxChars));
  return runes.length > maxChars ? '$taken…' : taken;
}

Warning _warning(String key, String subject) =>
    Warning(key: key, subject: subject);

Map<String, Object?> _detectionToJson(OrientationDetection detection) => {
  'rotateClockwise': detection.rotateClockwise,
  'confidence': detection.confidence,
  'method': detection.method,
};

OrientationDetection _detectionFromJson(Object? value) {
  final map = _strictObject(value, const {
    'rotateClockwise',
    'confidence',
    'method',
  }, 'OrientationDetection');
  return OrientationDetection(
    rotateClockwise: (_required(map, 'rotateClockwise') as num).toInt(),
    confidence: (_required(map, 'confidence') as num).toDouble(),
    method: _required(map, 'method') as String,
  );
}

Map<String, Object?> _cachedPdfToJson(_CachedPdf cached) => {
  'pageCount': cached.pageCount,
  'sourceSha256': cached.sourceSha256,
  'pages': [
    for (final page in cached.pages)
      {
        'currentRotation': page.currentRotation,
        'detection': page.detection == null
            ? null
            : _detectionToJson(page.detection!),
        'error': page.error,
      },
  ],
  'unsupported': cached.unsupported,
};

_CachedPdf _cachedPdfFromJson(Object? value) {
  final map = _strictObject(value, const {
    'pageCount',
    'sourceSha256',
    'pages',
    'unsupported',
  }, 'CachedPdfOrientation');
  final pages = _required(map, 'pages') as List<Object?>;
  return _CachedPdf(
    pageCount: (_required(map, 'pageCount') as num).toInt(),
    sourceSha256: map['sourceSha256'] as String?,
    pages: [
      for (final page in pages)
        _cachedPageFromJson(page),
    ],
    unsupported: map['unsupported'] as String?,
  );
}

_CachedPage _cachedPageFromJson(Object? value) {
  final map = _strictObject(value, const {
    'currentRotation',
    'detection',
    'error',
  }, 'CachedPageOrientation');
  final detection = map['detection'];
  return _CachedPage(
    currentRotation: (_required(map, 'currentRotation') as num).toInt(),
    detection: detection == null ? null : _detectionFromJson(detection),
    error: map['error'] as String?,
  );
}

Map<String, Object?> _strictObject(
  Object? value,
  Set<String> allowed,
  String label,
) {
  if (value is! Map) {
    throw FormatException('$label must be an object');
  }
  final map = Map<String, Object?>.from(value);
  for (final key in map.keys) {
    if (!allowed.contains(key)) {
      throw FormatException('unknown field `$key` in $label');
    }
  }
  return map;
}

Object? _required(Map<String, Object?> map, String key) {
  if (!map.containsKey(key) || map[key] == null) {
    throw FormatException('missing field `$key`');
  }
  return map[key];
}
