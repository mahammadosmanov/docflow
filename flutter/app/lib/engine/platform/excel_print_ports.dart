// Minimal print-queue types consumed by the Excel export pipeline.
//
// Port source: src-tauri/src/print_queue.rs (FileSignature, PrintJob,
// PrintSettings, PrintFailure, PrintFailureKind, CleanupReview,
// RetryDisposition, PrintSubmissionOutcome, file_signature_from_open_file).
//
// UNVERIFIED ownership: the full print queue is owned by the E11 unit
// (lib/engine/print_queue.dart), which does not exist yet. These are the
// exact fields and constructors excel.dart needs, mirrored one-to-one. When
// print_queue.dart lands, replace this file's imports with that module and
// delete this file; the names and semantics here must not drift.
library;

import 'dart:io';
import 'dart:typed_data';

import 'package:crypto/crypto.dart' as crypto;

import '../ooxml.dart' show WorksheetDescriptor, WorkbookSafetyFacts;

/// `FileSignature` (serde camelCase; `sha256` omitted when absent).
class FileSignature {
  const FileSignature({this.size = 0, this.mtimeNs = 0, this.sha256});

  final int size;
  final int mtimeNs;
  final String? sha256;

  bool get isLegacyEmpty => size == 0 && mtimeNs == 0 && sha256 == null;

  /// `has_valid_sha256`: exactly 64 lowercase hex characters.
  bool get hasValidSha256 {
    final value = sha256;
    if (value == null || value.length != 64) {
      return false;
    }
    return RegExp(r'^[0-9a-f]{64}$').hasMatch(value);
  }

  @override
  bool operator ==(Object other) =>
      other is FileSignature &&
      other.size == size &&
      other.mtimeNs == mtimeNs &&
      other.sha256 == sha256;

  @override
  int get hashCode => Object.hash(size, mtimeNs, sha256);
}

/// `file_signature_from_open_file` for a Dart [File] (size, mtime, and
/// optionally the full-content SHA-256 digest).
FileSignature fileSignatureFromFile(File file, {required bool includeSha256}) {
  final stat = file.statSync();
  final mtimeNs = stat.modified.microsecondsSinceEpoch * 1000;
  final digest = includeSha256
      ? crypto.sha256.convert(file.readAsBytesSync()).toString()
      : null;
  return FileSignature(size: stat.size, mtimeNs: mtimeNs, sha256: digest);
}

/// `PrintSettings`.
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
}

/// `PrintJob` (only the fields the Excel pipeline reads or clones).
class PrintJob {
  const PrintJob({
    this.jobId = '',
    this.path = '',
    this.printer = '',
    this.verb = '',
    this.format = '',
    this.paper = '',
    this.orientation = '',
    this.scaling = '',
    this.color = '',
    this.duplex = '',
    this.pageRange = 'all',
    this.timeoutMinutes = 0,
    this.retries = 0,
    this.onError = '',
    this.copy = 0,
    this.signature = const FileSignature(),
    this.worksheet,
    this.workbookSafety,
  });

  final String jobId;
  final String path;
  final String printer;
  final String verb;
  final String format;
  final String paper;
  final String orientation;
  final String scaling;
  final String color;
  final String duplex;
  final String pageRange;
  final int timeoutMinutes;
  final int retries;
  final String onError;
  final int copy;
  final FileSignature signature;
  final WorksheetDescriptor? worksheet;
  final WorkbookSafetyFacts? workbookSafety;

  PrintSettings settings() => PrintSettings(
    paper: paper,
    orientation: orientation,
    scaling: scaling,
    color: color,
    duplex: duplex,
    pageRange: pageRange,
    timeoutMinutes: timeoutMinutes,
  );

  PrintJob copyWith({
    String? pageRange,
    WorksheetDescriptor? worksheet,
    WorkbookSafetyFacts? workbookSafety,
  }) => PrintJob(
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
    pageRange: pageRange ?? this.pageRange,
    timeoutMinutes: timeoutMinutes,
    retries: retries,
    onError: onError,
    copy: copy,
    signature: signature,
    worksheet: worksheet ?? this.worksheet,
    workbookSafety: workbookSafety ?? this.workbookSafety,
  );
}

/// `PrintFailureKind`.
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

/// `CleanupReview`.
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

/// `PrintSubmissionOutcome`.
class PrintSubmissionOutcome {
  const PrintSubmissionOutcome({this.cleanupReview});

  final CleanupReview? cleanupReview;
}

/// `PrintFailure`. Thrown wherever Rust returns `Err(PrintFailure)`.
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

  factory PrintFailure.cancelled(String message, {required bool uncertain}) =>
      PrintFailure._(
        PrintFailureKind.cancelled,
        message,
        uncertain: uncertain,
        retryDisposition: RetryDisposition.never,
      );

  bool get hasCleanupReview => cleanupReview != null;

  /// Redacted diagnostic detail (`with_diagnostic`); never shown to users.
  final String? diagnosticDetail;

  PrintFailure withDiagnostic(String detail) => PrintFailure._(
    kind,
    message,
    uncertain: uncertain,
    retryDisposition: retryDisposition,
    cleanupReview: cleanupReview,
    diagnosticDetail: detail,
  );

  PrintFailure withCleanupReview(CleanupReview review) => PrintFailure._(
    kind,
    message,
    uncertain: uncertain,
    retryDisposition: retryDisposition,
    cleanupReview: review,
  );

  PrintFailure withRetryDisposition(RetryDisposition disposition) =>
      PrintFailure._(
        kind,
        message,
        uncertain: uncertain,
        retryDisposition: disposition,
        cleanupReview: cleanupReview,
      );

  bool get isUnsupported => kind == PrintFailureKind.unsupported;

  bool get isUnsupportedOptions => kind == PrintFailureKind.unsupportedOptions;

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

/// Bytes helper retained for callers that need a digest of raw content.
Uint8List sha256Bytes(List<int> bytes) =>
    Uint8List.fromList(crypto.sha256.convert(bytes).bytes);
