part of '../ooxml.dart';

// Worksheet and shared-string reading. Port of shared_strings, worksheet_data,
// the canonical-hierarchy checks, merged-range validation and cell reference
// helpers from ooxml.rs.

const int _maxCellReferenceBytes = 32;
const int _maxColumnWidthBytes = 64;
const int _maxMergeRangeBytes = 64;

/// Every element in document order (port of `descendants`).
Iterable<XmlElement> _descendantElements(List<XmlNode> nodes) sync* {
  for (final node in nodes) {
    if (node is XmlElement) {
      yield node;
      yield* _descendantElements(node.children);
    }
  }
}

bool _isWorkbookNamespace(String? namespace) =>
    namespace == _transitionalWorkbookNamespace ||
    namespace == _strictWorkbookNamespace;

Iterable<XmlElement> _childElementsInNamespace(
  XmlElement element,
  String localName,
  String namespace,
) => element.children.whereType<XmlElement>().where(
  (child) => hasLocalName(child, localName) && child.namespace == namespace,
);

/// Port of optional_single_child_element_in_namespace.
XmlElement? _optionalSingleChildElementInNamespace(
  XmlElement parent,
  String localName,
  String namespace,
  String error,
) {
  XmlElement? found;
  for (final child in parent.children.whereType<XmlElement>()) {
    if (!hasLocalName(child, localName)) continue;
    if (child.namespace != namespace || found != null) {
      throw OoxmlError.invalid(error);
    }
    found = child;
  }
  return found;
}

XmlElement _singleChildElementInNamespace(
  XmlElement parent,
  String localName,
  String namespace,
  String error,
) {
  final found = _optionalSingleChildElementInNamespace(
    parent,
    localName,
    namespace,
    error,
  );
  if (found == null) throw OoxmlError.invalid(error);
  return found;
}

/// Port of shared_strings: the sst items in canonical hierarchy.
List<String> _sharedStrings(Uint8List raw) {
  final document = parseDocument(raw);
  final root = _documentRootLocal(
    document,
    'sst',
    'shared strings root is invalid',
  );
  final namespace = root.namespace;
  if (!_isWorkbookNamespace(namespace)) {
    throw const OoxmlError.invalid('shared strings namespace is invalid');
  }
  final ns = namespace!;
  final strings = <String>[];
  final canonicalItems = <XmlElement>{};
  for (final item in _childElementsInNamespace(root, 'si', ns)) {
    canonicalItems.add(item);
    strings.add(_canonicalInlineString(item, ns));
  }
  for (final item in _descendantElements(document.nodes)) {
    if (!hasLocalName(item, 'si')) continue;
    if (item.namespace != ns || !canonicalItems.contains(item)) {
      throw const OoxmlError.invalid(
        'shared string is outside the canonical hierarchy',
      );
    }
  }
  return strings;
}

/// Port of worksheet_data.
WorksheetData _worksheetData(
  Uint8List raw,
  List<String> sharedStrings,
  MergeComparisonBudget mergeBudget,
) {
  final document = parseDocument(raw);
  final root = _documentRootLocal(
    document,
    'worksheet',
    'worksheet root is invalid',
  );
  final namespace = root.namespace;
  if (!_isWorkbookNamespace(namespace)) {
    throw const OoxmlError.invalid('worksheet namespace is invalid');
  }
  final ns = namespace!;
  final sheetData = _singleChildElementInNamespace(
    root,
    'sheetData',
    ns,
    'worksheet sheetData is invalid',
  );
  final hasProtection = _optionalSingleChildElementInNamespace(
        root,
        'sheetProtection',
        ns,
        'worksheet protection is invalid',
      ) !=
      null;
  final data = WorksheetData(
    hasSheetProtection: hasProtection,
    mergedRanges: _worksheetMergedRanges(root, ns, mergeBudget),
  );

  for (final columns in _childElementsInNamespace(root, 'cols', ns)) {
    for (final column in _childElementsInNamespace(columns, 'col', ns)) {
      final first = _parseU32Lenient(
        boundedUnqualifiedAttribute(
          column,
          'min',
          _maxU32AttributeBytes,
          'worksheet column minimum',
        ),
      );
      final last = _parseU32Lenient(
        boundedUnqualifiedAttribute(
          column,
          'max',
          _maxU32AttributeBytes,
          'worksheet column maximum',
        ),
      );
      final widthText = boundedUnqualifiedAttribute(
        column,
        'width',
        _maxColumnWidthBytes,
        'worksheet column width',
      );
      final width = widthText == null ? null : _parseF64(widthText);
      if (first == null || last == null || width == null) continue;
      if (first == 0 ||
          first > last ||
          last > _maxWorksheetColumns ||
          !width.isFinite ||
          width < 0.0) {
        throw const OoxmlError.invalid('worksheet column is invalid');
      }
      for (var c = first; c <= last; c++) {
        data.columnWidths[c] = width;
      }
    }
  }

  final canonicalCells = <XmlElement>{};
  final rowNumbers = <int>{};
  final coordinates = <(int, int)>{};
  for (final row in _childElementsInNamespace(sheetData, 'row', ns)) {
    final rowText = boundedUnqualifiedAttribute(
      row,
      'r',
      _maxU32AttributeBytes,
      'worksheet row identity',
    );
    final rowNumber = _parseU32Lenient(rowText);
    if (rowNumber == null ||
        rowNumber < 1 ||
        rowNumber > _maxWorksheetRows) {
      throw const OoxmlError.invalid('worksheet row identity is invalid');
    }
    if (!rowNumbers.add(rowNumber)) {
      throw const OoxmlError.invalid('duplicate worksheet row identity');
    }
    data.maxRow = math.max(data.maxRow, rowNumber);
    for (final cell in _childElementsInNamespace(row, 'c', ns)) {
      canonicalCells.add(cell);
      _rejectNoncanonicalCellValueShadows(cell, ns);
      final refText = boundedUnqualifiedAttribute(
        cell,
        'r',
        _maxCellReferenceBytes,
        'worksheet cell reference',
      );
      final reference = refText == null ? null : _parseReference(refText);
      if (reference == null ||
          reference.$1 != rowNumber ||
          reference.$1 < 1 ||
          reference.$1 > _maxWorksheetRows ||
          reference.$2 < 1 ||
          reference.$2 > _maxWorksheetColumns) {
        throw const OoxmlError.invalid('worksheet cell reference is invalid');
      }
      if (!coordinates.add(reference)) {
        throw const OoxmlError.invalid('duplicate worksheet cell reference');
      }
      final cellType =
          boundedUnqualifiedAttribute(
            cell,
            't',
            _maxCellReferenceBytes,
            'worksheet cell type',
          ) ??
          '';
      final rawValueElement = _optionalSingleChildElementInNamespace(
        cell,
        'v',
        ns,
        'worksheet cell value is invalid',
      );
      final rawValue = rawValueElement == null
          ? ''
          : elementText(rawValueElement);
      final inline = _optionalSingleChildElementInNamespace(
        cell,
        'is',
        ns,
        'worksheet inline string is invalid',
      );
      final String value;
      switch (cellType) {
        case 'inlineStr':
          value = inline == null ? '' : _canonicalInlineString(inline, ns);
        case 's':
          if (rawValue.isEmpty) {
            value = '';
          } else {
            final index = _parseUsizeLenient(rawValue);
            if (index == null || index < 0 || index >= sharedStrings.length) {
              throw const OoxmlError.invalid('shared string index is invalid');
            }
            value = sharedStrings[index];
          }
        default:
          value = rawValue;
      }
      final formulaElement = _optionalSingleChildElementInNamespace(
        cell,
        'f',
        ns,
        'worksheet cell formula is invalid',
      );
      final formula = formulaElement == null
          ? null
          : elementText(formulaElement);
      final style = boundedUnqualifiedAttribute(
        cell,
        's',
        _maxU32AttributeBytes,
        'worksheet cell style',
      );
      data.maxRow = math.max(data.maxRow, reference.$1);
      data.maxColumn = math.max(data.maxColumn, reference.$2);
      data.cells[reference] = CellData(
        value: value,
        formula: formula,
        style: style,
      );
    }
  }

  for (final element in _descendantElements(document.nodes)) {
    if (!hasLocalName(element, 'c')) continue;
    if (element.namespace == ns) {
      if (!canonicalCells.contains(element)) {
        throw const OoxmlError.invalid(
          'worksheet cell is outside the canonical hierarchy',
        );
      }
    } else {
      final ref = boundedUnqualifiedAttribute(
        element,
        'r',
        _maxCellReferenceBytes,
        'foreign worksheet cell reference',
      );
      if (ref != null && _parseReference(ref) != null) {
        throw const OoxmlError.invalid(
          'foreign namespace worksheet cell shadow is invalid',
        );
      }
    }
  }
  return data;
}

/// Port of canonical_inline_string.
String _canonicalInlineString(XmlElement element, String namespace) {
  final text = StringBuffer();
  for (final child in element.children.whereType<XmlElement>()) {
    _requireNamespace(
      child,
      namespace,
      'worksheet inline string namespace is invalid',
    );
    if (hasLocalName(child, 't')) {
      text.write(elementText(child));
    } else if (hasLocalName(child, 'r')) {
      _appendCanonicalRichText(child, namespace, text);
    } else if (hasLocalName(child, 'rPh') || hasLocalName(child, 'phoneticPr')) {
      _validateDescendantNamespace(child, namespace);
    } else {
      throw const OoxmlError.invalid('worksheet inline string hierarchy is invalid');
    }
  }
  return text.toString();
}

void _appendCanonicalRichText(
  XmlElement element,
  String namespace,
  StringBuffer output,
) {
  for (final child in element.children.whereType<XmlElement>()) {
    _requireNamespace(
      child,
      namespace,
      'worksheet inline string namespace is invalid',
    );
    if (hasLocalName(child, 't')) {
      output.write(elementText(child));
    } else if (hasLocalName(child, 'rPr')) {
      _validateDescendantNamespace(child, namespace);
    } else {
      throw const OoxmlError.invalid('worksheet rich text hierarchy is invalid');
    }
  }
}

void _requireNamespace(XmlElement element, String namespace, String error) {
  if (element.namespace != namespace) {
    throw OoxmlError.invalid(error);
  }
}

void _validateDescendantNamespace(XmlElement element, String namespace) {
  for (final child in element.children.whereType<XmlElement>()) {
    _requireNamespace(
      child,
      namespace,
      'worksheet rich text namespace is invalid',
    );
    _validateDescendantNamespace(child, namespace);
  }
}

bool _isCellValueName(XmlElement element) =>
    hasLocalName(element, 'f') ||
    hasLocalName(element, 'v') ||
    hasLocalName(element, 'is');

void _rejectNoncanonicalCellValueShadows(XmlElement cell, String namespace) {
  for (final child in cell.children.whereType<XmlElement>()) {
    if (_isCellValueName(child)) {
      _requireNamespace(
        child,
        namespace,
        'worksheet cell value namespace is invalid',
      );
    }
    _rejectNestedCellValueShadow(child);
  }
}

void _rejectNestedCellValueShadow(XmlElement element) {
  for (final child in element.children.whereType<XmlElement>()) {
    if (_isCellValueName(child)) {
      throw const OoxmlError.invalid(
        'worksheet cell value is outside the canonical hierarchy',
      );
    }
    _rejectNestedCellValueShadow(child);
  }
}

/// Port of worksheet_merged_ranges.
List<CellRange> _worksheetMergedRanges(
  XmlElement root,
  String namespace,
  MergeComparisonBudget mergeBudget,
) {
  final mergeCells = _optionalSingleChildElementInNamespace(
    root,
    'mergeCells',
    namespace,
    'worksheet merge collection is invalid',
  );
  if (mergeCells == null) return <CellRange>[];
  final ranges = <CellRange>[];
  for (final element in mergeCells.children.whereType<XmlElement>()) {
    if (!hasLocalName(element, 'mergeCell')) continue;
    if (element.namespace != namespace) {
      throw const OoxmlError.invalid('worksheet merge range is invalid');
    }
    if (ranges.length >= _maxMergedRanges) {
      throw const OoxmlError.limitExceeded(
        'worksheet merge range count exceeded the limit',
      );
    }
    final reference = boundedUnqualifiedAttribute(
      element,
      'ref',
      _maxMergeRangeBytes,
      'worksheet merge range',
    );
    if (reference == null) {
      throw const OoxmlError.invalid('worksheet merge range is invalid');
    }
    final colon = reference.indexOf(':');
    final first = colon < 0 ? null : _parseReference(reference.substring(0, colon));
    final last = colon < 0 ? null : _parseReference(reference.substring(colon + 1));
    if (first == null || last == null) {
      throw const OoxmlError.invalid('worksheet merge range is invalid');
    }
    final range = CellRange(
      firstRow: first.$1,
      firstColumn: first.$2,
      lastRow: last.$1,
      lastColumn: last.$2,
    );
    if (range.firstRow == 0 ||
        range.firstColumn == 0 ||
        range.firstRow > range.lastRow ||
        range.firstColumn > range.lastColumn ||
        range.lastRow > _maxWorksheetRows ||
        range.lastColumn > _maxWorksheetColumns) {
      throw const OoxmlError.invalid('worksheet merge range is invalid');
    }
    if (ranges.contains(range)) continue;
    for (final existing in ranges) {
      mergeBudget.consume();
      if (existing.firstRow <= range.lastRow &&
          range.firstRow <= existing.lastRow &&
          existing.firstColumn <= range.lastColumn &&
          range.firstColumn <= existing.lastColumn) {
        throw const OoxmlError.invalid('worksheet merge range is invalid');
      }
    }
    ranges.add(range);
  }
  final declared = boundedUnqualifiedAttribute(
    mergeCells,
    'count',
    _maxU32AttributeBytes,
    'worksheet merge count',
  );
  if (declared != null) {
    final parsed = _parseUsizeLenient(declared);
    if (parsed == null) {
      throw const OoxmlError.invalid('worksheet merge count is invalid');
    }
    if (parsed > _maxMergedRanges) {
      throw const OoxmlError.limitExceeded(
        'worksheet merge range count exceeded the limit',
      );
    }
  }
  return ranges;
}

/// Lenient unsigned parse used where Rust does `.parse::<u32>().ok()`.
int? _parseU32Lenient(String? value) {
  if (value == null) return null;
  final parsed = _parseU32(value.startsWith('+') ? value.substring(1) : value);
  return parsed;
}

int? _parseUsizeLenient(String value) {
  final text = value.startsWith('+') ? value.substring(1) : value;
  if (!RegExp(r'^[0-9]+$').hasMatch(text)) return null;
  return int.tryParse(text);
}

double? _parseF64(String value) {
  final lower = value.toLowerCase();
  if (lower == 'nan' || lower == 'inf' || lower == 'infinity' ||
      lower == '+inf' || lower == '+infinity' ||
      lower == '-inf' || lower == '-infinity') {
    return double.nan;
  }
  return double.tryParse(value);
}

/// Port of column_name (1 -> A, 27 -> AA).
String _columnName(int number) {
  if (number == 0 || number > 16384) {
    throw const OoxmlError.invalid('worksheet column is invalid');
  }
  var n = number;
  final out = StringBuffer();
  final letters = <String>[];
  while (n > 0) {
    final remainder = (n - 1) % 26;
    letters.add(String.fromCharCode(0x41 + remainder));
    n = (n - 1) ~/ 26;
  }
  for (final letter in letters.reversed) {
    out.write(letter);
  }
  return out.toString();
}

/// Port of cell_reference.
String _cellReference(int row, int column) =>
    '${_columnName(column)}$row';

/// Port of split_reference: letters then digits.
(String, int)? _splitReference(String reference) {
  final split = reference.indexOf(RegExp(r'[0-9]'));
  if (split < 0) return null;
  final digits = reference.substring(split);
  if (!RegExp(r'^[0-9]+$').hasMatch(digits)) return null;
  final row = int.tryParse(digits);
  if (row == null || row > 0xFFFFFFFF) return null;
  return (reference.substring(0, split), row);
}

/// Port of parse_reference: returns (row, column) for e.g. "B12".
(int, int)? _parseReference(String reference) {
  final split = _splitReference(reference);
  if (split == null) return null;
  final (column, row) = split;
  var columnNumber = 0;
  for (final unit in column.codeUnits) {
    final isLetter = (unit >= 0x41 && unit <= 0x5A) || (unit >= 0x61 && unit <= 0x7A);
    if (!isLetter) return null;
    final upper = unit & ~0x20;
    columnNumber = columnNumber * 26 + (upper - 0x41 + 1);
    if (columnNumber > 0xFFFFFFFF) return null;
  }
  if (columnNumber == 0) return null;
  return (row, columnNumber);
}
