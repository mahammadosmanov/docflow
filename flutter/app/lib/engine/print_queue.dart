/// Port of src-tauri/src/print_queue.rs: the batch print queue, the print
/// adapter contract, durable queue state, and the apply/resume engine.
///
/// Safety rules preserved from Rust:
///  * A print that is uncertain (may already have reached the spooler) is
///    never retried automatically; it pauses in `RecoveryRequired`.
///  * Every effect writes a pending journal record before it runs.
///  * Every source file is re-authorized through path grants and re-checked
///    by signature (and SHA-256 for workbooks) immediately before submission.
///
/// Platform adapters live under `platform/`. Excel worksheet exports go
/// through [ExcelExportPipelinePort] (E12 owns the implementation).
library;

import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:crypto/crypto.dart' as crypto;
import 'package:path/path.dart' as p;
import 'package:uuid/uuid.dart';

import 'cap_fs.dart' as cap;
import 'contracts.dart';
import 'errors.dart';
import 'journal.dart';
import 'ooxml.dart';
import 'operations/batch_print.dart' as batch_print;
import 'page_range.dart';
import 'paths.dart';
import 'plan_store.dart';
import 'progress.dart';

const int defaultTimeoutMinutes = 30;

const Uuid _uuid = Uuid();

const int _maxQueueBytes = 16 * 1024 * 1024;
const int _maxSavedJobs = 25000;

String _defaultPageRange() => 'all';

// ---- JSON helpers (serde-compatible field access) --------------------------

String _reqStr(Map<String, Object?> json, String key) {
  final value = json[key];
  if (value is! String) {
    throw FormatException('field `$key` must be a string');
  }
  return value;
}

String _strOr(Map<String, Object?> json, String key, String fallback) {
  if (!json.containsKey(key)) {
    return fallback;
  }
  final value = json[key];
  if (value is! String) {
    throw FormatException('field `$key` must be a string');
  }
  return value;
}

String? _optStrField(Map<String, Object?> json, String key) {
  final value = json[key];
  if (value == null) {
    return null;
  }
  if (value is! String) {
    throw FormatException('field `$key` must be a string');
  }
  return value;
}

bool _boolOr(Map<String, Object?> json, String key, bool fallback) {
  if (!json.containsKey(key)) {
    return fallback;
  }
  final value = json[key];
  if (value is! bool) {
    throw FormatException('field `$key` must be a boolean');
  }
  return value;
}

int _uintOr(Map<String, Object?> json, String key, int fallback) {
  if (!json.containsKey(key)) {
    return fallback;
  }
  return _asU32(json[key]!, key);
}

int _u64Or(Map<String, Object?> json, String key, int fallback) {
  if (!json.containsKey(key)) {
    return fallback;
  }
  final value = json[key];
  if (value is! int || value < 0) {
    throw FormatException('field `$key` must be an unsigned integer');
  }
  return value;
}

int _requiredU32(Map<String, Object?> json, String key) {
  if (!json.containsKey(key)) {
    throw FormatException('missing field `$key`');
  }
  return _asU32(json[key]!, key);
}

int _asU32(Object value, String key) {
  if (value is! int || value < 0 || value > 0xFFFFFFFF) {
    throw FormatException('field `$key` must be an unsigned 32-bit integer');
  }
  return value;
}

Map<String, Object?> _objectValue(Object? value, String message) {
  if (value is! Map) {
    throw FormatException(message);
  }
  return Map<String, Object?>.from(value);
}

Map<String, Object?> _copyJsonMap(Map<String, Object?> value) =>
    Map<String, Object?>.from(value);

// ---- Workbook metadata JSON (serde camelCase of ooxml.rs types) -----------

/// Serde shape of `WorksheetDescriptor`. Mirrors the Rust derive; the
/// ooxml module does not yet carry JSON helpers.
Map<String, Object?> worksheetDescriptorToJson(WorksheetDescriptor value) => {
  'workbookIndex': value.workbookIndex,
  'sheetId': value.sheetId,
  'relationshipId': value.relationshipId,
  'relationshipType': value.relationshipType,
  'partPath': value.partPath,
  'name': value.name,
  'visibility': _visibilityWire(value.visibility),
};

WorksheetDescriptor worksheetDescriptorFromJson(Object? value) {
  final json = _objectValue(value, 'invalid worksheet descriptor');
  return WorksheetDescriptor(
    workbookIndex: _asU32(_requiredNum(json, 'workbookIndex'), 'workbookIndex'),
    sheetId: _asU32(_requiredNum(json, 'sheetId'), 'sheetId'),
    relationshipId: _reqStr(json, 'relationshipId'),
    relationshipType: _reqStr(json, 'relationshipType'),
    partPath: _reqStr(json, 'partPath'),
    name: _reqStr(json, 'name'),
    visibility: _visibilityFromWire(_reqStr(json, 'visibility')),
  );
}

Object _requiredNum(Map<String, Object?> json, String key) {
  final value = json[key];
  if (value is! num) {
    throw FormatException('field `$key` must be a number');
  }
  return value;
}

String _visibilityWire(WorksheetVisibility value) => switch (value) {
  WorksheetVisibility.visible => 'visible',
  WorksheetVisibility.hidden => 'hidden',
  WorksheetVisibility.veryHidden => 'veryHidden',
};

WorksheetVisibility _visibilityFromWire(String value) => switch (value) {
  'visible' => WorksheetVisibility.visible,
  'hidden' => WorksheetVisibility.hidden,
  'veryHidden' => WorksheetVisibility.veryHidden,
  _ => throw FormatException('unknown worksheet visibility `$value`'),
};

/// Serde shape of `WorkbookSafetyFacts`.
Map<String, Object?> workbookSafetyToJson(WorkbookSafetyFacts value) => {
  'hasVbaProject': value.hasVbaProject,
  'hasXlmMacroSheet': value.hasXlmMacroSheet,
  'hasWorkbookProtection': value.hasWorkbookProtection,
  'externalLinkCount': value.externalLinkCount,
  'connectionCount': value.connectionCount,
  'queryTableCount': value.queryTableCount,
};

WorkbookSafetyFacts workbookSafetyFromJson(Object? value) {
  final json = _objectValue(value, 'invalid workbook safety facts');
  return WorkbookSafetyFacts(
    hasVbaProject: _boolOr(json, 'hasVbaProject', false),
    hasXlmMacroSheet: _boolOr(json, 'hasXlmMacroSheet', false),
    hasWorkbookProtection: _boolOr(json, 'hasWorkbookProtection', false),
    externalLinkCount: _uintOr(json, 'externalLinkCount', 0),
    connectionCount: _uintOr(json, 'connectionCount', 0),
    queryTableCount: _uintOr(json, 'queryTableCount', 0),
  );
}

// ---- Printer inventory and support ----------------------------------------

bool _isControlCodePoint(int c) =>
    c <= 0x1F || (c >= 0x7F && c <= 0x9F);

bool _validPrinterName(String printer) {
  if (printer.isEmpty) {
    return false;
  }
  final runes = printer.runes.toList();
  if (runes.length > 256) {
    return false;
  }
  for (final c in runes) {
    if (_isControlCodePoint(c) || c == 0 || c == 0x22) {
      return false;
    }
  }
  return true;
}

bool _asciiEqualsIgnoreCase(String a, String b) {
  if (a.length != b.length) {
    return false;
  }
  for (var i = 0; i < a.length; i++) {
    var x = a.codeUnitAt(i);
    var y = b.codeUnitAt(i);
    if (x >= 0x41 && x <= 0x5A) x += 0x20;
    if (y >= 0x41 && y <= 0x5A) y += 0x20;
    if (x != y) {
      return false;
    }
  }
  return true;
}

class PrinterInventory {
  const PrinterInventory({this.printers = const [], this.defaultPrinter});

  final List<String> printers;
  final String? defaultPrinter;

  /// Port of `PrinterInventory::normalized`: drop invalid names, sort
  /// case-insensitively (stable), drop consecutive case-insensitive
  /// duplicates, and move a present default printer to the front.
  PrinterInventory normalized() {
    final valid = [
      for (final printer in printers)
        if (_validPrinterName(printer)) printer,
    ];
    // Stable sort by lowercase key (Rust `sort_by_key` is stable).
    final indexed = [
      for (var i = 0; i < valid.length; i++) (i, valid[i], valid[i].toLowerCase()),
    ];
    indexed.sort((a, b) {
      final byKey = a.$3.compareTo(b.$3);
      return byKey != 0 ? byKey : a.$1.compareTo(b.$1);
    });
    final sorted = [for (final entry in indexed) entry.$2];
    final deduped = <String>[];
    for (final printer in sorted) {
      if (deduped.isNotEmpty &&
          _asciiEqualsIgnoreCase(deduped.last, printer)) {
        continue;
      }
      deduped.add(printer);
    }
    var defaultValue = defaultPrinter;
    final index = defaultValue == null ? -1 : deduped.indexOf(defaultValue);
    if (defaultValue != null && index >= 0) {
      final moved = deduped.removeAt(index);
      deduped.insert(0, moved);
    }
    if (defaultValue != null && !deduped.contains(defaultValue)) {
      defaultValue = null;
    }
    return PrinterInventory(printers: deduped, defaultPrinter: defaultValue);
  }

  bool contains(String printer) =>
      _validPrinterName(printer) && printers.any((known) => known == printer);
}

/// Port of `PrintJobSupport`.
class PrintJobSupport {
  const PrintJobSupport({
    required this.verb,
    required this.settingsLimited,
    required this.pageRange,
  });

  /// Route verb: `docflow` (controlled), `printto` (native), `print`.
  final String verb;
  final bool settingsLimited;
  final PageRangeCapability pageRange;

  factory PrintJobSupport.controlled() => const PrintJobSupport(
    verb: 'docflow',
    settingsLimited: false,
    pageRange: AllOnlyPageRangeCapability(),
  );

  factory PrintJobSupport.controlledPdf({int? pageCount}) => PrintJobSupport(
    verb: 'docflow',
    settingsLimited: false,
    pageRange: PdfPageRangeCapability(pageCount: pageCount),
  );

  factory PrintJobSupport.singlePage() => const PrintJobSupport(
    verb: 'docflow',
    settingsLimited: false,
    pageRange: SinglePagePageRangeCapability(),
  );

  factory PrintJobSupport.nativeApplication() => const PrintJobSupport(
    verb: 'printto',
    settingsLimited: true,
    pageRange: AllOnlyPageRangeCapability(),
  );
}

/// Port of `PrintSettings`.
class PrintSettings {
  const PrintSettings({
    required this.paper,
    required this.orientation,
    required this.scaling,
    required this.color,
    required this.duplex,
    required this.pageRange,
    required this.timeoutMinutes,
  });

  final String paper;
  final String orientation;
  final String scaling;
  final String color;
  final String duplex;
  final String pageRange;
  final int timeoutMinutes;

  @override
  bool operator ==(Object other) =>
      other is PrintSettings &&
      other.paper == paper &&
      other.orientation == orientation &&
      other.scaling == scaling &&
      other.color == color &&
      other.duplex == duplex &&
      other.pageRange == pageRange &&
      other.timeoutMinutes == timeoutMinutes;

  @override
  int get hashCode => Object.hash(
    paper,
    orientation,
    scaling,
    color,
    duplex,
    pageRange,
    timeoutMinutes,
  );

  @override
  String toString() =>
      'PrintSettings($paper, $orientation, $scaling, $color, $duplex, '
      '$pageRange, $timeoutMinutes)';
}

// ---- Failures --------------------------------------------------------------

enum PrintFailureKind {
  failed,
  unsupported,
  unsupportedOptions,
  submittedUnconfirmed,
  uncertain,
  cancelled,
}

/// `RetryDisposition`. `cleanupQuarantined` exists on Windows and in tests.
enum RetryDisposition { retryable, cleanupQuarantined, never }

/// Port of `CleanupReview`.
class CleanupReview {
  const CleanupReview({required this.warningKey, required this.locator});

  final String warningKey;
  final String locator;

  @override
  bool operator ==(Object other) =>
      other is CleanupReview &&
      other.warningKey == warningKey &&
      other.locator == locator;

  @override
  int get hashCode => Object.hash(warningKey, locator);

  @override
  String toString() => 'CleanupReview($warningKey, $locator)';
}

/// Port of `PrintSubmissionOutcome`.
class PrintSubmissionOutcome {
  const PrintSubmissionOutcome({this.cleanupReview});

  final CleanupReview? cleanupReview;
}

/// Port of `PrintFailure`. Thrown wherever Rust returns `Err(PrintFailure)`.
class PrintFailure implements Exception {
  const PrintFailure._(
    this.kind,
    this.message, {
    required this.uncertain,
    required this.retryDisposition,
    this.cleanupReview,
    this.diagnosticDetail,
  });

  final PrintFailureKind kind;

  /// Localization key (`error.print.*`) or a test-only message.
  final String message;
  final bool uncertain;
  final RetryDisposition retryDisposition;
  final CleanupReview? cleanupReview;
  final String? diagnosticDetail;

  factory PrintFailure.failed(String message) => PrintFailure._(
    PrintFailureKind.failed,
    message,
    uncertain: false,
    retryDisposition: RetryDisposition.retryable,
  );

  factory PrintFailure.unsupported(String message) => PrintFailure._(
    PrintFailureKind.unsupported,
    message,
    uncertain: false,
    retryDisposition: RetryDisposition.never,
  );

  factory PrintFailure.unsupportedOptions(String message) => PrintFailure._(
    PrintFailureKind.unsupportedOptions,
    message,
    uncertain: false,
    retryDisposition: RetryDisposition.never,
  );

  factory PrintFailure.uncertain(String message) => PrintFailure._(
    PrintFailureKind.uncertain,
    message,
    uncertain: true,
    retryDisposition: RetryDisposition.never,
  );

  factory PrintFailure.submittedUnconfirmed(String message) => PrintFailure._(
    PrintFailureKind.submittedUnconfirmed,
    message,
    uncertain: true,
    retryDisposition: RetryDisposition.never,
  );

  factory PrintFailure.cancelled(String message, {required bool uncertain}) =>
      PrintFailure._(
        PrintFailureKind.cancelled,
        message,
        uncertain: uncertain,
        retryDisposition: RetryDisposition.never,
      );

  bool get hasCleanupReview => cleanupReview != null;

  PrintFailure withCleanupReview(CleanupReview review) => PrintFailure._(
    kind,
    message,
    uncertain: uncertain,
    retryDisposition: retryDisposition,
    cleanupReview: review,
    diagnosticDetail: diagnosticDetail,
  );

  PrintFailure withRetryDisposition(RetryDisposition disposition) =>
      PrintFailure._(
        kind,
        message,
        uncertain: uncertain,
        retryDisposition: disposition,
        cleanupReview: cleanupReview,
        diagnosticDetail: diagnosticDetail,
      );

  PrintFailure withDiagnostic(String detail) => PrintFailure._(
    kind,
    message,
    uncertain: uncertain,
    retryDisposition: retryDisposition,
    cleanupReview: cleanupReview,
    diagnosticDetail: _redactError(detail),
  );

  bool get isUnsupported => kind == PrintFailureKind.unsupported;

  bool get isUnsupportedOptions => kind == PrintFailureKind.unsupportedOptions;

  BackendError intoBackendError() => switch (kind) {
    PrintFailureKind.unsupported ||
    PrintFailureKind.unsupportedOptions => BackendError.validation(message),
    _ => BackendError.requestFailed(message),
  };

  @override
  bool operator ==(Object other) =>
      other is PrintFailure &&
      other.kind == kind &&
      other.message == message &&
      other.uncertain == uncertain &&
      other.retryDisposition == retryDisposition &&
      other.cleanupReview == cleanupReview;

  @override
  int get hashCode =>
      Object.hash(kind, message, uncertain, retryDisposition, cleanupReview);

  @override
  String toString() => 'PrintFailure($kind, $message)';
}

bool _retryDispositionAllowsRetry(PrintFailure failure) =>
    failure.retryDisposition == RetryDisposition.retryable;

String _submissionPrimaryClassification(
  PrintSubmissionOutcome? outcome,
  PrintFailure? failure,
) {
  if (failure == null) {
    return 'completed';
  }
  return switch (failure.kind) {
    PrintFailureKind.failed => 'failed',
    PrintFailureKind.unsupported => 'unsupported',
    PrintFailureKind.unsupportedOptions => 'unsupported_options',
    PrintFailureKind.submittedUnconfirmed => 'submitted_unconfirmed',
    PrintFailureKind.uncertain => 'uncertain',
    PrintFailureKind.cancelled when failure.uncertain => 'cancelled_uncertain',
    PrintFailureKind.cancelled => 'cancelled',
  };
}

// ---- Adapter contract ------------------------------------------------------

/// Port of the `PrintAdapter` trait.
abstract interface class PrintAdapter {
  PrinterInventory inventory();

  PrintJobSupport inspect(
    String path,
    String printer,
    PrintSettings settings,
  );

  /// Submits one job. [cancelled] is polled by adapters that block.
  void submit(PrintJob job, bool Function() cancelled);
}

/// Port of the Excel export pipeline's submit entry point consumed by
/// [PrintService] for worksheet jobs. Implemented by the E12 Excel adapter.
abstract interface class ExcelExportPipelinePort {
  PrintSubmissionOutcome exportAndSubmit(
    PrintJob job,
    cap.AnchoredPath source,
    cap.OsHandle sourceFile,
    CancellationToken cancellation,
  );
}

// ---- Opened source files ---------------------------------------------------

/// The part of an open source file that signature and workbook checks read.
abstract interface class OpenedPrintFile {
  bool get isFile;
  int get size;
  int get mtimeNs;

  /// Reads the complete content from the start.
  Uint8List readAllFromStart();
}

class _HandlePrintFile implements OpenedPrintFile {
  _HandlePrintFile(this.handle);

  final cap.OsHandle handle;

  @override
  bool get isFile => handle.metadata().isFile;

  @override
  int get size => handle.metadata().size;

  @override
  int get mtimeNs => handle.metadata().mtimeNs;

  @override
  Uint8List readAllFromStart() {
    handle.seek(0);
    final out = BytesBuilder(copy: false);
    final buffer = Uint8List(64 * 1024);
    while (true) {
      final count = handle.read(buffer);
      if (count == 0) {
        break;
      }
      out.add(Uint8List.sublistView(buffer, 0, count));
    }
    handle.seek(0);
    return out.takeBytes();
  }
}

class _DiskPrintFile implements OpenedPrintFile {
  _DiskPrintFile(this.file) : _stat = file.statSync();

  final File file;
  final FileStat _stat;

  @override
  bool get isFile => _stat.type == FileSystemEntityType.file;

  @override
  int get size => _stat.size;

  @override
  int get mtimeNs => _stat.modified.microsecondsSinceEpoch * 1000;

  @override
  Uint8List readAllFromStart() => file.readAsBytesSync();
}

/// Port of `file_signature_from_open_file`.
FileSignature fileSignatureFromOpenFile(
  OpenedPrintFile file, {
  required bool includeSha256,
}) {
  final mtimeNs = file.mtimeNs;
  if (mtimeNs < 0) {
    throw BackendError.pathInvalid('print file timestamp is unavailable');
  }
  final sha256 = includeSha256
      ? crypto.sha256.convert(file.readAllFromStart()).toString()
      : null;
  return FileSignature(size: file.size, mtimeNs: mtimeNs, sha256: sha256);
}

/// Port of `file_signature` (path form, no digest unless requested).
FileSignature fileSignature(String path) {
  final File file = File(path);
  if (!file.existsSync()) {
    throw BackendError.pathInvalid('print file is unavailable');
  }
  return fileSignatureFromOpenFile(_DiskPrintFile(file), includeSha256: false);
}

/// Port of `file_signature_from_open_file` for a [File] with the digest.
FileSignature fileSignatureFromFile(File file, {required bool includeSha256}) =>
    fileSignatureFromOpenFile(
      _DiskPrintFile(file),
      includeSha256: includeSha256,
    );

/// Adapter over an opened capability handle, used by the queue engine.
OpenedPrintFile openedPrintFileFromHandle(cap.OsHandle handle) =>
    _HandlePrintFile(handle);

/// Adapter over a disk [File], used by previews outside the grant engine.
OpenedPrintFile openedPrintFileFromDisk(File file) => _DiskPrintFile(file);

// ---- Params and jobs -------------------------------------------------------

/// Port of `BatchPrintParams` (`deny_unknown_fields`, `default`).
class BatchPrintParams {
  BatchPrintParams({
    this.files = const [],
    this.printer = '',
    this.paper = 'A4',
    this.orientation = 'auto',
    this.scaling = 'fit',
    this.color = 'printer',
    this.duplex = 'printer',
    this.copies = 1,
    this.collate = true,
    this.retries = 0,
    this.onError = 'pause',
    this.timeoutMinutes = defaultTimeoutMinutes,
  });

  static const Set<String> _knownKeys = {
    'files',
    'printer',
    'paper',
    'orientation',
    'scaling',
    'color',
    'duplex',
    'copies',
    'collate',
    'retries',
    'onError',
    'timeoutMinutes',
  };

  final List<String> files;
  final String printer;
  final String paper;
  final String orientation;
  final String scaling;
  final String color;
  final String duplex;
  final int copies;
  final bool collate;
  final int retries;
  final String onError;
  final int timeoutMinutes;

  factory BatchPrintParams.fromJson(Map<String, Object?> json) {
    final unknown = json.keys.where((key) => !_knownKeys.contains(key));
    if (unknown.isNotEmpty) {
      throw FormatException('unknown field `${unknown.first}`');
    }
    final defaults = BatchPrintParams();
    final files = json['files'];
    return BatchPrintParams(
      files: files == null
          ? defaults.files
          : [
              for (final item in files as List) item as String,
            ],
      printer: _strOr(json, 'printer', defaults.printer),
      paper: _strOr(json, 'paper', defaults.paper),
      orientation: _strOr(json, 'orientation', defaults.orientation),
      scaling: _strOr(json, 'scaling', defaults.scaling),
      color: _strOr(json, 'color', defaults.color),
      duplex: _strOr(json, 'duplex', defaults.duplex),
      copies: _uintOr(json, 'copies', defaults.copies),
      collate: _boolOr(json, 'collate', defaults.collate),
      retries: _uintOr(json, 'retries', defaults.retries),
      onError: _strOr(json, 'onError', defaults.onError),
      timeoutMinutes: _uintOr(json, 'timeoutMinutes', defaults.timeoutMinutes),
    );
  }
}

/// Serialized file signature (`FileSignature`, serde camelCase).
class FileSignature {
  const FileSignature({this.size = 0, this.mtimeNs = 0, this.sha256});

  factory FileSignature.fromJson(Object? value) {
    final json = _objectValue(value, 'invalid file signature');
    return FileSignature(
      size: _u64Or(json, 'size', 0),
      mtimeNs: _u64Or(json, 'mtimeNs', 0),
      sha256: _optStrField(json, 'sha256'),
    );
  }

  final int size;
  final int mtimeNs;
  final String? sha256;

  bool get isLegacyEmpty => size == 0 && mtimeNs == 0 && sha256 == null;

  /// `has_valid_sha256`: exactly 64 lowercase hexadecimal characters.
  bool get hasValidSha256 {
    final value = sha256;
    if (value == null || value.length != 64) {
      return false;
    }
    for (final unit in value.codeUnits) {
      final digit = unit >= 0x30 && unit <= 0x39;
      final hex = unit >= 0x61 && unit <= 0x66;
      if (!digit && !hex) {
        return false;
      }
    }
    return true;
  }

  Map<String, Object?> toJson() => {
    'size': size,
    'mtimeNs': mtimeNs,
    if (sha256 != null) 'sha256': sha256,
  };

  @override
  bool operator ==(Object other) =>
      other is FileSignature &&
      other.size == size &&
      other.mtimeNs == mtimeNs &&
      other.sha256 == sha256;

  @override
  int get hashCode => Object.hash(size, mtimeNs, sha256);
}

/// Port of `PrintJob`. Unknown fields are kept in [extra] and re-emitted.
class PrintJob {
  PrintJob({
    this.jobId = '',
    this.path = '',
    this.printer = '',
    this.verb = '',
    this.format = '',
    this.paper = 'A4',
    this.orientation = 'auto',
    this.scaling = 'fit',
    this.color = 'printer',
    this.duplex = 'printer',
    this.pageRange = 'all',
    this.timeoutMinutes = defaultTimeoutMinutes,
    this.retries = 0,
    this.onError = 'pause',
    this.copy = 1,
    FileSignature? signature,
    this.worksheet,
    this.workbookSafety,
    this.status,
    this.attempts,
    this.error,
    this.completedAt,
    this.cleanupReview,
    Map<String, Object?>? extra,
  }) : signature = signature ?? const FileSignature(),
       extra = extra ?? <String, Object?>{};

  static const Set<String> _knownKeys = {
    'jobId',
    'path',
    'printer',
    'verb',
    'format',
    'paper',
    'orientation',
    'scaling',
    'color',
    'duplex',
    'pageRange',
    'timeoutMinutes',
    'retries',
    'onError',
    'copy',
    'signature',
    'worksheet',
    'workbookSafety',
    'status',
    'attempts',
    'error',
    'completedAt',
    'cleanupReview',
  };

  String jobId;
  String path;
  String printer;
  String verb;
  String format;
  String paper;
  String orientation;
  String scaling;
  String color;
  String duplex;
  String pageRange;
  int timeoutMinutes;
  int retries;
  String onError;
  int copy;
  FileSignature signature;
  WorksheetDescriptor? worksheet;
  WorkbookSafetyFacts? workbookSafety;
  String? status;
  int? attempts;
  String? error;
  String? completedAt;
  CleanupReviewEvidence? cleanupReview;
  final Map<String, Object?> extra;

  /// Port of the custom `Deserialize for PrintJob`: null metadata is refused,
  /// worksheet metadata must be complete, and orphan metadata is refused.
  factory PrintJob.fromJson(Object? value) {
    if (value is! Map) {
      throw const FormatException('print job must be an object');
    }
    final json = Map<String, Object?>.from(value);
    final worksheetPresent = json.containsKey('worksheet');
    final safetyPresent = json.containsKey('workbookSafety');
    final signaturePresent = json.containsKey('signature');
    for (final field in ['worksheet', 'workbookSafety', 'signature']) {
      if (json[field] == null && json.containsKey(field)) {
        throw FormatException('print job field $field cannot be null');
      }
    }
    final defaults = PrintJob();
    final extra = <String, Object?>{
      for (final entry in json.entries)
        if (!_knownKeys.contains(entry.key)) entry.key: entry.value,
    };
    final signature = json['signature'] == null
        ? defaults.signature
        : FileSignature.fromJson(json['signature']);
    final worksheet = json['worksheet'] == null
        ? null
        : worksheetDescriptorFromJson(json['worksheet']);
    final workbookSafety = json['workbookSafety'] == null
        ? null
        : workbookSafetyFromJson(json['workbookSafety']);
    final cleanupRaw = json['cleanupReview'];
    final job = PrintJob(
      jobId: _strOr(json, 'jobId', defaults.jobId),
      path: _strOr(json, 'path', defaults.path),
      printer: _strOr(json, 'printer', defaults.printer),
      verb: _strOr(json, 'verb', defaults.verb),
      format: _strOr(json, 'format', defaults.format),
      paper: _strOr(json, 'paper', defaults.paper),
      orientation: _strOr(json, 'orientation', defaults.orientation),
      scaling: _strOr(json, 'scaling', defaults.scaling),
      color: _strOr(json, 'color', defaults.color),
      duplex: _strOr(json, 'duplex', defaults.duplex),
      pageRange: _strOr(json, 'pageRange', _defaultPageRange()),
      timeoutMinutes: _uintOr(json, 'timeoutMinutes', defaults.timeoutMinutes),
      retries: _uintOr(json, 'retries', defaults.retries),
      onError: _strOr(json, 'onError', defaults.onError),
      copy: _uintOr(json, 'copy', defaults.copy),
      signature: signature,
      worksheet: worksheet,
      workbookSafety: workbookSafety,
      status: _optStrField(json, 'status'),
      attempts: json['attempts'] == null
          ? null
          : _asU32(json['attempts']!, 'attempts'),
      error: _optStrField(json, 'error'),
      completedAt: _optStrField(json, 'completedAt'),
      cleanupReview: cleanupRaw == null
          ? null
          : CleanupReviewEvidence.fromJson(cleanupRaw),
      extra: extra,
    );
    if (worksheetPresent) {
      if (!signaturePresent ||
          !safetyPresent ||
          worksheet == null ||
          workbookSafety == null ||
          !job.signature.hasValidSha256) {
        throw const FormatException('incomplete worksheet print metadata');
      }
    } else if (safetyPresent ||
        job.workbookSafety != null ||
        job.signature.sha256 != null) {
      throw const FormatException('orphan worksheet print metadata');
    }
    return job;
  }

  Map<String, Object?> toJson() => {
    ...extra,
    'jobId': jobId,
    'path': path,
    'printer': printer,
    'verb': verb,
    'format': format,
    'paper': paper,
    'orientation': orientation,
    'scaling': scaling,
    'color': color,
    'duplex': duplex,
    'pageRange': pageRange,
    'timeoutMinutes': timeoutMinutes,
    'retries': retries,
    'onError': onError,
    'copy': copy,
    'signature': signature.toJson(),
    if (worksheet != null)
      'worksheet': worksheetDescriptorToJson(worksheet!),
    if (workbookSafety != null)
      'workbookSafety': workbookSafetyToJson(workbookSafety!),
    if (status != null) 'status': status,
    if (attempts != null) 'attempts': attempts,
    if (error != null) 'error': error,
    if (completedAt != null) 'completedAt': completedAt,
    if (cleanupReview != null) 'cleanupReview': cleanupReview!.toJson(),
  };

  PrintSettings settings() => PrintSettings(
    paper: paper,
    orientation: orientation,
    scaling: scaling,
    color: color,
    duplex: duplex,
    pageRange: pageRange,
    timeoutMinutes: timeoutMinutes,
  );

  /// Deep copy (Rust `Clone`).
  PrintJob clone() => PrintJob(
    jobId: jobId,
    path: path,
    printer: printer,
    verb: verb,
    format: format,
    paper: paper,
    orientation: orientation,
    scaling: scaling,
    color: color,
    duplex: duplex,
    pageRange: pageRange,
    timeoutMinutes: timeoutMinutes,
    retries: retries,
    onError: onError,
    copy: copy,
    signature: signature,
    worksheet: worksheet,
    workbookSafety: workbookSafety,
    status: status,
    attempts: attempts,
    error: error,
    completedAt: completedAt,
    cleanupReview: cleanupReview == null
        ? null
        : CleanupReviewEvidence.fromJson(cleanupReview!.toJson()),
    extra: _copyJsonMap(extra),
  );

  /// Port of `PrintJob::normalize_legacy`.
  void normalizeLegacy(int index) {
    if (jobId.isEmpty) {
      jobId = 'legacy-${index + 1}';
    }
    if (paper.isEmpty) {
      paper = 'A4';
    }
    if (orientation.isEmpty) {
      orientation = 'auto';
    }
    if (scaling.isEmpty) {
      scaling = 'fit';
    }
    if (color.isEmpty) {
      color = 'printer';
    }
    if (duplex.isEmpty) {
      duplex = 'printer';
    }
    if (timeoutMinutes == 0) {
      timeoutMinutes = defaultTimeoutMinutes;
    }
    if (onError.isEmpty) {
      onError = 'pause';
    }
    if (copy == 0) {
      copy = 1;
    }
  }
}

/// Port of `QueueSourceIdentity`.
class QueueSourceIdentity {
  const QueueSourceIdentity({
    required this.path,
    required this.canonicalPath,
    required this.identity,
  });

  factory QueueSourceIdentity.fromJson(Object? value) {
    final json = _objectValue(value, 'invalid queue source identity');
    return QueueSourceIdentity(
      path: _reqStr(json, 'path'),
      canonicalPath: _reqStr(json, 'canonicalPath'),
      identity: FilesystemObjectIdentity.fromJson(json['identity']),
    );
  }

  final String path;
  final String canonicalPath;
  final FilesystemObjectIdentity identity;

  Map<String, Object?> toJson() => {
    'path': path,
    'canonicalPath': canonicalPath,
    'identity': identity.toJson(),
  };

  @override
  bool operator ==(Object other) =>
      other is QueueSourceIdentity &&
      other.path == path &&
      other.canonicalPath == canonicalPath &&
      other.identity == identity;

  @override
  int get hashCode => Object.hash(path, canonicalPath, identity);
}

/// Port of `QueueState`: the durable print queue document.
class QueueState {
  QueueState({
    required this.version,
    required this.queueId,
    this.ownerId = '',
    this.sessionId = '',
    required this.createdAt,
    required this.updatedAt,
    required this.status,
    this.currentJobId,
    required this.jobs,
    List<GrantRootIdentity>? grantRootIdentities,
    List<QueueSourceIdentity>? sourceIdentities,
    Map<String, Object?>? extra,
  }) : grantRootIdentities = grantRootIdentities ?? <GrantRootIdentity>[],
       sourceIdentities = sourceIdentities ?? <QueueSourceIdentity>[],
       extra = extra ?? <String, Object?>{};

  static const Set<String> _knownKeys = {
    'version',
    'queueId',
    'ownerId',
    'sessionId',
    'createdAt',
    'updatedAt',
    'status',
    'currentJobId',
    'jobs',
    'grantRootIdentities',
    'sourceIdentities',
  };

  int version;
  String queueId;
  String ownerId;
  String sessionId;
  String createdAt;
  String updatedAt;
  String status;
  String? currentJobId;
  List<PrintJob> jobs;
  List<GrantRootIdentity> grantRootIdentities;
  List<QueueSourceIdentity> sourceIdentities;
  final Map<String, Object?> extra;

  factory QueueState.fromJson(Object? value) {
    final json = _objectValue(value, 'invalid saved print queue');
    final rawJobs = json['jobs'];
    if (rawJobs is! List) {
      throw const FormatException('print queue jobs must be an array');
    }
    final rawGrants = json['grantRootIdentities'];
    final rawSources = json['sourceIdentities'];
    final extra = <String, Object?>{
      for (final entry in json.entries)
        if (!_knownKeys.contains(entry.key)) entry.key: entry.value,
    };
    return QueueState(
      version: _requiredU32(json, 'version'),
      queueId: _reqStr(json, 'queueId'),
      ownerId: _strOr(json, 'ownerId', ''),
      sessionId: _strOr(json, 'sessionId', ''),
      createdAt: _reqStr(json, 'createdAt'),
      updatedAt: _reqStr(json, 'updatedAt'),
      status: _reqStr(json, 'status'),
      currentJobId: _optStrField(json, 'currentJobId'),
      jobs: [for (final job in rawJobs) PrintJob.fromJson(job)],
      grantRootIdentities: rawGrants == null
          ? null
          : [
              for (final item in rawGrants as List)
                GrantRootIdentity.fromJson(_objectValue(item, 'invalid grant root')),
            ],
      sourceIdentities: rawSources == null
          ? null
          : [
              for (final item in rawSources as List)
                QueueSourceIdentity.fromJson(item),
            ],
      extra: extra,
    );
  }

  Map<String, Object?> toJson() => {
    ...extra,
    'version': version,
    'queueId': queueId,
    'ownerId': ownerId,
    'sessionId': sessionId,
    'createdAt': createdAt,
    'updatedAt': updatedAt,
    'status': status,
    'currentJobId': currentJobId,
    'jobs': [for (final job in jobs) job.toJson()],
    if (grantRootIdentities.isNotEmpty)
      'grantRootIdentities': [
        for (final root in grantRootIdentities) root.toJson(),
      ],
    if (sourceIdentities.isNotEmpty)
      'sourceIdentities': [
        for (final source in sourceIdentities) source.toJson(),
      ],
  };

  /// Deep copy (Rust `Clone`).
  QueueState clone() => QueueState(
    version: version,
    queueId: queueId,
    ownerId: ownerId,
    sessionId: sessionId,
    createdAt: createdAt,
    updatedAt: updatedAt,
    status: status,
    currentJobId: currentJobId,
    jobs: [for (final job in jobs) job.clone()],
    grantRootIdentities: List.of(grantRootIdentities),
    sourceIdentities: List.of(sourceIdentities),
    extra: _copyJsonMap(extra),
  );

  void authorizeOwner(CallerIdentity caller) {
    if (ownerId.isEmpty || ownerId != caller.ownerId) {
      throw BackendError.planUnavailable();
    }
  }

  void authorizeSession(CallerIdentity caller) {
    authorizeOwner(caller);
    if (sessionId.isEmpty || sessionId != caller.sessionId) {
      throw BackendError.planUnavailable();
    }
  }

  /// Port of `recover_interrupted`: an active job cannot be assumed unsent.
  bool recoverInterrupted() {
    var changed = false;
    for (final job in jobs) {
      if (job.status == 'printing') {
        job.status = 'uncertain';
        job.error = 'application closed while this job was active';
        changed = true;
      }
    }
    if (status == 'running') {
      status = 'paused';
      changed = true;
    }
    if (changed) {
      currentJobId = null;
    }
    return changed;
  }

  PrintQueueSummary summary() {
    final counts = queueCounts(jobs);
    var resumable = 0;
    for (final status in const ['pending', 'failed', 'cancelled', 'uncertain']) {
      resumable += counts[status] ?? 0;
    }
    final files = <String>{
      for (final job in jobs)
        if (job.path.isNotEmpty) job.path,
    };
    return PrintQueueSummary(
      queueId: queueId,
      status: status,
      createdAt: createdAt,
      updatedAt: updatedAt,
      counts: counts,
      resumable: resumable,
      fileCount: files.length,
    );
  }
}

// ---- Diagnostics -----------------------------------------------------------

/// Port of `diagnostics::PrintJobDiagnostic` (pathless job summary).
class PrintJobDiagnostic {
  const PrintJobDiagnostic({
    required this.format,
    required this.route,
    required this.status,
    required this.attempts,
    required this.paper,
    required this.orientation,
    required this.scaling,
    required this.color,
    required this.duplex,
    required this.pageRange,
    required this.errorDetail,
  });

  final String format;
  final String route;
  final String status;
  final int attempts;
  final String paper;
  final String orientation;
  final String scaling;
  final String color;
  final String duplex;
  final String pageRange;
  final String errorDetail;

  Map<String, Object?> toJson() => {
    'format': format,
    'route': route,
    'status': status,
    'attempts': attempts,
    'paper': paper,
    'orientation': orientation,
    'scaling': scaling,
    'color': color,
    'duplex': duplex,
    'pageRange': pageRange,
    'errorDetail': errorDetail,
  };
}

/// Port of `diagnostics::PrintQueueDiagnostic`.
class PrintQueueDiagnostic {
  const PrintQueueDiagnostic({
    required this.queueStatus,
    required this.printer,
    required this.fileCount,
    required this.counts,
    required this.jobs,
  });

  final String queueStatus;
  final String printer;
  final int fileCount;
  final Map<String, int> counts;
  final List<PrintJobDiagnostic> jobs;

  Map<String, Object?> toJson() => {
    'queueStatus': queueStatus,
    'printer': printer,
    'fileCount': fileCount,
    'counts': counts,
    'jobs': [for (final job in jobs) job.toJson()],
  };
}

/// Port of `diagnostics::PrinterDiagnostic`.
class PrinterDiagnostic {
  const PrinterDiagnostic({
    this.inventoryStatus = '',
    this.printerCount = 0,
    this.defaultPrinterAvailable = false,
    this.latestQueue,
  });

  final String inventoryStatus;
  final int printerCount;
  final bool defaultPrinterAvailable;
  final PrintQueueDiagnostic? latestQueue;

  Map<String, Object?> toJson() => {
    'inventoryStatus': inventoryStatus,
    'printerCount': printerCount,
    'defaultPrinterAvailable': defaultPrinterAvailable,
    'latestQueue': latestQueue?.toJson(),
  };
}

// ---- Queue store -----------------------------------------------------------

/// Port of `PrintQueueStore`: one JSON document per queue under `root`,
/// with an in-memory mirror used when the file is absent.
class PrintQueueStore {
  PrintQueueStore(this.root);

  final String root;
  final Map<String, QueueState> memory = {};
  final Set<String> _active = {};

  /// Test hooks: fail the N-th save (1-based); 0 disables.
  int saveFailOn = 0;
  int saveCount = 0;

  void begin(String queueId) {
    if (!_active.add(queueId)) {
      throw BackendError.operationInProgress();
    }
  }

  void finish(String queueId) {
    _active.remove(queueId);
  }

  bool isActive(String queueId) => _active.contains(queueId);

  void save(QueueState state) {
    state.updatedAt = _utcNow();
    _validateQueueId(state.queueId);
    final String encoded;
    try {
      encoded = jsonEncode(state.toJson());
    } catch (_) {
      throw BackendError.journalInvalid('print queue could not be encoded');
    }
    if (encoded.length > _maxQueueBytes) {
      throw BackendError.validation('print queue exceeds the size limit');
    }
    saveCount += 1;
    if (saveFailOn != 0 && saveCount == saveFailOn) {
      throw BackendError.requestFailed('injected print queue save failure');
    }
    try {
      Directory(root).createSync(recursive: true);
    } catch (_) {
      throw BackendError.requestFailed('print queue directory is unavailable');
    }
    final target = path(state.queueId);
    final temporary = p.join(
      root,
      '.${state.queueId}.json.${_uuid.v4().replaceAll('-', '')}.tmp',
    );
    try {
      final file = File(temporary).openSync(mode: FileMode.writeOnly);
      try {
        file.writeFromSync(utf8.encode(encoded));
        file.flushSync();
      } finally {
        file.closeSync();
      }
      File(temporary).renameSync(target);
    } catch (_) {
      try {
        final leftover = File(temporary);
        if (leftover.existsSync()) {
          leftover.deleteSync();
        }
      } catch (_) {
        // Best effort: the temporary file is never trusted.
      }
      throw BackendError.requestFailed('print queue could not be saved');
    }
    memory[state.queueId] = state.clone();
  }

  QueueState load(String queueId) {
    final target = path(queueId);
    final QueueState decoded;
    final file = File(target);
    try {
      if (file.existsSync()) {
        final length = file.lengthSync();
        if (length > _maxQueueBytes) {
          throw BackendError.validation('invalid saved print queue');
        }
        decoded = _decodeQueue(file.readAsStringSync());
      } else {
        final cached = memory[queueId];
        if (cached == null) {
          throw BackendError.validation('invalid saved print queue');
        }
        decoded = cached.clone();
      }
    } on BackendError {
      rethrow;
    } catch (_) {
      throw BackendError.validation('invalid saved print queue');
    }
    if (decoded.queueId != queueId || decoded.jobs.length > _maxSavedJobs) {
      throw BackendError.validation('invalid saved print queue');
    }
    return decoded;
  }

  QueueState adopt(CallerIdentity caller, QueueState state) {
    state.authorizeOwner(caller);
    if (state.sessionId != caller.sessionId) {
      state.sessionId = caller.sessionId;
      save(state);
    }
    state.authorizeSession(caller);
    return state;
  }

  QueueState loadAdopted(CallerIdentity caller, String queueId) =>
      adopt(caller, load(queueId));

  String path(String queueId) {
    _validateQueueId(queueId);
    return p.join(root, '$queueId.json');
  }

  /// Port of `PrintQueueStore::latest`.
  PrintQueueSummary? latest(CallerIdentity caller) {
    final candidates = _candidateFiles();
    final seen = <String>{};
    for (final candidate in candidates) {
      final queueId = p.basenameWithoutExtension(candidate.path);
      seen.add(queueId);
      if (isActive(queueId)) {
        continue;
      }
      final QueueState state;
      try {
        state = load(queueId);
      } catch (_) {
        continue;
      }
      try {
        state.authorizeOwner(caller);
      } catch (_) {
        continue;
      }
      state.recoverInterrupted();
      final summary = state.summary();
      if (summary.resumable > 0) {
        return summary;
      }
    }
    final remembered = memory.values
        .where((state) => !seen.contains(state.queueId))
        .map((state) => state.clone())
        .toList()
      ..sort((left, right) => right.updatedAt.compareTo(left.updatedAt));
    for (final state in remembered) {
      if (isActive(state.queueId)) {
        continue;
      }
      try {
        state.authorizeOwner(caller);
      } catch (_) {
        continue;
      }
      state.recoverInterrupted();
      final summary = state.summary();
      if (summary.resumable > 0) {
        return summary;
      }
    }
    return null;
  }

  /// Port of `latest_owned_state`.
  QueueState? latestOwnedState(CallerIdentity caller) {
    final candidates = _candidateFiles();
    final seen = <String>{};
    for (final candidate in candidates) {
      final queueId = p.basenameWithoutExtension(candidate.path);
      seen.add(queueId);
      final QueueState state;
      try {
        state = load(queueId);
      } catch (_) {
        continue;
      }
      try {
        state.authorizeOwner(caller);
        return state;
      } catch (_) {
        continue;
      }
    }
    final remembered = memory.values
        .where((state) => !seen.contains(state.queueId))
        .map((state) => state.clone())
        .toList()
      ..sort((left, right) => right.updatedAt.compareTo(left.updatedAt));
    for (final state in remembered) {
      try {
        state.authorizeOwner(caller);
        return state;
      } catch (_) {
        continue;
      }
    }
    return null;
  }

  /// Port of `PrintQueueStore::discard`.
  PrintQueueSummary discard(CallerIdentity caller, String queueId) {
    if (isActive(queueId)) {
      throw BackendError.operationInProgress();
    }
    final state = loadAdopted(caller, queueId);
    state.recoverInterrupted();
    for (final job in state.jobs) {
      if (const {'pending', 'failed', 'cancelled', 'uncertain'}
          .contains(job.status)) {
        job.status = 'abandoned';
      }
    }
    state.status = 'abandoned';
    state.currentJobId = null;
    save(state);
    return state.summary();
  }

  /// Regular `.json` files in the queue root, newest first. Stable for
  /// equal timestamps, matching the Rust `sort_by_key(Reverse(modified))`.
  List<_QueueCandidate> _candidateFiles() {
    final List<_QueueCandidate> out = [];
    final dir = Directory(root);
    if (!dir.existsSync()) {
      return out;
    }
    try {
      for (final entity in dir.listSync(followLinks: false)) {
        if (entity is! File || p.extension(entity.path) != '.json') {
          continue;
        }
        final FileStat stat;
        try {
          stat = entity.statSync();
        } catch (_) {
          continue;
        }
        out.add(_QueueCandidate(entity.path, stat.modified));
      }
    } catch (_) {
      return <_QueueCandidate>[];
    }
    final indexed = [for (var i = 0; i < out.length; i++) (i, out[i])];
    indexed.sort((a, b) {
      final byTime = b.$2.modified.compareTo(a.$2.modified);
      return byTime != 0 ? byTime : a.$1.compareTo(b.$1);
    });
    return [for (final entry in indexed) entry.$2];
  }
}

class _QueueCandidate {
  const _QueueCandidate(this.path, this.modified);

  final String path;
  final DateTime modified;
}

QueueState _decodeQueue(String text) {
  final Object? decoded;
  try {
    decoded = _decodeJson(text);
  } catch (_) {
    throw BackendError.validation('invalid saved print queue');
  }
  try {
    return QueueState.fromJson(decoded);
  } catch (_) {
    throw BackendError.validation('invalid saved print queue');
  }
}

// ---- Service ---------------------------------------------------------------

/// Port of `PrintService`: preview, revise, resume, and apply for batch print
/// plans, bound to path grants and the journal.
class PrintService {
  PrintService({
    required String queueRoot,
    required String journalRoot,
    required Duration ttl,
    required this.mutationLock,
    required this.adapter,
    this.excelPipeline,
  }) : queues = PrintQueueStore(queueRoot),
       journals = JournalStore(journalRoot),
       plans = PlanStore(ttl),
       grants = PathGrantStore(ttl: ttl),
       cancellations = CancellationRegistry();

  final PrintQueueStore queues;
  final JournalStore journals;
  final PlanStore plans;
  final PathGrantStore grants;
  final CancellationRegistry cancellations;
  final GlobalMutationLock mutationLock;
  final PrintAdapter adapter;
  final ExcelExportPipelinePort? excelPipeline;

  /// Test hook: fail the N-th journal write (1-based); 0 disables.
  int journalWriteFailOn = 0;
  int journalWriteCount = 0;

  void _writeJournal(JournalRecord record) {
    journalWriteCount += 1;
    if (journalWriteFailOn != 0 && journalWriteCount == journalWriteFailOn) {
      throw BackendError.journalInvalid('injected print journal write failure');
    }
    journals.write(record);
  }

  /// Port of `persist_cleanup_review`: both evidence writes are attempted,
  /// and the failure carries the cleanup evidence locator.
  void _persistCleanupReview(
    QueueState state,
    JournalRecord record,
    String primaryClassification,
    CleanupReview review,
  ) {
    BackendError? journalError;
    BackendError? queueError;
    try {
      _writeJournal(record);
    } on BackendError catch (error) {
      journalError = error;
    }
    try {
      queues.save(state);
    } on BackendError catch (error) {
      queueError = error;
    }
    if (journalError == null && queueError == null) {
      return;
    }
    BackendError error;
    String? secondary;
    if (journalError != null && queueError != null) {
      error = queueError;
      secondary = 'cleanup evidence journal also failed: ${journalError.message}';
    } else {
      error = (queueError ?? journalError)!;
    }
    var message = error.message;
    if (secondary != null) {
      message = '$message; $secondary';
    }
    message =
        '$message [cleanupEvidence: primaryClassification=$primaryClassification; '
        'warningKey=${review.warningKey}; locator=${review.locator}]';
    throw BackendError(error.code, message);
  }

  PrinterInventory inventory() {
    try {
      return adapter.inventory().normalized();
    } on PrintFailure catch (failure) {
      throw failure.intoBackendError();
    }
  }

  /// Port of `PrintService::diagnostics`.
  PrinterDiagnostic diagnostics(CallerIdentity? caller) {
    PrinterInventory? inventoryValue;
    try {
      inventoryValue = adapter.inventory().normalized();
    } on PrintFailure {
      inventoryValue = null;
    }
    PrintQueueDiagnostic? latestQueue;
    if (caller != null) {
      final state = queues.latestOwnedState(caller);
      if (state != null) {
        latestQueue = queueDiagnostic(state);
      }
    }
    if (inventoryValue == null) {
      return PrinterDiagnostic(
        inventoryStatus: 'unavailable',
        latestQueue: latestQueue,
      );
    }
    return PrinterDiagnostic(
      inventoryStatus: 'available',
      printerCount: inventoryValue.printers.length,
      defaultPrinterAvailable: inventoryValue.defaultPrinter != null,
      latestQueue: latestQueue,
    );
  }

  /// Port of `PrintService::preview`.
  Plan preview(
    CallerIdentity caller,
    BatchPrintParams params,
    void Function(ProgressEvent event) progress,
  ) {
    final inventoryValue = inventory();
    final binding = _issueFileBinding(caller, params.files);
    final workbookFiles = <String, cap.OsHandle>{};
    for (final path in params.files) {
      final extension = p.extension(path).toLowerCase();
      if (extension != '.xlsx' && extension != '.xlsm') {
        continue;
      }
      try {
        final canonical = grants.authorizeForPlan(
          binding.grantIds,
          caller,
          binding.operationId,
          path,
          PathIntent.read,
        );
        final authorized = grants.authorizeCapabilityForPlan(
          binding.grantIds,
          caller,
          binding.operationId,
          canonical,
          PathIntent.read,
        );
        final handle = authorized.anchor().openRead();
        workbookFiles.putIfAbsent(path, () => handle);
      } catch (_) {
        // Unreadable workbooks surface later as a per-file warning.
      }
    }
    progress(
      ProgressEvent(
        phase: 'printer_check',
        done: 1,
        total: 1,
        subject: params.printer,
      ),
    );
    final plan = batch_print.previewWithWorkbookOpener(
      params,
      inventoryValue,
      adapter,
      progress,
      (path) {
        final handle = workbookFiles.remove(path);
        return handle == null ? null : _HandlePrintFile(handle);
      },
    );
    return plans.insertBound(BackendOrigin.rust, binding, plan);
  }

  /// Port of `PrintService::revise`.
  Plan revise(CallerIdentity caller, String planId, List<Object?> rows) {
    final claimed = _resolve(planId, caller);
    final binding = claimed.binding ?? (throw BackendError.planUnavailable());
    final inventoryValue = inventory();
    final revised = batch_print.revise(
      claimed.plan,
      rows,
      inventoryValue,
      adapter,
    );
    _revalidateWorksheetPlan(binding, caller, revised);
    return plans.insertBound(BackendOrigin.rust, binding, revised);
  }

  /// Port of `PrintService::resume`.
  Plan resume(CallerIdentity caller, String queueId) {
    queues.begin(queueId);
    try {
      final state = queues.load(queueId);
      state.authorizeOwner(caller);
      for (var index = 0; index < state.jobs.length; index++) {
        final job = state.jobs[index];
        job.normalizeLegacy(index);
        batch_print.validateJob(job);
      }
      state.recoverInterrupted();
      final inventoryValue = inventory();
      final plan = batch_print.resumePlan(state.toJson(), inventoryValue);
      final prepared = _prepareResumeFileBinding(caller, state, plan);
      final binding = prepared.$1;
      final provisional = prepared.$2;
      try {
        state.sessionId = caller.sessionId;
        state.authorizeSession(caller);
        queues.save(state);
        final cached = plans.insertBound(BackendOrigin.rust, binding, plan);
        provisional.commit();
        return cached;
      } finally {
        provisional.revokeUnlessCommitted();
      }
    } finally {
      queues.finish(queueId);
    }
  }

  PrintQueueSummary? latest(CallerIdentity caller) => queues.latest(caller);

  PrintQueueSummary discard(CallerIdentity caller, String queueId) {
    final permit = mutationLock.tryAcquire();
    try {
      return queues.discard(caller, queueId);
    } finally {
      permit.release();
    }
  }

  bool containsPlan(String planId) {
    final snapshot = plans.snapshot(planId);
    return snapshot != null && snapshot.origin == BackendOrigin.rust;
  }

  int get activeGrantCountForTesting => grants.activeGrantCountForTesting;

  int get activePlanCountForTesting => plans.activePlanCountForTesting;

  bool cancel(String planId) => cancellations.cancel(planId);

  void revoke(CallerIdentity caller) {
    plans.revoke(caller);
    grants.revoke(caller);
    cancellations.revokeAll();
    queues.memory.removeWhere(
      (_, state) =>
          state.ownerId != caller.ownerId || state.sessionId != caller.sessionId,
    );
  }

  /// Port of `PrintService::apply`: executes one approved, cached plan.
  OperationResult apply(
    String planId,
    CallerIdentity caller,
    void Function(ProgressEvent event) progress,
  ) {
    final claim = plans.claimForApplyBound(planId, caller, mutationLock);
    try {
      return _applyClaim(claim, planId, caller, progress);
    } finally {
      claim.release();
    }
  }

  OperationResult _applyClaim(
    ApplyClaim claim,
    String planId,
    CallerIdentity caller,
    void Function(ProgressEvent event) progress,
  ) {
    if (claim.claimed.origin != BackendOrigin.rust ||
        claim.claimed.plan.opId != 'files.batch_print') {
      throw BackendError.validation('plan is not owned by the Rust print backend');
    }
    final binding = claim.claimed.binding ?? (throw BackendError.planUnavailable());
    _revalidateWorksheetPlan(binding, caller, claim.claimed.plan);
    final roots = grants.rootsForPlan(
      binding.grantIds,
      caller,
      binding.operationId,
    );
    final rootIdentities = grants.rootIdentitiesForPlan(
      binding.grantIds,
      caller,
      binding.operationId,
    );
    final token = cancellations.register(planId);
    try {
      final rawJobs = claim.claimed.plan.meta['jobs'];
      if (rawJobs == null) {
        throw BackendError.validation('print plan has no jobs');
      }
      final List<PrintJob> jobs;
      try {
        jobs = [
          for (final job in rawJobs as List) PrintJob.fromJson(job),
        ];
      } on Object {
        throw BackendError.validation('print plan contains invalid jobs');
      }
      for (var index = 0; index < jobs.length; index++) {
        jobs[index].normalizeLegacy(index);
        batch_print.validateJob(jobs[index]);
      }
      final identities = _sourceIdentitiesForPlan(
        binding,
        caller,
        jobs,
        rootIdentities,
      );
      final state = _stateForPlan(
        claim.claimed.plan,
        jobs,
        caller,
        identities.$1,
        identities.$2,
      );
      queues.begin(state.queueId);
      try {
        return _runApply(
          state,
          jobs,
          claim,
          binding,
          caller,
          roots,
          rootIdentities,
          token,
          progress,
        );
      } finally {
        queues.finish(state.queueId);
      }
    } finally {
      cancellations.finish(planId);
    }
  }

  OperationResult _runApply(
    QueueState state,
    List<PrintJob> jobs,
    ApplyClaim claim,
    PlanBinding binding,
    CallerIdentity caller,
    List<String> roots,
    List<GrantRootIdentity> rootIdentities,
    CancellationToken token,
    void Function(ProgressEvent event) progress,
  ) {
    state.status = 'running';
    queues.save(state);
    final record = JournalRecord.fresh(
      opId: 'files.batch_print',
      ownerId: binding.ownerId,
      sessionId: binding.sessionId,
      planDigest: claim.claimed.digest,
    );
    record.grantRoots = [...roots];
    record.grantRootIdentities = [...rootIdentities];
    record.state = JournalState.applying;
    for (final saved in state.jobs) {
      final evidence = saved.cleanupReview;
      if (evidence != null) {
        for (final review in evidence.entries) {
          record.cleanupReviews.add(
            JournalCleanupReview(
              jobId: saved.jobId,
              warningKey: review.warningKey,
              locator: review.locator,
            ),
          );
        }
      }
    }
    _writeJournal(record);

    final errors = <Warning>[];
    var applied = 0;
    var failed = 0;
    var paused = false;
    JournalState? cancelledState;
    progress(
      ProgressEvent(phase: 'apply', done: 0, total: jobs.length, subject: ''),
    );
    for (var index = 0; index < jobs.length; index++) {
      final job = jobs[index];
      if (token.isCancelled) {
        paused = true;
        cancelledState = record.steps.isEmpty
            ? JournalState.cancelledClean
            : JournalState.cancelledPartial;
        errors.add(_warning('warn.print.cancelled', ''));
        break;
      }
      progress(
        ProgressEvent(
          phase: 'print_waiting',
          done: index,
          total: jobs.length,
          subject: _displayName(job.path),
        ),
      );
      final savedIndex = state.jobs.indexWhere(
        (saved) => saved.jobId == job.jobId,
      );
      if (savedIndex < 0) {
        throw BackendError.validation('saved print job no longer exists');
      }
      final attemptsAllowed = 1 + job.retries;
      var completed = false;
      for (var attempt = 0; attempt < attemptsAllowed; attempt++) {
        final saved = state.jobs[savedIndex];
        saved.status = 'printing';
        saved.attempts = (saved.attempts ?? 0) + 1;
        saved.error = '';
        state.currentJobId = saved.jobId;
        queues.save(state);
        final pending = _journalStep(job);
        record.pendingStep = pending;
        record.state = JournalState.applying;
        _writeJournal(record);

        PrintSubmissionOutcome? outcome;
        PrintFailure? failure;
        try {
          outcome = _submitApprovedJob(job, binding, caller, token);
        } on PrintFailure catch (error) {
          failure = error;
        }
        final review = outcome?.cleanupReview ?? failure?.cleanupReview;
        if (review != null) {
          final primary = _submissionPrimaryClassification(outcome, failure);
          errors.add(_warning(review.warningKey, review.locator));
          final existing = saved.cleanupReview;
          bool appended;
          if (existing != null) {
            try {
              appended = existing.append(review.warningKey, review.locator);
            } on StateError {
              appended = true;
            }
          } else {
            saved.cleanupReview = CleanupReviewEvidence.single(
              review.warningKey,
              review.locator,
            );
            appended = true;
          }
          if (appended) {
            record.cleanupReviews.add(
              JournalCleanupReview(
                jobId: job.jobId,
                warningKey: review.warningKey,
                locator: review.locator,
              ),
            );
          }
          _persistCleanupReview(state, record, primary, review);
        }

        if (outcome != null) {
          saved.status = 'completed';
          saved.completedAt = _utcNow();
          saved.error = '';
          state.currentJobId = null;
          record.steps.add(pending);
          record.pendingStep = null;
          _writeJournal(record);
          queues.save(state);
          applied += 1;
          completed = true;
          break;
        }

        final error = failure!;
        if (error.kind == PrintFailureKind.cancelled) {
          saved.status = error.uncertain ? 'uncertain' : 'cancelled';
          saved.error = _failureEvidence(error);
          state.currentJobId = null;
          state.status = 'paused';
          record.pendingStep = error.uncertain ? pending : null;
          record.state = error.uncertain
              ? JournalState.recoveryRequired
              : (record.steps.isEmpty
                    ? JournalState.cancelledClean
                    : JournalState.cancelledPartial);
          if (!error.uncertain) {
            cancelledState = record.state;
          }
          _writeJournal(record);
          queues.save(state);
          errors.add(
            _warning(
              error.uncertain
                  ? 'warn.print.cancelledUncertain'
                  : 'warn.print.cancelled',
              _displayName(job.path),
            ),
          );
          paused = true;
          break;
        }
        if (error.kind == PrintFailureKind.uncertain) {
          saved.status = 'uncertain';
          saved.error = _failureEvidence(error);
          state.currentJobId = null;
          state.status = 'paused';
          record.pendingStep = pending;
          record.state = JournalState.recoveryRequired;
          _writeJournal(record);
          queues.save(state);
          errors.add(
            _warning('warn.print.stateUncertain', _displayName(job.path)),
          );
          paused = true;
          break;
        }
        if (error.kind == PrintFailureKind.submittedUnconfirmed) {
          saved.status = 'completed_unconfirmed';
          saved.completedAt = _utcNow();
          saved.error = _failureEvidence(error);
          state.currentJobId = null;
          pending.meta['outcome'] = 'submitted_unconfirmed';
          record.steps.add(pending);
          record.pendingStep = null;
          _writeJournal(record);
          queues.save(state);
          applied += 1;
          errors.add(
            _warning('warn.print.submittedUnconfirmed', _displayName(job.path)),
          );
          completed = true;
          break;
        }

        // Pre-effect failure: the only path that may retry.
        saved.error = _failureEvidence(error);
        state.currentJobId = null;
        record.pendingStep = null;
        _writeJournal(record);
        if ((saved.attempts ?? 0) < attemptsAllowed &&
            _retryDispositionAllowsRetry(error) &&
            !token.isCancelled) {
          saved.status = 'pending';
          queues.save(state);
          continue;
        }
        saved.status = 'failed';
        queues.save(state);
        failed += 1;
        final redacted = _redactError(error.message);
        errors.add(
          _isLocalizedPrintError(redacted)
              ? _warning(redacted, _displayName(job.path))
              : _warning(
                  'error.print.failed',
                  '${_displayName(job.path)}: $redacted',
                ),
        );
        if (job.onError == 'pause') {
          errors.add(
            _warning('error.print.queuePaused', '${jobs.length - index - 1}'),
          );
          paused = true;
        }
        break;
      }
      if (paused) {
        break;
      }
      if (!completed && job.onError != 'skip') {
        break;
      }
    }

    state.currentJobId = null;
    final counts = queueCounts(state.jobs);
    var resumable = 0;
    for (final status in const ['pending', 'failed', 'cancelled', 'uncertain']) {
      resumable += counts[status] ?? 0;
    }
    if (resumable == 0) {
      state.status = 'completed';
    } else if (paused || (counts['pending'] ?? 0) > 0) {
      state.status = 'paused';
    } else {
      state.status = 'completed_with_errors';
    }
    queues.save(state);
    if (record.state != JournalState.recoveryRequired) {
      record.pendingStep = null;
      record.state = cancelledState ??
          (failed == 0 && resumable == 0
              ? JournalState.committed
              : (record.steps.isNotEmpty
                    ? JournalState.partialCommitted
                    : JournalState.noEffectRetryable));
      _writeJournal(record);
    }
    final summary = state.summary();
    final meta = Map<String, Object?>.from(summary.toJson());
    meta['resumeAvailable'] = resumable > 0;
    meta['nonUndoable'] = true;
    meta['auditJournalId'] = record.journalId;
    meta['effectState'] = _journalEffectState(record.state);
    progress(
      ProgressEvent(
        phase: 'apply',
        done: jobs.length - (counts['pending'] ?? 0),
        total: jobs.length,
        subject: '',
      ),
    );
    return OperationResult(
      applied: applied,
      failed: failed,
      journalId: '',
      errors: errors,
      meta: meta,
    );
  }

  ClaimedPlan _resolve(String planId, CallerIdentity caller) {
    final claimed = plans.resolveBound(planId, caller);
    if (claimed.origin != BackendOrigin.rust ||
        claimed.plan.opId != 'files.batch_print') {
      throw BackendError.validation('plan is not owned by the Rust print backend');
    }
    return claimed;
  }

  (PlanBinding, ProvisionalPathGrants) _prepareResumeFileBinding(
    CallerIdentity caller,
    QueueState state,
    Plan plan,
  ) {
    final issued = _issueResumeFileBinding(caller, state);
    final binding = issued.$1;
    final provisional = issued.$2;
    try {
      _validateVerifiedResumePlanSources(plan, provisional.verifiedSources);
      _revalidateWorksheetPlan(binding, caller, plan);
    } catch (_) {
      provisional.revokeUnlessCommitted();
      rethrow;
    }
    return (binding, provisional);
  }

  (PlanBinding, ProvisionalPathGrants) _issueResumeFileBinding(
    CallerIdentity caller,
    QueueState state,
  ) {
    final queuePaths = {for (final job in state.jobs) job.path};
    final sourcePaths = {for (final source in state.sourceIdentities) source.path};
    if (queuePaths.isEmpty ||
        !_setEquals(queuePaths, sourcePaths) ||
        sourcePaths.length != state.sourceIdentities.length) {
      throw _persistedSourceReselectionRequired();
    }
    final canonicalParents = <String>{};
    for (final source in state.sourceIdentities) {
      final parent = _rustParent(source.canonicalPath);
      if (parent == null) {
        throw _persistedSourceReselectionRequired();
      }
      canonicalParents.add(parent);
    }
    final persistedRoots = {for (final root in state.grantRootIdentities) root.root};
    if (!_setEquals(canonicalParents, persistedRoots) ||
        persistedRoots.length != state.grantRootIdentities.length) {
      throw _persistedSourceReselectionRequired();
    }
    final scope = GrantScope.create(
      caller.ownerId,
      caller.sessionId,
      _uuid.v4(),
      'files.batch_print',
    );
    final expectedSources = [
      for (final source in state.sourceIdentities)
        (source.path, source.canonicalPath, source.identity),
    ];
    final provisional = grants.issuePersistedReadCapabilities(
      scope,
      state.grantRootIdentities,
      expectedSources,
    );
    try {
      final grantIds = [for (final grant in provisional.grants) grant.grantId];
      return (
        PlanBinding(
          ownerId: caller.ownerId,
          sessionId: caller.sessionId,
          operationId: 'files.batch_print',
          grantIds: grantIds,
        ),
        provisional,
      );
    } catch (_) {
      provisional.revokeUnlessCommitted();
      rethrow;
    }
  }

  PlanBinding _issueFileBinding(CallerIdentity caller, List<String> candidatePaths) {
    final scope = GrantScope.create(
      caller.ownerId,
      caller.sessionId,
      _uuid.v4(),
      'files.batch_print',
    );
    final parents = <String>[];
    final seen = <String>{};
    for (final path in candidatePaths) {
      final parent = _rustParent(path);
      if (parent == null) {
        throw BackendError.pathInvalid('print file has no parent directory');
      }
      final canonical = _canonicalizeOrNull(parent);
      if (canonical == null) {
        throw BackendError.pathInvalid('print file directory is unavailable');
      }
      if (seen.add(canonical)) {
        parents.add(canonical);
      }
    }
    final grantIds = [
      for (final parent in parents)
        grants.issue(scope, parent, GrantAccess.read).grantId,
    ];
    return PlanBinding(
      ownerId: caller.ownerId,
      sessionId: caller.sessionId,
      operationId: 'files.batch_print',
      grantIds: grantIds,
    );
  }

  QueueState _stateForPlan(
    Plan plan,
    List<PrintJob> jobs,
    CallerIdentity caller,
    List<GrantRootIdentity> grantRootIdentities,
    List<QueueSourceIdentity> sourceIdentities,
  ) {
    final rawQueueId = plan.meta['queueId'];
    final queueId = rawQueueId is String && rawQueueId.isNotEmpty
        ? rawQueueId
        : _uuid.v4();
    final rawResume = plan.meta['resumeQueueId'];
    final resumeQueueId = rawResume is String ? rawResume : '';
    if (resumeQueueId.isNotEmpty) {
      final state = queues.load(resumeQueueId);
      state.authorizeSession(caller);
      final editor = plan.meta['editor'];
      final rows = (editor is Map ? editor['rows'] : null);
      if (rows is! List) {
        throw BackendError.validation('print plan has no rows');
      }
      final savedById = <String, PrintJob>{};
      for (final saved in state.jobs) {
        if (savedById.containsKey(saved.jobId)) {
          throw BackendError.validation('saved print job identity is duplicated');
        }
        savedById[saved.jobId] = saved;
      }
      final plannedById = <String, PrintJob>{};
      for (final planned in jobs) {
        if (plannedById.containsKey(planned.jobId)) {
          throw BackendError.validation('planned print job identity is duplicated');
        }
        plannedById[planned.jobId] = planned;
      }
      final reordered = <PrintJob>[];
      final displayed = <String>{};
      for (final row in rows) {
        final rowMap = row as Map;
        final rawJobId = rowMap['_resumeJobId'];
        if (rawJobId is! String || rawJobId.isEmpty) {
          throw BackendError.validation('saved print job identity is missing');
        }
        final jobId = rawJobId;
        if (!displayed.add(jobId)) {
          throw BackendError.validation('saved print job identity is duplicated');
        }
        final saved = savedById.remove(jobId);
        if (saved == null) {
          throw BackendError.validation('saved print job no longer exists');
        }
        final enabled = rowMap['enabled'] is bool ? rowMap['enabled'] as bool : true;
        if (enabled) {
          if (!const {'pending', 'failed', 'cancelled', 'uncertain'}
              .contains(saved.status)) {
            throw BackendError.validation('saved print job is not resumable');
          }
          final planned = plannedById.remove(jobId);
          if (planned == null) {
            throw BackendError.validation(
              'enabled saved print job is missing from the plan',
            );
          }
          final next = planned.clone();
          next.status = 'pending';
          next.attempts = saved.attempts;
          next.error = '';
          next.completedAt = null;
          next.cleanupReview = saved.cleanupReview == null
              ? null
              : CleanupReviewEvidence.fromJson(saved.cleanupReview!.toJson());
          reordered.add(next);
        } else {
          if (plannedById.containsKey(jobId)) {
            throw BackendError.validation(
              'disabled saved print job is present in the plan',
            );
          }
          reordered.add(saved);
        }
      }
      if (savedById.isNotEmpty || plannedById.isNotEmpty) {
        throw BackendError.validation('saved print job set changed');
      }
      state.jobs = reordered;
      state.status = 'ready';
      state.currentJobId = null;
      return state;
    }
    final created = _utcNow();
    return QueueState(
      version: 1,
      queueId: queueId,
      ownerId: caller.ownerId,
      sessionId: caller.sessionId,
      createdAt: created,
      updatedAt: created,
      status: 'ready',
      currentJobId: null,
      jobs: [
        for (final job in jobs)
          job.clone()
            ..status = 'pending'
            ..attempts = 0
            ..error = '',
      ],
      grantRootIdentities: List.of(grantRootIdentities),
      sourceIdentities: List.of(sourceIdentities),
    );
  }

  (List<GrantRootIdentity>, List<QueueSourceIdentity>) _sourceIdentitiesForPlan(
    PlanBinding binding,
    CallerIdentity caller,
    List<PrintJob> jobs,
    List<GrantRootIdentity> allRootIdentities,
  ) {
    final seen = <String>{};
    final sources = <QueueSourceIdentity>[];
    for (final job in jobs) {
      if (!seen.add(job.path)) {
        continue;
      }
      final canonicalPath = grants.authorizeForPlan(
        binding.grantIds,
        caller,
        binding.operationId,
        job.path,
        PathIntent.read,
      );
      final identity = grants
          .authorizeCapabilityForPlan(
            binding.grantIds,
            caller,
            binding.operationId,
            canonicalPath,
            PathIntent.read,
          )
          .anchor()
          .identity();
      sources.add(
        QueueSourceIdentity(
          path: job.path,
          canonicalPath: canonicalPath,
          identity: identity,
        ),
      );
    }
    final rootPaths = <String>{
      for (final source in sources)
        if (_rustParent(source.canonicalPath) != null)
          _rustParent(source.canonicalPath)!,
    };
    final roots = [
      for (final root in allRootIdentities)
        if (rootPaths.contains(root.root)) root,
    ];
    if (roots.length != rootPaths.length || sources.isEmpty) {
      throw BackendError.pathInvalid('print file capability identity is unavailable');
    }
    return (roots, sources);
  }

  void _revalidateWorksheetPlan(
    PlanBinding binding,
    CallerIdentity caller,
    Plan plan,
  ) {
    final editor = plan.meta['editor'];
    final rows = editor is Map ? editor['rows'] : null;
    if (rows is! List) {
      throw BackendError.validation('print plan has no rows');
    }
    final discoveries = <String, (FileSignature, OrderedWorkbookDiscovery)>{};
    for (final row in rows) {
      final rowMap = row as Map;
      if (!rowMap.containsKey('worksheet')) {
        continue;
      }
      final path = rowMap['path'];
      if (path is! String) {
        throw BackendError.validation('invalid worksheet print row');
      }
      final FileSignature expectedSignature;
      final WorksheetDescriptor expectedWorksheet;
      final WorkbookSafetyFacts expectedSafety;
      try {
        expectedSignature = FileSignature.fromJson(rowMap['signature']);
      } catch (_) {
        throw BackendError.validation('invalid worksheet file signature');
      }
      try {
        expectedWorksheet = worksheetDescriptorFromJson(rowMap['worksheet']);
      } catch (_) {
        throw BackendError.validation('invalid worksheet descriptor');
      }
      try {
        expectedSafety = workbookSafetyFromJson(rowMap['workbookSafety']);
      } catch (_) {
        throw BackendError.validation('invalid workbook safety facts');
      }
      if (!discoveries.containsKey(path)) {
        final canonicalPath = grants.authorizeForPlan(
          binding.grantIds,
          caller,
          binding.operationId,
          path,
          PathIntent.read,
        );
        final source = grants
            .authorizeCapabilityForPlan(
              binding.grantIds,
              caller,
              binding.operationId,
              canonicalPath,
              PathIntent.read,
            )
            .anchor();
        final cap.OsHandle sourceFile;
        try {
          sourceFile = source.openRead();
        } on BackendError {
          throw BackendError.pathInvalid('print file is unavailable');
        }
        final OpenedPrintFile opened = _HandlePrintFile(sourceFile);
        final actualSignature = fileSignatureFromOpenFile(
          opened,
          includeSha256: true,
        );
        if (actualSignature != expectedSignature) {
          throw BackendError.validation('error.print.workbookChanged');
        }
        final OrderedWorkbookDiscovery discovery;
        try {
          discovery = orderedWorkbookDiscovery(opened.readAllFromStart());
        } catch (_) {
          throw BackendError.validation('error.print.worksheetChanged');
        }
        discoveries[path] = (actualSignature, discovery);
      }
      final entry = discoveries[path];
      if (entry == null) {
        throw BackendError.validation('error.print.worksheetChanged');
      }
      final (actualSignature, discovery) = entry;
      if (actualSignature != expectedSignature) {
        throw BackendError.validation('error.print.workbookChanged');
      }
      if (discovery.safety != expectedSafety ||
          workbookSafetyIsUnsafe(discovery.safety) ||
          !discovery.worksheets.contains(expectedWorksheet)) {
        throw BackendError.validation('error.print.worksheetChanged');
      }
    }
  }

  PrintSubmissionOutcome _submitApprovedJob(
    PrintJob job,
    PlanBinding binding,
    CallerIdentity caller,
    CancellationToken cancellation,
  ) {
    final cap.AnchoredPath source;
    final cap.OsHandle sourceFile;
    try {
      final canonicalPath = grants.authorizeForPlan(
        binding.grantIds,
        caller,
        binding.operationId,
        job.path,
        PathIntent.read,
      );
      source = grants
          .authorizeCapabilityForPlan(
            binding.grantIds,
            caller,
            binding.operationId,
            canonicalPath,
            PathIntent.read,
          )
          .anchor();
    } on BackendError catch (error) {
      throw PrintFailure.failed(error.message);
    }
    try {
      sourceFile = source.openRead();
    } on Object {
      throw PrintFailure.failed('print file is unavailable');
    }
    final OpenedPrintFile opened = _HandlePrintFile(sourceFile);
    if (!opened.isFile) {
      throw PrintFailure.failed('print file is unavailable');
    }
    if (!job.signature.isLegacyEmpty) {
      final FileSignature actual;
      try {
        actual = fileSignatureFromOpenFile(
          opened,
          includeSha256: job.worksheet != null,
        );
      } on BackendError catch (error) {
        throw PrintFailure.failed(error.message);
      }
      if (actual != job.signature) {
        throw PrintFailure.failed(
          job.worksheet != null
              ? 'error.print.workbookChanged'
              : 'file changed after the queue was previewed',
        );
      }
    }
    final worksheet = job.worksheet;
    if (worksheet != null) {
      final OrderedWorkbookDiscovery discovery;
      try {
        discovery = orderedWorkbookDiscovery(opened.readAllFromStart());
      } catch (_) {
        throw PrintFailure.failed('error.print.worksheetChanged');
      }
      if (job.workbookSafety != discovery.safety ||
          workbookSafetyIsUnsafe(discovery.safety) ||
          !discovery.worksheets.contains(worksheet)) {
        throw PrintFailure.failed('error.print.worksheetChanged');
      }
      final pipeline = excelPipeline;
      if (pipeline == null) {
        throw PrintFailure.unsupported('error.print.excelUnavailable');
      }
      return pipeline.exportAndSubmit(job, source, sourceFile, cancellation);
    }

    final workspace = _createPrivateWorkspace();
    try {
      final cap.CapabilityRoot workspaceCapability;
      try {
        workspaceCapability = cap.CapabilityRoot.open(workspace.path);
      } catch (_) {
        throw PrintFailure.failed('private print staging is unavailable');
      }
      final sourceName = _fileNameOf(job.path);
      if (sourceName == null || sourceName.isEmpty) {
        throw PrintFailure.failed('print file name is unavailable');
      }
      final stagedPath = p.join(workspace.path, sourceName);
      final cap.AnchoredPath staged;
      try {
        staged = workspaceCapability.authorize(stagedPath).anchor();
      } catch (_) {
        throw PrintFailure.failed('private print staging is unavailable');
      }
      try {
        sourceFile.seek(0);
        staged.copyNoClobberFromFile(sourceFile);
      } catch (_) {
        throw PrintFailure.failed('private print staging failed');
      }
      final inventoryValue = adapter.inventory().normalized();
      if (!inventoryValue.contains(job.printer)) {
        throw PrintFailure.failed('printer is no longer available');
      }
      final support = adapter.inspect(stagedPath, job.printer, job.settings());
      if (support.verb != job.verb) {
        throw PrintFailure.failed('print capability changed after preview');
      }
      final String canonical;
      try {
        canonical = canonicalFor(job.pageRange, support.pageRange);
      } on BackendError catch (error) {
        throw PrintFailure.failed(error.message);
      }
      if (canonical != job.pageRange) {
        throw PrintFailure.failed('error.print.pageRangeInvalid');
      }
      final stagedJob = job.clone()..path = stagedPath;
      adapter.submit(stagedJob, () => cancellation.isCancelled);
      return const PrintSubmissionOutcome();
    } finally {
      _removeWorkspace(workspace);
    }
  }
}

// ---- Module helpers --------------------------------------------------------

/// Port of `default_queue_root`.
String defaultQueueRoot(String appDataDir) {
  final override = Platform.environment['DOCFLOW_PRINT_QUEUE_DIR'];
  if (override != null && override.isNotEmpty) {
    return override;
  }
  return p.join(appDataDir, 'print-queues');
}

void _validateQueueId(String queueId) {
  if (queueId.isEmpty ||
      queueId.length > 128 ||
      !queueId.codeUnits.every(
        (unit) =>
            (unit >= 0x30 && unit <= 0x39) ||
            (unit >= 0x41 && unit <= 0x5A) ||
            (unit >= 0x61 && unit <= 0x7A) ||
            unit == 0x2D,
      )) {
    throw BackendError.validation('invalid print queue id');
  }
}

/// Port of `queue_counts`.
Map<String, int> queueCounts(List<PrintJob> jobs) {
  final counts = <String, int>{
    'pending': 0,
    'printing': 0,
    'completed': 0,
    'completed_unconfirmed': 0,
    'failed': 0,
    'cancelled': 0,
    'uncertain': 0,
  };
  for (final job in jobs) {
    final key = job.status ?? 'pending';
    if (counts.containsKey(key)) {
      counts[key] = counts[key]! + 1;
    }
  }
  return counts;
}

Step _journalStep(PrintJob job) {
  final meta = job.toJson()..remove('path');
  return Step(
    kind: StepKind.print,
    src: job.path,
    dst: job.path,
    noteKey: 'note.print.queued',
    meta: meta,
  );
}

/// Port of `queue_diagnostic`: at most the last 64 jobs, no file paths.
PrintQueueDiagnostic queueDiagnostic(QueueState state) {
  const maxDiagnosticJobs = 64;
  final counts = queueCounts(state.jobs);
  final files = {
    for (final job in state.jobs)
      if (job.path.isNotEmpty) job.path,
  };
  final printerCandidates = [
    for (final job in state.jobs) job.printer.trim(),
  ].where((printer) => printer.isNotEmpty);
  final printer = _takeChars(
    printerCandidates.isEmpty ? 'unknown' : printerCandidates.first,
    512,
  );
  final first = state.jobs.length > maxDiagnosticJobs
      ? state.jobs.length - maxDiagnosticJobs
      : 0;
  return PrintQueueDiagnostic(
    queueStatus: state.status,
    printer: printer,
    fileCount: files.length,
    counts: counts,
    jobs: [
      for (final job in state.jobs.sublist(first))
        PrintJobDiagnostic(
          format: job.format,
          route: job.verb,
          status: job.status ?? 'pending',
          attempts: job.attempts ?? 0,
          paper: job.paper,
          orientation: job.orientation,
          scaling: job.scaling,
          color: job.color,
          duplex: job.duplex,
          pageRange: job.pageRange,
          errorDetail: job.error == null ? '' : _redactError(job.error!),
        ),
    ],
  );
}

String _takeChars(String value, int count) {
  final runes = value.runes.take(count);
  return String.fromCharCodes(runes);
}

/// Port of `workbook_safety_is_unsafe`.
bool workbookSafetyIsUnsafe(WorkbookSafetyFacts safety) =>
    safety.hasXlmMacroSheet ||
    safety.hasWorkbookProtection ||
    safety.externalLinkCount > 0 ||
    safety.connectionCount > 0 ||
    safety.queryTableCount > 0;

String _journalEffectState(JournalState? state) => switch (state) {
  JournalState.committed || JournalState.undone => 'committed',
  JournalState.partialCommitted || JournalState.undoPartial =>
    'partial_committed',
  JournalState.noEffectRetryable => 'no_effect_retryable',
  JournalState.cancelledClean => 'cancelled_clean',
  JournalState.cancelledPartial => 'cancelled_partial',
  JournalState.recoveryRequired || JournalState.applying || null =>
    'outcome_unknown',
};

BackendError _persistedSourceReselectionRequired() =>
    BackendError.validation('error.print.queueSourceReselectionRequired');

void _validateVerifiedResumePlanSources(
  Plan plan,
  List<VerifiedPersistedSource> verifiedSources,
) {
  final verifiedPaths = {for (final source in verifiedSources) source.path};
  if (verifiedPaths.isEmpty) {
    throw _persistedSourceReselectionRequired();
  }
  final jobs = plan.meta['jobs'];
  if (jobs is! List) {
    throw _persistedSourceReselectionRequired();
  }
  for (final job in jobs) {
    final path = job is Map ? job['path'] : null;
    if (path is! String || !verifiedPaths.contains(path)) {
      throw _persistedSourceReselectionRequired();
    }
  }
  final editor = plan.meta['editor'];
  final rows = editor is Map ? editor['rows'] : null;
  if (rows is! List) {
    throw _persistedSourceReselectionRequired();
  }
  final rowPaths = <String>{};
  for (final row in rows) {
    final path = row is Map ? row['path'] : null;
    if (path is! String || !verifiedPaths.contains(path)) {
      throw _persistedSourceReselectionRequired();
    }
    rowPaths.add(path);
  }
  if (!_setEquals(rowPaths, verifiedPaths)) {
    throw _persistedSourceReselectionRequired();
  }
}

bool _setEquals(Set<String> a, Set<String> b) =>
    a.length == b.length && a.every(b.contains);

/// Port of `utc_now`: RFC 3339 UTC with microseconds and a `+00:00` offset.
String _utcNow() {
  final text = DateTime.now().toUtc().toIso8601String();
  final withoutZ = text.endsWith('Z') ? text.substring(0, text.length - 1) : text;
  final micro = withoutZ.contains('.')
      ? withoutZ.padRight(withoutZ.indexOf('.') + 7, '0')
      : '$withoutZ.000000';
  return '$micro+00:00';
}

String _displayName(String path) => _fileNameOf(path) ?? '';

String? _fileNameOf(String path) {
  final name = _rustFileName(path);
  return name == null || name.isEmpty ? null : name;
}

/// Rust `Path::file_name` for POSIX and Windows-style separators.
String? _rustFileName(String path) {
  final trimmed = path.replaceAll(RegExp(r'[\\/]+$'), '');
  if (trimmed.isEmpty) {
    return null;
  }
  final index = trimmed.lastIndexOf(RegExp(r'[\\/]'));
  final name = index < 0 ? trimmed : trimmed.substring(index + 1);
  if (name == '..' || name == '.' || name.isEmpty) {
    return null;
  }
  return name;
}

/// Rust `Path::parent`: `null` for roots, `''` for a bare file name.
String? _rustParent(String path) {
  final trimmed = path.replaceAll(RegExp(r'(?<=.)[\\/]+$'), '');
  if (trimmed.isEmpty || trimmed == '/' || trimmed == '\\') {
    return null;
  }
  final index = trimmed.lastIndexOf(RegExp(r'[\\/]'));
  if (index < 0) {
    return '';
  }
  if (index == 0) {
    return trimmed.substring(0, 1);
  }
  return trimmed.substring(0, index);
}

String? _canonicalizeOrNull(String path) {
  if (path.isEmpty) {
    return null;
  }
  try {
    return Directory(path).resolveSymbolicLinksSync();
  } on FileSystemException {
    return null;
  }
}

bool _isLocalizedPrintError(String message) {
  const prefix = 'error.print.';
  if (!message.startsWith(prefix)) {
    return false;
  }
  final suffix = message.substring(prefix.length);
  if (suffix.isEmpty || suffix.length > 64) {
    return false;
  }
  return suffix.codeUnits.every(
    (unit) =>
        (unit >= 0x30 && unit <= 0x39) ||
        (unit >= 0x41 && unit <= 0x5A) ||
        (unit >= 0x61 && unit <= 0x7A) ||
        unit == 0x2E,
  );
}

String _redactError(String message) {
  final singleLine = String.fromCharCodes(
    message.runes.where((rune) => rune != 0x0D && rune != 0x0A).take(512),
  );
  return singleLine.isEmpty ? 'print adapter failed' : singleLine;
}

String _failureEvidence(PrintFailure error) {
  final detail = error.diagnosticDetail;
  if (detail != null) {
    return _redactError(detail);
  }
  return _redactError(error.message);
}

Warning _warning(String key, String subject) =>
    Warning(key: key, subject: subject);

/// Private staging directory, removed when the job finishes (Rust `TempDir`).
Directory _createPrivateWorkspace() {
  try {
    return Directory.systemTemp.createTempSync('docflow-print-');
  } on FileSystemException {
    throw PrintFailure.failed('private print staging is unavailable');
  }
}

void _removeWorkspace(Directory workspace) {
  try {
    if (workspace.existsSync()) {
      workspace.deleteSync(recursive: true);
    }
  } on FileSystemException {
    // Best effort; staging names are unique and never reused.
  }
}

Object? _decodeJson(String text) => jsonDecode(text);
