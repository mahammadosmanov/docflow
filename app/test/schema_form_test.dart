import 'package:docflow/core/l10n/strings.dart';
import 'package:docflow/core/operations/models.dart';
import 'package:docflow/ui/forms/schema_form.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';

class _TestStringsDelegate extends LocalizationsDelegate<Strings> {
  const _TestStringsDelegate();

  @override
  bool isSupported(Locale locale) => true;

  @override
  Future<Strings> load(Locale locale) => SynchronousFuture(
    Strings({
      'param.print.files': 'Files',
      'param.print.files.help': 'Select files',
      'param.hint.files': 'No files selected',
      'print.filesSelected': '{count} files selected',
      'print.paper.all': 'All files',
      'print.paper.selected': 'Selected files',
      'action.addFiles': 'Add files',
      'action.clear': 'Clear',
      'action.moveUp': 'Move up',
      'action.moveDown': 'Move down',
      'action.remove': 'Remove',
    }),
  );

  @override
  bool shouldReload(_TestStringsDelegate old) => false;
}

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
          _TestStringsDelegate(),
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

  testWidgets('paper size applies to all, selected files, or one file', (
    tester,
  ) async {
    await tester.pumpWidget(const SizedBox.shrink());
    await tester.pump();
    final values = <String, dynamic>{
      'files': <Object>[
        {'path': r'C:\Docs\first.pdf', 'option': 'A4'},
        {'path': r'C:\Docs\second.pdf', 'option': 'A4'},
        {'path': r'C:\Docs\third.pdf', 'option': 'A4'},
      ],
    };
    final parameter = ParamDef.fromJson({
      'name': 'files',
      'kind': 'files',
      'labelKey': 'param.print.files',
      'required': true,
      'default': <Object>[],
      'itemChoices': ['A5', 'A4', 'A3'],
      'itemChoiceLabels': <String, String>{},
      'itemDefault': 'A4',
    });

    await tester.pumpWidget(
      MaterialApp(
        locale: const Locale('en'),
        supportedLocales: Strings.supported,
        localizationsDelegates: const [
          _TestStringsDelegate(),
          GlobalMaterialLocalizations.delegate,
          GlobalWidgetsLocalizations.delegate,
          GlobalCupertinoLocalizations.delegate,
        ],
        home: Scaffold(
          body: SizedBox(
            width: 900,
            height: 700,
            child: SchemaForm(
              params: [parameter],
              values: values,
              onChanged: () {},
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byKey(const ValueKey('files-paper-all')));
    await tester.pumpAndSettle();
    await tester.tap(find.text('A3').last);
    await tester.pump();
    expect(
      (values['files'] as List).map((item) => (item as Map)['option']),
      everyElement('A3'),
    );

    await tester.tap(find.byKey(const ValueKey('files-select-0')));
    await tester.tap(find.byKey(const ValueKey('files-select-2')));
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('files-paper-selected')));
    await tester.pumpAndSettle();
    await tester.tap(find.text('A5').last);
    await tester.pump();
    expect((values['files'] as List).map((item) => (item as Map)['option']), [
      'A5',
      'A3',
      'A5',
    ]);

    await tester.tap(find.byKey(const ValueKey('files-paper-1')));
    await tester.pumpAndSettle();
    await tester.tap(find.text('A4').last);
    await tester.pump();
    expect((values['files'] as List)[1]['option'], 'A4');
  });

  test('Gemini key is hidden until external AI is explicitly selected', () {
    final values = <String, dynamic>{'email_provider': 'offline'};
    final apiKey = ParamDef.fromJson({
      'name': 'gemini_api_key',
      'kind': 'secret',
      'labelKey': 'param.rfi.geminiApiKey',
      'required': false,
      'visibleWhen': {'email_provider': 'gemini'},
    });

    expect(isParamVisible(apiKey, values), isFalse);
    values['email_provider'] = 'gemini';
    expect(isParamVisible(apiKey, values), isTrue);
  });
}
