import 'package:docflow/ui/widgets/widgets.dart';
import 'package:flutter/material.dart' hide IconButton, Switch;
import 'package:flutter/semantics.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'harness.dart';

void main() {
  final s = testStrings();

  Widget region({FocusNode? focusNode, double height = 600}) => SizedBox(
    height: height,
    child: AdjustableDataRegion(
      s: s,
      label: 'Orders',
      focusNode: focusNode,
      child: const SizedBox.expand(key: Key('content')),
    ),
  );

  double contentHeight(WidgetTester tester) =>
      tester.getSize(find.byKey(const Key('content'))).height;

  testWidgets('starts at 320px and exposes the React labels', (tester) async {
    final semantics = tester.ensureSemantics();
    await pumpWidgetApp(tester, region());
    expect(contentHeight(tester), 320);
    expect(find.bySemanticsLabel('Resize Orders height'), findsOneWidget);
    expect(find.text('Reset height'), findsOneWidget);
    expect(find.text('Fit available space'), findsOneWidget);
    semantics.dispose();
  });

  testWidgets('arrow keys step by 16px and are clamped to the bounds', (
    tester,
  ) async {
    final node = FocusNode();
    addTearDown(node.dispose);
    await pumpWidgetApp(tester, region(focusNode: node));
    node.requestFocus();
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.pump();
    expect(contentHeight(tester), 336);

    await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
    await tester.pump();
    expect(contentHeight(tester), 304);
  });

  testWidgets('Home goes to 140 and End goes to the fit height', (
    tester,
  ) async {
    final node = FocusNode();
    addTearDown(node.dispose);
    await pumpWidgetApp(tester, region(focusNode: node));
    node.requestFocus();
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.home);
    await tester.pump();
    expect(contentHeight(tester), 140);

    await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
    await tester.pump();
    expect(contentHeight(tester), 140, reason: 'min is a hard floor');

    await tester.sendKeyEvent(LogicalKeyboardKey.end);
    await tester.pump();
    // 600px viewport minus the 28px controls row.
    expect(contentHeight(tester), 572);
  });

  testWidgets('Reset height returns to 320 and Fit fills the viewport', (
    tester,
  ) async {
    await pumpWidgetApp(tester, region());
    await tester.tap(find.text('Fit available space'));
    await tester.pump();
    expect(contentHeight(tester), 572);

    await tester.tap(find.text('Reset height'));
    await tester.pump();
    expect(contentHeight(tester), 320);
  });

  testWidgets('dragging the handle resizes the region', (tester) async {
    await pumpWidgetApp(tester, region());
    final grip = find.byType(CustomPaint).last;
    await tester.drag(grip, const Offset(0, 40));
    await tester.pump();
    expect(contentHeight(tester), greaterThan(320));
  });

  testWidgets('the handle is an adjustable slider for assistive technology', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    await pumpWidgetApp(tester, region());
    tester.semantics.performAction(
      find.semantics.byLabel('Resize Orders height'),
      SemanticsAction.increase,
    );
    await tester.pump();
    expect(contentHeight(tester), 336);
    semantics.dispose();
  });
}
