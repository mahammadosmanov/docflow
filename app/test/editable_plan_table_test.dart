import 'package:docflow/core/l10n/strings.dart';
import 'package:docflow/ui/forms/editable_plan_table.dart';
import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('ayni RFI satiri cogaltilir ve yalniz kopya silinebilir', (
    tester,
  ) async {
    final rows = <Map<String, dynamic>>[
      {
        '_rowId': 'row-1',
        'rfiNo': 'AZE-SUN-INT-QMS-RFI-CIV-0860',
        'area': 'North Site',
      },
    ];
    final editor = <String, dynamic>{
      'kind': 'recordTable',
      'groupField': 'rfiNo',
      'columns': [
        {
          'key': 'rfiNo',
          'labelKey': 'rfi.col.rfiNo',
          'kind': 'readonly',
          'width': 240,
        },
        {
          'key': 'area',
          'labelKey': 'rfi.col.area',
          'kind': 'text',
          'width': 160,
        },
      ],
      'duplicateEnabled': true,
    };
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
            width: 900,
            height: 500,
            child: EditablePlanTable(
              editor: editor,
              rows: rows,
              onChanged: () => changes++,
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(rows, hasLength(1));
    expect(
      tester
          .widget<IconButton>(
            find.widgetWithIcon(IconButton, Icons.remove_circle_outline),
          )
          .onPressed,
      isNull,
    );

    await tester.tap(find.byIcon(Icons.add_to_photos_outlined));
    await tester.pump();
    expect(rows, hasLength(2));
    expect(rows[1]['rfiNo'], rows[0]['rfiNo']);
    expect(rows[1]['area'], 'North Site');
    expect(changes, 1);

    final removeButtons = find.byIcon(Icons.remove_circle_outline);
    await tester.tap(removeButtons.last);
    await tester.pump();
    expect(rows, hasLength(1));
    expect(changes, 2);
  });

  test('Tag No degisince otomatik uretilen N ve S alanlari guncellenir', () {
    final row = <String, dynamic>{'tagNo': 'PCU-N12', 'area': ''};
    final editor = <String, dynamic>{
      'regexFill': [
        {
          'source': 'tagNo',
          'target': 'area',
          'pattern': r'(?:^|[^A-Z0-9])N\d{1,3}(?:[^A-Z0-9]|$)',
          'value': 'North Site',
        },
        {
          'source': 'tagNo',
          'target': 'area',
          'pattern': r'(?:^|[^A-Z0-9])S\d{1,3}(?:[^A-Z0-9]|$)',
          'value': 'South Site',
        },
      ],
    };

    expect(applyEditorRegexFill(editor, row, 'tagNo'), isTrue);
    expect(row['area'], 'North Site');

    row['tagNo'] = 'PCU-S12';
    expect(applyEditorRegexFill(editor, row, 'tagNo'), isTrue);
    expect(row['area'], 'South Site');
  });

  testWidgets('compact RFI editor shows suffixes and keeps auto fields editable', (
    tester,
  ) async {
    final rows = <Map<String, dynamic>>[
      {
        '_rowId': 'row-compact',
        'rfiNo': 'AZE-SUN-INT-QMS-RFI-CIV-0860',
        'itpNo': 'AZE-SUN-INT-QMS-ITP-CIV-0005',
        'itemNo': 'CIV-5.2.1',
        'description': 'AUTOMATIC DESCRIPTION',
      },
    ];
    final editor = <String, dynamic>{
      'kind': 'recordTable',
      'layout': 'compact',
      'groupField': 'rfiNo',
      'columns': [
        {
          'key': 'rfiNo',
          'labelKey': 'rfi.col.rfiNo',
          'kind': 'readonly',
          'width': 72,
          'compact': true,
          'display': 'suffix',
        },
        {
          'key': 'itpNo',
          'labelKey': 'rfi.col.itpNo',
          'kind': 'choice',
          'width': 72,
          'compact': true,
          'display': 'suffix',
        },
        {
          'key': 'itemNo',
          'labelKey': 'rfi.col.itemNo',
          'kind': 'text',
          'width': 100,
          'compact': true,
        },
        {
          'key': 'description',
          'labelKey': 'rfi.col.description',
          'kind': 'multiline',
          'detail': true,
        },
      ],
      'choices': {
        'itpNo': ['AZE-SUN-INT-QMS-ITP-CIV-0005'],
      },
    };

    expect(
      planTableDisplayValue(
        (editor['columns'] as List).first as Map<String, dynamic>,
        rows.first['rfiNo'] as String,
      ),
      '0860',
    );
    expect(
      planTableDisplayValue(
        (editor['columns'] as List)[1] as Map<String, dynamic>,
        rows.first['itpNo'] as String,
      ),
      '0005',
    );

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
            width: 1200,
            height: 420,
            child: EditablePlanTable(
              editor: editor,
              rows: rows,
              onChanged: () {},
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('AZE-SUN-INT-QMS-RFI-CIV-0860'), findsNothing);
    expect(find.text('AUTOMATIC DESCRIPTION'), findsNothing);

    await tester.tap(find.byIcon(Icons.tune_outlined));
    await tester.pumpAndSettle();

    expect(find.text('AUTOMATIC DESCRIPTION'), findsOneWidget);
  });
}
