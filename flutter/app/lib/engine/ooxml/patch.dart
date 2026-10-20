part of '../ooxml.dart';

// Cell patching, calcChain / data-validation maintenance, guarded verification
// and the public patch entry points. Port of ooxml.rs from patch_sheet_xml
// through verify_guarded_workbook_delta.
//
// The Rust functions write to files; these ports take and return package bytes.
// The caller owns file creation (create-new) and error cleanup.

XmlElement _cloneElement(XmlElement element) => XmlElement(
  qname: element.qname,
  namespace: element.namespace,
  attributes: [
    for (final attribute in element.attributes)
      ExpandedAttribute(
        qname: attribute.qname,
        namespace: attribute.namespace,
        localName: attribute.localName,
        value: attribute.value,
      ),
  ],
  children: [
    for (final child in element.children)
      child is XmlElement ? _cloneElement(child) : child,
  ],
  empty: element.empty,
  startRaw: element.startRaw,
  endRaw: element.endRaw,
);

List<(int, int)> _sortedCoordinates(Iterable<(int, int)> keys) {
  final list = keys.toList();
  list.sort((a, b) => a.$1 != b.$1 ? a.$1.compareTo(b.$1) : a.$2.compareTo(b.$2));
  return list;
}

/// Port of find_element_mut: first element in document order with the name.
XmlElement? _findElementMut(List<XmlNode> nodes, String name) {
  for (final node in nodes) {
    if (node is! XmlElement) continue;
    if (hasLocalName(node, name)) return node;
    final found = _findElementMut(node.children, name);
    if (found != null) return found;
  }
  return null;
}

Iterable<XmlElement> _childElementsMut(XmlElement element, String name) =>
    element.children.whereType<XmlElement>().where(
      (child) => hasLocalName(child, name),
    );

/// Port of nearest_style: style of the nearest cell in the same column.
String? _nearestStyle(XmlElement sheetData, int rowNumber, int columnNumber) {
  int? bestDistance;
  String? best;
  for (final row in _childElementsMut(sheetData, 'row')) {
    final rowText = attribute(row, 'r');
    final currentRow = rowText == null ? null : _parseU32Lenient(rowText);
    if (currentRow == null) continue;
    for (final cell in _childElementsMut(row, 'c')) {
      final reference = attribute(cell, 'r');
      if (reference == null) continue;
      final parsed = _parseReference(reference);
      if (parsed == null) continue;
      final style = attribute(cell, 's');
      if (style == null) continue;
      if (parsed.$2 == columnNumber) {
        final distance = (currentRow - rowNumber).abs();
        if (bestDistance == null || distance < bestDistance) {
          bestDistance = distance;
          best = style;
        }
      }
    }
  }
  return best;
}

/// Port of ensure_row: finds or inserts the `<row r=...>` in order.
XmlElement _ensureRow(XmlElement sheetData, int rowNumber) {
  var insertAt = sheetData.children.length;
  for (var index = 0; index < sheetData.children.length; index++) {
    final node = sheetData.children[index];
    if (node is! XmlElement) continue;
    if (!hasLocalName(node, 'row')) continue;
    final text = attribute(node, 'r');
    final current = text == null ? null : _parseU32Lenient(text);
    if (current == null) {
      throw const OoxmlError.invalid('worksheet row identity is invalid');
    }
    if (current == rowNumber || current > rowNumber) {
      insertAt = index;
      break;
    }
  }
  final isMatch = insertAt < sheetData.children.length &&
      sheetData.children[insertAt] is XmlElement &&
      hasLocalName(sheetData.children[insertAt] as XmlElement, 'row') &&
      attribute(sheetData.children[insertAt] as XmlElement, 'r') ==
          rowNumber.toString();
  if (!isMatch) {
    final row = XmlElement.named('row');
    setAttribute(row, 'r', rowNumber.toString());
    sheetData.children.insert(insertAt, row);
  }
  final node = sheetData.children[insertAt];
  if (node is! XmlElement) {
    throw const OoxmlError.invalid('worksheet row insertion failed');
  }
  return node;
}

/// Port of ensure_cell: finds or inserts the `<c r=...>` in column order.
XmlElement _ensureCell(
  XmlElement row,
  int rowNumber,
  int columnNumber,
  String? style,
) {
  final reference = _cellReference(rowNumber, columnNumber);
  var insertAt = row.children.length;
  for (var index = 0; index < row.children.length; index++) {
    final node = row.children[index];
    if (node is! XmlElement) continue;
    if (!hasLocalName(node, 'c')) continue;
    final text = attribute(node, 'r');
    final current = text == null ? null : _parseReference(text);
    if (current == null) {
      throw const OoxmlError.invalid('worksheet cell reference is invalid');
    }
    if (current.$2 == columnNumber || current.$2 > columnNumber) {
      insertAt = index;
      break;
    }
  }
  final isMatch = insertAt < row.children.length &&
      row.children[insertAt] is XmlElement &&
      hasLocalName(row.children[insertAt] as XmlElement, 'c') &&
      attribute(row.children[insertAt] as XmlElement, 'r') == reference;
  if (!isMatch) {
    final cell = XmlElement.named('c');
    setAttribute(cell, 'r', reference);
    if (style != null) {
      setAttribute(cell, 's', style);
    }
    row.children.insert(insertAt, cell);
  }
  final node = row.children[insertAt];
  if (node is! XmlElement) {
    throw const OoxmlError.invalid('worksheet cell insertion failed');
  }
  return node;
}

/// Port of ensure_guarded_cell: creates a bare `<c r=...>` in the namespace.
XmlElement _ensureGuardedCell(
  XmlElement row,
  int rowNumber,
  int columnNumber,
  String namespace,
  String? prefix,
) {
  final coordinate = _cellReference(rowNumber, columnNumber);
  final cell = _ensureCell(row, rowNumber, columnNumber, null);
  cell.namespace = namespace;
  final qualifiedName = prefix == null ? 'c' : '$prefix:c';
  cell.qname = qualifiedName;
  cell.attributes = [
    ExpandedAttribute(
      qname: 'r',
      namespace: null,
      localName: 'r',
      value: coordinate,
    ),
  ];
  cell._resetStart();
  return cell;
}

/// Port of write_update: replaces a cell's content for a typed update.
void _writeUpdate(XmlElement cell, CellUpdate update) {
  final existingFormula = childElement(cell, 'f');
  final existingClone = existingFormula == null
      ? null
      : _cloneElement(existingFormula);
  cell.children.clear();
  removeAttribute(cell, 't');
  switch (update) {
    case CellUpdateText(:final value):
      if (value.isEmpty) return;
      setAttribute(cell, 't', 'inlineStr');
      final inline = XmlElement.named('is');
      final text = XmlElement.named('t');
      if (value.trim() != value) {
        setAttribute(text, 'xml:space', 'preserve');
      }
      setElementText(text, value);
      inline.children.add(text);
      cell.children.add(inline);
    case CellUpdateNumber(:final value):
      cell.children.add(textElement('v', value));
    case CellUpdateBool(:final value):
      setAttribute(cell, 't', 'b');
      cell.children.add(textElement('v', value ? '1' : '0'));
    case CellUpdateFormula(:final formula, :final cached):
      final formulaElement = existingClone ?? XmlElement.named('f');
      setElementText(formulaElement, _trimStartChars(formula, '='));
      cell.children.add(formulaElement);
      if (cached != null) {
        setAttribute(cell, 't', 'str');
        cell.children.add(textElement('v', cached));
      }
  }
}

String _trimStartChars(String value, String char) {
  var start = 0;
  while (start < value.length && value[start] == char) {
    start++;
  }
  return value.substring(start);
}

/// Port of patch_sheet_xml. Returns the patched sheet and the references of
/// formula cells that were replaced (for calcChain maintenance).
(Uint8List, Set<String>) _patchSheetXml(
  Uint8List raw,
  Map<(int, int), CellUpdate> updates,
  int? duplicateColumn,
) {
  final document = parseDocument(raw);
  final sheetData = _findElementMut(document.nodes, 'sheetData');
  if (sheetData == null) {
    throw const OoxmlError.invalid('worksheet sheetData is missing');
  }
  final replacedFormulas = <String>{};
  for (final coordinate in _sortedCoordinates(updates.keys)) {
    final (rowNumber, columnNumber) = coordinate;
    final update = updates[coordinate]!;
    if (rowNumber == 0 || columnNumber == 0 || columnNumber > 16384) {
      throw const OoxmlError.invalid('worksheet cell reference is invalid');
    }
    final style = _nearestStyle(sheetData, rowNumber, columnNumber);
    final row = _ensureRow(sheetData, rowNumber);
    final cell = _ensureCell(row, rowNumber, columnNumber, style);
    if (update is! CellUpdateFormula && childElement(cell, 'f') != null) {
      replacedFormulas.add(_cellReference(rowNumber, columnNumber));
    }
    _writeUpdate(cell, update);
  }
  if (duplicateColumn != null) {
    _patchDuplicateValidation(document.nodes, duplicateColumn);
  }
  return (serializeDocument(document), replacedFormulas);
}

/// Port of patch_calc_chain: drops calcChain entries for replaced formulas
/// and restores the sheet id (`i`) on the entries that follow.
void _patchCalcChain(
  XmlDocument document,
  List<(int, Set<String>)> replacedBySheet,
) {
  XmlElement? root;
  for (final node in document.nodes) {
    if (node is XmlElement && hasLocalName(node, 'calcChain')) {
      root = node;
      break;
    }
  }
  if (root == null) {
    throw const OoxmlError.invalid('calcChain root is missing');
  }
  final removed = <int, Set<String>>{
    for (final (sheet, references) in replacedBySheet) sheet: references,
  };
  int? currentSheet;
  int? lastKeptSheet;
  root.children.removeWhere((node) {
    if (node is! XmlElement) return false;
    final cell = node;
    if (!hasLocalName(cell, 'c')) return false;
    final iText = attribute(cell, 'i');
    final parsedI = iText == null ? null : _parseU32Lenient(iText);
    if (parsedI != null) {
      currentSheet = parsedI;
    }
    final reference = attribute(cell, 'r') ?? '';
    if (currentSheet != null &&
        removed[currentSheet] != null &&
        removed[currentSheet]!.contains(reference)) {
      return true;
    }
    if (currentSheet != null &&
        currentSheet != lastKeptSheet &&
        attribute(cell, 'i') == null) {
      setAttribute(cell, 'i', currentSheet.toString());
    }
    lastKeptSheet = currentSheet;
    return false;
  });
}

/// Port of patch_duplicate_validation: rewrites the first custom COUNTIF
/// data-validation formula for the column to the duplicate-check formula.
void _patchDuplicateValidation(List<XmlNode> nodes, int columnNumber) {
  final column = _columnName(columnNumber);
  final validations = _findElementMut(nodes, 'dataValidations');
  if (validations == null) return;
  for (final validation in _childElementsMut(validations, 'dataValidation')) {
    if (attribute(validation, 'type') != 'custom') continue;
    final reference = attribute(validation, 'sqref');
    final formula = childElement(validation, 'formula1');
    if (formula == null) continue;
    if (!elementText(formula).toUpperCase().contains('COUNTIF')) continue;
    if (reference == null) continue;
    int? startRow;
    for (final range in reference.split(RegExp(r'\s+'))) {
      if (range.isEmpty) continue;
      final colon = range.indexOf(':');
      if (colon < 0) continue;
      final left = range.substring(0, colon);
      final right = range.substring(colon + 1);
      final leftSplit = _splitReference(left);
      final rightSplit = _splitReference(right);
      if (leftSplit == null || rightSplit == null) continue;
      if (leftSplit.$1.toLowerCase() == column.toLowerCase() &&
          rightSplit.$1.toLowerCase() == column.toLowerCase()) {
        startRow = leftSplit.$2;
        break;
      }
    }
    if (startRow == null) continue;
    final previous = math.max(math.max(startRow - 1, 0), 1);
    setElementText(
      formula,
      'OR($column$startRow="",COUNTIF(\$$column\$$startRow:$column$startRow,$column$startRow)=1,$column$startRow=$column$previous)',
    );
    break;
  }
}

/// Port of patch_sheet_xml_guarded: exact-cell writes for guarded status or
/// text values (inline strings only).
Uint8List _patchSheetXmlGuarded(
  Uint8List raw,
  Map<(int, int), GuardedCellUpdate> updates,
  bool statusValuesOnly,
) {
  final document = parseDocument(raw);
  final root = _documentRootMutLocal(
    document,
    'worksheet',
    'guarded worksheet root is invalid',
  );
  final namespace = _guardedNamespace(root);
  final prefix = _rootPrefix(root);
  final sheetData = _singleChildElementInNamespaceMut(
    root,
    'sheetData',
    namespace,
    'guarded worksheet sheetData is invalid',
  );
  for (final coordinate in _sortedCoordinates(updates.keys)) {
    final update = updates[coordinate]!;
    final (rowNumber, columnNumber) = coordinate;
    final row = _uniqueDirectChildByAttributeMut(
      sheetData,
      'row',
      namespace,
      'r',
      rowNumber.toString(),
      'guarded worksheet row is missing or ambiguous',
    );
    final coordinateText = _cellReference(rowNumber, columnNumber);
    final cellMatch = _optionalUniqueDirectChildByAttributeMut(
      row,
      'c',
      namespace,
      'r',
      coordinateText,
      'guarded worksheet cell is ambiguous',
    );
    final XmlElement cell;
    if (cellMatch != null && !update.createIfAbsent) {
      cell = cellMatch;
    } else if (cellMatch == null && update.createIfAbsent) {
      cell = _ensureGuardedCell(row, rowNumber, columnNumber, namespace, prefix);
    } else {
      throw const OoxmlError.invalid('guarded worksheet cell presence changed');
    }
    _patchGuardedCellValue(
      cell,
      namespace,
      prefix,
      update.replacement,
      statusValuesOnly,
    );
  }
  return serializeDocument(document);
}

String _guardedNamespace(XmlElement root) {
  final namespace = root.namespace;
  if (!_isWorkbookNamespace(namespace)) {
    throw const OoxmlError.invalid('guarded worksheet namespace is invalid');
  }
  return namespace!;
}

String? _rootPrefix(XmlElement root) {
  final name = root.qname;
  final colon = name.indexOf(':');
  return colon < 0 ? null : name.substring(0, colon);
}

/// Port of document_root_mut_local.
XmlElement _documentRootMutLocal(
  XmlDocument document,
  String localName,
  String error,
) {
  final elements = document.nodes.whereType<XmlElement>().toList();
  if (elements.length != 1 || !hasLocalName(elements.single, localName)) {
    throw OoxmlError.invalid(error);
  }
  return elements.single;
}

/// Port of single_child_element_in_namespace_mut.
XmlElement _singleChildElementInNamespaceMut(
  XmlElement parent,
  String localName,
  String namespace,
  String error,
) {
  final matches = <XmlElement>[];
  for (final child in parent.children.whereType<XmlElement>()) {
    if (!hasLocalName(child, localName)) continue;
    if (child.namespace != namespace) {
      throw OoxmlError.invalid(error);
    }
    matches.add(child);
  }
  if (matches.length != 1) {
    throw OoxmlError.invalid(error);
  }
  return matches.single;
}

/// Port of unique_direct_child_by_attribute_mut.
XmlElement _uniqueDirectChildByAttributeMut(
  XmlElement parent,
  String localName,
  String namespace,
  String attributeName,
  String attributeValue,
  String error,
) {
  final matches = <XmlElement>[];
  for (final child in parent.children.whereType<XmlElement>()) {
    if (child.namespace == namespace && hasLocalName(child, localName)) {
      if (attribute(child, attributeName) == attributeValue) {
        matches.add(child);
      }
    }
  }
  if (matches.length != 1) {
    throw OoxmlError.invalid(error);
  }
  return matches.single;
}

/// Port of optional_unique_direct_child_by_attribute_mut.
XmlElement? _optionalUniqueDirectChildByAttributeMut(
  XmlElement parent,
  String localName,
  String namespace,
  String attributeName,
  String attributeValue,
  String error,
) {
  final matches = <XmlElement>[];
  for (final child in parent.children.whereType<XmlElement>()) {
    if (child.namespace == namespace &&
        hasLocalName(child, localName) &&
        attribute(child, attributeName) == attributeValue) {
      matches.add(child);
    }
  }
  if (matches.isEmpty) return null;
  if (matches.length == 1) return matches.single;
  throw OoxmlError.invalid(error);
}

/// Port of patch_guarded_cell_value.
void _patchGuardedCellValue(
  XmlElement cell,
  String namespace,
  String? prefix,
  CellUpdate update,
  bool statusValuesOnly,
) {
  if (update is! CellUpdateText) {
    throw const OoxmlError.invalid('guarded replacement is invalid');
  }
  final value = update.value;
  if (statusValuesOnly && value != 'Signed' && value != 'Not Signed') {
    throw const OoxmlError.invalid('guarded replacement is invalid');
  }
  if (_childElementsInNamespace(cell, 'f', namespace).isNotEmpty) {
    throw const OoxmlError.invalid('guarded target cell contains a formula');
  }
  final representationIndices = <int>[];
  for (var index = 0; index < cell.children.length; index++) {
    final node = cell.children[index];
    if (node is XmlElement &&
        node.namespace == namespace &&
        (hasLocalName(node, 'v') || hasLocalName(node, 'is'))) {
      representationIndices.add(index);
    }
  }
  if (representationIndices.length > 1) {
    throw const OoxmlError.invalid(
      'guarded target cell value representation is ambiguous',
    );
  }
  final insertAt = representationIndices.isNotEmpty
      ? representationIndices.first
      : _extLstOrEnd(cell, namespace);
  for (final index in representationIndices.reversed) {
    cell.children.removeAt(index);
  }

  setUnqualifiedAttribute(cell, 't', 'inlineStr');
  String qualify(String local) => prefix == null ? local : '$prefix:$local';
  final inline = XmlElement.named(qualify('is'))..namespace = namespace;
  final text = XmlElement.named(qualify('t'))..namespace = namespace;
  if (value.trim() != value) {
    setAttribute(text, 'xml:space', 'preserve');
  }
  setElementText(text, value);
  inline.children.add(text);
  cell.children.insert(insertAt, inline);
  cell.empty = false;
}

int _extLstOrEnd(XmlElement cell, String namespace) {
  for (var index = 0; index < cell.children.length; index++) {
    final node = cell.children[index];
    if (node is XmlElement &&
        node.namespace == namespace &&
        hasLocalName(node, 'extLst')) {
      return index;
    }
  }
  return cell.children.length;
}

/// Port of guarded_sheet_tokens: canonical token stream of a sheet after the
/// guarded cells are normalized. [output] selects the destination-side rules.
List<String> _guardedSheetTokens(
  Uint8List raw,
  Map<(int, int), GuardedCellUpdate> updates,
  bool output,
) {
  final document = parseDocument(raw);
  final root = _documentRootMutLocal(
    document,
    'worksheet',
    'guarded worksheet root is invalid',
  );
  final namespace = _guardedNamespace(root);
  final prefix = _rootPrefix(root);
  final sheetData = _singleChildElementInNamespaceMut(
    root,
    'sheetData',
    namespace,
    'guarded worksheet sheetData is invalid',
  );
  for (final coordinate in _sortedCoordinates(updates.keys)) {
    final update = updates[coordinate]!;
    final (rowNumber, columnNumber) = coordinate;
    final row = _uniqueDirectChildByAttributeMut(
      sheetData,
      'row',
      namespace,
      'r',
      rowNumber.toString(),
      'guarded worksheet row is missing or ambiguous',
    );
    final coordinateText = _cellReference(rowNumber, columnNumber);
    final cellMatch = _optionalUniqueDirectChildByAttributeMut(
      row,
      'c',
      namespace,
      'r',
      coordinateText,
      'guarded worksheet cell is ambiguous',
    );
    final XmlElement cell;
    if (cellMatch != null && (!update.createIfAbsent || output)) {
      cell = cellMatch;
    } else if (cellMatch == null && update.createIfAbsent && !output) {
      cell = _ensureGuardedCell(row, rowNumber, columnNumber, namespace, prefix);
    } else {
      throw const OoxmlError.invalid('guarded worksheet cell presence changed');
    }
    _normalizeGuardedCellForTokens(
      cell,
      namespace,
      coordinateText,
      update,
      output,
    );
  }
  final tokens = <String>[];
  _canonicalXmlTokens(document.nodes, tokens);
  return tokens;
}

void _normalizeGuardedCellForTokens(
  XmlElement cell,
  String namespace,
  String coordinate,
  GuardedCellUpdate update,
  bool output,
) {
  final replacement = update.replacement;
  if (replacement is! CellUpdateText) {
    throw const OoxmlError.invalid('guarded replacement is invalid');
  }
  if (output) {
    _validateExactGuardedOutputValue(cell, namespace, replacement.value);
  } else if (_childElementsInNamespace(cell, 'f', namespace).isNotEmpty) {
    throw const OoxmlError.invalid('guarded source cell contains a formula');
  }

  cell.attributes.removeWhere(
    (attribute) => attribute.namespace == null && attribute.localName == 't',
  );
  final representationIndices = <int>[];
  for (var index = 0; index < cell.children.length; index++) {
    final node = cell.children[index];
    if (node is XmlElement &&
        node.namespace == namespace &&
        (hasLocalName(node, 'v') || hasLocalName(node, 'is'))) {
      representationIndices.add(index);
    }
  }
  if (representationIndices.length > 1) {
    throw const OoxmlError.invalid(
      'guarded cell value representation is ambiguous',
    );
  }
  final insertAt = representationIndices.isNotEmpty
      ? representationIndices.first
      : _extLstOrEnd(cell, namespace);
  for (final index in representationIndices.reversed) {
    cell.children.removeAt(index);
  }
  final marker = 'docflow-guarded:$coordinate:${replacement.value}';
  cell.children.insert(insertAt, XmlRawNode(xmlEscape(marker)));
  cell.empty = false;
}

/// Port of validate_exact_guarded_output_value. As in the Rust version, a
/// text run containing entity references is several quick-xml events and is
/// rejected here.
void _validateExactGuardedOutputValue(
  XmlElement cell,
  String namespace,
  String expected,
) {
  final type = boundedUnqualifiedAttribute(
    cell,
    't',
    _maxCellReferenceBytes,
    'guarded cell type',
  );
  if (type != 'inlineStr') {
    throw const OoxmlError.invalid('guarded output cell type is invalid');
  }
  if (_childElementsInNamespace(cell, 'f', namespace).isNotEmpty ||
      _childElementsInNamespace(cell, 'v', namespace).isNotEmpty) {
    throw const OoxmlError.invalid('guarded output value representation is invalid');
  }
  final inline = _singleChildElementInNamespace(
    cell,
    'is',
    namespace,
    'guarded output inline string is invalid',
  );
  if (inline.attributes.isNotEmpty || inline.children.length != 1) {
    throw const OoxmlError.invalid('guarded output inline string is invalid');
  }
  final only = inline.children.first;
  if (only is! XmlElement) {
    throw const OoxmlError.invalid('guarded output inline string is invalid');
  }
  final text = only;
  if (text.namespace != namespace || !hasLocalName(text, 't')) {
    throw const OoxmlError.invalid('guarded output inline string is invalid');
  }
  final allowsSpace = expected.trim() != expected;
  final attributesAreExact = allowsSpace
      ? text.attributes.length == 1 &&
            text.attributes[0].namespace == _xmlNamespaceXml &&
            text.attributes[0].localName == 'space' &&
            text.attributes[0].value == 'preserve'
      : text.attributes.isEmpty;
  if (!attributesAreExact) {
    throw const OoxmlError.invalid(
      'guarded output inline string attributes are invalid',
    );
  }
  final pieces = <QuickXmlTextPiece>[];
  for (final node in text.children) {
    if (node is! XmlRawNode || node.raw.startsWith('<')) {
      throw const OoxmlError.invalid('guarded output inline string text is invalid');
    }
    pieces.addAll(quickXmlTextPieces(node.raw));
  }
  final String decoded;
  if (pieces.isEmpty && expected.isEmpty) {
    decoded = '';
  } else if (pieces.length == 1 && !pieces.single.isReference) {
    decoded = pieces.single.raw;
  } else {
    throw const OoxmlError.invalid('guarded output inline string text is invalid');
  }
  if (_decodeEntities(decoded) != expected) {
    throw const OoxmlError.invalid('guarded output inline string text is invalid');
  }
}

/// Port of canonical_xml_tokens. Each token is serialized to a JSON string so
/// token streams compare with ==.
void _canonicalXmlTokens(List<XmlNode> nodes, List<String> output) {
  for (final node in nodes) {
    switch (node) {
      case XmlElement():
        final localName = _localPart(node.qname);
        final attributes = <List<String?>>[];
        for (final attribute in node.attributes) {
          if (attribute.localName == 'xmlns' ||
              attribute.namespace == 'http://www.w3.org/2000/xmlns/') {
            continue;
          }
          final value = xmlUnescape(attribute.value);
          if (value == null) {
            throw const OoxmlError.invalid('XML attribute is invalid');
          }
          attributes.add([attribute.namespace, attribute.localName, value]);
        }
        if (node.empty && node.children.isEmpty) {
          output.add(
            jsonEncode(['empty', node.namespace, localName, attributes]),
          );
        } else {
          output.add(
            jsonEncode(['start', node.namespace, localName, attributes]),
          );
          _canonicalXmlTokens(node.children, output);
          output.add(jsonEncode(['end', node.namespace, localName]));
        }
      case XmlRawNode(:final raw):
        if (raw.startsWith('<![CDATA[')) {
          output.add(jsonEncode(['cdata', raw]));
        } else if (raw.startsWith('<!--')) {
          output.add(jsonEncode(['comment', raw]));
        } else if (raw.startsWith('<?xml ') || raw.startsWith('<?xml?')) {
          output.add(jsonEncode(['decl', raw]));
        } else if (raw.startsWith('<?')) {
          output.add(jsonEncode(['pi', raw]));
        } else if (raw.startsWith('<!DOCTYPE')) {
          output.add(jsonEncode(['doctype', raw]));
        } else if (raw.startsWith('<')) {
          throw const OoxmlError.invalid('XML event model is invalid');
        } else {
          for (final piece in quickXmlTextPieces(raw)) {
            if (piece.isReference) {
              output.add(jsonEncode(['raw', 'general_ref', piece.payload]));
            } else {
              output.add(
                jsonEncode(['raw', 'text', _decodeEntities(piece.raw)]),
              );
            }
          }
        }
    }
  }
}

/// Port of patch_workbook_sheets: text/number/bool/formula cell writes for
/// the named worksheets, with calcChain and duplicate-validation upkeep.
/// Returns the new package bytes.
Uint8List patchWorkbookSheets(
  Uint8List source,
  Map<String, Map<(int, int), CellUpdate>> updatesBySheet, {
  int? allowConsecutiveDuplicatesColumn,
}) {
  if (updatesBySheet.isEmpty) {
    throw const OoxmlError.invalid('at least one worksheet update is required');
  }
  _validateSourceBytes(source);
  final discovery = orderedWorkbookDiscovery(source);
  final sheetInfos = _projectSheetInfos(discovery);
  final archive = _openArchive(source);
  final patched = <String, Uint8List>{};
  final replacedBySheet = <(int, Set<String>)>[];
  for (final sheetName in updatesBySheet.keys.toList()..sort()) {
    final updates = updatesBySheet[sheetName]!;
    final info = sheetInfos[sheetName];
    if (info == null) {
      throw OoxmlError.invalid('worksheet not found: $sheetName');
    }
    final raw = archive.read(info.path);
    final (xml, replaced) = _patchSheetXml(
      raw,
      updates,
      allowConsecutiveDuplicatesColumn,
    );
    patched[info.path] = xml;
    if (replaced.isNotEmpty) {
      replacedBySheet.add((info.sheetId, replaced));
    }
  }
  if (replacedBySheet.isNotEmpty && archive.hasEntry('xl/calcChain.xml')) {
    final document = parseDocument(archive.read('xl/calcChain.xml'));
    _patchCalcChain(document, replacedBySheet);
    patched['xl/calcChain.xml'] = serializeDocument(document);
  }
  return _writePackage(archive, patched);
}

void _validateGuardedUpdateSet(
  Map<String, Map<(int, int), GuardedCellUpdate>> updatesBySheet,
  bool statusValuesOnly,
) {
  if (updatesBySheet.isEmpty ||
      updatesBySheet.values.any((updates) => updates.isEmpty)) {
    throw const OoxmlError.invalid('at least one guarded cell update is required');
  }
  for (final updates in updatesBySheet.values) {
    for (final entry in updates.entries) {
      final (row, column) = entry.key;
      final update = entry.value;
      if (row == 0 || column == 0 || column > _maxWorksheetColumns) {
        throw const OoxmlError.invalid(
          'guarded worksheet cell reference is invalid',
        );
      }
      final replacement = update.replacement;
      final isText = replacement is CellUpdateText;
      final statusOk = !statusValuesOnly ||
          (replacement is CellUpdateText &&
              (replacement.value == 'Signed' ||
                  replacement.value == 'Not Signed'));
      final createInvalid = update.createIfAbsent &&
          (statusValuesOnly || update.expected != const CellData());
      if (!isText || !statusOk || createInvalid) {
        throw const OoxmlError.invalid('guarded replacement is invalid');
      }
    }
  }
}

void _validateGuardedSource(
  WorkbookData workbook,
  Map<String, Map<(int, int), GuardedCellUpdate>> updatesBySheet,
) {
  if (!workbook.mergeValidationComplete) {
    throw const OoxmlError.invalid('guarded merge validation facts are incomplete');
  }
  final mergeBudget = MergeComparisonBudget.forApply(
    workbook.mergeValidationComparisons,
  );
  for (final entry in updatesBySheet.entries) {
    final sheet = workbook.sheet(entry.key);
    if (sheet == null) {
      throw const OoxmlError.invalid('guarded worksheet is missing');
    }
    if (sheet.hasSheetProtection) {
      throw const OoxmlError.invalid('guarded worksheet is protected');
    }
    for (final coordinateEntry in entry.value.entries) {
      final coordinate = coordinateEntry.key;
      final update = coordinateEntry.value;
      final current = sheet.cells[coordinate];
      final absentIsExpected = update.createIfAbsent &&
          update.expected == const CellData() &&
          current == null &&
          sheet.cells.keys.any((key) => key.$1 == coordinate.$1);
      final presentIsExpected = !update.createIfAbsent &&
          current != null &&
          current == update.expected &&
          current.formula == null;
      if (!absentIsExpected && !presentIsExpected) {
        throw const OoxmlError.invalid(
          'guarded target cell presence or value no longer matches its approved state',
        );
      }
      for (final range in sheet.mergedRanges) {
        mergeBudget.consume();
        if (range.firstRow <= coordinate.$1 &&
            coordinate.$1 <= range.lastRow &&
            range.firstColumn <= coordinate.$2 &&
            coordinate.$2 <= range.lastColumn) {
          throw const OoxmlError.invalid('guarded target cell is merged');
        }
      }
    }
  }
}

Uint8List _patchGuardedInner(
  Uint8List source,
  Map<String, Map<(int, int), GuardedCellUpdate>> updatesBySheet, {
  required bool statusValuesOnly,
}) {
  _validateGuardedUpdateSet(updatesBySheet, statusValuesOnly);
  _validateSourceBytes(source);
  final discovery = orderedWorkbookDiscovery(source);
  if (discovery.safety.hasWorkbookProtection) {
    throw const OoxmlError.invalid('guarded workbook is protected');
  }
  final sheetInfos = _projectSheetInfos(discovery);
  if (packageHasSignature(source)) {
    throw const OoxmlError.invalid('guarded workbook is signed');
  }
  final workbook = readWorkbook(source);
  _validateGuardedSource(workbook, updatesBySheet);

  final archive = _openArchive(source);
  final patched = <String, Uint8List>{};
  for (final entry in updatesBySheet.entries) {
    final info = sheetInfos[entry.key];
    if (info == null) {
      throw const OoxmlError.invalid('guarded worksheet is missing');
    }
    final raw = archive.read(info.path);
    patched[info.path] = _patchSheetXmlGuarded(
      raw,
      entry.value,
      statusValuesOnly,
    );
  }
  return _writePackage(archive, patched);
}

/// Port of patch_workbook_sheets_guarded: status-value (Signed / Not Signed)
/// writes with exact-cell and closed-package-delta guarantees.
Uint8List patchWorkbookSheetsGuarded(
  Uint8List source,
  Map<String, Map<(int, int), GuardedCellUpdate>> updatesBySheet,
) => _patchGuardedInner(source, updatesBySheet, statusValuesOnly: true);

/// Port of patch_workbook_sheets_guarded_text: arbitrary text replacements
/// under the same guarantees. Callers validate their own vocabulary.
Uint8List patchWorkbookSheetsGuardedText(
  Uint8List source,
  Map<String, Map<(int, int), GuardedCellUpdate>> updatesBySheet,
) => _patchGuardedInner(source, updatesBySheet, statusValuesOnly: false);

void _verifyGuardedInner(
  Uint8List source,
  Uint8List destination,
  Map<String, Map<(int, int), GuardedCellUpdate>> updatesBySheet, {
  required bool statusValuesOnly,
}) {
  _validateGuardedUpdateSet(updatesBySheet, statusValuesOnly);
  final sourceDiscovery = orderedWorkbookDiscovery(source);
  final sourceInfos = _projectSheetInfos(sourceDiscovery);
  final targetParts = <String>{};
  for (final name in updatesBySheet.keys) {
    final info = sourceInfos[name];
    if (info == null) {
      throw const OoxmlError.invalid('guarded worksheet is missing');
    }
    targetParts.add(info.path);
  }

  final sourceArchive = _openArchive(source);
  final destinationArchive = _openArchive(destination);
  if (sourceArchive.length != destinationArchive.length) {
    throw const OoxmlError.invalid('guarded package member roster changed');
  }
  for (var index = 0; index < sourceArchive.length; index++) {
    final sourceName = sourceArchive.entries[index].name;
    final destinationName = destinationArchive.entries[index].name;
    if (sourceName != destinationName) {
      throw const OoxmlError.invalid('guarded package member roster changed');
    }
    final sourceBytes = sourceArchive.read(sourceName);
    final destinationBytes = destinationArchive.read(destinationName);
    if (targetParts.contains(sourceName)) {
      String? sheetName;
      for (final name in updatesBySheet.keys) {
        if (sourceInfos[name]?.path == sourceName) {
          sheetName = name;
          break;
        }
      }
      if (sheetName == null) {
        throw const OoxmlError.invalid('guarded worksheet is missing');
      }
      final updates = updatesBySheet[sheetName];
      if (updates == null) {
        throw const OoxmlError.invalid('guarded updates are missing');
      }
      final sourceTokens = _guardedSheetTokens(sourceBytes, updates, false);
      final destinationTokens = _guardedSheetTokens(
        destinationBytes,
        updates,
        true,
      );
      if (!_listEquals(destinationTokens, sourceTokens)) {
        throw const OoxmlError.invalid('guarded worksheet delta verification failed');
      }
    } else if (!_bytesEqual(destinationBytes, sourceBytes)) {
      throw const OoxmlError.invalid('guarded non-target package member changed');
    }
  }

  final destinationWorkbook = readWorkbook(destination);
  for (final entry in updatesBySheet.entries) {
    final sheet = destinationWorkbook.sheet(entry.key);
    if (sheet == null) {
      throw const OoxmlError.invalid('guarded worksheet is missing');
    }
    for (final coordinateEntry in entry.value.entries) {
      final cell = sheet.cells[coordinateEntry.key];
      if (cell == null) {
        throw const OoxmlError.invalid('guarded target cell is missing');
      }
      final replacement = coordinateEntry.value.replacement;
      if (replacement is! CellUpdateText) {
        throw const OoxmlError.invalid('guarded replacement is invalid');
      }
      if (cell.value != replacement.value ||
          cell.formula != null ||
          cell.style != coordinateEntry.value.expected.style) {
        throw const OoxmlError.invalid('guarded target cell verification failed');
      }
    }
  }
}

bool _bytesEqual(Uint8List a, Uint8List b) {
  if (a.length != b.length) return false;
  for (var i = 0; i < a.length; i++) {
    if (a[i] != b[i]) return false;
  }
  return true;
}

/// Port of verify_guarded_workbook_delta (status values).
void verifyGuardedWorkbookDelta(
  Uint8List source,
  Uint8List destination,
  Map<String, Map<(int, int), GuardedCellUpdate>> updatesBySheet,
) => _verifyGuardedInner(
  source,
  destination,
  updatesBySheet,
  statusValuesOnly: true,
);

/// Port of verify_guarded_workbook_delta_text.
void verifyGuardedWorkbookDeltaText(
  Uint8List source,
  Uint8List destination,
  Map<String, Map<(int, int), GuardedCellUpdate>> updatesBySheet,
) => _verifyGuardedInner(
  source,
  destination,
  updatesBySheet,
  statusValuesOnly: false,
);

/// Port of sheet_infos_from_path (in-memory form): sheet name -> part and id.
Map<String, SheetInfo> sheetInfosFromBytes(Uint8List source) {
  _validateSourceBytes(source);
  return _projectSheetInfos(orderedWorkbookDiscovery(source));
}

/// Port of read_member: decoded bytes of one package member.
Uint8List readMember(Uint8List source, String name) {
  _validateSourceBytes(source);
  return _openArchive(source).read(name);
}
