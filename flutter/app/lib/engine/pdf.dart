// Port of src-tauri/src/pdf.rs.
//
// A bounded, byte-preserving PDF reader and incremental writer. It parses
// classic and cross-reference-stream PDFs, checks fidelity before and after
// every write, and appends revisions instead of rewriting source bytes.
//
// Dictionaries are ordered by key bytes exactly like the Rust BTreeMap, so
// serialized object hashes agree with the Rust implementation. Names are held
// as Latin-1 strings (one code unit per byte), which keeps that ordering.
library;

import 'dart:collection';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:archive/archive.dart' show ZLibDecoder;
import 'package:crypto/crypto.dart' show sha256;

import 'cap_fs.dart' show OsHandle, createExclusivePath;

const int maxPdfBytes = 512 * 1024 * 1024;
const int _maxObjects = 1000000;
const int _maxDepth = 128;
const int _maxDecompressedStream = 256 * 1024 * 1024;
const int _u32Max = 0xFFFFFFFF;
const int _u16Max = 0xFFFF;

/// `(object number, generation)`.
typedef ObjectId = (int, int);

/// Port of `type Dictionary = BTreeMap<Vec<u8>, PdfObject>`. Keys are
/// Latin-1 names, so code-unit order equals byte order.
typedef Dictionary = SplayTreeMap<String, PdfObject>;

int _compareIds(ObjectId a, ObjectId b) =>
    a.$1 != b.$1 ? a.$1.compareTo(b.$1) : a.$2.compareTo(b.$2);

SplayTreeMap<ObjectId, V> _idMap<V>() => SplayTreeMap<ObjectId, V>(_compareIds);

Dictionary _dict([Map<String, PdfObject>? values]) {
  final result = Dictionary();
  if (values != null) {
    result.addAll(values);
  }
  return result;
}

Dictionary _copyDict(Dictionary source) => Dictionary()..addAll(source);

// ---------------------------------------------------------------------------
// Objects
// ---------------------------------------------------------------------------

sealed class PdfObject {
  const PdfObject();

  int? get asI64 => switch (this) {
    PdfNumber(:final text) => int.tryParse(text),
    _ => null,
  };

  int? get asUsize {
    final value = asI64;
    return value == null || value < 0 ? null : value;
  }

  String? get asName => switch (this) {
    PdfName(:final name) => name,
    _ => null,
  };

  ObjectId? get asReference => switch (this) {
    PdfReference(:final id) => id,
    _ => null,
  };

  List<PdfObject>? get asArray => switch (this) {
    PdfArray(:final values) => values,
    _ => null,
  };

  Dictionary? get asDictionary => switch (this) {
    PdfDictionary(:final entries) => entries,
    PdfStream(:final dictionary) => dictionary,
    _ => null,
  };
}

final class PdfNull extends PdfObject {
  const PdfNull();
}

final class PdfBool extends PdfObject {
  const PdfBool(this.value);
  final bool value;
}

/// A number kept as its source token, so it re-serializes byte-for-byte.
final class PdfNumber extends PdfObject {
  const PdfNumber(this.text);
  final String text;
}

final class PdfName extends PdfObject {
  const PdfName(this.name);
  final String name;
}

final class PdfString extends PdfObject {
  const PdfString(this.bytes);
  final Uint8List bytes;
}

final class PdfArray extends PdfObject {
  const PdfArray(this.values);
  final List<PdfObject> values;
}

final class PdfDictionary extends PdfObject {
  const PdfDictionary(this.entries);
  final Dictionary entries;
}

final class PdfReference extends PdfObject {
  const PdfReference(this.id);
  final ObjectId id;
}

final class PdfStream extends PdfObject {
  const PdfStream(this.dictionary, this.data);
  final Dictionary dictionary;
  final Uint8List data;
}

bool _containsReference(PdfObject object) => switch (object) {
  PdfReference() => true,
  PdfArray(:final values) => values.any(_containsReference),
  PdfDictionary(:final entries) => entries.values.any(_containsReference),
  PdfStream(:final dictionary) => dictionary.values.any(_containsReference),
  _ => false,
};

sealed class _XrefEntry {
  const _XrefEntry();
}

final class _XrefOffset extends _XrefEntry {
  const _XrefOffset(this.offset, this.generation);
  final int offset;
  final int generation;
}

final class _XrefCompressed extends _XrefEntry {
  const _XrefCompressed(this.stream, this.index);
  final int stream;
  final int index;
}

// ---------------------------------------------------------------------------
// Public facts and errors
// ---------------------------------------------------------------------------

/// Port of `PdfError`. Its message is the Display text; every error is
/// reviewable.
class PdfError implements Exception {
  const PdfError(this.message);

  final String message;

  bool get reviewable => true;

  @override
  String toString() => message;
}

bool _strListEq(List<String> a, List<String> b) {
  if (a.length != b.length) return false;
  for (var i = 0; i < a.length; i++) {
    if (a[i] != b[i]) return false;
  }
  return true;
}

bool _mapEq<V>(Map<String, V> a, Map<String, V> b) {
  if (a.length != b.length) return false;
  for (final entry in a.entries) {
    if (!b.containsKey(entry.key) || b[entry.key] != entry.value) {
      return false;
    }
  }
  return true;
}

/// Port of `PdfPageInspection`.
class PdfPageInspection {
  const PdfPageInspection({
    required this.objectId,
    required this.objectHash,
    required this.mediaBox,
    required this.cropBox,
    required this.rotation,
    required this.contentHash,
    required this.annotations,
    required this.links,
    required this.widgets,
  });

  final ObjectId objectId;
  final String objectHash;
  final List<String> mediaBox;
  final List<String> cropBox;
  final int rotation;
  final String contentHash;
  final int annotations;
  final int links;
  final int widgets;

  @override
  bool operator ==(Object other) =>
      other is PdfPageInspection &&
      other.objectId == objectId &&
      other.objectHash == objectHash &&
      _strListEq(other.mediaBox, mediaBox) &&
      _strListEq(other.cropBox, cropBox) &&
      other.rotation == rotation &&
      other.contentHash == contentHash &&
      other.annotations == annotations &&
      other.links == links &&
      other.widgets == widgets;

  @override
  int get hashCode => Object.hash(objectId, objectHash, rotation, contentHash);

  Map<String, Object?> toJson() => {
    'objectId': [objectId.$1, objectId.$2],
    'objectHash': objectHash,
    'mediaBox': mediaBox,
    'cropBox': cropBox,
    'rotation': rotation,
    'contentHash': contentHash,
    'annotations': annotations,
    'links': links,
    'widgets': widgets,
  };

  factory PdfPageInspection.fromJson(Map<String, Object?> json) {
    final id = json['objectId']! as List<Object?>;
    return PdfPageInspection(
      objectId: ((id[0]! as num).toInt(), (id[1]! as num).toInt()),
      objectHash: json['objectHash']! as String,
      mediaBox: (json['mediaBox']! as List<Object?>).cast<String>(),
      cropBox: (json['cropBox']! as List<Object?>).cast<String>(),
      rotation: (json['rotation']! as num).toInt(),
      contentHash: json['contentHash']! as String,
      annotations: (json['annotations']! as num).toInt(),
      links: (json['links']! as num).toInt(),
      widgets: (json['widgets']! as num).toInt(),
    );
  }
}

/// Port of `PdfInspection`.
class PdfInspection {
  const PdfInspection({
    required this.pageCount,
    required this.pages,
    required this.catalogEntryHashes,
    required this.metadataXmlSelfContained,
    required this.emptyOutlineTree,
    required this.infoHash,
    required this.embeddedFileHashes,
    required this.incrementalRevisions,
    required this.objectStreamCount,
    required this.encrypted,
    required this.signed,
  });

  final int pageCount;
  final List<PdfPageInspection> pages;
  final Map<String, String> catalogEntryHashes;
  final bool metadataXmlSelfContained;
  final bool emptyOutlineTree;
  final String? infoHash;
  final Map<String, String> embeddedFileHashes;
  final int incrementalRevisions;
  final int objectStreamCount;
  final bool encrypted;
  final bool signed;

  @override
  bool operator ==(Object other) {
    if (other is! PdfInspection ||
        other.pageCount != pageCount ||
        other.pages.length != pages.length ||
        other.metadataXmlSelfContained != metadataXmlSelfContained ||
        other.emptyOutlineTree != emptyOutlineTree ||
        other.infoHash != infoHash ||
        other.incrementalRevisions != incrementalRevisions ||
        other.objectStreamCount != objectStreamCount ||
        other.encrypted != encrypted ||
        other.signed != signed ||
        !_mapEq(other.catalogEntryHashes, catalogEntryHashes) ||
        !_mapEq(other.embeddedFileHashes, embeddedFileHashes)) {
      return false;
    }
    for (var i = 0; i < pages.length; i++) {
      if (pages[i] != other.pages[i]) return false;
    }
    return true;
  }

  @override
  int get hashCode => Object.hash(pageCount, infoHash, encrypted, signed);

  Map<String, Object?> toJson() => {
    'pageCount': pageCount,
    'pages': [for (final page in pages) page.toJson()],
    'catalogEntryHashes': catalogEntryHashes,
    'metadataXmlSelfContained': metadataXmlSelfContained,
    'emptyOutlineTree': emptyOutlineTree,
    'infoHash': infoHash,
    'embeddedFileHashes': embeddedFileHashes,
    'incrementalRevisions': incrementalRevisions,
    'objectStreamCount': objectStreamCount,
    'encrypted': encrypted,
    'signed': signed,
  };

  factory PdfInspection.fromJson(Map<String, Object?> json) => PdfInspection(
    pageCount: (json['pageCount']! as num).toInt(),
    pages: [
      for (final page in json['pages']! as List<Object?>)
        PdfPageInspection.fromJson(page! as Map<String, Object?>),
    ],
    catalogEntryHashes: Map<String, String>.from(
      json['catalogEntryHashes']! as Map<Object?, Object?>,
    ),
    metadataXmlSelfContained: json['metadataXmlSelfContained']! as bool,
    emptyOutlineTree: json['emptyOutlineTree']! as bool,
    infoHash: json['infoHash'] as String?,
    embeddedFileHashes: Map<String, String>.from(
      json['embeddedFileHashes']! as Map<Object?, Object?>,
    ),
    incrementalRevisions: (json['incrementalRevisions']! as num).toInt(),
    objectStreamCount: (json['objectStreamCount']! as num).toInt(),
    encrypted: json['encrypted']! as bool,
    signed: json['signed']! as bool,
  );
}

/// Port of `pdf::inspect`.
PdfInspection inspect(String path) => _ParsedPdf.load(path).inspect();

/// Port of `pdf::replace_first_page_incremental`.
void replaceFirstPageIncremental(
  String archivePath,
  String signedPath,
  String outputPath,
) =>
    replaceFirstPageIncrementalFromPage(archivePath, signedPath, 0, outputPath);

/// Port of `pdf::replace_first_page_incremental_from_page`.
void replaceFirstPageIncrementalFromPage(
  String archivePath,
  String signedPath,
  int signedPageIndex,
  String outputPath,
) {
  if (_pathExists(outputPath)) {
    throw PdfError('review destination already exists: $outputPath');
  }
  final archive = _ParsedPdf.load(archivePath);
  final signed = _ParsedPdf.load(signedPath);
  final archiveFacts = archive.inspect();
  final signedFacts = signed.inspect();
  _rejectUnsupported(archiveFacts, 'archive');
  _rejectUnsupported(signedFacts, 'signed scan');
  if (archiveFacts.pageCount == 0 || signedPageIndex >= signedFacts.pageCount) {
    throw const PdfError('PDF has no pages');
  }
  if (archiveFacts.pages[0].widgets > 0) {
    throw const PdfError(
      'archive page one contains a form widget that cannot be safely detached',
    );
  }
  if (signedFacts.pages[signedPageIndex].widgets > 0) {
    throw const PdfError(
      'signed page one contains a form widget without a safely importable field tree',
    );
  }

  final appended = _buildIncrementalReplacement(
    archive,
    signed,
    signedPageIndex,
  );
  _requireParent(
    outputPath,
    'review destination has no parent directory',
    'review destination parent does not exist',
  );
  final bytes = _concat(archive.bytes, appended);
  _publishNew(outputPath, 'review', bytes, () {
    _validateCompleted(
      outputPath,
      archive.bytes,
      archiveFacts,
      signedFacts,
      signedPageIndex,
    );
  });
}

/// Port of `pdf::extract_pages`. Writes a standalone PDF containing only the
/// approved source pages; unselected pages and catalog content are never
/// copied.
void extractPages(String sourcePath, List<int> pageIndexes, String outputPath) {
  if (_pathExists(outputPath)) {
    throw const PdfError('PDF extraction destination already exists');
  }
  if (pageIndexes.isEmpty) {
    throw const PdfError('no PDF pages were approved for extraction');
  }
  final source = _ParsedPdf.load(sourcePath);
  final before = source.inspect();
  if (!extractionSupported(before)) {
    throw const PdfError(
      'PDF contains features that cannot be safely separated',
    );
  }
  final pages = source.pages();
  final selected = <ObjectId>[];
  final unique = <int>{};
  for (final index in pageIndexes) {
    if (index < 0 || index >= pages.length || !unique.add(index)) {
      throw const PdfError('approved PDF page indexes are invalid');
    }
    selected.add(pages[index]);
  }
  final bytes = _buildExtractedPdf(source, selected);
  _requireParent(
    outputPath,
    'PDF extraction destination has no parent',
    'PDF extraction destination parent is missing',
  );
  _publishNew(outputPath, 'PDF extraction', bytes, () {
    _validateExtractedPdf(outputPath, source, selected);
  });
}

/// Port of `extraction_supported` (`pub(crate)`).
bool extractionSupported(PdfInspection facts) {
  const blockedKeys = [
    'StructTreeRoot',
    'OCProperties',
    'AcroForm',
    'Names',
    'PageLabels',
    'OpenAction',
    'ViewerPreferences',
    'Dests',
    'OutputIntents',
  ];
  return !facts.encrypted &&
      !facts.signed &&
      facts.embeddedFileHashes.isEmpty &&
      (!facts.catalogEntryHashes.containsKey('Outlines') ||
          facts.emptyOutlineTree) &&
      !blockedKeys.any(facts.catalogEntryHashes.containsKey) &&
      facts.pages.every((page) => page.annotations == 0 && page.widgets == 0);
}

/// Port of `pdf::rotate_pages_incremental`. Appends page-dictionary revisions
/// that change only the approved display rotations; the source bytes remain an
/// exact prefix of the result.
void rotatePagesIncremental(
  String sourcePath,
  String outputPath,
  List<(int, int)> rotations,
) {
  if (_pathExists(outputPath)) {
    throw PdfError('review destination already exists: $outputPath');
  }
  if (rotations.isEmpty) {
    throw const PdfError('no page rotations were approved');
  }
  final source = _ParsedPdf.load(sourcePath);
  final before = source.inspect();
  if (before.encrypted) {
    throw const PdfError(
      'source PDF is encrypted; orientation repair is intentionally disabled',
    );
  }
  if (before.signed) {
    throw const PdfError(
      'source PDF contains a digital signature; orientation repair would invalidate it',
    );
  }
  final approved = SplayTreeMap<int, int>();
  for (final (pageIndex, rotation) in rotations) {
    if (pageIndex < 0 || pageIndex >= before.pageCount) {
      throw const PdfError('approved page index is out of range');
    }
    if (!const {0, 90, 180, 270}.contains(rotation)) {
      throw const PdfError(
        'approved page rotation must be 0, 90, 180, or 270 degrees',
      );
    }
    if (approved.containsKey(pageIndex)) {
      throw const PdfError('approved page index is duplicated');
    }
    approved[pageIndex] = rotation;
  }
  final appended = _buildIncrementalRotations(source, approved);
  _requireParent(
    outputPath,
    'review destination has no parent directory',
    'review destination parent does not exist',
  );
  final bytes = _concat(source.bytes, appended);
  _publishNew(outputPath, 'review', bytes, () {
    _validateRotated(outputPath, source.bytes, before, approved);
  });
}

// ---------------------------------------------------------------------------
// File publication helpers
// ---------------------------------------------------------------------------

bool _pathExists(String path) =>
    FileSystemEntity.typeSync(path) != FileSystemEntityType.notFound;

void _requireParent(
  String outputPath,
  String missingMessage,
  String notDirMessage,
) {
  final parent = File(outputPath).parent.path;
  if (parent.isEmpty) {
    throw PdfError(missingMessage);
  }
  if (!Directory(parent).existsSync()) {
    throw PdfError(notDirMessage);
  }
}

/// Creates [outputPath] with no-clobber semantics, writes [bytes], then runs
/// [validate]. Any failure after creation removes the output again, matching
/// the Rust `write_result` block.
void _publishNew(
  String outputPath,
  String label,
  Uint8List bytes,
  void Function() validate,
) {
  final handle = _createOutput(outputPath, label);
  try {
    try {
      handle.writeAll(bytes);
      handle.syncAll();
    } catch (error) {
      throw PdfError('$label output write failed: $error');
    } finally {
      handle.close();
    }
    validate();
  } catch (_) {
    _removeQuietly(outputPath);
    rethrow;
  }
}

OsHandle _createOutput(String outputPath, String label) {
  try {
    return createExclusivePath(outputPath);
  } catch (error) {
    throw PdfError('$label output create failed: $error');
  }
}

void _removeQuietly(String path) {
  try {
    File(path).deleteSync();
  } on Object {
    // Mirrors `let _ = fs::remove_file(path)`.
  }
}

Uint8List _readFileBytes(String path) {
  try {
    return File(path).readAsBytesSync();
  } on FileSystemException catch (error) {
    throw PdfError('PDF read failed: $error');
  }
}

Uint8List _readBounded(String path) {
  final type = FileSystemEntity.typeSync(path, followLinks: false);
  if (type == FileSystemEntityType.notFound) {
    throw const PdfError('PDF metadata failed: no such file');
  }
  if (type != FileSystemEntityType.file) {
    throw const PdfError('PDF file type or size is unsupported');
  }
  final length = File(path).lengthSync();
  if (length == 0 || length > maxPdfBytes) {
    throw const PdfError('PDF file type or size is unsupported');
  }
  return _readFileBytes(path);
}

Uint8List _concat(Uint8List first, Uint8List second) {
  final builder = BytesBuilder(copy: false)
    ..add(first)
    ..add(second);
  return builder.takeBytes();
}

// ---------------------------------------------------------------------------
// Parsed document
// ---------------------------------------------------------------------------

class _ParsedPdf {
  _ParsedPdf._({
    required this.bytes,
    required this.entries,
    required this.trailers,
    required this.lastXref,
    required this.revisions,
    required this.maxId,
  });

  final Uint8List bytes;
  final SplayTreeMap<int, _XrefEntry> entries;
  final List<Dictionary> trailers;
  final int lastXref;
  final int revisions;
  final int maxId;

  static _ParsedPdf load(String path) => parse(_readBounded(path));

  static _ParsedPdf parse(Uint8List bytes) {
    if (!_startsWith(bytes, const [0x25, 0x50, 0x44, 0x46, 0x2D])) {
      // "%PDF-"
      throw const PdfError('file is not a PDF');
    }
    final lastXref = _findStartxref(bytes);
    final entries = SplayTreeMap<int, _XrefEntry>();
    final defined = SplayTreeSet<int>();
    final trailers = <Dictionary>[];
    int? current = lastXref;
    final seen = <int>{};
    var revisions = 0;
    while (current != null) {
      final offset = current;
      if (offset >= bytes.length || !seen.add(offset) || revisions > 1024) {
        throw const PdfError('PDF cross-reference chain is invalid');
      }
      final section = _parseXrefSection(bytes, offset);
      for (final number in section.defined) {
        if (defined.add(number)) {
          final entry = section.entries[number];
          if (entry != null) {
            entries[number] = entry;
          }
        }
      }
      current = section.trailer['Prev']?.asUsize;
      trailers.add(section.trailer);
      revisions++;
    }
    if (entries.length > _maxObjects) {
      throw const PdfError('PDF contains too many objects');
    }
    var maxId = 0;
    for (final key in entries.keys) {
      if (key > maxId) maxId = key;
    }
    for (final trailer in trailers) {
      final size = trailer['Size']?.asUsize;
      if (size == null) continue;
      final candidate = size > 0 ? size - 1 : 0;
      if (candidate <= _u32Max && candidate > maxId) {
        maxId = candidate;
      }
    }
    return _ParsedPdf._(
      bytes: bytes,
      entries: entries,
      trailers: trailers,
      lastXref: lastXref,
      revisions: revisions,
      maxId: maxId,
    );
  }

  PdfObject getObject(ObjectId id) =>
      _getObjectResolvingLengths(id, <ObjectId>{});

  PdfObject _getObjectResolvingLengths(ObjectId id, Set<ObjectId> resolving) {
    if (resolving.length > _maxDepth || !resolving.add(id)) {
      throw const PdfError(
        'PDF stream /Length reference is cyclic or too deep',
      );
    }
    final entry = entries[id.$1];
    if (entry == null) {
      throw PdfError('PDF object ${id.$1} is missing');
    }
    final PdfObject result;
    switch (entry) {
      case _XrefOffset(:final offset, :final generation):
        if (generation != id.$2) {
          throw PdfError('PDF object ${id.$1} generation is inconsistent');
        }
        final parsed = _parseIndirectObject(bytes, offset, (lengthId) {
          final value = _getObjectResolvingLengths(lengthId, resolving);
          final length = value.asUsize;
          if (length == null) {
            throw const PdfError(
              'PDF stream indirect /Length is not a non-negative integer',
            );
          }
          return length;
        });
        if (parsed.id != id) {
          throw PdfError(
            'PDF cross-reference for object ${id.$1} is inconsistent',
          );
        }
        result = parsed.object;
      case _XrefCompressed(:final stream, :final index):
        if (id.$2 != 0) {
          throw const PdfError('compressed PDF object has a generation');
        }
        result = _getCompressedObject(stream, index, id.$1, resolving);
    }
    resolving.remove(id);
    return result;
  }

  PdfObject _getCompressedObject(
    int streamId,
    int index,
    int expectedNumber,
    Set<ObjectId> resolving,
  ) {
    final stream = _getObjectResolvingLengths((streamId, 0), resolving);
    if (stream is! PdfStream) {
      throw const PdfError('object stream reference is not a stream');
    }
    final dictionary = stream.dictionary;
    if (dictionary['Type']?.asName != 'ObjStm') {
      throw const PdfError('compressed object container is not /ObjStm');
    }
    final count = dictionary['N']?.asUsize;
    if (count == null) {
      throw const PdfError('object stream has no valid /N');
    }
    final first = dictionary['First']?.asUsize;
    if (first == null) {
      throw const PdfError('object stream has no valid /First');
    }
    if (index >= count) {
      throw const PdfError('compressed object index is invalid');
    }
    final decoded = _decodeStream(dictionary, stream.data);
    if (first > decoded.length) {
      throw const PdfError('object stream header is invalid');
    }
    final parser = _Parser(Uint8List.sublistView(decoded, 0, first));
    final offsets = <(int, int)>[];
    for (var i = 0; i < count; i++) {
      final number = parser.readUnsigned();
      final offset = parser.readUnsigned();
      if (number > _u32Max) {
        throw const PdfError('object number overflow');
      }
      offsets.add((number, offset));
    }
    if (offsets[index].$1 != expectedNumber) {
      throw const PdfError('object stream index does not match xref');
    }
    final start = first + offsets[index].$2;
    final end = index + 1 < offsets.length
        ? first + offsets[index + 1].$2
        : decoded.length;
    if (start > end || end > decoded.length) {
      throw const PdfError('object stream object bounds are invalid');
    }
    return _Parser(Uint8List.sublistView(decoded, start, end)).parseObject(0);
  }

  ObjectId? _rootIdOrNull() {
    for (final trailer in trailers) {
      final root = trailer['Root'];
      if (root != null) {
        return root.asReference;
      }
    }
    return null;
  }

  ObjectId rootId() {
    final id = _rootIdOrNull();
    if (id == null) {
      throw const PdfError('PDF catalog reference is missing');
    }
    return id;
  }

  List<ObjectId> pages() {
    final root = getObject(rootId());
    final pagesRef = root.asDictionary?['Pages']?.asReference;
    if (pagesRef == null) {
      throw const PdfError('PDF page tree root is missing');
    }
    final result = <ObjectId>[];
    _walkPages(pagesRef, result, <ObjectId>{}, 0);
    return result;
  }

  void _walkPages(
    ObjectId id,
    List<ObjectId> result,
    Set<ObjectId> visiting,
    int depth,
  ) {
    if (depth > _maxDepth || !visiting.add(id)) {
      throw const PdfError('PDF page tree is cyclic or too deep');
    }
    final dictionary = getObject(id).asDictionary;
    if (dictionary == null) {
      throw const PdfError('PDF page tree object is not a dictionary');
    }
    final type = dictionary['Type']?.asName;
    if (type == 'Page') {
      result.add(id);
    } else if (type == 'Pages' || type == null) {
      final kids = dictionary['Kids']?.asArray;
      if (kids == null) {
        throw const PdfError('PDF page tree node has no /Kids');
      }
      for (final kid in kids) {
        final kidId = kid.asReference;
        if (kidId == null) {
          throw const PdfError('PDF page tree child is not indirect');
        }
        _walkPages(kidId, result, visiting, depth + 1);
      }
    } else {
      throw const PdfError('PDF page tree contains a non-page object');
    }
    visiting.remove(id);
  }

  PdfObject? _inheritedValue(ObjectId page, String key) {
    ObjectId? current = page;
    final seen = <ObjectId>{};
    while (current != null) {
      final id = current;
      if (!seen.add(id) || seen.length > _maxDepth) {
        throw const PdfError('PDF page inheritance chain is invalid');
      }
      final dictionary = getObject(id).asDictionary;
      if (dictionary == null) {
        throw const PdfError('PDF page ancestor is not a dictionary');
      }
      final value = dictionary[key];
      if (value != null) {
        return value;
      }
      current = dictionary['Parent']?.asReference;
    }
    return null;
  }

  PdfInspection inspect() {
    final encrypted = trailers.any((trailer) => trailer.containsKey('Encrypt'));
    if (encrypted) {
      return PdfInspection(
        pageCount: 0,
        pages: const [],
        catalogEntryHashes: const {},
        metadataXmlSelfContained: false,
        emptyOutlineTree: false,
        infoHash: null,
        embeddedFileHashes: const {},
        incrementalRevisions: revisions,
        objectStreamCount: _objectStreamCount(),
        encrypted: true,
        signed: _hasSignatureShallow(),
      );
    }
    final signed = _hasSignature();
    final pageIds = pages();
    final pageFacts = <PdfPageInspection>[
      for (final id in pageIds) inspectPage(id),
    ];
    final root = getObject(rootId());
    final catalog = root.asDictionary;
    if (catalog == null) {
      throw const PdfError('PDF catalog is not a dictionary');
    }
    final metadataSelfContained = _metadataXmlSelfContained(catalog);
    final emptyOutline = _emptyOutlineTree(catalog);
    final catalogHashes = SplayTreeMap<String, String>();
    for (final key in const [
      'Metadata',
      'Names',
      'Outlines',
      'PageLabels',
      'AcroForm',
      'OpenAction',
      'ViewerPreferences',
      'Dests',
      'StructTreeRoot',
      'OCProperties',
      'OutputIntents',
    ]) {
      final value = catalog[key];
      if (value == null) continue;
      final String hash;
      if (key == 'Metadata') {
        if (value is PdfReference) {
          hash = _metadataGraphHash(getObject(value.id));
        } else {
          hash = _metadataGraphHash(value);
        }
      } else {
        hash = _graphHash(value);
      }
      catalogHashes[key] = hash;
    }
    String? infoHash;
    for (final trailer in trailers) {
      final info = trailer['Info'];
      if (info != null) {
        infoHash = _graphHash(info);
        break;
      }
    }
    return PdfInspection(
      pageCount: pageFacts.length,
      pages: pageFacts,
      catalogEntryHashes: catalogHashes,
      metadataXmlSelfContained: metadataSelfContained,
      emptyOutlineTree: emptyOutline,
      infoHash: infoHash,
      embeddedFileHashes: _embeddedFiles(catalog),
      incrementalRevisions: revisions,
      objectStreamCount: _objectStreamCount(),
      encrypted: encrypted,
      signed: signed,
    );
  }

  bool _metadataXmlSelfContained(Dictionary catalog) {
    final value = catalog['Metadata'];
    if (value is! PdfReference) return false;
    final object = getObject(value.id);
    if (object is! PdfStream) return false;
    final dictionary = object.dictionary;
    final isXmlMetadata =
        dictionary['Type']?.asName == 'Metadata' &&
        dictionary['Subtype']?.asName == 'XML';
    return isXmlMetadata && !dictionary.values.any(_containsReference);
  }

  bool _emptyOutlineTree(Dictionary catalog) {
    final value = catalog['Outlines'];
    if (value == null) return true;
    final id = value.asReference;
    if (id == null) return false;
    final dictionary = getObject(id).asDictionary;
    if (dictionary == null) return false;
    return dictionary['Type']?.asName == 'Outlines' &&
        dictionary['Count']?.asI64 == 0 &&
        !dictionary.containsKey('First') &&
        !dictionary.containsKey('Last');
  }

  PdfPageInspection inspectPage(ObjectId id) {
    final object = getObject(id);
    final dictionary = object.asDictionary;
    if (dictionary == null) {
      throw const PdfError('PDF page is not a dictionary');
    }
    final mediaBox = _boxValues(_inheritedValue(id, 'MediaBox'));
    if (mediaBox == null) {
      throw const PdfError('PDF page has no valid media box');
    }
    final cropBox = _boxValues(_inheritedValue(id, 'CropBox')) ?? mediaBox;
    final rotation = _inheritedValue(id, 'Rotate')?.asI64 ?? 0;
    final contents = dictionary['Contents'];
    final contentHash = contents == null
        ? _sha256Hex(Uint8List(0))
        : _contentHash(contents);
    final annotationsValue = dictionary['Annots'];
    final annotations = annotationsValue == null
        ? const <PdfObject>[]
        : _resolveArray(annotationsValue);
    var links = 0;
    var widgets = 0;
    for (final annotation in annotations) {
      final resolved = _resolve(annotation);
      final subtype = resolved.asDictionary?['Subtype']?.asName;
      if (subtype == 'Link') links++;
      if (subtype == 'Widget') widgets++;
    }
    return PdfPageInspection(
      objectId: id,
      objectHash: _objectHash(id),
      mediaBox: mediaBox,
      cropBox: cropBox,
      rotation: rotation,
      contentHash: contentHash,
      annotations: annotations.length,
      links: links,
      widgets: widgets,
    );
  }

  PdfObject _resolve(PdfObject object) =>
      object is PdfReference ? getObject(object.id) : object;

  List<PdfObject> _resolveArray(PdfObject object) {
    final values = _resolve(object).asArray;
    if (values == null) {
      throw const PdfError('PDF object is not an array');
    }
    return values;
  }

  String _contentHash(PdfObject object) {
    final builder = BytesBuilder(copy: false);
    for (final value in _resolveArrayOrSingle(object)) {
      final resolved = _resolve(value);
      if (resolved is! PdfStream) {
        throw const PdfError('PDF page content is not a stream');
      }
      builder
        ..add(_u64be(resolved.data.length))
        ..add(resolved.data);
    }
    return _sha256Hex(builder.takeBytes());
  }

  List<PdfObject> _resolveArrayOrSingle(PdfObject object) {
    final resolved = _resolve(object);
    return switch (resolved) {
      PdfArray(:final values) => values,
      _ => [resolved],
    };
  }

  String _objectHash(ObjectId id) {
    final out = BytesBuilder(copy: false);
    _serializeObject(getObject(id), out, 0);
    return _sha256Hex(out.takeBytes());
  }

  String _graphHash(PdfObject object) {
    final out = BytesBuilder(copy: false);
    _hashGraph(object, out, <ObjectId>{}, 0);
    return _sha256Hex(out.takeBytes());
  }

  String _metadataGraphHash(PdfObject object) {
    final out = BytesBuilder(copy: false);
    _hashMetadataGraph(object, out, <ObjectId>{}, 0);
    return _sha256Hex(out.takeBytes());
  }

  void _hashMetadataGraph(
    PdfObject object,
    BytesBuilder out,
    Set<ObjectId> seen,
    int depth,
  ) {
    if (depth > _maxDepth) {
      throw const PdfError('PDF metadata graph is too deeply nested');
    }
    switch (object) {
      case PdfReference(:final id):
        _addText(out, 'reference');
        if (seen.add(id)) {
          _hashMetadataGraph(getObject(id), out, seen, depth + 1);
        } else {
          _addText(out, 'already-visited');
        }
      case PdfArray(:final values):
        _addText(out, 'array');
        for (final value in values) {
          _hashMetadataGraph(value, out, seen, depth + 1);
        }
      case PdfDictionary(:final entries):
        _addText(out, 'dictionary');
        for (final entry in entries.entries) {
          _addLengthPrefixed(out, _latin1Bytes(entry.key));
          _hashMetadataGraph(entry.value, out, seen, depth + 1);
        }
      case PdfStream(:final dictionary, :final data):
        _addText(out, 'stream');
        out
          ..add(_u64be(data.length))
          ..add(data);
        for (final entry in dictionary.entries) {
          if (entry.key == 'Length') continue;
          _addLengthPrefixed(out, _latin1Bytes(entry.key));
          _hashMetadataGraph(entry.value, out, seen, depth + 1);
        }
      default:
        final bytes = BytesBuilder(copy: false);
        _serializeObject(object, bytes, depth);
        _addLengthPrefixed(out, bytes.takeBytes());
    }
  }

  void _hashGraph(
    PdfObject object,
    BytesBuilder out,
    Set<ObjectId> seen,
    int depth,
  ) {
    if (depth > _maxDepth) {
      throw const PdfError('PDF object graph is too deeply nested');
    }
    final bytes = BytesBuilder(copy: false);
    _serializeObject(object, bytes, depth);
    _addLengthPrefixed(out, bytes.takeBytes());
    switch (object) {
      case PdfReference(:final id) when seen.add(id):
        final referenced = getObject(id);
        final type = referenced.asDictionary?['Type']?.asName;
        final isPageTreeObject = type == 'Page' || type == 'Pages';
        if (!isPageTreeObject) {
          _hashGraph(referenced, out, seen, depth + 1);
        }
      case PdfArray(:final values):
        for (final value in values) {
          _hashGraph(value, out, seen, depth + 1);
        }
      case PdfDictionary(:final entries):
        for (final value in entries.values) {
          _hashGraph(value, out, seen, depth + 1);
        }
      case PdfStream(:final dictionary):
        for (final value in dictionary.values) {
          _hashGraph(value, out, seen, depth + 1);
        }
      default:
        break;
    }
  }

  Map<String, String> _embeddedFiles(Dictionary catalog) {
    final namesValue = catalog['Names'];
    if (namesValue == null) return {};
    final names = _resolve(namesValue);
    final embedded = names.asDictionary?['EmbeddedFiles'];
    if (embedded == null) return {};
    final pairs = <(String, PdfObject)>[];
    _collectNameTree(embedded, pairs, 0);
    final result = <String, String>{};
    for (final (name, fileSpecObject) in pairs) {
      final fileSpec = _resolve(fileSpecObject);
      PdfObject? embeddedValue;
      final fileSpecDict = fileSpec.asDictionary;
      if (fileSpecDict != null) {
        final ef = fileSpecDict['EF'];
        if (ef != null) {
          final efDict = _resolve(ef).asDictionary;
          if (efDict != null) {
            embeddedValue = efDict['UF'] ?? efDict['F'];
          }
        }
      }
      if (embeddedValue == null) continue;
      final stream = _resolve(embeddedValue);
      if (stream is! PdfStream) {
        throw const PdfError('embedded file is not a stream');
      }
      result[name] = _sha256Hex(stream.data);
    }
    return result;
  }

  void _collectNameTree(
    PdfObject node,
    List<(String, PdfObject)> output,
    int depth,
  ) {
    if (depth > _maxDepth) {
      throw const PdfError('PDF name tree is too deeply nested');
    }
    final resolved = _resolve(node);
    final dictionary = resolved.asDictionary;
    if (dictionary == null) {
      throw const PdfError('PDF name tree node is not a dictionary');
    }
    final names = dictionary['Names'];
    if (names != null) {
      final values = _resolveArray(names);
      if (values.length.isOdd) {
        throw const PdfError('PDF name tree has an odd entry count');
      }
      for (var i = 0; i + 1 < values.length; i += 2) {
        output.add((_objectText(values[i]), values[i + 1]));
      }
    }
    final kids = dictionary['Kids'];
    if (kids != null) {
      for (final kid in _resolveArray(kids)) {
        _collectNameTree(kid, output, depth + 1);
      }
    }
  }

  int _objectStreamCount() {
    var count = 0;
    for (final entry in entries.entries) {
      final offsetEntry = entry.value;
      if (offsetEntry is! _XrefOffset) continue;
      final id = (entry.key, offsetEntry.generation);
      try {
        if (getObject(id).asDictionary?['Type']?.asName == 'ObjStm') {
          count++;
        }
      } on PdfError {
        // Matches `get_object(id).ok()`: unreadable objects do not count.
      }
    }
    return count;
  }

  bool _hasSignatureShallow() => trailers.any(
    (trailer) =>
        trailer.containsKey('Perms') || trailer.containsKey('ByteRange'),
  );

  bool _hasSignature() {
    final root = getObject(rootId());
    if (root.asDictionary?.containsKey('Perms') ?? false) {
      return true;
    }
    for (final entry in entries.entries) {
      final value = entry.value;
      final id = switch (value) {
        _XrefOffset(:final generation) => (entry.key, generation),
        _XrefCompressed() => (entry.key, 0),
      };
      final object = getObject(id);
      if (_containsSignatureMarker(object, 0)) {
        return true;
      }
    }
    return false;
  }
}

/// `contains_signature_marker`. Errors inside the walk count as a signature
/// (`unwrap_or(true)`), so an unreadable object fails closed.
bool _containsSignatureMarker(PdfObject object, int depth) {
  if (depth > _maxDepth) {
    throw const PdfError('PDF signature object is too deeply nested');
  }
  bool nested(PdfObject value) {
    try {
      return _containsSignatureMarker(value, depth + 1);
    } on PdfError {
      return true;
    }
  }

  final dictionary = object.asDictionary;
  if (dictionary != null) {
    return dictionary.containsKey('ByteRange') ||
        dictionary['Type']?.asName == 'Sig' ||
        dictionary['FT']?.asName == 'Sig' ||
        dictionary.values.any(nested);
  }
  if (object is PdfArray) {
    return object.values.any(nested);
  }
  return false;
}

// ---------------------------------------------------------------------------
// Page-level validation after writes
// ---------------------------------------------------------------------------

void _validateRotated(
  String outputPath,
  Uint8List sourceBytes,
  PdfInspection before,
  SplayTreeMap<int, int> rotations,
) {
  final outputBytes = _readBounded(outputPath);
  if (!_startsWithBytes(outputBytes, sourceBytes)) {
    throw const PdfError(
      'rotated PDF did not preserve the source revision bytes',
    );
  }
  final after = _ParsedPdf.parse(outputBytes).inspect();
  if (after.pageCount != before.pageCount ||
      after.incrementalRevisions != before.incrementalRevisions + 1 ||
      after.objectStreamCount != before.objectStreamCount ||
      !_mapEq(after.catalogEntryHashes, before.catalogEntryHashes) ||
      after.infoHash != before.infoHash ||
      !_mapEq(after.embeddedFileHashes, before.embeddedFileHashes) ||
      after.encrypted != before.encrypted ||
      after.signed != before.signed) {
    throw const PdfError(
      'rotated PDF failed document-structure fidelity validation',
    );
  }
  for (var index = 0; index < before.pages.length; index++) {
    final old = before.pages[index];
    final updated = after.pages[index];
    final expectedRotation = rotations[index] ?? old.rotation;
    if (updated.objectId != old.objectId ||
        !_strListEq(updated.mediaBox, old.mediaBox) ||
        !_strListEq(updated.cropBox, old.cropBox) ||
        updated.rotation != expectedRotation ||
        updated.contentHash != old.contentHash ||
        updated.annotations != old.annotations ||
        updated.links != old.links ||
        updated.widgets != old.widgets ||
        (!rotations.containsKey(index) &&
            updated.objectHash != old.objectHash)) {
      throw const PdfError('rotated PDF failed page-fidelity validation');
    }
  }
}

void _rejectUnsupported(PdfInspection facts, String label) {
  if (facts.encrypted) {
    throw PdfError(
      '$label PDF is encrypted; staging is intentionally disabled',
    );
  }
  if (facts.signed) {
    throw PdfError(
      '$label PDF contains a digital signature; staging is intentionally disabled',
    );
  }
  if (facts.catalogEntryHashes.containsKey('StructTreeRoot')) {
    throw PdfError(
      '$label PDF is tagged; staging cannot safely preserve the page structure tree',
    );
  }
  if (label == 'signed scan' &&
      facts.catalogEntryHashes.containsKey('OCProperties')) {
    throw const PdfError(
      'signed scan uses document-level optional-content state that cannot be imported safely',
    );
  }
}

void _validateCompleted(
  String outputPath,
  Uint8List archiveBytes,
  PdfInspection archive,
  PdfInspection signed,
  int signedPageIndex,
) {
  final outputBytes = _readBounded(outputPath);
  if (!_startsWithBytes(outputBytes, archiveBytes)) {
    throw const PdfError(
      'completed PDF did not preserve the archive revision bytes',
    );
  }
  final completed = _ParsedPdf.parse(outputBytes).inspect();
  if (completed.encrypted || completed.signed) {
    throw const PdfError(
      'completed PDF unexpectedly changed its encryption/signature policy',
    );
  }
  if (completed.pageCount != archive.pageCount ||
      completed.incrementalRevisions != archive.incrementalRevisions + 1 ||
      completed.objectStreamCount != archive.objectStreamCount ||
      !_mapEq(completed.catalogEntryHashes, archive.catalogEntryHashes) ||
      completed.infoHash != archive.infoHash ||
      !_mapEq(completed.embeddedFileHashes, archive.embeddedFileHashes) ||
      !_pageListEq(
        completed.pages.length > 1 ? completed.pages.sublist(1) : const [],
        archive.pages.length > 1 ? archive.pages.sublist(1) : const [],
      )) {
    throw const PdfError(
      'completed PDF failed archive-structure fidelity validation',
    );
  }
  if (completed.pages.isEmpty) {
    throw const PdfError('completed PDF has no first page');
  }
  final completedFirst = completed.pages.first;
  final signedSelected = signed.pages[signedPageIndex];
  if (!_strListEq(completedFirst.mediaBox, signedSelected.mediaBox) ||
      !_strListEq(completedFirst.cropBox, signedSelected.cropBox) ||
      completedFirst.rotation != signedSelected.rotation ||
      completedFirst.contentHash != signedSelected.contentHash ||
      completedFirst.annotations != signedSelected.annotations ||
      completedFirst.links != signedSelected.links ||
      completedFirst.widgets != signedSelected.widgets) {
    throw const PdfError('completed PDF failed signed first-page validation');
  }
}

bool _pageListEq(List<PdfPageInspection> a, List<PdfPageInspection> b) {
  if (a.length != b.length) return false;
  for (var i = 0; i < a.length; i++) {
    if (a[i] != b[i]) return false;
  }
  return true;
}

void _validateExtractedPdf(
  String path,
  _ParsedPdf source,
  List<ObjectId> selected,
) {
  final before = source.inspect();
  final expected = [for (final id in selected) source.inspectPage(id)];
  final after = _ParsedPdf.load(path).inspect();
  final metadataMatches =
      after.catalogEntryHashes['Metadata'] ==
      before.catalogEntryHashes['Metadata'];
  var pagesMatch = true;
  for (var i = 0; i < after.pages.length && i < expected.length; i++) {
    final actual = after.pages[i];
    final wanted = expected[i];
    if (actual.contentHash != wanted.contentHash ||
        !_strListEq(actual.mediaBox, wanted.mediaBox) ||
        !_strListEq(actual.cropBox, wanted.cropBox) ||
        actual.rotation != wanted.rotation ||
        actual.annotations != wanted.annotations ||
        actual.links != wanted.links ||
        actual.widgets != wanted.widgets) {
      pagesMatch = false;
    }
  }
  if (after.pageCount != selected.length || !metadataMatches || !pagesMatch) {
    throw const PdfError('extracted PDF failed page-fidelity validation');
  }
}

// ---------------------------------------------------------------------------
// Writers
// ---------------------------------------------------------------------------

Uint8List _buildExtractedPdf(_ParsedPdf source, List<ObjectId> selected) {
  final allPages = source.pages();
  final selectedTargets = <ObjectId, ObjectId>{};
  for (var index = 0; index < selected.length; index++) {
    selectedTargets[selected[index]] = (index + 3, 0);
  }
  final importer = _Importer(
    source: source,
    targetPages: Map<ObjectId, ObjectId>.of(selectedTargets),
    sourcePageIds: allPages.toSet(),
    nextId: selected.length + 3,
  );
  final writes = _idMap<PdfObject>();
  for (final sourcePage in selected) {
    final dictionary = source.getObject(sourcePage).asDictionary;
    if (dictionary == null) {
      throw const PdfError('selected PDF page is not a dictionary');
    }
    final page = _copyDict(dictionary);
    _materializeInheritedPageValues(source, sourcePage, page);
    page.remove('Parent');
    final imported = importer.importObject(PdfDictionary(page), 0);
    if (imported is! PdfDictionary) {
      throw const PdfError('selected PDF page import changed object type');
    }
    final importedPage = _copyDict(imported.entries)
      ..['Parent'] = const PdfReference((2, 0));
    writes[selectedTargets[sourcePage]!] = PdfDictionary(importedPage);
  }
  final trailerInfo = source.trailers.isEmpty
      ? null
      : source.trailers.first['Info'];
  final info = trailerInfo == null
      ? null
      : importer.importObject(trailerInfo, 0);
  final rootCatalog = source.getObject(source.rootId()).asDictionary;
  final metadataValue = rootCatalog?['Metadata'];
  final metadata = metadataValue == null
      ? null
      : importer.importObject(metadataValue, 0);
  writes.addAll(importer.objects);
  final kids = [
    for (final page in selected) PdfReference(selectedTargets[page]!),
  ];
  final pages = _dict({
    'Type': const PdfName('Pages'),
    'Kids': PdfArray(kids),
    'Count': PdfNumber('${selected.length}'),
  });
  writes[(2, 0)] = PdfDictionary(pages);
  final catalog = _dict({
    'Type': const PdfName('Catalog'),
    'Pages': const PdfReference((2, 0)),
  });
  if (metadata != null) {
    catalog['Metadata'] = metadata;
  }
  writes[(1, 0)] = PdfDictionary(catalog);
  if (writes.length > _maxObjects) {
    throw const PdfError('extracted PDF object graph is too large');
  }

  final bytes = BytesBuilder(copy: false)
    ..add(_latin1Bytes('%PDF-1.7\n%'))
    ..add(const [0xE2, 0xE3, 0xCF, 0xD3])
    ..add(_latin1Bytes('\n'));
  final offsets = _idMap<int>();
  for (final entry in writes.entries) {
    offsets[entry.key] = bytes.length;
    bytes.add(_latin1Bytes('${entry.key.$1} ${entry.key.$2} obj\n'));
    _serializeObject(entry.value, bytes, 0);
    bytes.add(_latin1Bytes('\nendobj\n'));
  }
  final xref = bytes.length;
  final size =
      (offsets.keys.isEmpty
          ? 0
          : offsets.keys.map((id) => id.$1).reduce((a, b) => a > b ? a : b)) +
      1;
  bytes.add(_latin1Bytes('xref\n0 $size\n0000000000 65535 f \n'));
  for (var number = 1; number < size; number++) {
    final offset = offsets[(number, 0)];
    if (offset == null) {
      throw const PdfError('extracted PDF object roster is incomplete');
    }
    bytes.add(_latin1Bytes('${_pad(offset, 10)} 00000 n \n'));
  }
  final trailer = _dict({
    'Size': PdfNumber('$size'),
    'Root': const PdfReference((1, 0)),
  });
  if (info != null) {
    trailer['Info'] = info;
  }
  bytes.add(_latin1Bytes('trailer\n'));
  _serializeObject(PdfDictionary(trailer), bytes, 0);
  bytes.add(_latin1Bytes('\nstartxref\n$xref\n%%EOF\n'));
  return bytes.takeBytes();
}

Uint8List _buildIncrementalRotations(
  _ParsedPdf source,
  SplayTreeMap<int, int> rotations,
) {
  final pages = source.pages();
  final writes = _idMap<PdfObject>();
  for (final entry in rotations.entries) {
    if (entry.key >= pages.length) {
      throw const PdfError('approved page index is out of range');
    }
    final pageId = pages[entry.key];
    final dictionary = source.getObject(pageId).asDictionary;
    if (dictionary == null) {
      throw const PdfError('PDF page is not a dictionary');
    }
    final page = _copyDict(dictionary)
      ..['Rotate'] = PdfNumber('${entry.value}');
    writes[pageId] = PdfDictionary(page);
  }
  return _buildIncrementalAppendix(source, writes, source.maxId + 1);
}

Uint8List _buildIncrementalReplacement(
  _ParsedPdf archive,
  _ParsedPdf signed,
  int signedPageIndex,
) {
  final archivePages = archive.pages();
  final signedPages = signed.pages();
  if (archivePages.isEmpty) {
    throw const PdfError('archive PDF has no pages');
  }
  final archivePage = archivePages.first;
  if (signedPageIndex >= signedPages.length) {
    throw const PdfError('signed PDF page index is out of range');
  }
  final signedPage = signedPages[signedPageIndex];
  final archiveParent = archive
      .getObject(archivePage)
      .asDictionary?['Parent']
      ?.asReference;
  if (archiveParent == null) {
    throw const PdfError('archive page one has no valid parent');
  }

  final pageDictionary = signed.getObject(signedPage).asDictionary;
  if (pageDictionary == null) {
    throw const PdfError('signed page one is not a page dictionary');
  }
  final page = _copyDict(pageDictionary);
  _materializeInheritedPageValues(signed, signedPage, page);
  page.remove('Parent');

  final importer = _Importer(
    source: signed,
    targetPages: {signedPage: archivePage},
    sourcePageIds: signedPages.toSet(),
    nextId: archive.maxId + 1,
  );
  final imported = importer.importObject(PdfDictionary(page), 0);
  if (imported is! PdfDictionary) {
    throw const PdfError('signed page import changed object type');
  }
  final importedPage = _copyDict(imported.entries)
    ..['Parent'] = PdfReference(archiveParent);

  final writes = _idMap<PdfObject>();
  writes[archivePage] = PdfDictionary(importedPage);
  writes.addAll(importer.objects);
  if (writes.length > _maxObjects) {
    throw const PdfError('signed page object graph is too large');
  }
  var maxWritten = archive.maxId;
  for (final id in writes.keys) {
    if (id.$1 > maxWritten) maxWritten = id.$1;
  }
  return _buildIncrementalAppendix(archive, writes, maxWritten + 1);
}

/// Shared tail of `build_incremental_appendix` and the replacement builder:
/// appends the objects, one-entry xref subsections, and a trailer that chains
/// to the previous xref with [size] as its /Size.
Uint8List _buildIncrementalAppendix(
  _ParsedPdf source,
  SplayTreeMap<ObjectId, PdfObject> writes,
  int size,
) {
  if (writes.isEmpty || writes.length > _maxObjects) {
    throw const PdfError('PDF incremental write set is invalid');
  }
  final endsWithNewline = source.bytes.isNotEmpty && source.bytes.last == 0x0A;
  final baseOffset = source.bytes.length + (endsWithNewline ? 0 : 1);
  final appendix = BytesBuilder(copy: false);
  if (!endsWithNewline) {
    appendix.addByte(0x0A);
  }
  final offsets = _idMap<int>();
  for (final entry in writes.entries) {
    offsets[entry.key] = baseOffset + appendix.length;
    appendix.add(_latin1Bytes('${entry.key.$1} ${entry.key.$2} obj\n'));
    _serializeObject(entry.value, appendix, 0);
    appendix.add(_latin1Bytes('\nendobj\n'));
  }
  final xrefOffset = baseOffset + appendix.length;
  appendix.add(_latin1Bytes('xref\n'));
  for (final entry in offsets.entries) {
    final (number, generation) = entry.key;
    appendix.add(
      _latin1Bytes(
        '$number 1\n${_pad(entry.value, 10)} ${_pad(generation, 5)} n \n',
      ),
    );
  }
  final latest = source.trailers.isEmpty ? null : source.trailers.first;
  if (latest == null) {
    throw const PdfError('source PDF has no trailer');
  }
  final trailer = _copyDict(latest);
  for (final key in const [
    'Size',
    'Prev',
    'XRefStm',
    'Encrypt',
    'Type',
    'W',
    'Index',
    'Length',
    'Filter',
    'DecodeParms',
  ]) {
    trailer.remove(key);
  }
  trailer['Size'] = PdfNumber('$size');
  trailer['Prev'] = PdfNumber('${source.lastXref}');
  appendix.add(_latin1Bytes('trailer\n'));
  _serializeObject(PdfDictionary(trailer), appendix, 0);
  appendix.add(_latin1Bytes('\nstartxref\n$xrefOffset\n%%EOF\n'));
  return appendix.takeBytes();
}

void _materializeInheritedPageValues(
  _ParsedPdf pdf,
  ObjectId page,
  Dictionary dictionary,
) {
  for (final key in const ['Resources', 'MediaBox', 'CropBox', 'Rotate']) {
    if (!dictionary.containsKey(key)) {
      final value = pdf._inheritedValue(page, key);
      if (value != null) {
        dictionary[key] = value;
      }
    }
  }
  if (!dictionary.containsKey('MediaBox')) {
    throw const PdfError('signed page has no media box');
  }
}

class _Importer {
  _Importer({
    required this.source,
    required this.targetPages,
    required this.sourcePageIds,
    required this.nextId,
  });

  final _ParsedPdf source;
  final Map<ObjectId, ObjectId> targetPages;
  final Set<ObjectId> sourcePageIds;
  int nextId;
  final Map<ObjectId, ObjectId> _assigned = {};
  final SplayTreeMap<ObjectId, PdfObject> objects = _idMap<PdfObject>();
  final Set<ObjectId> _visiting = {};

  PdfObject importObject(PdfObject object, int depth) {
    if (depth > _maxDepth) {
      throw const PdfError('signed page object graph is too deeply nested');
    }
    switch (object) {
      case PdfReference(:final id) when targetPages.containsKey(id):
        return PdfReference(targetPages[id]!);
      case PdfReference(:final id) when sourcePageIds.contains(id):
        throw const PdfError(
          'signed page references another signed-scan page that will not be staged',
        );
      case PdfReference(:final id):
        final existing = _assigned[id];
        if (existing != null) {
          return PdfReference(existing);
        }
        if (_assigned.length >= _maxObjects) {
          throw const PdfError('signed page object graph is too large');
        }
        final mapped = (nextId, 0);
        if (nextId >= _u32Max) {
          throw const PdfError('PDF object identity overflow');
        }
        nextId++;
        _assigned[id] = mapped;
        if (!_visiting.add(id)) {
          return PdfReference(mapped);
        }
        final imported = importObject(source.getObject(id), depth + 1);
        objects[mapped] = imported;
        _visiting.remove(id);
        return PdfReference(mapped);
      case PdfArray(:final values):
        return PdfArray([
          for (final value in values) importObject(value, depth + 1),
        ]);
      case PdfDictionary(:final entries):
        final result = _dict();
        for (final entry in entries.entries) {
          result[entry.key] = importObject(entry.value, depth + 1);
        }
        return PdfDictionary(result);
      case PdfStream(:final dictionary, :final data):
        final result = _dict();
        for (final entry in dictionary.entries) {
          if (entry.key == 'Length') continue;
          result[entry.key] = importObject(entry.value, depth + 1);
        }
        return PdfStream(result, data);
      default:
        return object;
    }
  }
}

// ---------------------------------------------------------------------------
// Serialization
// ---------------------------------------------------------------------------

void _serializeObject(PdfObject object, BytesBuilder output, int depth) {
  if (depth > _maxDepth) {
    throw const PdfError('PDF object is too deeply nested to serialize');
  }
  switch (object) {
    case PdfNull():
      output.add(_latin1Bytes('null'));
    case PdfBool(:final value):
      output.add(_latin1Bytes(value ? 'true' : 'false'));
    case PdfNumber(:final text):
      output.add(_latin1Bytes(text));
    case PdfName(:final name):
      _serializeName(name, output);
    case PdfString(:final bytes):
      output.addByte(0x3C);
      for (final byte in bytes) {
        output.add(
          _latin1Bytes(byte.toRadixString(16).toUpperCase().padLeft(2, '0')),
        );
      }
      output.addByte(0x3E);
    case PdfArray(:final values):
      output.addByte(0x5B);
      for (var index = 0; index < values.length; index++) {
        if (index > 0) output.addByte(0x20);
        _serializeObject(values[index], output, depth + 1);
      }
      output.addByte(0x5D);
    case PdfDictionary(:final entries):
      _serializeDictionary(entries, output, depth + 1);
    case PdfReference(:final id):
      output.add(_latin1Bytes('${id.$1} ${id.$2} R'));
    case PdfStream(:final dictionary, :final data):
      final withLength = _copyDict(dictionary)
        ..['Length'] = PdfNumber('${data.length}');
      _serializeDictionary(withLength, output, depth + 1);
      output.add(_latin1Bytes('\nstream\n'));
      output.add(data);
      output.add(_latin1Bytes('\nendstream'));
  }
}

void _serializeDictionary(
  Dictionary dictionary,
  BytesBuilder output,
  int depth,
) {
  output.add(_latin1Bytes('<<'));
  for (final entry in dictionary.entries) {
    output.addByte(0x20);
    _serializeName(entry.key, output);
    output.addByte(0x20);
    _serializeObject(entry.value, output, depth + 1);
  }
  output.add(_latin1Bytes(' >>'));
}

void _serializeName(String name, BytesBuilder output) {
  output.addByte(0x2F);
  for (final unit in name.codeUnits) {
    if (unit < 33 || unit > 126 || _isDelimiter(unit) || unit == 0x23) {
      output.add(
        _latin1Bytes(
          '#${unit.toRadixString(16).toUpperCase().padLeft(2, '0')}',
        ),
      );
    } else {
      output.addByte(unit);
    }
  }
}

// ---------------------------------------------------------------------------
// Cross-reference and object parsing
// ---------------------------------------------------------------------------

class _XrefSection {
  _XrefSection(this.entries, this.defined, this.trailer);
  final SplayTreeMap<int, _XrefEntry> entries;
  final SplayTreeSet<int> defined;
  final Dictionary trailer;
}

_XrefSection _parseXrefSection(Uint8List bytes, int offset) {
  final parser = _Parser.at(bytes, offset);
  parser.skipSpaceAndComments();
  if (parser.consumeKeyword('xref')) {
    return _parseClassicXref(parser);
  }
  final indirect = _parseIndirectObject(
    bytes,
    offset,
    (_) => throw const PdfError(
      'xref stream entries, including /Length, must be direct objects',
    ),
  );
  final stream = indirect.object;
  if (stream is! PdfStream) {
    throw const PdfError('startxref does not point to xref data');
  }
  final dictionary = stream.dictionary;
  if (dictionary['Type']?.asName != 'XRef') {
    throw const PdfError('startxref stream is not /XRef');
  }
  final wValue = dictionary['W'];
  if (wValue == null) {
    throw const PdfError('xref stream has no /W');
  }
  final widths = _integerArray(wValue);
  if (widths.length != 3 || widths.fold<int>(0, (a, b) => a + b) == 0) {
    throw const PdfError('xref stream /W is invalid');
  }
  final size = dictionary['Size']?.asUsize;
  if (size == null) {
    throw const PdfError('xref stream has no valid /Size');
  }
  final indexValue = dictionary['Index'];
  final indexes = indexValue == null
      ? <int>[0, size]
      : _integerArray(indexValue);
  if (indexes.length.isOdd) {
    throw const PdfError('xref stream /Index is invalid');
  }
  final decoded = _decodeStream(dictionary, stream.data);
  final entryWidth = widths.fold<int>(0, (a, b) => a + b);
  var expectedCount = 0;
  for (var i = 0; i + 1 < indexes.length; i += 2) {
    expectedCount += indexes[i + 1];
  }
  final expected = expectedCount * entryWidth;
  if (decoded.length < expected) {
    throw const PdfError('xref stream data is truncated');
  }
  final entries = SplayTreeMap<int, _XrefEntry>();
  final defined = SplayTreeSet<int>();
  var cursor = 0;
  for (var pair = 0; pair + 1 < indexes.length; pair += 2) {
    final start = indexes[pair];
    final count = indexes[pair + 1];
    for (var number = start; number < start + count; number++) {
      final kind = widths[0] == 0 ? 1 : _readBe(decoded, cursor, widths[0]);
      cursor += widths[0];
      final first = _readBe(decoded, cursor, widths[1]);
      cursor += widths[1];
      final second = _readBe(decoded, cursor, widths[2]);
      cursor += widths[2];
      if (number > _u32Max) {
        throw const PdfError('xref object number overflow');
      }
      defined.add(number);
      switch (kind) {
        case 1:
          if (second > _u16Max) {
            throw const PdfError('xref generation overflow');
          }
          entries[number] = _XrefOffset(first, second);
        case 2:
          if (first > _u32Max) {
            throw const PdfError('object stream number overflow');
          }
          entries[number] = _XrefCompressed(first, second);
        case 0:
          break;
        default:
          throw const PdfError('xref stream entry type is invalid');
      }
    }
  }
  return _XrefSection(entries, defined, dictionary);
}

_XrefSection _parseClassicXref(_Parser parser) {
  final entries = SplayTreeMap<int, _XrefEntry>();
  final defined = SplayTreeSet<int>();
  while (true) {
    parser.skipSpaceAndComments();
    if (parser.consumeKeyword('trailer')) {
      final trailer = parser.parseObject(0);
      if (trailer is! PdfDictionary) {
        throw const PdfError('classic xref trailer is not a dictionary');
      }
      if (trailer.entries.containsKey('XRefStm')) {
        throw const PdfError(
          'hybrid-reference PDF cross-reference sections are unsupported',
        );
      }
      return _XrefSection(entries, defined, trailer.entries);
    }
    final first = parser.readUnsigned();
    final count = parser.readUnsigned();
    for (var index = 0; index < count; index++) {
      final offset = parser.readUnsigned();
      final generation = parser.readUnsigned();
      final status = parser.readWord();
      final number = first + index;
      if (number > _u32Max) {
        throw const PdfError('xref object number overflow');
      }
      defined.add(number);
      if (status == 'n') {
        if (generation > _u16Max) {
          throw const PdfError('xref generation overflow');
        }
        entries[number] = _XrefOffset(offset, generation);
      } else if (status != 'f') {
        throw const PdfError('classic xref entry status is invalid');
      }
    }
  }
}

class _IndirectObject {
  _IndirectObject(this.id, this.object);
  final ObjectId id;
  final PdfObject object;
}

_IndirectObject _parseIndirectObject(
  Uint8List bytes,
  int offset,
  int Function(ObjectId) resolveLength,
) {
  final parser = _Parser.at(bytes, offset);
  final number = parser.readUnsigned();
  if (number > _u32Max) {
    throw const PdfError('PDF object number overflow');
  }
  final generation = parser.readUnsigned();
  if (generation > _u16Max) {
    throw const PdfError('PDF object generation overflow');
  }
  if (!parser.consumeKeyword('obj')) {
    throw const PdfError('xref offset does not point to an indirect object');
  }
  var object = parser.parseObject(0);
  if (object is PdfDictionary) {
    parser.skipSpaceAndComments();
    if (parser.consumeKeyword('stream')) {
      parser.consumeStreamEol();
      final lengthObject = object.entries['Length'];
      if (lengthObject == null) {
        throw const PdfError('PDF stream has no /Length');
      }
      final int length;
      final direct = lengthObject.asUsize;
      if (direct != null) {
        length = direct;
      } else if (lengthObject.asReference case final reference?) {
        length = resolveLength(reference);
      } else {
        throw const PdfError(
          'PDF stream /Length is not a non-negative integer or reference',
        );
      }
      final end = parser.position + length;
      if (end > bytes.length) {
        throw const PdfError('PDF stream length exceeds the file');
      }
      final data = Uint8List.fromList(bytes.sublist(parser.position, end));
      parser.position = end;
      parser.skipSpaceAndComments();
      if (!parser.consumeKeyword('endstream')) {
        throw const PdfError('PDF stream is missing endstream');
      }
      object = PdfStream(object.entries, data);
    }
  }
  parser.skipSpaceAndComments();
  if (!parser.consumeKeyword('endobj')) {
    throw const PdfError('PDF indirect object is missing endobj');
  }
  return _IndirectObject((number, generation), object);
}

class _Parser {
  _Parser(this.bytes, [this.position = 0]);

  factory _Parser.at(Uint8List bytes, int position) {
    if (position > bytes.length) {
      throw const PdfError('PDF parser offset exceeds the file');
    }
    return _Parser(bytes, position);
  }

  final Uint8List bytes;
  int position;

  PdfObject parseObject(int depth) {
    if (depth > _maxDepth) {
      throw const PdfError('PDF object nesting is too deep');
    }
    skipSpaceAndComments();
    final byte = _peek();
    if (byte == null) {
      throw const PdfError('unexpected end of PDF object');
    }
    if (byte == 0x2F) {
      return PdfName(parseName());
    }
    if (byte == 0x28) {
      return PdfString(Uint8List.fromList(parseLiteralString()));
    }
    if (byte == 0x5B) {
      return parseArray(depth + 1);
    }
    if (byte == 0x3C && _byteAt(position + 1) == 0x3C) {
      return parseDictionary(depth + 1);
    }
    if (byte == 0x3C) {
      return PdfString(Uint8List.fromList(parseHexString()));
    }
    if (byte == 0x6E && consumeKeyword('null')) {
      return const PdfNull();
    }
    if (byte == 0x74 && consumeKeyword('true')) {
      return const PdfBool(true);
    }
    if (byte == 0x66 && consumeKeyword('false')) {
      return const PdfBool(false);
    }
    if (byte == 0x2B ||
        byte == 0x2D ||
        byte == 0x2E ||
        (byte >= 0x30 && byte <= 0x39)) {
      return _parseNumberOrReference();
    }
    throw const PdfError('unsupported or malformed PDF object token');
  }

  PdfObject _parseNumberOrReference() {
    final first = readNumberToken();
    final saved = position;
    if (!first.contains('.') && !first.contains('+') && !first.contains('-')) {
      skipSpaceAndComments();
      final next = _peek();
      if (next != null && next >= 0x30 && next <= 0x39) {
        final second = readNumberToken();
        skipSpaceAndComments();
        if (consumeKeyword('R')) {
          final number = int.tryParse(first);
          if (number == null || number > _u32Max) {
            throw const PdfError('PDF reference number is invalid');
          }
          final generation = int.tryParse(second);
          if (generation == null || generation > _u16Max) {
            throw const PdfError('PDF reference generation is invalid');
          }
          return PdfReference((number, generation));
        }
      }
    }
    position = saved;
    return PdfNumber(first);
  }

  PdfObject parseArray(int depth) {
    position += 1;
    final values = <PdfObject>[];
    while (true) {
      skipSpaceAndComments();
      if (_peek() == 0x5D) {
        position += 1;
        return PdfArray(values);
      }
      values.add(parseObject(depth));
      if (values.length > _maxObjects) {
        throw const PdfError('PDF array is too large');
      }
    }
  }

  PdfObject parseDictionary(int depth) {
    position += 2;
    final values = _dict();
    while (true) {
      skipSpaceAndComments();
      if (_sliceEquals(position, const [0x3E, 0x3E])) {
        position += 2;
        return PdfDictionary(values);
      }
      final key = parseName();
      final value = parseObject(depth);
      values[key] = value;
      if (values.length > _maxObjects) {
        throw const PdfError('PDF dictionary is too large');
      }
    }
  }

  String parseName() {
    if (_peek() != 0x2F) {
      throw const PdfError('PDF dictionary key is not a name');
    }
    position += 1;
    final value = <int>[];
    while (true) {
      final byte = _peek();
      if (byte == null || _isWhitespace(byte) || _isDelimiter(byte)) {
        break;
      }
      if (byte == 0x23) {
        final end = position + 3;
        if (end > bytes.length) {
          throw const PdfError('PDF name escape is truncated');
        }
        value.add(_hexByte(bytes.sublist(position + 1, end)));
        position += 3;
      } else {
        value.add(byte);
        position += 1;
      }
    }
    return String.fromCharCodes(value);
  }

  List<int> parseLiteralString() {
    position += 1;
    var depth = 1;
    final value = <int>[];
    while (true) {
      final byte = next();
      if (byte == null) {
        throw const PdfError('PDF literal string is unterminated');
      }
      switch (byte) {
        case 0x28:
          depth += 1;
          value.add(byte);
        case 0x29:
          depth -= 1;
          if (depth == 0) {
            return value;
          }
          value.add(byte);
        case 0x5C:
          final escaped = next();
          if (escaped == null) {
            throw const PdfError('PDF literal string escape is truncated');
          }
          switch (escaped) {
            case 0x6E:
              value.add(0x0A);
            case 0x72:
              value.add(0x0D);
            case 0x74:
              value.add(0x09);
            case 0x62:
              value.add(8);
            case 0x66:
              value.add(12);
            case 0x28 || 0x29 || 0x5C:
              value.add(escaped);
            case 0x0D:
              if (_peek() == 0x0A) {
                position += 1;
              }
            case 0x0A:
              break;
            case >= 0x30 && <= 0x37:
              var octal = escaped - 0x30;
              for (var i = 0; i < 2; i++) {
                final digit = _peek();
                if (digit != null && digit >= 0x30 && digit <= 0x37) {
                  position += 1;
                  octal = octal * 8 + (digit - 0x30);
                } else {
                  break;
                }
              }
              value.add(octal & 0xFF);
            default:
              value.add(escaped);
          }
        default:
          value.add(byte);
      }
    }
  }

  List<int> parseHexString() {
    position += 1;
    final digits = <int>[];
    while (true) {
      final byte = next();
      if (byte == null) {
        throw const PdfError('PDF hex string is unterminated');
      }
      if (byte == 0x3E) break;
      if (!_isWhitespace(byte)) {
        if (!_isHexDigit(byte)) {
          throw const PdfError('PDF hex string contains invalid data');
        }
        digits.add(byte);
      }
    }
    if (digits.length.isOdd) {
      digits.add(0x30);
    }
    final result = <int>[];
    for (var i = 0; i < digits.length; i += 2) {
      result.add(_hexByte(digits.sublist(i, i + 2)));
    }
    return result;
  }

  int readUnsigned() {
    skipSpaceAndComments();
    final start = position;
    while (_isDigitAt(position)) {
      position += 1;
    }
    if (start == position) {
      throw const PdfError('expected unsigned PDF integer');
    }
    final value = int.tryParse(
      String.fromCharCodes(bytes.sublist(start, position)),
    );
    if (value == null) {
      throw const PdfError('PDF unsigned integer is invalid');
    }
    return value;
  }

  String readNumberToken() {
    skipSpaceAndComments();
    final start = position;
    final sign = _peek();
    if (sign == 0x2B || sign == 0x2D) {
      position += 1;
    }
    var digits = 0;
    while (_isDigitAt(position)) {
      digits += 1;
      position += 1;
    }
    if (_peek() == 0x2E) {
      position += 1;
      while (_isDigitAt(position)) {
        digits += 1;
        position += 1;
      }
    }
    if (digits == 0) {
      throw const PdfError('PDF number is invalid');
    }
    return String.fromCharCodes(bytes.sublist(start, position));
  }

  String readWord() {
    skipSpaceAndComments();
    final start = position;
    while (true) {
      final byte = _peek();
      if (byte == null || _isWhitespace(byte) || _isDelimiter(byte)) {
        break;
      }
      position += 1;
    }
    if (start == position) {
      throw const PdfError('expected PDF keyword');
    }
    return String.fromCharCodes(bytes.sublist(start, position));
  }

  bool consumeKeyword(String keyword) {
    skipSpaceAndComments();
    final units = keyword.codeUnits;
    if (!_sliceEquals(position, units)) {
      return false;
    }
    final end = position + units.length;
    final after = _byteAt(end);
    if (after != null && !_isWhitespace(after) && !_isDelimiter(after)) {
      return false;
    }
    position = end;
    return true;
  }

  void consumeStreamEol() {
    final first = next();
    final second = _peek();
    if (first == 0x0D && second == 0x0A) {
      position += 1;
      return;
    }
    if (first == 0x0D || first == 0x0A) {
      return;
    }
    throw const PdfError('PDF stream keyword has no line ending');
  }

  void skipSpaceAndComments() {
    while (true) {
      while (_peek() != null && _isWhitespace(_peek()!)) {
        position += 1;
      }
      if (_peek() != 0x25) {
        break;
      }
      while (_peek() != null && _peek() != 0x0D && _peek() != 0x0A) {
        position += 1;
      }
    }
  }

  int? _peek() => position < bytes.length ? bytes[position] : null;

  int? _byteAt(int index) => index < bytes.length ? bytes[index] : null;

  int? next() {
    final value = _peek();
    if (value == null) return null;
    position += 1;
    return value;
  }

  bool _isDigitAt(int index) {
    final byte = _byteAt(index);
    return byte != null && byte >= 0x30 && byte <= 0x39;
  }

  bool _sliceEquals(int start, List<int> expected) {
    if (start + expected.length > bytes.length) return false;
    for (var i = 0; i < expected.length; i++) {
      if (bytes[start + i] != expected[i]) return false;
    }
    return true;
  }
}

// ---------------------------------------------------------------------------
// Streams and predictors
// ---------------------------------------------------------------------------

Uint8List _decodeStream(Dictionary dictionary, Uint8List data) {
  final filterValue = dictionary['Filter'];
  final List<String> filters;
  if (filterValue == null) {
    filters = const [];
  } else if (filterValue is PdfName) {
    filters = [filterValue.name];
  } else if (filterValue is PdfArray) {
    filters = [
      for (final value in filterValue.values)
        value.asName ??
            (throw const PdfError('PDF stream filter array is invalid')),
    ];
  } else {
    throw const PdfError('PDF stream filter is invalid');
  }
  var decoded = data;
  for (final filter in filters) {
    if (filter == 'FlateDecode' || filter == 'Fl') {
      final List<int> next;
      try {
        next = const ZLibDecoder().decodeBytes(decoded);
      } on Object catch (error) {
        throw PdfError('PDF Flate stream is invalid: $error');
      }
      if (next.length > _maxDecompressedStream) {
        throw const PdfError('decompressed PDF stream exceeds the limit');
      }
      decoded = Uint8List.fromList(next);
    } else {
      throw PdfError(
        'unsupported PDF stream filter /${utf8.decode(latin1.encode(filter), allowMalformed: true)}',
      );
    }
  }
  return _applyPredictor(dictionary, decoded);
}

Uint8List _applyPredictor(Dictionary dictionary, Uint8List data) {
  final parametersValue = dictionary['DecodeParms'];
  if (parametersValue == null) {
    return data;
  }
  final Dictionary parameters;
  if (parametersValue is PdfDictionary) {
    parameters = parametersValue.entries;
  } else if (parametersValue is PdfArray &&
      parametersValue.values.length == 1) {
    final inner = parametersValue.values[0].asDictionary;
    if (inner == null) {
      throw const PdfError('PDF DecodeParms is invalid');
    }
    parameters = inner;
  } else if (parametersValue is PdfNull) {
    return data;
  } else {
    throw const PdfError('PDF DecodeParms is invalid');
  }
  final predictor = parameters['Predictor']?.asUsize ?? 1;
  if (predictor <= 1) {
    return data;
  }
  if (predictor < 10 || predictor > 15) {
    throw const PdfError('unsupported PDF stream predictor');
  }
  final columns = parameters['Columns']?.asUsize ?? 1;
  final colors = parameters['Colors']?.asUsize ?? 1;
  final bits = parameters['BitsPerComponent']?.asUsize ?? 8;
  if (bits != 8) {
    throw const PdfError('unsupported PDF predictor bit depth');
  }
  final bytesPerPixel = colors < 1 ? 1 : colors;
  final rowBytes = columns * colors;
  final stride = rowBytes + 1;
  if (stride == 0 || data.length % stride != 0) {
    throw const PdfError('PDF predictor data is truncated');
  }
  final output = BytesBuilder(copy: false);
  var previous = Uint8List(rowBytes);
  for (var offset = 0; offset < data.length; offset += stride) {
    final filter = data[offset];
    final row = Uint8List.fromList(data.sublist(offset + 1, offset + stride));
    for (var index = 0; index < rowBytes; index++) {
      final left = index >= bytesPerPixel ? row[index - bytesPerPixel] : 0;
      final up = previous[index];
      final upperLeft = index >= bytesPerPixel
          ? previous[index - bytesPerPixel]
          : 0;
      row[index] = switch (filter) {
        0 => row[index],
        1 => (row[index] + left) & 0xFF,
        2 => (row[index] + up) & 0xFF,
        3 => (row[index] + ((left + up) ~/ 2)) & 0xFF,
        4 => (row[index] + _paeth(left, up, upperLeft)) & 0xFF,
        _ => throw const PdfError('PDF predictor filter is invalid'),
      };
    }
    output.add(row);
    previous = row;
  }
  return output.takeBytes();
}

int _paeth(int left, int up, int upperLeft) {
  final estimate = left + up - upperLeft;
  final leftDistance = (estimate - left).abs();
  final upDistance = (estimate - up).abs();
  final upperLeftDistance = (estimate - upperLeft).abs();
  if (leftDistance <= upDistance && leftDistance <= upperLeftDistance) {
    return left;
  }
  if (upDistance <= upperLeftDistance) {
    return up;
  }
  return upperLeft;
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

int _findStartxref(Uint8List bytes) {
  const marker = [0x73, 0x74, 0x61, 0x72, 0x74, 0x78, 0x72, 0x65, 0x66];
  int? position;
  for (var i = bytes.length - marker.length; i >= 0; i--) {
    var matches = true;
    for (var j = 0; j < marker.length; j++) {
      if (bytes[i + j] != marker[j]) {
        matches = false;
        break;
      }
    }
    if (matches) {
      position = i;
      break;
    }
  }
  if (position == null) {
    throw const PdfError('PDF startxref is missing');
  }
  final parser = _Parser.at(bytes, position + marker.length);
  return parser.readUnsigned();
}

List<int> _integerArray(PdfObject object) {
  final values = object.asArray;
  if (values == null) {
    throw const PdfError('PDF integer array is invalid');
  }
  return [
    for (final value in values)
      value.asUsize ??
          (throw const PdfError('PDF integer array contains a non-integer')),
  ];
}

List<String>? _boxValues(PdfObject? object) {
  final values = object?.asArray;
  if (values == null || values.length != 4) {
    return null;
  }
  final result = <String>[];
  for (final value in values) {
    if (value is! PdfNumber) return null;
    result.add(value.text);
  }
  return result;
}

String _objectText(PdfObject object) => switch (object) {
  PdfString(:final bytes) => utf8.decode(bytes, allowMalformed: true),
  PdfName(:final name) => utf8.decode(
    latin1.encode(name),
    allowMalformed: true,
  ),
  _ => '',
};

int _readBe(Uint8List bytes, int start, int width) {
  var value = 0;
  for (var i = 0; i < width; i++) {
    value = (value << 8) | bytes[start + i];
  }
  return value;
}

int _hexByte(List<int> bytes) {
  if (bytes.length != 2) {
    throw const PdfError('PDF hex byte is truncated');
  }
  final high = _hexValue(bytes[0]);
  final low = _hexValue(bytes[1]);
  if (high == null || low == null) {
    throw const PdfError('PDF hex byte is invalid');
  }
  return high * 16 + low;
}

int? _hexValue(int byte) {
  if (byte >= 0x30 && byte <= 0x39) return byte - 0x30;
  if (byte >= 0x61 && byte <= 0x66) return byte - 0x61 + 10;
  if (byte >= 0x41 && byte <= 0x46) return byte - 0x41 + 10;
  return null;
}

bool _isHexDigit(int byte) => _hexValue(byte) != null;

bool _isWhitespace(int byte) =>
    byte == 0 ||
    byte == 0x09 ||
    byte == 0x0A ||
    byte == 12 ||
    byte == 0x0D ||
    byte == 0x20;

bool _isDelimiter(int byte) => switch (byte) {
  0x28 ||
  0x29 ||
  0x3C ||
  0x3E ||
  0x5B ||
  0x5D ||
  0x7B ||
  0x7D ||
  0x2F ||
  0x25 => true,
  _ => false,
};

bool _startsWith(Uint8List bytes, List<int> prefix) {
  if (bytes.length < prefix.length) return false;
  for (var i = 0; i < prefix.length; i++) {
    if (bytes[i] != prefix[i]) return false;
  }
  return true;
}

bool _startsWithBytes(Uint8List bytes, Uint8List prefix) {
  if (bytes.length < prefix.length) return false;
  for (var i = 0; i < prefix.length; i++) {
    if (bytes[i] != prefix[i]) return false;
  }
  return true;
}

String _sha256Hex(Uint8List bytes) => sha256.convert(bytes).toString();

Uint8List _u64be(int value) {
  final data = ByteData(8)..setUint64(0, value);
  return data.buffer.asUint8List();
}

Uint8List _latin1Bytes(String text) => Uint8List.fromList(latin1.encode(text));

void _addText(BytesBuilder out, String text) => out.add(_latin1Bytes(text));

void _addLengthPrefixed(BytesBuilder out, List<int> bytes) {
  out
    ..add(_u64be(bytes.length))
    ..add(bytes);
}

String _pad(int value, int width) => value.toString().padLeft(width, '0');
