/// Port of src-tauri/src/ocr.rs.
///
/// OCR engine seam: Tesseract (spawned as a child process, as in Rust) plus a
/// [PdfPageRenderer] for platform PDF page access. The Rust file also reaches
/// PDFKit (macOS) and the Windows PDF engine through FFI; those bridges are not
/// ported. [selectPdfPageRenderer] therefore fails closed on macOS and Windows
/// (UNVERIFIED) rather than silently returning no text.
///
/// Process authority: as in Rust, the engine is discovered only from bundled
/// locations next to the executable, Program Files on Windows, and PATH.
/// DOCFLOW_TESSERACT is never read. Every child is started with an argument
/// list (no shell), stdin closed, a 30 second timeout and a 1 MiB output cap.
library;

import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;
import 'dart:typed_data';

import 'package:archive/archive.dart' show ZLibDecoder, ZLibEncoder, getCrc32;
import 'package:path/path.dart' as p;

import 'operations/process_scans.dart';
import 'pdf_tool_renderer.dart';
import 'pdfkit_renderer.dart';
import 'windows_pdf_renderer.dart';
import 'rfi_code.dart';

const int textLayerMinChars = 50;
const int adaptiveC = 10;
const int fastCoverDpi = 150;
const int fastCoverRescueDpi = 300;
const int fastCoverTopPercent = 34;
const String _ocrAlphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-:.';
const String _clvSeparator = r'[\t ]*(?:\r?\n[\t ]*){0,2}-[\t ]*(?:\r?\n[\t ]*){0,2}';

class OcrAttempt {
  const OcrAttempt(this.dpi, this.band, this.block, this.psm);

  final int dpi;
  final double band;
  final int block;
  final int psm;
}

const List<OcrAttempt> ocrCascade = [
  OcrAttempt(300, 0.35, 31, 6),
  OcrAttempt(250, 0.35, 31, 6),
  OcrAttempt(200, 0.35, 31, 6),
  OcrAttempt(350, 0.35, 41, 6),
];
const List<int> rawFallbackDpi = [250, 300, 350, 400, 450];
const int rawFallbackPsm = 11;

// ---- Engine seam ----------------------------------------------------------

/// Engine selection seam: one OCR engine with its platform PDF access.
abstract interface class OcrEngine implements OcrAdapter, OrientationDetector {}

/// Port of the Rust `PdfOcrSession`: one open PDF decoder for a sequence of
/// page reads.
abstract interface class PdfPageSession {
  /// Searchable text of the page; empty when the platform exposes none.
  Future<String> pageText(int pageIndex);

  /// Renders the page to a PNG file at [dpi].
  Future<void> renderPagePng(String output, int dpi, int pageIndex);

  Future<void> close();
}

/// Platform PDF page access.
abstract interface class PdfPageRenderer {
  Future<PdfPageSession> openSession(String path);
}

/// Linux behaviour of the Rust module: no searchable text, no rendering.
class UnsupportedPdfPageRenderer implements PdfPageRenderer {
  const UnsupportedPdfPageRenderer();

  @override
  Future<PdfPageSession> openSession(String path) async => _UnsupportedSession();
}

class _UnsupportedSession implements PdfPageSession {
  @override
  Future<String> pageText(int pageIndex) async => '';

  @override
  Future<void> renderPagePng(String output, int dpi, int pageIndex) async {
    throw const OcrDocumentFailure(
      'native PDF image rendering is unsupported on this platform',
    );
  }

  @override
  Future<void> close() async {}
}

/// UNVERIFIED: the PDFKit (macOS) and Windows PDF engine bridges are not ported.
/// Opening fails explicitly so OCR never reports an empty document as readable.
class UnverifiedNativePdfPageRenderer implements PdfPageRenderer {
  const UnverifiedNativePdfPageRenderer(this.engine);

  final String engine;

  @override
  Future<PdfPageSession> openSession(String path) async {
    throw OcrDocumentFailure(
      '$engine page access is not ported to Dart yet (UNVERIFIED)',
    );
  }
}

/// Port of the `cfg(target_os)` selection of the PDF page backend.
PdfPageRenderer selectPdfPageRenderer() {
  if (Platform.isMacOS) {
    if (PdfKitPageRenderer.available) {
      return const PdfKitPageRenderer();
    }
    return PdfToolPageRenderer.discover() ??
        const UnverifiedNativePdfPageRenderer('PDFKit');
  }
  if (Platform.isWindows) {
    // UNVERIFIED: runs Windows.Data.Pdf through PowerShell; fails closed if absent.
    return const WindowsPdfPageRenderer();
  }
  return const UnsupportedPdfPageRenderer();
}

/// Port of `NativeOcr`: Tesseract plus the platform PDF renderer.
class NativeOcr implements OcrEngine {
  NativeOcr._(this.tesseract, this.tessdata, this.renderer);

  /// Port of `NativeOcr::with_tesseract`.
  factory NativeOcr.withTesseract(
    String path, {
    PdfPageRenderer? renderer,
  }) => NativeOcr._(path, findTessdata(path), renderer ?? selectPdfPageRenderer());

  /// Port of `NativeOcr::discover`. Searches bundled locations, Program Files
  /// on Windows, and PATH, in that order.
  factory NativeOcr.discover({PdfPageRenderer? renderer}) {
    final tesseract = tesseractCandidates().where(isBundledRegularFile).firstOrNull;
    return NativeOcr._(
      tesseract,
      tesseract == null ? null : findTessdata(tesseract),
      renderer ?? selectPdfPageRenderer(),
    );
  }

  final String? tesseract;
  final String? tessdata;
  final PdfPageRenderer renderer;

  /// Returns (engine available, language data available).
  (bool, bool) supportStatus() {
    final engine = tesseract != null && isBundledRegularFile(tesseract!);
    final language =
        engine &&
        tessdata != null &&
        isBundledRegularFile(p.join(tessdata!, 'eng.traineddata'));
    return (engine, language);
  }

  bool get _engineFileAvailable => tesseract != null && File(tesseract!).existsSync();

  @override
  Future<OcrRead> read(String path) => readPage(path, 0);

  @override
  PdfInspectionOutcome? inspectPdf(String path) => null;

  @override
  bool get supportsPageScan => true;

  @override
  bool get supportsPagePreviews => Platform.isMacOS || Platform.isWindows;

  @override
  bool get supportsParallelAnalysis => true;

  @override
  bool get supportsOrientation => true;

  @override
  Future<OrientationDetection> detectOrientation(String path, int pageIndex) =>
      detectPage(path, pageIndex);

  @override
  String diagnostics() =>
      'tesseract: ${tesseract ?? 'BULUNAMADI'}';

  @override
  String orientationDiagnostics() {
    final osd = tessdata != null &&
        isBundledRegularFile(p.join(tessdata!, 'osd.traineddata'));
    return 'tesseract=${tesseract ?? 'unavailable'}; osd=$osd';
  }

  @override
  Future<OcrRead> readCoverPage(String path, int pageIndex) =>
      readFastCoverPage(this, path, pageIndex);

  @override
  Future<OcrRead> readPage(String path, int pageIndex) async {
    final session = await renderer.openSession(path);
    try {
      final text = await session.pageText(pageIndex);
      final Set<String>? textLayer = text.trim().runes.length >= textLayerMinChars
          ? exactCodes(text)
          : null;
      if (!_engineFileAvailable) {
        throw const OcrEngineMissing();
      }
      final temporary = Directory.systemTemp.createTempSync('docflow-ocr-');
      try {
        final grayCache = <int, GrayImage>{};
        final observations = <(int, RfiMatch)>[];
        for (var index = 0; index < ocrCascade.length; index++) {
          final attempt = ocrCascade[index];
          if (!grayCache.containsKey(attempt.dpi)) {
            grayCache[attempt.dpi] = await _renderGray(
              session,
              path,
              temporary.path,
              attempt.dpi,
              pageIndex,
            );
          }
          final thresholded = adaptiveThreshold(
            grayCache[attempt.dpi]!,
            attempt.band,
            attempt.block,
            adaptiveC,
          );
          final image = p.join(temporary.path, 'attempt-$index.pgm');
          writePgm(image, thresholded);
          final text = await runTesseract(
            tesseract!,
            image,
            temporary.path,
            tessdata,
            attempt.psm,
            index,
            whitelist: _ocrAlphabet,
          );
          observations.addAll(rfiMatches(text).map((m) => (attempt.dpi, m)));
          if (index >= 2 &&
              selectRfiConsensus(observations, textLayer) != null) {
            break;
          }
        }
        final consensus = selectRfiConsensus(observations, textLayer);
        if (consensus != null) {
          return consensusRead(
            consensus.$1,
            consensus.$2,
            consensus.$3,
            textLayer,
            usedFallback: false,
          );
        }

        for (var index = 0; index < rawFallbackDpi.length; index++) {
          final dpi = rawFallbackDpi[index];
          if (!grayCache.containsKey(dpi)) {
            grayCache[dpi] = await _renderGray(
              session,
              path,
              temporary.path,
              dpi,
              pageIndex,
            );
          }
          final image = p.join(temporary.path, 'raw-$dpi.pgm');
          writePgm(image, grayCache[dpi]!);
          final text = await runTesseract(
            tesseract!,
            image,
            temporary.path,
            tessdata,
            rawFallbackPsm,
            ocrCascade.length + index,
          );
          observations.addAll(rfiMatches(text).map((m) => (dpi, m)));
        }

        final resolved = selectRfiConsensus(observations, textLayer);
        if (resolved != null) {
          return consensusRead(
            resolved.$1,
            resolved.$2,
            resolved.$3,
            textLayer,
            usedFallback: true,
          );
        }
        final unresolved = OcrRead(
          matched: find(''),
          source: unconfirmedPageSource(observations, textLayer),
        );
        OcrRead? cover;
        try {
          cover = await readFastCoverPage(this, path, pageIndex);
        } on Object {
          cover = null;
        }
        if (cover == null) {
          return unresolved;
        }
        return coverRecoveryRead(cover, observations, textLayer) ?? unresolved;
      } finally {
        temporary.deleteSync(recursive: true);
      }
    } finally {
      await session.close();
    }
  }

  Future<GrayImage> _renderGray(
    PdfPageSession session,
    String path,
    String temporary,
    int dpi,
    int pageIndex,
  ) async {
    final rendered = p.join(temporary, 'page-$pageIndex-$dpi.png');
    await session.renderPagePng(rendered, dpi, pageIndex);
    return readPngGray(rendered);
  }

  @override
  Future<OrientationDetection> detectPage(String path, int pageIndex) async {
    try {
      return await _detectPageUnchecked(path, pageIndex);
    } on OcrDocumentFailure catch (error) {
      throw OrientationError(error.message);
    }
  }

  Future<OrientationDetection> _detectPageUnchecked(
    String path,
    int pageIndex,
  ) async {
    const primaryDpi = 250;
    const secondaryDpi = 350;
    final tess = tesseract;
    if (tess == null || !File(tess).existsSync()) {
      throw const OrientationError('Tesseract is unavailable');
    }
    final data = tessdata;
    if (data == null ||
        !isBundledRegularFile(p.join(data, 'osd.traineddata'))) {
      throw const OrientationError(
        'Tesseract orientation data (osd.traineddata) is unavailable',
      );
    }
    final temporary = Directory.systemTemp.createTempSync('docflow-orient-');
    try {
      final primary = p.join(temporary.path, 'page-$pageIndex-$primaryDpi.png');
      await _renderOnce(path, primary, primaryDpi, pageIndex);
      final primaryDetection = await _tryOsd(tess, primary, temporary.path, data, primaryDpi);
      final single = selectOsdDetection(primaryDetection, null);
      if (single != null) {
        return single;
      }

      final secondary = p.join(temporary.path, 'page-$pageIndex-$secondaryDpi.png');
      final secondaryDetection = await _tryOsdAfterRender(
        path,
        secondary,
        secondaryDpi,
        pageIndex,
        tess,
        temporary.path,
        data,
      );
      final pair = selectOsdDetection(primaryDetection, secondaryDetection);
      if (pair != null) {
        return pair;
      }

      final first = await detectOrientationByDirectionScores(
        tess,
        primary,
        temporary.path,
        data,
        primaryDpi,
      );
      if (first.$1 != null) {
        return first.$1!;
      }
      if (!File(secondary).existsSync()) {
        await _renderOnce(path, secondary, secondaryDpi, pageIndex);
      }
      final second = await detectOrientationByDirectionScores(
        tess,
        secondary,
        temporary.path,
        data,
        secondaryDpi,
      );
      OrientationDetection? header;
      var headerConflict = false;
      try {
        header = selectCorrelatedHeaderScoreChecked(first.$2, second.$2);
      } on _CorrelationConflict {
        headerConflict = true;
      }
      final secondConflict = header != null &&
          second.$1 != null &&
          second.$1!.rotateClockwise != header.rotateClockwise;
      OrientationDetection? detection;
      if (!headerConflict && !secondConflict) {
        detection = second.$1 ?? header ??
            selectCorrelatedDirectionScore(
              first.$2,
              second.$2,
              primaryDetection,
              secondaryDetection,
            );
      }
      if (detection == null) {
        throw OrientationError(
          'orientation uncertain after 250/350-DPI OSD and four-direction OCR; '
          'OSD 250dpi ${osdSummary(primaryDetection)}, 350dpi ${osdSummary(secondaryDetection)}; '
          'OCR 250dpi [${directionSummary(first.$2)}], 350dpi [${directionSummary(second.$2)}]',
        );
      }
      return detection;
    } finally {
      temporary.deleteSync(recursive: true);
    }
  }

  Future<void> _renderOnce(String path, String output, int dpi, int pageIndex) async {
    final session = await renderer.openSession(path);
    try {
      await session.renderPagePng(output, dpi, pageIndex);
    } finally {
      await session.close();
    }
  }

  Future<OrientationDetection?> _tryOsdAfterRender(
    String path,
    String output,
    int dpi,
    int pageIndex,
    String tess,
    String temporary,
    String data,
  ) async {
    try {
      await _renderOnce(path, output, dpi, pageIndex);
    } on Object {
      return null;
    }
    return _tryOsd(tess, output, temporary, data, dpi);
  }

  Future<OrientationDetection?> _tryOsd(
    String tess,
    String image,
    String temporary,
    String data,
    int dpi,
  ) async {
    try {
      final text = await runOrientationTesseract(tess, image, temporary, data, dpi);
      return parseOrientationOutput(text, dpi);
    } on Object {
      return null;
    }
  }
}

/// Port of `read_fast_cover_page`.
Future<OcrRead> readFastCoverPage(NativeOcr ocr, String source, int pageIndex) async {
  final session = await ocr.renderer.openSession(source);
  try {
    final textLayer = await session.pageText(pageIndex);
    var possibleCover = coverSignals(textLayer, allowOcrAlias: false).requestHeading;
    if (!ocr._engineFileAvailable) {
      throw const OcrEngineMissing();
    }
    final tess = ocr.tesseract!;
    final tessdata = ocr.tessdata;
    final temporary = Directory.systemTemp.createTempSync('docflow-cover-');
    try {
      final rendered = p.join(temporary.path, 'cover-$pageIndex-$fastCoverDpi.png');
      await session.renderPagePng(rendered, fastCoverDpi, pageIndex);
      final image = readPngGray(rendered);
      var rescueUsed = false;
      var attempt = 0;

      for (final direction in [0, 90, 180, 270]) {
        final oriented = rotateGrayClockwise(image, direction);
        var text = await _readCoverBandText(
          tess,
          tessdata,
          oriented,
          pageIndex,
          direction,
          attempt,
          temporary.path,
        );
        attempt += 1;
        final firstRead = confirmedCoverRead(text, '150dpi');
        if (firstRead != null) {
          return firstRead;
        }
        var signals = coverSignals(text, allowOcrAlias: true);
        possibleCover = possibleCover || signals.requestHeading;
        final promising = [
              signals.requestHeading,
              signals.templateMark,
              signals.identityHint,
            ].where((flag) => flag).length >=
            2;
        if (promising) {
          for (final psm in [6, 12]) {
            final band = p.join(temporary.path, 'cover-$pageIndex-$direction.pgm');
            final retry = await runTesseract(
              tess,
              band,
              temporary.path,
              tessdata,
              psm,
              attempt,
              whitelist: null,
            );
            attempt += 1;
            text = '$text\n$retry';
            final retryRead = confirmedCoverRead(text, '150dpi-retry');
            if (retryRead != null) {
              return retryRead;
            }
            signals = coverSignals(text, allowOcrAlias: true);
            possibleCover = possibleCover || signals.requestHeading;
          }
        }

        if (!rescueUsed && signals.requestHeading && signals.templateMark) {
          rescueUsed = true;
          final rescue = p.join(temporary.path, 'cover-$pageIndex-$fastCoverRescueDpi.png');
          await session.renderPagePng(rescue, fastCoverRescueDpi, pageIndex);
          final high = readPngGray(rescue);
          final oriented2 = rotateGrayClockwise(high, direction);
          final rescueText = await _readCoverBandText(
            tess,
            tessdata,
            oriented2,
            pageIndex,
            direction + 360,
            attempt,
            temporary.path,
          );
          attempt += 1;
          final rescueRead = confirmedCoverRead(rescueText, '300dpi-rescue');
          if (rescueRead != null) {
            return rescueRead;
          }
          final rescueSignals = coverSignals(rescueText, allowOcrAlias: true);
          if (rescueSignals.requestHeading && rescueSignals.templateMark) {
            return uncertainCoverRead('300dpi-rescue');
          }
        }
      }
      if (possibleCover) {
        return uncertainCoverRead('partial-visible-cover-cues');
      }
      return OcrRead(matched: find(''), source: 'ocr:fast-cover:no-cover-signal');
    } finally {
      temporary.deleteSync(recursive: true);
    }
  } finally {
    await session.close();
  }
}

Future<String> _readCoverBandText(
  String tess,
  String? tessdata,
  GrayImage image,
  int pageIndex,
  int direction,
  int attempt,
  String temporary,
) async {
  final band = coverTopBand(image);
  if (!bandHasTextCues(band)) {
    return '';
  }
  final imagePath = p.join(temporary, 'cover-$pageIndex-$direction.pgm');
  writePgm(imagePath, band);
  return runTesseract(tess, imagePath, temporary, tessdata, 11, attempt, whitelist: null);
}

// ---- RFI signals and consensus --------------------------------------------

final RegExp _clvAlias = RegExp(
  'AZE$_clvSeparator'
  'SUN$_clvSeparator'
  'INT$_clvSeparator'
  'QMS$_clvSeparator'
  'RFI$_clvSeparator'
  'CLV$_clvSeparator'
  '([0-9]{1,4})',
);

final RegExp _alnum = RegExp(r'[\p{Alphabetic}\p{N}]', unicode: true);
final RegExp _whitespaceRun = RegExp(r'\p{White_Space}+', unicode: true);

bool _isAlnumChar(String ch) => _alnum.hasMatch(ch);

List<RfiMatch> ocrAliasMatches(String text) {
  final normalized = canonical(text);
  final matches = <RfiMatch>[];
  for (final m in _clvAlias.allMatches(normalized)) {
    if (m.start > 0 && _isAlnumChar(normalized[m.start - 1])) {
      continue;
    }
    if (m.end < normalized.length && _isAlnumChar(normalized[m.end])) {
      continue;
    }
    final base = find('$prefix-CIV-${m.group(1)}');
    matches.add(RfiMatch(
      code: base.code,
      discipline: base.discipline,
      number: base.number,
      method: RfiMethod.ocrAlias,
      confidence: base.confidence,
    ));
  }
  return matches;
}

String _asciiUpper(String value) => String.fromCharCodes(
  value.codeUnits.map((u) => u >= 0x61 && u <= 0x7a ? u - 32 : u),
);

class CoverSignals {
  const CoverSignals({
    required this.requestHeading,
    required this.templateMark,
    required this.strictIdentity,
    required this.identityHint,
  });

  final bool requestHeading;
  final bool templateMark;
  final bool strictIdentity;
  final bool identityHint;
}

CoverSignals coverSignals(String text, {required bool allowOcrAlias}) {
  final normalized = _asciiUpper(
    text.split(_whitespaceRun).where((s) => s.isNotEmpty).join(' '),
  );
  final requestHeading = normalized.contains('REQUEST FOR INSPECTION');
  const marks = ['SUNRISE PROJECT', 'INTEC', 'LIGHTSOURCEBP', 'LIGHTSOURCE BP'];
  final templateMark = marks.any(normalized.contains);
  final aliases = allowOcrAlias ? ocrAliasMatches(normalized) : <RfiMatch>[];
  final codes = exactCodes(normalized)..addAll(aliases.map((a) => a.code));
  final matched = find(normalized);
  final strictIdentity = codes.length == 1 &&
      ((matched.method == RfiMethod.strict && matched.confidence >= 0.8) ||
          aliases.any((alias) => codes.contains(alias.code)));
  final identityHint = codes.isNotEmpty || matched.ok;
  return CoverSignals(
    requestHeading: requestHeading,
    templateMark: templateMark,
    strictIdentity: strictIdentity,
    identityHint: identityHint,
  );
}

OcrRead? confirmedCoverRead(String text, String source) {
  final signals = coverSignals(text, allowOcrAlias: true);
  if (!(signals.requestHeading && signals.templateMark && signals.strictIdentity)) {
    return null;
  }
  final aliases = ocrAliasMatches(text);
  final matched = aliases.isNotEmpty ? aliases.first : find(text);
  final aliasEvidence = matched.method == RfiMethod.ocrAlias ? ':alias:CLV->CIV' : '';
  return OcrRead(
    matched: matched,
    source: 'ocr:fast-cover:confirmed:$source$aliasEvidence',
  );
}

OcrRead uncertainCoverRead(String source) =>
    OcrRead(matched: find(''), source: 'ocr:fast-cover:uncertain:$source');

List<RfiMatch> rfiMatches(String text) {
  final matches = [for (final code in exactCodes(text)) find(code)];
  for (final alias in ocrAliasMatches(text)) {
    if (!matches.any((m) => m.code == alias.code)) {
      matches.add(alias);
    }
  }
  if (matches.isEmpty) {
    matches.add(find(text));
  }
  return matches;
}

String _textLayerEvidence(Set<String>? textLayer, String code) {
  if (textLayer == null) {
    return 'no-text-layer';
  }
  if (textLayer.length == 1 && textLayer.contains(code)) {
    return 'text-agrees';
  }
  if (textLayer.isNotEmpty) {
    return 'text-conflicts';
  }
  return 'text-no-exact-code';
}

String _textLayerObservationSummary(Set<String>? textLayer) {
  if (textLayer == null) {
    return 'no-text-layer';
  }
  if (textLayer.isEmpty) {
    return 'text-no-exact-code';
  }
  final sorted = textLayer.toList()..sort();
  return 'text-codes=${sorted.join(',')}';
}

String _rfiObservationSummary(List<(int, RfiMatch)> observations) {
  final summary = <String, (int, Set<int>)>{};
  for (final (dpi, matched) in observations.where((o) => o.$2.ok)) {
    final entry = summary.putIfAbsent(matched.code, () => (0, <int>{}));
    summary[matched.code] = (entry.$1 + 1, entry.$2..add(dpi));
  }
  if (summary.isEmpty) {
    return 'no-candidate';
  }
  final codes = summary.keys.toList()..sort();
  return codes
      .map((code) => '$code=${summary[code]!.$1}votes/${summary[code]!.$2.length}dpi')
      .join(',');
}

String unconfirmedPageSource(List<(int, RfiMatch)> observations, Set<String>? textLayer) {
  final hasCandidate = observations.any((o) => o.$2.ok) ||
      (textLayer != null && textLayer.isNotEmpty);
  return 'ocr:${hasCandidate ? 'ambiguous-or-unconfirmed' : 'no-candidate'}:'
      '${_rfiObservationSummary(observations)}:'
      '${_textLayerObservationSummary(textLayer)}';
}

OcrRead consensusRead(
  RfiMatch matched,
  int votes,
  int dpiCount,
  Set<String>? textLayer, {
  required bool usedFallback,
}) {
  final fallback = usedFallback ? '-fallback' : '';
  final alias = matched.method == RfiMethod.ocrAlias ? ':alias:CLV->CIV' : '';
  return OcrRead(
    matched: matched,
    source: 'ocr:consensus$fallback:$votes'
        'votes:$dpiCount'
        'dpi$alias:${_textLayerEvidence(textLayer, matched.code)}',
  );
}

/// Port of `select_rfi_consensus`. Returns (match, votes, distinct dpis).
(RfiMatch, int, int)? selectRfiConsensus(
  List<(int, RfiMatch)> observations,
  Set<String>? textLayer,
) {
  const minIdentityVotes = 2;
  const minDistinctDpi = 3;
  const consensusConfidence = 0.95;

  final observed = {
    for (final o in observations)
      if (o.$2.ok) o.$2.code,
  };
  if (observed.length != 1) {
    return null;
  }
  final code = observed.first;
  if (textLayer != null &&
      textLayer.isNotEmpty &&
      (textLayer.length != 1 || !textLayer.contains(code))) {
    return null;
  }
  final identity = observations
      .where((o) =>
          (o.$2.method == RfiMethod.strict || o.$2.method == RfiMethod.ocrAlias) &&
          o.$2.code == code)
      .map((o) => o.$2)
      .toList();
  if (identity.isEmpty) {
    return null;
  }
  final identityVotes = identity.length;
  var matched = identity.first;
  if (observations.any((o) => o.$2.code == code && o.$2.method == RfiMethod.ocrAlias)) {
    matched = RfiMatch(
      code: matched.code,
      discipline: matched.discipline,
      number: matched.number,
      method: RfiMethod.ocrAlias,
      confidence: matched.confidence,
    );
  }
  final dpis = {
    for (final o in observations)
      if (o.$2.ok && o.$2.code == code) o.$1,
  };
  if (identityVotes < minIdentityVotes || dpis.length < minDistinctDpi) {
    return null;
  }
  return (
    RfiMatch(
      code: matched.code,
      discipline: matched.discipline,
      number: matched.number,
      method: matched.method,
      confidence: consensusConfidence,
    ),
    identityVotes,
    dpis.length,
  );
}

/// Port of `cover_recovery_read`.
OcrRead? coverRecoveryRead(
  OcrRead cover,
  List<(int, RfiMatch)> observations,
  Set<String>? textLayer,
) {
  final matched = cover.matched;
  if (!(matched.method == RfiMethod.strict || matched.method == RfiMethod.ocrAlias) ||
      matched.confidence < 0.8) {
    return null;
  }
  if (textLayer != null &&
      textLayer.isNotEmpty &&
      (textLayer.length != 1 || !textLayer.contains(matched.code))) {
    return null;
  }
  final identities = <String, (int, Set<int>)>{};
  for (final (dpi, observed) in observations.where((o) =>
      o.$2.method == RfiMethod.strict || o.$2.method == RfiMethod.ocrAlias)) {
    final entry = identities.putIfAbsent(observed.code, () => (0, <int>{}));
    identities[observed.code] = (entry.$1 + 1, entry.$2..add(dpi));
  }
  final repeated = identities.keys
      .where((code) => identities[code]!.$1 >= 2 && identities[code]!.$2.length >= 2)
      .toList()
    ..sort();
  if (repeated.length > 1 || (repeated.isNotEmpty && repeated.first != matched.code)) {
    return null;
  }
  final source = matched.method == RfiMethod.ocrAlias
      ? 'ocr:cover-recovery:confirmed:alias:CLV->CIV'
      : 'ocr:cover-recovery:confirmed';
  return OcrRead(matched: matched, source: source);
}

// ---- Image handling -------------------------------------------------------

/// Port of `GrayImage`: 8-bit luminance, row-major.
class GrayImage {
  const GrayImage({required this.width, required this.height, required this.pixels});

  final int width;
  final int height;
  final Uint8List pixels;
}

GrayImage coverTopBand(GrayImage image) {
  var height = (image.height * fastCoverTopPercent) ~/ 100;
  height = math.min(math.max(height, 1), image.height);
  final count = image.width * height;
  return GrayImage(
    width: image.width,
    height: height,
    pixels: Uint8List.sublistView(image.pixels, 0, count),
  );
}

bool bandHasTextCues(GrayImage band) {
  var dark = 0;
  for (final pixel in band.pixels) {
    if (pixel < 180) {
      dark += 1;
      if (dark >= 100) {
        return true;
      }
    }
  }
  return false;
}

GrayImage rotateGrayClockwise(GrayImage image, int degrees) {
  final (width, height) = switch (degrees) {
    0 || 180 => (image.width, image.height),
    90 || 270 => (image.height, image.width),
    _ => throw const OcrDocumentFailure('OCR fallback rotation is not a right angle'),
  };
  final pixels = Uint8List(width * height)..fillRange(0, width * height, 255);
  for (var y = 0; y < image.height; y++) {
    for (var x = 0; x < image.width; x++) {
      final (tx, ty) = switch (degrees) {
        0 => (x, y),
        90 => (image.height - 1 - y, x),
        180 => (image.width - 1 - x, image.height - 1 - y),
        _ => (y, image.width - 1 - x),
      };
      pixels[ty * width + tx] = image.pixels[y * image.width + x];
    }
  }
  return GrayImage(width: width, height: height, pixels: pixels);
}

GrayImage resizeGrayToFit(GrayImage image, int maxWidth, int maxHeight) {
  if (image.width == 0 || image.height == 0 || maxWidth == 0 || maxHeight == 0) {
    throw const OcrDocumentFailure('thumbnail dimensions are invalid');
  }
  final scale = math.min(
    math.min(maxWidth / image.width, maxHeight / image.height),
    1.0,
  );
  final width = math.max((image.width * scale).floor(), 1);
  final height = math.max((image.height * scale).floor(), 1);
  final pixels = Uint8List(width * height);
  for (var y = 0; y < height; y++) {
    final sy = (y * image.height) ~/ height;
    for (var x = 0; x < width; x++) {
      final sx = (x * image.width) ~/ width;
      pixels[y * width + x] = image.pixels[sy * image.width + sx];
    }
  }
  return GrayImage(width: width, height: height, pixels: pixels);
}

/// Port of `adaptive_threshold` (Gaussian-weighted local mean, reflect-101).
GrayImage adaptiveThreshold(GrayImage image, double band, int block, int constant) {
  if (image.width == 0 ||
      image.height == 0 ||
      image.pixels.length != image.width * image.height ||
      !(band > 0 && band <= 1) ||
      block < 3 ||
      block.isEven) {
    throw const OcrDocumentFailure('adaptive OCR preprocessing parameters are invalid');
  }
  final height = math.max((image.height * band).toInt(), 1);
  final kernel = _gaussianKernel(block);
  final radius = block ~/ 2;
  final horizontal = Float32List(image.width * height);
  for (var row = 0; row < height; row++) {
    for (var column = 0; column < image.width; column++) {
      var total = 0.0;
      for (var offset = 0; offset < kernel.length; offset++) {
        final source = _reflect101(column + offset - radius, image.width);
        total += image.pixels[row * image.width + source] * kernel[offset];
      }
      horizontal[row * image.width + column] = total;
    }
  }
  final pixels = Uint8List(image.width * height);
  for (var row = 0; row < height; row++) {
    for (var column = 0; column < image.width; column++) {
      var mean = 0.0;
      for (var offset = 0; offset < kernel.length; offset++) {
        final source = _reflect101(row + offset - radius, height);
        mean += horizontal[source * image.width + column] * kernel[offset];
      }
      final value = image.pixels[row * image.width + column];
      pixels[row * image.width + column] = value > mean - constant ? 255 : 0;
    }
  }
  return GrayImage(width: image.width, height: height, pixels: pixels);
}

List<double> _gaussianKernel(int block) {
  final radius = block ~/ 2;
  final sigma = 0.3 * ((block - 1) * 0.5 - 1) + 0.8;
  final kernel = [
    for (var offset = -radius; offset <= radius; offset++)
      math.exp(-(offset * offset) / (2 * sigma * sigma)),
  ];
  final total = kernel.reduce((a, b) => a + b);
  return [for (final value in kernel) value / total];
}

int _reflect101(int index, int length) {
  if (length == 1) {
    return 0;
  }
  var i = index;
  while (i < 0 || i >= length) {
    i = i < 0 ? -i : 2 * length - i - 2;
  }
  return i;
}

void writePgm(String path, GrayImage image) {
  if (File(path).existsSync()) {
    throw const OcrDocumentFailure('OCR PGM create: file already exists');
  }
  final header = ascii.encode('P5\n${image.width} ${image.height}\n255\n');
  final bytes = BytesBuilder(copy: false)
    ..add(header)
    ..add(image.pixels);
  try {
    File(path).writeAsBytesSync(bytes.toBytes(), flush: true);
  } on FileSystemException catch (error) {
    throw OcrDocumentFailure('OCR PGM write: ${error.message}');
  }
}

// ---- PNG ------------------------------------------------------------------

const List<int> _pngSignature = [137, 80, 78, 71, 13, 10, 26, 10];
const int _maxPixels = 100000000;

/// Port of `read_png_gray`: EXPAND + STRIP_16 semantics, luminance via the
/// same integer weights as the Rust module, alpha composited on white.
GrayImage readPngGray(String path) {
  final Uint8List data;
  try {
    data = File(path).readAsBytesSync();
  } on FileSystemException catch (error) {
    throw OcrDocumentFailure('OCR image open: ${error.message}');
  }
  return decodePngGray(data);
}

GrayImage decodePngGray(Uint8List data) {
  if (data.length < 8 || !_startsWith(data, _pngSignature)) {
    throw const OcrDocumentFailure('OCR PNG header: invalid signature');
  }
  var offset = 8;
  int? width, height, bitDepth, colorType, interlace;
  Uint8List? palette;
  Uint8List? transparency;
  final idat = BytesBuilder(copy: false);
  while (offset + 8 <= data.length) {
    final length = _be32(data, offset);
    final type = ascii.decode(data.sublist(offset + 4, offset + 8));
    final start = offset + 8;
    final end = start + length;
    if (end + 4 > data.length) {
      throw const OcrDocumentFailure('OCR PNG pixels: truncated chunk');
    }
    final body = data.sublist(start, end);
    final crc = _be32(data, end);
    if (getCrc32([...data.sublist(offset + 4, offset + 8), ...body]) != crc) {
      throw OcrDocumentFailure('OCR PNG pixels: CRC mismatch in $type');
    }
    switch (type) {
      case 'IHDR':
        width = _be32(body, 0);
        height = _be32(body, 4);
        bitDepth = body[8];
        colorType = body[9];
        interlace = body[12];
      case 'PLTE':
        palette = body;
      case 'tRNS':
        transparency = body;
      case 'IDAT':
        idat.add(body);
      default:
        break;
    }
    offset = end + 4;
    if (type == 'IEND') {
      break;
    }
  }
  if (width == null || height == null || bitDepth == null || colorType == null) {
    throw const OcrDocumentFailure('OCR PNG header: missing IHDR');
  }
  if (width <= 0 || height <= 0 || width * height > _maxPixels) {
    throw const OcrDocumentFailure('OCR PNG exceeds the pixel limit');
  }
  final channels = switch (colorType) {
    0 => 1,
    2 => 3,
    3 => 1,
    4 => 2,
    6 => 4,
    _ => throw const OcrDocumentFailure('OCR PNG color type is invalid'),
  };
  final raw = Uint8List.fromList(ZLibDecoder().decodeBytes(idat.toBytes()));
  final gray = Uint8List(width * height);
  final decoder = _PngPixels(
    colorType: colorType,
    bitDepth: bitDepth,
    channels: channels,
    palette: palette,
    transparency: transparency,
  );
  if (interlace == 0) {
    final stride = _rowBytes(width, channels, bitDepth);
    final rows = _unfilter(raw, height, stride, _bpp(channels, bitDepth));
    for (var y = 0; y < height; y++) {
      final row = Uint8List.sublistView(rows, y * stride, (y + 1) * stride);
      for (var x = 0; x < width; x++) {
        gray[y * width + x] = decoder.grayAt(row, x);
      }
    }
  } else {
    var cursor = 0;
    const passes = [
      [0, 0, 8, 8], [4, 0, 8, 8], [0, 4, 4, 8], [2, 0, 4, 4],
      [0, 2, 2, 4], [1, 0, 2, 2], [0, 1, 1, 2],
    ];
    for (final pass in passes) {
      final (xs, ys, dx, dy) = (pass[0], pass[1], pass[2], pass[3]);
      final pw = width > xs ? (width - xs + dx - 1) ~/ dx : 0;
      final ph = height > ys ? (height - ys + dy - 1) ~/ dy : 0;
      if (pw == 0 || ph == 0) {
        continue;
      }
      final stride = _rowBytes(pw, channels, bitDepth);
      final size = (stride + 1) * ph;
      if (cursor + size > raw.length) {
        throw const OcrDocumentFailure('OCR PNG pixels: truncated interlace data');
      }
      final rows = _unfilter(
        Uint8List.sublistView(raw, cursor, cursor + size),
        ph,
        stride,
        _bpp(channels, bitDepth),
        withFilterBytes: true,
      );
      cursor += size;
      for (var py = 0; py < ph; py++) {
        final row = Uint8List.sublistView(rows, py * stride, (py + 1) * stride);
        for (var px = 0; px < pw; px++) {
          gray[(ys + py * dy) * width + xs + px * dx] = decoder.grayAt(row, px);
        }
      }
    }
  }
  return GrayImage(width: width, height: height, pixels: gray);
}

bool _startsWith(Uint8List data, List<int> prefix) {
  for (var i = 0; i < prefix.length; i++) {
    if (data[i] != prefix[i]) {
      return false;
    }
  }
  return true;
}

int _be32(Uint8List data, int offset) =>
    (data[offset] << 24) | (data[offset + 1] << 16) | (data[offset + 2] << 8) | data[offset + 3];

int _rowBytes(int width, int channels, int bitDepth) =>
    (width * channels * bitDepth + 7) ~/ 8;

int _bpp(int channels, int bitDepth) => math.max(1, (channels * bitDepth) ~/ 8);

/// Reverses PNG scanline filters. When [withFilterBytes] is false the input is
/// unfiltered-length rows that still carry a leading filter byte per row.
Uint8List _unfilter(
  Uint8List input,
  int rows,
  int stride,
  int bpp, {
  bool withFilterBytes = true,
}) {
  final out = Uint8List(rows * stride);
  for (var y = 0; y < rows; y++) {
    final inStart = y * (stride + 1);
    final filter = input[inStart];
    final outStart = y * stride;
    for (var x = 0; x < stride; x++) {
      final raw = input[inStart + 1 + x];
      final left = x >= bpp ? out[outStart + x - bpp] : 0;
      final up = y > 0 ? out[outStart - stride + x] : 0;
      final upLeft = (y > 0 && x >= bpp) ? out[outStart - stride + x - bpp] : 0;
      final value = switch (filter) {
        0 => raw,
        1 => raw + left,
        2 => raw + up,
        3 => raw + ((left + up) >> 1),
        4 => raw + _paeth(left, up, upLeft),
        _ => throw const OcrDocumentFailure('OCR PNG pixels: invalid filter type'),
      };
      out[outStart + x] = value & 0xff;
    }
  }
  return out;
}

int _paeth(int a, int b, int c) {
  final p0 = a + b - c;
  final pa = (p0 - a).abs();
  final pb = (p0 - b).abs();
  final pc = (p0 - c).abs();
  if (pa <= pb && pa <= pc) {
    return a;
  }
  return pb <= pc ? b : c;
}

class _PngPixels {
  _PngPixels({
    required this.colorType,
    required this.bitDepth,
    required this.channels,
    required this.palette,
    required this.transparency,
  });

  final int colorType;
  final int bitDepth;
  final int channels;
  final Uint8List? palette;
  final Uint8List? transparency;

  int _sample(Uint8List row, int index) {
    if (bitDepth == 16) {
      return (row[index * 2] << 8) | row[index * 2 + 1];
    }
    if (bitDepth == 8) {
      return row[index];
    }
    final bit = index * bitDepth;
    final shift = 8 - bitDepth - (bit & 7);
    return (row[bit >> 3] >> shift) & ((1 << bitDepth) - 1);
  }

  int _to8(int raw) => switch (bitDepth) {
    16 => raw >> 8,
    8 => raw,
    _ => raw * 255 ~/ ((1 << bitDepth) - 1),
  };

  int grayAt(Uint8List row, int x) {
    final base = x * channels;
    switch (colorType) {
      case 0:
        final raw = _sample(row, base);
        final value = _to8(raw);
        final key = transparency;
        if (key != null && key.length >= 2 && raw == ((key[0] << 8) | key[1])) {
          return _composite(value, 0);
        }
        return value;
      case 4:
        return _composite(_to8(_sample(row, base)), _to8(_sample(row, base + 1)));
      case 2:
        final r = _to8(_sample(row, base));
        final g = _to8(_sample(row, base + 1));
        final b = _to8(_sample(row, base + 2));
        final key = transparency;
        final keyed = key != null && key.length >= 6 &&
            _sample(row, base) == ((key[0] << 8) | key[1]) &&
            _sample(row, base + 1) == ((key[2] << 8) | key[3]) &&
            _sample(row, base + 2) == ((key[4] << 8) | key[5]);
        return _composite(_rgbToGray(r, g, b), keyed ? 0 : 255);
      case 6:
        final r = _to8(_sample(row, base));
        final g = _to8(_sample(row, base + 1));
        final b = _to8(_sample(row, base + 2));
        return _composite(_rgbToGray(r, g, b), _to8(_sample(row, base + 3)));
      case 3:
        final index = _sample(row, base);
        final pal = palette;
        if (pal == null || index * 3 + 2 >= pal.length) {
          throw const OcrDocumentFailure('OCR PNG palette index is out of range');
        }
        final gray = _rgbToGray(pal[index * 3], pal[index * 3 + 1], pal[index * 3 + 2]);
        final key = transparency;
        final alpha = key != null && index < key.length ? key[index] : 255;
        return _composite(gray, alpha);
      default:
        throw const OcrDocumentFailure('OCR PNG palette was not expanded');
    }
  }
}

int _rgbToGray(int r, int g, int b) {
  final weighted = r * 77 + g * 150 + b * 29;
  return (weighted + 128) >> 8;
}

int _composite(int value, int alpha) {
  final result = value * alpha + 255 * (255 - alpha);
  return (result + 127) ~/ 255;
}

/// Encodes an 8-bit grayscale PNG (filter 0 per row).
Uint8List encodePngGray(GrayImage image) {
  final raw = BytesBuilder(copy: false);
  for (var y = 0; y < image.height; y++) {
    raw.addByte(0);
    raw.add(Uint8List.sublistView(
      image.pixels,
      y * image.width,
      (y + 1) * image.width,
    ));
  }
  final idat = ZLibEncoder().encode(raw.toBytes());
  final out = BytesBuilder(copy: false)..add(_pngSignature);
  void chunk(String type, List<int> body) {
    final typeBytes = ascii.encode(type);
    out.add(_u32(body.length));
    out.add(typeBytes);
    out.add(body);
    out.add(_u32(getCrc32([...typeBytes, ...body])));
  }

  chunk('IHDR', [
    ..._u32(image.width),
    ..._u32(image.height),
    8,
    0,
    0,
    0,
    0,
  ]);
  chunk('IDAT', idat);
  chunk('IEND', const []);
  return out.toBytes();
}

List<int> _u32(int value) => [
  (value >> 24) & 0xff,
  (value >> 16) & 0xff,
  (value >> 8) & 0xff,
  value & 0xff,
];

/// Port of `render_first_page_png`.
Future<void> renderFirstPagePng(
  String input,
  String output,
  int dpi, {
  PdfPageRenderer? renderer,
}) => renderPageToPng(input, output, dpi, 0, renderer: renderer);

/// Port of `render_page_png` through the platform renderer.
Future<void> renderPageToPng(
  String input,
  String output,
  int dpi,
  int pageIndex, {
  PdfPageRenderer? renderer,
}) async {
  final session = await (renderer ?? selectPdfPageRenderer()).openSession(input);
  try {
    await session.renderPagePng(output, dpi, pageIndex);
  } finally {
    await session.close();
  }
}

/// Port of `render_page_thumbnail_data_url`.
Future<String> renderPageThumbnailDataUrl(
  String input,
  int pageIndex, {
  PdfPageRenderer? renderer,
}) async {
  const maxWidth = 540;
  const maxHeight = 760;
  const maxPngBytes = 768 * 1024;
  final temporary = Directory.systemTemp.createTempSync('docflow-thumb-');
  try {
    final rendered = p.join(temporary.path, 'page.png');
    await renderPageToPng(input, rendered, 144, pageIndex, renderer: renderer);
    final thumbnail = resizeGrayToFit(readPngGray(rendered), maxWidth, maxHeight);
    final bytes = encodePngGray(thumbnail);
    if (bytes.length > maxPngBytes) {
      throw const OcrDocumentFailure('thumbnail exceeds the encoded size limit');
    }
    return 'data:image/png;base64,${base64Encode(bytes)}';
  } finally {
    temporary.deleteSync(recursive: true);
  }
}

/// Port of `image_pdf_from_png`: a one-page, image-only PDF.
void imagePdfFromPng(String input, String output) {
  final image = readPngGray(input);
  final compressed = ZLibEncoder().encode(image.pixels);
  const content = 'q 612 0 0 792 0 0 cm /Im0 Do Q\n';
  final imageHeader = ascii.encode(
    '<< /Type /XObject /Subtype /Image /Width ${image.width} /Height ${image.height} '
    '/ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode /Length ${compressed.length} >>\nstream\n',
  );
  final objects = <List<int>>[
    ascii.encode('<< /Type /Catalog /Pages 2 0 R >>'),
    ascii.encode('<< /Type /Pages /Kids [3 0 R] /Count 1 >>'),
    ascii.encode(
      '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>',
    ),
    [...imageHeader, ...compressed, ...ascii.encode('\nendstream')],
    [
      ...ascii.encode('<< /Length ${content.length} >>\nstream\n'),
      ...ascii.encode(content),
      ...ascii.encode('endstream'),
    ],
  ];
  final bytes = BytesBuilder(copy: false)
    ..add(ascii.encode('%PDF-1.4\n%'))
    ..add(const [0xE2, 0xE3, 0xCF, 0xD3, 0x0A]);
  final offsets = <int>[];
  for (var i = 0; i < objects.length; i++) {
    offsets.add(bytes.length);
    bytes.add(ascii.encode('${i + 1} 0 obj\n'));
    bytes.add(objects[i]);
    bytes.add(ascii.encode('\nendobj\n'));
  }
  final xref = bytes.length;
  final xrefText = StringBuffer('xref\n0 ${objects.length + 1}\n0000000000 65535 f \n');
  for (final offset in offsets) {
    xrefText.write('${offset.toString().padLeft(10, '0')} 00000 n \n');
  }
  bytes.add(ascii.encode(xrefText.toString()));
  bytes.add(ascii.encode(
    'trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n$xref\n%%EOF\n',
  ));
  File(output).writeAsBytesSync(bytes.toBytes(), flush: true);
}

// ---- Orientation ----------------------------------------------------------

OrientationDetection? parseOrientationOutput(String output, int dpi) {
  int? rotate;
  double? confidence;
  for (final line in output.split('\n')) {
    final colon = line.indexOf(':');
    if (colon < 0) {
      continue;
    }
    final key = line.substring(0, colon).trim();
    final value = line.substring(colon + 1).trim();
    if (key == 'Rotate') {
      rotate = int.tryParse(value);
    } else if (key == 'Orientation confidence') {
      confidence = double.tryParse(value);
    }
  }
  if (rotate == null || !const {0, 90, 180, 270}.contains(rotate)) {
    throw const OrientationError('orientation result has no valid rotation');
  }
  if (confidence == null || !confidence.isFinite || confidence < 0) {
    throw const OrientationError('orientation result has no valid confidence');
  }
  return OrientationDetection(
    rotateClockwise: rotate,
    confidence: confidence,
    method: 'tesseract-osd:${dpi}dpi',
  );
}

OrientationDetection? selectOsdDetection(
  OrientationDetection? primary,
  OrientationDetection? secondary,
) {
  const strong = 10.0;
  const conflict = 3.0;
  const consensusTotal = 12.0;
  if (primary != null && secondary != null) {
    if (primary.rotateClockwise == secondary.rotateClockwise) {
      final combined = primary.confidence + secondary.confidence;
      if (primary.confidence >= strong ||
          secondary.confidence >= strong ||
          (primary.confidence >= conflict &&
              secondary.confidence >= conflict &&
              combined >= consensusTotal)) {
        return OrientationDetection(
          rotateClockwise: primary.rotateClockwise,
          confidence: combined.clamp(strong, 100.0),
          method: 'tesseract-osd-consensus:250+350dpi',
        );
      }
      return null;
    }
    final firstStrong = primary.confidence >= strong && secondary.confidence < conflict;
    final secondStrong = secondary.confidence >= strong && primary.confidence < conflict;
    if (firstStrong) {
      return primary;
    }
    if (secondStrong) {
      return secondary;
    }
    return null;
  }
  final single = primary ?? secondary;
  if (single != null && single.confidence >= strong) {
    return single;
  }
  return null;
}

class DirectionScore {
  DirectionScore({
    required this.rotateClockwise,
    required this.wordCount,
    required this.confidenceSum,
    this.headerWordCount = 0,
    this.headerConfidenceSum = 0,
    Map<String, double>? headerRfiCodes,
  }) : headerRfiCodes = headerRfiCodes ?? <String, double>{};

  final int rotateClockwise;
  final int wordCount;
  final double confidenceSum;
  final int headerWordCount;
  final double headerConfidenceSum;
  final Map<String, double> headerRfiCodes;

  double get averageConfidence => wordCount == 0 ? 0 : confidenceSum / wordCount;
}

/// Stable descending sort (Rust `sort_by` is stable; Dart's `List.sort` is not).
List<T> _stableSorted<T>(Iterable<T> items, int Function(T a, T b) compare) {
  final indexed = [for (final (i, v) in items.indexed) (i, v)];
  indexed.sort((a, b) {
    final c = compare(a.$2, b.$2);
    return c != 0 ? c : a.$1.compareTo(b.$1);
  });
  return [for (final e in indexed) e.$2];
}

OrientationDetection? selectDirectionScore(List<DirectionScore> scores, int dpi) {
  const minWords = 20;
  const minAverage = 55.0;
  const minSumRatio = 1.45;
  const minMargin = 8.0;
  final ranked = _stableSorted(scores, (a, b) {
    final c = b.confidenceSum.compareTo(a.confidenceSum);
    return c != 0 ? c : b.wordCount.compareTo(a.wordCount);
  });
  if (ranked.length < 2) {
    return null;
  }
  final winner = ranked[0];
  final runner = ranked[1];
  final sumRatio = winner.confidenceSum / math.max(runner.confidenceSum, 1.0);
  final margin = winner.averageConfidence - runner.averageConfidence;
  if (winner.wordCount < minWords ||
      winner.averageConfidence < minAverage ||
      sumRatio < minSumRatio ||
      margin < minMargin) {
    return null;
  }
  return OrientationDetection(
    rotateClockwise: winner.rotateClockwise,
    confidence: (10.0 + (sumRatio - 1.0) * 35.0 + margin).clamp(10.0, 100.0),
    method: 'tesseract-eng-direction-score:${dpi}dpi',
  );
}

String directionSummary(List<DirectionScore> scores) {
  final ranked = _stableSorted(scores, (a, b) => b.confidenceSum.compareTo(a.confidenceSum));
  return ranked
      .take(2)
      .map((s) => '${s.rotateClockwise}°:${s.wordCount}w/${s.confidenceSum.round()}')
      .join(', ');
}

String osdSummary(OrientationDetection? detection) => detection == null
    ? 'unavailable'
    : '${detection.rotateClockwise}°/${detection.confidence.toStringAsFixed(1)}';

class _CorrelationConflict implements Exception {
  const _CorrelationConflict();
}

OrientationDetection? selectCorrelatedDirectionScore(
  List<DirectionScore> primary,
  List<DirectionScore> secondary,
  OrientationDetection? primaryOsd,
  OrientationDetection? secondaryOsd,
) {
  if (primaryOsd == null || secondaryOsd == null) {
    return null;
  }
  if (primaryOsd.rotateClockwise != secondaryOsd.rotateClockwise ||
      primaryOsd.confidence < 0.5 ||
      secondaryOsd.confidence < 0.5) {
    return null;
  }
  for (final scores in [primary, secondary]) {
    final ranked = _stableSorted(scores, (a, b) => b.confidenceSum.compareTo(a.confidenceSum));
    if (ranked.length < 2) {
      return null;
    }
    final winner = ranked[0];
    final runner = ranked[1];
    if (winner.rotateClockwise != primaryOsd.rotateClockwise ||
        winner.wordCount < 20 ||
        winner.averageConfidence < 45 ||
        winner.confidenceSum / math.max(runner.confidenceSum, 1.0) < 1.04 ||
        winner.averageConfidence - runner.averageConfidence < 5) {
      return null;
    }
  }
  return OrientationDetection(
    rotateClockwise: primaryOsd.rotateClockwise,
    confidence: 10,
    method: 'tesseract-osd-ocr-consensus:250+350dpi',
  );
}

/// Port of `select_correlated_header_score_checked`. Throws on conflict.
OrientationDetection? selectCorrelatedHeaderScoreChecked(
  List<DirectionScore> primary,
  List<DirectionScore> secondary,
) {
  final rfi = _selectCorrelatedRfiHeaderScore(primary, secondary);
  final text = _selectCorrelatedTextHeaderScore(primary, secondary);
  if (rfi != null && text != null && rfi.rotateClockwise != text.rotateClockwise) {
    throw const _CorrelationConflict();
  }
  return rfi ?? text;
}

OrientationDetection? _selectCorrelatedRfiHeaderScore(
  List<DirectionScore> primary,
  List<DirectionScore> secondary,
) {
  (DirectionScore, String)? unique(List<DirectionScore> scores) {
    if (scores.any((s) => s.headerRfiCodes.length > 1)) {
      throw const _CorrelationConflict();
    }
    final candidates = <(DirectionScore, String)>[];
    for (final score in scores) {
      if (score.headerRfiCodes.isEmpty) {
        continue;
      }
      final code = score.headerRfiCodes.keys.first;
      if (score.headerRfiCodes[code]! >= 60) {
        candidates.add((score, code));
      }
    }
    if (candidates.length > 1) {
      throw const _CorrelationConflict();
    }
    return candidates.isEmpty ? null : candidates.first;
  }

  final a = unique(primary);
  final b = unique(secondary);
  if (a == null && b == null) {
    return null;
  }
  if (a != null && b != null && a.$1.rotateClockwise == b.$1.rotateClockwise && a.$2 == b.$2) {
    return OrientationDetection(
      rotateClockwise: a.$1.rotateClockwise,
      confidence: 10,
      method: 'tesseract-rfi-header-consensus:250+350dpi',
    );
  }
  throw const _CorrelationConflict();
}

OrientationDetection? _selectCorrelatedTextHeaderScore(
  List<DirectionScore> primary,
  List<DirectionScore> secondary,
) {
  DirectionScore? winner(List<DirectionScore> scores) {
    final ranked = _stableSorted(
      scores.where((s) => s.headerWordCount >= 3 && s.headerConfidenceSum >= 120),
      (a, b) {
        final c = b.headerConfidenceSum.compareTo(a.headerConfidenceSum);
        return c != 0 ? c : b.headerWordCount.compareTo(a.headerWordCount);
      },
    );
    if (ranked.isEmpty) {
      return null;
    }
    final best = ranked.first;
    final runner = ranked.length > 1 ? ranked[1].headerConfidenceSum : 0.0;
    return best.headerConfidenceSum - runner >= 80 &&
            best.headerConfidenceSum / math.max(runner, 1.0) >= 1.8
        ? best
        : null;
  }

  final a = winner(primary);
  final b = winner(secondary);
  if (a == null || b == null || a.rotateClockwise != b.rotateClockwise) {
    return null;
  }
  return OrientationDetection(
    rotateClockwise: a.rotateClockwise,
    confidence: 10,
    method: 'tesseract-page-header-consensus:250+350dpi',
  );
}

/// Port of `parse_direction_score_tsv`.
DirectionScore parseDirectionScoreTsv(String output, int rotate, int imageHeight) {
  var wordCount = 0;
  var confidenceSum = 0.0;
  var headerWords = 0;
  var headerSum = 0.0;
  final headerCodes = <String, double>{};
  final lines = output.split('\n');
  for (final line in lines.skip(1)) {
    final columns = _splitN(line, '\t', 12);
    if (columns.length != 12 ||
        !columns[11].split('').any((c) => RegExp(r'[A-Za-z0-9]').hasMatch(c))) {
      continue;
    }
    final confidence = double.tryParse(columns[10]);
    if (confidence == null || !confidence.isFinite) {
      continue;
    }
    final top = _parseUsize(columns[7]) ?? imageHeight;
    final height = _parseUsize(columns[9]) ?? 0;
    final inHeader = top + height ~/ 2 <= imageHeight ~/ 6;
    if (inHeader && confidence >= 60) {
      for (final code in exactCodes(columns[11])) {
        final best = headerCodes[code];
        headerCodes[code] = best == null ? confidence : math.max(best, confidence);
      }
    }
    if (confidence >= 30) {
      wordCount += 1;
      confidenceSum += confidence;
      if (inHeader) {
        headerWords += 1;
        headerSum += confidence;
      }
    }
  }
  return DirectionScore(
    rotateClockwise: rotate,
    wordCount: wordCount,
    confidenceSum: confidenceSum,
    headerWordCount: headerWords,
    headerConfidenceSum: headerSum,
    headerRfiCodes: headerCodes,
  );
}

List<String> _splitN(String line, String sep, int n) {
  final parts = <String>[];
  var rest = line;
  while (parts.length < n - 1) {
    final i = rest.indexOf(sep);
    if (i < 0) {
      break;
    }
    parts.add(rest.substring(0, i));
    rest = rest.substring(i + sep.length);
  }
  if (rest.isNotEmpty || parts.isNotEmpty) {
    parts.add(rest);
  }
  return parts;
}

int? _parseUsize(String value) {
  final parsed = int.tryParse(value);
  return parsed == null || parsed < 0 ? null : parsed;
}

// ---- Tesseract process runner --------------------------------------------

enum _RunFailure { spawn, outputLimit, timeout }

class _RunOutcome {
  const _RunOutcome(this.exitCode, this.failure);

  final int? exitCode;
  final _RunFailure? failure;
}

Future<_RunOutcome> _runBounded({
  required String executable,
  required List<String> arguments,
  required Map<String, String> environment,
  required String stdoutPath,
  required String? stderrPath,
  required int maxOutputBytes,
  Duration timeout = const Duration(seconds: 30),
}) async {
  final Process process;
  try {
    process = await Process.start(
      executable,
      arguments,
      environment: environment,
      includeParentEnvironment: true,
    );
  } on Object {
    return const _RunOutcome(null, _RunFailure.spawn);
  }
  process.stdin.close().ignore();
  final stdoutSink = File(stdoutPath).openWrite();
  final stderrSink = stderrPath == null ? null : File(stderrPath).openWrite();
  var total = 0;
  var overflow = false;
  var timedOut = false;

  Future<void> pump(Stream<List<int>> stream, IOSink? sink, bool counted) async {
    await for (final chunk in stream) {
      if (counted) {
        total += chunk.length;
        if (total > maxOutputBytes) {
          overflow = true;
          process.kill(ProcessSignal.sigkill);
          break;
        }
      }
      sink?.add(chunk);
    }
  }

  final timer = Timer(timeout, () {
    timedOut = true;
    process.kill(ProcessSignal.sigkill);
  });
  final pumps = Future.wait([
    pump(process.stdout, stdoutSink, true),
    pump(process.stderr, stderrSink, stderrPath != null),
  ]);
  final exitCode = await process.exitCode;
  timer.cancel();
  await pumps;
  await stdoutSink.close();
  await stderrSink?.close();
  if (overflow) {
    return const _RunOutcome(null, _RunFailure.outputLimit);
  }
  if (timedOut) {
    return const _RunOutcome(null, _RunFailure.timeout);
  }
  return _RunOutcome(exitCode, null);
}

String _readTextOrEmpty(String path) {
  try {
    return utf8.decode(File(path).readAsBytesSync());
  } on Object {
    return '';
  }
}

/// Port of `run_tesseract_with_whitelist`. Output is read from stdout only.
Future<String> runTesseract(
  String executable,
  String image,
  String temporary,
  String? tessdata,
  int psm,
  int attempt, {
  String? whitelist = _ocrAlphabet,
}) async {
  const maxOutput = 1024 * 1024;
  final outputPath = p.join(temporary, 'tesseract-$attempt-$psm.txt');
  final args = [
    image,
    'stdout',
    '--oem',
    '3',
    '--psm',
    '$psm',
    '-l',
    'eng',
    if (whitelist != null) ...['-c', 'tessedit_char_whitelist=$whitelist'],
  ];
  final outcome = await _runBounded(
    executable: executable,
    arguments: args,
    environment: tessdata == null ? {} : {'TESSDATA_PREFIX': tessdata},
    stdoutPath: outputPath,
    stderrPath: null,
    maxOutputBytes: maxOutput,
  );
  switch (outcome.failure) {
    case _RunFailure.spawn:
      throw const OcrEngineMissing();
    case _RunFailure.outputLimit:
      throw const OcrDocumentFailure('Tesseract output exceeded the size limit');
    case _RunFailure.timeout:
      throw const OcrDocumentFailure('Tesseract timed out');
    case null:
      break;
  }
  if (outcome.exitCode != 0) {
    throw OcrDocumentFailure('Tesseract exited with exit code ${outcome.exitCode}');
  }
  if (File(outputPath).lengthSync() > maxOutput) {
    throw const OcrDocumentFailure('Tesseract output exceeded the size limit');
  }
  try {
    return utf8.decode(File(outputPath).readAsBytesSync());
  } on FormatException catch (error) {
    throw OcrDocumentFailure('OCR output decode: ${error.message}');
  }
}

/// Port of `run_orientation_tesseract`: OSD (`-l osd --psm 0`). Returns
/// stdout and stderr joined by a newline, as the Rust parser expects.
Future<String> runOrientationTesseract(
  String executable,
  String image,
  String temporary,
  String tessdata,
  int dpi,
) async {
  const maxOutput = 1024 * 1024;
  final stdoutPath = p.join(temporary, 'orientation-$dpi.stdout.txt');
  final stderrPath = p.join(temporary, 'orientation-$dpi.stderr.txt');
  final outcome = await _runBounded(
    executable: executable,
    arguments: [image, 'stdout', '--psm', '0', '-l', 'osd'],
    environment: {'TESSDATA_PREFIX': tessdata},
    stdoutPath: stdoutPath,
    stderrPath: stderrPath,
    maxOutputBytes: maxOutput,
  );
  switch (outcome.failure) {
    case _RunFailure.spawn:
      throw const OcrDocumentFailure('orientation detector could not start');
    case _RunFailure.outputLimit:
      throw const OcrDocumentFailure('orientation detector output exceeded the limit');
    case _RunFailure.timeout:
      throw const OcrDocumentFailure('orientation detector timed out');
    case null:
      break;
  }
  final combined = '${_readTextOrEmpty(stdoutPath)}\n${_readTextOrEmpty(stderrPath)}';
  if (outcome.exitCode != 0) {
    throw OcrDocumentFailure(
      'orientation detector failed: ${combined.split('\n').first}',
    );
  }
  return combined;
}

/// Port of `run_direction_tesseract` (`--oem 3 --psm 11 -l eng`, TSV output).
Future<DirectionScore> runDirectionTesseract(
  String executable,
  String image,
  String temporary,
  String tessdata,
  int rotate,
  int dpi,
  int imageHeight,
) async {
  const maxOutput = 8 * 1024 * 1024;
  final outputPath = p.join(temporary, 'direction-$dpi-$rotate.tsv');
  final errorPath = p.join(temporary, 'direction-$dpi-$rotate.stderr.txt');
  final outcome = await _runBounded(
    executable: executable,
    arguments: [
      image, 'stdout', '--oem', '3', '--psm', '11', '-l', 'eng',
      '-c', 'tessedit_create_tsv=1',
    ],
    environment: {'TESSDATA_PREFIX': tessdata},
    stdoutPath: outputPath,
    stderrPath: errorPath,
    maxOutputBytes: maxOutput,
  );
  switch (outcome.failure) {
    case _RunFailure.spawn:
      throw const OcrDocumentFailure('orientation fallback could not start');
    case _RunFailure.outputLimit:
      throw const OcrDocumentFailure('orientation fallback output exceeded the limit');
    case _RunFailure.timeout:
      throw const OcrDocumentFailure('orientation fallback timed out');
    case null:
      break;
  }
  if (outcome.exitCode != 0) {
    final error = _readTextOrEmpty(errorPath);
    throw OcrDocumentFailure(
      'orientation fallback failed: ${error.split('\n').first}',
    );
  }
  return parseDirectionScoreTsv(_readTextOrEmpty(outputPath), rotate, imageHeight);
}

/// Port of `detect_orientation_by_direction_scores`.
Future<(OrientationDetection?, List<DirectionScore>)> detectOrientationByDirectionScores(
  String executable,
  String image,
  String temporary,
  String tessdata,
  int dpi,
) async {
  if (!isBundledRegularFile(p.join(tessdata, 'eng.traineddata'))) {
    throw const OcrDocumentFailure(
      'Tesseract English data is unavailable for orientation fallback',
    );
  }
  final maxSide = dpi >= 350 ? 3500 : 2600;
  final source = readPngGray(image);
  final resized = resizeGrayToFit(source, maxSide, maxSide);
  final scores = <DirectionScore>[];
  for (final rotate in [0, 90, 180, 270]) {
    final rotated = rotateGrayClockwise(resized, rotate);
    final path = p.join(temporary, 'direction-$dpi-$rotate.pgm');
    writePgm(path, rotated);
    scores.add(await runDirectionTesseract(
      executable,
      path,
      temporary,
      tessdata,
      rotate,
      dpi,
      rotate == 90 || rotate == 270 ? resized.width : resized.height,
    ));
  }
  return (selectDirectionScore(scores, dpi), scores);
}

/// Port of `tesseract_candidates`. The DOCFLOW_TESSERACT override is not read.
List<String> tesseractCandidates() {
  final name = Platform.isWindows ? 'tesseract.exe' : 'tesseract';
  final candidates = <String>[];
  final dir = p.dirname(Platform.resolvedExecutable);
  candidates.addAll([
    p.join(dir, 'resources', 'tesseract', name),
    p.join(dir, '../Resources/resources/tesseract', name),
    p.join(dir, 'tesseract', name),
    p.join(dir, name),
  ]);
  if (Platform.isWindows) {
    for (final root in [
      r'C:\Program Files\Tesseract-OCR',
      r'C:\Program Files (x86)\Tesseract-OCR',
    ]) {
      candidates.add(p.join(root, name));
    }
    for (final variable in ['ProgramFiles', 'ProgramFiles(x86)']) {
      final value = Platform.environment[variable];
      if (value != null) {
        candidates.add(p.join(value, 'Tesseract-OCR', name));
      }
    }
    final local = Platform.environment['LOCALAPPDATA'];
    if (local != null) {
      candidates.add(p.join(local, 'Programs', 'Tesseract-OCR', name));
    }
  }
  final pathVar = Platform.environment['PATH'];
  if (pathVar != null) {
    for (final entry in pathVar.split(Platform.isWindows ? ';' : ':')) {
      if (entry.isNotEmpty) {
        candidates.add(p.join(entry, name));
      }
    }
  }
  return candidates;
}

/// Port of `bundled_regular_file`: a regular file, not a link.
/// UNVERIFIED on Windows: reparse points are not distinguished from links.
bool isBundledRegularFile(String path) =>
    FileSystemEntity.typeSync(path, followLinks: false) == FileSystemEntityType.file;

/// Port of `find_tessdata`.
String? findTessdata(String executable) {
  final dir = p.dirname(executable);
  for (final candidate in [
    p.join(dir, 'tessdata'),
    p.join(dir, '../tessdata'),
  ]) {
    if (isBundledRegularFile(p.join(candidate, 'eng.traineddata'))) {
      return candidate;
    }
  }
  final prefix = Platform.environment['TESSDATA_PREFIX'];
  if (prefix != null) {
    for (final candidate in [prefix, p.join(prefix, 'tessdata')]) {
      if (isBundledRegularFile(p.join(candidate, 'eng.traineddata'))) {
        return candidate;
      }
    }
  }
  if (Platform.isWindows) {
    for (final root in [
      r'C:\Program Files\Tesseract-OCR\tessdata',
      r'C:\Program Files (x86)\Tesseract-OCR\tessdata',
    ]) {
      if (isBundledRegularFile(p.join(root, 'eng.traineddata'))) {
        return root;
      }
    }
  }
  return null;
}
