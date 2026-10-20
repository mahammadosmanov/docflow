// Byte-level ZIP/XML fixture builders shared by the E6 OOXML tests.
// Port of the fixture helpers in src-tauri/src/ooxml.rs (mod tests).
import 'dart:convert';
import 'dart:typed_data';

import 'package:archive/archive.dart';
import 'package:docflow/engine/ooxml.dart';
import 'package:flutter_test/flutter_test.dart';

const String mainNs = 'http://schemas.openxmlformats.org/spreadsheetml/2006/main';
const String relNs =
    'http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet';
const String officeDocumentRel =
    'http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument';
const String packageRelNs =
    'http://schemas.openxmlformats.org/package/2006/relationships';

Uint8List _b(String text) => Uint8List.fromList(utf8.encode(text));

/// A ZIP package holding [members] in order (ZipWriter-equivalent).
Uint8List zipOf(List<(String, List<int>)> members) {
  final archive = Archive();
  for (final (name, bytes) in members) {
    archive.addFile(ArchiveFile(name, bytes.length, bytes));
  }
  return Uint8List.fromList(ZipEncoder().encode(archive)!);
}

/// Port of `fixture` / `fixture_from_members` with a standard package
/// relationship to `xl/workbook.xml`.
Uint8List fixtureFromMembers(
  String workbook,
  String relationships,
  List<(String, List<int>)> members,
) {
  final packageRelationships =
      '<Relationships xmlns="$packageRelNs"><Relationship Id="rIdWorkbook" '
      'Type="$officeDocumentRel" Target="xl/workbook.xml"/></Relationships>';
  return fixtureWithWorkbookPart(
    packageRelationships: packageRelationships,
    workbookPart: 'xl/workbook.xml',
    workbook: workbook,
    relationships: relationships,
    members: members,
  );
}

/// Port of `fixture_with_workbook_part`: rewrites the `urn:r` placeholder
/// namespace, adds a package-namespace default when missing, then packs.
Uint8List fixtureWithWorkbookPart({
  required String? packageRelationships,
  required String workbookPart,
  required String workbook,
  required String relationships,
  required List<(String, List<int>)> members,
}) {
  final fixedWorkbook = workbook.replaceAll(
    'xmlns:r="urn:r"',
    'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"',
  );
  final fixedRelationships = relationships.contains('xmlns=')
      ? relationships
      : relationships.replaceFirst(
          '<Relationships',
          '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"',
        );
  final slash = workbookPart.lastIndexOf('/');
  final parent = workbookPart.substring(0, slash);
  final name = workbookPart.substring(slash + 1);
  final workbookRelationshipsPart = '$parent/_rels/$name.rels';
  final out = <(String, List<int>)>[];
  if (packageRelationships != null) {
    out.add(('_rels/.rels', _b(packageRelationships)));
  }
  out.add((workbookPart, _b(fixedWorkbook)));
  out.add((workbookRelationshipsPart, _b(fixedRelationships)));
  out.addAll(members);
  return zipOf(out);
}

/// A bare EOCD record declaring [entryCount] entries with no directory.
Uint8List declaredEntryCountFixture(int entryCount) {
  final bytes = BytesBuilder()
    ..add(const [0x50, 0x4b, 0x05, 0x06])
    ..add(_u16(0))
    ..add(_u16(0))
    ..add(_u16(entryCount))
    ..add(_u16(entryCount))
    ..add(_u32(0))
    ..add(_u32(0))
    ..add(_u16(0));
  return bytes.takeBytes();
}

List<int> _u16(int value) => [value & 0xff, (value >> 8) & 0xff];
List<int> _u32(int value) => [
  value & 0xff,
  (value >> 8) & 0xff,
  (value >> 16) & 0xff,
  (value >> 24) & 0xff,
];

int _readU16(List<int> bytes, int offset) => bytes[offset] | (bytes[offset + 1] << 8);
int _readU32(List<int> bytes, int offset) =>
    bytes[offset] |
    (bytes[offset + 1] << 8) |
    (bytes[offset + 2] << 16) |
    (bytes[offset + 3] << 24);

void _writeAt(List<int> bytes, int offset, List<int> values) {
  for (var i = 0; i < values.length; i++) {
    bytes[offset + i] = values[i];
  }
}

/// Offset of the last `PK\x05\x06` (Rust `rposition`).
int finalEocdOffset(List<int> bytes) {
  for (var i = bytes.length - 4; i >= 0; i--) {
    if (bytes[i] == 0x50 &&
        bytes[i + 1] == 0x4b &&
        bytes[i + 2] == 0x05 &&
        bytes[i + 3] == 0x06) {
      return i;
    }
  }
  throw StateError('no EOCD');
}

/// Central-directory record offsets, walking the directory like the Rust helper.
List<int> centralDirectoryOffsets(List<int> bytes) {
  final eocd = finalEocdOffset(bytes);
  final entryCount = _readU16(bytes, eocd + 10);
  var offset = _readU32(bytes, eocd + 16);
  final offsets = <int>[];
  for (var i = 0; i < entryCount; i++) {
    if (!(bytes[offset] == 0x50 &&
        bytes[offset + 1] == 0x4b &&
        bytes[offset + 2] == 0x01 &&
        bytes[offset + 3] == 0x02)) {
      throw StateError('central directory signature');
    }
    offsets.add(offset);
    final nameLen = _readU16(bytes, offset + 28);
    final extraLen = _readU16(bytes, offset + 30);
    final commentLen = _readU16(bytes, offset + 32);
    offset += 46 + nameLen + extraLen + commentLen;
  }
  return offsets;
}

/// Port of `append_decoy_eocd`: an extra EOCD appended after the real one.
Uint8List appendDecoyEocd(Uint8List fixture) {
  final bytes = fixture.toList();
  final offset = bytes.length;
  bytes
    ..addAll(const [0x50, 0x4b, 0x05, 0x06])
    ..addAll(_u16(0))
    ..addAll(_u16(0))
    ..addAll(_u16(1))
    ..addAll(_u16(1))
    ..addAll(_u32(0))
    ..addAll(_u32(offset))
    ..addAll(_u16(0));
  return Uint8List.fromList(bytes);
}

/// Port of `append_parser_fallback_eocd`.
Uint8List appendParserFallbackEocd(Uint8List fixture) {
  final bytes = fixture.toList();
  final originalEocd = finalEocdOffset(bytes);
  final originalEntries = _readU16(bytes, originalEocd + 10);
  final directoryOffset = _readU32(bytes, originalEocd + 16);
  final fakeEocdOffset = bytes.length;
  final fakeEntries = originalEntries + 1;
  final fakeDirectorySize = fakeEocdOffset - directoryOffset;
  bytes
    ..addAll(const [0x50, 0x4b, 0x05, 0x06])
    ..addAll(_u16(0))
    ..addAll(_u16(0))
    ..addAll(_u16(fakeEntries))
    ..addAll(_u16(fakeEntries))
    ..addAll(_u32(fakeDirectorySize))
    ..addAll(_u32(directoryOffset))
    ..addAll(_u16(0));
  return Uint8List.fromList(bytes);
}

/// Port of `append_semantically_invalid_directory`: a relocated directory
/// whose first entry claims a bogus uncompressed size and an extra field.
Uint8List appendSemanticallyInvalidDirectory(Uint8List fixture) {
  final bytes = fixture.toList();
  final originalEocd = finalEocdOffset(bytes);
  final entryCount = _readU16(bytes, originalEocd + 10);
  final directorySize = _readU32(bytes, originalEocd + 12);
  final directoryOffset = _readU32(bytes, originalEocd + 16);
  final directory = bytes.sublist(directoryOffset, directoryOffset + directorySize);
  final nameLen = _readU16(directory, 28);
  final extraLen = _readU16(directory, 30);
  _writeAt(directory, 24, _u32(0xFFFFFFFF));
  _writeAt(directory, 30, _u16(extraLen + 4));
  directory.insertAll(46 + nameLen, const [0x01, 0x00, 0x08, 0x00]);

  final finalDirectoryOffset = bytes.length;
  final finalEocdOffsetValue = finalDirectoryOffset + directory.length;
  final out = bytes..addAll(directory);
  out
    ..addAll(const [0x50, 0x4b, 0x05, 0x06])
    ..addAll(_u16(0))
    ..addAll(_u16(0))
    ..addAll(_u16(entryCount))
    ..addAll(_u16(entryCount))
    ..addAll(_u32(finalEocdOffsetValue - finalDirectoryOffset))
    ..addAll(_u32(finalDirectoryOffset))
    ..addAll(_u16(0));
  return Uint8List.fromList(out);
}

/// Port of `mutate_first_central_field`.
Uint8List mutateFirstCentralField(
  Uint8List fixture,
  int fieldOffset,
  List<int> replacement,
) {
  final bytes = fixture.toList();
  final central = centralDirectoryOffsets(bytes)[0];
  _writeAt(bytes, central + fieldOffset, replacement);
  return Uint8List.fromList(bytes);
}

/// Port of `insert_first_central_extra`.
Uint8List insertFirstCentralExtra(Uint8List fixture, List<int> extra) {
  final bytes = fixture.toList();
  final eocd = finalEocdOffset(bytes);
  final central = centralDirectoryOffsets(bytes)[0];
  final directorySize = _readU32(bytes, eocd + 12);
  final nameLen = _readU16(bytes, central + 28);
  final extraLen = _readU16(bytes, central + 30);
  final insertionOffset = central + 46 + nameLen + extraLen;
  _writeAt(bytes, central + 30, _u16(extraLen + extra.length));
  bytes.insertAll(insertionOffset, extra);
  final newEocd = eocd + extra.length;
  _writeAt(bytes, newEocd + 12, _u32(directorySize + extra.length));
  return Uint8List.fromList(bytes);
}

/// Port of `mutate_first_local_field`.
Uint8List mutateFirstLocalField(
  Uint8List fixture,
  int fieldOffset,
  List<int> replacement,
) {
  final bytes = fixture.toList();
  final central = centralDirectoryOffsets(bytes)[0];
  final local = _readU32(bytes, central + 42);
  _writeAt(bytes, local + fieldOffset, replacement);
  return Uint8List.fromList(bytes);
}

/// Port of `insert_last_local_extra`: extra on the member with the highest
/// local header offset.
Uint8List insertLastLocalExtra(Uint8List fixture, List<int> extra) {
  final bytes = fixture.toList();
  final eocd = finalEocdOffset(bytes);
  final centrals = centralDirectoryOffsets(bytes);
  var central = centrals.first;
  for (final candidate in centrals) {
    if (_readU32(bytes, candidate + 42) > _readU32(bytes, central + 42)) {
      central = candidate;
    }
  }
  final local = _readU32(bytes, central + 42);
  final directoryOffset = _readU32(bytes, eocd + 16);
  final nameLen = _readU16(bytes, local + 26);
  final extraLen = _readU16(bytes, local + 28);
  final insertionOffset = local + 30 + nameLen + extraLen;
  _writeAt(bytes, local + 28, _u16(extraLen + extra.length));
  bytes.insertAll(insertionOffset, extra);
  final newEocd = eocd + extra.length;
  _writeAt(bytes, newEocd + 16, _u32(directoryOffset + extra.length));
  return Uint8List.fromList(bytes);
}

/// The ordered four-sheet fixture (`ordered_fixture` in ooxml.rs).
Uint8List orderedFixture() {
  const workbook =
      '<workbook xmlns="$mainNs" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">'
      '<sheets><sheet name="MEC" sheetId="17" r:id="rId7"/>'
      '<sheet name="CIV" sheetId="3" r:id="rId2"/>'
      '<sheet name="Hidden" sheetId="44" state="hidden" r:id="rId9"/>'
      '<sheet name="VeryHidden" sheetId="45" state="veryHidden" r:id="rId10"/>'
      '</sheets></workbook>';
  const relationships =
      '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
      '<Relationship Id="rId2" Type="$relNs" Target="worksheets/sheet2.xml"/>'
      '<Relationship Id="rId10" Type="$relNs" Target="worksheets/sheet10.xml"/>'
      '<Relationship Id="rId7" Type="$relNs" Target="/xl/worksheets/sheet7.xml"/>'
      '<Relationship Id="rId9" Type="$relNs" Target="worksheets/sheet9.xml"/>'
      '</Relationships>';
  return fixtureFromMembers(workbook, relationships, [
    ('xl/worksheets/sheet2.xml', _b('CIV')),
    ('xl/worksheets/sheet7.xml', _b('MEC')),
    ('xl/worksheets/sheet9.xml', _b('Hidden')),
    ('xl/worksheets/sheet10.xml', _b('VeryHidden')),
    ('docProps/unrelated.bin', _b('unrelated')),
  ]);
}

Uint8List bytesOf(String text) => _b(text);

/// Matcher for an [OoxmlError] of [kind] with exactly [message].
Matcher ooxmlError(String message, {OoxmlErrorKind? kind}) => isA<OoxmlError>()
    .having((error) => error.message, 'message', message)
    .having(
      (error) => error.kind,
      'kind',
      kind ?? anyOf(OoxmlErrorKind.invalid, OoxmlErrorKind.limitExceeded),
    );
