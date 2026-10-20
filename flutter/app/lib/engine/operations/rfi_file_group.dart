/// Port of src-tauri/src/operations/rfi_file_group.rs.
///
/// Groups RFI documents from a flat source folder into `<destination>/<DISC>/<CODE>/`
/// folders. Preview only: the source is never mutated, every destination is
/// checked for existing files and unsafe links, and the offered steps are
/// filtered by the user's row selection in [reviseRfiFileGroup].
///
/// Dart-specific notes (see the report for the full list):
/// - Per-file content inspection that does not need OCR runs in parallel
///   isolates through [boundedMapStream]. PDF content inspection needs the
///   OCR adapter, which is not sendable between isolates, so it runs on the
///   calling isolate after the parallel phase, in source order.
/// - The OCR adapter and PDF page counter are injected. Their production
///   implementations (process_scans.rs / pdf.rs) are not ported yet; see
///   [RfiOcrAdapter] and [RfiPdfPageCounter].
library;

import 'dart:collection';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:archive/archive.dart';
import 'package:path/path.dart' as p;
import 'package:xml/xml.dart' show XmlAttributeType, XmlEntityMapping;
import 'package:xml/xml_events.dart';

import '../contracts.dart';
import '../errors.dart';
import '../parallel.dart';
import '../paths.dart' show canonicalizePath;
import '../rfi_code.dart' as rfi_code;

const String rfiGroupFilesOperationId = 'rfi.group_files';

const String _reviewDir = '_review';
const int _maxFiles = 10000;
const int _maxRawBytes = 64 * 1024 * 1024;
const int _maxPackageBytes = 256 * 1024 * 1024;
const int _maxPackageEntries = 4096;
const int _maxPackageTextBytes = 64 * 1024 * 1024;
const int _maxPdfPages = 2000;

/// Port of `GroupRfiFilesParams`.
class GroupRfiFilesParams {
  const GroupRfiFilesParams({required this.source, required this.destination});

  final String source;
  final String destination;

  @override
  bool operator ==(Object other) =>
      other is GroupRfiFilesParams &&
      other.source == source &&
      other.destination == destination;

  @override
  int get hashCode => Object.hash(source, destination);
}

/// Read result of one OCR page. Port of `OcrRead`; file grouping only uses
/// [matched].
///
/// UNVERIFIED: minimal mirror of `operations/process_scans.rs::OcrRead` until
/// that module is ported to lib/engine/operations/process_scans.dart.
class RfiOcrRead {
  const RfiOcrRead({required this.matched, required this.source});

  final rfi_code.RfiMatch matched;
  final String source;
}

/// Port of `OcrFailure`, thrown by [RfiOcrAdapter] reads.
sealed class RfiOcrFailure implements Exception {
  const RfiOcrFailure();
}

/// Port of `OcrFailure::EngineMissing`.
final class RfiOcrEngineMissing extends RfiOcrFailure {
  const RfiOcrEngineMissing();
}

/// Port of `OcrFailure::Document`.
final class RfiOcrDocumentFailure extends RfiOcrFailure {
  const RfiOcrDocumentFailure(this.message);

  final String message;
}

/// Minimal port of the `OcrAdapter` trait surface used by file grouping.
///
/// UNVERIFIED: declared here because `process_scans.rs` has no Dart port yet.
/// The production adapter must be supplied by the engine composition root.
abstract class RfiOcrAdapter {
  const RfiOcrAdapter();

  /// Port of `OcrAdapter::read`. Throws [RfiOcrFailure].
  Future<RfiOcrRead> read(String path);

  /// Port of `OcrAdapter::read_page` (default body included): page 0 defers to
  /// [read]; other pages are unavailable unless an adapter overrides this.
  Future<RfiOcrRead> readPage(String path, int pageIndex) async {
    if (pageIndex == 0) {
      return read(path);
    }
    throw const RfiOcrDocumentFailure('page-level OCR is unavailable');
  }

  /// Port of `OcrAdapter::diagnostics`.
  String diagnostics();
}

/// Port of `pdf::inspect(path)?.page_count`. Returns the page count or throws
/// an error whose `toString()` is the Rust error message.
///
/// UNVERIFIED: stands in for `pdf.rs::inspect` until that module is ported.
typedef RfiPdfPageCounter = int Function(String path);

/// Port of `GroupRfiFilesParams`-driven preview output shape:
/// `(rows, offeredSteps)` are placed into [Plan.meta] exactly as Rust does.
class _Detection {
  const _Detection(this.code, this.methodKey, this.confidence);

  final String code;
  final String methodKey;
  final double confidence;
}

class _FileAnalysis {
  const _FileAnalysis({
    required this.path,
    required this.detections,
    required this.issue,
    required this.isMail,
  });

  final String path;
  final List<_Detection> detections;
  final String? issue;
  final bool isMail;
}

/// Result of the parallel phase. Sendable across isolates.
class _PartialAnalysis {
  const _PartialAnalysis({
    required this.path,
    required this.detections,
    required this.issue,
    required this.isMail,
    required this.needsPdfContent,
  });

  final String path;
  final List<_Detection> detections;
  final String? issue;
  final bool isMail;
  final bool needsPdfContent;
}

/// Internal failure carrying the Rust `Err(String)` message of an inspection.
class _InspectionFailure implements Exception {
  const _InspectionFailure(this.message);

  final String message;
}

/// Port of `preview`.
Future<Plan> previewRfiFileGroup({
  required GroupRfiFilesParams params,
  required RfiOcrAdapter ocr,
  required RfiPdfPageCounter pdfPageCount,
  required void Function(ProgressEvent event) progress,
  required bool Function() cancelled,
}) async {
  final source = _canonicalDirectory(params.source, 'RFI file source');
  final destination = _canonicalDirectory(
    params.destination,
    'RFI file destination',
  );
  if (_overlaps(source, destination)) {
    throw BackendError.pathInvalid(
      'RFI file source and destination must be separate',
    );
  }
  progress(
    ProgressEvent(
      phase: 'file_group_discovery',
      done: 0,
      total: 0,
      subject: source,
    ),
  );
  final files = _discoverFiles(source);
  files.sort(
    (left, right) => _compareUtf8(p.basename(left), p.basename(right)),
  );
  progress(
    ProgressEvent(
      phase: 'file_group_discovery',
      done: files.length,
      total: files.length,
      subject: '',
    ),
  );
  if (cancelled()) {
    throw BackendError.cancelled();
  }

  final total = files.length;
  var completed = 0;
  final (
    workerCount,
    partials,
  ) = await boundedMapStream<String, _PartialAnalysis>(
    files,
    _analyzeWithoutPdf,
    (index, partial) {
      // PDF content is counted when it is inspected on the calling isolate.
      if (partial.needsPdfContent) {
        return;
      }
      completed++;
      progress(
        ProgressEvent(
          phase: 'file_group_analysis',
          done: completed,
          total: total,
          subject: files[index],
        ),
      );
    },
  );

  final analyses = <_FileAnalysis>[];
  for (final partial in partials) {
    if (partial.needsPdfContent) {
      completed++;
      progress(
        ProgressEvent(
          phase: 'file_group_analysis',
          done: completed,
          total: total,
          subject: partial.path,
        ),
      );
      analyses.add(await _finishPdfAnalysis(partial, ocr, pdfPageCount));
    } else {
      analyses.add(
        _FileAnalysis(
          path: partial.path,
          detections: partial.detections,
          issue: partial.issue,
          isMail: partial.isMail,
        ),
      );
    }
  }
  if (cancelled()) {
    throw BackendError.cancelled();
  }

  progress(
    ProgressEvent(
      phase: 'file_group_planning',
      done: 0,
      total: total,
      subject: '',
    ),
  );
  final warnings = <Warning>[];
  final planSteps = <Step>[];
  final offeredSteps = <Step>[];
  final rows = <Map<String, Object?>>[];
  final plannedDirectories = <String>{};
  var reviewDirectoryPlanned = false;

  final mailCodes = <String>{
    for (final analysis in analyses)
      if (analysis.isMail)
        for (final detection in analysis.detections) detection.code,
  };
  final filesWithoutMail = SplayTreeMap<String, SplayTreeSet<String>>();
  for (final analysis in analyses) {
    final fileName = p.basename(analysis.path);
    for (final detection in analysis.detections) {
      if (!mailCodes.contains(detection.code)) {
        filesWithoutMail
            .putIfAbsent(
              detection.code,
              () => SplayTreeSet<String>(_compareUtf8),
            )
            .add(fileName);
      }
    }
  }
  for (final entry in filesWithoutMail.entries) {
    warnings.add(
      Warning(
        key: 'warn.rfi.fileGroup.missingEmail',
        subject: '${entry.key} — ${_summarizeFileNames(entry.value)}',
      ),
    );
  }

  for (var fileIndex = 0; fileIndex < analyses.length; fileIndex++) {
    final analysis = analyses[fileIndex];
    progress(
      ProgressEvent(
        phase: 'file_group_planning',
        done: fileIndex + 1,
        total: total,
        subject: analysis.path,
      ),
    );
    final fileName = p.basename(analysis.path);
    if (analysis.detections.isEmpty) {
      final warningKey = analysis.issue != null
          ? 'warn.rfi.fileGroup.unreadable'
          : 'warn.rfi.fileGroup.noCode';
      warnings.add(
        Warning(
          key: warningKey,
          subject: analysis.issue == null
              ? fileName
              : '$fileName — ${analysis.issue}',
        ),
      );
      final reviewDirectory = p.join(destination, _reviewDir);
      final reviewTarget = p.join(reviewDirectory, fileName);
      if (_unsafeExistingDirectory(reviewDirectory)) {
        warnings.add(
          Warning(key: 'warn.rfi.fileGroup.exists', subject: reviewDirectory),
        );
        continue;
      }
      if (_pathExists(reviewTarget)) {
        warnings.add(
          Warning(key: 'warn.rfi.fileGroup.exists', subject: reviewTarget),
        );
        continue;
      }
      if (!_isDirectory(reviewDirectory) && !reviewDirectoryPlanned) {
        reviewDirectoryPlanned = true;
        planSteps.add(
          _directoryStep(reviewDirectory, null, warningReview: true),
        );
      }
      planSteps.add(
        _copyStep(analysis.path, reviewTarget, null, null, warningReview: true),
      );
      continue;
    }
    if (analysis.issue != null) {
      warnings.add(
        Warning(
          key: 'warn.rfi.fileGroup.partialInspection',
          subject: '$fileName — ${analysis.issue}',
        ),
      );
    }
    for (
      var detectionIndex = 0;
      detectionIndex < analysis.detections.length;
      detectionIndex++
    ) {
      final detection = analysis.detections[detectionIndex];
      if (!mailCodes.contains(detection.code)) {
        continue;
      }
      final discipline = detection.code.split('-')[5];
      final directory = p.join(destination, discipline, detection.code);
      final target = p.join(directory, fileName);
      if (_unsafeExistingDirectory(directory)) {
        warnings.add(
          Warning(key: 'warn.rfi.fileGroup.exists', subject: directory),
        );
        continue;
      }
      if (_pathExists(target)) {
        warnings.add(
          Warning(key: 'warn.rfi.fileGroup.exists', subject: target),
        );
        continue;
      }
      if (!_isDirectory(directory) && plannedDirectories.add(detection.code)) {
        offeredSteps.add(
          _directoryStep(directory, detection.code, warningReview: false),
        );
      }
      final rowId = 'file-group-$fileIndex-$detectionIndex';
      offeredSteps.add(
        _copyStep(
          analysis.path,
          target,
          rowId,
          detection.code,
          warningReview: false,
        ),
      );
      rows.add({
        '_rowId': rowId,
        'enabled': true,
        'rfiCode': detection.code,
        'sourceFile': fileName,
        'method': detection.methodKey,
        'confidence': detection.confidence,
      });
    }
  }
  planSteps.addAll(offeredSteps);
  if (files.isEmpty) {
    warnings.add(Warning(key: 'warn.rfi.fileGroup.noFiles', subject: source));
  }

  final meta = <String, Object?>{
    'offeredSteps': [for (final step in offeredSteps) step.toJson()],
    'parallelAnalysis': files.isNotEmpty,
    'workerCount': workerCount,
    'sourceReadOnly': true,
    'missingEmailRfiCount': filesWithoutMail.length,
    'editor': {
      'kind': 'recordTable',
      'layout': 'compact',
      'titleKey': 'rfi.fileGroup.editor.title',
      'rowCountKey': 'rfi.fileGroup.editor.rowCount',
      'groupField': 'rfiCode',
      'selectionGroupFields': ['rfiCode', 'sourceFile'],
      'selectionEnabled': true,
      'bulkEnabled': true,
      'duplicateEnabled': false,
      'reorderable': false,
      'detailsEnabled': false,
      'copyEnabled': false,
      'requiredFields': ['enabled'],
      'columns': [
        {
          'key': 'enabled',
          'labelKey': 'rfi.fileGroup.column.enabled',
          'kind': 'bool',
          'width': 64,
        },
        {
          'key': 'rfiCode',
          'labelKey': 'rfi.fileGroup.column.rfiCode',
          'kind': 'readonly',
          'width': 260,
          'bulk': false,
        },
        {
          'key': 'sourceFile',
          'labelKey': 'rfi.fileGroup.column.sourceFile',
          'kind': 'readonly',
          'width': 300,
          'bulk': false,
        },
        {
          'key': 'method',
          'labelKey': 'rfi.fileGroup.column.method',
          'kind': 'readonly',
          'display': 'localizedKey',
          'width': 120,
          'bulk': false,
        },
        {
          'key': 'confidence',
          'labelKey': 'ocr.confidence',
          'kind': 'readonly',
          'display': 'percent',
          'width': 112,
          'bulk': false,
        },
      ],
      'rows': rows,
    },
  };
  progress(
    ProgressEvent(
      phase: 'file_group_planning',
      done: total,
      total: total,
      subject: '',
    ),
  );
  progress(
    const ProgressEvent(phase: 'preview_ready', done: 1, total: 1, subject: ''),
  );
  return Plan(
    planId: '',
    opId: rfiGroupFilesOperationId,
    steps: planSteps,
    warnings: warnings,
    meta: meta,
  );
}

/// Port of `revise`: keeps the review steps and only the offered steps whose
/// row (copy) or code (folder) the user left enabled.
Plan reviseRfiFileGroup(Plan plan, JsonObject payload) {
  if (plan.opId != rfiGroupFilesOperationId) {
    throw BackendError.validation('RFI file-group plan identity is invalid');
  }
  final editor = plan.meta['editor'];
  final originalRows = editor is Map ? editor['rows'] : null;
  if (originalRows is! List) {
    throw BackendError.validation('RFI file-group rows are missing');
  }
  final submittedRows = payload['rows'];
  if (submittedRows is! List) {
    throw BackendError.validation('RFI file-group rows are invalid');
  }
  if (originalRows.length != submittedRows.length) {
    throw BackendError.validation(
      'RFI file-group rows cannot be added or removed',
    );
  }
  final originals = <String, Map<String, Object?>>{};
  for (final row in originalRows) {
    final rowId = row is Map ? row['_rowId'] : null;
    if (rowId is! String) {
      throw BackendError.validation('RFI file-group row identity is invalid');
    }
    originals[rowId] = Map<String, Object?>.from(row! as Map);
  }
  final enabled = <String>{};
  final revisedRows = <Object?>[];
  final seen = <String>{};
  for (final submitted in submittedRows) {
    final rowId = submitted is Map ? submitted['_rowId'] : null;
    if (rowId is! String) {
      throw BackendError.validation('RFI file-group row identity is invalid');
    }
    if (!seen.add(rowId)) {
      throw BackendError.validation(
        'RFI file-group row identity is duplicated',
      );
    }
    final original = originals[rowId];
    if (original == null) {
      throw BackendError.validation('RFI file-group row is not approved');
    }
    final requested = (submitted as Map)['enabled'];
    if (requested is! bool) {
      throw BackendError.validation('RFI file-group row state is invalid');
    }
    final expected = _deepCopy(original) as Map<String, Object?>;
    expected['enabled'] = requested;
    if (!_jsonDeepEquals(submitted, expected)) {
      throw BackendError.validation(
        'RFI file-group row fields cannot be changed',
      );
    }
    if (requested) {
      enabled.add(rowId);
    }
    revisedRows.add(expected);
  }
  final List<Step> offered;
  final offeredRaw = plan.meta['offeredSteps'];
  if (!plan.meta.containsKey('offeredSteps')) {
    throw BackendError.validation('RFI file-group steps are missing');
  }
  if (offeredRaw is! List) {
    throw BackendError.validation('RFI file-group steps are invalid');
  }
  try {
    offered = [
      for (final item in offeredRaw) Step.fromJson(item! as JsonObject),
    ];
  } on Object {
    throw BackendError.validation('RFI file-group steps are invalid');
  }
  final selectedCodes = <String>{
    for (final row in originals.entries)
      if (enabled.contains(row.key) && row.value['rfiCode'] is String)
        row.value['rfiCode']! as String,
  };
  final steps = <Step>[
    for (final step in plan.steps)
      if (step.meta['warningReview'] == true) step,
  ];
  for (final step in offered) {
    final bool allowed;
    switch (step.kind) {
      case StepKind.mkdir:
        final code = step.meta['code'];
        if (code is! String) {
          throw BackendError.validation(
            'RFI file-group folder step is invalid',
          );
        }
        allowed = selectedCodes.contains(code);
      case StepKind.copy:
        final rowId = step.meta['rowId'];
        if (rowId is! String) {
          throw BackendError.validation('RFI file-group copy step is invalid');
        }
        allowed = enabled.contains(rowId);
      default:
        throw BackendError.validation('RFI file-group offered step is invalid');
    }
    if (allowed) {
      steps.add(step);
    }
  }
  final meta = _deepCopy(plan.meta) as Map<String, Object?>;
  (meta['editor']! as Map<String, Object?>)['rows'] = revisedRows;
  return Plan(
    planId: '',
    opId: rfiGroupFilesOperationId,
    steps: steps,
    warnings: plan.warnings,
    meta: meta,
  );
}

/// Port of the crate-private `exact_codes`: shared RFI codes plus the
/// exact-wrapped and multi-number suffix forms.
SplayTreeSet<String> exactCodes(String value) {
  final text = rfi_code.canonical(value);
  final codes = rfi_code.exactCodes(text);
  for (final captures in _rfiPattern.allMatches(text)) {
    final start = captures.start;
    final end = captures.end;
    final beforeInvalid = _isAlnumString(_codePointBefore(text, start));
    final afterInvalid = _isAlnumString(_codePointAt(text, end));
    final number = rfi_code.normalizeCandidateNumber(captures[2]!);
    if (number == null) {
      continue;
    }
    final discipline = captures[1]!;
    final canonical = 'AZE-SUN-INT-QMS-RFI-$discipline-$number';
    if (beforeInvalid || afterInvalid || !codes.contains(canonical)) {
      continue;
    }
    codes.add(canonical);
    var cursor = end;
    while (true) {
      cursor = _skipAsciiWhitespace(text, cursor);
      if (cursor >= text.length || !_suffixSeparators.contains(text[cursor])) {
        break;
      }
      cursor++;
      cursor = _skipAsciiWhitespace(text, cursor);
      final start2 = cursor;
      while (cursor < text.length && _isAsciiDigit(text.codeUnitAt(cursor))) {
        cursor++;
      }
      final rawNumber = text.substring(start2, cursor);
      final suffix = rfi_code.normalizeCandidateNumber(rawNumber);
      if (suffix == null) {
        break;
      }
      if (_isAlnumString(_codePointAt(text, cursor))) {
        break;
      }
      codes.add('AZE-SUN-INT-QMS-RFI-$discipline-$suffix');
    }
  }
  return codes;
}

final RegExp _rfiPattern = RegExp(
  r'AZE-SUN-INT-QMS-RFI-([A-Z]{3})-([0-9]{1,4})',
);
const String _suffixSeparators = '_/,;';
final RegExp _alnumChar = RegExp(r'^[\p{Alphabetic}\p{N}]$', unicode: true);

bool _isAlnumString(String? character) =>
    character != null && _alnumChar.hasMatch(character);

bool _isAsciiDigit(int unit) => unit >= 0x30 && unit <= 0x39;

bool _isAsciiWhitespace(int unit) =>
    unit == 0x20 ||
    unit == 0x09 ||
    unit == 0x0A ||
    unit == 0x0C ||
    unit == 0x0D;

int _skipAsciiWhitespace(String text, int index) {
  var cursor = index;
  while (cursor < text.length && _isAsciiWhitespace(text.codeUnitAt(cursor))) {
    cursor++;
  }
  return cursor;
}

/// The code point starting at [index] (surrogate-pair aware), or null at end.
String? _codePointAt(String text, int index) {
  if (index >= text.length) {
    return null;
  }
  final unit = text.codeUnitAt(index);
  if (_isHighSurrogate(unit) &&
      index + 1 < text.length &&
      _isLowSurrogate(text.codeUnitAt(index + 1))) {
    return text.substring(index, index + 2);
  }
  return text.substring(index, index + 1);
}

/// The code point ending at [index] (exclusive), or null at the start.
String? _codePointBefore(String text, int index) {
  if (index <= 0) {
    return null;
  }
  final unit = text.codeUnitAt(index - 1);
  if (_isLowSurrogate(unit) &&
      index - 2 >= 0 &&
      _isHighSurrogate(text.codeUnitAt(index - 2))) {
    return text.substring(index - 2, index);
  }
  return text.substring(index - 1, index);
}

bool _isHighSurrogate(int unit) => unit >= 0xD800 && unit <= 0xDBFF;

bool _isLowSurrogate(int unit) => unit >= 0xDC00 && unit <= 0xDFFF;

/// Port of `summarize_file_names`: first three names, then "(+n)".
String _summarizeFileNames(SplayTreeSet<String> fileNames) {
  const shown = 3;
  final first = fileNames.take(shown).toList();
  final remaining = fileNames.length - first.length;
  if (remaining == 0) {
    return first.join(', ');
  }
  return '${first.join(', ')} (+$remaining)';
}

/// Port of `analyze_file` without the PDF OCR step (runs in a worker isolate).
_PartialAnalysis _analyzeWithoutPdf(String path) {
  final fileName = p.basename(path);
  final filenameCodes = exactCodes(fileName);
  final extension = _asciiLowercase(_rustExtension(fileName) ?? '');
  final isMail = extension == 'eml' || extension == 'msg';
  final inspectContent = filenameCodes.isEmpty || isMail;
  final detections = SplayTreeMap<String, _Detection>();
  for (final code in filenameCodes) {
    detections[code] = _Detection(code, 'rfi.fileGroup.method.filename', 1.0);
  }
  String? issue;
  var needsPdfContent = false;
  if (inspectContent) {
    if (extension == 'pdf') {
      needsPdfContent = true;
    } else {
      final isOffice =
          extension == 'docx' ||
          extension == 'xlsx' ||
          extension == 'xlsm' ||
          extension == 'pptx';
      try {
        final codes = isOffice ? _packageCodes(path) : _rawCodes(path);
        final methodKey = switch (extension) {
          'eml' || 'msg' => 'rfi.fileGroup.method.mail',
          'docx' || 'xlsx' || 'xlsm' || 'pptx' => 'rfi.fileGroup.method.office',
          _ => 'rfi.fileGroup.method.content',
        };
        for (final code in codes) {
          detections.putIfAbsent(code, () => _Detection(code, methodKey, 1.0));
        }
      } on _InspectionFailure catch (error) {
        if (isMail) {
          detections.clear();
        }
        issue = error.message;
      }
    }
  }
  return _PartialAnalysis(
    path: path,
    detections: detections.values.toList(),
    issue: issue,
    isMail: isMail,
    needsPdfContent: needsPdfContent,
  );
}

/// Completes a PDF analysis on the calling isolate (OCR adapter is not
/// sendable between isolates).
Future<_FileAnalysis> _finishPdfAnalysis(
  _PartialAnalysis partial,
  RfiOcrAdapter ocr,
  RfiPdfPageCounter pdfPageCount,
) async {
  final detections = <String, _Detection>{
    for (final detection in partial.detections) detection.code: detection,
  };
  String? issue = partial.issue;
  try {
    final codes = await _pdfCodes(partial.path, ocr, pdfPageCount);
    for (final code in codes) {
      detections.putIfAbsent(
        code,
        () => _Detection(code, 'rfi.fileGroup.method.pdf', 1.0),
      );
    }
  } on _InspectionFailure catch (error) {
    issue = error.message;
  }
  final sorted = detections.values.toList()
    ..sort((left, right) => left.code.compareTo(right.code));
  return _FileAnalysis(
    path: partial.path,
    detections: sorted,
    issue: issue,
    isMail: partial.isMail,
  );
}

Future<SplayTreeSet<String>> _pdfCodes(
  String path,
  RfiOcrAdapter ocr,
  RfiPdfPageCounter pdfPageCount,
) async {
  final int pageCount;
  try {
    pageCount = pdfPageCount(path);
  } on Object catch (error) {
    throw _InspectionFailure(error.toString());
  }
  if (pageCount > _maxPdfPages) {
    throw _InspectionFailure(
      'PDF exceeds the $_maxPdfPages-page inspection limit',
    );
  }
  final codes = SplayTreeSet<String>();
  for (var pageIndex = 0; pageIndex < pageCount; pageIndex++) {
    try {
      final read = await ocr.readPage(path, pageIndex);
      if (read.matched.method == rfi_code.RfiMethod.strict) {
        codes.add(read.matched.code);
      }
    } on RfiOcrEngineMissing {
      throw _InspectionFailure(ocr.diagnostics());
    } on RfiOcrDocumentFailure catch (error) {
      throw _InspectionFailure(error.message);
    }
  }
  return codes;
}

SplayTreeSet<String> _packageCodes(String path) {
  final File file = File(path);
  final int length;
  try {
    length = file.lengthSync();
  } on FileSystemException catch (error) {
    throw _InspectionFailure('metadata: $error');
  }
  if (length > _maxPackageBytes) {
    throw const _InspectionFailure('Office package exceeds the size limit');
  }
  final Uint8List bytes;
  try {
    bytes = file.readAsBytesSync();
  } on FileSystemException catch (error) {
    throw _InspectionFailure('open: $error');
  }
  final Archive archive;
  try {
    archive = ZipDecoder().decodeBytes(bytes);
  } on Object catch (error) {
    throw _InspectionFailure('Office package: $error');
  }
  final entries = archive.files;
  if (entries.length > _maxPackageEntries) {
    throw const _InspectionFailure('Office package has too many entries');
  }
  var total = 0;
  final codes = SplayTreeSet<String>();
  for (final entry in entries) {
    if (!_asciiLowercase(entry.name).endsWith('.xml')) {
      continue;
    }
    total += entry.size;
    if (total > _maxPackageTextBytes) {
      throw const _InspectionFailure(
        'Office package text exceeds the size limit',
      );
    }
    final List<int> content;
    try {
      content = entry.content as List<int>;
    } on Object catch (error) {
      throw _InspectionFailure('Office package read: $error');
    }
    codes.addAll(exactCodes(_xmlText(content)));
  }
  return codes;
}

/// Port of `xml_text`: concatenates raw (not entity-decoded) text and CDATA,
/// with newlines for `<br/>` and closing `p`, `tc` and `tr` elements.
String _xmlText(List<int> bytes) {
  var source = utf8.decode(bytes, allowMalformed: true);
  if (source.startsWith('﻿')) {
    source = source.substring(1);
  }
  final buffer = StringBuffer();
  try {
    for (final event in parseEvents(
      source,
      entityMapping: const _RawEntityMapping(),
      validateNesting: true,
    )) {
      switch (event) {
        case XmlTextEvent():
          buffer.write(event.value);
        case XmlCDATAEvent():
          buffer.write(event.value);
        case XmlStartElementEvent():
          // Rust `Event::Empty` (self-closing) emits a newline only for `br`.
          if (event.isSelfClosing && _xmlLocalName(event.name) == 'br') {
            buffer.write('\n');
          }
        case XmlEndElementEvent():
          if (_isBlockName(event.name)) {
            buffer.write('\n');
          }
        default:
          break;
      }
    }
  } on Object catch (error) {
    throw _InspectionFailure('Office package XML: $error');
  }
  return buffer.toString();
}

bool _isBlockName(String name) {
  final local = _xmlLocalName(name);
  return local == 'p' || local == 'tc' || local == 'tr';
}

String _xmlLocalName(String name) => name.split(':').last;

/// Keeps entity references verbatim, like quick-xml's un-unescaped text.
class _RawEntityMapping extends XmlEntityMapping {
  const _RawEntityMapping();

  @override
  String? decodeEntity(String input) => '&$input;';

  @override
  String encodeText(String input) => input;

  @override
  String encodeAttributeValue(String input, XmlAttributeType type) => input;
}

SplayTreeSet<String> _rawCodes(String path) {
  final File file = File(path);
  final int length;
  try {
    length = file.lengthSync();
  } on FileSystemException catch (error) {
    throw _InspectionFailure('metadata: $error');
  }
  if (length > _maxRawBytes) {
    throw const _InspectionFailure(
      'file exceeds the content-inspection size limit',
    );
  }
  final Uint8List bytes;
  try {
    bytes = file.readAsBytesSync();
  } on FileSystemException catch (error) {
    throw _InspectionFailure('read: $error');
  }
  final codes = exactCodes(utf8.decode(bytes, allowMalformed: true));
  for (var offset = 0; offset <= 1; offset++) {
    if (bytes.length <= offset + 1) {
      continue;
    }
    final pairCount = (bytes.length - offset) ~/ 2;
    final little = List<int>.filled(pairCount, 0);
    final big = List<int>.filled(pairCount, 0);
    for (var pair = 0; pair < pairCount; pair++) {
      final first = bytes[offset + pair * 2];
      final second = bytes[offset + pair * 2 + 1];
      little[pair] = first | (second << 8);
      big[pair] = (first << 8) | second;
    }
    codes.addAll(exactCodes(_utf16Lossy(little)));
    codes.addAll(exactCodes(_utf16Lossy(big)));
  }
  return codes;
}

/// Port of `String::from_utf16_lossy`: unpaired surrogates become U+FFFD.
String _utf16Lossy(List<int> units) {
  final out = <int>[];
  for (var index = 0; index < units.length; index++) {
    final unit = units[index];
    if (_isHighSurrogate(unit)) {
      if (index + 1 < units.length && _isLowSurrogate(units[index + 1])) {
        out
          ..add(unit)
          ..add(units[index + 1]);
        index++;
      } else {
        out.add(0xFFFD);
      }
    } else if (_isLowSurrogate(unit)) {
      out.add(0xFFFD);
    } else {
      out.add(unit);
    }
  }
  return String.fromCharCodes(out);
}

Step _directoryStep(
  String destination,
  String? code, {
  required bool warningReview,
}) {
  final meta = <String, Object?>{};
  if (code != null) {
    meta['code'] = code;
  }
  if (warningReview) {
    meta['warningReview'] = true;
  }
  return Step(
    kind: StepKind.mkdir,
    src: null,
    dst: destination,
    noteKey: 'note.rfi.fileGroup.folderCreated',
    meta: meta,
  );
}

Step _copyStep(
  String source,
  String destination,
  String? rowId,
  String? code, {
  required bool warningReview,
}) {
  final meta = <String, Object?>{};
  if (rowId != null) {
    meta['rowId'] = rowId;
  }
  if (code != null) {
    meta['code'] = code;
  }
  if (warningReview) {
    meta['warningReview'] = true;
  }
  return Step(
    kind: StepKind.copy,
    src: source,
    dst: destination,
    noteKey: warningReview
        ? 'note.rfi.fileGroup.reviewCopy'
        : 'note.rfi.fileGroup.copy',
    meta: meta,
  );
}

List<String> _discoverFiles(String root) {
  final List<FileSystemEntity> entries;
  try {
    entries = Directory(root).listSync(followLinks: false);
  } on FileSystemException {
    throw BackendError.pathInvalid('RFI file source could not be read');
  }
  final files = <String>[];
  for (final entry in entries) {
    final type = FileSystemEntity.typeSync(entry.path, followLinks: false);
    if (type == FileSystemEntityType.link) {
      throw BackendError.pathInvalid(
        'RFI file source contains a symbolic link',
      );
    }
    final hidden = p.basename(entry.path).startsWith('.');
    if (type == FileSystemEntityType.file && !hidden) {
      files.add(entry.path);
      if (files.length > _maxFiles) {
        throw BackendError.validation(
          'RFI file source exceeds the $_maxFiles-file limit',
        );
      }
    }
  }
  return files;
}

String _canonicalDirectory(String path, String label) {
  final String resolved;
  try {
    resolved = canonicalizePath(path);
  } on Object {
    throw BackendError.pathInvalid('$label does not exist');
  }
  if (FileSystemEntity.isDirectorySync(resolved)) {
    return resolved;
  }
  throw BackendError.pathInvalid('$label is not a directory');
}

bool _overlaps(String first, String second) =>
    _startsWith(first, second) || _startsWith(second, first);

/// Port of `Path::starts_with`: component-wise, equal paths included.
bool _startsWith(String path, String base) =>
    p.equals(path, base) || p.isWithin(base, path);

/// Port of `unsafe_existing_directory`: any existing ancestor (including the
/// path itself) that is a symbolic link or not a directory.
bool _unsafeExistingDirectory(String path) {
  var current = path;
  while (true) {
    final type = FileSystemEntity.typeSync(current, followLinks: false);
    if (type == FileSystemEntityType.link ||
        (type != FileSystemEntityType.notFound &&
            type != FileSystemEntityType.directory)) {
      return true;
    }
    final parent = p.dirname(current);
    if (parent == current) {
      return false;
    }
    current = parent;
  }
}

bool _pathExists(String path) =>
    FileSystemEntity.typeSync(path) != FileSystemEntityType.notFound;

bool _isDirectory(String path) => FileSystemEntity.isDirectorySync(path);

/// Port of `Path::extension` (a leading dot does not start an extension).
String? _rustExtension(String fileName) {
  final index = fileName.lastIndexOf('.');
  if (index <= 0) {
    return null;
  }
  return fileName.substring(index + 1);
}

String _asciiLowercase(String value) {
  final units = value.codeUnits.map((unit) {
    return unit >= 0x41 && unit <= 0x5A ? unit + 0x20 : unit;
  }).toList();
  return String.fromCharCodes(units);
}

/// Byte-order (code point) string comparison, matching Rust `String` / `OsStr`
/// ordering.
int _compareUtf8(String a, String b) {
  final left = a.runes.iterator;
  final right = b.runes.iterator;
  while (true) {
    final hasLeft = left.moveNext();
    final hasRight = right.moveNext();
    if (!hasLeft || !hasRight) {
      return (hasLeft ? 1 : 0) - (hasRight ? 1 : 0);
    }
    final comparison = left.current.compareTo(right.current);
    if (comparison != 0) {
      return comparison;
    }
  }
}

/// Deep-copies JSON-like values (maps, lists and scalars).
Object? _deepCopy(Object? value) {
  if (value is Map) {
    return <String, Object?>{
      for (final entry in value.entries)
        entry.key as String: _deepCopy(entry.value),
    };
  }
  if (value is List) {
    return [for (final item in value) _deepCopy(item)];
  }
  return value;
}

/// Deep structural equality of JSON-like values (`serde_json::Value` `==`).
bool _jsonDeepEquals(Object? a, Object? b) {
  if (identical(a, b)) {
    return true;
  }
  if (a is Map && b is Map) {
    if (a.length != b.length) {
      return false;
    }
    for (final key in a.keys) {
      if (!b.containsKey(key) || !_jsonDeepEquals(a[key], b[key])) {
        return false;
      }
    }
    return true;
  }
  if (a is List && b is List) {
    if (a.length != b.length) {
      return false;
    }
    for (var index = 0; index < a.length; index++) {
      if (!_jsonDeepEquals(a[index], b[index])) {
        return false;
      }
    }
    return true;
  }
  return a == b;
}
