import 'package:docflow/core/l10n/strings.dart';
import 'package:docflow/core/operations/models.dart';
import 'package:docflow/ui/forms/schema_form.dart';
import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('Process Scans folder layout defaults on and can be disabled', (
    tester,
  ) async {
    final values = <String, dynamic>{};
    final parameter = ParamDef.fromJson({
      'name': 'organize_into_rfi_folders',
      'kind': 'bool',
      'labelKey': 'param.organize_into_rfi_folders',
      'required': false,
      'default': true,
      'helpKey': 'param.organize_into_rfi_folders.help',
    });
    var changes = 0;

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
          body: SchemaForm(
            params: [parameter],
            values: values,
            onChanged: () => changes++,
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    final toggle = tester.widget<SwitchListTile>(find.byType(SwitchListTile));
    expect(toggle.value, isTrue);
    expect(values['organize_into_rfi_folders'], isTrue);
    expect(find.text('Create a separate folder for each RFI'), findsOneWidget);

    await tester.tap(find.byType(SwitchListTile));
    await tester.pumpAndSettle();

    expect(values['organize_into_rfi_folders'], isFalse);
    expect(changes, 1);
  });
}
