/// Port of src-tauri/src/operations/rfi_print_find.rs: resolves requested RFI
/// numbers to first-page DOCX and sketch PDF files under a selected RFI System
/// root.
///
/// Safety rules preserved from Rust:
///  * Matching is exact. The `-RFI-` anchor is required, and a filename must
///    carry a supported discipline (CIV, MEC, ELE, PRC) and a 1 to 4 digit
///    number. A miss is recoverable; a false label is silent corruption.
///  * Symbolic links are never followed or listed as candidates.
///  * Traversal is bounded by entry, depth, request, and result limits. Any
///    limit or read failure marks the scan as truncated, and truncated scans
///    never report a document as missing.
///
/// The wire DTOs (`RfiPrintFindResult` and friends) live in
/// `lib/engine/contracts.dart`. The handler that issues selections lives in
/// the native backend, which owns selection grants.
library;

import 'dart:io';

import 'package:path/path.dart' as p;

import '../rfi_code.dart' as rfi_code;

const int maxEntries = 50000;
const int maxDepth = 32;
const int maxRequests = 500;
const int maxInputBytes = 64;
const int maxCandidatesPerRequest = 250;
const int maxResults = 5000;

const String _marker = 'AZE-SUN-INT-QMS-RFI-';

/// Port of `DocumentType`. Declaration order matches the Rust `Ord` derive.
enum DocumentType {
  firstPage,
  sketch;

  /// Wire name used by `RfiPrintCandidate.documentType`.
  String get wireName => switch (this) {
    DocumentType.firstPage => 'firstPage',
    DocumentType.sketch => 'sketch',
  };
}

/// Port of `Candidate`. [path] is the canonical path under the root.
final class Candidate {
  const Candidate({
    required this.requested,
    required this.rfiCode,
    required this.documentType,
    required this.path,
  });

  final String requested;
  final String rfiCode;
  final DocumentType documentType;
  final String path;

  @override
  bool operator ==(Object other) =>
      other is Candidate &&
      other.requested == requested &&
      other.rfiCode == rfiCode &&
      other.documentType == documentType &&
      other.path == path;

  @override
  int get hashCode => Object.hash(requested, rfiCode, documentType, path);

  @override
  String toString() =>
      'Candidate($requested, $rfiCode, ${documentType.wireName}, $path)';
}

/// Port of `Resolution`.
final class Resolution {
  const Resolution({
    required this.requested,
    required this.rfiCodes,
    required this.candidates,
    required this.missingFirstPage,
    required this.missingSketch,
    required this.ambiguous,
  });

  final String requested;
  final List<String> rfiCodes;
  final List<Candidate> candidates;
  final bool missingFirstPage;
  final bool missingSketch;
  final bool ambiguous;

  @override
  bool operator ==(Object other) =>
      other is Resolution &&
      other.requested == requested &&
      _listEquals(other.rfiCodes, rfiCodes) &&
      _listEquals(other.candidates, candidates) &&
      other.missingFirstPage == missingFirstPage &&
      other.missingSketch == missingSketch &&
      other.ambiguous == ambiguous;

  @override
  int get hashCode => Object.hash(
    requested,
    Object.hashAll(rfiCodes),
    Object.hashAll(candidates),
    missingFirstPage,
    missingSketch,
    ambiguous,
  );
}

/// Port of `Discovery`.
final class Discovery {
  const Discovery({
    required this.resolutions,
    required this.candidates,
    required this.visited,
    required this.truncated,
  });

  final List<Resolution> resolutions;
  final List<Candidate> candidates;
  final int visited;
  final bool truncated;
}

/// Port of `IncludeTypes`.
final class IncludeTypes {
  const IncludeTypes({required this.firstPage, required this.sketch});

  final bool firstPage;
  final bool sketch;
}

/// Thrown where Rust returns `Err(&'static str)`. The message is the stable
/// internal reason; the native handler maps every failure to
/// `error.print.rfi.invalidRequest`.
class RfiPrintFindException implements Exception {
  const RfiPrintFindException(this.message);

  final String message;

  @override
  String toString() => 'RfiPrintFindException($message)';
}

/// Port of `discover`.
Discovery discover(
  String root,
  List<String> inputs,
  IncludeTypes include,
) {
  if (!include.firstPage && !include.sketch) {
    throw const RfiPrintFindException(
      'at least one document type is required',
    );
  }
  if (inputs.isEmpty || inputs.length > maxRequests) {
    throw const RfiPrintFindException(
      'RFI request count is outside the supported range',
    );
  }
  final canonicalRoot = _canonicalize(root);
  if (canonicalRoot == null) {
    throw const RfiPrintFindException('RFI System root is unavailable');
  }
  final rootType = FileSystemEntity.typeSync(canonicalRoot, followLinks: false);
  if (rootType == FileSystemEntityType.notFound) {
    throw const RfiPrintFindException('RFI System root cannot be inspected');
  }
  if (rootType != FileSystemEntityType.directory) {
    throw const RfiPrintFindException('RFI System root must be a folder');
  }
  final requests = [for (final input in inputs) parseRequest(input)];

  final files = <String>[];
  final pending = <(String, int)>[(canonicalRoot, 0)];
  var visited = 0;
  var truncated = false;
  while (pending.isNotEmpty) {
    final (directory, depth) = pending.removeLast();
    if (depth > maxDepth) {
      truncated = true;
      continue;
    }
    final List<FileSystemEntity> entries;
    try {
      entries = Directory(directory).listSync(followLinks: false);
    } on FileSystemException {
      truncated = true;
      continue;
    }
    for (final entry in entries) {
      if (visited >= maxEntries) {
        truncated = true;
        break;
      }
      visited += 1;
      final path = entry.path;
      final FileSystemEntityType type;
      try {
        type = FileSystemEntity.typeSync(path, followLinks: false);
      } on FileSystemException {
        truncated = true;
        continue;
      }
      if (type == FileSystemEntityType.link) {
        continue;
      }
      if (type == FileSystemEntityType.directory) {
        pending.add((path, depth + 1));
      } else if (type == FileSystemEntityType.file &&
          p.isWithin(canonicalRoot, path)) {
        files.add(path);
      }
    }
    if (truncated) {
      break;
    }
  }
  files.sort(_comparePaths);
  final scanIncomplete = truncated;

  final filesByCode = <String, List<(String, DocumentType)>>{};
  for (final path in files) {
    final code = codeForFilename(path);
    if (code == null) {
      continue;
    }
    final documentType = _classify(path, code);
    if (documentType != null) {
      filesByCode.putIfAbsent(code, () => []).add((path, documentType));
    }
  }
  final codes = filesByCode.keys.toList()..sort();

  final resolutions = <Resolution>[];
  final allCandidates = <Candidate>[];
  for (final request in requests) {
    final (requested, discipline, number) = request;
    final matchingCodes = [
      for (final code in codes)
        if (code.split('-').last == number &&
            (discipline == null || _disciplineForCode(code) == discipline))
          code,
    ];
    final ambiguous =
        discipline == null &&
        {
              for (final code in matchingCodes) _disciplineForCode(code) ?? '',
            }.length >
            1;
    final candidates = <Candidate>[];
    var foundFirstPage = false;
    var foundSketch = false;
    for (final code in matchingCodes) {
      final matched = filesByCode[code];
      if (matched == null) {
        continue;
      }
      for (final (file, documentType) in matched) {
        if ((documentType == DocumentType.firstPage && !include.firstPage) ||
            (documentType == DocumentType.sketch && !include.sketch)) {
          continue;
        }
        switch (documentType) {
          case DocumentType.firstPage:
            foundFirstPage = true;
          case DocumentType.sketch:
            foundSketch = true;
        }
        if (candidates.length == maxCandidatesPerRequest) {
          truncated = true;
          continue;
        }
        candidates.add(
          Candidate(
            requested: requested,
            rfiCode: code,
            documentType: documentType,
            path: file,
          ),
        );
      }
    }
    candidates.sort((a, b) => _comparePaths(a.path, b.path));
    final missingFirstPage =
        !scanIncomplete && include.firstPage && !foundFirstPage;
    final missingSketch = !scanIncomplete && include.sketch && !foundSketch;
    allCandidates.addAll(candidates);
    resolutions.add(
      Resolution(
        requested: requested,
        rfiCodes: matchingCodes,
        candidates: candidates,
        missingFirstPage: missingFirstPage,
        missingSketch: missingSketch,
        ambiguous: ambiguous,
      ),
    );
  }
  allCandidates.sort((a, b) {
    final byPath = _comparePaths(a.path, b.path);
    if (byPath != 0) {
      return byPath;
    }
    final byType = a.documentType.index.compareTo(b.documentType.index);
    if (byType != 0) {
      return byType;
    }
    return a.requested.compareTo(b.requested);
  });

  // Consecutive duplicates only, matching Rust `dedup_by` after the sort.
  final deduplicated = <Candidate>[];
  for (final candidate in allCandidates) {
    if (deduplicated.isNotEmpty && deduplicated.last == candidate) {
      continue;
    }
    deduplicated.add(candidate);
  }
  var results = deduplicated;
  if (results.length > maxResults) {
    results = results.sublist(0, maxResults);
    truncated = true;
  }
  return Discovery(
    resolutions: resolutions,
    candidates: results,
    visited: visited,
    truncated: truncated,
  );
}

/// Port of `initially_selected`: a candidate is pre-selected only when the
/// resolution is unambiguous and exactly one candidate of its type exists.
bool initiallySelected(Resolution resolution, Candidate candidate) =>
    !resolution.ambiguous &&
    resolution.candidates
            .where((row) => row.documentType == candidate.documentType)
            .length ==
        1;

bool _isSupportedDiscipline(String discipline) => switch (discipline) {
  'CIV' || 'MEC' || 'ELE' || 'PRC' => true,
  _ => false,
};

/// Port of `parse_request`. Returns (trimmed requested text, discipline, 4-digit
/// number).
(String, String?, String) parseRequest(String input) {
  final trimmed = input.trim();
  if (trimmed.isEmpty ||
      _utf8Length(trimmed) > maxInputBytes) {
    throw const RfiPrintFindException('RFI request value is invalid');
  }
  final value = _asciiUpper(trimmed);
  const invalid = RfiPrintFindException(
    'RFI number must be a four-digit number or canonical full code',
  );
  final String? discipline;
  final String number;
  if (value.startsWith('AZE-SUN-INT-QMS-RFI-')) {
    final suffix = value.substring('AZE-SUN-INT-QMS-RFI-'.length);
    final split = _splitOnce(suffix, '-');
    if (split == null) {
      throw invalid;
    }
    final normalized = rfi_code.normalizeCandidateNumber(split.$2);
    if (normalized == null || !_isSupportedDiscipline(split.$1)) {
      throw invalid;
    }
    discipline = split.$1;
    number = normalized;
  } else if (_splitOnce(value, '-') case final split?) {
    final normalized = rfi_code.normalizeCandidateNumber(split.$2);
    if (normalized == null || !_isSupportedDiscipline(split.$1)) {
      throw invalid;
    }
    discipline = split.$1;
    number = normalized;
  } else {
    final normalized = rfi_code.normalizeCandidateNumber(value);
    if (normalized == null) {
      throw invalid;
    }
    discipline = null;
    number = normalized;
  }
  return (input.trim(), discipline, number);
}

String? _disciplineForCode(String code) {
  if (!code.startsWith(_marker)) {
    return null;
  }
  return code.substring(_marker.length).split('-').first;
}

/// Port of `code_for_filename`. Scans for the strict `-RFI-` anchor anywhere
/// in the uppercased name, then reads one alphanumeric/hyphen token.
String? codeForFilename(String path) {
  final name = _asciiUpper(p.basename(path));
  final start = name.indexOf(_marker);
  if (start < 0) {
    return null;
  }
  final fragment = name.substring(start);
  final tail = fragment.split(RegExp(r'[^A-Za-z0-9-]')).first;
  if (!tail.startsWith(_marker)) {
    return null;
  }
  final suffix = tail.substring(_marker.length);
  final split = _splitOnce(suffix, '-');
  if (split == null) {
    return null;
  }
  final number = rfi_code.normalizeCandidateNumber(split.$2);
  if (number == null || !_isSupportedDiscipline(split.$1)) {
    return null;
  }
  return '$_marker${split.$1}-$number';
}

/// Port of `classify`: a DOCX whose stem equals the code is a first page; a
/// PDF whose stem is the code followed by `sketch` (ignoring surrounding
/// spaces, underscores, and hyphens, case-insensitively) is a sketch.
DocumentType? _classify(String path, String code) {
  final extension = _extensionOf(path);
  final stem = _asciiLower(_stemOf(path));
  final lowerCode = _asciiLower(code);
  if (extension == 'docx' && stem == lowerCode) {
    return DocumentType.firstPage;
  }
  if (extension == 'pdf') {
    if (!stem.startsWith(lowerCode)) {
      return null;
    }
    final rest = stem.substring(lowerCode.length);
    if (rest.replaceAll(RegExp(r'^[ _-]+|[ _-]+$'), '') == 'sketch') {
      return DocumentType.sketch;
    }
  }
  return null;
}

/// Port of `Path::extension` (lowercased ASCII). Empty when there is none.
String _extensionOf(String path) {
  final extension = p.extension(path);
  if (extension.isEmpty) {
    return '';
  }
  return _asciiLower(extension.substring(1));
}

/// Port of `Path::file_stem`.
String _stemOf(String path) => p.basenameWithoutExtension(path);

String? _canonicalize(String root) {
  try {
    return Directory(root).resolveSymbolicLinksSync();
  } on FileSystemException {
    return null;
  }
}

/// Splits on the first occurrence of [separator] (Rust `split_once`).
(String, String)? _splitOnce(String value, String separator) {
  final index = value.indexOf(separator);
  if (index < 0) {
    return null;
  }
  return (
    value.substring(0, index),
    value.substring(index + separator.length),
  );
}

/// Component-wise path ordering matching Rust `PathBuf` `Ord`.
int _comparePaths(String a, String b) {
  final left = a.split(p.separator);
  final right = b.split(p.separator);
  final count = left.length < right.length ? left.length : right.length;
  for (var i = 0; i < count; i++) {
    final byComponent = left[i].compareTo(right[i]);
    if (byComponent != 0) {
      return byComponent;
    }
  }
  return left.length.compareTo(right.length);
}

int _utf8Length(String value) => value.codeUnits.fold<int>(0, (total, unit) {
  if (unit < 0x80) {
    return total + 1;
  }
  if (unit < 0x800) {
    return total + 2;
  }
  if (unit >= 0xD800 && unit <= 0xDFFF) {
    return total + 2;
  }
  return total + 3;
});

String _asciiUpper(String value) => String.fromCharCodes(
  value.codeUnits.map((unit) => unit >= 0x61 && unit <= 0x7A ? unit - 32 : unit),
);

String _asciiLower(String value) => String.fromCharCodes(
  value.codeUnits.map((unit) => unit >= 0x41 && unit <= 0x5A ? unit + 32 : unit),
);

bool _listEquals<T>(List<T> a, List<T> b) {
  if (a.length != b.length) {
    return false;
  }
  for (var i = 0; i < a.length; i++) {
    if (a[i] != b[i]) {
      return false;
    }
  }
  return true;
}
