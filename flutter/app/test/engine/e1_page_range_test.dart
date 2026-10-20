// Port of the inline tests in src-tauri/src/page_range.rs.
import 'package:flutter_test/flutter_test.dart';
import 'package:docflow/engine/errors.dart';
import 'package:docflow/engine/page_range.dart';

void expectValidation(void Function() body, String key) {
  expect(
    body,
    throwsA(
      isA<BackendError>()
          .having((error) => error.code, 'code', 'validation')
          .having((error) => error.message, 'message', key),
    ),
  );
}

void main() {
  test('parses all case-insensitively', () {
    expect(PageRange.parse('all').canonical(), 'all');
    expect(PageRange.parse(' ALL ').canonical(), 'all');
  });

  test('sorts, deduplicates and coalesces numeric ranges', () {
    expect(PageRange.parse(' 3, 1-2,2,7 ').canonical(), '1-3,7');
  });

  test('resolves numeric ranges to zero-based pages', () {
    expect(PageRange.parse('1-3,5').resolveZeroBased(5), [0, 1, 2, 4]);
  });

  test('selected pdf pages resolve canonical ranges to exact indexes', () {
    expect(selectedPdfPages('2-3', 4), [1, 2]);
    expect(selectedPdfPages('3,1-2,2', 4), [0, 1, 2]);
    expect(selectedPdfPages('all', 4), [0, 1, 2, 3]);
  });

  test('all page ranges reject page counts above the selection limit', () {
    const limit = maxSelectedPages;
    const overLimit = limit + 1;

    expect(PageRangeAll().resolveZeroBased(limit).length, maxSelectedPages);
    expectValidation(
      () => PageRangeAll().resolveZeroBased(overLimit),
      'error.print.pageRangeBounds',
    );
    expectValidation(
      () => selectedPdfPages('all', overLimit),
      'error.print.pageRangeBounds',
    );
    expectValidation(
      () => canonicalFor(
        'all',
        const PdfPageRangeCapability(pageCount: overLimit),
      ),
      'error.print.pageRangeBounds',
    );
    expect(selectedPdfPages('1,1000001', overLimit), [0, 1000000]);
  });

  test('rejects empty input', () {
    expectValidation(() => PageRange.parse(''), 'error.print.pageRangeInvalid');
  });

  test('rejects zero, negatives, empty segments, bad tokens and overflow', () {
    for (final raw in [
      '0',
      '-1',
      '1,,2',
      '1-2-3',
      '5-3',
      'all,2',
      'one',
      '4294967296',
    ]) {
      expectValidation(
        () => PageRange.parse(raw),
        'error.print.pageRangeInvalid',
      );
    }
  });

  test('rejects pages above the supplied page count', () {
    expectValidation(
      () => PageRange.parse('1-3').resolveZeroBased(2),
      'error.print.pageRangeBounds',
    );
  });

  test('pdf capability canonicalizes and enforces a known bound', () {
    const capability = PdfPageRangeCapability(pageCount: 3);
    expect(canonicalFor('3,1-2', capability), '1-3');
    expectValidation(
      () => canonicalFor('4', capability),
      'error.print.pageRangeBounds',
    );
  });

  test('single page capability normalizes page one to all', () {
    const capability = SinglePagePageRangeCapability();
    expect(canonicalFor('all', capability), 'all');
    expect(canonicalFor('1', capability), 'all');
    expectValidation(
      () => canonicalFor('2', capability),
      'error.print.pageRangeImageUnsupported',
    );
  });

  test('all-only capability rejects numeric ranges', () {
    const capability = AllOnlyPageRangeCapability();
    expect(canonicalFor('all', capability), 'all');
    expectValidation(
      () => canonicalFor('1', capability),
      'error.print.pageRangeNativeUnsupported',
    );
  });
}
