import 'package:docflow/ui/widgets/widgets.dart';
import 'package:flutter/material.dart' hide IconButton, Switch;
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'harness.dart';

void main() {
  final s = testStrings();

  Widget handle({
    required double width,
    required List<double> commits,
    List<double>? previews,
    FocusNode? focusNode,
  }) => SizedBox(
    height: 100,
    child: Stack(
      children: [
        Positioned(
          top: 0,
          bottom: 0,
          right: 0,
          child: ColumnResizeHandle(
            width: width,
            label: 'Name',
            s: s,
            focusNode: focusNode,
            onWidthChange: commits.add,
            onWidthPreview: previews?.add,
          ),
        ),
      ],
    ),
  );

  testWidgets('is announced as an adjustable column with the React text', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    await pumpWidgetApp(tester, handle(width: 100, commits: []));
    expect(find.bySemanticsLabel('Resize Name column'), findsOneWidget);
    final node = tester.getSemantics(
      find.bySemanticsLabel('Resize Name column'),
    );
    expect(node.value, '100 pixels wide');
    semantics.dispose();
  });

  testWidgets('arrow keys step 16px; Home and End go to min and max', (
    tester,
  ) async {
    final commits = <double>[];
    final node = FocusNode();
    addTearDown(node.dispose);
    await pumpWidgetApp(
      tester,
      handle(width: 100, commits: commits, focusNode: node),
    );
    node.requestFocus();
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowLeft);
    expect(commits, [116, 84]);

    await tester.sendKeyEvent(LogicalKeyboardKey.home);
    await tester.sendKeyEvent(LogicalKeyboardKey.end);
    expect(commits.last, 720);
    expect(commits[commits.length - 2], 48);
  });

  testWidgets('drag previews the width and commits it on release', (
    tester,
  ) async {
    final commits = <double>[];
    final previews = <double>[];
    await pumpWidgetApp(
      tester,
      handle(width: 100, commits: commits, previews: previews),
    );
    await tester.drag(find.byType(GestureDetector).last, const Offset(30, 0));
    await tester.pump();
    expect(previews, isNotEmpty);
    expect(commits, hasLength(1));
    expect(commits.single, previews.last);
    expect(commits.single, greaterThan(100));
  });

  test('clampColumnWidth keeps widths within 48..720', () {
    expect(clampColumnWidth(10), 48);
    expect(clampColumnWidth(900), 720);
    expect(clampColumnWidth(200), 200);
  });
}
