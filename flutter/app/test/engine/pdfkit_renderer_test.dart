// Tests for the PDFKit helper renderer. Runs only on macOS with xcrun; the
// helper is compiled into a temporary cache on first use (slow once).
import 'dart:io';

import 'package:docflow/engine/pdfkit_renderer.dart';
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

void main() {
  final skip = PdfKitPageRenderer.available ? false : 'needs macOS with xcrun';

  test(
    'PDFKit helper extracts page text and renders a PNG',
    () async {
      final dir = Directory.systemTemp.createTempSync('docflow-pdfkit-');
      addTearDown(() => dir.deleteSync(recursive: true));
      final input = p.join(dir.path, 'page.pdf');
      File(input).writeAsStringSync(_pdfText);
      final renderer = PdfKitPageRenderer(
        cacheDirectory: p.join(dir.path, 'cache'),
      );
      final session = await renderer.openSession(input);
      expect(await session.pageText(0), contains('HELLO'));
      final output = p.join(dir.path, 'page.png');
      await session.renderPagePng(output, 72, 0);
      await session.close();
      expect(File(output).readAsBytesSync().sublist(0, 4), [
        0x89,
        0x50,
        0x4E,
        0x47,
      ]);
    },
    skip: skip,
    timeout: const Timeout(Duration(minutes: 5)),
  );
}
