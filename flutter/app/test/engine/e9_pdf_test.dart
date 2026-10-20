// Port of the PDF-level tests in src-tauri/tests/signed_pages.rs
// (incremental rotation, first-page replacement, indirect /Length, object
// streams, and fail-closed inputs) and of pdf::extract_pages.
library;

import 'dart:io';
import 'dart:typed_data';

import 'package:docflow/engine/pdf.dart';
import 'package:flutter_test/flutter_test.dart';

import 'e9_pdf_fixture.dart';

void main() {
  late Directory fixture;

  setUp(() {
    fixture = Directory.systemTemp.createTempSync('e9_pdf');
  });

  tearDown(() {
    fixture.deleteSync(recursive: true);
  });

  String at(String name) => '${fixture.path}/$name';

  test('incremental page rotation preserves unrelated PDF content', () {
    final source = at('source.pdf');
    final output = at('review.pdf');
    writeRichArchive(source);
    final sourceBytes = File(source).readAsBytesSync();
    final before = inspect(source);

    rotatePagesIncremental(source, output, [(0, 90), (1, 270)]);

    final outputBytes = File(output).readAsBytesSync();
    final after = inspect(output);
    expect(_startsWith(outputBytes, sourceBytes), isTrue);
    expect(after.pageCount, before.pageCount);
    expect(after.incrementalRevisions, before.incrementalRevisions + 1);
    expect(after.pages[0].rotation, 90);
    expect(after.pages[1].rotation, 270);
    expect(after.pages[2], before.pages[2]);
    expect(after.pages[0].contentHash, before.pages[0].contentHash);
    expect(after.pages[1].annotations, before.pages[1].annotations);
    expect(after.catalogEntryHashes, before.catalogEntryHashes);
    expect(after.infoHash, before.infoHash);
    expect(after.embeddedFileHashes, before.embeddedFileHashes);
  });

  test('rotation rejects duplicate, out-of-range, and non-right-angle input', () {
    final source = at('source.pdf');
    writeRichArchive(source);
    expect(
      () => rotatePagesIncremental(source, at('a.pdf'), [(0, 90), (0, 180)]),
      throwsA(_message('approved page index is duplicated')),
    );
    expect(
      () => rotatePagesIncremental(source, at('b.pdf'), [(9, 90)]),
      throwsA(_message('approved page index is out of range')),
    );
    expect(
      () => rotatePagesIncremental(source, at('c.pdf'), [(0, 45)]),
      throwsA(
        _message('approved page rotation must be 0, 90, 180, or 270 degrees'),
      ),
    );
    expect(
      () => rotatePagesIncremental(source, at('d.pdf'), const []),
      throwsA(_message('no page rotations were approved')),
    );
    expect(File(at('a.pdf')).existsSync(), isFalse);
  });

  test(
    'incremental first-page replacement preserves the rich archive graph and prefix bytes',
    () {
      final archive = at('archive.pdf');
      final signed = at('signed.pdf');
      final output = at('output.pdf');
      writeRichArchive(archive);
      writeSigned(signed, extraPage: true);
      final archiveBytes = File(archive).readAsBytesSync();
      final archiveFacts = inspect(archive);
      final signedFacts = inspect(signed);
      for (final key in const [
        'Metadata',
        'Names',
        'Outlines',
        'PageLabels',
        'AcroForm',
        'OpenAction',
        'ViewerPreferences',
      ]) {
        expect(
          archiveFacts.catalogEntryHashes.containsKey(key),
          isTrue,
          reason: 'fixture must exercise catalog structure $key',
        );
      }
      expect(archiveFacts.infoHash, isNotNull);
      expect(archiveFacts.embeddedFileHashes.length, 1);

      replaceFirstPageIncremental(archive, signed, output);

      final outputBytes = File(output).readAsBytesSync();
      expect(_startsWith(outputBytes, archiveBytes), isTrue);
      final outputFacts = inspect(output);
      expect(outputFacts.pageCount, 3);
      expect(
        outputFacts.incrementalRevisions,
        archiveFacts.incrementalRevisions + 1,
      );
      expect(outputFacts.catalogEntryHashes, archiveFacts.catalogEntryHashes);
      expect(outputFacts.infoHash, archiveFacts.infoHash);
      expect(outputFacts.embeddedFileHashes, archiveFacts.embeddedFileHashes);
      expect(outputFacts.pages.sublist(1), archiveFacts.pages.sublist(1));
      expect(outputFacts.pages[0].mediaBox, signedFacts.pages[0].mediaBox);
      expect(outputFacts.pages[0].cropBox, signedFacts.pages[0].cropBox);
      expect(outputFacts.pages[0].rotation, signedFacts.pages[0].rotation);
      expect(outputFacts.pages[0].contentHash, signedFacts.pages[0].contentHash);
      expect(outputFacts.pages[0].links, 1);
      expect(outputFacts.pages[1].annotations, 3);
      expect(outputFacts.pages[1].links, 1);
      expect(outputFacts.pages[1].widgets, 1);
    },
  );

  test('an arbitrary signed-scan page can replace archive page one', () {
    final archive = at('archive.pdf');
    final signed = at('signed.pdf');
    final output = at('output.pdf');
    writeRichArchive(archive);
    writeSigned(signed, extraPage: true);
    final signedFacts = inspect(signed);

    replaceFirstPageIncrementalFromPage(archive, signed, 1, output);

    final completed = inspect(output);
    expect(completed.pages[0].contentHash, signedFacts.pages[1].contentHash);
    expect(completed.pages.sublist(1), inspect(archive).pages.sublist(1));
  });

  test('valid indirect stream length is inspected and staged without fidelity loss', () {
    final archive = at('archive.pdf');
    final signed = at('signed.pdf');
    final output = at('output.pdf');
    writeRichArchive(archive);
    writeSignedWithIndirectStreamLength(signed);

    final signedFacts = inspect(signed);
    expect(signedFacts.pageCount, 1);

    replaceFirstPageIncremental(archive, signed, output);
    final completed = inspect(output);
    expect(completed.pageCount, 3);
    expect(completed.pages[0].contentHash, signedFacts.pages[0].contentHash);
  });

  test('malformed and cyclic indirect stream lengths fail closed', () {
    final malformed = at('malformed-length.pdf');
    final cyclic = at('cyclic-length.pdf');
    writeSignedWithIndirectStreamLength(malformed);
    writeSignedWithIndirectStreamLength(cyclic);

    final malformedBytes = File(malformed).readAsBytesSync();
    final lengthObject =
        _indexOf(malformedBytes, '10 0 obj\n'.codeUnits) +
        '10 0 obj\n'.length;
    var lengthEnd = lengthObject;
    while (malformedBytes[lengthEnd] != 0x0A) {
      lengthEnd++;
    }
    expect(lengthEnd - lengthObject, 2, reason: 'fixture length is two digits');
    malformedBytes.setRange(lengthObject, lengthEnd, '[]'.codeUnits);
    File(malformed).writeAsBytesSync(malformedBytes);

    final cyclicBytes = File(cyclic).readAsBytesSync();
    final reference = _indexOf(cyclicBytes, '/Length 10 0 R'.codeUnits);
    cyclicBytes.setRange(
      reference,
      reference + '/Length 10 0 R'.length,
      '/Length 06 0 R'.codeUnits,
    );
    File(cyclic).writeAsBytesSync(cyclicBytes);

    expect(
      () => inspect(malformed),
      throwsA(_messageContains('not a non-negative integer')),
    );
    expect(
      () => inspect(cyclic),
      throwsA(_messageContains('cyclic or too deep')),
    );
  });

  test('object-stream archive remains object-stream based and keeps later page objects', () {
    final archive = at('archive-object-stream.pdf');
    final signed = at('signed.pdf');
    final output = at('output.pdf');
    writeObjectStreamPdf(
      archive,
      firstText: 'ARCHIVE fixture-code',
      secondText: 'OBJECT STREAM SECOND PAGE SENTINEL',
    );
    writeSigned(signed, extraPage: false);
    final before = inspect(archive);

    replaceFirstPageIncremental(archive, signed, output);

    final after = inspect(output);
    expect(before.objectStreamCount, 1);
    expect(after.objectStreamCount, 1);
    expect(after.pages[1], before.pages[1]);
    expect(
      _startsWith(File(output).readAsBytesSync(), File(archive).readAsBytesSync()),
      isTrue,
    );
  });

  test('signed, encrypted, tagged, and malformed PDFs fail closed without output', () {
    final plainArchive = at('plain.pdf');
    final plainSigned = at('signed.pdf');
    final encrypted = at('encrypted.pdf');
    final certified = at('certified.pdf');
    final tagged = at('tagged.pdf');
    final malformed = at('malformed.pdf');
    writeRichArchive(plainArchive);
    writeSigned(plainSigned, extraPage: false);
    writeEncrypted(encrypted);
    writeSignedDocument(certified);
    writeTaggedDocument(tagged);
    File(malformed).writeAsBytesSync('%PDF-1.7\nmalformed without xref'.codeUnits);

    final cases = <(String, String, String)>[
      (encrypted, plainSigned, 'encrypted archive'),
      (plainArchive, encrypted, 'encrypted scan'),
      (certified, plainSigned, 'signed archive'),
      (plainArchive, certified, 'signed scan'),
      (tagged, plainSigned, 'tagged archive'),
      (plainArchive, tagged, 'tagged scan'),
      (malformed, plainSigned, 'malformed archive'),
      (plainArchive, malformed, 'malformed scan'),
    ];
    for (final (archive, signed, label) in cases) {
      final output = at('${label.replaceAll(' ', '-')}.pdf');
      expect(
        () => replaceFirstPageIncremental(archive, signed, output),
        throwsA(
          isA<PdfError>().having((e) => e.reviewable, 'reviewable', isTrue),
        ),
        reason: label,
      );
      expect(File(output).existsSync(), isFalse, reason: label);
    }
  });

  test('replacement refuses an existing review destination without touching it', () {
    final archive = at('archive.pdf');
    final signed = at('signed.pdf');
    final output = at('output.pdf');
    writeRichArchive(archive);
    writeSigned(signed, extraPage: false);
    File(output).writeAsBytesSync('USER FILE'.codeUnits);

    expect(
      () => replaceFirstPageIncremental(archive, signed, output),
      throwsA(_messageContains('review destination already exists')),
    );
    expect(File(output).readAsBytesSync(), 'USER FILE'.codeUnits);
  });

  test('extract_pages writes only the approved pages and keeps their content', () {
    final source = at('source.pdf');
    final output = at('extract.pdf');
    writePlainTwoPage(source);
    final before = inspect(source);

    extractPages(source, [1, 0], output);

    final extracted = inspect(output);
    expect(extracted.pageCount, 2);
    expect(extracted.pages[0].contentHash, before.pages[1].contentHash);
    expect(extracted.pages[1].contentHash, before.pages[0].contentHash);
    expect(
      () => extractPages(source, [0, 0], at('dup.pdf')),
      throwsA(_message('approved PDF page indexes are invalid')),
    );
    expect(
      () => extractPages(source, [], at('empty.pdf')),
      throwsA(_message('no PDF pages were approved for extraction')),
    );
  });

  test('extract_pages refuses catalogs with structure it cannot separate', () {
    final rich = at('rich.pdf');
    writeRichArchive(rich);
    expect(
      () => extractPages(rich, [0], at('rich-out.pdf')),
      throwsA(_message('PDF contains features that cannot be safely separated')),
    );
    expect(File(at('rich-out.pdf')).existsSync(), isFalse);
  });

  test('inspect rejects a missing file and a non-PDF file', () {
    final text = at('text.pdf');
    File(text).writeAsStringSync('hello');
    expect(
      () => inspect(text),
      throwsA(_message('file is not a PDF')),
    );
    expect(
      () => inspect(at('missing.pdf')),
      throwsA(isA<PdfError>()),
    );
  });

  test('PDF inspection round-trips through its JSON projection', () {
    final source = at('source.pdf');
    writeRichArchive(source);
    final facts = inspect(source);
    final restored = PdfInspection.fromJson(facts.toJson());
    expect(restored, facts);
  });
}

bool _startsWith(Uint8List bytes, Uint8List prefix) {
  if (bytes.length < prefix.length) return false;
  for (var i = 0; i < prefix.length; i++) {
    if (bytes[i] != prefix[i]) return false;
  }
  return true;
}

int _indexOf(Uint8List haystack, List<int> needle) {
  for (var i = 0; i + needle.length <= haystack.length; i++) {
    var matches = true;
    for (var j = 0; j < needle.length; j++) {
      if (haystack[i + j] != needle[j]) {
        matches = false;
        break;
      }
    }
    if (matches) return i;
  }
  return -1;
}

Matcher _message(String expected) => isA<PdfError>().having(
  (error) => error.message,
  'message',
  expected,
);

Matcher _messageContains(String fragment) => isA<PdfError>().having(
  (error) => error.message,
  'message',
  contains(fragment),
);
