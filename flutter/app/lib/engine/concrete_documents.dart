/// Port of src-tauri/src/concrete_documents.rs.
///
/// Generates one template-preserving OOXML package (CON-CIV-010/011/013/014)
/// per source record. Only XML text values in the template parts are
/// rewritten; every other member is copied byte-for-byte.
///
/// The Rust module edits XML with regular expressions over raw text. This
/// port keeps those same patterns and rewrite rules so the output matches.
library;

import 'dart:convert';
import 'dart:math' as math;
import 'dart:typed_data';

import 'package:xml/xml_events.dart';

import 'concrete_document_sources.dart' show ConcreteDocumentError;
import 'ooxml.dart';

const int _maxEntries = 4096;
const int _maxEntryBytes = 128 * 1024 * 1024;
const int _maxUncompressedBytes = 512 * 1024 * 1024;

final BigInt _mask64 = (BigInt.one << 64) - BigInt.one;

enum DocumentType { civ010, civ011, civ013, civ014 }

extension DocumentTypeExtension on DocumentType {
  String get expectedExtension => switch (this) {
    DocumentType.civ010 || DocumentType.civ013 => 'xlsx',
    DocumentType.civ011 || DocumentType.civ014 => 'docx',
  };
}

class ConcreteBatch {
  const ConcreteBatch({
    this.tag = '',
    this.concreteClass = '',
    this.supplier = '',
    this.batchTicket = '',
    this.truck = '',
    this.quantity = '',
    this.totalQuantity = '',
    this.batchTime = '',
    this.startTime = '',
    this.endTime = '',
    this.plantSlump = '',
    this.inSituSlump = '',
    this.plantTemp = '',
    this.inSituTemp = '',
    this.samples = '',
  });

  final String tag;
  final String concreteClass;
  final String supplier;
  final String batchTicket;
  final String truck;
  final String quantity;
  final String totalQuantity;
  final String batchTime;
  final String startTime;
  final String endTime;
  final String plantSlump;
  final String inSituSlump;
  final String plantTemp;
  final String inSituTemp;
  final String samples;
}

class SourceRecord {
  SourceRecord({
    required this.rfi,
    required this.location,
    this.tag = '',
    this.concreteClass = '',
    this.supplier = '',
    this.batchTicket = '',
    this.truck = '',
    this.quantity = '',
    this.totalQuantity = '',
    this.batchTime = '',
    this.pourDate = '',
    this.startTime = '',
    this.endTime = '',
    this.plantSlump = '',
    this.inSituSlump = '',
    this.plantTemp = '',
    this.inSituTemp = '',
    this.samples = '',
    this.waterCementRatio,
    this.ambientRange,
    this.drawing = '',
    List<ConcreteBatch>? additionalBatches,
  }) : additionalBatches = additionalBatches ?? <ConcreteBatch>[];

  String rfi;
  String location;
  String tag;
  String concreteClass;
  String supplier;
  String batchTicket;
  String truck;
  String quantity;
  String totalQuantity;
  String batchTime;
  String pourDate;
  String startTime;
  String endTime;
  String plantSlump;
  String inSituSlump;
  String plantTemp;
  String inSituTemp;
  String samples;
  String? waterCementRatio;
  (int, int)? ambientRange;
  String drawing;
  List<ConcreteBatch> additionalBatches;
}

class GeneratedFile {
  const GeneratedFile({required this.filename, required this.bytes});
  final String filename;
  final Uint8List bytes;
}

/// Port of `generate`: one output package per record.
List<GeneratedFile> generateConcreteDocuments({
  required DocumentType documentType,
  required Uint8List template,
  required List<SourceRecord> records,
  required (int, int) ambientRange,
  required int rngSeed,
}) {
  if (records.isEmpty) {
    throw const ConcreteDocumentError(
      'No concrete source records were provided',
    );
  }
  if (ambientRange.$1 > ambientRange.$2) {
    throw const ConcreteDocumentError('Ambient temperature range is invalid');
  }
  if (template.length > maxPackageBytes) {
    throw const ConcreteDocumentError(
      'Office template exceeds the supported package limit',
    );
  }
  final OfficePackage archive;
  try {
    archive = OfficePackage.open(template);
  } on OoxmlError catch (error) {
    throw ConcreteDocumentError('Invalid Office package: $error');
  }
  if (archive.length == 0 || archive.length > _maxEntries) {
    throw const ConcreteDocumentError('Office package entry count is invalid');
  }
  final parts = <(String, Uint8List)>[];
  var totalUncompressed = 0;
  var hasDocumentXml = false;
  for (var index = 0; index < archive.length; index++) {
    final name = archive.nameAt(index);
    final entrySize = archive.uncompressedSizeAt(index);
    totalUncompressed += entrySize;
    if (entrySize > _maxEntryBytes ||
        totalUncompressed > _maxUncompressedBytes) {
      throw const ConcreteDocumentError(
        'Office package expands beyond its supported limit',
      );
    }
    final Uint8List bytes;
    try {
      bytes = archive.readAt(index);
    } on OoxmlError catch (error) {
      throw ConcreteDocumentError(
        'Could not read Office package entry: $error',
      );
    }
    if (name == 'word/document.xml') {
      hasDocumentXml = true;
    }
    parts.add((name, bytes));
  }
  final validPackage = switch (documentType) {
    DocumentType.civ011 || DocumentType.civ014 => hasDocumentXml,
    DocumentType.civ010 || DocumentType.civ013 => parts.any(
      (part) =>
          part.$1.startsWith('xl/worksheets/') && part.$1.endsWith('.xml'),
    ),
  };
  if (!validPackage) {
    throw ConcreteDocumentError(
      'Template is not a valid .${documentType.expectedExtension} Office template',
    );
  }

  final sampleRfi = parts
      .where(
        (part) => switch (documentType) {
          DocumentType.civ011 ||
          DocumentType.civ014 => part.$1 == 'word/document.xml',
          DocumentType.civ010 ||
          DocumentType.civ013 => part.$1.endsWith('.xml'),
        },
      )
      .map((part) => _rfiAnchor(_decode(part.$2)))
      .firstWhere((value) => value != null, orElse: () => null);
  if (sampleRfi == null) {
    throw const ConcreteDocumentError(
      'Template does not contain a recognizable CIV RFI number',
    );
  }
  final sampleLocation = _locationAnchor(parts);
  if (sampleLocation == null) {
    throw const ConcreteDocumentError(
      'Template does not contain a location value adjacent to a location label',
    );
  }

  final rfiRegex = RegExp(r'CIV-\d{3,}', caseSensitive: false);
  var rng = BigInt.from(rngSeed) & _mask64;
  final generated = <GeneratedFile>[];
  for (final record in records) {
    if (record.rfi.trim().isEmpty || record.location.trim().isEmpty) {
      throw const ConcreteDocumentError(
        'A source record is missing its RFI number or location',
      );
    }
    final replacements = documentType == DocumentType.civ010
        ? <(String, String)>[]
        : <(String, String)>[
            (sampleRfi, record.rfi),
            (sampleLocation, record.location),
          ];
    if (documentType == DocumentType.civ011) {
      final range = record.ambientRange ?? ambientRange;
      rng = _xorshift64(rng);
      final width = BigInt.from(((range.$2 - range.$1) & 0xff) + 1);
      final ambient = (BigInt.from(range.$1) + rng % width).toString();
      final old = _ambientAnchor(parts);
      if (old == null) {
        throw const ConcreteDocumentError(
          'CON-CIV-011 template is missing its ambient-temperature value',
        );
      }
      final suffixStart = old.codeUnits.indexWhere(
        (unit) =>
            !_isAsciiDigit(unit) &&
            unit != 0x2e &&
            unit != 0x2c &&
            unit != 0x2d,
      );
      final suffix = suffixStart < 0 ? '' : old.substring(suffixStart);
      replacements.add((old, '$ambient$suffix'));
    }
    final civ010Ambients = <String>[];
    if (documentType == DocumentType.civ010) {
      final temperatures = [
        record.plantTemp,
        for (final batch in record.additionalBatches) batch.plantTemp,
      ];
      for (final temperature in temperatures) {
        rng = _xorshift64(rng);
        final plantTemperature = _rustF64Parse(
          temperature.trim().replaceAll(',', '.'),
        );
        if (plantTemperature == null) {
          throw ConcreteDocumentError(
            'CON-CIV-010 source has no numeric in-plant fresh-concrete temperature for ${record.rfi}',
          );
        }
        final offset = 5 + (rng % BigInt.from(3)).toInt();
        civ010Ambients.add(
          (plantTemperature - offset / 10.0).toStringAsFixed(1),
        );
      }
    }
    final changed = <String, Uint8List>{};
    for (final (name, original) in parts) {
      if (documentType == DocumentType.civ010) {
        continue;
      }
      if (name.endsWith('.xml')) {
        final xml = _decode(original);
        final replaced = _replaceXmlText(xml, replacements, rfiRegex);
        if (replaced != xml) {
          changed[name] = Uint8List.fromList(utf8.encode(replaced));
        }
      }
    }
    if (documentType == DocumentType.civ010) {
      for (final (name, original) in parts) {
        if (name == 'xl/worksheets/sheet1.xml') {
          final xml = _decode(original);
          changed[name] = Uint8List.fromList(
            utf8.encode(_patchCivSheet(xml, record, civ010Ambients)),
          );
        }
      }
    }
    generated.add(
      GeneratedFile(
        filename: _outputFilename(documentType, record),
        bytes: archive.write(changed),
      ),
    );
  }
  return generated;
}

/// Port of patch_civ010_numeric_cells applied to worksheet sheet1.
String _patchCivSheet(String xml, SourceRecord record, List<String> ambients) {
  final batches = <ConcreteBatch>[
    ConcreteBatch(
      tag: record.tag,
      concreteClass: record.concreteClass,
      supplier: record.supplier,
      batchTicket: record.batchTicket,
      truck: record.truck,
      quantity: record.quantity,
      totalQuantity: record.totalQuantity,
      batchTime: record.batchTime,
      startTime: record.startTime,
      endTime: record.endTime,
      plantSlump: record.plantSlump,
      inSituSlump: record.inSituSlump,
      plantTemp: record.plantTemp,
      inSituTemp: record.inSituTemp,
      samples: record.samples,
    ),
    ...record.additionalBatches,
  ];
  if (batches.length == 1) {
    batches.add(const ConcreteBatch());
  }
  var output = expandCivBatchRows(xml, batches.length);
  for (var index = 0; index < batches.length; index++) {
    final batch = batches[index];
    final row = 19 + index * 2;
    final ambient = index < ambients.length ? ambients[index] : '';
    for (final (column, value) in [
      ('AI', batch.quantity),
      ('AP', batch.totalQuantity),
      ('AW', batch.batchTime),
      ('BD', ambient),
      ('BK', batch.startTime),
      ('BR', batch.endTime),
      ('BY', batch.plantSlump),
      ('CC', batch.inSituSlump),
      ('CF', batch.plantTemp),
      ('CJ', batch.inSituTemp),
      ('CM', record.waterCementRatio ?? ''),
    ]) {
      final numeric = _timeToFraction(value) ?? value;
      output = _setNumericCell(output, '$column$row', numeric);
    }
    for (final (column, value) in [
      ('A', batch.tag),
      ('H', batch.concreteClass),
      ('O', batch.supplier),
      ('V', batch.batchTicket),
      ('AC', batch.truck),
      ('CS', batch.samples),
    ]) {
      output = _setTextCell(output, '$column$row', value);
    }
  }
  final headerUpdates = [
    ('A11', 'DRAWING NO: ${record.drawing}'),
    ('A12', 'REPORT (RFI) NUMBER: ${record.rfi}'),
    ('A13', 'LOCATION: ${record.location}'),
    ('BB14', 'CONCRETE POURING DATE:${record.pourDate}'),
  ];
  for (final (reference, value) in headerUpdates) {
    output = _setTextCell(output, reference, value);
  }
  return output;
}

/// `HH:MM` to a fraction of a day, or null when the value is not a time.
String? _timeToFraction(String value) {
  final colon = value.indexOf(':');
  if (colon < 0) return null;
  final hour = _parseU32(value.substring(0, colon));
  final minute = _parseU32(value.substring(colon + 1));
  if (hour == null || minute == null || hour >= 24 || minute >= 60) {
    return value;
  }
  return ((hour * 60 + minute) / 1440.0).toStringAsFixed(15);
}

/// Port of expand_civ010_batch_rows: grows the CON-CIV-010 batch table to
/// [batchCount] rows, shifting rows below it and cloning the second row.
String expandCivBatchRows(String xml, int batchCount) {
  if (batchCount <= 2) return xml;
  final rowPattern = _rowPattern();
  if (!rowPattern.hasMatch(xml)) {
    throw const ConcreteDocumentError(
      'CON-CIV-010 template has no second concrete-batch row to extend',
    );
  }
  final extraRows = (batchCount - 2) * 2;
  final shifted = _shiftCivRowReferences(xml, extraRows);
  final shiftedRowMatch = _rowPattern().firstMatch(shifted);
  if (shiftedRowMatch == null) {
    throw const ConcreteDocumentError(
      'CON-CIV-010 worksheet row structure is invalid',
    );
  }
  final shiftedRow = shiftedRowMatch.group(0)!;
  final clones = StringBuffer();
  for (var index = 2; index < batchCount; index++) {
    final targetRow = 19 + index * 2;
    clones.write(_cloneCivRow(shiftedRow, targetRow));
  }
  final replacement = '$shiftedRow$clones';
  return shifted.replaceRange(
    shiftedRowMatch.start,
    shiftedRowMatch.end,
    replacement,
  );
}

RegExp _rowPattern() =>
    RegExp(r'<row\b[^>]*\br="21"[^>]*>.*?</row>', dotAll: true);

final RegExp _referenceAttribute = RegExp(r'\b(r|ref|sqref)="([^"]+)"');
final RegExp _coordinate = RegExp(r'([A-Z]{1,3})([0-9]+)');

String _shiftCivRowReferences(String xml, int rows) {
  return xml.replaceAllMapped(_referenceAttribute, (match) {
    final prefix = '${match.group(1)}="';
    final value = match.group(2)!;
    final String shifted;
    if (prefix == 'r="') {
      final row = int.tryParse(value);
      if (row != null) {
        shifted = row >= 22 ? '${row + rows}' : value;
      } else {
        shifted = _shiftCoordinateRows(value, rows, 22);
      }
    } else {
      shifted = _shiftCoordinateRows(value, rows, 22);
    }
    return '$prefix$shifted"';
  });
}

String _cloneCivRow(String row, int targetRow) {
  return row.replaceAllMapped(_referenceAttribute, (match) {
    final prefix = '${match.group(1)}="';
    final value = match.group(2)!;
    final String cloned;
    if (prefix == 'r="' && value == '21') {
      cloned = '$targetRow';
    } else {
      cloned = _shiftCoordinateRows(value, math.max(targetRow - 21, 0), 21);
    }
    return '$prefix$cloned"';
  });
}

String _shiftCoordinateRows(String value, int delta, int threshold) {
  return value.replaceAllMapped(_coordinate, (match) {
    final row = int.tryParse(match.group(2)!);
    if (row == null) return match.group(0)!;
    if (row < threshold) return match.group(0)!;
    return '${match.group(1)}${row + delta}';
  });
}

RegExp _cellPattern(String reference) => RegExp(
  '<c\\b[^>]*\\br="${RegExp.escape(reference)}"[^>]*>.*?</c>',
  dotAll: true,
);

String _setNumericCell(String xml, String reference, String value) {
  final cell = _cellPattern(reference).firstMatch(xml);
  if (cell == null) return xml;
  var replacement = cell.group(0)!.replaceAll(' t="s"', '');
  final valueNode = RegExp(r'<v>.*?</v>', dotAll: true).firstMatch(replacement);
  if (valueNode != null) {
    replacement = replacement.replaceRange(
      valueNode.start,
      valueNode.end,
      value.isEmpty ? '' : '<v>$value</v>',
    );
  } else if (value.isNotEmpty) {
    final index = replacement.lastIndexOf('</c>');
    if (index < 0) {
      throw ConcreteDocumentError('Malformed workbook cell $reference');
    }
    replacement = replacement.replaceRange(index, index, '<v>$value</v>');
  }
  return xml.replaceRange(cell.start, cell.end, replacement);
}

String _setTextCell(String xml, String reference, String value) {
  final cell = _cellPattern(reference).firstMatch(xml);
  if (cell == null) {
    throw ConcreteDocumentError(
      'CON-CIV-010 template is missing cell $reference',
    );
  }
  final matched = cell.group(0)!;
  final openingEnd = matched.indexOf('>');
  if (openingEnd < 0) {
    throw ConcreteDocumentError('Malformed cell $reference');
  }
  var opening = matched
      .substring(0, openingEnd + 1)
      .replaceAll(RegExp(r'\s+t="[^"]*"'), '');
  opening = '${opening.substring(0, opening.length - 1)} t="inlineStr">';
  final body = value.isEmpty
      ? '<is><t></t></is>'
      : '<is><t xml:space="preserve">${_xmlEscape(value)}</t></is>';
  return xml.replaceRange(cell.start, cell.end, '$opening$body</c>');
}

final RegExp _rfiAnchorPattern = RegExp(
  r'AZE-SUN-INT-QMS-RFI-CIV-\d{3,}|CIV-\d{3,}',
  caseSensitive: false,
);

String? _rfiAnchor(String xml) =>
    _rfiAnchorPattern.firstMatch(_xmlTextNodes(xml).join())?.group(0);

String? _locationAnchor(List<(String, Uint8List)> parts) => _labeledAnchor(
  parts,
  const ['LOCATION/STRUCTURE', 'LOCATION', 'AREA / LOCATION'],
);

const List<String> _knownLabels = [
  'REPORT (RFI) NUMBER',
  'REPORT (RFI) NO.',
  'CONCRETE TYPE (CLASS)',
  'SUPPLIER',
  'BATCH TICKET NO.',
  'TRUCK NUMBER',
  'QUANTITY POURED',
  'TOTAL QUANTITY POURED',
  'CONCRETE BATCH TIME',
  'CONCRETE POURING DATE',
  'CONCRETE POURING START TIME',
  'CONCRETE POURING END TIME',
  'IN-PLANT SLUMP',
  'IN-SITU SLUMP',
  'IN-PLANT TEMP',
  'IN-SITU TEMP',
  'WATER/CEMENT RATIO',
  'CONCRETE SAMPLE NO.',
  'DRAWING NO.',
  'TAG NO.',
  'AMBIENT TEMP.',
  'AREA / LOCATION',
];

final RegExp _rfiLabelFilter = RegExp(r'^CIV-\d+$', caseSensitive: false);

String? _labeledAnchor(List<(String, Uint8List)> parts, List<String> labels) {
  for (final (name, bytes) in parts) {
    if (!name.endsWith('.xml')) continue;
    final texts = _xmlTextNodes(_decode(bytes));
    for (var index = 0; index < texts.length; index++) {
      final text = texts[index];
      final normalized = _normalize(text);
      if (labels.any((label) => normalized.contains(_normalize(label)))) {
        final inline = _inlineValueAfterLabel(text, labels);
        if (inline != null) return inline;
        for (final value in texts.skip(index + 1).take(5)) {
          final trimmed = value.trim();
          final normalizedValue = _normalize(value);
          if (trimmed.isNotEmpty &&
              !labels.any(
                (label) => normalizedValue.contains(_normalize(label)),
              ) &&
              !_knownLabels.any(
                (label) => normalizedValue == _normalize(label),
              ) &&
              !trimmed.startsWith('AZE-SUN-INT-QMS-RFI-') &&
              !_rfiLabelFilter.hasMatch(trimmed)) {
            return value;
          }
        }
      }
    }
  }
  return null;
}

String? _inlineValueAfterLabel(String text, List<String> labels) {
  final upper = text.toUpperCase();
  for (final label in labels) {
    final index = upper.indexOf(label.toUpperCase());
    if (index < 0) continue;
    final tailStart = math.min(index + label.length, text.length);
    final tail = text.substring(tailStart);
    final colon = tail.lastIndexOf(':');
    if (colon >= 0) {
      final value = tail.substring(colon + 1).trim();
      if (value.isNotEmpty) return value;
    }
  }
  return null;
}

final RegExp _trailingUnitSet = RegExp(r'[ Cc°]+$');

String? _ambientAnchor(List<(String, Uint8List)> parts) {
  for (final (name, bytes) in parts) {
    if (name != 'word/document.xml' && !name.startsWith('xl/worksheets/')) {
      continue;
    }
    final texts = _xmlTextNodes(_decode(bytes));
    for (var start = 0; start < texts.length; start++) {
      var label = '';
      final last = math.min(texts.length, start + 5);
      for (var end = start; end < last; end++) {
        label += _normalize(texts[end]);
        if (label.contains('AMBIENTTEMPERATURE') ||
            label.contains('AMBIENTTEMP')) {
          for (final value in texts.skip(end + 1).take(5)) {
            final number = value.trim().replaceAll(_trailingUnitSet, '');
            if (_rustF64Parse(number) != null) {
              return value.trim();
            }
          }
          break;
        }
      }
    }
  }
  return null;
}

String _normalize(String value) =>
    String.fromCharCodes(value.codeUnits.where(_isAsciiAlphanumeric))
        .toUpperCase();

bool _isAsciiDigit(int unit) => unit >= 0x30 && unit <= 0x39;

bool _isAsciiAlphanumeric(int unit) =>
    _isAsciiDigit(unit) ||
    (unit >= 0x41 && unit <= 0x5a) ||
    (unit >= 0x61 && unit <= 0x7a);

String _decode(Uint8List bytes) => utf8.decode(bytes, allowMalformed: true);

/// Port of xml_text_nodes: plain text runs and CDATA contents, in document
/// order. Text is split at general references (quick-xml 0.41 emits them as
/// separate events and this function ignores them). Malformed XML yields an
/// empty list.
List<String> _xmlTextNodes(String xml) {
  final out = <String>[];
  try {
    final events = parseEvents(
      xml,
      validateNesting: true,
      withBuffer: true,
      withLocation: true,
    );
    for (final event in events) {
      if (event is XmlTextEvent) {
        final raw = event.buffer!.substring(event.start!, event.stop!);
        for (final piece in quickXmlTextPieces(raw)) {
          if (!piece.isReference) out.add(piece.raw);
        }
      } else if (event is XmlCDATAEvent) {
        out.add(event.value);
      }
    }
  } catch (_) {
    return <String>[];
  }
  return out;
}

final RegExp _textNode = RegExp(
  r'<([A-Za-z_][\w.-]*:)?t\b[^>]*>.*?</([A-Za-z_][\w.-]*:)?t\s*>',
  dotAll: true,
);
final RegExp _tagPattern = RegExp(r'<[^>]*>', dotAll: true);

String _replaceXmlText(
  String xml,
  List<(String, String)> replacements,
  RegExp rfiRegex,
) {
  var output = xml;
  for (final (old, replacementValue) in replacements) {
    if (old.isEmpty) continue;
    var replaced = false;
    output = _replaceAllMatches(output, _textNode, (match) {
      if (replaced) return match.group(0)!;
      final node = match.group(0)!;
      final openEnd = node.indexOf('>') + 1;
      final closeStart = _lastIndexOfOr(node, '</', node.length);
      final content = node.substring(openEnd, closeStart);
      final decoded = xmlUnescape(content) ?? '';
      if (decoded == old || (_byteLength(old) > 8 && decoded.contains(old))) {
        replaced = true;
        final rewritten = decoded.replaceFirst(old, replacementValue);
        return '${node.substring(0, openEnd)}${_xmlEscape(rewritten)}'
            '${node.substring(closeStart)}';
      }
      return node;
    });
    if (!replaced && _byteLength(old) > 8) {
      output = _replaceSplitTextNodes(output, old, replacementValue);
    }
  }
  var replacedRfi = false;
  return _replaceAllMatches(output, _textNode, (match) {
    if (replacedRfi) return match.group(0)!;
    final node = match.group(0)!;
    final openEnd = node.indexOf('>') + 1;
    final closeStart = _lastIndexOfOr(node, '</', node.length);
    final decoded = xmlUnescape(node.substring(openEnd, closeStart)) ?? '';
    final found = rfiRegex.firstMatch(decoded);
    if (found == null) return node;
    replacedRfi = true;
    return '${node.substring(0, openEnd)}'
        '${_xmlEscape(decoded.substring(0, found.start))}'
        '${_xmlEscape(replacements.first.$2)}'
        '${_xmlEscape(decoded.substring(found.end))}'
        '${node.substring(closeStart)}';
  });
}

/// Port of replace_split_text_nodes: rewrites [old] when it spans several
/// adjacent text nodes of the same run.
String _replaceSplitTextNodes(String xml, String old, String replacement) {
  final nodes = _textNode.allMatches(xml).toList();
  if (nodes.length < 2) return xml;
  final edits = <(int, int, String)>[];
  var index = 0;
  while (index < nodes.length) {
    var end = index + 1;
    while (end < nodes.length) {
      final gap = xml.substring(nodes[end - 1].end, nodes[end].start);
      if (gap.contains('</w:p') ||
          gap.contains('</p') ||
          gap.contains('</w:tc') ||
          gap.contains('</tc') ||
          gap.replaceAll(_tagPattern, '').trim().isNotEmpty) {
        break;
      }
      end++;
    }
    if (end - index > 1) {
      final group = nodes.sublist(index, end);
      final values = group.map((match) {
        final raw = match.group(0)!;
        final openEnd = raw.indexOf('>') + 1;
        final closeStart = _lastIndexOfOr(raw, '</', raw.length);
        return xmlUnescape(raw.substring(openEnd, closeStart)) ?? '';
      }).toList();
      final joined = values.join();
      final start = joined.indexOf(old);
      if (start >= 0) {
        final finish = start + old.length;
        var offset = 0;
        var inserted = false;
        for (var nodeIndex = 0; nodeIndex < values.length; nodeIndex++) {
          final value = values[nodeIndex];
          final nodeEnd = offset + value.length;
          if (start < nodeEnd && finish > offset) {
            final left = math.max(start - offset, 0);
            final right = math.min(finish, nodeEnd) - offset;
            final text =
                value.substring(0, left) +
                (inserted ? '' : replacement) +
                value.substring(right);
            inserted = true;
            final match = group[nodeIndex];
            final raw = match.group(0)!;
            final openEnd = match.start + raw.indexOf('>') + 1;
            final closeStart =
                match.start + _lastIndexOfOr(raw, '</', raw.length);
            edits.add((openEnd, closeStart, _xmlEscape(text)));
          }
          offset = nodeEnd;
        }
      }
    }
    index = end;
  }
  var result = xml;
  for (final (start, end, text) in edits.reversed) {
    result = result.replaceRange(start, end, text);
  }
  return result;
}

String _replaceAllMatches(
  String input,
  RegExp pattern,
  String Function(Match match) replace,
) {
  final out = StringBuffer();
  var cursor = 0;
  for (final match in pattern.allMatches(input)) {
    out.write(input.substring(cursor, match.start));
    out.write(replace(match));
    cursor = match.end;
  }
  out.write(input.substring(cursor));
  return out.toString();
}

/// Port of the `xml_escape` helper in concrete_documents.rs: escapes only
/// `&`, `<` and `>`.
String _xmlEscape(String value) => value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;');

int _lastIndexOfOr(String haystack, String needle, int fallback) {
  final index = haystack.lastIndexOf(needle);
  return index < 0 ? fallback : index;
}

int _byteLength(String value) => utf8.encode(value).length;

/// Port of xorshift64 on u64 state (BigInt with 64-bit masking).
BigInt _xorshift64(BigInt state) {
  var value = state == BigInt.zero
      ? BigInt.parse('9E3779B97F4A7C15', radix: 16)
      : state;
  value ^= (value << 13) & _mask64;
  value ^= value >> 7;
  return value ^ ((value << 17) & _mask64);
}

/// Port of `u32` parsing with `str::parse::<u32>`: optional leading `+`,
/// ASCII digits only, range 0..=u32::MAX.
int? _parseU32(String value) {
  final digits = value.startsWith('+') ? value.substring(1) : value;
  if (digits.isEmpty || !digits.codeUnits.every(_isAsciiDigit)) return null;
  final parsed = BigInt.parse(digits);
  if (parsed > BigInt.from(0xFFFFFFFF)) return null;
  return parsed.toInt();
}

final RegExp _decimalF64 = RegExp(r'^(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$');

/// Port of `str::parse::<f64>` for the forms the workbook values use:
/// optional sign, decimal digits with optional fraction and exponent, and
/// the case-insensitive `inf`, `infinity` and `nan` words.
double? _rustF64Parse(String value) {
  var body = value;
  var negative = false;
  if (body.startsWith('+') || body.startsWith('-')) {
    negative = body.startsWith('-');
    body = body.substring(1);
  }
  final lower = body.toLowerCase();
  if (lower == 'inf' || lower == 'infinity') {
    return negative ? double.negativeInfinity : double.infinity;
  }
  if (lower == 'nan') return double.nan;
  if (!_decimalF64.hasMatch(body)) return null;
  final parsed = double.tryParse(body);
  if (parsed == null) return null;
  return negative ? -parsed : parsed;
}

String _outputFilename(DocumentType documentType, SourceRecord record) {
  final serial = record.rfi
      .split('-')
      .last
      .replaceAll(RegExp(r'[^A-Za-z0-9]'), '');
  final kind = switch (documentType) {
    DocumentType.civ010 => 'CON-CIV-010',
    DocumentType.civ011 => 'CON-CIV-011',
    DocumentType.civ013 => 'CON-CIV-013',
    DocumentType.civ014 => 'CON-CIV-014',
  };
  return '${kind}_CIV-$serial.${documentType.expectedExtension}';
}
