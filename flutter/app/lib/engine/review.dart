/// Port of src-tauri/src/review.rs.
///
/// Review inbox scanning and the mutation-free preview for review decisions.
/// OCR is supplied through [ReviewOcrAdapter], which the OCR engine (E10)
/// implements; this module never performs OCR itself.
library;

import 'dart:io';

import 'package:path/path.dart' as p;

import 'contracts.dart';
import 'errors.dart';
import 'naming.dart';
import 'rfi_code.dart';

const int maxReviewItems = 10000;

enum ReviewAction {
  put('put'),
  delete('delete');

  const ReviewAction(this.wire);

  final String wire;

  static ReviewAction fromWire(Object? wire) {
    for (final value in values) {
      if (value.wire == wire) {
        return value;
      }
    }
    throw FormatException('unknown review action: $wire');
  }
}

/// `ReviewDecision` (serde `deny_unknown_fields`).
class ReviewDecision {
  const ReviewDecision({
    required this.path,
    required this.action,
    required this.code,
  });

  final String path;
  final ReviewAction action;
  final String code;

  factory ReviewDecision.fromJson(Map<String, Object?> json) {
    const allowed = {'path', 'action', 'code'};
    for (final key in json.keys) {
      if (!allowed.contains(key)) {
        throw FormatException('unknown field: $key');
      }
    }
    final path = json['path'];
    final code = json['code'];
    if (path is! String || code is! String) {
      throw const FormatException('review decision is malformed');
    }
    return ReviewDecision(
      path: path,
      action: ReviewAction.fromWire(json['action']),
      code: code,
    );
  }

  @override
  bool operator ==(Object other) =>
      other is ReviewDecision &&
      other.path == path &&
      other.action == action &&
      other.code == code;

  @override
  int get hashCode => Object.hash(path, action, code);
}

/// Result of one OCR read of a review PDF.
class ReviewOcrRead {
  const ReviewOcrRead({required this.matched, required this.source});

  final RfiMatch matched;
  final String source;
}

/// Port for the OCR adapter. Implementations may throw; a throw is treated
/// as "no OCR result", matching the Rust `.ok()` handling.
abstract interface class ReviewOcrAdapter {
  Future<ReviewOcrRead> read(String path);
}

Future<List<ReviewItem>> scan(
  String destinationRoot,
  ReviewOcrAdapter ocr,
) async {
  final root = _canonicalOrInvalid(
    destinationRoot,
    'review destination does not exist',
  );
  final reviewRoot = p.join(root, '_review');
  if (FileSystemEntity.typeSync(reviewRoot) != FileSystemEntityType.directory) {
    return <ReviewItem>[];
  }
  final List<String> files;
  try {
    files = <String>[];
    for (final entity in Directory(reviewRoot).listSync(followLinks: false)) {
      if (files.length > maxReviewItems) {
        break;
      }
      final name = p.basename(entity.path);
      if (name.startsWith('.') ||
          FileSystemEntity.typeSync(entity.path, followLinks: false) !=
              FileSystemEntityType.file ||
          !_isPdfName(name)) {
        continue;
      }
      files.add(entity.path);
    }
  } on FileSystemException {
    throw BackendError.pathInvalid('review directory could not be read');
  }
  _enforceItemLimit(files.length);
  files.sort((left, right) => _compareNames(p.basename(left), p.basename(right)));

  return [
    for (final path in files) await _reviewItem(root, path, ocr),
  ];
}

Future<ReviewItem> _reviewItem(
  String destinationRoot,
  String path,
  ReviewOcrAdapter ocr,
) async {
  ReviewOcrRead? read;
  try {
    read = await ocr.read(path);
  } on Object {
    read = null;
  }
  final matched = read?.matched;
  final code = (matched != null && matched.ok) ? matched.code : '';
  final target = code.isEmpty ? null : targetFor(destinationRoot, code);
  return ReviewItem(
    name: p.basename(path),
    path: path,
    code: code,
    confidence: matched?.confidence ?? 0.0,
    targetExists: target != null && _exists(target),
    target: target,
  );
}

Plan preview(
  String destinationRoot,
  String trashRoot,
  List<ReviewDecision> decisions,
  String timestamp,
) {
  _enforceItemLimit(decisions.length);
  _validateTimestamp(timestamp);
  final root = _canonicalOrInvalid(
    destinationRoot,
    'review destination does not exist',
  );
  final reviewRoot = _canonicalOrInvalid(
    p.join(root, '_review'),
    'review directory does not exist',
  );
  final trash = _canonicalOrInvalid(
    trashRoot,
    'recoverable trash does not exist',
  );
  final steps = <Step>[];
  final warnings = <Warning>[];

  for (final decision in decisions) {
    final source = _validateReviewSource(reviewRoot, decision.path);
    final sourceName = p.basename(source);
    switch (decision.action) {
      case ReviewAction.delete:
        steps.add(
          _reviewStep(
            StepKind.move,
            source,
            p.join(trash, '${timestamp}_$sourceName'),
            'note.deleted',
            'delete',
            null,
          ),
        );
      case ReviewAction.put:
        final (code, matched) = canonicalAndFind(decision.code.trim());
        if (code.isEmpty ||
            matched.method != RfiMethod.strict ||
            matched.code != code) {
          warnings.add(Warning(key: 'warn.no_code', subject: sourceName));
          continue;
        }
        final target = targetFor(root, code);
        final folder = folderFor(root, code);
        if (_exists(target)) {
          steps.add(
            _reviewStep(
              StepKind.move,
              target,
              p.join(trash, '${timestamp}_${_nameOr(target, 'RFI.pdf')}'),
              'note.displaced',
              'put',
              code,
            ),
          );
        }
        if (FileSystemEntity.typeSync(folder) !=
            FileSystemEntityType.directory) {
          steps.add(
            _reviewStep(
              StepKind.mkdir,
              null,
              folder,
              'note.folder_created',
              'put',
              code,
            ),
          );
        }
        steps.add(
          _reviewStep(
            StepKind.move,
            source,
            target,
            'note.renamed',
            'put',
            code,
          ),
        );
    }
  }
  return Plan(
    planId: '',
    opId: 'rfi.review',
    steps: steps,
    warnings: warnings,
    meta: JsonObject(),
  );
}

/// Builds the mutation-free portion of a review decision that moves
/// unreadable PDFs into DocFlow's recoverable trash. The caller still has to
/// cache and apply this plan through the filesystem authority with grants for
/// both roots.
Plan recoverableDeletePlan(
  String destinationRoot,
  String trashRoot,
  List<String> sources,
  String timestamp,
) {
  _enforceItemLimit(sources.length);
  _validateTimestamp(timestamp);
  final root = _canonicalOrInvalid(
    destinationRoot,
    'review destination does not exist',
  );
  final reviewRoot = _canonicalOrInvalid(
    p.join(root, '_review'),
    'review directory does not exist',
  );
  final trash = _canonicalOrInvalid(
    trashRoot,
    'recoverable trash does not exist',
  );
  if (!_pathStartsWith(reviewRoot, root)) {
    throw BackendError.pathEscape();
  }

  final steps = <Step>[];
  for (final raw in sources) {
    final source = _validateReviewSource(reviewRoot, raw);
    final name = p.basename(source);
    if (name.startsWith('.')) {
      throw BackendError.pathInvalid('review source name is invalid');
    }
    steps.add(
      _reviewStep(
        StepKind.move,
        source,
        p.join(trash, '${timestamp}_$name'),
        'note.deleted',
        'delete',
        null,
      ),
    );
  }
  return Plan(
    planId: '',
    opId: 'rfi.review',
    steps: steps,
    warnings: const [],
    meta: JsonObject(),
  );
}

void _enforceItemLimit(int count) {
  if (count > maxReviewItems) {
    throw BackendError.validation(
      'review request exceeds the $maxReviewItems-item limit',
    );
  }
}

void _validateTimestamp(String timestamp) {
  if (timestamp.isEmpty ||
      timestamp.length > 32 ||
      !RegExp(r'^[A-Za-z0-9]+$').hasMatch(timestamp)) {
    throw BackendError.validation('review trash timestamp is invalid');
  }
}

String _validateReviewSource(String reviewRoot, String source) {
  final type = FileSystemEntity.typeSync(source, followLinks: false);
  if (type == FileSystemEntityType.notFound) {
    throw BackendError.pathInvalid('review source does not exist');
  }
  if (type != FileSystemEntityType.file) {
    throw BackendError.pathInvalid('review source must be a regular file');
  }
  final resolved = _canonicalOrInvalid(
    source,
    'review source cannot be resolved',
  );
  if (p.dirname(resolved) != reviewRoot) {
    throw BackendError.pathEscape();
  }
  if (!_isPdfName(p.basename(resolved))) {
    throw BackendError.pathInvalid('review source must be a PDF');
  }
  return resolved;
}

Step _reviewStep(
  StepKind kind,
  String? source,
  String? destination,
  String noteKey,
  String action,
  String? code,
) {
  final meta = JsonObject();
  meta['action'] = action;
  if (code != null) {
    meta['code'] = code;
  }
  return Step(
    kind: kind,
    src: source,
    dst: destination,
    noteKey: noteKey,
    meta: meta,
  );
}

String _canonicalOrInvalid(String path, String message) {
  try {
    if (FileSystemEntity.typeSync(path) == FileSystemEntityType.notFound) {
      throw BackendError.pathInvalid(message);
    }
    return File(path).resolveSymbolicLinksSync();
  } on FileSystemException {
    throw BackendError.pathInvalid(message);
  }
}

bool _isPdfName(String name) => p.extension(name).toLowerCase() == '.pdf';

bool _exists(String path) =>
    FileSystemEntity.typeSync(path) != FileSystemEntityType.notFound;

bool _pathStartsWith(String path, String base) {
  final parts = p.split(path);
  final prefix = p.split(base);
  if (prefix.length > parts.length) {
    return false;
  }
  for (var index = 0; index < prefix.length; index++) {
    if (parts[index] != prefix[index]) {
      return false;
    }
  }
  return true;
}

String _nameOr(String path, String fallback) {
  final name = p.basename(path);
  return name.isEmpty ? fallback : name;
}

int _compareNames(String left, String right) {
  final a = left.codeUnits;
  final b = right.codeUnits;
  final length = a.length < b.length ? a.length : b.length;
  for (var index = 0; index < length; index++) {
    if (a[index] != b[index]) {
      return a[index].compareTo(b[index]);
    }
  }
  return a.length.compareTo(b.length);
}
