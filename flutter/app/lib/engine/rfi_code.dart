/// Port of src-tauri/src/rfi_code.rs.
///
/// Rust `char::is_alphanumeric` is modelled as `\p{Alphabetic}|\p{N}` and
/// `char::is_whitespace` as `\p{White_Space}`. Indices are UTF-16 offsets,
/// consistent with Dart `RegExp` matches on the same string.
library;

import 'dart:collection';

import 'package:unorm_dart/unorm_dart.dart' as unorm;

const String prefix = 'AZE-SUN-INT-QMS-RFI';
const String _prefixHead = 'AZE-SUN-INT-QMS';
const List<String> _disciplines = ['CIV', 'ELE', 'MEC', 'PRC'];

enum RfiMethod { strict, repaired, ocrAlias, fuzzy, miss }

class RfiMatch {
  const RfiMatch({
    required this.code,
    required this.discipline,
    required this.number,
    required this.method,
    required this.confidence,
  });

  final String code;
  final String discipline;
  final String number;
  final RfiMethod method;
  final double confidence;

  bool get ok => method != RfiMethod.miss;

  @override
  bool operator ==(Object other) =>
      other is RfiMatch &&
      other.code == code &&
      other.discipline == discipline &&
      other.number == number &&
      other.method == method &&
      other.confidence == confidence;

  @override
  int get hashCode => Object.hash(code, discipline, number, method, confidence);

  @override
  String toString() =>
      'RfiMatch(code: $code, discipline: $discipline, number: $number, '
      'method: $method, confidence: $confidence)';
}

/// NFC normalization followed by full Unicode uppercase.
String canonical(String value) => unorm.nfc(value).toUpperCase();

RfiMatch find(String value) => canonicalAndFind(value).$2;

(String, RfiMatch) canonicalAndFind(String value) {
  final text = canonical(value);
  return (text, _findCanonical(text));
}

/// Port of `normalize_candidate_number`: one to four ASCII digits, left
/// padded with zeros to four characters.
String? normalizeCandidateNumber(String value) {
  if (value.isEmpty ||
      value.length > 4 ||
      !value.codeUnits.every(_isAsciiDigit)) {
    return null;
  }
  return value.padLeft(4, '0');
}

final RegExp _fullCode = RegExp(
  r'^AZE-SUN-INT-QMS-RFI-(CIV|ELE|MEC|PRC)-([0-9]{1,4})$',
);

String? normalizeFullCode(String value) {
  final normalized = canonical(value);
  final captures = _fullCode.firstMatch(normalized);
  if (captures == null) {
    return null;
  }
  final number = normalizeCandidateNumber(captures[2]!);
  if (number == null) {
    return null;
  }
  return '$prefix-${captures[1]}-$number';
}

SplayTreeSet<String> exactCodes(String value) {
  final text = canonical(value);
  final codes = SplayTreeSet<String>();
  for (final captures in _exactWrapped.allMatches(text)) {
    final matched = captures.group(0)!;
    final start = captures.start;
    final end = captures.end;
    if (_disciplines.contains(captures[1]) &&
        _lineBreakCount(matched) <= 2 &&
        !_lastCharIsAlphanumeric(text.substring(0, start)) &&
        !_firstCharIsAlphanumeric(text.substring(end)) &&
        !_followedByAlphanumeric(text, end)) {
      codes.add(
        '$prefix-${captures[1]}-${normalizeCandidateNumber(captures[2]!)!}',
      );
    }
  }
  return codes;
}

RfiMatch _findCanonical(String text) {
  for (final captures in _strict.allMatches(text)) {
    if (_disciplines.contains(captures[1]) &&
        _validRfiSuffixBoundary(text, captures.end, captures[2]!)) {
      return _build(captures[1]!, captures[2]!, RfiMethod.strict, 1.0);
    }
  }
  for (final captures in _exactWrapped.allMatches(text)) {
    final matched = captures.group(0)!;
    if (_disciplines.contains(captures[1]) &&
        _lineBreakCount(matched) <= 2 &&
        _validRfiSuffixBoundary(text, captures.end, captures[2]!)) {
      return _build(captures[1]!, captures[2]!, RfiMethod.strict, 1.0);
    }
  }

  for (final candidate in _loose.allMatches(text)) {
    if (_followedByAlphanumeric(text, candidate.end)) {
      continue;
    }
    final token = candidate.group(0)!;
    final fixed = _repair(token);
    final strictCaptures = _strict.firstMatch(fixed);
    if (strictCaptures != null &&
        _disciplines.contains(strictCaptures[1]) &&
        !_followedByAlphanumeric(fixed, strictCaptures.end)) {
      return _build(
        strictCaptures[1]!,
        strictCaptures[2]!,
        RfiMethod.repaired,
        0.8,
      );
    }
    final segments = fixed.split('-');
    if (segments.length != 7 || segments[4] != 'RFI') {
      continue;
    }
    final number = segments[6];
    if (number.length != 4 || !number.codeUnits.every(_isAsciiDigit)) {
      continue;
    }
    final discipline = segments[5];
    if (!_disciplines.contains(discipline)) {
      continue;
    }
    final distance = _levenshtein(
      segments.sublist(0, 4).join('-'),
      _prefixHead,
    );
    if (distance > 2) {
      continue;
    }
    return _build(discipline, number, RfiMethod.fuzzy, 1.0 - distance / 10.0);
  }
  return _miss();
}

final RegExp _strict = RegExp(r'AZE-SUN-INT-QMS-RFI-([A-Z]{3})-([0-9]{1,4})');

final RegExp _exactWrapped = () {
  const separator = r'[\t ]*(?:\r?\n[\t ]*){0,2}-[\t ]*(?:\r?\n[\t ]*){0,2}';
  return RegExp(
    'AZE$separator'
    'SUN$separator'
    'INT$separator'
    'QMS$separator'
    'RFI$separator'
    '([A-Z]{3})$separator'
    '([0-9]{1,4})',
  );
}();

final RegExp _loose = RegExp(r'[A-Z0-9]{2,4}(?:-[A-Z0-9]{2,4}){5,6}');

final RegExp _alphanumericChar = RegExp(
  r'^[\p{Alphabetic}\p{N}]$',
  unicode: true,
);

final RegExp _whitespaceChar = RegExp(r'^\p{White_Space}$', unicode: true);

bool _isAsciiDigit(int codeUnit) => codeUnit >= 0x30 && codeUnit <= 0x39;

bool _isAsciiAlphabetic(int codeUnit) =>
    (codeUnit >= 0x41 && codeUnit <= 0x5A) ||
    (codeUnit >= 0x61 && codeUnit <= 0x7A);

/// Code point (rune) at the start of [text], or null when empty.
String? _firstChar(String text) {
  if (text.isEmpty) {
    return null;
  }
  final rune = text.runes.first;
  return String.fromCharCode(rune);
}

/// Code point (rune) at the end of [text], or null when empty.
String? _lastChar(String text) {
  if (text.isEmpty) {
    return null;
  }
  return String.fromCharCode(text.runes.last);
}

bool _isAlphanumericChar(String character) =>
    _alphanumericChar.hasMatch(character);

bool _lastCharIsAlphanumeric(String text) {
  final character = _lastChar(text);
  return character != null && _isAlphanumericChar(character);
}

bool _firstCharIsAlphanumeric(String text) {
  final character = _firstChar(text);
  return character != null && _isAlphanumericChar(character);
}

int _lineBreakCount(String value) =>
    value.codeUnits.where((codeUnit) => codeUnit == 0x0A).length;

bool _followedByAlphanumeric(String text, int end) {
  final rest = text.substring(end);
  return _firstCharIsAlphanumeric(rest);
}

bool _validRfiSuffixBoundary(String text, int end, String number) {
  // First non-whitespace character after the match must not be an ASCII digit.
  final rest = text.substring(end);
  for (final character in rest.runes) {
    final value = String.fromCharCode(character);
    if (_whitespaceChar.hasMatch(value)) {
      continue;
    }
    if (_isAsciiDigit(character)) {
      return false;
    }
    break;
  }
  if (!_followedByAlphanumeric(text, end)) {
    return true;
  }
  if (number.length != 4) {
    return false;
  }
  var trailingAsciiLetters = 0;
  while (end + trailingAsciiLetters < text.length &&
      _isAsciiAlphabetic(text.codeUnitAt(end + trailingAsciiLetters))) {
    trailingAsciiLetters++;
  }
  return trailingAsciiLetters > 0 &&
      !_followedByAlphanumeric(text, end + trailingAsciiLetters);
}

String _repair(String token) {
  final segments = token.split('-');
  final last = segments.length - 1;
  for (var index = 0; index < last; index++) {
    segments[index] = String.fromCharCodes(
      segments[index].runes.map((rune) => _toLetter(rune)),
    );
  }
  segments[last] = String.fromCharCodes(
    segments[last].runes.map((rune) => _toDigit(rune)),
  );
  return segments.join('-');
}

int _toLetter(int character) {
  switch (String.fromCharCode(character)) {
    case '0':
      return 'O'.codeUnitAt(0);
    case '1':
      return 'I'.codeUnitAt(0);
    case '2':
      return 'Z'.codeUnitAt(0);
    case '4':
      return 'A'.codeUnitAt(0);
    case '5':
      return 'S'.codeUnitAt(0);
    case '8':
      return 'B'.codeUnitAt(0);
    default:
      return character;
  }
}

int _toDigit(int character) {
  switch (String.fromCharCode(character)) {
    case 'O':
      return '0'.codeUnitAt(0);
    case 'I':
    case 'L':
      return '1'.codeUnitAt(0);
    case 'S':
      return '5'.codeUnitAt(0);
    case 'B':
      return '8'.codeUnitAt(0);
    case 'Z':
      return '2'.codeUnitAt(0);
    default:
      return character;
  }
}

int _levenshtein(String left, String right) {
  final rightRunes = right.runes.toList();
  var previous = List<int>.generate(rightRunes.length + 1, (index) => index);
  var row = 0;
  for (final leftCharacter in left.runes) {
    final current = <int>[row + 1];
    for (var column = 0; column < rightRunes.length; column++) {
      final substitution =
          previous[column] + (leftCharacter != rightRunes[column] ? 1 : 0);
      final insertion = previous[column + 1] + 1;
      final deletion = current[column] + 1;
      current.add(_min3(insertion, deletion, substitution));
    }
    previous = current;
    row++;
  }
  return previous[rightRunes.length];
}

int _min3(int a, int b, int c) {
  var result = a;
  if (b < result) {
    result = b;
  }
  if (c < result) {
    result = c;
  }
  return result;
}

RfiMatch _build(
  String discipline,
  String number,
  RfiMethod method,
  double confidence,
) {
  final normalized = normalizeCandidateNumber(number)!;
  return RfiMatch(
    code: '$prefix-$discipline-$normalized',
    discipline: discipline,
    number: normalized,
    method: method,
    confidence: confidence,
  );
}

RfiMatch _miss() => const RfiMatch(
  code: '',
  discipline: '',
  number: '',
  method: RfiMethod.miss,
  confidence: 0.0,
);
