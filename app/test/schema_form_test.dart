import 'package:docflow/core/l10n/strings.dart';
import 'package:docflow/core/operations/models.dart';
import 'package:docflow/ui/forms/schema_form.dart';
import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('multi-file queue can be reordered and edited before preview', (
    tester,
  ) async {
    final values = <String, dynamic>{
      'files': <String>[r'C:\Docs\first.pdf', r'C:\Docs\second.docx'],
    };
    final parameter = ParamDef.fromJson({
      'name': 'files',
      'kind': 'files',
      'labelKey': 'param.print.files',
      'required': true,
      'default': <String>[],
      'helpKey': 'param.print.files.help',
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
          body: SizedBox(
            width: 800,
            height: 600,
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

    expect(find.text('first.pdf'), findsOneWidget);
    expect(find.text('second.docx'), findsOneWidget);
    expect(find.text('2 files selected'), findsOneWidget);

    await tester.tap(find.byIcon(Icons.arrow_downward).first);
    await tester.pump();
    expect((values['files'] as List<String>).first, r'C:\Docs\second.docx');

    await tester.tap(find.byIcon(Icons.close).last);
    await tester.pump();
    expect(values['files'], [r'C:\Docs\second.docx']);
    expect(changes, 2);
  });
}
