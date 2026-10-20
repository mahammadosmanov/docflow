// Port of the review.rs decision logic (review.rs has no inline Rust tests;
// these cover its contract: scan ordering and filtering, preview steps,
// warnings, and the fail-closed source and timestamp checks).
import 'dart:io';

import 'package:docflow/engine/contracts.dart';
import 'package:docflow/engine/errors.dart';
import 'package:docflow/engine/naming.dart';
import 'package:docflow/engine/review.dart';
import 'package:docflow/engine/rfi_code.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

const _code = 'AZE-SUN-INT-QMS-RFI-MEC-0123';

class _FakeOcr implements ReviewOcrAdapter {
  _FakeOcr(this.codes);

  final Map<String, String> codes;

  @override
  Future<ReviewOcrRead> read(String path) async {
    final code = codes[p.basename(path)];
    if (code == null) {
      throw StateError('no OCR result');
    }
    final (canonical, matched) = canonicalAndFind(code);
    return ReviewOcrRead(matched: matched, source: canonical);
  }
}

void main() {
  late Directory fixture;
  late String destination;
  late String review;
  late String trash;

  setUp(() {
    fixture = Directory.systemTemp.createTempSync('e3-review-');
    destination = p.join(fixture.path, 'dest');
    review = p.join(destination, '_review');
    trash = p.join(fixture.path, 'trash');
    Directory(review).createSync(recursive: true);
    Directory(trash).createSync(recursive: true);
    destination = Directory(destination).resolveSymbolicLinksSync();
    review = Directory(review).resolveSymbolicLinksSync();
    trash = Directory(trash).resolveSymbolicLinksSync();
  });

  tearDown(() => fixture.deleteSync(recursive: true));

  String write(String name) =>
      (File(p.join(review, name))..writeAsBytesSync([1])).path;

  test('scan lists visible PDFs in name order and attaches OCR codes', () async {
    write('b.PDF');
    write('a.pdf');
    write('notes.txt');
    write('.hidden.pdf');
    final items = await scan(
      destination,
      _FakeOcr({'a.pdf': _code, 'b.PDF': 'nothing readable here'}),
    );

    expect(items.map((item) => item.name).toList(), ['a.pdf', 'b.PDF']);
    expect(items[0].code, _code);
    expect(items[0].targetExists, isFalse);
    expect(items[0].target, targetFor(destination, _code));
    expect(items[1].code, '');
    expect(items[1].target, isNull);
  });

  test('scan returns nothing when the review directory is absent', () async {
    Directory(review).deleteSync(recursive: true);
    expect(await scan(destination, _FakeOcr({})), isEmpty);
  });

  test('preview plans a put as folder creation plus rename, and delete to trash', () {
    final rfi = write('rfi.pdf');
    final junk = write('junk.pdf');
    final plan = preview(destination, trash, [
      ReviewDecision(path: rfi, action: ReviewAction.put, code: _code),
      ReviewDecision(path: junk, action: ReviewAction.delete, code: ''),
    ], '20260101T000000');

    expect(plan.opId, 'rfi.review');
    expect(plan.warnings, isEmpty);
    expect(plan.steps.map((step) => step.kind).toList(), [
      StepKind.mkdir,
      StepKind.move,
      StepKind.move,
    ]);
    expect(plan.steps[0].dst, folderFor(destination, _code));
    expect(plan.steps[1].src, rfi);
    expect(plan.steps[1].dst, targetFor(destination, _code));
    expect(plan.steps[1].noteKey, 'note.renamed');
    expect(plan.steps[2].src, junk);
    expect(plan.steps[2].dst, p.join(trash, '20260101T000000_junk.pdf'));
    expect(plan.steps[2].noteKey, 'note.deleted');
    expect(plan.steps[2].meta['action'], 'delete');
  });

  test('preview warns instead of planning when the code is not strict', () {
    final rfi = write('rfi.pdf');
    final plan = preview(destination, trash, [
      ReviewDecision(path: rfi, action: ReviewAction.put, code: 'garbage'),
    ], '20260101T000000');
    expect(plan.steps, isEmpty);
    expect(plan.warnings.single.key, 'warn.no_code');
    expect(plan.warnings.single.subject, 'rfi.pdf');
  });

  test('preview rejects bad timestamps and oversized requests', () {
    final rfi = write('rfi.pdf');
    expect(
      () => preview(destination, trash, [
        ReviewDecision(path: rfi, action: ReviewAction.delete, code: ''),
      ], 'bad-stamp'),
      throwsA(isA<BackendError>()),
    );
    expect(
      () => preview(destination, trash, const [], ''),
      throwsA(isA<BackendError>()),
    );
  });

  test('preview refuses a source outside the review directory', () {
    final outside = File(p.join(fixture.path, 'outside.pdf'))..writeAsBytesSync([1]);
    expect(
      () => preview(destination, trash, [
        ReviewDecision(path: outside.path, action: ReviewAction.delete, code: ''),
      ], '20260101T000000'),
      throwsA(
        isA<BackendError>().having((e) => e.code, 'code', 'path_escape'),
      ),
    );
  });

  test('recoverableDeletePlan builds trash moves and rejects non-PDF sources', () {
    final pdf = write('unreadable.pdf');
    final plan = recoverableDeletePlan(destination, trash, [pdf], '20260101T000000');
    expect(plan.steps.single.kind, StepKind.move);
    expect(plan.steps.single.src, pdf);
    expect(plan.steps.single.dst, p.join(trash, '20260101T000000_unreadable.pdf'));

    final text = write('unreadable.txt');
    expect(
      () => recoverableDeletePlan(destination, trash, [text], '20260101T000000'),
      throwsA(isA<BackendError>()),
    );
  });

  test('recoverableDeletePlan enforces the review item limit', () {
    expect(
      () => recoverableDeletePlan(
        destination,
        trash,
        List.filled(maxReviewItems + 1, 'x'),
        '20260101T000000',
      ),
      throwsA(isA<BackendError>()),
    );
  });
}
