import 'package:docflow/core/l10n/strings.dart';
import 'package:docflow/core/operations/models.dart';
import 'package:docflow/ui/forms/schema_form.dart';
import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('multiple RFI sheets have independent compact number controls', (
    tester,
  ) async {
    final values = <String, dynamic>{};
    final parameter = ParamDef.fromJson({
      'name': 'sheet_counts',
      'kind': 'sheetCounts',
      'labelKey': 'param.rfi.sheetCounts',
      'required': true,
      'default': {'CIV': 3, 'MEC': 0, 'ELE': 0, 'PRC': 0},
      'choices': ['CIV', 'MEC', 'ELE', 'PRC'],
      'helpKey': 'param.rfi.sheetCounts.help',
    });
    var changes = 0;
    expect(parameter.kind, ParamKind.sheetCounts);
    expect(parameter.choices, ['CIV', 'MEC', 'ELE', 'PRC']);

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
          body: SizedBox(
            width: 800,
            height: 500,
            child: SchemaForm(
              params: [parameter],
              values: values,
              onChanged: () => changes++,
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.byKey(const ValueKey('sheet-count-CIV')), findsOneWidget);
    expect(find.byKey(const ValueKey('sheet-count-PRC')), findsOneWidget);
    expect(find.text('3 RFI numbers in this operation'), findsOneWidget);

    await tester.tap(find.byKey(const ValueKey('sheet-enabled-MEC')));
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('sheet-plus-MEC')));
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('sheet-minus-CIV')));
    await tester.pump();

    expect(values['sheet_counts'], {
      'CIV': 2,
      'MEC': 2,
      'ELE': 0,
      'PRC': 0,
    });
    expect(find.text('4 RFI numbers in this operation'), findsOneWidget);
    expect(changes, 3);
  });
}
