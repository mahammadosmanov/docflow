import 'dart:convert';
import 'dart:io';

import 'package:docflow/l10n/generate_catalog.dart';
import 'package:docflow/l10n/message_catalog.dart';
import 'package:docflow/l10n/strings.dart';
import 'package:docflow/models/models.dart';
import 'package:flutter_test/flutter_test.dart';

const String _frontendLocales = 'tool/locales';

RegExp _placeholder = RegExp(r'\{[^{}]+\}');

void main() {
  test('all four locales carry the same keys and placeholders', () {
    final en = messageCatalog['en']!;
    expect(messageCatalog.keys.toSet(), {'az', 'en', 'ru', 'tr'});
    for (final locale in ['az', 'ru', 'tr']) {
      final keys = messageCatalog[locale]!;
      expect(keys.keys.toSet(), en.keys.toSet(), reason: locale);
      for (final key in en.keys) {
        expect(
          _placeholder.allMatches(keys[key]!).map((m) => m.group(0)).toSet(),
          _placeholder.allMatches(en[key]!).map((m) => m.group(0)).toSet(),
          reason: '$locale $key',
        );
      }
    }
  });

  test(
    'generated catalog and gen-l10n inputs match the frontend ARB files',
    () {
      final catalogs = readCatalogs(_frontendLocales);
      expect(
        File('lib/l10n/message_catalog.dart').readAsStringSync(),
        renderMessageCatalog(catalogs),
        reason: 'run: dart run lib/l10n/generate_catalog.dart',
      );
      const encoder = JsonEncoder.withIndent('  ');
      for (final locale in catalogLocales) {
        final arb = File('lib/l10n/app_$locale.arb').readAsStringSync();
        final generated = toGenL10nArb(catalogs[locale]!, locale);
        expect(
          arb,
          '${encoder.convert(generated)}\n',
          reason: 'run: dart run lib/l10n/generate_catalog.dart',
        );
      }
    },
  );

  test('every catalog key has a typed gen-l10n accessor', () {
    final generated = File('lib/l10n/app_localizations.dart')
        .readAsStringSync();
    for (final key in messageCatalog['en']!.keys) {
      final identifier = identifierForKey(key);
      expect(
        generated.contains(RegExp('[ ]$identifier\\(|[ ]$identifier;')),
        isTrue,
        reason: '$key -> $identifier',
      );
    }
  });

  test('identifierForKey maps dotted keys to camelCase names', () {
    expect(identifierForKey('app.title'), 'appTitle');
    expect(
      identifierForKey('op.rfi.process_scans.title'),
      'opRfiProcessScansTitle',
    );
    expect(identifierForKey('choice.print.paper.A4'), 'choicePrintPaperA4');
    expect(() => identifierForKey('Bad.key'), throwsFormatException);
  });

  test('Strings.t substitutes placeholders and falls back to the key', () {
    final en = makeStrings(AppLocale.en);
    expect(en.t('result.applied', {'count': 3}), '3 applied');
    expect(en.t('no.such.key'), 'no.such.key');
    expect(en.locale, AppLocale.en);
  });

  test('a null preference follows the system language', () {
    expect(makeStrings(null, systemLanguage: 'ru-RU').locale, AppLocale.ru);
    expect(makeStrings(null, systemLanguage: 'az').locale, AppLocale.az);
    expect(makeStrings(null, systemLanguage: 'de').locale, AppLocale.tr);
    expect(systemLocale(language: 'EN-us'), AppLocale.en);
  });

  test('the explicit preference wins over the system language', () {
    final strings = makeStrings(AppLocale.tr, systemLanguage: 'ru');
    expect(strings.locale, AppLocale.tr);
    expect(strings.t('app.title'), 'DocFlow');
  });
}
