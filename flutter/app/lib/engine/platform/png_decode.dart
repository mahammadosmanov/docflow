/// Pure-Dart PNG decoder used by the controlled Windows PDF and image print
/// path. Port of the `decode_png_reader` behaviour in
/// src-tauri/src/platform/windows/printing.rs, which relied on the Rust `png`
/// crate with `EXPAND | STRIP_16`.
///
/// Output matches the Rust bitmap contract: 24-bit BGR rows with a 4-byte
/// stride, padding bytes set to 0xFF, and alpha ignored. Palettes are expanded
/// to RGB, sub-byte grayscale is scaled to 8 bits, and 16-bit samples keep
/// their high byte. Interlaced (Adam7) images are supported.
///
/// Every failure is a [PrintFailure] whose message is the same localized or
/// diagnostic string the Rust decoder produced.
library;

import 'dart:typed_data';

import 'package:archive/archive.dart' show ZLibDecoder;

import '../print_queue.dart';

const List<int> _signature = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

/// Decoded page in the GDI DIB layout the Windows printer consumes.
final class PngBgrBitmap {
  const PngBgrBitmap({
    required this.width,
    required this.height,
    required this.bgr,
  });

  final int width;
  final int height;

  /// Rows of BGR triples, each row padded to a multiple of four bytes.
  final Uint8List bgr;
}

/// Port of `decode_png_bytes` / `decode_png_reader`.
PngBgrBitmap decodePngBgr(Uint8List bytes) {
  final invalid = PrintFailure.failed('Windows rendered an invalid PDF page');
  if (bytes.length < 8 || !_hasSignature(bytes)) {
    throw invalid;
  }

  var offset = 8;
  int? width;
  int? height;
  int? bitDepth;
  int? colorType;
  int? interlace;
  Uint8List? palette;
  final idat = BytesBuilder(copy: false);
  var sawEnd = false;
  while (offset + 12 <= bytes.length) {
    final length = _u32(bytes, offset);
    final type = String.fromCharCodes(bytes.sublist(offset + 4, offset + 8));
    final dataStart = offset + 8;
    final dataEnd = dataStart + length;
    if (length > 0x7fffffff || dataEnd + 4 > bytes.length) {
      throw invalid;
    }
    final data = Uint8List.sublistView(bytes, dataStart, dataEnd);
    if (_crc32(bytes.sublist(offset + 4, dataEnd)) != _u32(bytes, dataEnd)) {
      throw invalid;
    }
    switch (type) {
      case 'IHDR':
        if (length != 13) {
          throw invalid;
        }
        width = _u32(data, 0);
        height = _u32(data, 4);
        bitDepth = data[8];
        colorType = data[9];
        if (data[10] != 0 || data[11] != 0) {
          throw invalid;
        }
        interlace = data[12];
      case 'PLTE':
        palette = Uint8List.fromList(data);
      case 'IDAT':
        idat.add(data);
      case 'IEND':
        sawEnd = true;
    }
    offset = dataEnd + 4;
    if (sawEnd) {
      break;
    }
  }
  if (!sawEnd ||
      width == null ||
      height == null ||
      bitDepth == null ||
      colorType == null ||
      interlace == null ||
      width == 0 ||
      height == 0) {
    throw invalid;
  }
  final int ctype = colorType;
  final int depth = bitDepth;
  final channels = _channelsFor(ctype);
  if (!_validDepth(colorType, bitDepth) ||
      (interlace != 0 && interlace != 1)) {
    throw invalid;
  }
  if (colorType == 3 && palette == null) {
    throw invalid;
  }

  final Uint8List raw;
  try {
    raw = Uint8List.fromList(
      const ZLibDecoder().decodeBytes(idat.takeBytes()),
    );
  } on Object {
    throw PrintFailure.failed('Windows could not decode a PDF page');
  }

  final w = width;
  final h = height;
  final rgb = Uint8List(w * h * 3);
  final bitsPerPixel = channels * depth;
  final bytesPerPixel = (bitsPerPixel + 7) ~/ 8;
  var cursor = 0;

  void decodePass(int x0, int y0, int dx, int dy, int passWidth, int passHeight) {
    if (passWidth == 0 || passHeight == 0) {
      return;
    }
    final rowBytes = (passWidth * bitsPerPixel + 7) ~/ 8;
    var previous = Uint8List(rowBytes);
    for (var row = 0; row < passHeight; row++) {
      if (cursor + 1 + rowBytes > raw.length) {
        throw PrintFailure.failed('Windows could not decode a PDF page');
      }
      final filter = raw[cursor];
      final line = Uint8List(rowBytes);
      for (var i = 0; i < rowBytes; i++) {
        line[i] = raw[cursor + 1 + i];
      }
      cursor += 1 + rowBytes;
      _unfilter(filter, line, previous, bytesPerPixel);
      final y = y0 + row * dy;
      for (var col = 0; col < passWidth; col++) {
        final x = x0 + col * dx;
        final sample = _pixelRgb(line, col, ctype, depth, channels, palette);
        final target = (y * w + x) * 3;
        rgb[target] = sample.$1;
        rgb[target + 1] = sample.$2;
        rgb[target + 2] = sample.$3;
      }
      previous = line;
    }
  }

  if (interlace == 0) {
    decodePass(0, 0, 1, 1, w, h);
  } else {
    const xStart = [0, 4, 0, 2, 0, 1, 0];
    const yStart = [0, 0, 4, 0, 2, 0, 1];
    const xStep = [8, 8, 4, 4, 2, 2, 1];
    const yStep = [8, 8, 8, 4, 4, 2, 2];
    for (var pass = 0; pass < 7; pass++) {
      final passWidth = w > xStart[pass] ? (w - xStart[pass] + xStep[pass] - 1) ~/ xStep[pass] : 0;
      final passHeight = h > yStart[pass] ? (h - yStart[pass] + yStep[pass] - 1) ~/ yStep[pass] : 0;
      decodePass(xStart[pass], yStart[pass], xStep[pass], yStep[pass], passWidth, passHeight);
    }
  }

  final stride = ((w * 3) + 3) & ~3;
  final total = stride * h;
  if (w > 0x7fffffff || h > 0x7fffffff) {
    throw PrintFailure.failed('Windows PDF page is too large');
  }
  final bgr = Uint8List(total)..fillRange(0, total, 0xff);
  for (var y = 0; y < h; y++) {
    for (var x = 0; x < w; x++) {
      final source = (y * w + x) * 3;
      final target = y * stride + x * 3;
      bgr[target] = rgb[source + 2];
      bgr[target + 1] = rgb[source + 1];
      bgr[target + 2] = rgb[source];
    }
  }
  return PngBgrBitmap(width: w, height: h, bgr: bgr);
}

bool _hasSignature(Uint8List bytes) {
  for (var i = 0; i < _signature.length; i++) {
    if (bytes[i] != _signature[i]) {
      return false;
    }
  }
  return true;
}

int _channelsFor(int colorType) => switch (colorType) {
  0 => 1,
  2 => 3,
  3 => 1,
  4 => 2,
  6 => 4,
  _ => throw PrintFailure.failed('Windows rendered an invalid PDF page'),
};

bool _validDepth(int colorType, int depth) => switch (colorType) {
  0 => const {1, 2, 4, 8, 16}.contains(depth),
  3 => const {1, 2, 4, 8}.contains(depth),
  2 || 4 || 6 => const {8, 16}.contains(depth),
  _ => false,
};

/// Reads one sample from a scanline and returns it as 8-bit RGB. 16-bit
/// samples keep the high byte (`STRIP_16`). Low-bit grayscale and palette
/// indexes are scaled or looked up as `EXPAND` does.
(int, int, int) _pixelRgb(
  Uint8List line,
  int column,
  int colorType,
  int bitDepth,
  int channels,
  Uint8List? palette,
) {
  int sample(int channel) {
    final index = column * channels + channel;
    if (bitDepth == 8) {
      return line[index];
    }
    if (bitDepth == 16) {
      return line[index * 2];
    }
    // Sub-byte samples: `index` counts samples, not bytes.
    final bitOffset = index * bitDepth;
    final byte = line[bitOffset >> 3];
    final shift = 8 - bitDepth - (bitOffset & 7);
    return (byte >> shift) & ((1 << bitDepth) - 1);
  }

  switch (colorType) {
    case 3:
      final entry = sample(0) * 3;
      final table = palette!;
      if (entry + 2 >= table.length) {
        throw PrintFailure.failed('Windows could not decode a PDF page');
      }
      return (table[entry], table[entry + 1], table[entry + 2]);
    case 0 || 4:
      final value = _scaleGray(sample(0), bitDepth);
      return (value, value, value);
    default:
      return (
        _scaleGray(sample(0), bitDepth),
        _scaleGray(sample(1), bitDepth),
        _scaleGray(sample(2), bitDepth),
      );
  }
}

int _scaleGray(int value, int bitDepth) {
  if (bitDepth >= 8) {
    return value;
  }
  return value * 255 ~/ ((1 << bitDepth) - 1);
}

void _unfilter(int filter, Uint8List line, Uint8List previous, int bpp) {
  switch (filter) {
    case 0:
      return;
    case 1:
      for (var i = bpp; i < line.length; i++) {
        line[i] = (line[i] + line[i - bpp]) & 0xff;
      }
    case 2:
      for (var i = 0; i < line.length; i++) {
        line[i] = (line[i] + previous[i]) & 0xff;
      }
    case 3:
      for (var i = 0; i < line.length; i++) {
        final left = i >= bpp ? line[i - bpp] : 0;
        line[i] = (line[i] + ((left + previous[i]) >> 1)) & 0xff;
      }
    case 4:
      for (var i = 0; i < line.length; i++) {
        final left = i >= bpp ? line[i - bpp] : 0;
        final upper = previous[i];
        final upperLeft = i >= bpp ? previous[i - bpp] : 0;
        line[i] = (line[i] + _paeth(left, upper, upperLeft)) & 0xff;
      }
    default:
      throw PrintFailure.failed('Windows could not decode a PDF page');
  }
}

int _paeth(int a, int b, int c) {
  final p = a + b - c;
  final pa = (p - a).abs();
  final pb = (p - b).abs();
  final pc = (p - c).abs();
  if (pa <= pb && pa <= pc) {
    return a;
  }
  return pb <= pc ? b : c;
}

int _u32(Uint8List bytes, int offset) =>
    (bytes[offset] << 24) |
    (bytes[offset + 1] << 16) |
    (bytes[offset + 2] << 8) |
    bytes[offset + 3];

final Uint32List _crcTable = () {
  final table = Uint32List(256);
  for (var n = 0; n < 256; n++) {
    var c = n;
    for (var k = 0; k < 8; k++) {
      c = (c & 1) != 0 ? 0xedb88320 ^ (c >> 1) : c >> 1;
    }
    table[n] = c;
  }
  return table;
}();

int _crc32(List<int> bytes) {
  var crc = 0xffffffff;
  for (final byte in bytes) {
    crc = _crcTable[(crc ^ byte) & 0xff] ^ (crc >> 8);
  }
  return (crc ^ 0xffffffff) & 0xffffffff;
}
