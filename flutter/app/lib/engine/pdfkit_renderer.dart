// PDFKit page access on macOS. A small Swift helper is compiled once with
// `xcrun swiftc` into the user's cache directory, keyed by the source hash, and
// then run per request. It is the PDFKit path the Rust module used
// (`PDFDocument`, `page.string`, and a rasterized thumbnail for PNG output).
//
// UNVERIFIED on Windows, and on macOS hosts where `xcrun swiftc` is missing:
// [PdfKitPageRenderer.available] is then false and selection falls back.
library;

import 'dart:convert';
import 'dart:io';

import 'operations/process_scans.dart' show OcrDocumentFailure;
import 'ocr.dart';

/// Swift source of the helper. Kept in sync with the compiled binary by hash.
const String pdfKitHelperSource = r'''
// DocFlow PDFKit page helper. Usage:
//   pdfkit_helper text <pdf> <pageIndex>            (UTF-8 page text to stdout)
//   pdfkit_helper render <pdf> <pageIndex> <dpi> <out.png>
import AppKit
import Foundation
import PDFKit

func fail(_ message: String, _ code: Int32) -> Never {
    FileHandle.standardError.write(Data((message + "\n").utf8))
    exit(code)
}

let args = CommandLine.arguments
guard args.count >= 4, let index = Int(args[3]) else {
    fail("usage: pdfkit_helper text|render <pdf> <pageIndex> [dpi out.png]", 2)
}
guard let document = PDFDocument(url: URL(fileURLWithPath: args[2])) else {
    fail("PDFKit could not open the document", 3)
}
guard index >= 0, index < document.pageCount, let page = document.page(at: index) else {
    fail("page index is out of range", 4)
}

switch args[1] {
case "text":
    FileHandle.standardOutput.write(Data((page.string ?? "").utf8))
case "render":
    guard args.count >= 6, let dpi = Double(args[4]), dpi > 0 else {
        fail("render needs a positive dpi and an output path", 2)
    }
    let box = page.bounds(for: .mediaBox)
    let scale = dpi / 72.0
    let size = NSSize(width: box.width * scale, height: box.height * scale)
    guard size.width >= 1, size.height >= 1, size.width * size.height <= 60_000_000 else {
        fail("page bitmap size is out of bounds", 5)
    }
    let image = page.thumbnail(of: size, for: .mediaBox)
    guard let tiff = image.tiffRepresentation,
          let bitmap = NSBitmapImageRep(data: tiff),
          let png = bitmap.representation(using: .png, properties: [:]) else {
        fail("PDFKit could not rasterize the page", 6)
    }
    do {
        try png.write(to: URL(fileURLWithPath: args[5]))
    } catch {
        fail("could not write the PNG", 7)
    }
default:
    fail("unknown mode", 2)
}
''';

final class PdfKitPageRenderer implements PdfPageRenderer {
  const PdfKitPageRenderer({this.cacheDirectory});

  /// Overrides the cache location (tests). Defaults to the user's caches folder.
  final String? cacheDirectory;

  /// True when the macOS toolchain needed to build the helper is present.
  static bool get available =>
      Platform.isMacOS && File('/usr/bin/xcrun').existsSync();

  @override
  Future<PdfPageSession> openSession(String path) async =>
      _PdfKitSession(await _helperPath(), path);

  Future<String> _helperPath() async {
    final home = Platform.environment['HOME'] ?? Directory.systemTemp.path;
    final dir = cacheDirectory ?? '$home/Library/Caches/DocFlow';
    final digest = _fnv1a(pdfKitHelperSource);
    final binary = '$dir/pdfkit-helper-$digest';
    if (File(binary).existsSync()) return binary;
    if (!available) {
      throw const OcrDocumentFailure('PDFKit helper needs xcrun (UNVERIFIED)');
    }
    Directory(dir).createSync(recursive: true);
    final source = File('$binary.swift')..writeAsStringSync(pdfKitHelperSource);
    final build = await Process.run('/usr/bin/xcrun', [
      'swiftc',
      '-O',
      source.path,
      '-o',
      '$binary.tmp',
    ]).timeout(const Duration(minutes: 3));
    if (build.exitCode != 0) {
      throw OcrDocumentFailure('PDFKit helper failed to build: ${build.stderr}');
    }
    File('$binary.tmp').renameSync(binary);
    return binary;
  }

  static String _fnv1a(String text) {
    var hash = 0x811c9dc5;
    for (final unit in utf8.encode(text)) {
      hash ^= unit;
      hash = (hash * 0x01000193) & 0xffffffff;
    }
    return hash.toRadixString(16);
  }
}

final class _PdfKitSession implements PdfPageSession {
  _PdfKitSession(this._helper, this._input);

  final String _helper;
  final String _input;

  Future<ProcessResult> _run(List<String> args) =>
      Process.run(_helper, args, stdoutEncoding: utf8)
          .timeout(const Duration(seconds: 60));

  @override
  Future<String> pageText(int pageIndex) async {
    final result = await _run(['text', _input, '$pageIndex']);
    if (result.exitCode != 0) {
      throw OcrDocumentFailure('PDFKit text failed (exit ${result.exitCode})');
    }
    return result.stdout as String;
  }

  @override
  Future<void> renderPagePng(String output, int dpi, int pageIndex) async {
    final result = await _run(['render', _input, '$pageIndex', '$dpi', output]);
    if (result.exitCode != 0) {
      throw OcrDocumentFailure('PDFKit render failed (exit ${result.exitCode})');
    }
  }

  @override
  Future<void> close() async {}
}
