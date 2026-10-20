part of '../ooxml.dart';

// Raw-preserving XML tree. Port of the quick-xml based tree in ooxml.rs
// (XmlNode / XmlElement / ExpandedAttribute / parse_document / serialize).
// Untouched elements and text are re-emitted from their exact source bytes,
// so an unmodified document serializes byte-for-byte identical.

const String _xmlNamespaceXml = 'http://www.w3.org/XML/1998/namespace';
const String _xmlNamespaceXmlns = 'http://www.w3.org/2000/xmlns/';

sealed class XmlNode {
  const XmlNode();
}

/// A non-element event (text, CDATA, comment, PI, declaration, doctype),
/// kept as its exact source text.
final class XmlRawNode extends XmlNode {
  const XmlRawNode(this.raw);
  final String raw;
}

class ExpandedAttribute {
  ExpandedAttribute({
    required this.qname,
    required this.namespace,
    required this.localName,
    required this.value,
  });

  String qname;

  /// Expanded namespace URI, `null` when unbound, or an
  /// `"\u0000unresolved:<prefix>"` marker for an undeclared prefix.
  String? namespace;
  String localName;

  /// Raw (still XML-escaped) attribute value, as in the source document.
  String value;
}

final class XmlElement extends XmlNode {
  XmlElement({
    required this.qname,
    this.namespace,
    List<ExpandedAttribute>? attributes,
    List<XmlNode>? children,
    this.empty = false,
    this.startRaw,
    this.endRaw,
  }) : attributes = attributes ?? <ExpandedAttribute>[],
       children = children ?? <XmlNode>[];

  /// Creates an element with a fresh start tag, the equivalent of
  /// `XmlElement::new(name)` in ooxml.rs.
  factory XmlElement.named(String name) => XmlElement(qname: name);

  String qname;
  String? namespace;
  List<ExpandedAttribute> attributes;
  List<XmlNode> children;

  /// True when the element was parsed as (or set up as) an empty element.
  bool empty;

  /// Exact source start tag, or `null` once attributes were modified.
  String? startRaw;

  /// Exact source end tag, or `null` when regenerated from [qname].
  String? endRaw;

  String get localName => _localPart(qname);

  String _startBase() {
    final raw = startRaw;
    if (raw != null) {
      if (raw.endsWith('/>')) return raw.substring(0, raw.length - 2);
      if (raw.endsWith('>')) return raw.substring(0, raw.length - 1);
      return raw;
    }
    final buffer = StringBuffer('<$qname');
    for (final attribute in attributes) {
      buffer.write(' ${attribute.qname}="${attribute.value}"');
    }
    return buffer.toString();
  }

  void _writeTo(StringBuffer out) {
    if (empty && children.isEmpty) {
      out.write('${_startBase()}/>');
      return;
    }
    out.write('${_startBase()}>');
    for (final child in children) {
      _writeNode(out, child);
    }
    out.write(endRaw ?? '</$qname>');
  }

  /// Replaces the start tag's attributes, regenerating the tag on write.
  void _resetStart() {
    startRaw = null;
  }
}

class XmlDocument {
  XmlDocument(this.nodes);
  List<XmlNode> nodes;
}

String _localPart(String qualified) {
  final index = qualified.lastIndexOf(':');
  return index < 0 ? qualified : qualified.substring(index + 1);
}

void _writeNode(StringBuffer out, XmlNode node) {
  switch (node) {
    case XmlRawNode(:final raw):
      out.write(raw);
    case XmlElement():
      node._writeTo(out);
  }
}

/// Port of `serialize_document`. Output is UTF-8.
Uint8List serializeDocument(XmlDocument document) {
  final out = StringBuffer();
  for (final node in document.nodes) {
    _writeNode(out, node);
  }
  return Uint8List.fromList(utf8.encode(out.toString()));
}

const int _maxXmlDepth = 256;
const int _maxXmlNodes = 20000000;

/// Port of `parse_document` (default limits).
XmlDocument parseDocument(Uint8List raw) =>
    _parseDocumentWithLimits(raw, _maxXmlDepth, _maxXmlNodes);

XmlDocument _parseDocumentWithLimits(Uint8List raw, int maxDepth, int maxNodes) {
  if (raw.length > _maxEntryBytes) {
    throw OoxmlError.limitExceeded('XML member exceeded the size limit');
  }
  var offset = 0;
  final roots = <XmlNode>[];
  final stack = <XmlElement>[];
  final scopes = <Map<String, String?>>[];
  var nodeCount = 0;

  void recordNode() {
    nodeCount++;
    if (nodeCount > maxNodes) {
      throw OoxmlError.limitExceeded('XML node count exceeded the limit');
    }
  }

  void push(XmlNode node) {
    if (stack.isEmpty) {
      roots.add(node);
    } else {
      stack.last.children.add(node);
    }
  }

  // A leading byte-order mark is kept as raw text so output stays identical.
  var text = '';
  if (raw.length >= 3 && raw[0] == 0xEF && raw[1] == 0xBB && raw[2] == 0xBF) {
    text = '﻿';
    offset = 3;
  }
  final String source;
  try {
    source = text + utf8.decode(raw.sublist(offset), allowMalformed: false);
  } on FormatException {
    throw OoxmlError.invalid('XML member is not valid UTF-8');
  }
  if (text.isNotEmpty) {
    roots.add(XmlRawNode(text));
  }

  final Iterable<XmlEvent> events;
  try {
    events = parseEvents(
      source,
      validateNesting: true,
      withBuffer: true,
      withLocation: true,
    );
  } catch (_) {
    throw OoxmlError.invalid('XML document is invalid');
  }

  try {
    for (final event in events) {
      final eventRaw = event.buffer!.substring(event.start!, event.stop!);
      switch (event) {
        case XmlStartElementEvent():
          recordNode();
          if (stack.length >= maxDepth) {
            throw OoxmlError.limitExceeded(
              'XML element depth exceeded the limit',
            );
          }
          final element = _openElement(eventRaw, scopes);
          if (event.isSelfClosing) {
            element.empty = true;
            push(element);
          } else {
            scopes.add(_declaredBindings(element));
            stack.add(element);
          }
        case XmlEndElementEvent():
          final element = stack.isEmpty
              ? throw OoxmlError.invalid('XML element stack is invalid')
              : stack.removeLast();
          final closingName = eventRaw
              .substring(2, eventRaw.length - 1)
              .trimRight();
          if (closingName != element.qname) {
            throw OoxmlError.invalid('XML closing tag is invalid');
          }
          element.endRaw = eventRaw;
          scopes.removeLast();
          push(element);
        default:
          recordNode();
          push(XmlRawNode(eventRaw));
      }
    }
  } on OoxmlError {
    rethrow;
  } catch (_) {
    throw OoxmlError.invalid('XML document is invalid');
  }
  if (stack.isNotEmpty || !roots.any((node) => node is XmlElement)) {
    throw OoxmlError.invalid('XML document is incomplete');
  }
  return XmlDocument(roots);
}

/// Namespace bindings declared by an element's own xmlns attributes.
/// The key is the prefix ('' for the default namespace); a null value
/// means the prefix was explicitly unbound (xmlns="").
Map<String, String?> _declaredBindings(XmlElement element) {
  final bindings = <String, String?>{};
  for (final attribute in element.attributes) {
    if (attribute.qname == 'xmlns') {
      final value = attribute.value;
      bindings[''] = value.isEmpty ? null : _decodeAttributeValue(value);
    } else if (attribute.qname.startsWith('xmlns:')) {
      bindings[attribute.qname.substring(6)] = _decodeAttributeValue(
        attribute.value,
      );
    }
  }
  return bindings;
}

String _decodeAttributeValue(String raw) => xmlUnescape(raw) ?? raw;

/// Builds the element for a raw start tag, resolving its namespace and
/// attributes against the in-scope bindings (quick-xml NsReader behavior).
XmlElement _openElement(String raw, List<Map<String, String?>> scopes) {
  final parsed = _scanStartTag(raw);
  final element = XmlElement(qname: parsed.name, startRaw: raw);
  element.attributes = [
    for (final (qname, value) in parsed.attributes)
      ExpandedAttribute(
        qname: qname,
        namespace: null,
        localName: _localPart(qname),
        value: value,
      ),
  ];
  // Own declarations are in scope for the element and its attributes.
  final ownScopes = [...scopes, _declaredBindings(element)];
  element.namespace = _resolveElementNamespace(parsed.name, ownScopes);
  for (final attribute in element.attributes) {
    attribute.namespace = _resolveAttributeNamespace(attribute.qname, ownScopes);
  }
  return element;
}

String? _lookupPrefix(List<Map<String, String?>> scopes, String prefix) {
  for (var index = scopes.length - 1; index >= 0; index--) {
    final scope = scopes[index];
    if (scope.containsKey(prefix)) return scope[prefix];
  }
  return null;
}

bool _isPrefixDeclared(List<Map<String, String?>> scopes, String prefix) {
  for (final scope in scopes) {
    if (scope.containsKey(prefix)) return true;
  }
  return false;
}

String? _resolveElementNamespace(
  String qname,
  List<Map<String, String?>> scopes,
) {
  final colon = qname.indexOf(':');
  if (colon < 0) {
    return _lookupPrefix(scopes, '');
  }
  final prefix = qname.substring(0, colon);
  if (prefix == 'xml') return _xmlNamespaceXml;
  if (prefix == 'xmlns') return _xmlNamespaceXmlns;
  if (_isPrefixDeclared(scopes, prefix)) {
    final uri = _lookupPrefix(scopes, prefix);
    return uri ?? _unresolvedNamespace(prefix);
  }
  return _unresolvedNamespace(prefix);
}

String? _resolveAttributeNamespace(
  String qname,
  List<Map<String, String?>> scopes,
) {
  final colon = qname.indexOf(':');
  if (colon < 0) return null;
  final prefix = qname.substring(0, colon);
  if (prefix == 'xmlns') return _xmlNamespaceXmlns;
  if (prefix == 'xml') return _xmlNamespaceXml;
  if (_isPrefixDeclared(scopes, prefix)) {
    return _lookupPrefix(scopes, prefix) ?? _unresolvedNamespace(prefix);
  }
  return _unresolvedNamespace(prefix);
}

String _unresolvedNamespace(String prefix) => '\u0000unresolved:$prefix';

/// Scans a raw start tag into its name and raw attribute pairs. Duplicate
/// attribute names are rejected, as quick-xml's attribute checks do.
({String name, List<(String, String)> attributes}) _scanStartTag(String raw) {
  var i = 1; // skip '<'
  final nameStart = i;
  while (i < raw.length && !_isTagDelimiter(raw.codeUnitAt(i))) {
    i++;
  }
  final name = raw.substring(nameStart, i);
  if (name.isEmpty) {
    throw OoxmlError.invalid('XML attribute is invalid');
  }
  final attributes = <(String, String)>[];
  final seen = <String>{};
  while (i < raw.length) {
    while (i < raw.length && _isXmlSpace(raw.codeUnitAt(i))) {
      i++;
    }
    if (i >= raw.length || raw[i] == '>' || raw[i] == '/') break;
    final keyStart = i;
    while (i < raw.length && raw[i] != '=' && !_isXmlSpace(raw.codeUnitAt(i))) {
      i++;
    }
    final key = raw.substring(keyStart, i);
    while (i < raw.length && _isXmlSpace(raw.codeUnitAt(i))) {
      i++;
    }
    if (i >= raw.length || raw[i] != '=' || key.isEmpty) {
      throw OoxmlError.invalid('XML attribute is invalid');
    }
    i++;
    while (i < raw.length && _isXmlSpace(raw.codeUnitAt(i))) {
      i++;
    }
    if (i >= raw.length || (raw[i] != '"' && raw[i] != "'")) {
      throw OoxmlError.invalid('XML attribute is invalid');
    }
    final quote = raw[i];
    final close = raw.indexOf(quote, i + 1);
    if (close < 0) {
      throw OoxmlError.invalid('XML attribute is invalid');
    }
    final value = raw.substring(i + 1, close);
    i = close + 1;
    if (!seen.add(key)) {
      throw OoxmlError.invalid('XML attribute is invalid');
    }
    attributes.add((key, value));
  }
  return (name: name, attributes: attributes);
}

bool _isXmlSpace(int unit) =>
    unit == 0x20 || unit == 0x09 || unit == 0x0A || unit == 0x0D;

bool _isTagDelimiter(int unit) => _isXmlSpace(unit) || unit == 0x2F || unit == 0x3E;

const _xmlEscapes = {
  '<': '&lt;',
  '>': '&gt;',
  '&': '&amp;',
  "'": '&apos;',
  '"': '&quot;',
};

/// Port of quick_xml::escape::escape (also used for BytesText::new).
String xmlEscape(String value) => value.replaceAllMapped(
  RegExp('[<>&\'"]'),
  (match) => _xmlEscapes[match.group(0)!]!,
);

/// Port of quick_xml::escape::unescape. Returns `null` on an unknown or
/// malformed reference (the Rust version returns an error there).
String? xmlUnescape(String value) {
  if (!value.contains('&')) return value;
  final out = StringBuffer();
  var i = 0;
  while (i < value.length) {
    final ch = value[i];
    if (ch != '&') {
      out.write(ch);
      i++;
      continue;
    }
    final end = value.indexOf(';', i + 1);
    if (end < 0) return null;
    final entity = value.substring(i + 1, end);
    switch (entity) {
      case 'lt':
        out.write('<');
      case 'gt':
        out.write('>');
      case 'amp':
        out.write('&');
      case 'apos':
        out.write("'");
      case 'quot':
        out.write('"');
      default:
        if (!entity.startsWith('#')) return null;
        final isHex = entity.startsWith('#x');
        final digits = entity.substring(isHex ? 2 : 1);
        final code = digits.isEmpty
            ? null
            : int.tryParse(digits, radix: isHex ? 16 : 10);
        if (code == null || code < 0 || code > 0x10FFFF) return null;
        if (code >= 0xD800 && code <= 0xDFFF) return null;
        out.writeCharCode(code);
    }
    i = end + 1;
  }
  return out.toString();
}

/// Text content of an element with entities decoded and CDATA included.
String elementText(XmlElement element) {
  final output = StringBuffer();
  _collectText(element, output);
  return output.toString();
}

void _collectText(XmlElement element, StringBuffer output) {
  for (final node in element.children) {
    switch (node) {
      case XmlRawNode(:final raw):
        if (raw.startsWith('<![CDATA[')) {
          output.write(raw.substring(9, raw.length - 3));
        } else if (!raw.startsWith('<')) {
          for (final piece in quickXmlTextPieces(raw)) {
            output.write(_decodeEntities(piece.raw));
          }
        }
      case XmlElement():
        _collectText(node, output);
    }
  }
}

/// One event of a quick-xml text run: plain text, or a general reference
/// such as `&amp;` (quick-xml 0.41 emits these as separate events).
class QuickXmlTextPiece {
  const QuickXmlTextPiece({required this.isReference, required this.raw});

  /// True for `&name;`; [raw] is then the whole reference including `&` and `;`.
  final bool isReference;
  final String raw;

  /// Event payload: the reference name for references, the text otherwise.
  String get payload => isReference ? raw.substring(1, raw.length - 1) : raw;
}

/// Splits a raw text run into the events quick-xml would produce for it.
List<QuickXmlTextPiece> quickXmlTextPieces(String raw) {
  final pieces = <QuickXmlTextPiece>[];
  var start = 0;
  var index = 0;
  while (index < raw.length) {
    if (raw[index] != '&') {
      index++;
      continue;
    }
    final end = raw.indexOf(';', index + 1);
    if (end < 0) {
      throw const OoxmlError.invalid('XML text reference is invalid');
    }
    if (index > start) {
      pieces.add(
        QuickXmlTextPiece(isReference: false, raw: raw.substring(start, index)),
      );
    }
    pieces.add(
      QuickXmlTextPiece(isReference: true, raw: raw.substring(index, end + 1)),
    );
    index = end + 1;
    start = index;
  }
  if (start < raw.length) {
    pieces.add(
      QuickXmlTextPiece(isReference: false, raw: raw.substring(start)),
    );
  }
  return pieces;
}

String _decodeEntities(String value) => xmlUnescape(value) ?? value;

/// Replaces the element's children with one escaped text run
/// (port of set_element_text).
void setElementText(XmlElement element, String value) {
  element.children.clear();
  if (value.isNotEmpty) {
    element.children.add(XmlRawNode(xmlEscape(value)));
  }
  element.endRaw = null;
  element.empty = value.isEmpty;
}

XmlElement textElement(String name, String value) {
  final element = XmlElement.named(name);
  setElementText(element, value);
  return element;
}

// ---- Attribute access (port of attribute / set_attribute / ...) ----

/// Raw (still escaped) value of the first attribute whose local name matches.
String? rawAttributeByLocalName(XmlElement element, String local) {
  for (final attribute in element.attributes) {
    if (_localPart(attribute.qname) == local) return attribute.value;
  }
  return null;
}

void _setAttributes(XmlElement element, List<ExpandedAttribute> attributes) {
  element.attributes = attributes;
  element._resetStart();
}

/// Port of set_attribute: matches by local name, keeps position, else
/// appends an unqualified attribute. The value is written raw.
void setAttribute(XmlElement element, String local, String value) {
  final attributes = [...element.attributes];
  final index = attributes.indexWhere(
    (attribute) => _localPart(attribute.qname) == local,
  );
  if (index >= 0) {
    attributes[index].value = value;
  } else {
    attributes.add(
      ExpandedAttribute(
        qname: local,
        namespace: null,
        localName: local,
        value: value,
      ),
    );
  }
  _setAttributes(element, attributes);
}

/// Port of set_unqualified_attribute: matches the exact qualified name.
void setUnqualifiedAttribute(XmlElement element, String name, String value) {
  final attributes = [...element.attributes];
  final index = attributes.indexWhere((attribute) => attribute.qname == name);
  if (index >= 0) {
    attributes[index].value = value;
  } else {
    attributes.add(
      ExpandedAttribute(
        qname: name,
        namespace: null,
        localName: name,
        value: value,
      ),
    );
  }
  _setAttributes(element, attributes);
}

/// Port of remove_attribute: removes every attribute with the local name.
void removeAttribute(XmlElement element, String local) {
  _setAttributes(
    element,
    element.attributes
        .where((attribute) => _localPart(attribute.qname) != local)
        .toList(),
  );
}

bool hasLocalName(XmlElement element, String expected) =>
    _localPart(element.qname) == expected;

bool hasExpandedName(
  XmlElement element,
  String localName,
  List<String> namespaces,
) =>
    hasLocalName(element, localName) &&
    element.namespace != null &&
    namespaces.contains(element.namespace);

/// Port of attribute(): first raw value whose local name matches.
String? attribute(XmlElement element, String local) =>
    rawAttributeByLocalName(element, local);

/// Port of bounded_expanded_attribute / bounded_unqualified_attribute.
String? boundedExpandedAttribute(
  XmlElement element,
  String localName,
  bool Function(String? namespace) namespaceMatches,
  int maxBytes,
  String field,
) {
  String? value;
  for (final attribute in element.attributes) {
    if (attribute.localName != localName ||
        !namespaceMatches(attribute.namespace)) {
      continue;
    }
    if (value != null) {
      throw OoxmlError.invalid('$field is ambiguous');
    }
    if (attribute.value.length > maxBytes) {
      throw OoxmlError.limitExceeded('$field exceeded the size limit');
    }
    final decoded = xmlUnescape(attribute.value);
    if (decoded == null) {
      throw OoxmlError.invalid('$field is invalid');
    }
    if (decoded.length > maxBytes) {
      throw OoxmlError.limitExceeded('$field exceeded the size limit');
    }
    value = decoded;
  }
  return value;
}

String? boundedUnqualifiedAttribute(
  XmlElement element,
  String localName,
  int maxBytes,
  String field,
) => boundedExpandedAttribute(
  element,
  localName,
  (namespace) => namespace == null,
  maxBytes,
  field,
);

String? boundedOfficeRelationshipAttribute(
  XmlElement element,
  String localName,
  int maxBytes,
  String field,
) => boundedExpandedAttribute(
  element,
  localName,
  (namespace) =>
      namespace == _transitionalRelationshipNamespace ||
      namespace == _strictRelationshipNamespace,
  maxBytes,
  field,
);

/// Port of bounded_qualified_attribute: exact qualified-name match.
String? boundedQualifiedAttribute(
  XmlElement element,
  String qualifiedName,
  int maxBytes,
  String field,
) {
  String? value;
  for (final attribute in element.attributes) {
    if (attribute.qname != qualifiedName) continue;
    if (value != null) {
      throw OoxmlError.invalid('$field is ambiguous');
    }
    if (attribute.value.length > maxBytes) {
      throw OoxmlError.limitExceeded('$field exceeded the size limit');
    }
    final decoded = xmlUnescape(attribute.value);
    if (decoded == null) {
      throw OoxmlError.invalid('$field is invalid');
    }
    if (decoded.length > maxBytes) {
      throw OoxmlError.limitExceeded('$field exceeded the size limit');
    }
    value = decoded;
  }
  return value;
}

/// Direct element children whose local name matches.
Iterable<XmlElement> childElements(XmlElement element, String name) =>
    element.children.whereType<XmlElement>().where(
      (child) => hasLocalName(child, name),
    );

Iterable<XmlElement> childElementsExpanded(
  XmlElement element,
  String localName,
  List<String> namespaces,
) => element.children.whereType<XmlElement>().where(
  (child) => hasExpandedName(child, localName, namespaces),
);

XmlElement? childElement(XmlElement element, String name) {
  for (final child in childElements(element, name)) {
    return child;
  }
  return null;
}

XmlElement singleChildElementExpanded(
  XmlElement parent,
  String localName,
  List<String> namespaces,
  String error,
) {
  final matches = childElementsExpanded(parent, localName, namespaces).toList();
  if (matches.length != 1) {
    throw OoxmlError.invalid(error);
  }
  return matches.single;
}
