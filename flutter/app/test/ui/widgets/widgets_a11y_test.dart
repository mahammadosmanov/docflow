import 'package:docflow/models/progress.dart';
import 'package:docflow/ui/widgets/widgets.dart';
import 'package:flutter/material.dart' hide IconButton, Switch;
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'harness.dart';

void main() {
  final s = testStrings();

  testWidgets('IconButton is a labelled, enabled button that fires on tap', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    var taps = 0;
    await pumpWidgetApp(
      tester,
      IconButton(
        label: 'Back',
        onPressed: () => taps++,
        child: const Icon(Icons.arrow_back),
      ),
    );
    expect(
      tester.getSemantics(find.bySemanticsLabel('Back')),
      matchesSemantics(
        isButton: true,
        hasEnabledState: true,
        isEnabled: true,
        isFocusable: true,
        hasTapAction: true,
        hasFocusAction: true,
        label: 'Back',
      ),
    );
    await tester.tap(find.bySemanticsLabel('Back'));
    expect(taps, 1);
    semantics.dispose();
  });

  testWidgets('disabled DfButton does not fire and reports disabled', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    var taps = 0;
    await pumpWidgetApp(
      tester,
      DfButton(variant: DfButtonVariant.danger, child: const Text('Delete')),
    );
    await tester.tap(find.text('Delete'));
    expect(taps, 0);
    expect(
      tester.getSemantics(find.bySemanticsLabel('Delete')),
      matchesSemantics(
        isButton: true,
        hasEnabledState: true,
        isEnabled: false,
        label: 'Delete',
      ),
    );
    semantics.dispose();
  });

  testWidgets('Switch reports toggled state and flips on tap', (tester) async {
    final semantics = tester.ensureSemantics();
    final values = <bool>[];
    await pumpWidgetApp(
      tester,
      Switch(checked: false, semanticsLabel: 'Notify', onChanged: values.add),
    );
    expect(
      tester.getSemantics(find.bySemanticsLabel('Notify')),
      matchesSemantics(
        hasEnabledState: true,
        isEnabled: true,
        hasToggledState: true,
        isToggled: false,
        isFocusable: true,
        hasTapAction: true,
        hasFocusAction: true,
        label: 'Notify',
      ),
    );
    await tester.tap(find.bySemanticsLabel('Notify'));
    expect(values, [true]);
    semantics.dispose();
  });

  testWidgets('Segmented marks the selected option and reports taps', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    final values = <String>[];
    await pumpWidgetApp(
      tester,
      Segmented(
        value: 'a',
        options: const [('a', 'Alpha'), ('b', 'Beta')],
        onChanged: values.add,
      ),
    );
    expect(
      tester.getSemantics(find.bySemanticsLabel('Alpha')),
      matchesSemantics(
        isButton: true,
        hasSelectedState: true,
        isSelected: true,
        hasEnabledState: true,
        isEnabled: true,
        isFocusable: true,
        hasTapAction: true,
        hasFocusAction: true,
        label: 'Alpha',
      ),
    );
    await tester.tap(find.bySemanticsLabel('Beta'));
    expect(values, ['b']);
    semantics.dispose();
  });

  testWidgets('Field names its input and reports each edit', (tester) async {
    final semantics = tester.ensureSemantics();
    final controller = TextEditingController();
    addTearDown(controller.dispose);
    final values = <String>[];
    await pumpWidgetApp(
      tester,
      Field(
        label: 'Project name',
        controller: controller,
        onChanged: values.add,
        help: 'Shown on reports',
      ),
    );
    expect(find.bySemanticsLabel('Project name'), findsOneWidget);
    await tester.enterText(find.byType(TextField), 'Tower');
    expect(values, ['Tower']);
    expect(controller.text, 'Tower');
    expect(find.text('Shown on reports'), findsOneWidget);
    semantics.dispose();
  });

  testWidgets('Modal closes on Escape, backdrop tap and the close button', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    var closes = 0;
    await pumpWidgetApp(
      tester,
      Modal(
        title: 'Confirm',
        s: s,
        onClose: () => closes++,
        child: const Text('Body text'),
      ),
    );
    expect(find.text('Confirm'), findsOneWidget);
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    expect(closes, 1);

    await tester.tapAt(const Offset(5, 5));
    expect(closes, 2);

    await tester.tap(find.bySemanticsLabel(s.t('action.close')));
    expect(closes, 3);
    // Taps on the dialog body do not close it.
    await tester.tap(find.text('Body text'));
    expect(closes, 3);
    semantics.dispose();
  });

  testWidgets('StepIndicator marks the current step as selected', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    await pumpWidgetApp(
      tester,
      const StepIndicator(current: 1, labels: ['One', 'Two', 'Three']),
    );
    expect(
      tester.getSemantics(find.bySemanticsLabel('Two')),
      matchesSemantics(hasSelectedState: true, isSelected: true, label: 'Two'),
    );
    expect(
      tester.getSemantics(find.bySemanticsLabel('Three')),
      matchesSemantics(
        hasSelectedState: true,
        isSelected: false,
        label: 'Three',
      ),
    );
    semantics.dispose();
  });

  testWidgets('Header shows the back button and a heading title', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    var backs = 0;
    await pumpWidgetApp(tester, Header(title: 'Plans', onBack: () => backs++));
    expect(find.text('Plans'), findsOneWidget);
    await tester.tap(find.bySemanticsLabel('←'));
    expect(backs, 1);
    semantics.dispose();
  });

  testWidgets('Notice and EmptyState render their text', (tester) async {
    await pumpWidgetApp(
      tester,
      Column(
        children: [
          const Notice(
            kind: NoticeKind.error,
            title: 'Failed',
            child: Text('Try again'),
          ),
          EmptyState(
            icon: const Icon(Icons.inbox),
            title: 'Nothing here',
            message: 'Add a file to begin',
            action: DfButton(onPressed: () {}, child: const Text('Add')),
          ),
        ],
      ),
    );
    expect(find.text('Failed'), findsOneWidget);
    expect(find.text('Try again'), findsOneWidget);
    expect(find.text('Nothing here'), findsOneWidget);
    expect(find.text('Add a file to begin'), findsOneWidget);
    expect(find.text('Add'), findsOneWidget);
  });

  testWidgets('BusyState shows the phase, detail and stage trail', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    await pumpWidgetApp(
      tester,
      BusyState(
        label: 'Working',
        s: s,
        progress: const ProgressEvent(
          phase: 'scan',
          done: 2,
          total: 4,
          subject: 'a.pdf',
        ),
        progressTrail: const [
          ProgressEvent(phase: 'scan_discovery', done: 4, total: 4),
          ProgressEvent(phase: 'scan', done: 2, total: 4, subject: 'a.pdf'),
        ],
      ),
    );
    expect(find.text('Working'), findsOneWidget);
    expect(find.text(s.t('progress.phase.scan')), findsWidgets);
    expect(find.text('2 / 4 · a.pdf'), findsOneWidget);
    expect(find.bySemanticsLabel(s.t('progress.stages')), findsOneWidget);
    semantics.dispose();
  });

  testWidgets('ActionBar lays out both groups and Centered holds its child', (
    tester,
  ) async {
    await pumpWidgetApp(
      tester,
      Column(
        children: [
          Centered(child: const Text('Middle')),
          ActionBar(
            left: [
              DfButton(
                variant: DfButtonVariant.ghost,
                child: const Text('Back'),
                onPressed: () {},
              ),
            ],
            right: [DfButton(child: const Text('Next'), onPressed: () {})],
          ),
        ],
      ),
    );
    expect(find.text('Middle'), findsOneWidget);
    expect(find.text('Back'), findsOneWidget);
    expect(find.text('Next'), findsOneWidget);
  });
}
