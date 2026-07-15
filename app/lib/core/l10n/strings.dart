import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

/// Runtime i18n. Anahtarlar noktali ("op.rfi.process_scans.title") ve sidecar
/// tarafindan string olarak donuyor -> gen-l10n getter'lari uygun degil.
/// Dort ARB dosyasini asset olarak yukleyip anahtar->metin cozuyoruz.
///
/// Kural 4 (CLAUDE.md): kullaniciya gorunen her string bir ARB anahtaridir.
/// Cozulemeyen anahtar = ekranda anahtarin kendisi (sessiz bosluk yok).
class Strings {
  Strings(this._map);

  final Map<String, dynamic> _map;

  static const List<Locale> supported = [
    Locale('tr'),
    Locale('en'),
    Locale('ru'),
    Locale('az'),
  ];

  String t(String key) {
    final v = _map[key];
    if (v is String) return v;
    if (kDebugMode) debugPrint('i18n: eksik anahtar "$key"');
    return key;
  }

  /// {count} gibi ICU placeholder'i basitce doldurur (MVP icin yeterli).
  String count(String key, int n) => t(key).replaceAll('{count}', '$n');

  static Future<Strings> load(Locale locale) async {
    final code = supported.any((l) => l.languageCode == locale.languageCode)
        ? locale.languageCode
        : 'en';
    final raw = await rootBundle.loadString('l10n/app_$code.arb');
    final map = json.decode(raw) as Map<String, dynamic>;
    return Strings(map);
  }

  static Strings of(BuildContext context) =>
      Localizations.of<Strings>(context, Strings)!;
}

class StringsDelegate extends LocalizationsDelegate<Strings> {
  const StringsDelegate();

  @override
  bool isSupported(Locale locale) => true; // fallback -> en, load() halleder

  @override
  Future<Strings> load(Locale locale) => Strings.load(locale);

  @override
  bool shouldReload(StringsDelegate old) => false;
}
