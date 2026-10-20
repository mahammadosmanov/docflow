// Port of src-tauri/tests/batch_print.rs. Each group mirrors a Rust section;
// Rust thread gates are replaced by the synchronous hooks on FakeBatchAdapter.
@TestOn('vm')
library;

import 'dart:convert';
import 'dart:io';

import 'package:docflow/engine/contracts.dart';
import 'package:docflow/engine/errors.dart';
import 'package:docflow/engine/paths.dart';
import 'package:docflow/engine/ooxml.dart';
import 'package:docflow/engine/operations/batch_print.dart';
import 'package:docflow/engine/print_queue.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

import 'e11_batch_support.dart';

/// Deep copy of the editor rows, as the UI would send them back to revise.
List<Map<String, Object?>> editorRows(Plan plan) =>
    (jsonDecode(jsonEncode((plan.meta['editor']! as Map)['rows'])) as List)
        .cast<Map<String, Object?>>();

/// Asserts [call] throws a BackendError whose message is [message].
void expectBackendMessage(void Function() call, String message) {
  expect(
    call,
    throwsA(isA<BackendError>().having((e) => e.message, 'message', message)),
  );
}

void main() {
  group('workbook preview', () {
    test('preview expands Excel worksheets', () {
      final root = batchTempDir('expand');
      addTearDown(() => root.deleteSync(recursive: true));
      final first = p.join(root.path, 'first.pdf');
      final workbook = p.join(root.path, 'book.xlsx');
      final last = p.join(root.path, 'last.png');
      File(first).writeAsBytesSync('first'.codeUnits);
      writeWorkbook(workbook, [
        ('MEC', 17, null),
        ('Hidden', 44, 'hidden'),
        ('VeryHidden', 45, 'veryHidden'),
      ]);
      File(last).writeAsBytesSync('last'.codeUnits);
      final service = batchService(root, FakeBatchAdapter());

      final plan = service.preview(
        batchCaller,
        batchParams([first, workbook, last]),
        (_) {},
      );

      final rows = ((plan.meta['editor']! as Map)['rows']! as List)
          .cast<Map<String, Object?>>();
      expect(
        [
          for (final row in rows)
            (row['file'], (row['worksheet'] as Map?)?['name']),
        ],
        [
          ('first.pdf', null),
          ('book.xlsx', 'MEC'),
          ('book.xlsx', 'Hidden'),
          ('book.xlsx', 'VeryHidden'),
          ('last.png', null),
        ],
      );
      expect(
        [for (final row in rows) row['enabled']],
        [true, true, false, false, true],
      );
      expect((plan.meta['editor']! as Map)['reorderable'], isTrue);
      for (final row in rows.sublist(1, 4)) {
        final worksheet = row['worksheet']! as Map;
        expect(
          worksheet['relationshipType'],
          'http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet',
        );
        final sha = (row['signature']! as Map)['sha256']! as String;
        expect(sha.length, 64);
        expect(RegExp(r'^[0-9a-f]{64}$').hasMatch(sha), isTrue);
        final safety = row['workbookSafety']! as Map;
        expect(safety['hasXlmMacroSheet'], isFalse);
        expect(safety['hasWorkbookProtection'], isFalse);
      }
      final sig = [for (final row in rows.sublist(1, 4)) row['signature']];
      expect(sig[0], sig[1]);
      expect(sig[1], sig[2]);
      expect(jsonEncode(plan.toJson()), isNot(contains('PRIVATE CELL VALUE')));
    });

    test(
      'workbook preview uses a capability before reading the visible path',
      () {
        final root = batchTempDir('capability');
        addTearDown(() => root.deleteSync(recursive: true));
        final selectedDir = Directory(p.join(root.path, 'selected'))
          ..createSync();
        final pinnedDir = p.join(root.path, 'pinned');
        final outsideDir = Directory(p.join(root.path, 'outside'))
          ..createSync();
        final visible = p.join(selectedDir.path, 'book.xlsx');
        final outside = p.join(outsideDir.path, 'book.xlsx');
        writeWorkbook(visible, [('Selected', 1, null)]);
        writeWorkbook(outside, [('Outside', 2, null)]);
        final service = batchService(root, FakeBatchAdapter());
        var swapped = false;

        final plan = service.preview(batchCaller, batchParams([visible]), (_) {
          if (!swapped) {
            selectedDir.renameSync(pinnedDir);
            Link(selectedDir.path).createSync(outsideDir.path);
            swapped = true;
          }
        });

        expect(plan.steps, hasLength(1));
        final encoded = jsonEncode(plan.toJson());
        expect(encoded, contains('Selected'));
        expect(encoded, isNot(contains('Outside')));
      },
    );
  });

  group('unsafe facts and row limits', () {
    const officeRel =
        'http://schemas.openxmlformats.org/officeDocument/2006/relationships';
    const microsoftRel =
        'http://schemas.microsoft.com/office/2006/relationships';

    test('preview rejects unsafe workbook facts and expanded row overflow', () {
      final cases = <(String, String, Map<String, List<int>>, String)>[
        (
          '<workbookProtection lockStructure="1"/>',
          '',
          {},
          'warn.print.workbookUnsafeProtection',
        ),
        (
          '',
          '<Relationship Id="unsafe" Type="$microsoftRel/xlMacrosheet" Target="macrosheets/sheet1.xml"/>',
          {'xl/macrosheets/sheet1.xml': 'macrosheet'.codeUnits},
          'warn.print.workbookUnsafeXlm',
        ),
        (
          '',
          '<Relationship Id="unsafe" Type="$officeRel/externalLink" Target="externalLinks/externalLink1.xml"/>',
          {'xl/externalLinks/externalLink1.xml': 'external-link'.codeUnits},
          'warn.print.workbookUnsafeExternalLinks',
        ),
        (
          '',
          '<Relationship Id="unsafe" Type="$officeRel/connections" Target="connections.xml"/>',
          {
            'xl/connections.xml': '<connections xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><connection/></connections>'
                .codeUnits,
          },
          'warn.print.workbookUnsafeConnections',
        ),
      ];
      for (var index = 0; index < cases.length; index++) {
        final (workbookExtra, relationships, extra, warningKey) = cases[index];
        final root = batchTempDir('unsafe-$index');
        addTearDown(() => root.deleteSync(recursive: true));
        final workbook = p.join(root.path, 'unsafe-$index.xlsx');
        writeWorkbook(
          workbook,
          [('MEC', 1, null)],
          workbookExtra: workbookExtra,
          extraRelationships: relationships,
          extra: extra,
        );
        final service = batchService(root, FakeBatchAdapter());

        final plan = service.preview(
          batchCaller,
          batchParams([workbook]),
          (_) {},
        );

        expect(plan.steps, isEmpty);
        expect(plan.warnings, hasLength(2));
        expect(plan.warnings[0].key, warningKey);
        expect(plan.warnings[1].key, 'warn.print.noJobsSelected');
      }

      final root = batchTempDir('query-tables');
      addTearDown(() => root.deleteSync(recursive: true));
      final workbook = p.join(root.path, 'query.xlsx');
      const queryRelationships =
          '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="query" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="../queryTables/queryTable1.xml"/></Relationships>';
      writeWorkbook(
        workbook,
        [('MEC', 1, null)],
        extra: {
          'xl/worksheets/_rels/sheet1.xml.rels': queryRelationships.codeUnits,
          'xl/queryTables/queryTable1.xml': 'query-table'.codeUnits,
        },
      );
      final queryPlan = batchService(
        root,
        FakeBatchAdapter(),
      ).preview(batchCaller, batchParams([workbook]), (_) {});
      expect(queryPlan.steps, isEmpty);
      expect(queryPlan.warnings[0].key, 'warn.print.workbookUnsafeQueryTables');

      final overflowRoot = batchTempDir('overflow');
      addTearDown(() => overflowRoot.deleteSync(recursive: true));
      final first = p.join(overflowRoot.path, 'first.pdf');
      final overflow = p.join(overflowRoot.path, 'too-many.xlsx');
      File(first).writeAsBytesSync('first'.codeUnits);
      writeWorkbook(overflow, [
        for (var i = 1; i <= 250; i++) ('Sheet$i', i, null),
      ]);
      final overflowPlan = batchService(
        overflowRoot,
        FakeBatchAdapter(),
      ).preview(batchCaller, batchParams([first, overflow]), (_) {});
      expect(overflowPlan.steps, isEmpty);
      expect(overflowPlan.warnings[0].key, 'warn.print.expandedOrderTooLarge');
    });

    test(
      'preview rejects workbook discovery errors without authorizing rows',
      () {
        final root = batchTempDir('discovery');
        addTearDown(() => root.deleteSync(recursive: true));
        final malformed = p.join(root.path, 'malformed.xlsx');
        File(malformed).writeAsBytesSync('not an OOXML package'.codeUnits);
        final service = batchService(root, FakeBatchAdapter());

        final plan = service.preview(
          batchCaller,
          batchParams([malformed]),
          (_) {},
        );
        expect(plan.steps, isEmpty);
        expect(plan.warnings[0].key, 'warn.print.workbookDiscoveryFailed');

        final tooMany = p.join(root.path, 'too-many-sheets.xlsx');
        writeWorkbook(tooMany, [
          for (var i = 1; i <= 251; i++) ('Sheet$i', i, null),
        ]);
        final tooManyPlan = service.preview(
          batchCaller,
          batchParams([tooMany]),
          (_) {},
        );
        expect(tooManyPlan.steps, isEmpty);
        expect(
          tooManyPlan.warnings[0].key,
          'warn.print.workbookTooManyWorksheets',
        );
      },
    );

    test('expanded row limit applies after workbook-first and workbook-middle appends', () {
      for (final (sheetCount, workbookPosition) in [(250, 0), (249, 1)]) {
        final root = batchTempDir('row-limit-$sheetCount');
        addTearDown(() => root.deleteSync(recursive: true));
        final workbook = p.join(root.path, 'book.xlsx');
        final first = p.join(root.path, 'first.pdf');
        final trailing = p.join(root.path, 'trailing.png');
        File(first).writeAsBytesSync('first'.codeUnits);
        File(trailing).writeAsBytesSync('trailing'.codeUnits);
        writeWorkbook(workbook, [
          for (var i = 1; i <= sheetCount; i++) ('Sheet$i', i, null),
        ]);
        final files = workbookPosition == 0
            ? [workbook, trailing]
            : [first, workbook, trailing];
        final adapter = FakeBatchAdapter();
        final service = batchService(root, adapter);

        final plan = service.preview(batchCaller, batchParams(files), (_) {});

        expect(plan.steps, isEmpty);
        expect(plan.warnings, hasLength(1));
        expect(plan.warnings[0].key, 'warn.print.expandedOrderTooLarge');
        expect(adapter.submitted, isEmpty);
      }
    });
  });

  group('worksheet revise', () {
    test(
      'worksheet revise rejects row set, metadata, page range and source drift',
      () {
        final root = batchTempDir('revise-reject');
        addTearDown(() => root.deleteSync(recursive: true));
        final workbook = p.join(root.path, 'book.xlsx');
        writeWorkbook(workbook, [
          ('MEC', 17, null),
          ('Hidden', 44, 'hidden'),
          ('VeryHidden', 45, 'veryHidden'),
        ]);
        final service = batchService(root, FakeBatchAdapter());
        final plan = service.preview(
          batchCaller,
          batchParams([workbook]),
          (_) {},
        );
        final rows = editorRows(plan);

        expectBackendMessage(
          () => service.revise(batchCaller, plan.planId, rows.sublist(0, 2)),
          'print queue rows cannot be added or removed',
        );

        final duplicate = editorRows(plan);
        duplicate[2] = Map<String, Object?>.of(duplicate[0]);
        expectBackendMessage(
          () => service.revise(batchCaller, plan.planId, duplicate),
          'invalid print row identity',
        );

        final forged = editorRows(plan);
        forged[0]['_rowId'] = 'forged-row';
        expectBackendMessage(
          () => service.revise(batchCaller, plan.planId, forged),
          'invalid print row identity',
        );

        final tamperings = <(String, Object?)>[
          ('worksheet', {'name': 'Forged'}),
          ('signature', {'size': 1, 'mtimeNs': 2, 'sha256': 'a' * 64}),
          ('workbookSafety', {'connectionCount': 1}),
        ];
        for (final (field, value) in tamperings) {
          final tampered = editorRows(plan);
          tampered[0][field] = value;
          expectBackendMessage(
            () => service.revise(batchCaller, plan.planId, tampered),
            'immutable print row metadata changed',
          );
        }

        for (final invalidRange in ['1', 'ALL']) {
          final invalid = editorRows(plan);
          invalid[0]['pageRange'] = invalidRange;
          expectBackendMessage(
            () => service.revise(batchCaller, plan.planId, invalid),
            'error.print.worksheetPageRangeAllOnly',
          );
        }

        final disabled = editorRows(plan);
        for (final row in disabled) {
          row['enabled'] = false;
        }
        expectBackendMessage(
          () => service.revise(batchCaller, plan.planId, disabled),
          'print queue requires an enabled worksheet row',
        );

        for (final malformedEnabled in <Object?>[null, 'true']) {
          final edited = editorRows(plan);
          final hidden = edited.firstWhere(
            (row) => row['worksheetVisibility'] == 'hidden',
          );
          if (malformedEnabled == null) {
            hidden.remove('enabled');
          } else {
            hidden['enabled'] = malformedEnabled;
          }
          final revised = service.revise(batchCaller, plan.planId, edited);
          final revisedHidden = editorRows(revised)
              .firstWhere((row) => row['worksheetVisibility'] == 'hidden');
          expect(revisedHidden['enabled'], isFalse);
        }

        writeWorkbook(workbook, [('Changed', 99, null)]);
        expectBackendMessage(
          () => service.revise(batchCaller, plan.planId, rows),
          'error.print.workbookChanged',
        );
      },
    );
  });

  group('reorder and queue persistence', () {
    test(
      'worksheet revise preserves complete reordered row set and identity',
      () {
        final root = batchTempDir('reorder');
        addTearDown(() => root.deleteSync(recursive: true));
        final workbook = p.join(root.path, 'book.xlsm');
        writeWorkbook(
          workbook,
          [
            ('MEC', 17, null),
            ('Hidden', 44, 'hidden'),
            ('VeryHidden', 45, 'veryHidden'),
          ],
          extra: {'xl/vbaProject.bin': 'macro bytes'.codeUnits},
        );
        final service = batchService(root, FakeBatchAdapter());
        final plan = service.preview(
          batchCaller,
          batchParams([workbook]),
          (_) {},
        );
        final original = editorRows(plan);
        final originalIds = [for (final row in original) row['_rowId']];
        final reordered = [original[1], original[2], original[0]];
        reordered[0]['enabled'] = true;

        final revised = service.revise(batchCaller, plan.planId, reordered);

        final revisedRows = editorRows(revised);
        expect(
          [for (final row in revisedRows) row['_rowId']],
          [originalIds[1], originalIds[2], originalIds[0]],
        );
        expect(
          [
            for (final job in revised.meta['jobs']! as List)
              ((job as Map)['worksheet'] as Map)['name'],
          ],
          ['Hidden', 'MEC'],
        );
        expect(revisedRows[0]['worksheet'], original[1]['worksheet']);
        expect(revisedRows[0]['signature'], original[1]['signature']);
        expect(revisedRows[0]['workbookSafety'], original[1]['workbookSafety']);
      },
    );

    test('worksheet print queue persists and resumes typed metadata without submission', () {
      final root = batchTempDir('queue-persist');
      addTearDown(() => root.deleteSync(recursive: true));
      final workbook = p.join(root.path, 'book.xlsx');
      writeWorkbook(workbook, [('MEC', 17, null)]);
      final adapter = FakeBatchAdapter();
      final service = batchService(root, adapter);
      final plan = service.preview(
        batchCaller,
        batchParams([workbook]),
        (_) {},
      );
      final jobs = plan.meta['jobs']! as List;
      final expectedWorksheet = (jobs[0] as Map)['worksheet'];
      final expectedSafety = (jobs[0] as Map)['workbookSafety'];
      final expectedSignature = (jobs[0] as Map)['signature'];

      final result = service.apply(plan.planId, batchCaller, (_) {});

      expect(result.applied, 0);
      expect(result.failed, 1);
      expect(adapter.submitted, isEmpty);
      expect(result.errors[0].key, 'error.print.excelUnavailable');
      expect(result.errors[0].subject, 'book.xlsx');
      final queueId = result.meta['queueId']! as String;
      final saved = jsonDecode(
        File(p.join(root.path, 'print-queues', '$queueId.json'))
            .readAsStringSync(),
      ) as Map<String, Object?>;
      final savedJob = (saved['jobs']! as List)[0] as Map;
      expect(savedJob['worksheet'], expectedWorksheet);
      expect(savedJob['workbookSafety'], expectedSafety);
      expect(savedJob['signature'], expectedSignature);

      final restarted = batchService(root, adapter);
      final resumed = restarted.resume(batchCaller, queueId);
      final resumedRow = editorRows(resumed)[0];
      expect(resumedRow['worksheet'], expectedWorksheet);
      expect(resumedRow['workbookSafety'], expectedSafety);
      expect(resumedRow['signature'], expectedSignature);
    });
  });

  group('signatures, resume and job validation', () {
    test('same workbook rows with inconsistent signatures fail before authorization', () {
      final root = batchTempDir('inconsistent-sig');
      addTearDown(() => root.deleteSync(recursive: true));
      final workbook = p.join(root.path, 'book.xlsx');
      writeWorkbook(workbook, [('MEC', 17, null), ('CIV', 18, null)]);
      final adapter = FakeBatchAdapter();
      final service = batchService(root, adapter);
      final plan = service.preview(
        batchCaller,
        batchParams([workbook]),
        (_) {},
      );
      final result = service.apply(plan.planId, batchCaller, (_) {});
      final queueId = result.meta['queueId']! as String;
      final queuePath = p.join(root.path, 'print-queues', '$queueId.json');
      final saved = jsonDecode(
        File(queuePath).readAsStringSync(),
      ) as Map<String, Object?>;
      ((((saved['jobs']! as List)[1]) as Map)['signature'] as Map)['sha256'] =
          'b' * 64;
      File(queuePath).writeAsStringSync(jsonEncode(saved));

      final restarted = batchService(root, adapter);
      expectBackendMessage(() {
        final resumed = restarted.resume(batchCaller, queueId);
        final rows = editorRows(resumed);
        restarted.revise(batchCaller, resumed.planId, rows);
      }, 'error.print.workbookChanged');
      expect(adapter.submitted, isEmpty);
    });

    test('worksheet resume revalidates source before caching a plan', () {
      final root = batchTempDir('resume-revalidate');
      addTearDown(() => root.deleteSync(recursive: true));
      final workbook = p.join(root.path, 'book.xlsx');
      writeWorkbook(workbook, [('MEC', 17, null)]);
      final adapter = FakeBatchAdapter();
      final service = batchService(root, adapter);
      final plan = service.preview(
        batchCaller,
        batchParams([workbook]),
        (_) {},
      );
      final result = service.apply(plan.planId, batchCaller, (_) {});
      final queueId = result.meta['queueId']! as String;
      writeWorkbook(workbook, [('Changed', 99, null)]);

      final restarted = batchService(root, adapter);
      expectBackendMessage(
        () => restarted.resume(batchCaller, queueId),
        'error.print.workbookChanged',
      );
    });

    test('reordered resumed worksheet jobs persist across another pause', () {
      final root = batchTempDir('reorder-resume');
      addTearDown(() => root.deleteSync(recursive: true));
      final workbook = p.join(root.path, 'book.xlsx');
      writeWorkbook(workbook, [
        ('MEC', 17, null),
        ('CIV', 18, null),
        ('ELE', 19, null),
      ]);
      final adapter = FakeBatchAdapter();
      final service = batchService(root, adapter);
      final plan = service.preview(
        batchCaller,
        batchParams([workbook]),
        (_) {},
      );
      final first = service.apply(plan.planId, batchCaller, (_) {});
      final queueId = first.meta['queueId']! as String;

      final resumedService = batchService(root, adapter);
      final resumed = resumedService.resume(batchCaller, queueId);
      final rows = editorRows(resumed);
      final reordered = [rows[2], rows[1], rows[0]];
      final revised = resumedService.revise(
        batchCaller,
        resumed.planId,
        reordered,
      );
      resumedService.apply(revised.planId, batchCaller, (_) {});

      final restarted = batchService(root, adapter);
      final resumedAgain = restarted.resume(batchCaller, queueId);
      expect(
        [for (final row in editorRows(resumedAgain)) row['worksheetName']],
        ['ELE', 'CIV', 'MEC'],
      );
    });

    test('worksheet apply rejects source drift before any submission', () {
      final root = batchTempDir('apply-drift');
      addTearDown(() => root.deleteSync(recursive: true));
      final workbook = p.join(root.path, 'book.xlsx');
      writeWorkbook(workbook, [('MEC', 17, null)]);
      final adapter = FakeBatchAdapter();
      final service = batchService(root, adapter);
      final plan = service.preview(
        batchCaller,
        batchParams([workbook]),
        (_) {},
      );
      writeWorkbook(workbook, [('Changed', 99, null)]);

      expectBackendMessage(
        () => service.apply(plan.planId, batchCaller, (_) {}),
        'error.print.workbookChanged',
      );
      expect(adapter.submitted, isEmpty);
    });

    test(
      'worksheet job validation requires typed sha, safety and all-only range',
      () {
        PrintJob validJob() => PrintJob(
          jobId: 'worksheet-c1',
          path: '/approved/book.xlsx',
          printer: 'Office Printer',
          verb: 'docflow',
          format: 'XLSX',
          signature: FileSignature(size: 42, mtimeNs: 99, sha256: 'a' * 64),
          worksheet: const WorksheetDescriptor(
            workbookIndex: 1,
            sheetId: 17,
            relationshipId: 'rId1',
            relationshipType: 'http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet',
            partPath: 'xl/worksheets/sheet1.xml',
            name: 'MEC',
            visibility: WorksheetVisibility.visible,
          ),
          workbookSafety: const WorkbookSafetyFacts(),
        );

        validateJob(validJob());

        final missingSha = validJob()
          ..signature = FileSignature(size: 42, mtimeNs: 99);
        expect(() => validateJob(missingSha), throwsA(isA<Object>()));
        final uppercaseSha = validJob()
          ..signature = FileSignature(size: 42, mtimeNs: 99, sha256: 'A' * 64);
        expect(() => validateJob(uppercaseSha), throwsA(isA<Object>()));
        final missingSafety = validJob()..workbookSafety = null;
        expect(() => validateJob(missingSafety), throwsA(isA<Object>()));
        final unsafe = validJob()
          ..workbookSafety = const WorkbookSafetyFacts(externalLinkCount: 1);
        expect(() => validateJob(unsafe), throwsA(isA<Object>()));
        final ranged = validJob()..pageRange = '1';
        expectBackendMessage(
          () => validateJob(ranged),
          'error.print.worksheetPageRangeAllOnly',
        );
      },
    );

    test('legacy file job defaults optional worksheet metadata and sha256', () {
      final legacy = PrintJob.fromJson({
        'jobId': 'legacy-c1',
        'path': '/approved/legacy.pdf',
        'printer': 'Office Printer',
        'verb': 'docflow',
        'format': 'PDF',
        'signature': {'size': 12, 'mtimeNs': 34},
      });

      expect(legacy.signature.size, 12);
      expect(legacy.signature.mtimeNs, 34);
      expect(legacy.signature.sha256, isNull);
      expect(legacy.worksheet, isNull);
      expect(legacy.workbookSafety, isNull);
      validateJob(legacy);
      expect(jsonDecode(jsonEncode(legacy.toJson()))['signature'], {
        'size': 12,
        'mtimeNs': 34,
      });
    });
  });

  group('persisted metadata and resume authority', () {
    Map<String, Object?> legacyJson() => {
      'jobId': 'legacy-c1',
      'path': '/approved/legacy.pdf',
      'printer': 'Office Printer',
      'verb': 'docflow',
      'format': 'PDF',
      'signature': {'size': 12, 'mtimeNs': 34},
    };

    Map<String, Object?> worksheetJson() => {
      'jobId': 'worksheet-c1',
      'path': '/approved/book.xlsx',
      'printer': 'Office Printer',
      'verb': 'docflow',
      'format': 'XLSX',
      'signature': {'size': 12, 'mtimeNs': 34, 'sha256': 'a' * 64},
      'worksheet': {
        'workbookIndex': 1,
        'sheetId': 17,
        'relationshipId': 'rId1',
        'relationshipType': 'http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet',
        'partPath': 'xl/worksheets/sheet1.xml',
        'name': 'MEC',
        'visibility': 'visible',
      },
      'workbookSafety': {
        'hasVbaProject': false,
        'hasXlmMacroSheet': false,
        'hasWorkbookProtection': false,
        'externalLinkCount': 0,
        'connectionCount': 0,
        'queryTableCount': 0,
      },
    };

    test('persisted job metadata distinguishes legacy absence from null, malformed and orphans', () {
      final legacy = legacyJson();
      final parsedLegacy = PrintJob.fromJson(legacy);
      expect(parsedLegacy.worksheet, isNull);
      expect(parsedLegacy.workbookSafety, isNull);
      expect(parsedLegacy.signature.sha256, isNull);

      final validWorksheet = worksheetJson();
      expect(PrintJob.fromJson(validWorksheet).worksheet, isNotNull);

      Object? parse(Map<String, Object?> json) => PrintJob.fromJson(json);
      expect(
        () => parse({...legacy, 'worksheet': null}),
        throwsA(isA<Object>()),
        reason: 'explicit null worksheet',
      );
      expect(
        () => parse({
          ...legacy,
          'worksheet': {'name': 'MEC'},
        }),
        throwsA(isA<Object>()),
        reason: 'malformed worksheet',
      );
      expect(
        () => parse({...legacy, 'workbookSafety': null}),
        throwsA(isA<Object>()),
        reason: 'explicit null safety',
      );
      expect(
        () => parse({...legacy, 'worksheet': validWorksheet['worksheet']}),
        throwsA(isA<Object>()),
        reason: 'worksheet without safety or sha',
      );
      expect(
        () => parse({
          ...legacy,
          'workbookSafety': validWorksheet['workbookSafety'],
        }),
        throwsA(isA<Object>()),
        reason: 'safety without worksheet',
      );
      expect(
        () => parse({
          ...legacy,
          'signature': {'size': 12, 'mtimeNs': 34, 'sha256': 'a' * 64},
        }),
        throwsA(isA<Object>()),
        reason: 'sha without worksheet',
      );
      for (final missing in ['signature', 'workbookSafety']) {
        final orphan = Map<String, Object?>.of(validWorksheet)..remove(missing);
        expect(
          () => parse(orphan),
          throwsA(isA<Object>()),
          reason: 'worksheet metadata without $missing',
        );
      }
    });

    test(
      'resume authority requires original source and root capability identity',
      () {
        for (final replaceParent in [false, true]) {
          final root = batchTempDir('resume-authority-$replaceParent');
          addTearDown(() => root.deleteSync(recursive: true));
          final selectedParent = Directory(p.join(root.path, 'selected'))
            ..createSync();
          final approved = p.join(selectedParent.path, 'approved.pdf');
          File(approved)
              .writeAsBytesSync('SIGNATURE COMPATIBLE BYTES'.codeUnits);
          final adapter = FakeBatchAdapter()
            ..push(PrintFailure.failed('pause queue'));
          final service = batchService(root, adapter);
          final plan = service.preview(
            batchCaller,
            batchParams([approved]),
            (_) {},
          );
          final result = service.apply(plan.planId, batchCaller, (_) {});
          final queueId = result.meta['queueId']! as String;
          adapter.submitted.clear();
          final queuePath = p.join(root.path, 'print-queues', '$queueId.json');
          final saved = jsonDecode(
            File(queuePath).readAsStringSync(),
          ) as Map<String, Object?>;
          final job = (saved['jobs']! as List)[0] as Map<String, Object?>;

          final String replacement;
          if (replaceParent) {
            selectedParent.renameSync(p.join(root.path, 'pinned-selected'));
            Directory(selectedParent.path).createSync();
            replacement = p.join(selectedParent.path, 'approved.pdf');
            File(replacement)
                .writeAsBytesSync('SIGNATURE COMPATIBLE BYTES'.codeUnits);
          } else {
            replacement = p.join(selectedParent.path, 'tampered.pdf');
            File(replacement)
                .writeAsBytesSync('SIGNATURE COMPATIBLE BYTES'.codeUnits);
            job['path'] = replacement;
          }
          final signature = fileSignature(replacement);
          job['signature'] = {
            'size': signature.size,
            'mtimeNs': signature.mtimeNs,
            'sha256': signature.sha256,
          };
          File(queuePath).writeAsStringSync(jsonEncode(saved));

          final restarted = batchService(root, adapter);
          expectBackendMessage(
            () => restarted.resume(batchCaller, queueId),
            'error.print.queueSourceReselectionRequired',
          );
          expect(adapter.submitted, isEmpty);
          expect((saved['grantRootIdentities']! as List), isNotEmpty);
          expect((saved['sourceIdentities']! as List), isNotEmpty);
        }
      },
    );
  });

  group('resume identity and reentrancy', () {
    test(
      'resume rejects coherently rewritten job and source identity paths',
      () {
        final root = batchTempDir('rewritten-identity');
        addTearDown(() => root.deleteSync(recursive: true));
        final selectedParent = Directory(p.join(root.path, 'selected'))
          ..createSync();
        final approved = p.join(selectedParent.path, 'approved.pdf');
        final replacement = p.join(selectedParent.path, 'replacement.pdf');
        File(approved).writeAsBytesSync('ORIGINAL APPROVED PDF'.codeUnits);
        File(replacement).writeAsBytesSync('UNAPPROVED REPLACEMENT'.codeUnits);
        final adapter = FakeBatchAdapter()
          ..push(PrintFailure.failed('pause queue'));
        final service = batchService(root, adapter);
        final plan = service.preview(
          batchCaller,
          batchParams([approved]),
          (_) {},
        );
        final result = service.apply(plan.planId, batchCaller, (_) {});
        final queueId = result.meta['queueId']! as String;
        adapter.submitted.clear();
        final inspectedBefore = adapter.inspected;
        final queuePath = p.join(root.path, 'print-queues', '$queueId.json');
        final saved = jsonDecode(
          File(queuePath).readAsStringSync(),
        ) as Map<String, Object?>;
        final job = (saved['jobs']! as List)[0] as Map<String, Object?>;
        final source =
            (saved['sourceIdentities']! as List)[0] as Map<String, Object?>;
        final originalCanonical = source['canonicalPath'];
        final originalIdentity = source['identity'];
        job['path'] = replacement;
        source['path'] = replacement;
        expect(source['canonicalPath'], originalCanonical);
        expect(source['identity'], originalIdentity);
        File(queuePath).writeAsStringSync(jsonEncode(saved));
        final queueBefore = File(queuePath).readAsBytesSync();

        final restarted = batchService(root, adapter);
        expectBackendMessage(
          () => restarted.resume(batchCaller, queueId),
          'error.print.queueSourceReselectionRequired',
        );
        expect(File(queuePath).readAsBytesSync(), queueBefore);
        expect(adapter.inspected, inspectedBefore);
        expect(adapter.submitted, isEmpty);
      },
    );

    test('resumed plan rejects same-signature source identity swap before submission', () {
      final root = batchTempDir('same-signature-swap');
      addTearDown(() => root.deleteSync(recursive: true));
      final selectedParent = Directory(p.join(root.path, 'selected'))
        ..createSync();
      final approved = p.join(selectedParent.path, 'approved.pdf');
      final originalBackup = p.join(selectedParent.path, 'original.pdf');
      File(approved).writeAsBytesSync('SIGNATURE COMPATIBLE BYTES'.codeUnits);
      final originalModified = File(approved).lastModifiedSync();
      final adapter = FakeBatchAdapter()
        ..push(PrintFailure.failed('pause queue'));
      final service = batchService(root, adapter);
      final plan = service.preview(
        batchCaller,
        batchParams([approved]),
        (_) {},
      );
      final result = service.apply(plan.planId, batchCaller, (_) {});
      final queueId = result.meta['queueId']! as String;
      adapter.submitted.clear();

      final restarted = batchService(root, adapter);
      final resumed = restarted.resume(batchCaller, queueId);
      File(approved).renameSync(originalBackup);
      File(approved).writeAsBytesSync('SIGNATURE COMPATIBLE BYTES'.codeUnits);
      File(approved).setLastModifiedSync(originalModified);
      expect(fileSignature(approved).size, fileSignature(originalBackup).size);

      expectBackendMessage(
        () => restarted.apply(resumed.planId, batchCaller, (_) {}),
        'error.print.queueSourceReselectionRequired',
      );
      expect(adapter.submitted, isEmpty);
    });

    test('concurrent resume reserves queue before loading and caching', () {
      final root = batchTempDir('concurrent-resume');
      addTearDown(() => root.deleteSync(recursive: true));
      final approved = p.join(root.path, 'approved.pdf');
      File(approved).writeAsBytesSync('approved'.codeUnits);
      final adapter = FakeBatchAdapter()
        ..push(PrintFailure.failed('pause queue'));
      final service = batchService(root, adapter);
      final plan = service.preview(
        batchCaller,
        batchParams([approved]),
        (_) {},
      );
      final result = service.apply(plan.planId, batchCaller, (_) {});
      final queueId = result.meta['queueId']! as String;

      final restarted = batchService(root, adapter);
      BackendError? secondError;
      var reentered = false;
      adapter.onInventory = () {
        if (reentered) {
          return;
        }
        reentered = true;
        try {
          restarted.resume(batchCaller, queueId);
        } on BackendError catch (error) {
          secondError = error;
        }
      };
      restarted.resume(batchCaller, queueId);

      expect(reentered, isTrue, reason: 'resume must consult the adapter');
      expect(secondError?.code, 'operation_in_progress');
    });
  });

  group('resume save failure and revalidation cleanup', () {
    test('resume save failure does not publish plan or retain grants', () {
      final root = batchTempDir('save-failure');
      addTearDown(() {
        Process.runSync('chmod', ['755', p.join(root.path, 'print-queues')]);
        root.deleteSync(recursive: true);
      });
      final approved = p.join(root.path, 'approved.pdf');
      File(approved).writeAsBytesSync('approved'.codeUnits);
      final adapter = FakeBatchAdapter()
        ..push(PrintFailure.failed('pause queue'));
      final service = batchService(root, adapter);
      final plan = service.preview(
        batchCaller,
        batchParams([approved]),
        (_) {},
      );
      final result = service.apply(plan.planId, batchCaller, (_) {});
      final queueId = result.meta['queueId']! as String;

      final restarted = batchService(root, adapter);
      final queueRoot = p.join(root.path, 'print-queues');
      Process.runSync('chmod', ['500', queueRoot]);
      expectBackendMessage(
        () => restarted.resume(batchCaller, queueId),
        'print queue could not be saved',
      );
      Process.runSync('chmod', ['755', queueRoot]);
      expect(restarted.activeGrantCountForTesting, 0);
      expect(restarted.activePlanCountForTesting, 0);
      restarted.discard(batchCaller, queueId);
    });

    test('legacy queue without persisted source identity requires explicit reselection', () {
      final root = batchTempDir('legacy-identity');
      addTearDown(() => root.deleteSync(recursive: true));
      final approved = p.join(root.path, 'approved.pdf');
      File(approved).writeAsBytesSync('approved'.codeUnits);
      final adapter = FakeBatchAdapter()
        ..push(PrintFailure.failed('pause queue'));
      final service = batchService(root, adapter);
      final plan = service.preview(
        batchCaller,
        batchParams([approved]),
        (_) {},
      );
      final result = service.apply(plan.planId, batchCaller, (_) {});
      final queueId = result.meta['queueId']! as String;
      final queuePath = p.join(root.path, 'print-queues', '$queueId.json');
      final legacy = jsonDecode(
        File(queuePath).readAsStringSync(),
      ) as Map<String, Object?>;
      legacy.remove('grantRootIdentities');
      legacy.remove('sourceIdentities');
      File(queuePath).writeAsStringSync(jsonEncode(legacy));

      final restarted = batchService(root, adapter);
      expectBackendMessage(
        () => restarted.resume(batchCaller, queueId),
        'error.print.queueSourceReselectionRequired',
      );
      expect(restarted.activeGrantCountForTesting, 0);
      expect(restarted.activePlanCountForTesting, 0);
    });

    test('failed resume revalidation releases provisional grants', () {
      for (final corruptDescriptor in [false, true]) {
        final root = batchTempDir('revalidate-$corruptDescriptor');
        addTearDown(() => root.deleteSync(recursive: true));
        final selectedParent = batchTempDir('selected-$corruptDescriptor');
        addTearDown(() => selectedParent.deleteSync(recursive: true));
        final workbook = p.join(selectedParent.path, 'book.pdf');
        writeWorkbook(workbook, [('MEC', 17, null)]);
        final adapter = FakeBatchAdapter()
          ..push(PrintFailure.failed('pause queue'));
        final service = batchService(root, adapter);
        final plan = service.preview(
          batchCaller,
          batchParams([workbook]),
          (_) {},
        );
        final result = service.apply(plan.planId, batchCaller, (_) {});
        final queueId = result.meta['queueId']! as String;
        final queuePath = p.join(root.path, 'print-queues', '$queueId.json');
        final saved = jsonDecode(
          File(queuePath).readAsStringSync(),
        ) as Map<String, Object?>;
        final job = (saved['jobs']! as List)[0] as Map<String, Object?>;
        final signature = fileSignatureFromFile(
          File(workbook),
          includeSha256: true,
        );
        job['format'] = 'XLSX';
        job['signature'] = {
          'size': signature.size,
          'mtimeNs': signature.mtimeNs,
          'sha256': signature.sha256,
        };
        job['worksheet'] = {
          'workbookIndex': 1,
          'sheetId': 17,
          'relationshipId': 'rId1',
          'relationshipType': 'http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet',
          'partPath': 'xl/worksheets/sheet1.xml',
          'name': corruptDescriptor ? 'Forged' : 'MEC',
          'visibility': 'visible',
        };
        job['workbookSafety'] = {
          'hasVbaProject': false,
          'hasXlmMacroSheet': false,
          'hasWorkbookProtection': false,
          'externalLinkCount': 0,
          'connectionCount': 0,
          'queryTableCount': 0,
        };
        File(queuePath).writeAsStringSync(jsonEncode(saved));
        if (!corruptDescriptor) {
          writeWorkbook(workbook, [('Changed', 99, null)]);
        }
        final restarted = batchService(root, adapter);
        final grantsBefore = restarted.activeGrantCountForTesting;

        expectBackendMessage(
          () => restarted.resume(batchCaller, queueId),
          corruptDescriptor
              ? 'error.print.worksheetChanged'
              : 'error.print.workbookChanged',
        );
        expect(
          restarted.activeGrantCountForTesting,
          grantsBefore,
          reason: 'failed resume retained a capability grant',
        );
      }
    });
  });

  group('regression: preview signature matches apply', () {
    test('an unchanged PDF previewed then applied is submitted', () {
      final root = batchTempDir('pdf-apply');
      addTearDown(() => root.deleteSync(recursive: true));
      final pdf = p.join(root.path, 'approved.pdf');
      File(pdf).writeAsBytesSync('%PDF-1.4 approved'.codeUnits);
      final adapter = FakeBatchAdapter();
      final service = batchService(root, adapter);
      final plan = service.preview(batchCaller, batchParams([pdf]), (_) {});

      final result = service.apply(plan.planId, batchCaller, (_) {});

      expect(
        result.failed,
        0,
        reason: '${result.errors.map((e) => e.key).toList()}',
      );
      expect(adapter.submitted, hasLength(1));
    });
  });
  group('drift, pinned bytes and per-file settings', () {
    Map<String, Object?> queueOf(String path) =>
        jsonDecode(File(path).readAsStringSync()) as Map<String, Object?>;

    List<int> arm(String queuePath) {
      final saved = queueOf(queuePath);
      saved['sessionId'] = 'session';
      saved['status'] = 'running';
      saved['currentJobId'] = ((saved['jobs']! as List)[0] as Map)['jobId'];
      ((saved['jobs']! as List)[0] as Map)['status'] = 'printing';
      File(queuePath).writeAsStringSync(jsonEncode(saved));
      return File(queuePath).readAsBytesSync();
    }

    test('failed resume drift, grant and cache revalidation leave queue bytes unchanged', () {
      final freshSession = CallerIdentity('operator', 'fresh-session');

      final driftRoot = batchTempDir('drift');
      addTearDown(() => driftRoot.deleteSync(recursive: true));
      final driftWorkbook = p.join(driftRoot.path, 'drift.xlsx');
      writeWorkbook(driftWorkbook, [('MEC', 17, null)]);
      final driftAdapter = FakeBatchAdapter();
      final driftService = batchService(driftRoot, driftAdapter);
      final driftPlan = driftService.preview(
        batchCaller,
        batchParams([driftWorkbook]),
        (_) {},
      );
      final driftResult = driftService.apply(
        driftPlan.planId,
        batchCaller,
        (_) {},
      );
      final driftQueueId = driftResult.meta['queueId']! as String;
      final driftQueuePath = p.join(
        driftRoot.path,
        'print-queues',
        '$driftQueueId.json',
      );
      final driftBefore = arm(driftQueuePath);
      writeWorkbook(driftWorkbook, [('Changed', 99, null)]);
      expectBackendMessage(
        () => batchService(
          driftRoot,
          driftAdapter,
        ).resume(freshSession, driftQueueId),
        'error.print.workbookChanged',
      );
      expect(File(driftQueuePath).readAsBytesSync(), driftBefore);

      final cacheRoot = batchTempDir('cache');
      addTearDown(() => cacheRoot.deleteSync(recursive: true));
      final cacheWorkbook = p.join(cacheRoot.path, 'cache.xlsx');
      writeWorkbook(cacheWorkbook, [('MEC', 17, null)]);
      final cacheAdapter = FakeBatchAdapter();
      final cacheService = batchService(cacheRoot, cacheAdapter);
      final cachePlan = cacheService.preview(
        batchCaller,
        batchParams([cacheWorkbook]),
        (_) {},
      );
      final cacheResult = cacheService.apply(
        cachePlan.planId,
        batchCaller,
        (_) {},
      );
      final cacheQueueId = cacheResult.meta['queueId']! as String;
      final cacheQueuePath = p.join(
        cacheRoot.path,
        'print-queues',
        '$cacheQueueId.json',
      );
      arm(cacheQueuePath);
      final cacheSaved = queueOf(cacheQueuePath);
      (((cacheSaved['jobs']! as List)[0] as Map)['worksheet'] as Map)['name'] =
          'Forged';
      File(cacheQueuePath).writeAsStringSync(jsonEncode(cacheSaved));
      final cacheBefore = File(cacheQueuePath).readAsBytesSync();
      expectBackendMessage(
        () => batchService(
          cacheRoot,
          cacheAdapter,
        ).resume(freshSession, cacheQueueId),
        'error.print.worksheetChanged',
      );
      expect(File(cacheQueuePath).readAsBytesSync(), cacheBefore);

      final grantRoot = batchTempDir('grant');
      addTearDown(() => grantRoot.deleteSync(recursive: true));
      final selectedParent = Directory(p.join(grantRoot.path, 'selected'))
        ..createSync();
      final grantedFile = p.join(selectedParent.path, 'one.pdf');
      File(grantedFile).writeAsBytesSync('one'.codeUnits);
      final grantAdapter = FakeBatchAdapter()
        ..push(PrintFailure.failed('pause queue'));
      final grantService = batchService(grantRoot, grantAdapter);
      final grantPlan = grantService.preview(
        batchCaller,
        batchParams([grantedFile]),
        (_) {},
      );
      final grantResult = grantService.apply(
        grantPlan.planId,
        batchCaller,
        (_) {},
      );
      grantAdapter.submitted.clear();
      final grantQueueId = grantResult.meta['queueId']! as String;
      final grantQueuePath = p.join(
        grantRoot.path,
        'print-queues',
        '$grantQueueId.json',
      );
      final grantBefore = arm(grantQueuePath);
      selectedParent.renameSync(p.join(grantRoot.path, 'pinned-selected'));
      Directory(selectedParent.path).createSync();
      File(grantedFile).writeAsBytesSync('one'.codeUnits);
      expectBackendMessage(
        () => batchService(
          grantRoot,
          grantAdapter,
        ).resume(freshSession, grantQueueId),
        'error.print.queueSourceReselectionRequired',
      );
      expect(File(grantQueuePath).readAsBytesSync(), grantBefore);
      expect(grantAdapter.submitted, isEmpty);
    });

    test(
      'apply prints the pinned file bytes after the visible parent is swapped',
      () {
        final root = batchTempDir('pinned-bytes');
        addTearDown(() => root.deleteSync(recursive: true));
        final visibleParent = Directory(p.join(root.path, 'approved'))
          ..createSync();
        final pinnedParent = p.join(root.path, 'pinned-approved');
        final outsideParent = Directory(p.join(root.path, 'outside'))
          ..createSync();
        final approved = p.join(visibleParent.path, 'approved.pdf');
        File(approved).writeAsBytesSync('ORIGINAL APPROVED PDF'.codeUnits);
        File(p.join(outsideParent.path, 'approved.pdf'))
            .writeAsBytesSync('OUTSIDE REPLACEMENT PDF'.codeUnits);
        final adapter = FakeBatchAdapter();
        var swapArmed = false;
        adapter.onInspect = (_) {
          if (swapArmed) {
            swapArmed = false;
            visibleParent.renameSync(pinnedParent);
            Link(visibleParent.path).createSync(outsideParent.path);
          }
        };
        final service = batchService(root, adapter);
        final plan = service.preview(
          batchCaller,
          batchParams([approved]),
          (_) {},
        );
        swapArmed = true;

        final result = service.apply(plan.planId, batchCaller, (_) {});

        expect(
          result.applied,
          1,
          reason: '${result.errors.map((e) => e.key).toList()}',
        );
        expect(result.failed, 0);
        expect(
          adapter.submittedBytes.single,
          'ORIGINAL APPROVED PDF'.codeUnits,
        );
        expect(adapter.submitted.single.path, isNot(approved));
        expect(p.extension(adapter.submitted.single.path), '.pdf');
        expect(
          File(p.join(outsideParent.path, 'approved.pdf')).readAsStringSync(),
          'OUTSIDE REPLACEMENT PDF',
        );
        expect(
          File(p.join(pinnedParent, 'approved.pdf')).readAsStringSync(),
          'ORIGINAL APPROVED PDF',
        );
      },
    );

    test('preview preserves order, deduplicates, collates and maps per-file settings', () {
      final root = batchTempDir('order');
      addTearDown(() => root.deleteSync(recursive: true));
      final first = p.join(root.path, 'first.pdf');
      final second = p.join(root.path, 'second.docx');
      File(first).writeAsBytesSync('first'.codeUnits);
      File(second).writeAsBytesSync('second'.codeUnits);
      final service = batchService(
        root,
        FakeBatchAdapter(printers: ['Office Printer', 'Backup Printer']),
      );
      final values = BatchPrintParams(
        files: [first, second, first],
        printer: 'Office Printer',
        copies: 2,
        timeoutMinutes: 30,
      );

      final plan = service.preview(batchCaller, values, (_) {});

      expect([for (final step in plan.steps) step.src], [first, second]);
      final jobs = plan.meta['jobs']! as List;
      expect(
        [for (final job in jobs) (job as Map)['path']],
        [first, second, first, second],
      );
      expect(plan.meta['fileCount'], 2);
      expect(plan.meta['jobCount'], 4);
      expect(
        plan.warnings.any((w) => w.key == 'warn.print.settingsLimited'),
        isTrue,
      );
    });

    test('revise preserves row identity and applies per-file selection and settings', () {
      final root = batchTempDir('revise-identity');
      addTearDown(() => root.deleteSync(recursive: true));
      final first = p.join(root.path, 'first.pdf');
      final second = p.join(root.path, 'second.pdf');
      File(first).writeAsBytesSync('first'.codeUnits);
      File(second).writeAsBytesSync('second'.codeUnits);
      final service = batchService(
        root,
        FakeBatchAdapter(printers: ['Office Printer', 'Backup Printer']),
      );
      final plan = service.preview(
        batchCaller,
        BatchPrintParams(
          files: [first, second],
          printer: 'Office Printer',
          copies: 2,
          timeoutMinutes: 30,
        ),
        (_) {},
      );
      final rows = editorRows(plan);
      rows[0]['enabled'] = false;
      rows[1]['printer'] = 'Backup Printer';
      rows[1]['paper'] = 'A3';
      rows[1]['orientation'] = 'landscape';
      rows[1]['color'] = 'grayscale';

      final revised = service.revise(batchCaller, plan.planId, rows);

      expect(revised.steps, hasLength(1));
      expect(revised.steps[0].src, second);
      final revisedJobs = revised.meta['jobs']! as List;
      expect(revisedJobs, hasLength(2));
      for (final job in revisedJobs.cast<Map>()) {
        expect(job['printer'], 'Backup Printer');
        expect(job['paper'], 'A3');
        expect(job['orientation'], 'landscape');
        expect(job['color'], 'grayscale');
      }
    });
  });
  group('page ranges', () {
    test('preview defaults page range to all', () {
      final root = batchTempDir('range-default');
      addTearDown(() => root.deleteSync(recursive: true));
      final file = p.join(root.path, 'one.pdf');
      File(file).writeAsBytesSync('one'.codeUnits);
      final adapter = FakeBatchAdapter()..pdfPageCount = 3;
      final service = batchService(root, adapter);

      final plan = service.preview(batchCaller, batchParams([file]), (_) {});

      final row = editorRows(plan)[0];
      expect(row['pageRange'], 'all');
      expect((plan.meta['jobs']! as List)[0]['pageRange'], 'all');
      expect(plan.steps[0].meta['pageRange'], 'all');
      final columns = (plan.meta['editor']! as Map)['columns']! as List;
      expect(
        columns.cast<Map>().any(
          (c) => c['key'] == 'pageRange' && c['kind'] == 'text',
        ),
        isTrue,
      );
    });

    test('revise canonicalizes per-file page ranges', () {
      final root = batchTempDir('range-canon');
      addTearDown(() => root.deleteSync(recursive: true));
      final first = p.join(root.path, 'first.pdf');
      final second = p.join(root.path, 'second.pdf');
      File(first).writeAsBytesSync('first'.codeUnits);
      File(second).writeAsBytesSync('second'.codeUnits);
      final adapter = FakeBatchAdapter()..pdfPageCount = 10;
      final service = batchService(root, adapter);
      final plan = service.preview(
        batchCaller,
        batchParams([first, second]),
        (_) {},
      );
      final rows = editorRows(plan);
      rows[0]['pageRange'] = ' 3, 1-2,2 ';
      rows[1]['pageRange'] = '7, 5-6';

      final revised = service.revise(batchCaller, plan.planId, rows);

      expect(
        [for (final step in revised.steps) (step.src, step.meta['pageRange'])],
        [(first, '1-3'), (second, '5-7')],
      );
      final revisedRows = editorRows(revised);
      expect(revisedRows[0]['pageRange'], '1-3');
      expect(revisedRows[1]['pageRange'], '5-7');
      final jobs = revised.meta['jobs']! as List;
      expect((jobs[0] as Map)['pageRange'], '1-3');
      expect((jobs[1] as Map)['pageRange'], '5-7');
    });

    test('out-of-bounds PDF revision leaves the prior plan usable', () {
      final root = batchTempDir('range-bounds');
      addTearDown(() => root.deleteSync(recursive: true));
      final file = p.join(root.path, 'one.pdf');
      File(file).writeAsBytesSync('one'.codeUnits);
      final adapter = FakeBatchAdapter()..pdfPageCount = 2;
      final service = batchService(root, adapter);
      final plan = service.preview(batchCaller, batchParams([file]), (_) {});
      final rows = editorRows(plan);
      rows[0]['pageRange'] = '3';

      expectBackendMessage(
        () => service.revise(batchCaller, plan.planId, rows),
        'error.print.pageRangeBounds',
      );
      expect(service.apply(plan.planId, batchCaller, (_) {}).applied, 1);
      expect(adapter.submitted[0].pageRange, 'all');
    });

    test(
      'approved PDF range is rechecked against page count before submission',
      () {
        final root = batchTempDir('range-recheck');
        addTearDown(() => root.deleteSync(recursive: true));
        final file = p.join(root.path, 'one.pdf');
        File(file).writeAsBytesSync('one'.codeUnits);
        final adapter = FakeBatchAdapter()..pdfPageCount = 3;
        final service = batchService(root, adapter);
        final plan = service.preview(batchCaller, batchParams([file]), (_) {});
        final rows = editorRows(plan);
        rows[0]['pageRange'] = '3';
        final revised = service.revise(batchCaller, plan.planId, rows);
        adapter.pdfPageCount = 2;

        final result = service.apply(revised.planId, batchCaller, (_) {});

        expect(result.applied, 0);
        expect(result.failed, 1);
        expect(adapter.submitted, isEmpty);
      },
    );

    test('image page one canonicalizes to all and other pages fail', () {
      final root = batchTempDir('range-image');
      addTearDown(() => root.deleteSync(recursive: true));
      final file = p.join(root.path, 'one.png');
      File(file).writeAsBytesSync('one'.codeUnits);
      final adapter = FakeBatchAdapter();
      final service = batchService(root, adapter);
      final plan = service.preview(batchCaller, batchParams([file]), (_) {});
      final rows = editorRows(plan);
      rows[0]['pageRange'] = '1';

      final revised = service.revise(batchCaller, plan.planId, rows);

      expect(editorRows(revised)[0]['pageRange'], 'all');
      expect((revised.meta['jobs']! as List)[0]['pageRange'], 'all');
      final invalid = editorRows(revised);
      invalid[0]['pageRange'] = '2';
      expectBackendMessage(
        () => service.revise(batchCaller, revised.planId, invalid),
        'error.print.pageRangeImageUnsupported',
      );
      expect(adapter.submitted, isEmpty);
    });

    test('native application numeric page range fails before apply', () {
      final root = batchTempDir('range-native');
      addTearDown(() => root.deleteSync(recursive: true));
      final file = p.join(root.path, 'one.docx');
      File(file).writeAsBytesSync('one'.codeUnits);
      final adapter = FakeBatchAdapter();
      final service = batchService(root, adapter);
      final plan = service.preview(batchCaller, batchParams([file]), (_) {});
      final rows = editorRows(plan);
      rows[0]['pageRange'] = '1';

      expectBackendMessage(
        () => service.revise(batchCaller, plan.planId, rows),
        'error.print.pageRangeNativeUnsupported',
      );
      expect(adapter.submitted, isEmpty);
    });
  });
  group('canonical page ranges and persistence', () {
    test('canonical page ranges persist through copies, retries, queue, journal and resume', () {
      final root = batchTempDir('range-persist');
      addTearDown(() => root.deleteSync(recursive: true));
      final first = p.join(root.path, 'first.pdf');
      final second = p.join(root.path, 'second.pdf');
      File(first).writeAsBytesSync('first'.codeUnits);
      File(second).writeAsBytesSync('second'.codeUnits);
      final adapter = FakeBatchAdapter()
        ..pdfPageCount = 10
        ..push(null)
        ..push(PrintFailure.failed('first failure'))
        ..push(PrintFailure.failed('retry failure'));
      final service = batchService(root, adapter);
      final plan = service.preview(
        batchCaller,
        BatchPrintParams(
          files: [first, second],
          printer: 'Office Printer',
          copies: 2,
          retries: 1,
          timeoutMinutes: 30,
        ),
        (_) {},
      );
      final rows = editorRows(plan);
      rows[0]['pageRange'] = '3,1-2';
      rows[1]['pageRange'] = '5';
      final revised = service.revise(batchCaller, plan.planId, rows);

      expect(revised.steps[0].meta['pageRange'], '1-3');
      expect(revised.steps[1].meta['pageRange'], '5');
      final jobs = revised.meta['jobs']! as List;
      expect(
        [for (final job in jobs) (job as Map)['pageRange']],
        ['1-3', '5', '1-3', '5'],
      );

      final result = service.apply(revised.planId, batchCaller, (_) {});
      final queueId = result.meta['queueId']! as String;
      expect(
        [for (final job in adapter.submitted) job.pageRange],
        ['1-3', '5', '5'],
      );
      expect(
        adapter.inspectedSettings.any((s) => s.pageRange == '1-3'),
        isTrue,
      );
      final saved = jsonDecode(
        File(p.join(root.path, 'print-queues', '$queueId.json'))
            .readAsStringSync(),
      ) as Map<String, Object?>;
      expect(
        [for (final job in saved['jobs']! as List) (job as Map)['pageRange']],
        ['1-3', '5', '1-3', '5'],
      );
      final journalFile = Directory(p.join(root.path, 'journal'))
          .listSync()
          .whereType<File>()
          .firstWhere((f) => f.path.endsWith('.json'));
      final journal =
          jsonDecode(journalFile.readAsStringSync()) as Map<String, Object?>;
      expect(
        ((((journal['steps']! as List)[0]) as Map)['meta'] as Map)['pageRange'],
        '1-3',
      );

      final restarted = batchService(root, adapter);
      final resumed = restarted.resume(batchCaller, queueId);
      expect(
        [for (final row in editorRows(resumed)) row['pageRange']],
        ['1-3', '5', '1-3', '5'],
      );
    });

    test('legacy queue without page range resumes as all', () {
      final root = batchTempDir('range-legacy');
      addTearDown(() => root.deleteSync(recursive: true));
      final file = p.join(root.path, 'one.pdf');
      File(file).writeAsBytesSync('one'.codeUnits);
      final adapter = FakeBatchAdapter()
        ..push(PrintFailure.failed('pause queue'))
        ..pdfPageCount = 3;
      final service = batchService(root, adapter);
      final plan = service.preview(batchCaller, batchParams([file]), (_) {});
      final result = service.apply(plan.planId, batchCaller, (_) {});
      final queueId = result.meta['queueId']! as String;
      final queuePath = p.join(root.path, 'print-queues', '$queueId.json');
      final saved = jsonDecode(
        File(queuePath).readAsStringSync(),
      ) as Map<String, Object?>;
      ((saved['jobs']! as List)[0] as Map).remove('pageRange');
      File(queuePath).writeAsStringSync(jsonEncode(saved));

      final restarted = batchService(
        root,
        FakeBatchAdapter()..pdfPageCount = 3,
      );
      final resumed = restarted.resume(batchCaller, queueId);

      expect(editorRows(resumed)[0]['pageRange'], 'all');
      expect(((resumed.meta['jobs']! as List)[0] as Map)['pageRange'], 'all');
    });

    test(
      'owned persisted queue with empty page range is rejected on resume',
      () {
        final root = batchTempDir('range-empty');
        addTearDown(() => root.deleteSync(recursive: true));
        final file = p.join(root.path, 'one.pdf');
        File(file).writeAsBytesSync('one'.codeUnits);
        const queueId = '44444444-4444-4444-8444-444444444444';
        final queueRoot = Directory(p.join(root.path, 'print-queues'))
          ..createSync(recursive: true);
        File(p.join(queueRoot.path, '$queueId.json')).writeAsStringSync(
          jsonEncode({
            'version': 1,
            'queueId': queueId,
            'ownerId': 'operator',
            'sessionId': 'session',
            'createdAt': '2026-08-25T00:00:00+00:00',
            'updatedAt': '2026-08-25T00:00:01+00:00',
            'status': 'paused',
            'currentJobId': null,
            'jobs': [
              {
                'jobId': 'file-1-c1',
                'path': file,
                'printer': 'Office Printer',
                'verb': 'docflow',
                'format': 'PDF',
                'pageRange': '',
                'signature': <String, Object?>{},
                'status': 'pending',
                'attempts': 0,
                'error': '',
              },
            ],
          }),
        );
        final adapter = FakeBatchAdapter()..pdfPageCount = 3;
        final service = batchService(root, adapter);

        expectBackendMessage(
          () => service.resume(batchCaller, queueId),
          'error.print.pageRangeInvalid',
        );
      },
    );
  });
  group('forged jobs and queue journaling', () {
    test(
      'forged job with invalid page range fails validation without submission',
      () {
        final job = PrintJob(
          jobId: 'forged-c1',
          path: '/approved/forged.pdf',
          printer: 'Office Printer',
          verb: 'docflow',
          format: 'PDF',
          pageRange: '0',
        );
        expectBackendMessage(
          () => validateJob(job),
          'error.print.pageRangeInvalid',
        );
      },
    );

    test('queue is persisted and journaled before failure, then resumes and discards', () {
      final root = batchTempDir('queue-journal');
      addTearDown(() => root.deleteSync(recursive: true));
      final files = [
        for (final n in ['one.pdf', 'two.pdf', 'three.pdf'])
          p.join(root.path, n),
      ];
      for (final file in files) {
        File(file).writeAsBytesSync(p.basename(file).codeUnits);
      }
      final adapter = FakeBatchAdapter()
        ..push(null)
        ..push(PrintFailure.failed('paper jam\nprivate path omitted'));
      final service = batchService(root, adapter);
      final plan = service.preview(batchCaller, batchParams(files), (_) {});

      final result = service.apply(plan.planId, batchCaller, (_) {});

      expect(result.applied, 1);
      expect(result.failed, 1);
      expect(result.journalId, isEmpty, reason: 'printing is not undoable');
      expect(result.meta['nonUndoable'], isTrue);
      expect(result.meta['resumeAvailable'], isTrue);
      final queueId = result.meta['queueId']! as String;
      final saved = jsonDecode(
        File(p.join(root.path, 'print-queues', '$queueId.json'))
            .readAsStringSync(),
      ) as Map<String, Object?>;
      expect(saved['version'], 1);
      expect(saved['ownerId'], 'operator');
      expect(saved['sessionId'], 'session');
      expect(saved['status'], 'paused');
      expect(saved['currentJobId'], isNull);
      final jobs = saved['jobs']! as List;
      expect(
        [for (final job in jobs) (job as Map)['status']],
        ['completed', 'failed', 'pending'],
      );
      expect(((jobs[1] as Map)['error']! as String).contains('\n'), isFalse);
      final journalFiles = Directory(p.join(root.path, 'journal'))
          .listSync()
          .whereType<File>()
          .where((f) => f.path.endsWith('.json'))
          .toList();
      expect(journalFiles, hasLength(1));
      final journal = jsonDecode(
        journalFiles.single.readAsStringSync(),
      ) as Map<String, Object?>;
      expect(journal['opId'], 'files.batch_print');
      final steps = journal['steps']! as List;
      expect(steps, hasLength(1));
      expect((steps[0] as Map)['kind'], 'print');
      expect(journal.containsKey('pendingStep'), isFalse);

      final restarted = batchService(root, adapter);
      final resumed = restarted.resume(batchCaller, queueId);
      expect(
        [for (final row in editorRows(resumed)) row['enabled']],
        [false, true, true],
      );
      expect((resumed.meta['jobs']! as List), hasLength(2));
      final discarded = restarted.discard(batchCaller, queueId);
      expect(discarded.resumable, 0);
      expect(restarted.latest(batchCaller), isNull);
    });
  });
  group('partial resume and owner adoption', () {
    Map<String, Object?> readQueue(String root, String queueId) => jsonDecode(
      File(p.join(root, 'print-queues', '$queueId.json')).readAsStringSync(),
    ) as Map<String, Object?>;

    test('partial resume rebuilds the complete displayed order for legacy and worksheet queues', () {
      final legacyRoot = batchTempDir('partial-legacy');
      addTearDown(() => legacyRoot.deleteSync(recursive: true));
      final legacyFiles = [
        for (final name in ['one.pdf', 'two.pdf', 'three.pdf'])
          p.join(legacyRoot.path, name),
      ];
      for (final path in legacyFiles) {
        File(path).writeAsBytesSync(p.basename(path).codeUnits);
      }
      final legacyAdapter = FakeBatchAdapter()
        ..push(null)
        ..push(PrintFailure.failed('pause queue'));
      final legacyService = batchService(legacyRoot, legacyAdapter);
      final legacyPlan = legacyService.preview(
        batchCaller,
        batchParams(legacyFiles),
        (_) {},
      );
      final legacyResult = legacyService.apply(
        legacyPlan.planId,
        batchCaller,
        (_) {},
      );
      final legacyQueueId = legacyResult.meta['queueId']! as String;
      final legacyRestarted = batchService(legacyRoot, legacyAdapter);
      final legacyResumed = legacyRestarted.resume(batchCaller, legacyQueueId);
      final legacyRows = editorRows(legacyResumed);
      expect(
        [for (final row in legacyRows) row['status']],
        ['completed', 'failed', 'pending'],
      );
      final legacyRevised = legacyRestarted.revise(
        batchCaller,
        legacyResumed.planId,
        [legacyRows[2], legacyRows[0], legacyRows[1]],
      );
      legacyRestarted.apply(legacyRevised.planId, batchCaller, (_) {});

      final legacySaved = readQueue(legacyRoot.path, legacyQueueId);
      final legacyJobs = (legacySaved['jobs']! as List).cast<Map>();
      expect(
        [for (final job in legacyJobs) p.basename(job['path'] as String)],
        [
          p.basename(legacyFiles[2]),
          p.basename(legacyFiles[0]),
          p.basename(legacyFiles[1]),
        ],
      );
      expect(
        [for (final job in legacyJobs) job['status']],
        ['completed', 'completed', 'completed'],
      );
      expect([for (final job in legacyJobs) job['attempts']], [1, 1, 2]);

      final worksheetRoot = batchTempDir('partial-worksheet');
      addTearDown(() => worksheetRoot.deleteSync(recursive: true));
      final workbook = p.join(worksheetRoot.path, 'book.xlsx');
      writeWorkbook(workbook, [
        ('MEC', 17, null),
        ('CIV', 18, null),
        ('ELE', 19, null),
      ]);
      final worksheetAdapter = FakeBatchAdapter();
      final worksheetService = batchService(worksheetRoot, worksheetAdapter);
      final worksheetPlan = worksheetService.preview(
        batchCaller,
        batchParams([workbook]),
        (_) {},
      );
      final worksheetResult = worksheetService.apply(
        worksheetPlan.planId,
        batchCaller,
        (_) {},
      );
      final worksheetQueueId = worksheetResult.meta['queueId']! as String;
      final worksheetQueuePath = p.join(
        worksheetRoot.path,
        'print-queues',
        '$worksheetQueueId.json',
      );
      final worksheetSaved = readQueue(worksheetRoot.path, worksheetQueueId);
      final statuses = [
        ['completed', 1],
        ['failed', 2],
        ['pending', 0],
      ];
      final savedJobs = (worksheetSaved['jobs']! as List)
          .cast<Map<String, Object?>>();
      for (var i = 0; i < savedJobs.length; i++) {
        savedJobs[i]['status'] = statuses[i][0];
        savedJobs[i]['attempts'] = statuses[i][1];
      }
      File(worksheetQueuePath).writeAsStringSync(jsonEncode(worksheetSaved));

      final worksheetRestarted = batchService(worksheetRoot, worksheetAdapter);
      final worksheetResumed = worksheetRestarted.resume(
        batchCaller,
        worksheetQueueId,
      );
      final worksheetRows = editorRows(worksheetResumed);
      final worksheetRevised = worksheetRestarted.revise(
        batchCaller,
        worksheetResumed.planId,
        [worksheetRows[2], worksheetRows[0], worksheetRows[1]],
      );
      worksheetRestarted.apply(worksheetRevised.planId, batchCaller, (_) {});

      final afterWorksheet = readQueue(worksheetRoot.path, worksheetQueueId);
      final worksheetJobs = (afterWorksheet['jobs']! as List).cast<Map>();
      expect(
        [for (final job in worksheetJobs) (job['worksheet'] as Map)['name']],
        ['ELE', 'MEC', 'CIV'],
      );
      expect(
        [for (final job in worksheetJobs) job['status']],
        ['failed', 'completed', 'pending'],
      );
      expect([for (final job in worksheetJobs) job['attempts']], [1, 1, 2]);
      expect(worksheetAdapter.submitted, isEmpty);
    });

    test(
      'persisted queues are adopted only by the exact owner after restart',
      () {
        final root = batchTempDir('adopt');
        addTearDown(() => root.deleteSync(recursive: true));
        final file = p.join(root.path, 'one.pdf');
        File(file).writeAsBytesSync('one'.codeUnits);
        final adapter = FakeBatchAdapter()
          ..push(PrintFailure.failed('pause queue'));
        final service = batchService(root, adapter);
        final original = CallerIdentity('operator-a', 'session-a');
        final plan = service.preview(original, batchParams([file]), (_) {});
        final result = service.apply(plan.planId, original, (_) {});
        final queueId = result.meta['queueId']! as String;
        final restarted = batchService(root, adapter);
        final otherOwner = CallerIdentity('operator-b', 'session-a');
        final freshSession = CallerIdentity('operator-a', 'session-b');

        expect(restarted.latest(otherOwner), isNull);
        expect(
          () => restarted.resume(otherOwner, queueId),
          throwsA(
            isA<BackendError>().having(
              (e) => e.code,
              'code',
              'plan_not_found_or_expired',
            ),
          ),
        );
        expect(
          () => restarted.discard(otherOwner, queueId),
          throwsA(
            isA<BackendError>().having(
              (e) => e.code,
              'code',
              'plan_not_found_or_expired',
            ),
          ),
        );
        expect(restarted.latest(freshSession)!.queueId, queueId);
        expect(readQueue(root.path, queueId)['ownerId'], 'operator-a');
        expect(readQueue(root.path, queueId)['sessionId'], 'session-a');
        expect(
          restarted.resume(freshSession, queueId).opId,
          'files.batch_print',
        );
        expect(readQueue(root.path, queueId)['sessionId'], 'session-b');
        expect(restarted.discard(freshSession, queueId).status, 'abandoned');
        expect(restarted.latest(original), isNull);
      },
    );
  });
  group('retry, uncertainty and unconfirmed submission', () {
    test('retry, timeout and skip semantics produce partial results without reordering', () {
      final root = batchTempDir('retry-skip');
      addTearDown(() => root.deleteSync(recursive: true));
      final first = p.join(root.path, 'one.pdf');
      final second = p.join(root.path, 'two.pdf');
      File(first).writeAsBytesSync('one'.codeUnits);
      File(second).writeAsBytesSync('two'.codeUnits);
      final adapter = FakeBatchAdapter()
        ..push(PrintFailure.failed('temporary spool failure'))
        ..push(PrintFailure.failed('retry exhausted'))
        ..push(null);
      final service = batchService(root, adapter);
      final plan = service.preview(
        batchCaller,
        BatchPrintParams(
          files: [first, second],
          printer: 'Office Printer',
          retries: 1,
          onError: 'skip',
          timeoutMinutes: 7,
        ),
        (_) {},
      );

      final result = service.apply(plan.planId, batchCaller, (_) {});

      expect(result.applied, 1);
      expect(result.failed, 1);
      expect(
        [
          for (final job in adapter.submitted)
            (p.basename(job.path), job.timeoutMinutes),
        ],
        [
          (p.basename(first), 7),
          (p.basename(first), 7),
          (p.basename(second), 7),
        ],
      );
      expect(result.meta['status'], 'completed_with_errors');
      expect((result.meta['counts']! as Map)['failed'], 1);
      expect((result.meta['counts']! as Map)['completed'], 1);
    });

    test('uncertain and cancelled jobs are never retried automatically', () {
      final cases = <(PrintFailure, String, String)>[
        (
          PrintFailure.uncertain('spool outcome unknown'),
          'uncertain',
          'warn.print.stateUncertain',
        ),
        (
          PrintFailure.cancelled('cancelled while active', uncertain: false),
          'cancelled',
          'warn.print.cancelled',
        ),
        (
          PrintFailure.cancelled(
            'cancelled before acknowledgement',
            uncertain: true,
          ),
          'uncertain',
          'warn.print.cancelledUncertain',
        ),
      ];
      for (final (failure, expectedStatus, expectedWarning) in cases) {
        final root = batchTempDir('uncertain-$expectedStatus');
        addTearDown(() => root.deleteSync(recursive: true));
        final file = p.join(root.path, 'one.pdf');
        File(file).writeAsBytesSync('one'.codeUnits);
        final adapter = FakeBatchAdapter()
          ..push(failure)
          ..push(null);
        final service = batchService(root, adapter);
        final plan = service.preview(
          batchCaller,
          BatchPrintParams(
            files: [file],
            printer: 'Office Printer',
            retries: 3,
            timeoutMinutes: 30,
          ),
          (_) {},
        );

        final result = service.apply(plan.planId, batchCaller, (_) {});

        expect(adapter.submitted, hasLength(1));
        expect((result.meta['counts']! as Map)[expectedStatus], 1);
        expect(result.errors[0].key, expectedWarning);
      }
    });

    test(
      'submitted unconfirmed job is not retried and the queue continues',
      () {
        final root = batchTempDir('unconfirmed');
        addTearDown(() => root.deleteSync(recursive: true));
        final first = p.join(root.path, 'one.pdf');
        final second = p.join(root.path, 'two.pdf');
        File(first).writeAsBytesSync('one'.codeUnits);
        File(second).writeAsBytesSync('two'.codeUnits);
        final adapter = FakeBatchAdapter()
          ..push(
            PrintFailure.submittedUnconfirmed(
              'Windows released the receipt after accepting the job',
            ),
          )
          ..push(null);
        final service = batchService(root, adapter);
        final plan = service.preview(
          batchCaller,
          BatchPrintParams(
            files: [first, second],
            printer: 'Office Printer',
            retries: 3,
            onError: 'pause',
            timeoutMinutes: 30,
          ),
          (_) {},
        );

        final result = service.apply(plan.planId, batchCaller, (_) {});

        expect(adapter.submitted, hasLength(2));
        expect(result.applied, 2);
        expect(result.failed, 0);
        expect(result.meta['status'], 'completed');
        expect(result.meta['resumable'], 0);
        expect((result.meta['counts']! as Map)['completed_unconfirmed'], 1);
        expect((result.meta['counts']! as Map)['completed'], 1);
        expect(result.errors, hasLength(1));
        expect(result.errors[0].key, 'warn.print.submittedUnconfirmed');

        final journalFile = Directory(p.join(root.path, 'journal'))
            .listSync()
            .whereType<File>()
            .firstWhere((f) => f.path.endsWith('.json'));
        final journal =
            jsonDecode(journalFile.readAsStringSync()) as Map<String, Object?>;
        expect(
          ((((journal['steps']! as List)[0]) as Map)['meta'] as Map)['outcome'],
          'submitted_unconfirmed',
        );
        expect(journal['state'], 'committed');
      },
    );
  });
  group('restart recovery, unowned queues and rejection', () {
    test('restart recovery marks an active job uncertain and requires explicit selection', () {
      final root = batchTempDir('restart-recovery');
      addTearDown(() => root.deleteSync(recursive: true));
      final file = p.join(root.path, 'one.pdf');
      File(file).writeAsBytesSync('one'.codeUnits);
      final adapter = FakeBatchAdapter()
        ..push(PrintFailure.failed('pause queue'));
      final service = batchService(root, adapter);
      final plan = service.preview(batchCaller, batchParams([file]), (_) {});
      final result = service.apply(plan.planId, batchCaller, (_) {});
      final queueId = result.meta['queueId']! as String;
      adapter.submitted.clear();
      final queuePath = p.join(root.path, 'print-queues', '$queueId.json');
      final saved = jsonDecode(
        File(queuePath).readAsStringSync(),
      ) as Map<String, Object?>;
      final job = (saved['jobs']! as List)[0] as Map<String, Object?>;
      saved['status'] = 'running';
      saved['currentJobId'] = job['jobId'];
      job['status'] = 'printing';
      job['attempts'] = 1;
      job['error'] = '';
      File(queuePath).writeAsStringSync(jsonEncode(saved));
      final restarted = batchService(root, adapter);

      final summary = restarted.latest(batchCaller)!;
      expect(summary.status, 'paused');
      expect(summary.counts['uncertain'], 1);
      final beforeResume = jsonDecode(
        File(queuePath).readAsStringSync(),
      ) as Map<String, Object?>;
      expect(beforeResume['status'], 'running');
      expect(((beforeResume['jobs']! as List)[0] as Map)['status'], 'printing');
      final resumed = restarted.resume(batchCaller, queueId);
      final rows = editorRows(resumed);
      expect(rows[0]['status'], 'uncertain');
      expect(rows[0]['enabled'], isFalse);
      expect(resumed.meta['jobs'] as List, isEmpty);
      rows[0]['enabled'] = true;
      final approved = restarted.revise(batchCaller, resumed.planId, rows);
      final applied = restarted.apply(approved.planId, batchCaller, (_) {});
      expect(applied.applied, 1);
      expect(applied.failed, 0);
      expect((applied.meta['counts']! as Map)['completed'], 1);
    });

    test('legacy unowned queues remain untrusted and unavailable', () {
      final root = batchTempDir('legacy-unowned');
      addTearDown(() => root.deleteSync(recursive: true));
      const queueId = '22222222-2222-4222-8222-222222222222';
      final queueRoot = Directory(p.join(root.path, 'print-queues'))
        ..createSync(recursive: true);
      final file = p.join(root.path, 'one.pdf');
      File(file).writeAsBytesSync('one'.codeUnits);
      File(p.join(queueRoot.path, '$queueId.json')).writeAsStringSync(
        jsonEncode({
          'version': 1,
          'queueId': queueId,
          'createdAt': '2026-08-25T00:00:00+00:00',
          'updatedAt': '2026-08-25T00:00:01+00:00',
          'status': 'paused',
          'currentJobId': null,
          'jobs': [
            {
              'jobId': 'file-1-c1',
              'path': file,
              'printer': 'Office Printer',
              'verb': 'docflow',
              'format': 'PDF',
              'signature': <String, Object?>{},
              'status': 'pending',
              'attempts': 0,
              'error': '',
            },
          ],
        }),
      );
      final service = batchService(root, FakeBatchAdapter());

      expect(service.latest(batchCaller), isNull);
      expect(
        () => service.resume(batchCaller, queueId),
        throwsA(
          isA<BackendError>().having(
            (e) => e.code,
            'code',
            'plan_not_found_or_expired',
          ),
        ),
      );
      expect(
        () => service.discard(batchCaller, queueId),
        throwsA(
          isA<BackendError>().having(
            (e) => e.code,
            'code',
            'plan_not_found_or_expired',
          ),
        ),
      );
    });

    test('unknown printers and unsupported options fail before submit', () {
      final root = batchTempDir('printer-validation');
      addTearDown(() => root.deleteSync(recursive: true));
      final file = p.join(root.path, 'one.pdf');
      File(file).writeAsBytesSync('one'.codeUnits);
      final adapter = FakeBatchAdapter();
      final service = batchService(root, adapter);
      final malicious = BatchPrintParams(
        files: [file],
        printer: 'Office Printer"; Remove-Item C:\\\\*',
        timeoutMinutes: 30,
      );

      final plan = service.preview(batchCaller, malicious, (_) {});

      expect(plan.steps, isEmpty);
      expect(plan.warnings[0].key, 'warn.print.printerUnavailable');
      expect(adapter.inspected, 0);
      expect(adapter.submitted, isEmpty);

      adapter.unsupportedPaper = 'A3';
      final unsupported = BatchPrintParams(
        files: [file],
        printer: 'Office Printer',
        paper: 'A3',
        timeoutMinutes: 30,
      );
      final unsupportedPlan = service.preview(batchCaller, unsupported, (_) {});
      expect(unsupportedPlan.steps, isEmpty);
      expect(unsupportedPlan.warnings[0].key, 'warn.print.unsupportedOptions');
      expect(adapter.submitted, isEmpty);
    });

    test(
      'active queue is not recovered or discarded while submission is running',
      () {
        final root = batchTempDir('active-queue');
        addTearDown(() => root.deleteSync(recursive: true));
        final file = p.join(root.path, 'one.pdf');
        File(file).writeAsBytesSync('one'.codeUnits);
        final adapter = FakeBatchAdapter();
        final service = batchService(root, adapter);
        final plan = service.preview(batchCaller, batchParams([file]), (_) {});
        final queueId = plan.meta['queueId']! as String;
        var checked = false;
        adapter.onSubmit = (_) {
          checked = true;
          expect(service.latest(batchCaller), isNull);
          expect(
            () => service.discard(batchCaller, queueId),
            throwsA(
              isA<BackendError>().having(
                (e) => e.code,
                'code',
                'operation_in_progress',
              ),
            ),
          );
        };

        final applied = service.apply(plan.planId, batchCaller, (_) {});

        expect(checked, isTrue);
        expect(applied.applied, 1);
      },
    );
  });

  group('operator Ventoy PDF', () {
    test(
      'operator Ventoy PDF print preview and fake submit preserve source',
      () {
        final source = File(Platform.environment['DOCFLOW_VENTOY_PRINT_PDF']!);
        final original = source.readAsBytesSync();
        final root = batchTempDir('ventoy');
        addTearDown(() => root.deleteSync(recursive: true));
        final file = p.join(root.path, 'ventoy-sample.pdf');
        source.copySync(file);
        final adapter = FakeBatchAdapter()..pdfPageCount = 1;
        final service = batchService(root, adapter);
        final plan = service.preview(batchCaller, batchParams([file]), (_) {});
        expect(plan.meta['fileCount'], 1);
        expect(plan.meta['jobCount'], 1);
        expect(adapter.submitted, isEmpty);

        final result = service.apply(plan.planId, batchCaller, (_) {});
        expect(result.applied, 1);
        expect(result.failed, 0);
        expect(result.meta['status'], 'completed');
        expect(adapter.submitted, hasLength(1));
        expect(source.readAsBytesSync(), original);
        expect(File(file).readAsBytesSync(), original);
      },
      skip: Platform.environment['DOCFLOW_VENTOY_PRINT_PDF'] == null
          ? 'set DOCFLOW_VENTOY_PRINT_PDF to a private Ventoy PDF copy; submission uses only the fake printer'
          : false,
    );
  });
}
