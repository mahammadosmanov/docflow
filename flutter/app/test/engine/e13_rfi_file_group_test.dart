// Port of src-tauri/tests/rfi_file_group.rs and the inline tests of
// src-tauri/src/operations/rfi_file_group.rs.
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:archive/archive.dart';
import 'package:docflow/engine/contracts.dart';
import 'package:docflow/engine/errors.dart';
import 'package:docflow/engine/operations/rfi_file_group.dart';
import 'package:docflow/engine/rfi_code.dart' as rfi_code;
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

class _NoOcr extends RfiOcrAdapter {
  const _NoOcr();

  @override
  Future<RfiOcrRead> read(String path) async => const RfiOcrRead(
    matched: rfi_code.RfiMatch(
      code: '',
      discipline: '',
      number: '',
      method: rfi_code.RfiMethod.miss,
      confidence: 0.0,
    ),
    source: 'test',
  );

  @override
  String diagnostics() => 'test OCR';
}

/// Junk bytes are not a PDF: the page counter fails like `pdf::inspect`.
int _junkPdfPageCount(String path) {
  throw const FormatException('invalid PDF');
}

Future<Plan> _preview(
  String source,
  String destination, {
  void Function(ProgressEvent)? progress,
}) => previewRfiFileGroup(
  params: GroupRfiFilesParams(source: source, destination: destination),
  ocr: const _NoOcr(),
  pdfPageCount: _junkPdfPageCount,
  progress: progress ?? (_) {},
  cancelled: () => false,
);

ArchiveFile _entry(String name, String text) {
  final bytes = utf8.encode(text);
  return ArchiveFile(name, bytes.length, bytes);
}

void _writeDocx(String path, String text) {
  final archive = Archive()
    ..addFile(
      _entry(
        'word/document.xml',
        '<w:document xmlns:w="urn:test"><w:body><w:p><w:r><w:t>$text</w:t>'
            '</w:r></w:p></w:body></w:document>',
      ),
    );
  File(path).writeAsBytesSync(ZipEncoder().encode(archive)!);
}

void _writeDocxXml(String path, String xml) {
  final archive = Archive()..addFile(_entry('word/document.xml', xml));
  File(path).writeAsBytesSync(ZipEncoder().encode(archive)!);
}

List<Map<String, Object?>> _rows(Plan plan) {
  final editor = plan.meta['editor']! as Map<String, Object?>;
  return [
    for (final row in editor['rows']! as List)
      Map<String, Object?>.from(row as Map),
  ];
}

Set<String> _dsts(Plan plan) => {
  for (final step in plan.steps)
    if (step.dst != null) step.dst!,
};

late Directory _tmp;

void main() {
  setUp(() {
    _tmp = Directory.systemTemp.createTempSync('e13_rfi_group_');
  });

  tearDown(() {
    _tmp.deleteSync(recursive: true);
  });

  String sub(String name) {
    final dir = Directory(_join(_tmp.path, name))..createSync();
    return dir.resolveSymbolicLinksSync();
  }

  group('exact codes (inline)', () {
    test('file group identifies short RFI suffixes but not other numbers', () {
      expect(exactCodes('AZE-SUN-INT-QMS-RFI-CIV-901'), {
        'AZE-SUN-INT-QMS-RFI-CIV-0901',
      });
      expect(
        exactCodes('AZE-SUN-INT-QMS-RFI-CIV-0901'),
        exactCodes('AZE-SUN-INT-QMS-RFI-CIV-901'),
      );
      expect(exactCodes('ITP-CIV-901'), isEmpty);
      expect(exactCodes('2026-09-01'), isEmpty);
      expect(exactCodes('AZE-SUN-INT-QMS-RFI-CIV-09010'), isEmpty);
    });
  });

  test('unsupported exact discipline creates no grouped output', () async {
    final source = sub('incoming');
    final destination = sub('grouped');
    File(_join(source, 'AZE-SUN-INT-QMS-RFI-ARC-1263.pdf'))
        .writeAsBytesSync(utf8.encode('unsupported discipline'));
    File(_join(source, 'AZE-SUN-INT-QMS-RFI-ARC-1263.eml'))
        .writeAsStringSync('Subject: AZE-SUN-INT-QMS-RFI-ARC-1263');

    final plan = await _preview(source, destination);

    expect(_rows(plan), isEmpty);
    expect(
      plan.steps.every((step) => step.meta['warningReview'] == true),
      isTrue,
      reason: 'unsupported identifiers may only result in _review handling',
    );
    expect(Directory(destination).listSync(), isEmpty);
  });

  test('preview groups exact filename, office, email and utf16 codes without mutating sources', () async {
    final source = sub('incoming');
    final destination = sub('grouped');
    Directory(_join(source, 'Old Yerlesdirilmis')).createSync();
    File(_join(source, 'AZE-SUN-INT-QMS-RFI-CIV-1260 SKETCH.pdf'))
        .writeAsStringSync('filename is sufficient');
    _writeDocx(
      _join(source, '2.AF110-CON-MEC-002 Checklist.docx'),
      'AZE-SUN-INT-QMS-RFI-\nCIV-1263',
    );
    File(_join(source, 'AZE-SUN-INT-QMS-RFI-CIV-1260_1261.eml'))
        .writeAsStringSync('Subject: RFIs\r\n\r\nAZE-SUN-INT-QMS-RFI-CIV-1262');
    const message =
        'AZE-SUN-INT-QMS-RFI-CIV-1263 and AZE-SUN-INT-QMS-RFI-CIV-1264';
    final utf16 = Uint8List(message.length * 2);
    for (var index = 0; index < message.length; index++) {
      utf16[index * 2] = message.codeUnitAt(index) & 0xff;
      utf16[index * 2 + 1] = message.codeUnitAt(index) >> 8;
    }
    File(_join(source, 'Outlook item.msg')).writeAsBytesSync(utf16);
    File(_join(source, 'unmatched.bin')).writeAsStringSync('no code');
    File(
      _join(source, 'Old Yerlesdirilmis', 'AZE-SUN-INT-QMS-RFI-CIV-9999.pdf'),
    ).writeAsStringSync('ignored subfolder');

    final progress = <ProgressEvent>[];
    final plan = await _preview(source, destination, progress: progress.add);

    expect(Directory(destination).listSync(), isEmpty);
    final rows = _rows(plan);
    expect(rows, hasLength(7));
    final codes = [for (final row in rows) row['rfiCode']];
    expect(
      codes,
      containsAll(const [
        'AZE-SUN-INT-QMS-RFI-CIV-1260',
        'AZE-SUN-INT-QMS-RFI-CIV-1261',
        'AZE-SUN-INT-QMS-RFI-CIV-1262',
        'AZE-SUN-INT-QMS-RFI-CIV-1263',
        'AZE-SUN-INT-QMS-RFI-CIV-1264',
      ]),
    );
    expect(
      plan.warnings.any(
        (warning) =>
            warning.key == 'warn.rfi.fileGroup.noCode' &&
            warning.subject == 'unmatched.bin',
      ),
      isTrue,
    );
    expect(jsonEncode(plan.toJson()), isNot(contains('9999')));
    expect(
      plan.steps.any(
        (step) =>
            step.src != null &&
            step.src!.endsWith('Outlook item.msg') &&
            step.dst != null &&
            step.dst!.contains('CIV-1264'),
      ),
      isTrue,
    );
    expect(
      plan.steps.any(
        (step) =>
            step.src != null &&
            step.src!.endsWith('unmatched.bin') &&
            step.dst != null &&
            step.dst!.contains('_review'),
      ),
      isTrue,
    );
    expect(progress.any((e) => e.phase == 'file_group_discovery'), isTrue);
    expect(progress.any((e) => e.phase == 'file_group_analysis'), isTrue);
    expect(progress.last.phase, 'preview_ready');

    final revisedRows = [
      for (final row in rows)
        if (row['rfiCode'] == 'AZE-SUN-INT-QMS-RFI-CIV-1264')
          {...row, 'enabled': false}
        else
          row,
    ];
    final revised = reviseRfiFileGroup(plan, {'rows': revisedRows});
    expect(
      revised.steps.any((step) => (step.dst ?? '').contains('CIV-1264')),
      isFalse,
    );
    expect(
      revised.steps.any((step) => (step.dst ?? '').contains('_review')),
      isTrue,
    );
  });

  test(
    'group files routes supported disciplines into discipline and code folders',
    () async {
      final source = sub('incoming');
      final destination = sub('grouped');
      const codes = [
        'AZE-SUN-INT-QMS-RFI-CIV-0001',
        'AZE-SUN-INT-QMS-RFI-MEC-0002',
        'AZE-SUN-INT-QMS-RFI-ELE-0003',
        'AZE-SUN-INT-QMS-RFI-PRC-0004',
      ];
      for (final code in codes) {
        File(_join(source, '$code.eml')).writeAsStringSync('Subject: RFI');
        File(_join(source, '$code.docx'))
            .writeAsBytesSync(utf8.encode('document'));
      }
      final sourceEntries = Directory(
        source,
      ).listSync().map((e) => p.basename(e.path)).toList()..sort();

      final plan = await _preview(source, destination);

      for (final (discipline, code) in [
        ('CIV', codes[0]),
        ('MEC', codes[1]),
        ('ELE', codes[2]),
        ('PRC', codes[3]),
      ]) {
        final folder = _join(destination, discipline, code);
        expect(_dsts(plan), contains(folder));
        expect(_dsts(plan), contains(_join(folder, '$code.docx')));
        expect(_dsts(plan), contains(_join(folder, '$code.eml')));
      }
      expect(plan.steps, hasLength(12));
      final currentEntries = Directory(
        source,
      ).listSync().map((e) => p.basename(e.path)).toList()..sort();
      expect(currentEntries, sourceEntries);

      final rows = _rows(plan);
      final revisedRows = [
        for (final row in rows)
          if (row['rfiCode'] == codes[1]) {...row, 'enabled': false} else row,
      ];
      final revised = reviseRfiFileGroup(plan, {'rows': revisedRows});
      expect(
        revised.steps.any((step) => (step.dst ?? '').contains(codes[1])),
        isFalse,
      );
      expect(
        revised.steps.any((step) => (step.dst ?? '').contains(codes[0])),
        isTrue,
      );

      final occupied = _join(destination, 'CIV', codes[0]);
      Directory(occupied).createSync(recursive: true);
      File(_join(occupied, '${codes[0]}.docx')).writeAsStringSync('keep');
      final collision = await _preview(source, destination);
      expect(
        File(_join(occupied, '${codes[0]}.docx')).readAsStringSync(),
        'keep',
      );
      expect(
        collision.warnings.any(
          (w) =>
              w.key == 'warn.rfi.fileGroup.exists' &&
              w.subject.contains(codes[0]),
        ),
        isTrue,
      );
    },
  );

  test(
    'preview rejects a symlinked intermediate discipline directory',
    () async {
      final source = sub('incoming');
      final destination = sub('grouped');
      final external = sub('external');
      Link(_join(destination, 'CIV')).createSync(external);

      const code = 'AZE-SUN-INT-QMS-RFI-CIV-1263';
      File(_join(source, '$code.pdf'))
          .writeAsBytesSync(utf8.encode('document'));
      File(_join(source, '$code.eml')).writeAsStringSync('Subject: $code');

      final plan = await _preview(source, destination);

      expect(
        plan.warnings.any(
          (w) =>
              w.key == 'warn.rfi.fileGroup.exists' && w.subject.contains(code),
        ),
        isTrue,
        reason: 'preview must surface an unsafe intermediate directory',
      );
      expect(
        plan.steps.every((step) => !(step.dst ?? '').contains(code)),
        isTrue,
        reason: 'preview must not offer writes through the symlink',
      );
      expect(Directory(external).listSync(), isEmpty);
    },
    skip: Platform.isWindows ? 'symlink creation needs privileges' : false,
  );

  test(
    'preview finds a wrapped RFI in a Word table without joining the next cell',
    () async {
      final source = sub('incoming');
      final destination = sub('grouped');
      _writeDocxXml(
        _join(source, 'Tracker Installation Checklist.docx'),
        '<w:document xmlns:w="urn:test"><w:body><w:tbl><w:tr>'
        '<w:tc><w:p><w:r><w:t>Report/RFI No.</w:t></w:r></w:p></w:tc>'
        '<w:tc><w:p><w:r><w:t>AZE-SUN-INT-QMS-RFI-</w:t><w:br/><w:t>MEC-0765</w:t></w:r></w:p></w:tc>'
        '</w:tr><w:tr><w:tc><w:p><w:r><w:t>MOS No.</w:t></w:r></w:p></w:tc></w:tr>'
        '</w:tbl></w:body></w:document>',
      );
      File(_join(source, 'AZE-SUN-INT-QMS-RFI-MEC-0765.eml'))
          .writeAsStringSync('Subject: AZE-SUN-INT-QMS-RFI-MEC-0765');

      final plan = await _preview(source, destination);

      final rows = _rows(plan);
      expect(
        rows.any(
          (row) =>
              row['sourceFile'] == 'Tracker Installation Checklist.docx' &&
              row['rfiCode'] == 'AZE-SUN-INT-QMS-RFI-MEC-0765',
        ),
        isTrue,
      );
      expect(
        plan.warnings.any(
          (w) =>
              w.key == 'warn.rfi.fileGroup.noCode' &&
              w.subject.contains('Tracker Installation Checklist.docx'),
        ),
        isFalse,
      );
    },
  );

  test('preview rejects overlapping source and destination', () async {
    final source = sub('incoming');
    await expectLater(
      _preview(source, source),
      throwsA(
        isA<BackendError>().having((e) => e.code, 'code', 'path_invalid'),
      ),
    );
  });

  test('decorated or extended identifiers are not silently accepted', () async {
    final source = sub('incoming');
    final destination = sub('grouped');
    File(_join(source, 'ÂAZE-SUN-INT-QMS-RFI-CIV-1263.pdf'))
        .writeAsStringSync('invalid prefix');
    File(_join(source, 'AZE-SUN-INT-QMS-RFI-CIV-1264X.pdf'))
        .writeAsStringSync('invalid suffix');

    final plan = await _preview(source, destination);

    expect(_rows(plan), isEmpty);
    expect(
      plan.warnings
          .where((w) => w.key == 'warn.rfi.fileGroup.unreadable')
          .length,
      2,
    );
  });

  test('preview skips existing targets and unsafe folder collisions', () async {
    final source = sub('incoming');
    final destination = sub('grouped');
    const existingName = 'AZE-SUN-INT-QMS-RFI-CIV-1263.pdf';
    File(_join(source, existingName)).writeAsStringSync('new source');
    final existingDirectory = _join(
      destination,
      'CIV',
      'AZE-SUN-INT-QMS-RFI-CIV-1263',
    );
    Directory(existingDirectory).createSync(recursive: true);
    File(_join(existingDirectory, existingName))
        .writeAsStringSync('existing target');
    File(_join(source, 'AZE-SUN-INT-QMS-RFI-CIV-1264.pdf'))
        .writeAsStringSync('other source');
    File(_join(source, 'AZE-SUN-INT-QMS-RFI-CIV-1263_1264.eml'))
        .writeAsStringSync('Subject: collision fixtures');
    File(_join(existingDirectory, 'AZE-SUN-INT-QMS-RFI-CIV-1263_1264.eml'))
        .writeAsStringSync('existing mail target');
    File(_join(destination, 'CIV', 'AZE-SUN-INT-QMS-RFI-CIV-1264'))
        .writeAsStringSync('folder-name collision');

    final plan = await _preview(source, destination);

    expect(plan.steps, isEmpty);
    expect(
      plan.warnings.where((w) => w.key == 'warn.rfi.fileGroup.exists').length,
      4,
    );
  });

  test(
    'preview refuses every RFI output when that RFI has no email file',
    () async {
      final source = sub('incoming');
      final destination = sub('grouped');
      File(_join(source, 'AZE-SUN-INT-QMS-RFI-MEC-0765.pdf'))
          .writeAsStringSync('known RFI without mail');
      _writeDocx(
        _join(source, 'Tracker Installation Checklist.docx'),
        'AZE-SUN-INT-QMS-RFI-\nMEC-0765',
      );

      final plan = await _preview(source, destination);

      expect(_rows(plan), isEmpty);
      expect(plan.steps, isEmpty);
      expect(
        plan.warnings.any(
          (w) =>
              w.key == 'warn.rfi.fileGroup.missingEmail' &&
              w.subject.contains('AZE-SUN-INT-QMS-RFI-MEC-0765') &&
              w.subject.contains('Tracker Installation Checklist.docx'),
        ),
        isTrue,
      );
    },
  );

  test('email requirement is enforced independently for each RFI', () async {
    final source = sub('incoming');
    final destination = sub('grouped');
    File(_join(source, 'AZE-SUN-INT-QMS-RFI-MEC-0765_0766.pdf'))
        .writeAsStringSync('two related RFIs');
    File(_join(source, 'AZE-SUN-INT-QMS-RFI-MEC-0765.eml'))
        .writeAsStringSync('Subject: RFI package');

    final plan = await _preview(source, destination);

    final rows = _rows(plan);
    expect(
      rows.every((row) => row['rfiCode'] == 'AZE-SUN-INT-QMS-RFI-MEC-0765'),
      isTrue,
    );
    expect(
      rows.any(
        (row) => row['sourceFile'] == 'AZE-SUN-INT-QMS-RFI-MEC-0765_0766.pdf',
      ),
      isTrue,
    );
    expect(
      plan.warnings.any(
        (w) =>
            w.key == 'warn.rfi.fileGroup.missingEmail' &&
            w.subject.contains('AZE-SUN-INT-QMS-RFI-MEC-0766'),
      ),
      isTrue,
    );
    expect(
      plan.steps.any((step) => (step.dst ?? '').contains('MEC-0766')),
      isFalse,
    );
  });

  // UNVERIFIED: the Ventoy operator test needs the NativeOcr adapter and the
  // private document sets (DOCFLOW_VENTOY_GROUP_*). NativeOcr has no Dart port
  // yet, so this stays skipped rather than faked.
  test(
    'operator ventoy group files routes real documents and OCR scans',
    () {},
    skip: 'requires DOCFLOW_VENTOY_GROUP_* private inputs and a NativeOcr port',
  );
}

String _join(String a, [String? b, String? c, String? d]) =>
    p.joinAll([a, ?b, ?c, ?d]);
