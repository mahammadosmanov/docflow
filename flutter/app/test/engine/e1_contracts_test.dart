// Port of the inline tests in src-tauri/src/contracts.rs.
import 'package:flutter_test/flutter_test.dart';
import 'package:docflow/engine/contracts.dart';
import 'package:docflow/engine/path_selection.dart';

void main() {
  test(
    'operation supportsPresets serializes enabled and defaults disabled',
    () {
      const enabled = OperationDef(
        id: 'files.batch_print',
        titleKey: 'op.files.batch_print.title',
        descriptionKey: 'op.files.batch_print.desc',
        schema: ParamSchema(params: []),
        supportsPresets: true,
      );
      expect(enabled.toJson()['supportsPresets'], true);

      final disabled = OperationDef.fromJson({
        'id': 'rfi.process_scans',
        'titleKey': 'op.rfi.process_scans.title',
        'descriptionKey': 'op.rfi.process_scans.desc',
        'schema': <Object?>[],
      });
      expect(disabled.supportsPresets, isFalse);
    },
  );

  test(
    'protocol one plan and journal snapshots preserve nulls and omissions',
    () {
      const step = Step(
        kind: StepKind.move,
        src: '/in/scan.pdf',
        dst: '/out/RFI.pdf',
        noteKey: null,
        meta: {},
      );
      const plan = Plan(
        planId: 'plan-1',
        opId: 'rfi.process_scans',
        steps: [step],
        warnings: [Warning(key: 'warn.exists', subject: 'scan.pdf')],
        meta: {},
      );
      expect(plan.toJson(), {
        'planId': 'plan-1',
        'opId': 'rfi.process_scans',
        'steps': [
          {
            'kind': 'move',
            'src': '/in/scan.pdf',
            'dst': '/out/RFI.pdf',
            'noteKey': null,
            'meta': <String, Object?>{},
          },
        ],
        'warnings': [
          {'key': 'warn.exists', 'subject': 'scan.pdf'},
        ],
        'meta': <String, Object?>{},
      });

      const journal = JournalEntry(
        journalId: 'journal-1',
        opId: 'rfi.process_scans',
        ts: '2026-08-25T00:00:00+00:00',
        steps: [step],
      );
      final encoded = journal.toJson();
      expect(encoded.containsKey('pendingStep'), isFalse);
      expect((encoded['steps']! as List).first, containsPair('noteKey', null));
    },
  );

  test('protocol one param and result snapshots match frozen contracts', () {
    const parameter = ParamDef(
      name: 'source',
      kind: ParamKind.folder,
      labelKey: 'param.source',
      defaultValue: null,
      required: true,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: 'param.source.help',
      remember: true,
    );
    expect(parameter.toJson(), {
      'name': 'source',
      'kind': 'folder',
      'labelKey': 'param.source',
      'default': null,
      'required': true,
      'choices': <String>[],
      'choiceLabels': <String, String>{},
      'visibleWhen': <String, Object?>{},
      'helpKey': 'param.source.help',
    });

    const result = OperationResult(
      applied: 0,
      failed: 0,
      journalId: '',
      errors: [],
      meta: {},
    );
    expect(result.toJson(), {
      'applied': 0,
      'failed': 0,
      'journalId': '',
      'errors': <Object?>[],
      'meta': <String, Object?>{},
    });
  });

  test('protocol one application response snapshots match frozen facade', () {
    const bootstrap = Bootstrap(
      protocol: 1,
      version: '0.6.0',
      auth: AuthState(
        signedIn: false,
        allowed: false,
        isAdmin: false,
        session: null,
        user: null,
      ),
      prefs: AppPrefs(),
      operations: [],
    );
    expect(bootstrap.toJson(), {
      'protocol': 1,
      'version': '0.6.0',
      'auth': {
        'signedIn': false,
        'allowed': false,
        'isAdmin': false,
        'session': null,
        'user': null,
      },
      'prefs': {
        'locale': null,
        'theme': 'system',
        'font': 'system',
        'fontScale': 1.0,
        'compact': false,
        'startWithWindows': false,
        'replicaSyncEnabled': false,
      },
      'operations': <Object?>[],
    });

    const history = HistoryPayload(
      online: false,
      rows: [
        HistoryRow(
          when: '2026-08-25T00:00:00Z',
          opId: 'rfi.process_scans',
          moves: 1,
          failed: 0,
          journalId: 'local-1',
          steps: null,
          cloudId: null,
          undone: false,
          undoAvailable: true,
          fromCloud: false,
          userEmail: '',
        ),
      ],
    );
    final historyJson = history.toJson();
    final row = (historyJson['rows']! as List).first! as Map<String, Object?>;
    expect(row['steps'], isNull);
    expect(row['cloudId'], isNull);
    expect(row.containsKey('undoAvailable'), isTrue);

    expect(const ProgressEvent(phase: 'preview', done: 1, total: 2).toJson(), {
      'phase': 'preview',
      'done': 1,
      'total': 2,
      'subject': '',
    });

    const review = ReviewItem(
      name: 'scan.pdf',
      path: '/review/scan.pdf',
      code: '',
      confidence: 0.0,
      target: null,
      targetExists: false,
    );
    final reviewJson = review.toJson();
    expect(reviewJson['target'], isNull);
    expect(reviewJson['targetExists'], isFalse);

    const queue = PrintQueueSummary(
      queueId: 'queue-1',
      status: 'paused',
      createdAt: '2026-08-25T00:00:00Z',
      updatedAt: '2026-08-25T00:01:00Z',
      counts: {'pending': 1},
      resumable: 1,
      fileCount: 1,
    );
    expect(queue.toJson()['fileCount'], 1);

    const apply = OperationResult(
      applied: 1,
      failed: 0,
      journalId: 'journal-1',
      errors: [],
      meta: {},
      cloudSynced: false,
    );
    expect(apply.toJson()['cloudSynced'], false);
  });

  test('RFI print find request rejects unknown fields', () {
    final request = RfiPrintFindRequest.fromJson({
      'root': {'selectionId': 's', 'displayPath': '/r', 'name': 'r'},
      'numbers': ['0064'],
      'firstPage': true,
      'sketch': false,
    });
    expect(
      request.root,
      const PathSelection(selectionId: 's', displayPath: '/r', name: 'r'),
    );
    expect(
      () => RfiPrintFindRequest.fromJson({
        'root': {'selectionId': 's', 'displayPath': '/r', 'name': 'r'},
        'numbers': <String>[],
        'firstPage': true,
        'sketch': false,
        'extra': 1,
      }),
      throwsFormatException,
    );
  });
}
