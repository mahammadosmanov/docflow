// Widget tests for the review, result, history and recovery screens. They
// drive the real Bridge through a fake Engine that answers with the engine's
// JSON shapes, so the typed command map and models are exercised too.

import 'package:docflow/app/routes.dart';
import 'package:docflow/app/screen_registry.dart';
import 'package:docflow/bridge/bridge.dart';
import 'package:docflow/engine/engine.dart';
import 'package:docflow/l10n/strings.dart';
import 'package:docflow/models/models.dart';
import 'package:docflow/ui/screens/history_screen.dart';
import 'package:docflow/ui/screens/recovery_screen.dart';
import 'package:docflow/ui/screens/result_view.dart';
import 'package:docflow/ui/screens/review_screen.dart';
import 'package:docflow/ui/screens/screen_common.dart';
import 'package:docflow/ui/widgets/widgets.dart';
import 'package:flutter/material.dart' hide IconButton, Switch;
import 'package:flutter_test/flutter_test.dart';

import '../widgets/harness.dart';

typedef Handler = Object? Function(JsonMap params);

/// Answers each engine command from [handlers] and records every call.
final class FakeEngine extends Engine {
  FakeEngine(this.handlers);

  final Map<String, Handler> handlers;
  final List<(String, JsonMap)> calls = [];

  List<JsonMap> callsTo(String command) => [
    for (final call in calls)
      if (call.$1 == command) call.$2,
  ];

  @override
  Future<Object?> call(
    String command,
    JsonMap params, {
    ProgressHandler? onProgress,
  }) async {
    calls.add((command, params));
    final handler = handlers[command];
    if (handler == null) {
      throw StateError('Unexpected engine command $command');
    }
    return handler(params);
  }
}

const _destination = PathSelection(
  selectionId: 'dest-1',
  displayPath: 'C:/rfi',
  name: 'rfi',
);

JsonMap _reviewItem(
  String id,
  String name, {
  String code = '',
  double confidence = 0.876,
  bool? targetExists,
}) => {
  'selectionId': id,
  'path': 'C:/rfi/$name',
  'name': name,
  'code': code,
  'confidence': confidence,
  'targetExists': targetExists,
};

JsonMap _plan(String planId) => {
  'planId': planId,
  'opId': 'rfi.review',
  'steps': [
    {'kind': 'move', 'src': 'a', 'dst': 'b', 'noteKey': null, 'meta': {}},
  ],
  'warnings': <Object?>[],
  'meta': <String, Object?>{},
};

JsonMap _result({
  int applied = 2,
  int failed = 0,
  String journalId = 'j-1',
  JsonMap? meta,
  bool? cloudSynced,
}) => {
  'applied': applied,
  'failed': failed,
  'journalId': journalId,
  'errors': <Object?>[],
  'meta': meta,
  'cloudSynced': cloudSynced,
};

JsonMap _historyRow({
  String opId = 'rfi.review',
  String journalId = 'j-1',
  bool undone = false,
  bool undoAvailable = true,
  String userEmail = '',
  String when = '2026-10-20T07:09:00Z',
}) => {
  'when': when,
  'opId': opId,
  'moves': 3,
  'failed': 0,
  'journalId': journalId,
  'steps': null,
  'cloudId': null,
  'undone': undone,
  'undoAvailable': undoAvailable,
  'fromCloud': false,
  'userEmail': userEmail,
};

JsonMap _queue({int resumable = 2}) => {
  'queueId': 'q-1',
  'status': 'paused',
  'createdAt': '2026-10-20T07:00:00Z',
  'updatedAt': '2026-10-20T07:05:00Z',
  'counts': {'pending': resumable},
  'resumable': resumable,
  'fileCount': 4,
};

OperationDef _operation(String id, String titleKey) => OperationDef(
  id: id,
  titleKey: titleKey,
  descriptionKey: '$titleKey.help',
  schema: const [],
  supportsPresets: false,
);

ScreenContext _context({
  required Bridge bridge,
  required AppRoute route,
  List<OperationDef> operations = const [],
  void Function(AppRoute route)? navigate,
}) {
  final bootstrap = Bootstrap.fromJson({
    'protocol': bootstrapProtocolVersion,
    'version': '2.0.0',
    'auth': AuthState.signedOut.toJson(),
    'prefs': AppPrefs.defaults.toJson(),
    'operations': [for (final operation in operations) operation.toJson()],
  });
  return ScreenContext(
    bridge: bridge,
    strings: testStrings(),
    bootstrap: bootstrap,
    route: route,
    navigate: navigate ?? (_) {},
    updateAuth: (_) {},
    updatePrefs: (_) {},
  );
}

/// Lets the fake engine's futures and any follow-up frames complete.
Future<void> settle(WidgetTester tester) async {
  for (var i = 0; i < 8; i++) {
    await tester.pump(const Duration(milliseconds: 20));
  }
}

Bridge _bridgeFor(FakeEngine engine) => Bridge(engine);

/// A desktop window. Rows keep their controls beside the text at this width.
const _desktop = Size(1100, 800);

void main() {
  final s = testStrings();

  group('result helpers', () {
    OperationResult result({int applied = 0, int failed = 0, JsonMap? meta}) =>
        OperationResult.fromJson(
          _result(applied: applied, failed: failed, meta: meta),
        );

    test('effect state follows the engine value, then the counts', () {
      expect(
        resultEffectState(result(meta: {'effectState': 'outcome_unknown'})),
        'outcome_unknown',
      );
      expect(
        resultEffectState(result(meta: {'resumeAvailable': true})),
        'partial_committed',
      );
      expect(
        resultEffectState(result(applied: 1, failed: 1)),
        'partial_committed',
      );
      expect(
        resultEffectState(result(applied: 0, failed: 2)),
        'no_effect_retryable',
      );
      expect(resultEffectState(result(applied: 3)), 'committed');
    });

    test('only a committed result without resume counts as success', () {
      expect(resultIsSuccess(result(applied: 3)), isTrue);
      expect(
        resultIsSuccess(result(applied: 3, meta: {'resumeAvailable': true})),
        isFalse,
      );
      expect(resultIsSuccess(result(applied: 1, failed: 1)), isFalse);
    });

    test('confidence is shown as a clamped whole percentage', () {
      expect(formatConfidencePercent(0.876), '88%');
      expect(formatConfidencePercent(1.5), '100%');
      expect(formatConfidencePercent(-0.2), '0%');
      expect(formatConfidencePercent(42, percentScale: true), '42%');
      expect(formatConfidencePercent('0.5'), '50%');
      expect(formatConfidencePercent(''), isNull);
      expect(formatConfidencePercent('abc'), isNull);
      expect(formatConfidencePercent(null), isNull);
    });
  });

  group('ResultView', () {
    testWidgets('shows counts, effect, cloud badge and the enabled actions', (
      tester,
    ) async {
      var done = 0;
      var undone = 0;
      await pumpWidgetApp(
        tester,
        SingleChildScrollView(
          child: ResultView(
            result: OperationResult.fromJson(
              _result(applied: 5, failed: 0, cloudSynced: true),
            ),
            s: s,
            onDone: () => done++,
            onUndo: () => undone++,
          ),
        ),
      );
      await settle(tester);

      expect(find.text(s.t('result.applied', {'count': 5})), findsOneWidget);
      expect(find.text(s.t('result.failed', {'count': 0})), findsOneWidget);
      expect(find.text(s.t('effect.committed')), findsOneWidget);
      expect(find.text(s.t('history.cloud')), findsOneWidget);

      await tester.tap(find.text(s.t('action.undo')));
      await tester.tap(find.text(s.t('action.done')));
      await settle(tester);
      expect(undone, 1);
      expect(done, 1);
    });

    testWidgets('undo is disabled without a journal or when not undoable', (
      tester,
    ) async {
      var undone = 0;
      await pumpWidgetApp(
        tester,
        SingleChildScrollView(
          child: ResultView(
            result: OperationResult.fromJson(
              _result(journalId: '', meta: {'nonUndoable': true}),
            ),
            s: s,
            onDone: () {},
            onUndo: () => undone++,
          ),
        ),
      );
      await settle(tester);
      await tester.tap(find.text(s.t('action.undo')));
      await settle(tester);
      expect(undone, 0);
    });

    testWidgets('renders one copy button per sheet and the resume summary', (
      tester,
    ) async {
      final copied = <String?>[];
      await pumpWidgetApp(
        tester,
        SingleChildScrollView(
          child: ResultView(
            result: OperationResult.fromJson(
              _result(
                applied: 1,
                failed: 1,
                meta: {'resumeAvailable': true, 'resumable': 4},
              ),
            ),
            s: s,
            onDone: () {},
            onUndo: () {},
            copySheets: const ['Sheet1', 'Sheet2'],
            onCopy: copied.add,
          ),
        ),
      );
      await settle(tester);

      expect(find.text(s.t('print.resume.incomplete')), findsOneWidget);
      expect(
        find.text(s.t('print.resume.summary', {'count': 4})),
        findsOneWidget,
      );
      await tester.tap(find.text('Sheet2 · ${s.t('rfi.action.copySheet')}'));
      await settle(tester);
      expect(copied, ['Sheet2']);
    });
  });

  group('ReviewScreen', () {
    late FakeEngine engine;
    var scans = 0;

    Map<String, Handler> reviewHandlers({
      required List<JsonMap> items,
      Handler? preview,
      Handler? apply,
      Handler? undo,
    }) => {
      'engine/reviewScan': (_) {
        scans++;
        return items;
      },
      'engine/reviewPreview': preview ?? (_) => _plan('p-1'),
      'engine/apply': apply ?? (_) => _result(),
      'engine/undo': undo ?? (_) => _result(applied: 0),
    };

    /// A stand-in for PlanView: shows the plan id and the two actions.
    Widget previewStub({
      required Plan plan,
      required Strings s,
      required String error,
      required VoidCallback onCancel,
      required VoidCallback onApply,
    }) => Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        Text('PLAN ${plan.planId}'),
        if (error.isNotEmpty) Text(error),
        DfButton(onPressed: onApply, child: const Text('APPLY')),
        DfButton(
          variant: DfButtonVariant.ghost,
          onPressed: onCancel,
          child: const Text('CANCEL'),
        ),
      ],
    );

    ScreenContext contextFor() => _context(
      bridge: _bridgeFor(engine),
      route: const ReviewRoute(destination: _destination),
    );

    setUp(() {
      scans = 0;
    });

    testWidgets('lists scanned files with their confidence', (tester) async {
      engine = FakeEngine(
        reviewHandlers(
          items: [
            _reviewItem('a', 'first.pdf', code: 'RFI-1'),
            _reviewItem('b', 'second.pdf', confidence: 0.4),
          ],
        ),
      );
      await pumpWidgetApp(
        tester,
        ReviewScreen(ctx: contextFor(), planPreview: previewStub),
      );
      await settle(tester);

      expect(find.text('first.pdf'), findsOneWidget);
      expect(find.text('second.pdf'), findsOneWidget);
      expect(find.text('${s.t('ocr.confidence')}: 88%'), findsOneWidget);
      expect(find.text('${s.t('ocr.confidence')}: 40%'), findsOneWidget);
      expect(
        engine.callsTo('engine/reviewScan').single['destination'],
        _destination.toJson(),
      );
      expect(find.text(s.t('review.noChanges')), findsOneWidget);
    });

    testWidgets('shows the empty state when the folder has no files', (
      tester,
    ) async {
      engine = FakeEngine(reviewHandlers(items: const []));
      await pumpWidgetApp(
        tester,
        ReviewScreen(ctx: contextFor(), planPreview: previewStub),
      );
      await settle(tester);

      expect(find.text(s.t('review.empty')), findsOneWidget);
      expect(find.text(s.t('review.empty.help')), findsOneWidget);
    });

    testWidgets('select all put keeps files without a code', (tester) async {
      engine = FakeEngine(
        reviewHandlers(
          items: [
            _reviewItem('a', 'coded.pdf', code: 'RFI-7'),
            _reviewItem('b', 'blank.pdf'),
          ],
        ),
      );
      await pumpWidgetApp(
        tester,
        ReviewScreen(ctx: contextFor(), planPreview: previewStub),
      );
      await settle(tester);

      await tester.tap(find.text(s.t('review.selectAllPut')));
      await settle(tester);

      expect(find.text(s.t('review.selected', {'count': 1})), findsOneWidget);
      await tester.tap(find.text(s.t('action.preview')));
      await settle(tester);

      final decisions = engine.callsTo('engine/reviewPreview').single;
      expect(decisions['decisions'], [
        {'itemId': 'a', 'action': 'put', 'code': 'RFI-7'},
      ]);
    });

    testWidgets('a put decision sends the typed code and reaches apply', (
      tester,
    ) async {
      engine = FakeEngine(
        reviewHandlers(items: [_reviewItem('a', 'typed.pdf')]),
      );
      await pumpWidgetApp(
        tester,
        ReviewScreen(ctx: contextFor(), planPreview: previewStub),
      );
      await settle(tester);

      await tester.tap(find.text(s.t('review.put')));
      await settle(tester);
      await tester.enterText(find.byType(TextField), 'RFI-42');
      await settle(tester);

      await tester.tap(find.text(s.t('action.preview')));
      await settle(tester);
      expect(engine.callsTo('engine/reviewPreview').single['decisions'], [
        {'itemId': 'a', 'action': 'put', 'code': 'RFI-42'},
      ]);
      expect(find.text('PLAN p-1'), findsOneWidget);

      await tester.tap(find.text('APPLY'));
      await settle(tester);
      expect(engine.callsTo('engine/apply').single, {
        'opId': 'rfi.review',
        'planId': 'p-1',
      });
      expect(find.text(s.t('result.applied', {'count': 2})), findsOneWidget);
    });

    testWidgets('delete-all previews every file and cancel returns to list', (
      tester,
    ) async {
      engine = FakeEngine(
        reviewHandlers(
          items: [_reviewItem('a', 'x.pdf'), _reviewItem('b', 'y.pdf')],
        ),
      );
      await pumpWidgetApp(
        tester,
        ReviewScreen(ctx: contextFor(), planPreview: previewStub),
      );
      await settle(tester);

      await tester.tap(find.text(s.t('review.selectAllTrash')));
      await settle(tester);
      await tester.tap(find.text(s.t('action.preview')));
      await settle(tester);

      expect(engine.callsTo('engine/reviewPreview').single['decisions'], [
        {'itemId': 'a', 'action': 'delete', 'code': ''},
        {'itemId': 'b', 'action': 'delete', 'code': ''},
      ]);
      await tester.tap(find.text('CANCEL'));
      await settle(tester);
      expect(find.text(s.t('review.selectAllTrash')), findsOneWidget);
      expect(find.text('PLAN p-1'), findsNothing);
    });

    testWidgets('undo after apply rescans and returns to the list', (
      tester,
    ) async {
      engine = FakeEngine(
        reviewHandlers(items: [_reviewItem('a', 'done.pdf', code: 'RFI-9')]),
      );
      await pumpWidgetApp(
        tester,
        ReviewScreen(ctx: contextFor(), planPreview: previewStub),
      );
      await settle(tester);
      await tester.tap(find.text(s.t('review.selectAllPut')));
      await settle(tester);
      await tester.tap(find.text(s.t('action.preview')));
      await settle(tester);
      await tester.tap(find.text('APPLY'));
      await settle(tester);

      await tester.tap(find.text(s.t('action.undo')));
      await settle(tester);

      expect(engine.callsTo('engine/undo').single, {'journalId': 'j-1'});
      expect(scans, 2);
      expect(find.text('done.pdf'), findsOneWidget);
    });

    testWidgets('a scan failure is shown with its localized message', (
      tester,
    ) async {
      engine = FakeEngine({
        'engine/reviewScan': (_) => throw const BridgeError(
          code: 'boom',
          message: 'error.rfi.workbookInvalid',
        ),
      });
      await pumpWidgetApp(
        tester,
        ReviewScreen(ctx: contextFor(), planPreview: previewStub),
      );
      await settle(tester);

      expect(find.text(s.t('error.rfi.workbookInvalid')), findsOneWidget);
    });

    testWidgets('the warning shows when a put would replace an existing file', (
      tester,
    ) async {
      engine = FakeEngine(
        reviewHandlers(
          items: [
            _reviewItem('a', 'clash.pdf', code: 'RFI-3', targetExists: true),
          ],
        ),
      );
      await pumpWidgetApp(
        tester,
        ReviewScreen(ctx: contextFor(), planPreview: previewStub),
      );
      await settle(tester);
      expect(find.text(s.t('review.willReplace')), findsNothing);

      await tester.tap(find.text(s.t('review.put')));
      await settle(tester);
      expect(find.text(s.t('review.willReplace')), findsOneWidget);
    });

    testWidgets('registerReviewScreen builds the screen through the registry', (
      tester,
    ) async {
      engine = FakeEngine(reviewHandlers(items: const []));
      final registry = ScreenRegistry();
      registerReviewScreen(registry, planPreview: previewStub);
      expect(registry.isRegistered(ScreenId.review), isTrue);
      await pumpWidgetApp(
        tester,
        registry.build(ScreenId.review, contextFor()),
      );
      await settle(tester);
      expect(find.text(s.t('review.empty')), findsOneWidget);
    });
  });

  group('HistoryScreen', () {
    testWidgets('lists rows with titles, states and undo availability', (
      tester,
    ) async {
      final engine = FakeEngine({
        'history/list': (_) => {
          'online': true,
          'rows': [
            _historyRow(journalId: 'j-1', userEmail: 'ops@example.test'),
            _historyRow(
              opId: 'rfi.process_scans',
              journalId: 'j-2',
              undone: true,
            ),
            _historyRow(
              opId: 'mystery.op',
              journalId: 'j-3',
              undoAvailable: false,
            ),
          ],
        },
      });
      await pumpWidgetApp(
        tester,
        size: _desktop,
        HistoryScreen(
          ctx: _context(
            bridge: _bridgeFor(engine),
            route: const HistoryRoute(),
            operations: [
              _operation('files.batch_print', 'op.files.batch_print.title'),
            ],
          ),
        ),
      );
      await settle(tester);

      expect(engine.callsTo('history/list').single, {'limit': 50});
      expect(find.text(s.t('history.cloud')), findsOneWidget);
      expect(find.text(s.t('review.title')), findsOneWidget);
      expect(find.text(s.t('op.rfi.process_scans.title')), findsOneWidget);
      expect(find.text(s.t('history.unknownOperation')), findsOneWidget);
      expect(find.text(s.t('history.undone')), findsOneWidget);
      expect(find.text(s.t('history.localJournalRequired')), findsOneWidget);
      expect(find.textContaining('ops@example.test'), findsOneWidget);
      expect(find.text(s.t('action.undo')), findsOneWidget);
    });

    testWidgets('an empty list shows the empty state and local source', (
      tester,
    ) async {
      final engine = FakeEngine({
        'history/list': (_) => {'online': false, 'rows': <Object?>[]},
      });
      await pumpWidgetApp(
        tester,
        size: _desktop,
        HistoryScreen(
          ctx: _context(
            bridge: _bridgeFor(engine),
            route: const HistoryRoute(),
          ),
        ),
      );
      await settle(tester);

      expect(find.text(s.t('history.local')), findsOneWidget);
      expect(find.text(s.t('history.sourceLocal')), findsOneWidget);
      expect(find.text(s.t('history.empty')), findsOneWidget);
    });

    testWidgets('undo in place sends the journal and reloads', (tester) async {
      var loads = 0;
      final engine = FakeEngine({
        'history/list': (_) {
          loads++;
          return {
            'online': false,
            'rows': [_historyRow(journalId: 'j-7')],
          };
        },
        'engine/undo': (_) => _result(applied: 3, journalId: 'j-7'),
      });
      await pumpWidgetApp(
        tester,
        size: _desktop,
        HistoryScreen(
          ctx: _context(
            bridge: _bridgeFor(engine),
            route: const HistoryRoute(),
          ),
        ),
      );
      await settle(tester);

      await tester.tap(find.text(s.t('action.undo')));
      await settle(tester);
      expect(find.text(s.t('history.undoOriginal')), findsOneWidget);
      expect(find.text(s.t('history.undoToFolder')), findsOneWidget);

      await tester.tap(find.text(s.t('history.undoOriginal')));
      await settle(tester);

      expect(engine.callsTo('engine/undo').single, {'journalId': 'j-7'});
      expect(loads, 2);
      expect(find.text(s.t('history.undoOriginal')), findsNothing);
    });

    testWidgets('undo to folder picks a folder and restores there', (
      tester,
    ) async {
      final engine = FakeEngine({
        'history/list': (_) => {
          'online': false,
          'rows': [_historyRow(journalId: 'j-8')],
        },
        'paths/select': (_) => [
          {'selectionId': 'out-1', 'displayPath': 'D:/out', 'name': 'out'},
        ],
        'engine/undo': (_) => _result(applied: 3, journalId: 'j-8'),
      });
      await pumpWidgetApp(
        tester,
        size: _desktop,
        HistoryScreen(
          ctx: _context(
            bridge: _bridgeFor(engine),
            route: const HistoryRoute(),
          ),
        ),
      );
      await settle(tester);

      await tester.tap(find.text(s.t('action.undo')));
      await settle(tester);
      await tester.tap(find.text(s.t('history.undoToFolder')));
      await settle(tester);

      expect(engine.callsTo('paths/select').single, {
        'opId': 'history.undo',
        'paramName': 'destination',
      });
      expect(engine.callsTo('engine/undo').single, {
        'journalId': 'j-8',
        'destination': {
          'selectionId': 'out-1',
          'displayPath': 'D:/out',
          'name': 'out',
        },
      });
    });

    testWidgets('the undo dialog closes without an engine call', (
      tester,
    ) async {
      final engine = FakeEngine({
        'history/list': (_) => {
          'online': true,
          'rows': [_historyRow()],
        },
      });
      await pumpWidgetApp(
        tester,
        size: _desktop,
        HistoryScreen(
          ctx: _context(
            bridge: _bridgeFor(engine),
            route: const HistoryRoute(),
          ),
        ),
      );
      await settle(tester);
      await tester.tap(find.text(s.t('action.undo')));
      await settle(tester);

      await tester.tap(find.byIcon(Icons.close));
      await settle(tester);
      expect(find.text(s.t('history.undoOriginal')), findsNothing);
      expect(engine.callsTo('engine/undo'), isEmpty);
    });

    testWidgets('a failed history load shows the error', (tester) async {
      final engine = FakeEngine({
        'history/list': (_) =>
            throw const BridgeError(code: 'x', message: 'error.preset.stale'),
      });
      await pumpWidgetApp(
        tester,
        size: _desktop,
        HistoryScreen(
          ctx: _context(
            bridge: _bridgeFor(engine),
            route: const HistoryRoute(),
          ),
        ),
      );
      await settle(tester);
      expect(find.text(s.t('error.preset.stale')), findsOneWidget);
      expect(find.text(s.t('history.empty')), findsOneWidget);
    });
  });

  group('RecoveryScreen', () {
    testWidgets('lists journals and the resumable print queue', (tester) async {
      final engine = FakeEngine({
        'recovery/list': (_) => {
          'journals': [
            {
              'journalId': 'jr-1',
              'disposition': 'no_effect',
              'restoreOriginalAvailable': true,
            },
            {
              'journalId': 'jr-2',
              'disposition': 'outcome_unknown',
              'restoreOriginalAvailable': false,
            },
          ],
          'printQueue': _queue(resumable: 2),
        },
      });
      await pumpWidgetApp(
        tester,
        size: _desktop,
        RecoveryScreen(
          ctx: _context(
            bridge: _bridgeFor(engine),
            route: const RecoveryRoute(),
          ),
        ),
      );
      await settle(tester);

      expect(find.text(s.t('recovery.noEffect')), findsOneWidget);
      expect(find.text(s.t('recovery.outcomeUnknown')), findsOneWidget);
      expect(find.text('jr-1'), findsOneWidget);
      expect(find.text(s.t('recovery.manual')), findsNWidgets(2));
      expect(find.text(s.t('recovery.restoreOriginal')), findsOneWidget);
      expect(find.text(s.t('recovery.printQueue')), findsOneWidget);
      expect(
        find.text(s.t('print.resume.summary', {'count': 2})),
        findsOneWidget,
      );
    });

    testWidgets('restore original reports a failed restore', (tester) async {
      var recoveryCalls = 0;
      final engine = FakeEngine({
        'recovery/list': (_) {
          recoveryCalls++;
          return {
            'journals': [
              {
                'journalId': 'jr-1',
                'disposition': 'no_effect',
                'restoreOriginalAvailable': true,
              },
            ],
            'printQueue': null,
          };
        },
        'engine/undo': (_) => _result(applied: 0, failed: 1, journalId: 'jr-1'),
      });
      await pumpWidgetApp(
        tester,
        size: _desktop,
        RecoveryScreen(
          ctx: _context(
            bridge: _bridgeFor(engine),
            route: const RecoveryRoute(),
          ),
        ),
      );
      await settle(tester);

      await tester.tap(find.text(s.t('recovery.restoreOriginal')));
      await settle(tester);

      expect(engine.callsTo('engine/undo').single, {'journalId': 'jr-1'});
      expect(recoveryCalls, 2);
      expect(
        find.text(s.t('error.rfi.followUp.recoveryRequired')),
        findsOneWidget,
      );
    });

    testWidgets('discard calls the queue discard and reloads', (tester) async {
      var queueVisible = true;
      final engine = FakeEngine({
        'recovery/list': (_) => {
          'journals': <Object?>[],
          'printQueue': queueVisible ? _queue() : null,
        },
        'engine/printQueueDiscard': (_) {
          queueVisible = false;
          return _queue()..['status'] = 'discarded';
        },
      });
      await pumpWidgetApp(
        tester,
        size: _desktop,
        RecoveryScreen(
          ctx: _context(
            bridge: _bridgeFor(engine),
            route: const RecoveryRoute(),
          ),
        ),
      );
      await settle(tester);

      await tester.tap(find.text(s.t('print.resume.discard')));
      await settle(tester);

      expect(engine.callsTo('engine/printQueueDiscard').single, {
        'queueId': 'q-1',
      });
      expect(find.text(s.t('recovery.printQueue')), findsNothing);
      expect(find.text(s.t('recovery.empty')), findsOneWidget);
    });

    testWidgets('review opens the batch print operation', (tester) async {
      AppRoute? opened;
      final engine = FakeEngine({
        'recovery/list': (_) => {
          'journals': <Object?>[],
          'printQueue': _queue(),
        },
      });
      final batchPrint = _operation(
        'files.batch_print',
        'op.files.batch_print.title',
      );
      await pumpWidgetApp(
        tester,
        size: _desktop,
        RecoveryScreen(
          ctx: _context(
            bridge: _bridgeFor(engine),
            route: const RecoveryRoute(),
            operations: [batchPrint],
            navigate: (route) => opened = route,
          ),
        ),
      );
      await settle(tester);

      await tester.tap(find.text(s.t('print.resume.review')));
      await settle(tester);
      expect(opened, isA<OperationRoute>());
      expect((opened! as OperationRoute).operation.id, 'files.batch_print');
    });

    testWidgets('no journals and no queue shows the empty state', (
      tester,
    ) async {
      final engine = FakeEngine({
        'recovery/list': (_) => {'journals': <Object?>[], 'printQueue': null},
      });
      await pumpWidgetApp(
        tester,
        size: _desktop,
        RecoveryScreen(
          ctx: _context(
            bridge: _bridgeFor(engine),
            route: const RecoveryRoute(),
          ),
        ),
      );
      await settle(tester);

      expect(find.text(s.t('recovery.empty')), findsOneWidget);
      expect(find.text(s.t('recovery.empty.help')), findsOneWidget);
    });
  });
}
