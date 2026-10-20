// PDF page access through poppler's `pdftoppm` (PNG rendering) and `pdftotext`
// (searchable text). Used on macOS when poppler is installed; the PDFKit
// bridge itself is not ported. UNVERIFIED on macOS and Windows hosts: poppler
// is not bundled with the app, so this path runs only where the tools exist
// on PATH or in the standard Homebrew/system bin directories.
library;

import 'dart:io';

import 'ocr.dart';
import 'operations/process_scans.dart' show OcrDocumentFailure;

const List<String> _toolDirectories = [
  '/opt/homebrew/bin',
  '/usr/local/bin',
  '/usr/bin',
];

/// Finds [tool] on PATH or in the standard bin directories. Null when absent.
String? findPopplerTool(String tool) {
  final path = (Platform.environment['PATH'] ?? '').split(':');
  for (final directory in [...path, ..._toolDirectories]) {
    if (directory.isEmpty) continue;
    final candidate = '$directory/$tool';
    if (File(candidate).existsSync()) return candidate;
  }
  return null;
}

/// [PdfPageRenderer] over `pdftoppm` and `pdftotext`.
final class PdfToolPageRenderer implements PdfPageRenderer {
  const PdfToolPageRenderer({required this.pdftoppm, this.pdftotext});

  final String pdftoppm;
  final String? pdftotext;

  @override
  Future<PdfPageSession> openSession(String path) async =>
      _PdfToolSession(pdftoppm, pdftotext, path);

  /// The renderer when poppler is installed, otherwise null.
  static PdfToolPageRenderer? discover() {
    final pdftoppm = findPopplerTool('pdftoppm');
    if (pdftoppm == null) return null;
    return PdfToolPageRenderer(
      pdftoppm: pdftoppm,
      pdftotext: findPopplerTool('pdftotext'),
    );
  }
}

final class _PdfToolSession implements PdfPageSession {
  _PdfToolSession(this._pdftoppm, this._pdftotext, this._input);

  final String _pdftoppm;
  final String? _pdftotext;
  final String _input;

  @override
  Future<String> pageText(int pageIndex) async {
    final tool = _pdftotext;
    if (tool == null) return '';
    final page = '${pageIndex + 1}';
    final result = await Process.run(tool, [
      '-f',
      page,
      '-l',
      page,
      '-layout',
      _input,
      '-',
    ]);
    if (result.exitCode != 0) {
      throw OcrDocumentFailure('pdftotext failed: ${result.stderr}');
    }
    return result.stdout as String;
  }

  @override
  Future<void> renderPagePng(String output, int dpi, int pageIndex) async {
    final prefix = output.endsWith('.png')
        ? output.substring(0, output.length - 4)
        : output;
    final page = '${pageIndex + 1}';
    final result = await Process.run(_pdftoppm, [
      '-png',
      '-r',
      '$dpi',
      '-f',
      page,
      '-l',
      page,
      '-singlefile',
      _input,
      prefix,
    ]);
    if (result.exitCode != 0) {
      throw OcrDocumentFailure('pdftoppm failed: ${result.stderr}');
    }
    if ('$prefix.png' != output) {
      File('$prefix.png').renameSync(output);
    }
  }

  @override
  Future<void> close() async {}
}
