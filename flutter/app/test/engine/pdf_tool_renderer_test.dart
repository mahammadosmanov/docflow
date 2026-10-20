// Tests for the poppler-backed PDF page renderer. Skipped when pdftoppm is
// not installed (poppler is not bundled, so this is UNVERIFIED on other hosts).
import 'dart:io';

import 'package:docflow/engine/pdf_tool_renderer.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

const String _pdfText = '''%PDF-1.4
1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj
2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj
3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >> endobj
4 0 obj << /Length 44 >> stream
BT /F1 24 Tf 20 100 Td (HELLO) Tj ET
endstream endobj
5 0 obj << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> endobj
trailer << /Root 1 0 R >>
%%EOF
''';

final PdfToolPageRenderer? _renderer = PdfToolPageRenderer.discover();

void main() {
  final skip = _renderer == null ? 'pdftoppm is not installed' : false;

  test('renders page one to a PNG file', () async {
    final dir = Directory.systemTemp.createTempSync('docflow-pdftool-');
    addTearDown(() => dir.deleteSync(recursive: true));
    final input = p.join(dir.path, 'page.pdf');
    File(input).writeAsStringSync(_pdfText);
    final session = await _renderer!.openSession(input);
    final output = p.join(dir.path, 'page.png');
    await session.renderPagePng(output, 72, 0);
    await session.close();
    final bytes = File(output).readAsBytesSync();
    expect(bytes.sublist(0, 4), [0x89, 0x50, 0x4E, 0x47]);
  }, skip: skip);

  test('extracts searchable text when pdftotext is available', () async {
    final dir = Directory.systemTemp.createTempSync('docflow-pdftool-');
    addTearDown(() => dir.deleteSync(recursive: true));
    final input = p.join(dir.path, 'page.pdf');
    File(input).writeAsStringSync(_pdfText);
    final session = await _renderer!.openSession(input);
    final text = await session.pageText(0);
    await session.close();
    if (_renderer!.pdftotext == null) {
      expect(text, isEmpty);
    } else {
      expect(text, contains('HELLO'));
    }
  }, skip: skip);
}
