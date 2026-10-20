// Widget tests for the Plan preview port (PlanView and its sections).

import 'package:docflow/models/models.dart';
import 'package:docflow/ui/plan/plan.dart';
import 'package:docflow/ui/plan/plan_logic.dart' show excelCell;
import 'package:docflow/ui/screens/operation_screen.dart';
import 'package:flutter/material.dart' hide IconButton, Step, Switch;
import 'package:flutter_test/flutter_test.dart';

import '../widgets/harness.dart';

Plan _plan({
  String opId = 'files.move',
  List<JsonMap> steps = const [],
  List<JsonMap> warnings = const [],
  JsonMap meta = const {},
}) => Plan.fromJson({
  'planId': 'p1',
  'opId': opId,
  'steps': steps,
  'warnings': warnings,
  'meta': meta,
});

JsonMap _step(
  String kind, {
  String? src,
  String? dst,
  JsonMap meta = const {},
}) => {'kind': kind, 'src': src, 'dst': dst, 'noteKey': null, 'meta': meta};

const _png = 'data:image/png;base64,iVBORw0KGgoAAAA=';

void main() {
  final s = testStrings();

  testWidgets('preview lists steps and warnings; Apply and Cancel fire', (
    tester,
  ) async {
    var applied = 0;
    var cancelled = 0;
    final plan = _plan(
      steps: [
        _step('mkdir', dst: 'C:/out'),
        _step('move', src: r'C:\in\a.pdf', dst: 'C:/out/b.pdf'),
      ],
      warnings: [
        {'key': 'warn.sample', 'subject': 'a.pdf'},
      ],
    );
    await pumpWidgetApp(
      tester,
      planPreview(
        plan: plan,
        s: s,
        error: 'Boom',
        onCancel: () => cancelled++,
        onApply: () => applied++,
      ),
    );

    expect(find.text(s.t('plan.impact.title')), findsOneWidget);
    expect(find.text(s.t('plan.summary', {'count': 1})), findsOneWidget);
    expect(find.text('a.pdf'), findsWidgets);
    expect(find.text('→ b.pdf'), findsOneWidget);
    expect(find.text('warn.sample'), findsOneWidget);
    expect(find.text('Boom'), findsOneWidget);

    await tester.tap(find.text(s.t('action.apply')));
    await tester.tap(find.text(s.t('action.cancel')));
    expect(applied, 1);
    expect(cancelled, 1);
  });

  testWidgets('apply is disabled for an empty plan', (tester) async {
    var applied = 0;
    await pumpWidgetApp(
      tester,
      PlanView.preview(
        plan: _plan(),
        s: s,
        error: '',
        onCancel: () {},
        onApply: () => applied++,
      ),
    );
    expect(find.text(s.t('plan.empty')), findsOneWidget);
    await tester.tap(find.text(s.t('action.apply')));
    expect(applied, 0);
  });

  testWidgets('editable table edits call onEditorRowsChanged', (tester) async {
    final changes = <List<Map<String, Object?>>>[];
    var rows = <Map<String, Object?>>[
      {'_rowId': 'r1', 'rfiNo': 'A', 'note': 'one', 'enabled': true},
      {'_rowId': 'r2', 'rfiNo': 'B', 'note': 'two', 'enabled': false},
    ];
    final plan = _plan(
      steps: [_step('write', dst: 'C:/x.txt')],
      meta: {
        'editor': {
          'kind': 'recordTable',
          'columns': [
            {'key': 'note', 'labelKey': 'col.note'},
            {'key': 'enabled', 'labelKey': 'col.enabled', 'kind': 'bool'},
          ],
        },
      },
    );
    await pumpWidgetApp(
      tester,
      StatefulBuilder(
        builder: (context, setState) => PlanView.fromProps(
          PlanStageProps(
            plan: plan,
            s: s,
            error: '',
            onCancel: () {},
            onApply: () {},
            editorRows: rows,
            onEditorRowsChanged: (next) {
              changes.add(next);
              setState(() => rows = next);
            },
            editorDirty: false,
            onUpdatePreview: () {},
            isUpdatingPreview: false,
            scanPreviewUpdated: false,
          ),
        ),
      ),
      size: const Size(1000, 1000),
    );

    await tester.enterText(find.widgetWithText(TextField, 'one'), 'uno');
    await tester.pump();
    expect(changes.last.first['note'], 'uno');
    expect(changes.last[1]['note'], 'two');

    await tester.tap(find.byType(Checkbox).last);
    await tester.pump();
    expect(changes.last[1]['enabled'], true);

    // The review filter keeps only disabled or flagged rows.
    await tester.tap(find.text(s.t('filter.manualReview')));
    await tester.pump();
    expect(find.widgetWithText(TextField, 'uno'), findsNothing);
  });

  testWidgets('signed staging shows navigator, review summary and warnings', (
    tester,
  ) async {
    const code = 'AZE-SUN-INT-QMS-RFI-CIV-0001';
    final plan = _plan(
      opId: 'rfi.stage_signed_first_pages',
      steps: [
        _step('pdf_first_page_replace', src: 'C:/s.pdf', dst: 'C:/a.pdf'),
      ],
      warnings: [
        {'key': 'warn.signed', 'subject': '$code signed.pdf'},
      ],
      meta: {
        'editor': {
          'kind': 'recordTable',
          'columns': [
            {'key': 'code', 'labelKey': 'col.code', 'kind': 'readonly'},
          ],
        },
        'warningReviewItems': [
          {
            'sourceFile': 'signed.pdf',
            'destination': 'C:/a.pdf',
            'status': 'blocked',
            'reasons': [
              {'key': 'reason.key', 'pageNumber': 2, 'detail': 'Detail'},
            ],
          },
        ],
        'pageComparisons': {
          'r1': {'signed': _png, 'archives': <String, Object?>{}},
        },
      },
    );
    await pumpWidgetApp(
      tester,
      PlanView(
        plan: plan,
        s: s,
        error: '',
        onCancel: () {},
        onApply: () {},
        editorRows: const [
          {'_rowId': 'r1', 'code': code, 'enabled': true},
        ],
        onEditorRowsChanged: (_) {},
      ),
      size: const Size(1000, 1400),
    );

    expect(find.text(s.t('rfi.signed.nav.approved')), findsOneWidget);
    expect(find.text(s.t('rfi.signed.nav.review')), findsOneWidget);
    expect(find.text(s.t('rfi.signed.warningReview.title')), findsOneWidget);
    expect(find.text('signed.pdf'), findsOneWidget);
    expect(find.text(s.t('rfi.signed.previewTitle')), findsOneWidget);
    expect(find.text(s.t('plan.warnings.focus')), findsOneWidget);
    await tester.ensureVisible(find.text(s.t('plan.warnings.focus')));
    await tester.tap(find.text(s.t('plan.warnings.focus')));
    await tester.pumpAndSettle();
    await tester.tap(find.text(s.t('rfi.signed.nav.review')));
    await tester.pumpAndSettle();
    expect(find.text(s.t('rfi.signed.warningReview.title')), findsOneWidget);
  });

  testWidgets('warnings filter by search once there are five', (tester) async {
    await pumpWidgetApp(
      tester,
      PlanWarnings(
        warnings: [
          for (var i = 0; i < 5; i++)
            Warning(key: 'warn.$i', subject: 'file-$i.pdf'),
        ],
        focusableCodes: const [],
        s: s,
      ),
    );
    expect(find.text('file-3.pdf'), findsOneWidget);
    await tester.enterText(find.byType(TextField), 'file-3');
    await tester.pump();
    expect(find.text('file-1.pdf'), findsNothing);
    expect(find.text('file-3.pdf'), findsOneWidget);
    expect(
      find.text(s.t('plan.warnings.showing', {'shown': 1, 'count': 5})),
      findsOneWidget,
    );
  });

  testWidgets('workbook change preview lists the planned cell edits', (
    tester,
  ) async {
    final plan = _plan(
      steps: [
        _step(
          'workbook_update',
          dst: 'C:/log.xlsx',
          meta: {
            'updates': [
              {
                'proposedStatus': 'Closed',
                'canonicalRfi': 'AZE-SUN-INT-QMS-RFI-MEC-0042',
                'target': {
                  'worksheet': {'name': 'Log'},
                  'row': 5,
                  'statusColumn': 3,
                  'statusCell': {'value': 'Open'},
                },
              },
            ],
          },
        ),
      ],
    );
    await pumpWidgetApp(
      tester,
      SingleChildScrollView(
        child: WorkbookChangePreview(plan: plan, s: s),
      ),
    );
    await tester.tap(find.text(s.t('plan.workbookChanges.title')));
    await tester.pumpAndSettle();
    expect(find.text('Log!C5'), findsOneWidget);
    expect(find.text('AZE-SUN-INT-QMS-RFI-MEC-0042'), findsOneWidget);
    expect(find.text('Open'), findsOneWidget);
    expect(find.text('Closed'), findsOneWidget);
  });

  test('plan helpers follow the React rules', () {
    expect(excelCell(5, 3), 'C5');
    expect(excelCell(1, 28), 'AB1');
    expect(excelCell(0, 1), isNull);
    expect(
      canonicalRfiInText('x AZE-SUN-INT-QMS-RFI-ELE-0007 y'),
      'AZE-SUN-INT-QMS-RFI-ELE-0007',
    );
    expect(safePngThumbnail(_png), _png);
    expect(safePngThumbnail('data:image/png;base64,abc'), isNull);
    expect(
      isPlanEditorReady(
        {
          'kind': 'rfiScan',
          'requiredFields': ['code'],
        },
        [
          {'code': 'AZE-SUN-INT-QMS-RFI-CIV-0001'},
          {'code': 'AZE-SUN-INT-QMS-RFI-CIV-0001'},
        ],
      ),
      isFalse,
    );
    expect(
      isPlanEditorReady(
        {
          'requiredFields': ['note'],
        },
        [
          {'note': ' ', 'enabled': false},
          {'note': 'x'},
        ],
      ),
      isTrue,
    );
    final impact = summarizePlanImpact(
      _plan(steps: [_step('copy'), _step('print'), _step('mkdir')]),
    );
    expect((impact.outputs, impact.printJobs, impact.moves), (1, 1, 0));
  });
}
