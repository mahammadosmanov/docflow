import 'package:flutter/material.dart';

import 'settings_store.dart';

/// Yazi tipi secenekleri.
///
/// TEK BIR font dosyasi paketlemiyoruz — her PC'de calismasi lazim ve indirilen
/// font hem boyut hem de kiril/turkce/azerice kapsama riski demek. Bunun yerine
/// her platformda VAR OLAN aileleri fallback zinciriyle veriyoruz: ilk bulunan
/// kullanilir, hicbiri yoksa Flutter sistem varsayilanina duser.
enum AppFont { system, sans, serif, mono }

extension AppFontSpec on AppFont {
  String get id => name;

  /// i18n anahtari — kural 4: kodda cıplak metin yok.
  String get labelKey => 'prefs.font.$name';

  /// Aile null ise Flutter platform varsayilanini kullanir.
  String? get family => switch (this) {
        AppFont.system => null,
        AppFont.sans => 'Helvetica Neue',
        AppFont.serif => 'Georgia',
        AppFont.mono => 'Menlo',
      };

  /// macOS / Windows / Linux sirasiyla denenir.
  List<String> get fallback => switch (this) {
        AppFont.system => const [],
        AppFont.sans => const ['Helvetica', 'Segoe UI', 'Arial', 'DejaVu Sans'],
        AppFont.serif => const ['Times New Roman', 'DejaVu Serif', 'Liberation Serif'],
        AppFont.mono => const ['Consolas', 'DejaVu Sans Mono', 'Courier New'],
      };
}

/// Kullanici tercihleri: dil, tema, yazi tipi, yazi boyutu, yogunluk.
///
/// ChangeNotifier — degisince MaterialApp aninda yeniden kurulur; ekranlarin
/// haber vermesi gerekmez. Diske ~/.docflow/settings.json icindeki 'prefs'
/// bolumune yazilir.
class AppPrefs extends ChangeNotifier {
  AppPrefs(this._store) {
    final s = _store.section(_section) ?? const {};
    // Elle duzenlenmis / eski surumden kalmis / bozuk settings.json uygulamayi
    // ACILISTA cokertmemeli: her alan tipini dogrula, uymayani varsayilana dus.
    // ("as String?" bir int gorunce firlatiyordu — test yakaladi.)
    _localeCode = _stringOr(s['locale'], null);
    _themeMode = _modeFrom(_stringOr(s['theme'], null));
    _font = _fontFrom(_stringOr(s['font'], null));
    _fontScale = _clampScale(s['fontScale'] is num ? (s['fontScale'] as num).toDouble() : 1.0);
    _compact = s['compact'] is bool ? s['compact'] as bool : false;
  }

  static String? _stringOr(dynamic v, String? fallback) =>
      v is String ? v : fallback;

  static const _section = 'prefs';

  /// Kullanici bunlarin disina cikamaz: 0.8'in altinda okunmuyor, 1.6'nin
  /// ustunde butonlar tasiyor.
  static const double minScale = 0.8;
  static const double maxScale = 1.6;

  final SettingsStore _store;

  String? _localeCode; // null = sistem dili
  ThemeMode _themeMode = ThemeMode.system;
  AppFont _font = AppFont.system;
  double _fontScale = 1.0;
  bool _compact = false;

  String? get localeCode => _localeCode;
  Locale? get locale => _localeCode == null ? null : Locale(_localeCode!);
  ThemeMode get themeMode => _themeMode;
  AppFont get font => _font;
  double get fontScale => _fontScale;
  bool get compact => _compact;

  static double _clampScale(double v) => v.clamp(minScale, maxScale);

  static ThemeMode _modeFrom(String? s) => switch (s) {
        'light' => ThemeMode.light,
        'dark' => ThemeMode.dark,
        _ => ThemeMode.system,
      };

  static AppFont _fontFrom(String? s) =>
      AppFont.values.firstWhere((f) => f.id == s, orElse: () => AppFont.system);

  Future<void> setLocale(String? code) async {
    if (_localeCode == code) return;
    _localeCode = code;
    await _persist();
  }

  Future<void> setThemeMode(ThemeMode mode) async {
    if (_themeMode == mode) return;
    _themeMode = mode;
    await _persist();
  }

  Future<void> setFont(AppFont f) async {
    if (_font == f) return;
    _font = f;
    await _persist();
  }

  Future<void> setFontScale(double v) async {
    final next = _clampScale(v);
    if (_fontScale == next) return;
    _fontScale = next;
    await _persist();
  }

  Future<void> setCompact(bool v) async {
    if (_compact == v) return;
    _compact = v;
    await _persist();
  }

  /// Her sey varsayilana — "bozdum, geri al" kacis kapisi.
  Future<void> reset() async {
    _localeCode = null;
    _themeMode = ThemeMode.system;
    _font = AppFont.system;
    _fontScale = 1.0;
    _compact = false;
    await _persist();
  }

  Future<void> _persist() async {
    notifyListeners(); // UI'yi bekletme; disk yazimi arkada.
    await _store.saveSection(_section, {
      'locale': _localeCode,
      'theme': _themeMode.name,
      'font': _font.id,
      'fontScale': _fontScale,
      'compact': _compact,
    });
  }
}
