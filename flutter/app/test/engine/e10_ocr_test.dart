// Ports the inline unit tests of src-tauri/src/ocr.rs (consensus, cover
// detection, orientation parsing, image helpers) plus process-runner checks.
@TestOn('vm')
library;

import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:docflow/engine/ocr.dart';
import 'package:docflow/engine/operations/process_scans.dart';
import 'package:docflow/engine/rfi_code.dart';
import 'package:flutter_test/flutter_test.dart' hide find;
import 'package:path/path.dart' as p;

const String _civ1286 = 'AZE-SUN-INT-QMS-RFI-CIV-1286';

(int, RfiMatch) _obs(int dpi, String text) => (dpi, find(text));

void main() {
  group('RFI consensus', () {
    test('rejects one conflicting strict digit read', () {
      final observations = [
        _obs(300, 'AZE-SUN-INT-QMS-RFI-CIV-4286'),
        _obs(250, _civ1286),
        _obs(200, _civ1286),
        _obs(350, _civ1286),
      ];
      expect(selectRfiConsensus(observations, null), isNull);
    });

    test('requires three distinct render resolutions', () {
      final observations = [
        _obs(300, _civ1286),
        _obs(300, _civ1286),
        _obs(250, _civ1286),
      ];
      expect(selectRfiConsensus(observations, null), isNull);
    });

    test('requires two exact reads, not only repairs', () {
      final observations = [
        _obs(300, _civ1286),
        _obs(250, 'AZE-SUN-INT-QMS-RFI-CIV-I286'),
        _obs(200, 'AZE-SUN-INT-QMS-RFI-CIV-I286'),
      ];
      expect(selectRfiConsensus(observations, null), isNull);
    });

    test('accepts two exact fallback reads after consistent repairs', () {
      final observations = [
        _obs(300, 'AZE-SUN-INT-QMS-RFI-CIV-O648'),
        _obs(250, 'AZE-SUN-INT-QMS-RFI-CIV-O648'),
        _obs(200, 'AZE-SUN-INT-QMS-RFI-CIV-O648'),
        _obs(250, 'AZE-SUN-INT-QMS-RFI-CIV-0648'),
        _obs(300, 'AZE-SUN-INT-QMS-RFI-CIV-0648'),
      ];
      final result = selectRfiConsensus(observations, null)!;
      expect(result.$1.code, 'AZE-SUN-INT-QMS-RFI-CIV-0648');
      expect((result.$2, result.$3), (2, 3));
    });

    test('accepts one exact code across three resolutions', () {
      final observations = [
        _obs(300, _civ1286),
        _obs(250, _civ1286),
        _obs(200, _civ1286),
      ];
      final result = selectRfiConsensus(observations, null)!;
      expect(result.$1.code, _civ1286);
      expect(result.$1.confidence, 0.95);
      expect((result.$2, result.$3), (3, 3));
    });

    test('short and zero-padded reads are one identity; distinct codes conflict', () {
      final equivalent = [
        _obs(300, 'AZE-SUN-INT-QMS-RFI-CIV-9'),
        _obs(250, 'AZE-SUN-INT-QMS-RFI-CIV-0009'),
        _obs(200, 'AZE-SUN-INT-QMS-RFI-CIV-0009'),
      ];
      expect(
        selectRfiConsensus(equivalent, null)!.$1.code,
        'AZE-SUN-INT-QMS-RFI-CIV-0009',
      );
      final conflict = [
        _obs(300, 'AZE-SUN-INT-QMS-RFI-CIV-901'),
        _obs(250, 'AZE-SUN-INT-QMS-RFI-CIV-0902'),
        _obs(200, 'AZE-SUN-INT-QMS-RFI-CIV-0902'),
      ];
      expect(selectRfiConsensus(conflict, null), isNull);
    });

    test('malformed alphanumeric suffixes cannot become OCR identity', () {
      for (final suffix in ['901A', '901А', '09010']) {
        final matched = rfiMatches('AZE-SUN-INT-QMS-RFI-CIV-$suffix').first;
        final observations = [300, 250, 200].map((dpi) => (dpi, matched)).toList();
        expect(observations.every((o) => !o.$2.ok), isTrue, reason: suffix);
        expect(selectRfiConsensus(observations, null), isNull, reason: suffix);
      }
    });

    test('rejects a conflicting or multi-code searchable text layer', () {
      final observations = [
        _obs(300, _civ1286),
        _obs(250, _civ1286),
        _obs(200, _civ1286),
      ];
      expect(
        selectRfiConsensus(observations, {'AZE-SUN-INT-QMS-RFI-CIV-4286'}),
        isNull,
      );
      expect(
        selectRfiConsensus(observations, {
          _civ1286,
          'AZE-SUN-INT-QMS-RFI-CIV-4286',
        }),
        isNull,
      );
    });

    test('one OCR pass with multiple exact codes is ambiguous', () {
      final matches = rfiMatches(
        'AZE-SUN-INT-QMS-RFI-CIV-1286 AZE-SUN-INT-QMS-RFI-CIV-4286',
      );
      expect(matches, hasLength(2));
      expect(matches.every((m) => m.method == RfiMethod.strict), isTrue);
    });

    test('confirmed cover resolves one noisy digit without lowering consensus', () {
      final observations = [
        _obs(300, 'AZE-SUN-INT-QMS-RFI-CIV-1296'),
        _obs(250, 'AZE-SUN-INT-QMS-RFI-CIV-1296'),
        _obs(200, 'AZE-SUN-INT-QMS-RFI-CIV-1296'),
        _obs(350, 'AZE-SUN-INT-QMS-RFI-CIV-1296'),
        _obs(400, 'AZE-SUN-INT-QMS-RFI-CIV-1206'),
      ];
      final cover = confirmedCoverRead(
        'REQUEST FOR INSPECTION INTEC AZE-SUN-INT-QMS-RFI-CIV-1296',
        'test',
      )!;
      expect(selectRfiConsensus(observations, null), isNull);
      expect(
        coverRecoveryRead(cover, observations, null)!.matched.code,
        'AZE-SUN-INT-QMS-RFI-CIV-1296',
      );
    });

    test('confirmed cover does not override stable or searchable conflicts', () {
      final cover = confirmedCoverRead(
        'REQUEST FOR INSPECTION INTEC AZE-SUN-INT-QMS-RFI-CIV-1296',
        'test',
      )!;
      final stableConflict = [
        _obs(300, 'AZE-SUN-INT-QMS-RFI-CIV-1296'),
        _obs(250, 'AZE-SUN-INT-QMS-RFI-CIV-1296'),
        _obs(200, 'AZE-SUN-INT-QMS-RFI-CIV-1206'),
        _obs(350, 'AZE-SUN-INT-QMS-RFI-CIV-1206'),
      ];
      expect(coverRecoveryRead(cover, stableConflict, null), isNull);
      final agreeing = [
        _obs(300, 'AZE-SUN-INT-QMS-RFI-CIV-1296'),
        _obs(250, 'AZE-SUN-INT-QMS-RFI-CIV-1296'),
        _obs(200, 'AZE-SUN-INT-QMS-RFI-CIV-1296'),
      ];
      expect(
        coverRecoveryRead(cover, agreeing, {'AZE-SUN-INT-QMS-RFI-CIV-1206'}),
        isNull,
      );
    });

    test('unconfirmed cover identity cannot recover an unresolved page', () {
      final unresolved = OcrRead(
        matched: find(''),
        source: 'ocr:fast-cover:uncertain:test',
      );
      expect(coverRecoveryRead(unresolved, const [], null), isNull);
    });

    test('unconfirmed page source distinguishes empty from ambiguous', () {
      expect(
        unconfirmedPageSource([(150, find(''))], <String>{}),
        startsWith('ocr:no-candidate:'),
      );
      expect(
        unconfirmedPageSource([
          (150, find('AZE-SUN-INT-QMS-RFI-CIV-0064')),
          (200, find('AZE-SUN-INT-QMS-RFI-MEC-0064')),
        ], null),
        startsWith('ocr:ambiguous-or-unconfirmed:'),
      );
      expect(
        unconfirmedPageSource([(150, find(''))], {'AZE-SUN-INT-QMS-RFI-CIV-0064'}),
        startsWith('ocr:ambiguous-or-unconfirmed:'),
      );
    });

    test('diagnostics preserve text-layer evidence', () {
      expect(
        textLayerObservationSummaryForTest({
          'AZE-SUN-INT-QMS-RFI-CIV-1286',
          'AZE-SUN-INT-QMS-RFI-CIV-4286',
        }),
        'text-codes=AZE-SUN-INT-QMS-RFI-CIV-1286,AZE-SUN-INT-QMS-RFI-CIV-4286',
      );
    });
  });

  group('cover detection', () {
    test('requires heading, template mark and one strict identity', () {
      expect(
        confirmedCoverRead(
          'SUNRISE PROJECT\nREQUEST FOR INSPECTION (RFI)\nRFI No: AZE-SUN-INT-QMS-RFI-CIV-1348',
          'test',
        ),
        isNotNull,
      );
      expect(
        confirmedCoverRead(
          'INTEC ENERGY SOLUTIONS\nREQUEST FOR INSPECTION\nAZE-SUN-INT-QMS-RFI-CIV-1348',
          'test',
        ),
        isNotNull,
      );
      expect(
        confirmedCoverRead(
          'REQUEST FOR INSPECTION\nSUNRISE PROJECT\n'
          'AZE-SUN-INT-QMS-RFI-CIV-1348 AZE-SUN-INT-QMS-RFI-CIV-1349',
          'test',
        ),
        isNull,
      );
      expect(
        confirmedCoverRead('AZE-SUN-INT-QMS-ITP-CIV-0006 inspection stage', 'test'),
        isNull,
      );
      expect(
        confirmedCoverRead('REQUEST FOR INSPECTION only', 'test'),
        isNull,
      );
    });

    test('recognizes OCR CLV as CIV with alias evidence only on OCR paths', () {
      const text = 'SUNRISE PROJECT\nREQUEST FOR INSPECTION (RFI)\n'
          'RFI No: AZE-SUN-INT-QMS-RFI-CLV-1348';
      expect(exactCodes(text), isEmpty);
      final read = confirmedCoverRead(text, 'test')!;
      expect(read.matched.method, RfiMethod.ocrAlias);
      expect(read.matched.code, 'AZE-SUN-INT-QMS-RFI-CIV-1348');
      expect(read.source, contains('CLV->CIV'));
      expect(coverSignals(text, allowOcrAlias: false).strictIdentity, isFalse);

      final mixed = rfiMatches('AZE-SUN-INT-QMS-RFI-CLV-1348 AZE-SUN-INT-QMS-RFI-CIV-1348');
      expect(mixed, hasLength(1));
      expect(mixed.first.method, RfiMethod.strict);

      for (final unsupported in ['CLX', 'CIL', 'CVV', 'ARC']) {
        expect(
          confirmedCoverRead(
            'SUNRISE PROJECT\nREQUEST FOR INSPECTION\nAZE-SUN-INT-QMS-RFI-$unsupported-1348',
            'test',
          ),
          isNull,
          reason: unsupported,
        );
      }
      expect(
        confirmedCoverRead(
          'SUNRISE PROJECT\nREQUEST FOR INSPECTION\nAZE-SUN-INT-QMS-RFI-CLV-13481',
          'test',
        ),
        isNull,
      );
    });
  });

  group('orientation', () {
    test('parses tesseract OSD rotation and confidence', () {
      final result = parseOrientationOutput(
        'Page number: 0\nOrientation in degrees: 270\nRotate: 90\n'
        'Orientation confidence: 18.42\nScript: Latin\n',
        250,
      )!;
      expect(result.rotateClockwise, 90);
      expect(result.confidence, 18.42);
    });

    test('rejects missing or non-right-angle OSD results', () {
      expect(
        () => parseOrientationOutput('Rotate: 45\nOrientation confidence: 9\n', 250),
        throwsA(isA<OrientationError>()),
      );
      expect(
        () => parseOrientationOutput('Rotate: 90\n', 250),
        throwsA(isA<OrientationError>()),
      );
    });

    test('OSD selection requires strong or consistent confidence', () {
      const a = OrientationDetection(rotateClockwise: 90, confidence: 12, method: 'a');
      const b = OrientationDetection(rotateClockwise: 90, confidence: 4, method: 'b');
      expect(selectOsdDetection(a, null), a);
      expect(selectOsdDetection(null, null), isNull);
      expect(
        selectOsdDetection(
          const OrientationDetection(rotateClockwise: 0, confidence: 5, method: 'x'),
          const OrientationDetection(rotateClockwise: 90, confidence: 4, method: 'y'),
        ),
        isNull,
      );
      expect(selectOsdDetection(a, b)!.rotateClockwise, 90);
    });

    test('direction TSV parsing scores words and header codes', () {
      const tsv = 'level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n'
          '5\t1\t1\t1\t1\t1\t0\t10\t50\t20\t90\tAZE-SUN-INT-QMS-RFI-CIV-0064\n'
          '5\t1\t1\t1\t1\t2\t0\t900\t50\t20\t40\tnoise\n'
          '5\t1\t1\t1\t1\t3\t0\t900\t50\t20\t10\tlow\n';
      final score = parseDirectionScoreTsv(tsv, 0, 1000);
      expect(score.wordCount, 2);
      expect(score.confidenceSum, 130);
      expect(score.headerWordCount, 1);
      expect(score.headerRfiCodes.keys, ['AZE-SUN-INT-QMS-RFI-CIV-0064']);
    });

    test('direction selection needs a clear winner', () {
      final winner = DirectionScore(rotateClockwise: 90, wordCount: 30, confidenceSum: 2400);
      final runner = DirectionScore(rotateClockwise: 0, wordCount: 30, confidenceSum: 1000);
      expect(selectDirectionScore([winner, runner], 250)!.rotateClockwise, 90);
      final tie = DirectionScore(rotateClockwise: 0, wordCount: 30, confidenceSum: 2300);
      expect(selectDirectionScore([winner, tie], 250), isNull);
    });
  });

  group('image helpers', () {
    test('adaptive threshold crops the upper band and emits binary pixels', () {
      final image = GrayImage(
        width: 5,
        height: 10,
        pixels: Uint8List.fromList(List.generate(50, (i) => (i * 5) & 0xff)),
      );
      final thresholded = adaptiveThreshold(image, 0.35, 3, 10);
      expect((thresholded.width, thresholded.height), (5, 3));
      expect(thresholded.pixels, hasLength(15));
      expect(thresholded.pixels.every((v) => v == 0 || v == 255), isTrue);
    });

    test('thumbnail resize keeps nearest-neighbour samples', () {
      final image = GrayImage(
        width: 4,
        height: 2,
        pixels: Uint8List.fromList([0, 1, 2, 3, 4, 5, 6, 7]),
      );
      final resized = resizeGrayToFit(image, 2, 2);
      expect((resized.width, resized.height), (2, 1));
      expect(resized.pixels, [0, 2]);
    });

    test('bandHasTextCues needs 100 dark pixels', () {
      final dark = GrayImage(width: 10, height: 10, pixels: Uint8List(100));
      expect(bandHasTextCues(dark), isTrue);
      final blank = GrayImage(
        width: 10,
        height: 10,
        pixels: Uint8List(100)..fillRange(0, 100, 255),
      );
      expect(bandHasTextCues(blank), isFalse);
    });

    test('PNG encode and decode round-trip 8-bit grayscale', () {
      final image = GrayImage(
        width: 7,
        height: 3,
        pixels: Uint8List.fromList(List.generate(21, (i) => (i * 11) & 0xff)),
      );
      final decoded = decodePngGray(encodePngGray(image));
      expect(decoded.width, 7);
      expect(decoded.height, 3);
      expect(decoded.pixels, image.pixels);
    });

    test('PNG decoder rejects a bad signature', () {
      expect(
        () => decodePngGray(Uint8List.fromList(List.filled(32, 0))),
        throwsA(isA<OcrDocumentFailure>()),
      );
    });

    test('rotation is a pixel-exact right-angle transform', () {
      final image = GrayImage(
        width: 2,
        height: 1,
        pixels: Uint8List.fromList([10, 20]),
      );
      final rotated = rotateGrayClockwise(image, 90);
      expect((rotated.width, rotated.height), (1, 2));
      expect(rotated.pixels, [10, 20]);
      expect(
        () => rotateGrayClockwise(image, 45),
        throwsA(isA<OcrDocumentFailure>()),
      );
    });
  });

  group('engine selection and process runner', () {
    test('discovery with no engine reports the engine as unavailable', () {
      final ocr = NativeOcr.withTesseract(
        '/nonexistent/docflow/tesseract',
        renderer: const UnsupportedPdfPageRenderer(),
      );
      expect(ocr.supportStatus(), (false, false));
      expect(ocr.diagnostics(), 'tesseract: /nonexistent/docflow/tesseract');
    });

    test('unsupported renderer on Linux fails render with the Rust message', () async {
      final dir = Directory.systemTemp.createTempSync('e10-ocr-');
      addTearDown(() => dir.deleteSync(recursive: true));
      final tess = p.join(dir.path, 'tesseract');
      File(tess).writeAsStringSync('');
      final ocr = NativeOcr.withTesseract(
        tess,
        renderer: const UnsupportedPdfPageRenderer(),
      );
      await expectLater(
        ocr.readPage(p.join(dir.path, 'x.pdf'), 0),
        throwsA(
          isA<OcrDocumentFailure>().having(
            (e) => e.message,
            'message',
            'native PDF image rendering is unsupported on this platform',
          ),
        ),
      );
    });

    test('unverified macOS/Windows renderer fails closed', () async {
      const renderer = UnverifiedNativePdfPageRenderer('PDFKit');
      await expectLater(
        renderer.openSession('/x.pdf'),
        throwsA(isA<OcrDocumentFailure>()),
      );
    });

    test('missing executable is reported as EngineMissing', () async {
      final dir = Directory.systemTemp.createTempSync('e10-run-');
      addTearDown(() => dir.deleteSync(recursive: true));
      await expectLater(
        runTesseract('/nonexistent/docflow/tess', 'x.pgm', dir.path, null, 6, 0),
        throwsA(isA<OcrEngineMissing>()),
      );
    }, skip: Platform.isWindows ? 'POSIX process fixtures' : false);

    test('child process exit status and stdout are returned', () async {
      final dir = Directory.systemTemp.createTempSync('e10-run-');
      addTearDown(() => dir.deleteSync(recursive: true));
      final ok = p.join(dir.path, 'ok.sh');
      File(ok).writeAsStringSync('#!/bin/sh\necho AZE-SUN-INT-QMS-RFI-CIV-0064\n');
      Process.runSync('chmod', ['+x', ok]);
      final text = await runTesseract(ok, 'x.pgm', dir.path, null, 6, 1);
      expect(text.trim(), 'AZE-SUN-INT-QMS-RFI-CIV-0064');

      final bad = p.join(dir.path, 'bad.sh');
      File(bad).writeAsStringSync('#!/bin/sh\nexit 3\n');
      Process.runSync('chmod', ['+x', bad]);
      await expectLater(
        runTesseract(bad, 'x.pgm', dir.path, null, 6, 2),
        throwsA(
          isA<OcrDocumentFailure>().having(
            (e) => e.message,
            'message',
            'Tesseract exited with exit code 3',
          ),
        ),
      );
    }, skip: Platform.isWindows ? 'POSIX process fixtures' : false);

    test('image PDF export writes a one-page PDF header', () {
      final dir = Directory.systemTemp.createTempSync('e10-pdf-');
      addTearDown(() => dir.deleteSync(recursive: true));
      final png = p.join(dir.path, 'page.png');
      File(png).writeAsBytesSync(encodePngGray(GrayImage(
        width: 4,
        height: 4,
        pixels: Uint8List(16)..fillRange(0, 16, 200),
      )));
      final pdf = p.join(dir.path, 'page.pdf');
      imagePdfFromPng(png, pdf);
      expect(latin1.decode(File(pdf).readAsBytesSync().sublist(0, 8)), startsWith('%PDF-1.4'));
    });
  });
}

String textLayerObservationSummaryForTest(Set<String> codes) {
  final sorted = codes.toList()..sort();
  return 'text-codes=${sorted.join(',')}';
}
