import 'package:docflow/core/l10n/strings.dart';
import 'package:docflow/ui/shell/operation_flow_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('sheet copy buttons wrap above Done on compact windows', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        locale: const Locale('en'),
        supportedLocales: Strings.supported,
        localizationsDelegates: const [
          StringsDelegate(),
          GlobalMaterialLocalizations.delegate,
          GlobalWidgetsLocalizations.delegate,
          GlobalCupertinoLocalizations.delegate,
        ],
        home: Scaffold(
          body: Align(
            alignment: Alignment.bottomCenter,
            child: SizedBox(
              width: 340,
              child: ResultActionLayout(
                copySheets: const ['CIV', 'MEC', 'ELE', 'PRC'],
                copyingSheet: null,
                legacyCopy: false,
                onCopy: (_) {},
                left: const OutlinedButton(
                  onPressed: null,
                  child: Text('Undo'),
                ),
                right: const FilledButton(
                  key: ValueKey('test-done'),
                  onPressed: null,
                  child: Text('Done'),
                ),
              ),
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.byKey(const ValueKey('result-copy-actions')), findsOneWidget);
    for (final sheet in const ['CIV', 'MEC', 'ELE', 'PRC']) {
      expect(find.byKey(ValueKey('copy-sheet-$sheet')), findsOneWidget);
    }
    final copyArea = tester.getRect(
      find.byKey(const ValueKey('result-copy-actions')),
    );
    final done = tester.getRect(find.byKey(const ValueKey('test-done')));
    expect(copyArea.bottom, lessThanOrEqualTo(done.top));
    expect(tester.takeException(), isNull);
  });
}
