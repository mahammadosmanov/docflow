/// Port of src-tauri/src/email_extract.rs: offline and Gemini-validated RFI
/// email extraction.
///
/// The Rust regular expressions use Unicode semantics for `\b`, `\d`, `\s`
/// and `\w`. Dart's `RegExp` keeps `\b`, `\d` and `\w` ASCII-only, so every
/// pattern here spells those classes out explicitly (`\p{Nd}`,
/// `\p{White_Space}` and the Unicode word class used by [_wordBoundary]).
///
/// This file also hosts the Rust-compatible text helpers that the other E7
/// modules share (trimming, code point counting, UTF-8 byte lengths, and the
/// code point ordering that Rust `String`/`BTreeMap` use).
library;

import 'dart:convert';
import 'dart:math' as math;

import 'package:unorm_dart/unorm_dart.dart' as unorm;

import 'errors.dart';

const int maxEmailChars = 30000;
const int maxRfiCount = 100;
const int _maxExtractedTextChars = 4096;
const int _maxNotes = 100;
const int _maxNoteChars = 500;
const int _maxExtractionBytes = 512 * 1024;

// ---------------------------------------------------------------------------
// Shared Rust-compatible text helpers
// ---------------------------------------------------------------------------

final RegExp _whiteSpaceRun = RegExp(
  r'^\p{White_Space}+|\p{White_Space}+$',
  unicode: true,
);

/// Port of Rust `str::trim` (Unicode `White_Space`).
String rsTrim(String value) => value.replaceAll(_whiteSpaceRun, '');

/// Port of `str::trim_matches` for an explicit set of characters.
String rsTrimChars(String value, Set<String> chars) {
  final runes = value.runes.toList();
  var start = 0;
  var end = runes.length;
  while (start < end && chars.contains(String.fromCharCode(runes[start]))) {
    start++;
  }
  while (end > start &&
      chars.contains(String.fromCharCode(runes[end - 1]))) {
    end--;
  }
  return String.fromCharCodes(runes.sublist(start, end));
}

/// Port of `str::trim_end_matches` for an explicit set of characters.
String rsTrimEndChars(String value, Set<String> chars) {
  final runes = value.runes.toList();
  var end = runes.length;
  while (end > 0 && chars.contains(String.fromCharCode(runes[end - 1]))) {
    end--;
  }
  return String.fromCharCodes(runes.sublist(0, end));
}

/// Port of `Regex::split`: the pieces between non-empty matches.
List<String> rsRegexSplit(RegExp pattern, String text) {
  final parts = <String>[];
  var last = 0;
  for (final match in pattern.allMatches(text)) {
    parts.add(text.substring(last, match.start));
    last = match.end;
  }
  parts.add(text.substring(last));
  return parts;
}

/// Port of `str::chars().count()`.
int rsCharCount(String value) => value.runes.length;

/// Port of `str::len()` (UTF-8 bytes).
int rsUtf8Len(String value) => utf8.encode(value).length;

/// Port of `s.chars().take(limit).collect()`.
String rsTruncateChars(String value, int limit) {
  if (value.runes.length <= limit) {
    return value;
  }
  return String.fromCharCodes(value.runes.take(limit));
}

/// Rust `str` ordering: lexicographic by Unicode scalar value, which is the
/// same order as the UTF-8 byte order used by `BTreeMap<String, _>`.
int rsCompare(String left, String right) {
  final a = left.runes.toList();
  final b = right.runes.toList();
  final length = math.min(a.length, b.length);
  for (var index = 0; index < length; index++) {
    final difference = a[index].compareTo(b[index]);
    if (difference != 0) {
      return difference;
    }
  }
  return a.length.compareTo(b.length);
}

/// Rust `Vec<String>` ordering (element-wise [rsCompare], shorter first).
int rsCompareLists(List<String> left, List<String> right) {
  final length = math.min(left.length, right.length);
  for (var index = 0; index < length; index++) {
    final difference = rsCompare(left[index], right[index]);
    if (difference != 0) {
      return difference;
    }
  }
  return left.length.compareTo(right.length);
}

/// Port of `str::to_ascii_uppercase`.
String rsAsciiUpper(String value) => String.fromCharCodes(
  value.codeUnits.map((unit) => unit >= 0x61 && unit <= 0x7a ? unit - 32 : unit),
);

/// Port of `str::to_ascii_lowercase`.
String rsAsciiLower(String value) => String.fromCharCodes(
  value.codeUnits.map((unit) => unit >= 0x41 && unit <= 0x5a ? unit + 32 : unit),
);

/// Port of `str::eq_ignore_ascii_case`.
bool rsEqualsIgnoreAsciiCase(String left, String right) =>
    rsAsciiLower(left) == rsAsciiLower(right);

/// Port of `Vec::dedup`-style first-occurrence `deduplicate` helper.
void rsDeduplicate(List<String> values) {
  final seen = <String>{};
  values.retainWhere(seen.add);
}

/// Stable sort (Rust `sort_by_key` is stable; Dart `List.sort` is not).
void rsStableSortBy<T, K>(
  List<T> values,
  K Function(T value) key,
  int Function(K left, K right) compare,
) {
  final indexed = [
    for (var index = 0; index < values.length; index++)
      (values[index], index),
  ];
  indexed.sort((left, right) {
    final order = compare(key(left.$1), key(right.$1));
    return order != 0 ? order : left.$2.compareTo(right.$2);
  });
  for (var index = 0; index < values.length; index++) {
    values[index] = indexed[index].$1;
  }
}

/// Unicode word characters as Rust `\w` (Alphabetic, M, Nd, Pc, Join_Control).
const String _wordClass =
    r'\p{Alphabetic}\p{M}\p{Nd}\p{Pc}‌‍';

/// Rust `\b` (Unicode word boundary) expressed as lookarounds.
const String _wordBoundary =
    '(?:(?<=[$_wordClass])(?![$_wordClass])|(?<![$_wordClass])(?=[$_wordClass]))';

/// Rust `\s` (Unicode White_Space).
const String _ws = r'\p{White_Space}';

/// Rust `\d` (Unicode decimal digits).
const String _digit = r'\p{Nd}';

RegExp _re(String pattern, {bool ci = false, bool multiline = false}) =>
    RegExp(pattern, caseSensitive: !ci, multiLine: multiline, unicode: true);

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Port of `EmailContext`.
class EmailContext {
  const EmailContext({
    required this.discipline,
    required this.itpToItems,
    required this.knownTags,
    required this.knownSubcontractors,
  });

  final String discipline;
  final Map<String, List<String>> itpToItems;
  final List<String> knownTags;
  final List<String> knownSubcontractors;
}

/// Port of `EmailRow`.
class EmailRow {
  const EmailRow({
    required this.rfiIndex,
    required this.itpNo,
    required this.itemNo,
    required this.tagNo,
    required this.area,
    required this.location,
    required this.subcontractor,
  });

  final int rfiIndex;
  final String itpNo;
  final String itemNo;
  final String tagNo;
  final String area;
  final String location;
  final String subcontractor;

  Map<String, Object?> toJson() => {
    'rfiIndex': rfiIndex,
    'itpNo': itpNo,
    'itemNo': itemNo,
    'tagNo': tagNo,
    'area': area,
    'location': location,
    'subcontractor': subcontractor,
  };

  @override
  bool operator ==(Object other) =>
      other is EmailRow &&
      other.rfiIndex == rfiIndex &&
      other.itpNo == itpNo &&
      other.itemNo == itemNo &&
      other.tagNo == tagNo &&
      other.area == area &&
      other.location == location &&
      other.subcontractor == subcontractor;

  @override
  int get hashCode => Object.hash(
    rfiIndex,
    itpNo,
    itemNo,
    tagNo,
    area,
    location,
    subcontractor,
  );
}

/// Port of `EmailExtraction`.
class EmailExtraction {
  const EmailExtraction({
    required this.provider,
    this.model,
    required this.discipline,
    required this.count,
    required this.rows,
    required this.confidence,
    required this.notes,
  });

  final String provider;
  final String? model;
  final String discipline;
  final int count;
  final List<EmailRow> rows;
  final double confidence;
  final List<String> notes;

  /// Serde shape: `model` is omitted when absent (`skip_serializing_if`).
  Map<String, Object?> toJson() => {
    'provider': provider,
    if (model != null) 'model': model,
    'discipline': discipline,
    'count': count,
    'rows': [for (final row in rows) row.toJson()],
    'confidence': confidence,
    'notes': notes,
  };
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

final RegExp _disciplinePattern = _re(
  r'(?:^|[^A-Za-z])(CIV|MEC|ELE|PRC)(?:$|[^A-Za-z])',
  ci: true,
);

/// Port of `detect_discipline`.
String? detectDiscipline(String text) {
  final match = _disciplinePattern.firstMatch(text);
  final value = match?.group(1);
  return value == null ? null : rsAsciiUpper(value);
}

/// Port of `offline_extract`.
EmailExtraction offlineExtract(String text, EmailContext context) {
  var clean = rsTruncateChars(text, maxEmailChars).replaceAll(r'\_', '_');
  clean = clean.replaceAll(_markdownPattern, '');
  final allItems = [
    for (final items in context.itpToItems.values) ...items,
  ];
  var items = _mentionedKnown(clean, allItems);
  var itps = _mentionedIdentifiers(clean, context.itpToItems.keys.toList());

  if (itps.isEmpty) {
    final captures = _itpNumberPattern.firstMatch(clean);
    final suffixText = captures?.group(1);
    final suffix = suffixText == null ? null : int.tryParse(suffixText);
    if (suffix != null) {
      final padded = suffix.toString().padLeft(4, '0');
      itps = [
        for (final label in context.itpToItems.keys)
          if (label.endsWith(padded)) label,
      ];
    }
  }
  if (items.isEmpty) {
    final item = _abbreviatedItem(clean, itps, context.itpToItems);
    if (item != null) {
      items = [item];
    }
  }

  var tagValues = [
    for (final captures in _tagLinePattern.allMatches(clean))
      if (captures.group(1) != null)
        ..._splitValues(captures.group(1)!),
  ];
  if (tagValues.isEmpty) {
    final compact = _expandCompactTags(clean);
    final tokens = _extractTagTokens(clean);
    final known = _mentionedKnown(clean, context.knownTags);
    tagValues = compact.isNotEmpty
        ? compact
        : tokens.isNotEmpty
        ? tokens
        : known;
  }
  rsDeduplicate(tagValues);
  if (tagValues.length > maxRfiCount) {
    tagValues = tagValues.sublist(0, maxRfiCount);
  }

  final count = _explicitCount(clean);
  final inferredCount =
      count ?? (tagValues.isNotEmpty ? tagValues.length : null);
  final area = _firstLine(_areaLinePattern, clean);
  final location = _firstLine(_locationLinePattern, clean);
  var subcontractor = _firstLine(_subcontractorLinePattern, clean);
  if (subcontractor.isEmpty) {
    final known = _mentionedKnown(clean, context.knownSubcontractors);
    subcontractor = known.isEmpty ? '' : known.first;
  }

  final sharedItp = itps.length == 1 ? itps[0] : '';
  final sharedItem = items.length == 1 ? items[0] : '';
  final rowCount = math
      .max(inferredCount ?? 0, tagValues.length)
      .clamp(1, maxRfiCount)
      .toInt();
  final indexLimit = math.max(inferredCount ?? rowCount, 1);
  final rows = [
    for (var index = 0; index < rowCount; index++)
      EmailRow(
        rfiIndex: math.min(index + 1, indexLimit),
        itpNo: sharedItp,
        itemNo: sharedItem,
        tagNo: index < tagValues.length ? tagValues[index] : '',
        area: area,
        location: location,
        subcontractor: subcontractor,
      ),
  ];
  final notes = <String>[];
  if (items.length > 1) {
    notes.add('Multiple Item No. values were mentioned; review each row.');
  }
  if (itps.length > 1) {
    notes.add('Multiple ITP No. values were mentioned; review each row.');
  }
  if (count == null) {
    notes.add('RFI count was not explicit; the form count was kept.');
  }

  final recognized = [
    detectDiscipline(clean) != null,
    count != null,
    items.isNotEmpty,
    itps.isNotEmpty,
    tagValues.isNotEmpty,
    area.isNotEmpty,
    location.isNotEmpty,
    subcontractor.isNotEmpty,
  ].where((value) => value).length;
  return EmailExtraction(
    provider: 'offline',
    model: null,
    discipline: detectDiscipline(clean) ?? context.discipline,
    count: count ?? 0,
    rows: rows,
    confidence: math.min(0.35 + recognized * 0.08, 0.95),
    notes: notes,
  );
}

/// Port of `validate_gemini_extraction`. [value] is the decoded JSON object.
EmailExtraction validateGeminiExtraction(Object? value) {
  final String encoded;
  try {
    encoded = jsonEncode(value);
  } catch (_) {
    throw BackendError.validation('Gemini extraction is invalid');
  }
  if (utf8.encode(encoded).length > _maxExtractionBytes) {
    throw BackendError.validation('Gemini extraction exceeded the size limit');
  }
  final EmailExtraction extraction;
  try {
    extraction = _decodeExtraction(value);
  } on FormatException {
    throw BackendError.validation('Gemini extraction DTO is invalid');
  }
  final model = extraction.model;
  if (extraction.provider != 'gemini' ||
      !_isDiscipline(extraction.discipline) ||
      extraction.count > maxRfiCount ||
      extraction.rows.length > maxRfiCount ||
      !extraction.confidence.isFinite ||
      extraction.confidence < 0.0 ||
      extraction.confidence > 1.0 ||
      extraction.notes.length > _maxNotes ||
      model == null ||
      model.isEmpty ||
      rsCharCount(model) > 128) {
    throw BackendError.validation('Gemini extraction values are out of range');
  }
  if (extraction.rows.any(
        (row) =>
            row.rfiIndex == 0 ||
            row.rfiIndex > maxRfiCount ||
            _rowFields(row).any(
              (field) => rsCharCount(field) > _maxExtractedTextChars,
            ),
      ) ||
      extraction.notes.any((note) => rsCharCount(note) > _maxNoteChars)) {
    throw BackendError.validation('Gemini extraction values are out of range');
  }
  return extraction;
}

// ---------------------------------------------------------------------------
// Decoding (serde-compatible strictness)
// ---------------------------------------------------------------------------

EmailExtraction _decodeExtraction(Object? value) {
  final map = _object(value);
  final notes = _list(map, 'notes').map(_string).toList();
  final rowsRaw = _list(map, 'rows');
  final rows = [
    for (final row in rowsRaw) _decodeRow(row),
  ];
  final count = _unsignedInt(map, 'count');
  return EmailExtraction(
    provider: _string(map['provider']),
    model: map.containsKey('model') && map['model'] != null
        ? _string(map['model'])
        : null,
    discipline: _string(map['discipline']),
    count: count,
    rows: rows,
    confidence: _float(map, 'confidence'),
    notes: notes,
  );
}

EmailRow _decodeRow(Object? value) {
  final map = _object(value);
  return EmailRow(
    rfiIndex: _unsignedInt(map, 'rfiIndex'),
    itpNo: _string(map['itpNo']),
    itemNo: _string(map['itemNo']),
    tagNo: _string(map['tagNo']),
    area: _string(map['area']),
    location: _string(map['location']),
    subcontractor: _string(map['subcontractor']),
  );
}

Map<String, Object?> _object(Object? value) {
  if (value is Map<String, Object?>) {
    return value;
  }
  if (value is Map) {
    return value.map((key, value) => MapEntry(key.toString(), value));
  }
  throw const FormatException('expected object');
}

List<Object?> _list(Map<String, Object?> map, String key) {
  final value = map[key];
  if (value is List) {
    return value.cast<Object?>();
  }
  throw FormatException('missing list $key');
}

String _string(Object? value) {
  if (value is String) {
    return value;
  }
  throw const FormatException('expected string');
}

int _unsignedInt(Map<String, Object?> map, String key) {
  final value = map[key];
  if (value is int && value >= 0 && value <= 0x7fffffffffffffff) {
    return value;
  }
  throw FormatException('expected unsigned integer $key');
}

double _float(Map<String, Object?> map, String key) {
  final value = map[key];
  if (value is num) {
    return value.toDouble();
  }
  throw FormatException('expected number $key');
}

// ---------------------------------------------------------------------------
// Offline extraction internals
// ---------------------------------------------------------------------------

bool _isDiscipline(String value) =>
    value == 'CIV' || value == 'MEC' || value == 'ELE' || value == 'PRC';

List<String> _rowFields(EmailRow row) => [
  row.itpNo,
  row.itemNo,
  row.tagNo,
  row.area,
  row.location,
  row.subcontractor,
];

int? _explicitCount(String text) {
  for (final pattern in _countPatterns) {
    final captured = pattern.firstMatch(text)?.group(1);
    final value = captured == null ? null : int.tryParse(captured);
    if (value != null && value >= 1 && value <= maxRfiCount) {
      return value;
    }
  }
  return null;
}

List<String> _mentionedKnown(String text, List<String> values) {
  final upper = text.toUpperCase();
  return [
    for (final value in values)
      if (value.isNotEmpty && upper.contains(value.toUpperCase())) value,
  ];
}

final RegExp _identifierToken = _re(r'[A-Z]+|' + _digit + r'+');

String _identifierText(String value) {
  final normalized = unorm.nfkc(value).toUpperCase();
  return _identifierToken
      .allMatches(normalized)
      .map((match) => match.group(0)!)
      .join('-');
}

List<String> _mentionedIdentifiers(String text, List<String> values) {
  final normalizedText = _identifierText(text);
  return [
    for (final value in values)
      if (_identifierText(value).isNotEmpty &&
          normalizedText.contains(_identifierText(value)))
        value,
  ];
}

String? _abbreviatedItem(
  String text,
  List<String> itps,
  Map<String, List<String>> mapping,
) {
  final candidates = itps.length == 1
      ? List<String>.of(mapping[itps[0]] ?? const <String>[])
      : [for (final items in mapping.values) ...items];
  final matches = <String>[];
  for (final item in candidates) {
    final numbers = _numberPattern
        .allMatches(item)
        .map((match) => match.group(0)!)
        .toList();
    if (numbers.length < 2) {
      continue;
    }
    final short =
        '${numbers[numbers.length - 2]}.${numbers[numbers.length - 1]}';
    if (_containsBoundedNumber(text, short) && !matches.contains(item)) {
      matches.add(item);
    }
  }
  return matches.length == 1 ? matches.first : null;
}

bool _containsBoundedNumber(String text, String needle) {
  var start = 0;
  while (true) {
    final index = text.indexOf(needle, start);
    if (index < 0) {
      return false;
    }
    final end = index + needle.length;
    final before = index > 0 ? text.codeUnitAt(index - 1) : null;
    final after = end < text.length ? text.codeUnitAt(end) : null;
    if (!_isDigitOrDot(before) && !_isDigitOrDot(after)) {
      return true;
    }
    start = end;
  }
}

bool _isDigitOrDot(int? unit) =>
    unit != null && ((unit >= 0x30 && unit <= 0x39) || unit == 0x2e);

List<String> _splitValues(String value) => [
  for (final item in rsRegexSplit(_splitPattern, value))
    if (rsTrimChars(item, const {' ', '\t', '.', ';'}).isNotEmpty)
      rsTrimChars(item, const {' ', '\t', '.', ';'}),
];

List<String> _expandCompactTags(String text) {
  final tags = <String>[];
  for (final captures in _compactTagPattern.allMatches(text.toUpperCase())) {
    final prefix = captures.group(1);
    final numbers = captures.group(2);
    if (prefix == null || numbers == null) {
      continue;
    }
    tags.addAll(
      rsRegexSplit(_slashPattern, numbers).map((number) => '$prefix$number'),
    );
  }
  return tags;
}

List<String> _extractTagTokens(String text) {
  final upper = text.toUpperCase();
  final tokens = <String>[
    for (final captures in _tagTokenPattern.allMatches(upper))
      if (captures.group(1) != null) captures.group(1)!,
  ].where((token) {
    if (token.contains('QMS-ITP')) {
      return false;
    }
    return !const ['AZE-SUN-', 'CIV-', 'MEC-', 'ELE-', 'PRC-'].any(
      (prefix) => token.startsWith(prefix),
    );
  }).toList();
  rsDeduplicate(tokens);
  return tokens;
}

String _firstLine(RegExp pattern, String text) {
  final value = pattern.firstMatch(text)?.group(1);
  return value == null ? '' : rsTrim(value);
}

// ---------------------------------------------------------------------------
// Patterns
// ---------------------------------------------------------------------------

final RegExp _markdownPattern = _re(r'[*`]+');
final RegExp _numberPattern = _re(_digit + r'+');
final RegExp _itpNumberPattern = _re(
  _wordBoundary +
      r'ITP(?:' +
      _ws +
      r'+NO\.?)?' +
      _ws +
      r'*[:#-]?' +
      _ws +
      r'*0*(' +
      _digit +
      r'{1,4})' +
      _wordBoundary,
  ci: true,
);
final List<RegExp> _countPatterns = [
  _re(
    '$_wordBoundary($_digit{1,3})$_ws*(?:CIV|MEC|ELE|PRC)?$_ws*RFI(?:$_ws+numbers?)?$_wordBoundary',
    ci: true,
  ),
  _re(
    '$_wordBoundary($_digit{1,3})$_ws*(?:adet|ədəd|pcs?\\.?|rfi(?:$_ws+numbers?)?|numara(?:s[ıi])?|n[oö]mr[əe])$_wordBoundary',
    ci: true,
  ),
  _re(
    _wordBoundary +
        r'(?:give|create|open|ver|olu[sş]tur|ay[ıi]r)[^\r\n]{0,18}(' +
        _digit +
        r'{1,3})' +
        _ws +
        r'*(?:new' +
        _ws +
        r'+)?(?:rfi|number|numara)',
    ci: true,
  ),
];
final RegExp _splitPattern = _re(
  _ws + r'*[,;]' + _ws + r'*|' + _ws + r'+/' + _ws + r'+',
);
final RegExp _slashPattern = _re(_ws + r'*/' + _ws + r'*');
final RegExp _compactTagPattern = _re(
  _wordBoundary +
      r'([A-Z][A-Z0-9]*-)(' +
      _digit +
      r'+(?:' +
      _ws +
      r'*/' +
      _ws +
      r'*' +
      _digit +
      r'+)+)' +
      _wordBoundary,
  ci: true,
);
final RegExp _tagTokenPattern = _re(
  r'(?:^|[^A-Z0-9])([A-Z0-9]+(?:[_-][A-Z0-9]+)+(?:/[A-Z0-9]+)*)(?:$|[^A-Z0-9])',
  ci: true,
);
final RegExp _tagLinePattern = _re(
  r'^' +
      _ws +
      r'*(?:tag(?:' +
      _ws +
      r'+no\.?)?|etiket)' +
      _ws +
      r'*[:=-]' +
      _ws +
      r'*([^\r\n]+)',
  ci: true,
  multiline: true,
);
final RegExp _areaLinePattern = _re(
  r'^' + _ws + r'*area' + _ws + r'*[:=-]' + _ws + r'*([^\r\n]+)',
  ci: true,
  multiline: true,
);
final RegExp _locationLinePattern = _re(
  r'^' +
      _ws +
      r'*(?:inspection' +
      _ws +
      r'+)?location' +
      _ws +
      r'*[:=-]' +
      _ws +
      r'*([^\r\n]+)',
  ci: true,
  multiline: true,
);
final RegExp _subcontractorLinePattern = _re(
  r'^' + _ws + r'*subcontractor' + _ws + r'*[:=-]' + _ws + r'*([^\r\n]+)',
  ci: true,
  multiline: true,
);
