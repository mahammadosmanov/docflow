// Port of src-tauri/src/diagnostics.rs.
//
// The support bundle is local-only, pathless and secret-free. Every value is
// recursively redacted before it is encoded. Copying a bundle back to the
// clipboard accepts only the exact canonical content that this module built.

import 'dart:collection';
import 'dart:convert';
import 'dart:ffi' show Abi;
import 'dart:io';

import 'errors.dart';
import 'settings.dart' show canonicalJsonPretty, jsonDeepEquals;

const int maxBundleBytes = 32 * 1024;
const int maxRecentErrorCodes = 20;
const int _maxDiagnosticDepth = 8;
const int _maxDiagnosticItems = 64;
const int _maxDiagnosticStringChars = 512;
const int _maxErrorCodeBytes = 64;
const String _redactedSecret = '[REDACTED_SECRET]';
const String _redactedContent = '[REDACTED_CONTENT]';
const String _redactedPath = '[REDACTED_PATH]';
const String _redactedDepth = '[REDACTED_DEPTH]';
const String _redactedErrorCode = 'redacted_error_code';

final RegExp _errorCodePattern = RegExp(r'^[a-z0-9_.\-]+$');

/// Port of `DiagnosticsInput`.
final class DiagnosticsInput {
  const DiagnosticsInput({
    required this.version,
    required this.platformOs,
    required this.platformArch,
    required this.ocrAvailable,
    required this.ocrLanguageDataAvailable,
    required this.recentErrorCodes,
    required this.printer,
  });

  final String version;
  final String platformOs;
  final String platformArch;
  final bool ocrAvailable;
  final bool ocrLanguageDataAvailable;
  final List<String> recentErrorCodes;
  final PrinterDiagnostic printer;
}

/// Port of `PrinterDiagnostic` (camelCase on the wire).
final class PrinterDiagnostic {
  const PrinterDiagnostic({
    required this.inventoryStatus,
    required this.printerCount,
    required this.defaultPrinterAvailable,
    this.latestQueue,
  });

  final String inventoryStatus;
  final int printerCount;
  final bool defaultPrinterAvailable;
  final PrintQueueDiagnostic? latestQueue;

  Map<String, Object?> toJson() => <String, Object?>{
    'inventoryStatus': inventoryStatus,
    'printerCount': printerCount,
    'defaultPrinterAvailable': defaultPrinterAvailable,
    'latestQueue': latestQueue?.toJson(),
  };
}

/// Port of `PrintQueueDiagnostic`.
final class PrintQueueDiagnostic {
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

  Map<String, Object?> toJson() => <String, Object?>{
    'queueStatus': queueStatus,
    'printer': printer,
    'fileCount': fileCount,
    'counts': counts,
    'jobs': [for (final job in jobs) job.toJson()],
  };
}

/// Port of `PrintJobDiagnostic`.
final class PrintJobDiagnostic {
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

  Map<String, Object?> toJson() => <String, Object?>{
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

/// Port of `SupportBundlePreview`.
final class SupportBundlePreview {
  const SupportBundlePreview({
    required this.fileName,
    required this.mediaType,
    required this.byteLength,
    required this.content,
  });

  final String fileName;
  final String mediaType;
  final int byteLength;
  final String content;

  Map<String, Object?> toJson() => <String, Object?>{
    'fileName': fileName,
    'mediaType': mediaType,
    'byteLength': byteLength,
    'content': content,
  };

  @override
  bool operator ==(Object other) =>
      other is SupportBundlePreview &&
      other.fileName == fileName &&
      other.mediaType == mediaType &&
      other.byteLength == byteLength &&
      other.content == content;

  @override
  int get hashCode => Object.hash(fileName, mediaType, byteLength, content);
}

/// Port of `RecentErrorCodes`: the last 20 sanitized codes, newest first in
/// snapshots.
final class RecentErrorCodes {
  final Queue<String> _codes = Queue<String>();

  void record(String code) {
    final sanitized = _sanitizeErrorCode(code);
    if (_codes.length == maxRecentErrorCodes) {
      _codes.removeFirst();
    }
    _codes.addLast(sanitized);
  }

  List<String> snapshot() => _codes.toList().reversed.toList();
}

/// Port of `build_bundle`.
SupportBundlePreview buildBundle(DiagnosticsInput input) {
  final document = <String, Object?>{
    'schemaVersion': 1,
    'docflow': <String, Object?>{'version': input.version},
    'platform': <String, Object?>{
      'arch': input.platformArch,
      'os': input.platformOs,
    },
    'components': <String, Object?>{
      'ocr': <String, Object?>{
        'engine': 'tesseract',
        'languageDataAvailable': input.ocrLanguageDataAvailable,
        'status': input.ocrAvailable ? 'available' : 'missing',
      },
      'sidecar': <String, Object?>{'required': false, 'status': 'retired'},
      'printer': input.printer.toJson(),
    },
    'recentErrorCodes': [
      for (final code in input.recentErrorCodes.take(maxRecentErrorCodes))
        _sanitizeErrorCode(code),
    ],
    'privacy': <String, Object?>{
      'delivery': 'local-only',
      'redaction': 'recursive',
    },
  };
  final value = _sanitizeValue(document, 0);
  final content = '${canonicalJsonPretty(value)}\n';
  final byteLength = utf8.encode(content).length;
  if (byteLength > maxBundleBytes) {
    throw BackendError.protocol('diagnostics bundle exceeded the byte limit');
  }
  return SupportBundlePreview(
    fileName: 'docflow-support-diagnostics-v1.json',
    mediaType: 'application/json',
    byteLength: byteLength,
    content: content,
  );
}

/// Port of `validate_bundle_content`. Throws `validation` unless [content] is
/// exactly a canonical, redacted bundle that this module could have built.
void validateBundleContent(String content) {
  BackendError invalid() =>
      BackendError.validation('diagnostics bundle is invalid or was changed');
  if (content.isEmpty ||
      utf8.encode(content).length > maxBundleBytes ||
      !content.endsWith('\n')) {
    throw invalid();
  }
  final Object? value;
  try {
    value = jsonDecode(content);
  } on FormatException {
    throw invalid();
  }
  final sanitized = _sanitizeValue(value, 0);
  if (!jsonDeepEquals(sanitized, value) || !_isBundleShape(value)) {
    throw invalid();
  }
  final canonical = '${canonicalJsonPretty(value)}\n';
  if (canonical != content) {
    throw invalid();
  }
}

/// Current OS and architecture names in the Rust `std::env::consts` spelling.
({String os, String arch}) currentPlatformNames() {
  final abi = Abi.current().toString();
  final arch = abi.substring(abi.indexOf('_') + 1);
  final rustArch = switch (arch) {
    'x64' => 'x86_64',
    'arm64' => 'aarch64',
    'ia32' => 'x86',
    _ => arch,
  };
  return (os: Platform.operatingSystem, arch: rustArch);
}

bool _isBundleShape(Object? value) {
  final root = _exactObject(value, const [
    'components',
    'docflow',
    'platform',
    'privacy',
    'recentErrorCodes',
    'schemaVersion',
  ]);
  if (root == null) {
    return false;
  }
  if (root['schemaVersion'] is! int || root['schemaVersion'] != 1) {
    return false;
  }
  final docflow = _exactObject(root['docflow'], const ['version']);
  if (docflow == null || !_isBoundedSafeText(docflow['version'])) {
    return false;
  }
  final platform = _exactObject(root['platform'], const ['arch', 'os']);
  if (platform == null ||
      !_isBoundedSafeText(platform['arch']) ||
      !_isBoundedSafeText(platform['os'])) {
    return false;
  }
  final components = _exactObject(root['components'], const [
    'ocr',
    'printer',
    'sidecar',
  ]);
  if (components == null) {
    return false;
  }
  final ocr = _exactObject(components['ocr'], const [
    'engine',
    'languageDataAvailable',
    'status',
  ]);
  if (ocr == null ||
      ocr['engine'] != 'tesseract' ||
      ocr['languageDataAvailable'] is! bool ||
      !(ocr['status'] == 'available' || ocr['status'] == 'missing')) {
    return false;
  }
  final sidecar = _exactObject(components['sidecar'], const [
    'required',
    'status',
  ]);
  if (sidecar == null ||
      sidecar['required'] != false ||
      sidecar['status'] != 'retired') {
    return false;
  }
  if (!_isPrinterShape(components['printer'])) {
    return false;
  }
  final errors = root['recentErrorCodes'];
  if (errors is! List || errors.length > maxRecentErrorCodes) {
    return false;
  }
  for (final code in errors) {
    if (code is! String || _sanitizeErrorCode(code) != code) {
      return false;
    }
  }
  final privacy = _exactObject(root['privacy'], const [
    'delivery',
    'redaction',
  ]);
  return privacy != null &&
      privacy['delivery'] == 'local-only' &&
      privacy['redaction'] == 'recursive';
}

/// Returns [value] as a map when it has exactly [expectedKeys].
Map<String, Object?>? _exactObject(Object? value, List<String> expectedKeys) {
  if (value is! Map) {
    return null;
  }
  if (value.length != expectedKeys.length ||
      !expectedKeys.every(value.containsKey)) {
    return null;
  }
  return Map<String, Object?>.from(value);
}

bool _isBoundedSafeText(Object? value) =>
    value is String &&
    value.isNotEmpty &&
    value.runes.length <= _maxDiagnosticStringChars &&
    !_containsPrivatePath(value);

bool _isPrinterShape(Object? value) {
  final printer = _exactObject(value, const [
    'defaultPrinterAvailable',
    'inventoryStatus',
    'latestQueue',
    'printerCount',
  ]);
  if (printer == null) {
    return false;
  }
  if (!(printer['inventoryStatus'] == 'available' ||
          printer['inventoryStatus'] == 'unavailable') ||
      !_isU64(printer['printerCount']) ||
      printer['defaultPrinterAvailable'] is! bool) {
    return false;
  }
  final queue = printer['latestQueue'];
  if (queue == null) {
    return true;
  }
  final object = _exactObject(queue, const [
    'counts',
    'fileCount',
    'jobs',
    'printer',
    'queueStatus',
  ]);
  if (object == null) {
    return false;
  }
  if (!_isBoundedSafeText(object['printer']) ||
      !_isBoundedSafeText(object['queueStatus']) ||
      !_isU64(object['fileCount'])) {
    return false;
  }
  final counts = object['counts'];
  if (counts is! Map ||
      counts.length > _maxDiagnosticItems ||
      counts.entries.any(
        (entry) =>
            !_isBoundedSafeText(entry.key) || !_isU64(entry.value),
      )) {
    return false;
  }
  final jobs = object['jobs'];
  if (jobs is! List || jobs.length > _maxDiagnosticItems) {
    return false;
  }
  for (final job in jobs) {
    final fields = _exactObject(job, const [
      'attempts',
      'color',
      'duplex',
      'errorDetail',
      'format',
      'orientation',
      'pageRange',
      'paper',
      'route',
      'scaling',
      'status',
    ]);
    if (fields == null || !_isU64(fields['attempts'])) {
      return false;
    }
    for (final entry in fields.entries) {
      if (entry.key == 'attempts') {
        continue;
      }
      final text = entry.value;
      if (text is! String ||
          !(text.isEmpty || _isBoundedSafeText(text))) {
        return false;
      }
    }
  }
  return true;
}

bool _isU64(Object? value) => value is int && value >= 0;

String _sanitizeErrorCode(String code) {
  if (code.isNotEmpty &&
      utf8.encode(code).length <= _maxErrorCodeBytes &&
      _errorCodePattern.hasMatch(code)) {
    return code;
  }
  return _redactedErrorCode;
}

/// Recursively redacts [value] as the bundle builder does.
Object? sanitizeDiagnosticValue(Object? value) => _sanitizeValue(value, 0);

/// Port of `sanitize_value`: returns a redacted copy of [value].
Object? _sanitizeValue(Object? value, int depth) {
  if (depth >= _maxDiagnosticDepth) {
    return _redactedDepth;
  }
  if (value is Map) {
    final keys = value.keys.map((key) => key.toString()).toList()..sort();
    final bounded = keys.take(_maxDiagnosticItems);
    return <String, Object?>{
      for (final key in bounded)
        key: _sanitizeEntry(key, value[key], depth),
    };
  }
  if (value is List) {
    return [
      for (final item in value.take(_maxDiagnosticItems))
        _sanitizeValue(item, depth + 1),
    ];
  }
  if (value is String) {
    if (_containsPrivatePath(value)) {
      return _redactedPath;
    }
    if (value.runes.length > _maxDiagnosticStringChars) {
      return String.fromCharCodes(
        value.runes.take(_maxDiagnosticStringChars),
      );
    }
    return value;
  }
  return value;
}

Object? _sanitizeEntry(String key, Object? nested, int depth) {
  final normalized = _normalizeKey(key);
  if (_isSecretKey(normalized)) {
    return _redactedSecret;
  }
  if (_isContentKey(normalized)) {
    return _redactedContent;
  }
  if (_isPathKey(normalized)) {
    return _redactedPath;
  }
  return _sanitizeValue(nested, depth + 1);
}

String _normalizeKey(String key) => key
    .replaceAll(RegExp('[^A-Za-z0-9]'), '')
    .toLowerCase();

bool _isSecretKey(String key) {
  const exact = {
    'accesstoken',
    'refreshtoken',
    'sessiontoken',
    'token',
    'password',
    'passwd',
    'credential',
    'credentials',
    'secret',
    'apikey',
    'authorization',
    'privatekey',
    'cookie',
  };
  return exact.contains(key) ||
      key.endsWith('token') ||
      key.endsWith('password') ||
      key.endsWith('secret') ||
      key.endsWith('credential');
}

bool _isContentKey(String key) {
  const exact = {
    'content',
    'contents',
    'document',
    'documents',
    'documentdata',
    'filedata',
    'body',
    'payload',
    'bytes',
    'raw',
  };
  return exact.contains(key) || key.endsWith('contents');
}

bool _isPathKey(String key) =>
    key.contains('path') ||
    const {
      'file',
      'files',
      'directory',
      'folder',
      'home',
      'cwd',
      'workingdirectory',
    }.contains(key);

bool _containsPrivatePath(String text) {
  final trimmed = text.trim();
  final lower = trimmed.toLowerCase();
  if (lower.startsWith('file://') ||
      trimmed.startsWith('/') ||
      trimmed.startsWith(r'\\') ||
      trimmed.startsWith('//') ||
      _containsWindowsDrivePath(utf8.encode(trimmed))) {
    return true;
  }
  for (final raw in trimmed.split(RegExp(r'[ \t\n\f\r]+'))) {
    if (raw.isEmpty) {
      continue;
    }
    final part = raw.replaceAll(RegExp(r'''^["'()\[\],;]+|["'()\[\],;]+$'''), '');
    if (part.startsWith('/') ||
        part.startsWith(r'\\') ||
        part.toLowerCase().startsWith('file://') ||
        _containsWindowsDrivePath(utf8.encode(part))) {
      return true;
    }
  }
  return false;
}

bool _containsWindowsDrivePath(List<int> bytes) {
  for (var index = 0; index + 2 < bytes.length; index++) {
    final letter = bytes[index];
    final isAlpha =
        (letter >= 0x41 && letter <= 0x5a) || (letter >= 0x61 && letter <= 0x7a);
    if (isAlpha &&
        bytes[index + 1] == 0x3a &&
        (bytes[index + 2] == 0x5c || bytes[index + 2] == 0x2f)) {
      return true;
    }
  }
  return false;
}
