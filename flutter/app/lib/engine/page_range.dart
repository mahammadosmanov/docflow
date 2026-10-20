/// Port of src-tauri/src/page_range.rs.
library;

import 'dart:collection';

import 'errors.dart';

/// Port of `MAX_SELECTED_PAGES`.
const int maxSelectedPages = 1000000;
const String _invalidRangeKey = 'error.print.pageRangeInvalid';
const String _rangeBoundsKey = 'error.print.pageRangeBounds';
const String _imageRangeKey = 'error.print.pageRangeImageUnsupported';
const String _nativeRangeKey = 'error.print.pageRangeNativeUnsupported';

/// Rust `u32` upper bound.
const int _maxU32 = 0xFFFFFFFF;

sealed class PageRange {
  const PageRange();

  /// Port of `PageRange::parse`.
  static PageRange parse(String raw) {
    raw = raw.trim();
    if (raw.isEmpty) {
      throw _invalidRange();
    }
    if (_eqIgnoreAsciiCase(raw, 'all')) {
      return const PageRangeAll();
    }

    final pages = SplayTreeSet<int>();
    for (var token in raw.split(',')) {
      token = token.trim();
      if (token.isEmpty || _eqIgnoreAsciiCase(token, 'all')) {
        throw _invalidRange();
      }
      final dash = token.indexOf('-');
      if (dash >= 0) {
        final start0 = token.substring(0, dash);
        final end0 = token.substring(dash + 1);
        if (end0.contains('-')) {
          throw _invalidRange();
        }
        final start = _positivePage(start0);
        final end = _positivePage(end0);
        final span = end - start;
        if (start > end ||
            span >= maxSelectedPages ||
            pages.length + (span + 1) > maxSelectedPages) {
          throw _invalidRange();
        }
        pages.addAll([for (var page = start; page <= end; page++) page]);
      } else {
        pages.add(_positivePage(token));
        if (pages.length > maxSelectedPages) {
          throw _invalidRange();
        }
      }
    }
    if (pages.isEmpty) {
      throw _invalidRange();
    }
    return PageRangePages(pages);
  }

  /// Port of `PageRange::canonical`.
  String canonical();

  /// Port of `PageRange::resolve_zero_based`.
  List<int> resolveZeroBased(int pageCount);
}

final class PageRangeAll extends PageRange {
  const PageRangeAll();

  @override
  String canonical() => 'all';

  @override
  List<int> resolveZeroBased(int pageCount) {
    if (pageCount > maxSelectedPages) {
      throw BackendError.validation(_rangeBoundsKey);
    }
    return [for (var page = 0; page < pageCount; page++) page];
  }

  @override
  bool operator ==(Object other) => other is PageRangeAll;

  @override
  int get hashCode => 1;
}

final class PageRangePages extends PageRange {
  PageRangePages(this.pages);

  /// Sorted, de-duplicated 1-based page numbers (Rust `BTreeSet<u32>`).
  final SplayTreeSet<int> pages;

  @override
  String canonical() {
    if (pages.isEmpty) {
      return '';
    }
    final ranges = <String>[];
    var start = pages.first;
    var end = start;
    for (final page in pages.skip(1)) {
      if (end + 1 == page) {
        end = page;
      } else {
        ranges.add(_formatRange(start, end));
        start = page;
        end = page;
      }
    }
    ranges.add(_formatRange(start, end));
    return ranges.join(',');
  }

  @override
  List<int> resolveZeroBased(int pageCount) {
    if (pages.isNotEmpty && pages.last > pageCount) {
      throw BackendError.validation(_rangeBoundsKey);
    }
    return [for (final page in pages) page - 1];
  }

  @override
  bool operator ==(Object other) =>
      other is PageRangePages &&
      other.pages.length == pages.length &&
      other.pages.every(pages.contains);

  @override
  int get hashCode => Object.hashAll(pages);
}

sealed class PageRangeCapability {
  const PageRangeCapability();
}

/// Port of `PageRangeCapability::Pdf { page_count }`.
final class PdfPageRangeCapability extends PageRangeCapability {
  const PdfPageRangeCapability({this.pageCount});

  final int? pageCount;
}

/// Port of `PageRangeCapability::SinglePage`.
final class SinglePagePageRangeCapability extends PageRangeCapability {
  const SinglePagePageRangeCapability();
}

/// Port of `PageRangeCapability::AllOnly`.
final class AllOnlyPageRangeCapability extends PageRangeCapability {
  const AllOnlyPageRangeCapability();
}

/// Port of `canonical_for`.
String canonicalFor(String raw, PageRangeCapability capability) {
  final range = PageRange.parse(raw);
  switch (capability) {
    case PdfPageRangeCapability(:final pageCount):
      if (pageCount != null) {
        range.resolveZeroBased(pageCount);
      }
      return range.canonical();
    case SinglePagePageRangeCapability():
      switch (range) {
        case PageRangeAll():
          return 'all';
        case PageRangePages(:final pages)
            when pages.length == 1 && pages.contains(1):
          return 'all';
        case PageRangePages():
          throw BackendError.validation(_imageRangeKey);
      }
    case AllOnlyPageRangeCapability():
      switch (range) {
        case PageRangeAll():
          return 'all';
        case PageRangePages():
          throw BackendError.validation(_nativeRangeKey);
      }
  }
}

/// Port of `selected_pdf_pages` (Windows/test helper in Rust).
List<int> selectedPdfPages(String raw, int pageCount) =>
    PageRange.parse(raw).resolveZeroBased(pageCount);

int _positivePage(String raw) {
  raw = raw.trim();
  if (raw.isEmpty || !raw.codeUnits.every((c) => c >= 0x30 && c <= 0x39)) {
    throw _invalidRange();
  }
  final page = int.tryParse(raw);
  if (page == null || page > _maxU32) {
    throw _invalidRange();
  }
  if (page == 0) {
    throw _invalidRange();
  }
  return page;
}

String _formatRange(int start, int end) =>
    start == end ? '$start' : '$start-$end';

BackendError _invalidRange() => BackendError.validation(_invalidRangeKey);

/// ASCII-only case-insensitive comparison (Rust `eq_ignore_ascii_case`).
bool _eqIgnoreAsciiCase(String value, String lowerAscii) {
  if (value.length != lowerAscii.length) {
    return false;
  }
  for (var index = 0; index < value.length; index++) {
    var unit = value.codeUnitAt(index);
    if (unit >= 0x41 && unit <= 0x5A) {
      unit += 0x20;
    }
    if (unit != lowerAscii.codeUnitAt(index)) {
      return false;
    }
  }
  return true;
}
