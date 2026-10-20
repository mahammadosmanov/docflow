part of '../ooxml.dart';

// Workbook discovery, relationship validation and package-signature checks.
// Port of the discovery half of ooxml.rs (ordered_workbook_discovery,
// authenticated_workbook_parts, relationship helpers, read_workbook_*).

const int _maxU32AttributeBytesInternal = _maxU32AttributeBytes;

const String _transitionalSignatureOrigin =
    'http://schemas.openxmlformats.org/package/2006/relationships/digital-signature/origin';
const String _strictSignatureOrigin =
    'http://purl.oclc.org/ooxml/package/relationships/digital-signature/origin';

const List<String> _knownWorkbookRelationshipClasses = [
  'worksheet',
  'styles',
  'theme',
  'sharedStrings',
  'calcChain',
  'externalLink',
  'connections',
  'vbaProject',
  'vbaProjectSignature',
  'xlMacrosheet',
  'xlIntlMacrosheet',
  'pivotCacheDefinition',
  'customXml',
  'customXmlMappings',
  'xmlMaps',
  'metadata',
  'sheetMetadata',
  'person',
  'richValueRel',
  'slicerCache',
  'timelineCache',
  'chartsheet',
  'dialogsheet',
  'table',
  'queryTable',
  'pivotTable',
  'printerSettings',
  'drawing',
  'vmlDrawing',
  'image',
  'rdRichValueTypes',
  'rdRichValueStructure',
  'rdArray',
  'model',
  'dataModel',
  'webextension',
  'feature',
  'features',
  'calcFeatures',
  'volatileDependencies',
  'customData',
  'customDataProperties',
  'customProperty',
  'customProperties',
  'extendedProperties',
  'package',
  'hyperlink',
  'customUI',
  'attachedToolbars',
  'activeXControlBinary',
  'usernames',
  'revisionLog',
  'revisionHeaders',
];

const List<String> _knownWorksheetRelationshipClasses = [
  'hyperlink',
  'drawing',
  'vmlDrawing',
  'comments',
  'threadedComment',
  'table',
  'pivotTable',
  'queryTable',
  'printerSettings',
  'oleObject',
  'ctrlProp',
  'image',
  'slicer',
  'timeline',
  'customXml',
  'metadata',
  'chart',
  'chartUserShapes',
  'diagramData',
  'diagramLayout',
  'diagramStyle',
  'diagramColors',
  'package',
  'webextension',
  'customProperty',
  'feature',
];

bool _equalsIgnoreAsciiCase(String a, String b) =>
    a.length == b.length && a.toLowerCase() == b.toLowerCase();

/// Port of validate_source: the in-memory package is size-checked the same
/// way the file-based entry points check the file.
void _validateSourceBytes(Uint8List bytes) {
  if (bytes.length > maxPackageBytes) {
    throw const OoxmlError.limitExceeded(
      'workbook package exceeded the size limit',
    );
  }
}

/// Port of document_root_local: exactly one element root with the local name.
XmlElement _documentRootLocal(
  XmlDocument document,
  String localName,
  String error,
) {
  final roots = document.nodes.whereType<XmlElement>().toList();
  if (roots.isEmpty || !hasLocalName(roots.first, localName)) {
    throw OoxmlError.invalid(error);
  }
  if (roots.length > 1) {
    throw OoxmlError.invalid(error);
  }
  return roots.first;
}

/// Port of validate_relationship_document_namespace.
XmlElement _validateRelationshipDocumentNamespace(
  XmlDocument document,
  String subject,
) {
  final root = _documentRootLocal(
    document,
    'Relationships',
    '$subject relationships root is invalid',
  );
  if (!hasExpandedName(root, 'Relationships', _packageRelationshipNamespaces)) {
    throw OoxmlError.invalid('$subject relationships namespace is invalid');
  }
  return root;
}

Iterable<XmlElement> _relationshipElements(XmlElement root) =>
    childElementsExpanded(root, 'Relationship', _packageRelationshipNamespaces);

/// Port of relationship_class: OOXML and Microsoft office relationship
/// classes, with an optional ui/ prefix stripped.
String? _relationshipClass(String relationshipType) {
  final ooxml = _ooxmlRelationshipClass(relationshipType);
  if (ooxml != null) return ooxml;
  const microsoftPrefix = 'http://schemas.microsoft.com/office/';
  if (!relationshipType.startsWith(microsoftPrefix)) return null;
  final rest = relationshipType.substring(microsoftPrefix.length);
  final relIndex = rest.indexOf('/relationships/');
  if (relIndex < 0) return null;
  var cls = rest.substring(relIndex + '/relationships/'.length);
  if (cls.startsWith('ui/')) cls = cls.substring(3);
  return (cls.isNotEmpty && !cls.contains('/')) ? cls : null;
}

String? _ooxmlRelationshipClass(String relationshipType) {
  String? cls;
  for (final prefix in const [
    _transitionalRelationshipPrefix,
    _strictRelationshipPrefix,
    'http://schemas.openxmlformats.org/package/2006/relationships/',
    'http://purl.oclc.org/ooxml/package/relationships/',
  ]) {
    if (relationshipType.startsWith(prefix)) {
      cls = relationshipType.substring(prefix.length);
      break;
    }
  }
  if (cls == null) return null;
  if (cls.startsWith('ui/')) cls = cls.substring(3);
  return (cls.isNotEmpty && !cls.contains('/')) ? cls : null;
}

/// Port of relationship_target_mode_is_external.
bool _relationshipTargetModeIsExternal(XmlElement relationship, String subject) {
  final context = '$subject relationship target mode';
  final mode = boundedUnqualifiedAttribute(
    relationship,
    'TargetMode',
    _maxStateBytes,
    context,
  );
  switch (mode) {
    case null:
    case 'Internal':
      return false;
    case 'External':
      return true;
    default:
      throw OoxmlError.invalid('$subject relationship target mode is invalid');
  }
}

/// Port of relationship_target_is_external (by relationship id).
bool _relationshipTargetIsExternal(XmlDocument document, String relationshipId) {
  final root = _validateRelationshipDocumentNamespace(document, 'workbook');
  for (final element in _relationshipElements(root)) {
    final id = boundedUnqualifiedAttribute(
      element,
      'Id',
      _maxRelationshipIdBytes,
      'workbook relationship identity',
    );
    if (id == relationshipId) {
      return _relationshipTargetModeIsExternal(element, 'workbook');
    }
  }
  return false;
}

/// Port of relationship_attribute.
String? _relationshipAttribute(
  XmlDocument document,
  String relationshipId,
  String name,
) {
  final root = _validateRelationshipDocumentNamespace(document, 'workbook');
  for (final element in _relationshipElements(root)) {
    final id = boundedUnqualifiedAttribute(
      element,
      'Id',
      _maxRelationshipIdBytes,
      'workbook relationship identity',
    );
    if (id == relationshipId) {
      return boundedUnqualifiedAttribute(
        element,
        name,
        _maxRelationshipTypeBytes,
        'workbook relationship type',
      );
    }
  }
  return null;
}

/// Port of has_relationship_type.
bool _hasRelationshipType(
  XmlDocument document,
  bool Function(String relationshipType) predicate,
) {
  final root = _validateRelationshipDocumentNamespace(document, 'workbook');
  for (final element in _relationshipElements(root)) {
    final type = boundedUnqualifiedAttribute(
      element,
      'Type',
      _maxRelationshipTypeBytes,
      'workbook relationship type',
    );
    if (type != null && predicate(type)) return true;
  }
  return false;
}

/// Port of relationship_type_count.
int _relationshipTypeCount(XmlDocument document, String expectedClass) {
  var count = 0;
  final root = _validateRelationshipDocumentNamespace(document, 'workbook');
  for (final element in _relationshipElements(root)) {
    final type = boundedUnqualifiedAttribute(
      element,
      'Type',
      _maxRelationshipTypeBytes,
      'workbook relationship type',
    );
    final cls = type == null ? null : _relationshipClass(type);
    if (cls == expectedClass) count++;
  }
  return count;
}

/// Port of relationship_targets: Id -> raw Target, duplicates rejected.
Map<String, String> _relationshipTargets(XmlDocument document) {
  final targets = <String, String>{};
  final root = _validateRelationshipDocumentNamespace(document, 'workbook');
  for (final element in _relationshipElements(root)) {
    final id = boundedUnqualifiedAttribute(
      element,
      'Id',
      _maxRelationshipIdBytes,
      'workbook relationship identity',
    );
    if (id == null) {
      throw const OoxmlError.invalid('workbook relationship identity is missing');
    }
    final target = boundedUnqualifiedAttribute(
      element,
      'Target',
      _maxRelationshipTargetBytes,
      'workbook relationship target',
    );
    if (target == null) {
      throw const OoxmlError.invalid('workbook relationship target is missing');
    }
    if (targets.containsKey(id)) {
      throw const OoxmlError.invalid('duplicate workbook relationship identity');
    }
    targets[id] = target;
  }
  return targets;
}

/// Port of internal_relationship_target_for_class.
String? _internalRelationshipTargetForClass(
  XmlDocument document,
  String sourcePartPath,
  String expectedClass,
) {
  final root = _validateRelationshipDocumentNamespace(document, 'workbook');
  String? target;
  for (final relationship in _relationshipElements(root)) {
    final type = boundedUnqualifiedAttribute(
      relationship,
      'Type',
      _maxRelationshipTypeBytes,
      'workbook relationship type',
    );
    if (type == null) {
      throw const OoxmlError.invalid('workbook relationship type is missing');
    }
    if (_relationshipClass(type) != expectedClass) continue;
    if (target != null) {
      throw const OoxmlError.invalid('workbook relationship class is ambiguous');
    }
    if (_relationshipTargetModeIsExternal(relationship, 'workbook')) {
      throw const OoxmlError.invalid(
        'external workbook relationship target is not allowed',
      );
    }
    final rawTarget = boundedUnqualifiedAttribute(
      relationship,
      'Target',
      _maxRelationshipTargetBytes,
      'workbook relationship target',
    );
    if (rawTarget == null) {
      throw const OoxmlError.invalid('workbook relationship target is missing');
    }
    target = _normalizeWorkbookRelationshipTarget(sourcePartPath, rawTarget);
  }
  return target;
}

/// Port of validate_workbook_relationship_classes. Returns true when a
/// vbaProject relationship is present.
bool _validateWorkbookRelationshipClasses(
  _ZipPackage archive,
  XmlDocument document,
  String workbookPartPath,
) {
  final root = _validateRelationshipDocumentNamespace(document, 'workbook');
  final relationshipTargets = <String>{};
  final internalTargets = <(String, bool)>[];
  var hasVbaProject = false;
  for (final relationship in _relationshipElements(root)) {
    final type = boundedUnqualifiedAttribute(
      relationship,
      'Type',
      _maxRelationshipTypeBytes,
      'workbook relationship type',
    );
    if (type == null) {
      throw const OoxmlError.invalid('workbook relationship type is missing');
    }
    final cls = _relationshipClass(type);
    if (cls == null || !_knownWorkbookRelationshipClasses.contains(cls)) {
      throw const OoxmlError.invalid('unknown workbook relationship class');
    }
    if (_relationshipTargetModeIsExternal(relationship, 'workbook')) {
      if (cls == 'externalLink' || cls == 'hyperlink') continue;
      throw const OoxmlError.invalid(
        'external workbook relationship target is not allowed',
      );
    }
    final target = boundedUnqualifiedAttribute(
      relationship,
      'Target',
      _maxRelationshipTargetBytes,
      'workbook relationship target',
    );
    if (target == null) {
      throw const OoxmlError.invalid('workbook relationship target is missing');
    }
    final normalized = _normalizeRelationshipTarget(workbookPartPath, target);
    if (!relationshipTargets.add(normalized)) {
      throw const OoxmlError.invalid('duplicate workbook relationship target');
    }
    internalTargets.add((normalized, cls == 'vbaProject'));
    hasVbaProject |= cls == 'vbaProject';
  }
  for (final (normalized, isVbaProject) in internalTargets) {
    if (!_hasEntry(archive, normalized)) {
      throw OoxmlError.invalid(
        isVbaProject
            ? 'VBA project package member is missing'
            : 'workbook relationship package member is missing',
      );
    }
  }
  return hasVbaProject;
}

/// Port of validate_worksheet_relationship_classes.
void _validateWorksheetRelationshipClasses(
  _ZipPackage archive,
  XmlDocument document,
  String worksheetPartPath,
) {
  final root = _validateRelationshipDocumentNamespace(document, 'worksheet');
  final relationshipIds = <String>{};
  final relationshipTargets = <String>{};
  for (final relationship in _relationshipElements(root)) {
    final id = boundedUnqualifiedAttribute(
      relationship,
      'Id',
      _maxRelationshipIdBytes,
      'worksheet relationship identity',
    );
    if (id == null) {
      throw const OoxmlError.invalid('worksheet relationship identity is missing');
    }
    if (!relationshipIds.add(id)) {
      throw const OoxmlError.invalid('duplicate worksheet relationship identity');
    }
    final type = boundedUnqualifiedAttribute(
      relationship,
      'Type',
      _maxRelationshipTypeBytes,
      'worksheet relationship type',
    );
    if (type == null) {
      throw const OoxmlError.invalid('worksheet relationship type is missing');
    }
    final cls = _relationshipClass(type);
    if (cls == null || !_knownWorksheetRelationshipClasses.contains(cls)) {
      throw const OoxmlError.invalid('unknown worksheet relationship class');
    }
    final target = boundedUnqualifiedAttribute(
      relationship,
      'Target',
      _maxRelationshipTargetBytes,
      'worksheet relationship target',
    );
    if (target == null) {
      throw const OoxmlError.invalid('worksheet relationship target is missing');
    }
    if (target.isEmpty) {
      throw const OoxmlError.invalid('worksheet relationship target is missing');
    }
    if (_relationshipTargetModeIsExternal(relationship, 'worksheet')) {
      throw const OoxmlError.invalid(
        'external worksheet relationship target is not allowed',
      );
    }
    final normalized = _normalizeRelationshipTarget(worksheetPartPath, target);
    if (!relationshipTargets.add(normalized)) {
      throw const OoxmlError.invalid('duplicate worksheet relationship target');
    }
  }
  for (final normalized in relationshipTargets) {
    if (!_hasEntry(archive, normalized)) {
      throw const OoxmlError.invalid(
        'worksheet relationship package member is missing',
      );
    }
  }
}

/// Port of related_collection_count (e.g. connections).
int _relatedCollectionCount(
  _ZipPackage archive,
  XmlDocument relationships,
  String workbookPartPath,
  String expectedClass,
  String collectionName,
  String itemName,
) {
  var count = 0;
  final root = _validateRelationshipDocumentNamespace(relationships, 'workbook');
  for (final relationship in _relationshipElements(root)) {
    final type = boundedUnqualifiedAttribute(
      relationship,
      'Type',
      _maxRelationshipTypeBytes,
      'workbook relationship type',
    );
    if (type == null) continue;
    if (_relationshipClass(type) != expectedClass) continue;
    if (_relationshipTargetModeIsExternal(relationship, 'workbook')) {
      throw const OoxmlError.invalid(
        'external workbook relationship target is not allowed',
      );
    }
    final target = boundedUnqualifiedAttribute(
      relationship,
      'Target',
      _maxRelationshipTargetBytes,
      'workbook relationship target',
    );
    if (target == null) {
      throw const OoxmlError.invalid('workbook relationship target is missing');
    }
    final partPath = _normalizeWorkbookRelationshipTarget(
      workbookPartPath,
      target,
    );
    final document = parseDocument(archive.read(partPath));
    final collection = _documentRootLocal(
      document,
      collectionName,
      'relationship count is invalid',
    );
    if (!hasExpandedName(collection, collectionName, _workbookNamespaces)) {
      throw const OoxmlError.invalid('relationship count is invalid');
    }
    final partCount = childElementsExpanded(
      collection,
      itemName,
      _workbookNamespaces,
    ).length;
    final declared = boundedUnqualifiedAttribute(
      collection,
      'count',
      _maxU32AttributeBytesInternal,
      'relationship count',
    );
    if (declared != null) {
      final parsed = int.tryParse(declared);
      if (parsed == null || parsed < 0 || parsed != partCount) {
        throw const OoxmlError.invalid('relationship count is invalid');
      }
    }
    count += partCount;
    if (count > 0xFFFFFFFF) {
      throw const OoxmlError.invalid('relationship count is invalid');
    }
  }
  return count;
}

/// Port of worksheet_relationship_type_count.
int _worksheetRelationshipTypeCount(
  _ZipPackage archive,
  List<WorksheetDescriptor> worksheets,
  String expectedClass,
) {
  var count = 0;
  for (final worksheet in worksheets) {
    final relationshipsPath = _relationshipPartPath(worksheet.partPath);
    if (!_hasEntry(archive, relationshipsPath)) continue;
    final relationships = parseDocument(archive.read(relationshipsPath));
    _validateWorksheetRelationshipClasses(
      archive,
      relationships,
      worksheet.partPath,
    );
    count += _relationshipTypeCount(relationships, expectedClass);
    if (count > 0xFFFFFFFF) {
      throw const OoxmlError.invalid('relationship count is invalid');
    }
  }
  return count;
}

/// Port of normalize_package_root_target.
String _normalizePackageRootTarget(String target) {
  final decoded = _percentDecodeTarget(target).replaceAll('\\', '/');
  if (decoded.split('/').any((segment) => segment == '..')) {
    throw const OoxmlError.invalid('worksheet relationship escapes the package');
  }
  return _normalizeRelationshipTargetFromParts(<String>[], target);
}

/// Port of normalize_workbook_relationship_target.
String _normalizeWorkbookRelationshipTarget(
  String workbookPartPath,
  String target,
) {
  final decoded = _percentDecodeTarget(target).replaceAll('\\', '/');
  if (decoded.split('/').any((segment) => segment == '..')) {
    throw const OoxmlError.invalid('worksheet relationship escapes the package');
  }
  return _normalizeRelationshipTarget(workbookPartPath, target);
}

/// Port of normalize_relationship_target.
String _normalizeRelationshipTarget(String sourcePartPath, String target) {
  final sourceParts = sourcePartPath.split('/').toList();
  if (sourceParts.isEmpty ||
      sourceParts.any((part) => part.isEmpty || part == '.' || part == '..')) {
    throw const OoxmlError.invalid('worksheet relationship escapes the package');
  }
  sourceParts.removeLast();
  return _normalizeRelationshipTargetFromParts(sourceParts, target);
}

/// Port of normalize_relationship_target_from_parts.
String _normalizeRelationshipTargetFromParts(
  List<String> sourceParts,
  String target,
) {
  bool invalidShape(String value) =>
      value.isEmpty ||
      value.startsWith('//') ||
      value.contains('?') ||
      value.contains('#') ||
      value.contains('\u0000') ||
      value.contains(':');
  if (invalidShape(target)) {
    throw const OoxmlError.invalid('worksheet relationship escapes the package');
  }
  final decoded = _percentDecodeTarget(target).replaceAll('\\', '/');
  if (invalidShape(decoded)) {
    throw const OoxmlError.invalid('worksheet relationship escapes the package');
  }
  final absolute = decoded.startsWith('/');
  final parts = absolute ? <String>[] : [...sourceParts];
  final trimmed = decoded.replaceFirst(RegExp(r'^/+'), '');
  for (final component in trimmed.split('/')) {
    switch (component) {
      case '':
        throw const OoxmlError.invalid('worksheet relationship escapes the package');
      case '.':
        break;
      case '..':
        if (parts.isEmpty) {
          throw const OoxmlError.invalid('worksheet relationship escapes the package');
        }
        parts.removeLast();
      default:
        parts.add(component);
    }
  }
  if (parts.isEmpty) {
    throw const OoxmlError.invalid('worksheet relationship escapes the package');
  }
  return parts.join('/');
}

/// Port of relationship_part_path (`dir/_rels/name.rels`).
String _relationshipPartPath(String partPath) {
  final slash = partPath.lastIndexOf('/');
  final fileName = slash < 0 ? partPath : partPath.substring(slash + 1);
  if (fileName.isEmpty || fileName == '.' || fileName == '..') {
    throw const OoxmlError.invalid('relationship source part path is invalid');
  }
  final parent = slash < 0 ? '' : partPath.substring(0, slash);
  if (parent.isEmpty) {
    return '_rels/$fileName.rels';
  }
  return '$parent/_rels/$fileName.rels';
}

/// Port of percent_decode_target. Malformed escapes and invalid UTF-8 are
/// rejected.
String _percentDecodeTarget(String target) {
  final bytes = utf8.encode(target);
  final decoded = <int>[];
  var index = 0;
  while (index < bytes.length) {
    if (bytes[index] == 0x25) {
      if (index + 3 > bytes.length) {
        throw const OoxmlError.invalid('worksheet relationship target is invalid');
      }
      final pair = utf8.decode(bytes.sublist(index + 1, index + 3), allowMalformed: true);
      final value = int.tryParse(pair, radix: 16);
      if (value == null || pair.length != 2 || !RegExp(r'^[0-9a-fA-F]{2}$').hasMatch(pair)) {
        throw const OoxmlError.invalid('worksheet relationship target is invalid');
      }
      decoded.add(value);
      index += 3;
    } else {
      decoded.add(bytes[index]);
      index++;
    }
  }
  try {
    return utf8.decode(decoded);
  } on FormatException {
    throw const OoxmlError.invalid('worksheet relationship target is invalid');
  }
}

/// Port of authenticated_workbook_parts: resolves the workbook part from the
/// package-level office document relationship.
({String workbook, String relationships}) _authenticatedWorkbookParts(
  _ZipPackage archive,
) {
  final packageRelationships = parseDocument(archive.read('_rels/.rels'));
  final root = _validateRelationshipDocumentNamespace(
    packageRelationships,
    'package',
  );
  String? workbookPart;
  for (final relationship in _relationshipElements(root)) {
    final type = boundedUnqualifiedAttribute(
      relationship,
      'Type',
      _maxRelationshipTypeBytes,
      'package relationship type',
    );
    final isOfficeDocument = type != null &&
        (type == _transitionalOfficeDocumentRelationship ||
            type == _strictOfficeDocumentRelationship);
    if (!isOfficeDocument) continue;
    if (workbookPart != null) {
      throw const OoxmlError.invalid('package workbook relationship is ambiguous');
    }
    final id = boundedUnqualifiedAttribute(
      relationship,
      'Id',
      _maxRelationshipIdBytes,
      'package workbook relationship identity',
    );
    if (id == null) {
      throw const OoxmlError.invalid(
        'package workbook relationship identity is missing',
      );
    }
    if (_relationshipTargetModeIsExternal(relationship, 'package workbook')) {
      throw const OoxmlError.invalid(
        'external package workbook relationship target is not allowed',
      );
    }
    final target = boundedUnqualifiedAttribute(
      relationship,
      'Target',
      _maxRelationshipTargetBytes,
      'package workbook relationship target',
    );
    if (target == null) {
      throw const OoxmlError.invalid(
        'package workbook relationship target is missing',
      );
    }
    workbookPart = _normalizePackageRootTarget(target);
  }
  final workbook = workbookPart;
  if (workbook == null) {
    throw const OoxmlError.invalid('package workbook relationship is missing');
  }
  if (!_hasEntry(archive, workbook)) {
    throw const OoxmlError.invalid('workbook package member is missing');
  }
  final relationships = _relationshipPartPath(workbook);
  if (!_hasEntry(archive, relationships)) {
    throw const OoxmlError.invalid(
      'workbook relationships package member is missing',
    );
  }
  return (workbook: workbook, relationships: relationships);
}

/// Port of ordered_workbook_discovery: sheet order, identities, visibility,
/// and workbook safety facts.
OrderedWorkbookDiscovery orderedWorkbookDiscovery(Uint8List source) {
  final archive = _openArchive(source);
  final parts = _authenticatedWorkbookParts(archive);
  final workbook = parseDocument(archive.read(parts.workbook));
  final relationships = parseDocument(archive.read(parts.relationships));
  _validateRelationshipDocumentNamespace(relationships, 'workbook');
  final relationTargets = _relationshipTargets(relationships);
  final connectionCount = _relatedCollectionCount(
    archive,
    relationships,
    parts.workbook,
    'connections',
    'connections',
    'connection',
  );
  var safety = WorkbookSafetyFacts(
    hasVbaProject: _hasEntry(archive, 'xl/vbaProject.bin'),
    hasXlmMacroSheet: _hasRelationshipType(relationships, (type) {
      final cls = _relationshipClass(type);
      return cls == 'xlMacrosheet' || cls == 'xlIntlMacrosheet';
    }),
    externalLinkCount: _relationshipTypeCount(relationships, 'externalLink'),
    connectionCount: connectionCount,
  );
  final worksheets = <WorksheetDescriptor>[];
  final names = <String>{};
  final sheetIds = <int>{};
  final relationshipIds = <String>{};
  final partPaths = <String>{};
  final workbookRoot = _documentRootLocal(
    workbook,
    'workbook',
    'workbook root is invalid',
  );
  if (!hasExpandedName(workbookRoot, 'workbook', _workbookNamespaces)) {
    throw const OoxmlError.invalid('workbook namespace is invalid');
  }
  safety = safety.copyWith(
    hasWorkbookProtection: childElementsExpanded(
      workbookRoot,
      'workbookProtection',
      _workbookNamespaces,
    ).isNotEmpty,
  );
  final sheets = singleChildElementExpanded(
    workbookRoot,
    'sheets',
    _workbookNamespaces,
    'workbook sheets container is invalid',
  );
  var index = 0;
  for (final element in childElementsExpanded(
    sheets,
    'sheet',
    _workbookNamespaces,
  )) {
    if (index >= _maxWorksheets) {
      throw const OoxmlError.limitExceeded('workbook has too many worksheets');
    }
    final name = boundedUnqualifiedAttribute(
      element,
      'name',
      _maxSheetNameBytes,
      'workbook sheet name',
    );
    if (name == null) {
      throw const OoxmlError.invalid('workbook sheet name is missing');
    }
    if (!names.add(name)) {
      throw const OoxmlError.invalid('duplicate workbook sheet name');
    }
    final relationshipId = boundedOfficeRelationshipAttribute(
      element,
      'id',
      _maxRelationshipIdBytes,
      'workbook sheet relationship identity',
    );
    if (relationshipId == null &&
        boundedQualifiedAttribute(
              element,
              'r:id',
              _maxRelationshipIdBytes,
              'workbook sheet relationship identity',
            ) !=
            null) {
      throw const OoxmlError.invalid('workbook relationship namespace is invalid');
    }
    if (relationshipId == null) {
      throw const OoxmlError.invalid('workbook sheet relationship is missing');
    }
    if (!relationshipIds.add(relationshipId)) {
      throw const OoxmlError.invalid(
        'duplicate workbook sheet relationship identity',
      );
    }
    final sheetIdText = boundedUnqualifiedAttribute(
      element,
      'sheetId',
      _maxU32AttributeBytes,
      'workbook sheet identity',
    );
    final sheetId = sheetIdText == null ? null : _parseU32(sheetIdText);
    if (sheetId == null) {
      throw const OoxmlError.invalid('workbook sheet identity is invalid');
    }
    if (!sheetIds.add(sheetId)) {
      throw const OoxmlError.invalid('duplicate workbook sheet identity');
    }
    final target = relationTargets[relationshipId];
    if (target == null) {
      throw const OoxmlError.invalid('workbook sheet target is missing');
    }
    if (_relationshipTargetIsExternal(relationships, relationshipId)) {
      throw const OoxmlError.invalid(
        'external workbook relationship target is not allowed',
      );
    }
    final relationshipType = _relationshipAttribute(
      relationships,
      relationshipId,
      'Type',
    );
    if (relationshipType == null) {
      throw const OoxmlError.invalid(
        'workbook sheet relationship type is missing',
      );
    }
    if (_ooxmlRelationshipClass(relationshipType) != 'worksheet') {
      throw const OoxmlError.invalid('workbook sheet relationship is not a worksheet');
    }
    final state = boundedUnqualifiedAttribute(
      element,
      'state',
      _maxStateBytes,
      'worksheet visibility state',
    );
    final WorksheetVisibility visibility;
    switch (state) {
      case null:
      case 'visible':
        visibility = WorksheetVisibility.visible;
      case 'hidden':
        visibility = WorksheetVisibility.hidden;
      case 'veryHidden':
        visibility = WorksheetVisibility.veryHidden;
      default:
        throw const OoxmlError.invalid('worksheet visibility state is invalid');
    }
    final partPath = _normalizeWorkbookRelationshipTarget(parts.workbook, target);
    if (!partPaths.add(partPath)) {
      throw const OoxmlError.invalid('duplicate worksheet part path');
    }
    if (!_hasEntry(archive, partPath)) {
      throw const OoxmlError.invalid('worksheet package member is missing');
    }
    worksheets.add(
      WorksheetDescriptor(
        workbookIndex: index + 1,
        sheetId: sheetId,
        relationshipId: relationshipId,
        relationshipType: relationshipType,
        partPath: partPath,
        name: name,
        visibility: visibility,
      ),
    );
    index++;
  }
  if (worksheets.isEmpty) {
    throw const OoxmlError.invalid('workbook contains no worksheets');
  }
  final vbaFromRelationships = _validateWorkbookRelationshipClasses(
    archive,
    relationships,
    parts.workbook,
  );
  safety = safety.copyWith(
    hasVbaProject: safety.hasVbaProject || vbaFromRelationships,
    queryTableCount: _worksheetRelationshipTypeCount(
      archive,
      worksheets,
      'queryTable',
    ),
  );
  return OrderedWorkbookDiscovery(worksheets: worksheets, safety: safety);
}

/// u32 parse matching Rust's `str::parse::<u32>` (no sign, no overflow).
int? _parseU32(String value) {
  if (value.isEmpty || !RegExp(r'^[0-9]+$').hasMatch(value)) return null;
  final parsed = int.tryParse(value);
  if (parsed == null || parsed > 0xFFFFFFFF) return null;
  return parsed;
}

Map<String, SheetInfo> _projectSheetInfos(OrderedWorkbookDiscovery discovery) {
  return {
    for (final worksheet in discovery.worksheets)
      worksheet.name: SheetInfo(
        path: worksheet.partPath,
        sheetId: worksheet.sheetId,
      ),
  };
}

/// Port of read_workbook_and_discovery.
(OrderedWorkbookDiscovery, WorkbookData) readWorkbookAndDiscovery(
  Uint8List source,
) {
  final discovery = orderedWorkbookDiscovery(source);
  final infos = _projectSheetInfos(discovery);
  final archive = _openArchive(source);
  final parts = _authenticatedWorkbookParts(archive);
  final relationships = parseDocument(archive.read(parts.relationships));
  final sharedStringsPart = _internalRelationshipTargetForClass(
    relationships,
    parts.workbook,
    'sharedStrings',
  );
  final sharedStrings = sharedStringsPart == null
      ? <String>[]
      : _sharedStrings(archive.read(sharedStringsPart));
  final sheets = SplayTreeMap<String, WorksheetData>();
  final budget = MergeComparisonBudget.forValidation();
  for (final entry in infos.entries) {
    final raw = archive.read(entry.value.path);
    sheets[entry.key] = _worksheetData(raw, sharedStrings, budget);
  }
  return (
    discovery,
    WorkbookData(
      sheets: sheets,
      mergeValidationComparisons: budget.consumed,
      mergeValidationComplete: true,
    ),
  );
}

/// Port of read_workbook_from_reader / read_workbook.
WorkbookData readWorkbook(Uint8List source) {
  _validateSourceBytes(source);
  return readWorkbookAndDiscovery(source).$2;
}

/// Port of read_workbook_selective_from_reader (ASCII case-insensitive).
WorkbookData readWorkbookSelective(Uint8List source, List<String> wantedSheets) {
  _validateSourceBytes(source);
  final discovery = orderedWorkbookDiscovery(source);
  final infos = _projectSheetInfos(discovery);
  final archive = _openArchive(source);
  final parts = _authenticatedWorkbookParts(archive);
  final relationships = parseDocument(archive.read(parts.relationships));
  final sharedStringsPart = _internalRelationshipTargetForClass(
    relationships,
    parts.workbook,
    'sharedStrings',
  );
  final sharedStrings = sharedStringsPart == null
      ? <String>[]
      : _sharedStrings(archive.read(sharedStringsPart));
  final sheets = SplayTreeMap<String, WorksheetData>();
  final budget = MergeComparisonBudget.forValidation();
  for (final entry in infos.entries) {
    if (wantedSheets.any((wanted) => _equalsIgnoreAsciiCase(wanted, entry.key))) {
      final raw = archive.read(entry.value.path);
      sheets[entry.key] = _worksheetData(raw, sharedStrings, budget);
    }
  }
  return WorkbookData(
    sheets: sheets,
    mergeValidationComparisons: budget.consumed,
    mergeValidationComplete: true,
  );
}

/// Port of package_has_signature.
bool packageHasSignature(Uint8List source) {
  final archive = _openArchive(source);
  final relationships = parseDocument(archive.read('_rels/.rels'));
  final root = _validateRelationshipDocumentNamespace(relationships, 'package');
  String? signatureTarget;
  for (final relationship in _relationshipElements(root)) {
    final type = boundedUnqualifiedAttribute(
      relationship,
      'Type',
      _maxRelationshipTypeBytes,
      'package relationship type',
    );
    if (type == null) {
      throw const OoxmlError.invalid('package relationship type is missing');
    }
    if (type != _transitionalSignatureOrigin &&
        type != _strictSignatureOrigin) {
      continue;
    }
    if (signatureTarget != null) {
      throw const OoxmlError.invalid('package signature relationship is ambiguous');
    }
    final id = boundedUnqualifiedAttribute(
      relationship,
      'Id',
      _maxRelationshipIdBytes,
      'package signature relationship identity',
    );
    if (id == null) {
      throw const OoxmlError.invalid(
        'package signature relationship identity is missing',
      );
    }
    if (_relationshipTargetModeIsExternal(relationship, 'package signature')) {
      throw const OoxmlError.invalid(
        'external package signature relationship target is not allowed',
      );
    }
    final target = boundedUnqualifiedAttribute(
      relationship,
      'Target',
      _maxRelationshipTargetBytes,
      'package signature relationship target',
    );
    if (target == null) {
      throw const OoxmlError.invalid(
        'package signature relationship target is missing',
      );
    }
    final normalized = _normalizePackageRootTarget(target);
    if (!_isSignatureMember(normalized)) {
      throw const OoxmlError.invalid(
        'package signature relationship target is invalid',
      );
    }
    signatureTarget = normalized;
  }
  if (signatureTarget != null && !_hasEntry(archive, signatureTarget)) {
    throw const OoxmlError.invalid(
      'package signature relationship target is missing',
    );
  }
  var hasSignatureMember = false;
  for (final entry in archive.entries) {
    hasSignatureMember |= _isSignatureMember(entry.name);
  }
  return signatureTarget != null || hasSignatureMember;
}

bool _isSignatureMember(String name) {
  final first = name.split('/').first;
  return _equalsIgnoreAsciiCase(first, '_xmlsignatures');
}
