part of '../ooxml.dart';

// ZIP container layer. Port of preflight_archive / validate_archive /
// read_entry / has_entry / write_package from ooxml.rs.
//
// The whole package is held as bytes (the Rust side caps packages at
// 256 MiB). Untouched members are copied as their original compressed bytes
// (raw copy), so their content is byte-identical after a round trip.

const String _cp437High =
    'ÇüéâäàåçêëèïîìÄÅÉæÆôöòûùÿÖÜ¢£¥₧ƒáíóúñÑªº¿⌐¬½¼¡«»'
    '░▒▓│┤╡╢╖╕╣║╗╝╜╛┐└┴┬├─┼╞╟╚╔╩╦╠═╬╧╨╤╥╙╘╒╓╫╪┘┌█▄▌▐▀'
    'αßΓπΣσµτΦΘΩδ∞φε∩≡±≥≤⌠⌡÷≈°∙·√ⁿ²■ ';

class _ZipCentralEntry {
  _ZipCentralEntry({
    required this.name,
    required this.rawName,
    required this.versionMadeBy,
    required this.versionNeeded,
    required this.flags,
    required this.method,
    required this.time,
    required this.date,
    required this.crc,
    required this.compressedSize,
    required this.uncompressedSize,
    required this.internalAttributes,
    required this.externalAttributes,
    required this.localOffset,
    required this.extra,
    required this.comment,
  });

  final String name;
  final Uint8List rawName;
  final int versionMadeBy;
  final int versionNeeded;
  final int flags;
  final int method;
  final int time;
  final int date;
  final int crc;
  final int compressedSize;
  final int uncompressedSize;
  final int internalAttributes;
  final int externalAttributes;
  final int localOffset;
  final Uint8List extra;
  final Uint8List comment;

  bool get isEncrypted => (flags & 0x1) != 0;
}

class _ZipPreflight {
  const _ZipPreflight(this.entryCount, this.centralDirectoryStart, this.comment);
  final int entryCount;
  final int centralDirectoryStart;
  final Uint8List comment;
}

class _ZipPackage {
  _ZipPackage._(
    this.bytes,
    this.comment,
    this.entries,
    this.centralDirectoryStart,
  );

  final Uint8List bytes;
  final Uint8List comment;
  final List<_ZipCentralEntry> entries;
  final int centralDirectoryStart;

  int get length => entries.length;

  _ZipCentralEntry? byName(String name) {
    for (final entry in entries) {
      if (entry.name == name) return entry;
    }
    return null;
  }

  bool hasEntry(String name) => byName(name) != null;

  Uint8List _compressedBytes(_ZipCentralEntry entry) {
    final header = entry.localOffset;
    final nameLen = _u16(bytes, header + 26);
    final extraLen = _u16(bytes, header + 28);
    final start = header + 30 + nameLen + extraLen;
    final end = start + entry.compressedSize;
    // Member data must stay before the central directory.
    if (end > centralDirectoryStart) {
      throw const OoxmlError.invalid('workbook ZIP metadata is invalid');
    }
    return Uint8List.sublistView(bytes, start, end);
  }

  /// Port of read_entry: decoded member bytes, size and CRC verified.
  Uint8List read(String name) {
    final entry = byName(name);
    if (entry == null) {
      throw OoxmlError.invalid('workbook member not found: $name');
    }
    if (entry.uncompressedSize > _maxEntryBytes) {
      throw const OoxmlError.limitExceeded(
        'workbook member exceeded the size limit',
      );
    }
    final data = _decode(entry);
    if (data.length > _maxEntryBytes) {
      throw const OoxmlError.limitExceeded(
        'workbook member exceeded the size limit',
      );
    }
    return data;
  }

  Uint8List _decode(_ZipCentralEntry entry) {
    if (entry.isEncrypted) {
      throw const OoxmlError.invalid('workbook ZIP entry is invalid');
    }
    final compressed = _compressedBytes(entry);
    final Uint8List data;
    switch (entry.method) {
      case 0:
        data = Uint8List.fromList(compressed);
      case 8:
        data = Uint8List.fromList(
          Inflate(compressed, entry.uncompressedSize).getBytes(),
        );
      default:
        throw const OoxmlError.invalid('workbook ZIP entry is invalid');
    }
    if (data.length != entry.uncompressedSize || getCrc32(data) != entry.crc) {
      throw const OoxmlError.invalid('workbook ZIP entry is invalid');
    }
    return data;
  }
}

int _u16(Uint8List bytes, int offset) =>
    bytes[offset] | (bytes[offset + 1] << 8);

int _u32(Uint8List bytes, int offset) =>
    bytes[offset] |
    (bytes[offset + 1] << 8) |
    (bytes[offset + 2] << 16) |
    (bytes[offset + 3] << 24);

bool _sigAt(Uint8List bytes, int offset, List<int> signature) {
  if (offset < 0 || offset + signature.length > bytes.length) return false;
  for (var i = 0; i < signature.length; i++) {
    if (bytes[offset + i] != signature[i]) return false;
  }
  return true;
}

const _eocdSignature = [0x50, 0x4b, 0x05, 0x06];
const _centralHeaderSignature = [0x50, 0x4b, 0x01, 0x02];
const _localHeaderSignature = [0x50, 0x4b, 0x03, 0x04];

/// Port of reject_zip64_or_malformed_extra over an in-memory buffer.
void _rejectZip64OrMalformedExtra(Uint8List bytes, int start, int end) {
  const zip64ExtraHeaderId = 0x0001;
  var cursor = start;
  while (cursor < end) {
    final headerEnd = cursor + 4;
    if (headerEnd > end) {
      throw const OoxmlError.invalid('workbook ZIP metadata is invalid');
    }
    final headerId = _u16(bytes, cursor);
    final dataLen = _u16(bytes, cursor + 2);
    if (headerId == zip64ExtraHeaderId) {
      throw const OoxmlError.invalid('workbook ZIP metadata is invalid');
    }
    cursor = headerEnd + dataLen;
    if (cursor > end) {
      throw const OoxmlError.invalid('workbook ZIP metadata is invalid');
    }
  }
}

/// Port of preflight_archive.
_ZipPreflight _preflightArchive(Uint8List bytes) {
  const eocdMinBytes = 22;
  const eocdMaxCommentBytes = 0xFFFF;
  const centralHeaderBytes = 46;

  final sourceLen = bytes.length;
  if (sourceLen == 0) {
    throw const OoxmlError.invalid('workbook source is empty');
  }
  if (sourceLen > maxPackageBytes) {
    throw const OoxmlError.limitExceeded(
      'workbook source exceeded the size limit',
    );
  }
  final tailLen = math.min(sourceLen, eocdMinBytes + eocdMaxCommentBytes);
  final tailStart = sourceLen - tailLen;
  int? eocdOffset;
  for (var i = tailLen - 4; i >= 0; i--) {
    if (_sigAt(bytes, tailStart + i, _eocdSignature)) {
      eocdOffset = i;
      break;
    }
  }
  if (eocdOffset == null) {
    throw const OoxmlError.invalid('workbook ZIP metadata is invalid');
  }
  final eocd = tailStart + eocdOffset;
  if (tailLen - eocdOffset < eocdMinBytes) {
    throw const OoxmlError.invalid('workbook ZIP metadata is invalid');
  }
  final commentLen = _u16(bytes, eocd + 20);
  if (eocdOffset + eocdMinBytes + commentLen != tailLen) {
    throw const OoxmlError.invalid('workbook ZIP metadata is invalid');
  }
  final diskNumber = _u16(bytes, eocd + 4);
  final directoryDisk = _u16(bytes, eocd + 6);
  final entriesOnDisk = _u16(bytes, eocd + 8);
  final entryCount = _u16(bytes, eocd + 10);
  final directorySize = _u32(bytes, eocd + 12);
  final directoryOffset = _u32(bytes, eocd + 16);
  if (diskNumber != 0 || directoryDisk != 0 || entriesOnDisk != entryCount) {
    throw const OoxmlError.invalid('workbook ZIP metadata is invalid');
  }
  if (entryCount > _maxEntries) {
    throw const OoxmlError.limitExceeded('workbook ZIP entry count is invalid');
  }
  if (entryCount == 0xFFFF || directorySize == 0xFFFFFFFF || directoryOffset == 0xFFFFFFFF) {
    throw const OoxmlError.invalid('workbook ZIP metadata is invalid');
  }
  final absoluteEocd = eocd;
  final directoryEnd = directoryOffset + directorySize;
  if (directoryEnd > absoluteEocd) {
    throw const OoxmlError.invalid('workbook ZIP metadata is invalid');
  }
  if (entryCount > 0) {
    final minimumDirectorySize = entryCount * centralHeaderBytes;
    if (directorySize < minimumDirectorySize ||
        directoryEnd != absoluteEocd ||
        directoryOffset >= absoluteEocd) {
      throw const OoxmlError.invalid('workbook ZIP metadata is invalid');
    }
    var cursor = directoryOffset;
    for (var index = 0; index < entryCount; index++) {
      final fixedEnd = cursor + centralHeaderBytes;
      if (fixedEnd > directoryEnd) {
        throw const OoxmlError.invalid('workbook ZIP metadata is invalid');
      }
      if (!_sigAt(bytes, cursor, _centralHeaderSignature)) {
        throw const OoxmlError.invalid('workbook ZIP metadata is invalid');
      }
      final compressedSize = _u32(bytes, cursor + 20);
      final uncompressedSize = _u32(bytes, cursor + 24);
      final fileNameLen = _u16(bytes, cursor + 28);
      final extraFieldLen = _u16(bytes, cursor + 30);
      final commentLenEntry = _u16(bytes, cursor + 32);
      final diskNumberStart = _u16(bytes, cursor + 34);
      final localHeaderOffset = _u32(bytes, cursor + 42);
      if (diskNumberStart != 0 ||
          compressedSize == 0xFFFFFFFF ||
          uncompressedSize == 0xFFFFFFFF ||
          localHeaderOffset == 0xFFFFFFFF) {
        throw const OoxmlError.invalid('workbook ZIP metadata is invalid');
      }
      final fileNameEnd = fixedEnd + fileNameLen;
      if (fileNameEnd > directoryEnd) {
        throw const OoxmlError.invalid('workbook ZIP metadata is invalid');
      }
      final extraFieldEnd = fileNameEnd + extraFieldLen;
      if (extraFieldEnd > directoryEnd) {
        throw const OoxmlError.invalid('workbook ZIP metadata is invalid');
      }
      cursor = extraFieldEnd + commentLenEntry;
      if (cursor > directoryEnd) {
        throw const OoxmlError.invalid('workbook ZIP metadata is invalid');
      }
      _rejectZip64OrMalformedExtra(bytes, fileNameEnd, extraFieldEnd);

      final localFixedEnd = localHeaderOffset + 30;
      if (localFixedEnd > directoryOffset || localFixedEnd > sourceLen) {
        throw const OoxmlError.invalid('workbook ZIP metadata is invalid');
      }
      if (!_sigAt(bytes, localHeaderOffset, _localHeaderSignature)) {
        throw const OoxmlError.invalid('workbook ZIP metadata is invalid');
      }
      final localCompressedSize = _u32(bytes, localHeaderOffset + 18);
      final localUncompressedSize = _u32(bytes, localHeaderOffset + 22);
      final localFileNameLen = _u16(bytes, localHeaderOffset + 26);
      final localExtraFieldLen = _u16(bytes, localHeaderOffset + 28);
      if (localCompressedSize == 0xFFFFFFFF ||
          localUncompressedSize == 0xFFFFFFFF ||
          localFileNameLen != fileNameLen) {
        throw const OoxmlError.invalid('workbook ZIP metadata is invalid');
      }
      final localFileNameEnd = localFixedEnd + localFileNameLen;
      if (localFileNameEnd > directoryOffset || localFileNameEnd > sourceLen) {
        throw const OoxmlError.invalid('workbook ZIP metadata is invalid');
      }
      final localExtraFieldEnd = localFileNameEnd + localExtraFieldLen;
      if (localExtraFieldEnd > directoryOffset || localExtraFieldEnd > sourceLen) {
        throw const OoxmlError.invalid('workbook ZIP metadata is invalid');
      }
      _rejectZip64OrMalformedExtra(bytes, localFileNameEnd, localExtraFieldEnd);
    }
    if (cursor != directoryEnd) {
      throw const OoxmlError.invalid('workbook ZIP metadata is invalid');
    }
  }
  final commentStart = eocd + eocdMinBytes;
  return _ZipPreflight(
    entryCount,
    directoryOffset,
    Uint8List.fromList(Uint8List.sublistView(bytes, commentStart, commentStart + commentLen)),
  );
}

/// Decodes a ZIP file name: UTF-8 when the language flag is set, else CP437.
String _zipName(Uint8List raw, int flags) {
  if ((flags & 0x800) != 0) {
    return utf8.decode(raw, allowMalformed: true);
  }
  final out = StringBuffer();
  for (final byte in raw) {
    if (byte < 0x80) {
      out.writeCharCode(byte);
    } else {
      out.write(_cp437High[byte - 0x80]);
    }
  }
  return out.toString();
}

/// Port of open_archive + validate_archive: parses the central directory
/// with the preflight checks and rejects empty, oversized, encrypted,
/// unsafe-named or duplicate members.
_ZipPackage _openArchive(Uint8List bytes) {
  final preflight = _preflightArchive(bytes);
  final entries = <_ZipCentralEntry>[];
  var cursor = preflight.centralDirectoryStart;
  for (var index = 0; index < preflight.entryCount; index++) {
    if (!_sigAt(bytes, cursor, _centralHeaderSignature)) {
      throw const OoxmlError.invalid('workbook ZIP metadata is invalid');
    }
    final flags = _u16(bytes, cursor + 8);
    final nameLen = _u16(bytes, cursor + 28);
    final extraLen = _u16(bytes, cursor + 30);
    final commentLen = _u16(bytes, cursor + 32);
    final nameStart = cursor + 46;
    final extraStart = nameStart + nameLen;
    final commentStart = extraStart + extraLen;
    final next = commentStart + commentLen;
    if (next > bytes.length) {
      throw const OoxmlError.invalid('workbook ZIP metadata is invalid');
    }
    entries.add(
      _ZipCentralEntry(
        name: _zipName(
          Uint8List.sublistView(bytes, nameStart, extraStart),
          flags,
        ),
        rawName: Uint8List.fromList(
          Uint8List.sublistView(bytes, nameStart, extraStart),
        ),
        versionMadeBy: _u16(bytes, cursor + 4),
        versionNeeded: _u16(bytes, cursor + 6),
        flags: flags,
        method: _u16(bytes, cursor + 10),
        time: _u16(bytes, cursor + 12),
        date: _u16(bytes, cursor + 14),
        crc: _u32(bytes, cursor + 16),
        compressedSize: _u32(bytes, cursor + 20),
        uncompressedSize: _u32(bytes, cursor + 24),
        internalAttributes: _u16(bytes, cursor + 36),
        externalAttributes: _u32(bytes, cursor + 38),
        localOffset: _u32(bytes, cursor + 42),
        extra: Uint8List.fromList(
          Uint8List.sublistView(bytes, extraStart, commentStart),
        ),
        comment: Uint8List.fromList(
          Uint8List.sublistView(bytes, commentStart, next),
        ),
      ),
    );
    cursor = next;
  }
  final package = _ZipPackage._(
    bytes,
    preflight.comment,
    entries,
    preflight.centralDirectoryStart,
  );
  _validateArchive(package);
  return package;
}

/// Port of validate_archive.
void _validateArchive(_ZipPackage archive) {
  if (archive.length == 0) {
    throw const OoxmlError.invalid('workbook ZIP entry count is invalid');
  }
  if (archive.length > _maxEntries) {
    throw const OoxmlError.limitExceeded('workbook ZIP entry count is invalid');
  }
  final names = <String>{};
  var total = 0;
  for (final entry in archive.entries) {
    if (entry.uncompressedSize > _maxEntryBytes) {
      throw const OoxmlError.limitExceeded(
        'workbook member exceeded the size limit',
      );
    }
    if (entry.isEncrypted ||
        !_isEnclosedName(entry.name) ||
        !names.add(entry.name)) {
      throw const OoxmlError.invalid('workbook ZIP entry is invalid');
    }
    total += entry.uncompressedSize;
    if (total > _maxUncompressedBytes) {
      throw const OoxmlError.limitExceeded(
        'workbook uncompressed data exceeded the size limit',
      );
    }
  }
}

/// Mirror of zip's enclosed_name: a NUL byte or a '..' that escapes the root
/// is rejected; a leading root or drive prefix is skipped.
bool _isEnclosedName(String name) {
  if (name.contains('\u0000')) return false;
  final parts = name.split(RegExp(r'[/\\]'));
  var depth = 0;
  var first = true;
  for (final part in parts) {
    if (first && RegExp(r'^[A-Za-z]:$').hasMatch(part)) {
      first = false;
      continue;
    }
    first = false;
    if (part.isEmpty || part == '.') continue;
    if (part == '..') {
      if (depth == 0) return false;
      depth--;
    } else {
      depth++;
    }
  }
  return true;
}

bool _hasEntry(_ZipPackage archive, String name) => archive.hasEntry(name);

/// Port of write_package / write_package_guarded.
///
/// Members in [patched] are re-encoded with their original compression
/// method and metadata; every other member is raw-copied (compressed bytes
/// and CRC unchanged). The archive comment is preserved.
Uint8List _writePackage(_ZipPackage archive, Map<String, Uint8List> patched) {
  final localBlocks = BytesBuilder(copy: false);
  final centralBlocks = BytesBuilder(copy: false);
  var offset = 0;
  final entries = <_ZipCentralEntry>[];
  for (final original in archive.entries) {
    final nameBytes = original.rawName;
    final replacement = patched[original.name];
    final Uint8List compressed;
    final int method;
    final int crc;
    final int compressedSize;
    final int uncompressedSize;
    if (replacement != null) {
      method = original.method == 0 ? 0 : 8;
      uncompressedSize = replacement.length;
      crc = getCrc32(replacement);
      compressed = method == 0
          ? replacement
          : Uint8List.fromList(Deflate(replacement).getBytes());
      compressedSize = compressed.length;
    } else {
      method = original.method;
      crc = original.crc;
      compressed = archive._compressedBytes(original);
      compressedSize = compressed.length;
      uncompressedSize = original.uncompressedSize;
    }
    final flags = original.flags & ~0x08;
    final entry = _ZipCentralEntry(
      name: original.name,
      rawName: nameBytes,
      versionMadeBy: original.versionMadeBy,
      versionNeeded: original.versionNeeded,
      flags: flags,
      method: method,
      time: original.time,
      date: original.date,
      crc: crc,
      compressedSize: compressedSize,
      uncompressedSize: uncompressedSize,
      internalAttributes: original.internalAttributes,
      externalAttributes: original.externalAttributes,
      localOffset: offset,
      extra: original.extra,
      comment: original.comment,
    );
    entries.add(entry);

    final local = ByteData(30);
    local.setUint32(0, 0x04034b50, Endian.little);
    local.setUint16(4, entry.versionNeeded, Endian.little);
    local.setUint16(6, flags, Endian.little);
    local.setUint16(8, method, Endian.little);
    local.setUint16(10, entry.time, Endian.little);
    local.setUint16(12, entry.date, Endian.little);
    local.setUint32(14, crc, Endian.little);
    local.setUint32(18, compressedSize, Endian.little);
    local.setUint32(22, uncompressedSize, Endian.little);
    local.setUint16(26, nameBytes.length, Endian.little);
    local.setUint16(28, entry.extra.length, Endian.little);
    localBlocks.add(local.buffer.asUint8List());
    localBlocks.add(nameBytes);
    localBlocks.add(entry.extra);
    localBlocks.add(compressed);
    offset += 30 + nameBytes.length + entry.extra.length + compressedSize;

    final central = ByteData(46);
    central.setUint32(0, 0x02014b50, Endian.little);
    central.setUint16(4, entry.versionMadeBy, Endian.little);
    central.setUint16(6, entry.versionNeeded, Endian.little);
    central.setUint16(8, flags, Endian.little);
    central.setUint16(10, method, Endian.little);
    central.setUint16(12, entry.time, Endian.little);
    central.setUint16(14, entry.date, Endian.little);
    central.setUint32(16, crc, Endian.little);
    central.setUint32(20, compressedSize, Endian.little);
    central.setUint32(24, uncompressedSize, Endian.little);
    central.setUint16(28, nameBytes.length, Endian.little);
    central.setUint16(30, entry.extra.length, Endian.little);
    central.setUint16(32, entry.comment.length, Endian.little);
    central.setUint16(34, 0, Endian.little);
    central.setUint16(36, entry.internalAttributes, Endian.little);
    central.setUint32(38, entry.externalAttributes, Endian.little);
    central.setUint32(42, entry.localOffset, Endian.little);
    centralBlocks.add(central.buffer.asUint8List());
    centralBlocks.add(nameBytes);
    centralBlocks.add(entry.extra);
    centralBlocks.add(entry.comment);
  }
  final centralBytes = centralBlocks.takeBytes();
  final end = ByteData(22);
  end.setUint32(0, 0x06054b50, Endian.little);
  end.setUint16(8, entries.length, Endian.little);
  end.setUint16(10, entries.length, Endian.little);
  end.setUint32(12, centralBytes.length, Endian.little);
  end.setUint32(16, offset, Endian.little);
  end.setUint16(20, archive.comment.length, Endian.little);

  final out = BytesBuilder(copy: false)
    ..add(localBlocks.takeBytes())
    ..add(centralBytes)
    ..add(end.buffer.asUint8List())
    ..add(archive.comment);
  final result = out.takeBytes();
  if (result.length > maxPackageBytes) {
    throw const OoxmlError.limitExceeded(
      'guarded output exceeded the size limit',
    );
  }
  return result;
}
