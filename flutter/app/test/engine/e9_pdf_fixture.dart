/// Port of src-tauri/tests/support/pdf_fixture.rs.
///
/// Writes the same classic and object-stream PDF fixtures the Rust tests use,
/// byte for byte, so the Dart and Rust suites exercise identical inputs.
library;

import 'dart:io';
import 'dart:typed_data';

const String fixtureCode = 'AZE-SUN-INT-QMS-RFI-CIV-0064';

Uint8List _b(String text) => Uint8List.fromList(text.codeUnits);

Uint8List _cat(List<List<int>> parts) {
  final out = BytesBuilder(copy: false);
  for (final part in parts) {
    out.add(part);
  }
  return out.takeBytes();
}

/// `ClassicPdf` from the Rust fixture.
class _ClassicPdf {
  _ClassicPdf({this.root = 0, this.info, this.trailerExtra = ''});

  final SortedObjects objects = SortedObjects();
  int root;
  int? info;
  String trailerExtra;

  void object(int number, List<int> body) {
    objects.put(number, Uint8List.fromList(body));
  }

  Uint8List finish() {
    final maxId = objects.keys.isEmpty ? 0 : objects.keys.last;
    final bytes = BytesBuilder(copy: false)
      ..add(const [0x25, 0x50, 0x44, 0x46, 0x2D, 0x31, 0x2E, 0x37, 0x0A])
      ..add(const [0x25, 0xE2, 0xE3, 0xCF, 0xD3, 0x0A]);
    final offsets = <int, int>{};
    for (final number in objects.keys) {
      offsets[number] = bytes.length;
      bytes.add(_b('$number 0 obj\n'));
      bytes.add(objects.get(number));
      bytes.add(_b('\nendobj\n'));
    }
    final xref = bytes.length;
    bytes.add(_b('xref\n0 ${maxId + 1}\n'));
    bytes.add(_b('0000000000 65535 f \n'));
    for (var number = 1; number <= maxId; number++) {
      final offset = offsets[number];
      if (offset != null) {
        bytes.add(_b('${offset.toString().padLeft(10, '0')} 00000 n \n'));
      } else {
        bytes.add(_b('0000000000 00000 f \n'));
      }
    }
    final infoPart = info == null ? '' : ' /Info $info 0 R';
    bytes.add(
      _b(
        'trailer\n<< /Size ${maxId + 1} /Root $root 0 R$infoPart$trailerExtra >>\n'
        'startxref\n$xref\n%%EOF\n',
      ),
    );
    return bytes.takeBytes();
  }
}

/// Object bodies keyed by number, iterated in ascending order like BTreeMap.
class SortedObjects {
  final Map<int, Uint8List> _values = {};

  void put(int number, Uint8List body) => _values[number] = body;

  Uint8List get(int number) => _values[number]!;

  List<int> get keys => _values.keys.toList()..sort();
}

String _literal(String value) => value
    .replaceAll(r'\', r'\\')
    .replaceAll('(', r'\(')
    .replaceAll(')', r'\)');

Uint8List _textStream(String text) {
  final content = 'BT /F1 16 Tf 36 700 Td (${_literal(text)}) Tj ET\n';
  return _b('<< /Length ${content.length} >>\nstream\n${content}endstream');
}

Uint8List _rawStream(String dictionary, List<int> data) => _cat([
  _b('<< $dictionary /Length ${data.length} >>\nstream\n'),
  data,
  _b('\nendstream'),
]);

void _ensureParent(String path) {
  Directory(File(path).parent.path).createSync(recursive: true);
}

void writeRichArchive(String path) {
  final pdf = _ClassicPdf(root: 1, info: 14);
  pdf.object(
    1,
    _b(
      '<< /Type /Catalog /Pages 2 0 R /Metadata 15 0 R /Names << /EmbeddedFiles 16 0 R >> /Outlines 18 0 R /PageLabels 20 0 R /AcroForm 22 0 R /OpenAction [3 0 R /Fit] /ViewerPreferences << /DisplayDocTitle true >> >>',
    ),
  );
  pdf.object(2, _b('<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 >>'));
  pdf.object(
    3,
    _b(
      '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources 6 0 R /Contents 7 0 R >>',
    ),
  );
  pdf.object(
    4,
    _b(
      '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /CropBox [12 18 580 760] /Rotate 90 /Resources 6 0 R /Contents 8 0 R /Annots [23 0 R 24 0 R 25 0 R] >>',
    ),
  );
  pdf.object(
    5,
    _b(
      '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources 6 0 R /Contents 9 0 R >>',
    ),
  );
  pdf.object(6, _b('<< /Font << /F1 10 0 R >> >>'));
  pdf.object(7, _textStream('ARCHIVE OLD FIRST PAGE $fixtureCode'));
  pdf.object(8, _textStream('ARCHIVE SECOND PAGE BYTE FIDELITY SENTINEL'));
  pdf.object(9, _textStream('ARCHIVE THIRD PAGE BYTE FIDELITY SENTINEL'));
  pdf.object(10, _b('<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>'));
  pdf.object(
    14,
    _b('<< /Title (Original RFI) /Author (AZFEN) /Custom (KEEP-METADATA) >>'),
  );
  pdf.object(
    15,
    _rawStream('/Type /Metadata /Subtype /XML', _b('<x:xmpmeta>KEEP-XMP</x:xmpmeta>')),
  );
  pdf.object(16, _b('<< /Names [(archive-note.txt) 17 0 R] >>'));
  pdf.object(
    17,
    _b(
      '<< /Type /Filespec /F (archive-note.txt) /EF << /F 21 0 R >> >>',
    ),
  );
  pdf.object(18, _b('<< /Type /Outlines /First 19 0 R /Last 27 0 R /Count 2 >>'));
  pdf.object(
    19,
    _b('<< /Title (RFI cover) /Parent 18 0 R /Next 27 0 R /Dest [3 0 R /Fit] >>'),
  );
  pdf.object(20, _b('<< /Nums [0 << /S /D /P (RFI-) /St 1 >>] >>'));
  pdf.object(
    21,
    _rawStream('/Type /EmbeddedFile', _b('keep this embedded archive data')),
  );
  pdf.object(
    22,
    _b('<< /Fields [25 0 R] /NeedAppearances false /DA (/F1 10 Tf 0 g) >>'),
  );
  pdf.object(
    23,
    _b(
      '<< /Type /Annot /Subtype /Text /Rect [40 40 60 60] /Contents (KEEP ANNOTATION) /P 4 0 R >>',
    ),
  );
  pdf.object(
    24,
    _b(
      '<< /Type /Annot /Subtype /Link /Rect [70 70 160 90] /A << /S /URI /URI (https://example.invalid/keep) >> /P 4 0 R >>',
    ),
  );
  pdf.object(
    25,
    _b(
      '<< /Type /Annot /Subtype /Widget /FT /Tx /T (KeepField) /V (keep-value) /Rect [100 100 240 125] /P 4 0 R /AP << /N 26 0 R >> >>',
    ),
  );
  pdf.object(
    26,
    _rawStream('/Type /XObject /Subtype /Form /BBox [0 0 140 25]', _b('q Q')),
  );
  pdf.object(
    27,
    _b('<< /Title (RFI continuation) /Parent 18 0 R /Prev 19 0 R /Dest [4 0 R /Fit] >>'),
  );
  _ensureParent(path);
  File(path).writeAsBytesSync(pdf.finish());
}

void writeSigned(String path, {required bool extraPage}) {
  final pdf = _ClassicPdf(root: 1);
  final kids = extraPage ? '[3 0 R 4 0 R]' : '[3 0 R]';
  final count = extraPage ? 2 : 1;
  pdf.object(1, _b('<< /Type /Catalog /Pages 2 0 R >>'));
  pdf.object(
    2,
    _b(
      '<< /Type /Pages /Kids $kids /Count $count /Resources 5 0 R /MediaBox [0 0 612 792] /Rotate 270 >>',
    ),
  );
  pdf.object(
    3,
    _b(
      '<< /Type /Page /Parent 2 0 R /CropBox [10 20 600 770] /Contents 6 0 R /Annots [8 0 R] >>',
    ),
  );
  if (extraPage) {
    pdf.object(4, _b('<< /Type /Page /Parent 2 0 R /Contents 7 0 R >>'));
  }
  pdf.object(5, _b('<< /Font << /F1 9 0 R >> >>'));
  pdf.object(6, _textStream('SIGNED FIRST PAGE $fixtureCode'));
  if (extraPage) {
    pdf.object(7, _textStream('SIGNED SECOND PAGE MUST BE IGNORED'));
  }
  pdf.object(
    8,
    _b(
      '<< /Type /Annot /Subtype /Link /Rect [30 30 180 50] /A << /S /URI /URI (https://example.invalid/signed) >> /P 3 0 R >>',
    ),
  );
  pdf.object(9, _b('<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>'));
  _ensureParent(path);
  File(path).writeAsBytesSync(pdf.finish());
}

void writeSignedWithIndirectStreamLength(String path) {
  final pdf = _ClassicPdf(root: 1);
  final content = 'BT /F1 16 Tf 36 700 Td (SIGNED FIRST PAGE $fixtureCode) Tj ET\n';
  pdf.object(1, _b('<< /Type /Catalog /Pages 2 0 R >>'));
  pdf.object(2, _b('<< /Type /Pages /Kids [3 0 R] /Count 1 >>'));
  pdf.object(
    3,
    _b(
      '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources 5 0 R /Contents 6 0 R >>',
    ),
  );
  pdf.object(5, _b('<< /Font << /F1 9 0 R >> >>'));
  pdf.object(6, _b('<< /Length 10 0 R >>\nstream\n${content}endstream'));
  pdf.object(9, _b('<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>'));
  pdf.object(10, _b('${content.length}'));
  _ensureParent(path);
  File(path).writeAsBytesSync(pdf.finish());
}

void writeEncrypted(String path) {
  final pdf = _ClassicPdf(
    root: 1,
    trailerExtra: ' /Encrypt 6 0 R /ID [<00112233><00112233>]',
  );
  pdf.object(1, _b('<< /Type /Catalog /Pages 2 0 R >>'));
  pdf.object(2, _b('<< /Type /Pages /Kids [3 0 R] /Count 1 >>'));
  pdf.object(
    3,
    _b('<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R >>'),
  );
  pdf.object(4, _textStream('ENCRYPTED $fixtureCode'));
  pdf.object(
    6,
    _b('<< /Filter /Standard /V 2 /R 3 /Length 128 /O <00> /U <00> /P -4 >>'),
  );
  _ensureParent(path);
  File(path).writeAsBytesSync(pdf.finish());
}

void writeSignedDocument(String path) {
  final pdf = _ClassicPdf(root: 1);
  pdf.object(
    1,
    _b(
      '<< /Type /Catalog /Pages 2 0 R /AcroForm 5 0 R /Perms << /DocMDP 7 0 R >> >>',
    ),
  );
  pdf.object(2, _b('<< /Type /Pages /Kids [3 0 R] /Count 1 >>'));
  pdf.object(
    3,
    _b(
      '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Annots [6 0 R] >>',
    ),
  );
  pdf.object(4, _textStream('SIGNED-DOCUMENT $fixtureCode'));
  pdf.object(5, _b('<< /Fields [6 0 R] /SigFlags 3 >>'));
  pdf.object(
    6,
    _b(
      '<< /Type /Annot /Subtype /Widget /FT /Sig /T (Signature1) /V 7 0 R /Rect [0 0 100 40] /P 3 0 R >>',
    ),
  );
  pdf.object(
    7,
    _b(
      '<< /Type /Sig /Filter /Adobe.PPKLite /SubFilter /adbe.pkcs7.detached /ByteRange [0 100 200 300] /Contents <00112233> >>',
    ),
  );
  _ensureParent(path);
  File(path).writeAsBytesSync(pdf.finish());
}

void writeTaggedDocument(String path) {
  final pdf = _ClassicPdf(root: 1);
  pdf.object(
    1,
    _b(
      '<< /Type /Catalog /Pages 2 0 R /MarkInfo << /Marked true >> /StructTreeRoot 5 0 R >>',
    ),
  );
  pdf.object(2, _b('<< /Type /Pages /Kids [3 0 R] /Count 1 >>'));
  pdf.object(
    3,
    _b(
      '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /StructParents 0 >>',
    ),
  );
  pdf.object(4, _textStream('TAGGED $fixtureCode'));
  pdf.object(
    5,
    _b('<< /Type /StructTreeRoot /K [6 0 R] /ParentTree << /Nums [0 [6 0 R]] >> >>'),
  );
  pdf.object(6, _b('<< /Type /StructElem /S /P /P 5 0 R /Pg 3 0 R /K 0 >>'));
  _ensureParent(path);
  File(path).writeAsBytesSync(pdf.finish());
}

void writeObjectStreamPdf(
  String path, {
  required String firstText,
  required String secondText,
}) {
  const header = '7 0 ';
  final pageTwo = _b(
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources 5 0 R /Contents 8 0 R /Rotate 90 >>',
  );
  final objectStreamData = _cat([_b(header), pageTwo]);

  final objects = SortedObjects()
    ..put(1, _b('<< /Type /Catalog /Pages 2 0 R >>'))
    ..put(2, _b('<< /Type /Pages /Kids [3 0 R 7 0 R] /Count 2 >>'))
    ..put(
      3,
      _b(
        '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources 5 0 R /Contents 4 0 R >>',
      ),
    )
    ..put(4, _textStream(firstText))
    ..put(5, _b('<< /Font << /F1 10 0 R >> >>'))
    ..put(
      6,
      _rawStream(
        '/Type /ObjStm /N 1 /First ${header.length}',
        objectStreamData,
      ),
    )
    ..put(8, _textStream(secondText))
    ..put(10, _b('<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>'));

  final bytes = BytesBuilder(copy: false)
    ..add(_b('%PDF-1.7\n%'))
    ..add(const [0xE2, 0xE3, 0xCF, 0xD3])
    ..add(_b('\n'));
  final offsets = <int, int>{};
  for (final number in objects.keys) {
    offsets[number] = bytes.length;
    bytes.add(_b('$number 0 obj\n'));
    bytes.add(objects.get(number));
    bytes.add(_b('\nendobj\n'));
  }
  const xrefId = 11;
  final xrefOffset = bytes.length;
  const size = 12;
  final entries = BytesBuilder(copy: false);
  void entry(int kind, int first, int second) {
    entries.addByte(kind);
    final data = ByteData(6)
      ..setUint32(0, first)
      ..setUint16(4, second);
    entries.add(data.buffer.asUint8List());
  }

  for (var number = 0; number < size; number++) {
    if (number == 0) {
      entry(0, 0, 65535);
    } else if (number == 7) {
      entry(2, 6, 0);
    } else if (number == xrefId) {
      entry(1, xrefOffset, 0);
    } else if (offsets.containsKey(number)) {
      entry(1, offsets[number]!, 0);
    } else {
      entry(0, 0, 0);
    }
  }
  final xrefBody = _rawStream(
    '/Type /XRef /Size $size /Root 1 0 R /W [1 4 2] /Index [0 $size]',
    entries.takeBytes(),
  );
  bytes.add(_b('$xrefId 0 obj\n'));
  bytes.add(xrefBody);
  bytes.add(_b('\nendobj\n'));
  bytes.add(_b('startxref\n$xrefOffset\n%%EOF\n'));
  _ensureParent(path);
  File(path).writeAsBytesSync(bytes.takeBytes());
}

/// A two-page PDF with no annotations, forms, or catalog extras. Used where
/// extraction must be accepted (the rich and signed fixtures are refused).
void writePlainTwoPage(String path) {
  final pdf = _ClassicPdf(root: 1);
  pdf.object(1, _b('<< /Type /Catalog /Pages 2 0 R >>'));
  pdf.object(
    2,
    _b('<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 /Resources 5 0 R /MediaBox [0 0 612 792] >>'),
  );
  pdf.object(3, _b('<< /Type /Page /Parent 2 0 R /Contents 6 0 R >>'));
  pdf.object(4, _b('<< /Type /Page /Parent 2 0 R /Contents 7 0 R >>'));
  pdf.object(5, _b('<< /Font << /F1 8 0 R >> >>'));
  pdf.object(6, _textStream('PLAIN FIRST PAGE'));
  pdf.object(7, _textStream('PLAIN SECOND PAGE'));
  pdf.object(8, _b('<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>'));
  _ensureParent(path);
  File(path).writeAsBytesSync(pdf.finish());
}
