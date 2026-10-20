// Port of src-tauri/src/windows_pdf.rs.
//
// The Rust module renders PDF pages through Windows.Data.Pdf. That runtime
// exists only on Windows, so this port keeps the rendering contract behind the
// [PdfRasterizer] interface. Page counting and the render-limit arithmetic are
// pure Dart and fully verified. The Windows binding is an UNVERIFIED adapter.
library;

import 'dart:typed_data';

import 'errors.dart';
import 'pdf.dart' as pdf;

const int maxRenderEdge = 20000;
const int maxRenderPixels = 100000000;

/// Rendering contract shared by the Windows adapter and any future backend.
abstract interface class PdfRasterizer {
  /// Number of pages. Throws [BackendError] when the document is unreadable
  /// or has no pages.
  int pageCount(String path);

  /// Renders one page to PNG bytes at [dpi] (72..=600).
  Uint8List renderPagePng(String path, int pageIndex, {required int dpi});
}

/// Port of the DPI and size checks in `render_page_png` /
/// `render_document_page_png_bytes`. Returns the destination pixel size for a
/// page of [widthDip] x [heightDip] device-independent pixels.
(int, int) pdfRenderDimensions(double widthDip, double heightDip, int dpi) {
  if (dpi < 72 || dpi > 600) {
    throw const BackendError(
      'validation',
      'Windows PDF render DPI must be between 72 and 600',
    );
  }
  final scale = dpi / 96.0;
  final width = (widthDip * scale).ceilToDouble();
  final height = (heightDip * scale).ceilToDouble();
  if (!width.isFinite ||
      !height.isFinite ||
      width < 1 ||
      height < 1 ||
      width > maxRenderEdge ||
      height > maxRenderEdge ||
      width * height > maxRenderPixels) {
    throw const BackendError(
      'request_failed',
      'Windows PDF page exceeds the render limit',
    );
  }
  return (width.toInt(), height.toInt());
}

/// Device-independent pixel size of a PDF page. The Windows runtime reports
/// `PdfPage.Size` in DIPs (1/96 inch), so the PDF point box (1/72 inch) is
/// scaled by 96/72. UNVERIFIED: the mapping is taken from the Rust scaling and
/// the DIP definition, not from a Windows run.
(double, double) pageSizeDip(pdf.PdfInspection facts, int pageIndex) {
  final page = facts.pages[pageIndex];
  final box = page.cropBox.map(double.parse).toList();
  var width = (box[2] - box[0]).abs() * 96.0 / 72.0;
  var height = (box[3] - box[1]).abs() * 96.0 / 72.0;
  if (page.rotation % 180 != 0) {
    final swap = width;
    width = height;
    height = swap;
  }
  return (width, height);
}

/// Pure-Dart page counting. Counting uses the verified PDF parser, so it does
/// not depend on the Windows runtime.
int countPdfPages(String path) {
  final facts = pdf.inspect(path);
  if (facts.pageCount == 0) {
    throw const BackendError('request_failed', 'Windows PDF has no pages');
  }
  return facts.pageCount;
}

/// Windows adapter for [PdfRasterizer].
///
// UNVERIFIED: the Windows.Data.Pdf render path (RoInitialize, PdfDocument
/// load from stream, PdfPageRenderOptions, RenderWithOptionsToStreamAsync) has
/// no Dart binding in this port and has not run on Windows. Page counting and
/// the render-limit checks are verified. Rendering on any platform raises a
/// `request_failed` error rather than returning fabricated pixels.
final class WindowsPdfRasterizer implements PdfRasterizer {
  const WindowsPdfRasterizer();

  @override
  int pageCount(String path) => countPdfPages(path);

  @override
  Uint8List renderPagePng(String path, int pageIndex, {required int dpi}) {
    if (dpi < 72 || dpi > 600) {
      throw const BackendError(
        'validation',
        'Windows PDF render DPI must be between 72 and 600',
      );
    }
    final facts = pdf.inspect(path);
    if (pageIndex < 0 || pageIndex >= facts.pageCount) {
      throw const BackendError(
        'validation',
        'Windows PDF page index is out of range',
      );
    }
    final (widthDip, heightDip) = pageSizeDip(facts, pageIndex);
    final (width, height) = pdfRenderDimensions(widthDip, heightDip, dpi);
    throw BackendError(
      'request_failed',
      'Windows PDF rendering is unavailable on this platform '
          '(requested ${width}x$height at $dpi DPI)',
    );
  }
}
