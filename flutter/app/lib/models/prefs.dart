import 'json.dart';

/// Interface languages (`AppPrefs.locale`). `null` in AppPrefs means "follow the
/// system language".
enum AppLocale {
  tr,
  en,
  ru,
  az;

  static AppLocale fromWire(String value) => AppLocale.values.firstWhere(
    (locale) => locale.name == value,
    orElse: () => throw FormatException('Unknown locale "$value"'),
  );
}

enum ThemePreference {
  system,
  light,
  dark;

  static ThemePreference fromWire(String value) =>
      ThemePreference.values.firstWhere(
        (theme) => theme.name == value,
        orElse: () => throw FormatException('Unknown theme "$value"'),
      );
}

enum FontPreference {
  system,
  sans,
  serif,
  mono;

  static FontPreference fromWire(String value) =>
      FontPreference.values.firstWhere(
        (font) => font.name == value,
        orElse: () => throw FormatException('Unknown font "$value"'),
      );
}

/// User preferences (`AppPrefs`).
final class AppPrefs {
  const AppPrefs({
    required this.locale,
    required this.theme,
    required this.font,
    required this.fontScale,
    required this.compact,
    required this.startWithWindows,
    required this.replicaSyncEnabled,
  });

  /// Defaults used by the browser demo bridge and before the first load.
  static const AppPrefs defaults = AppPrefs(
    locale: AppLocale.tr,
    theme: ThemePreference.system,
    font: FontPreference.system,
    fontScale: 1,
    compact: false,
    startWithWindows: false,
    replicaSyncEnabled: false,
  );

  factory AppPrefs.fromJson(JsonMap json) => AppPrefs(
    locale: json['locale'] == null
        ? null
        : AppLocale.fromWire(jsonString(json, 'locale')),
    theme: ThemePreference.fromWire(jsonString(json, 'theme')),
    font: FontPreference.fromWire(jsonString(json, 'font')),
    fontScale: jsonDouble(json, 'fontScale'),
    compact: jsonBool(json, 'compact'),
    startWithWindows: jsonBool(json, 'startWithWindows'),
    replicaSyncEnabled: jsonBool(json, 'replicaSyncEnabled'),
  );

  final AppLocale? locale;
  final ThemePreference theme;
  final FontPreference font;
  final double fontScale;
  final bool compact;
  final bool startWithWindows;
  final bool replicaSyncEnabled;

  JsonMap toJson() => {
    'locale': locale?.name,
    'theme': theme.name,
    'font': font.name,
    'fontScale': fontScale,
    'compact': compact,
    'startWithWindows': startWithWindows,
    'replicaSyncEnabled': replicaSyncEnabled,
  };
}

/// Partial update of [AppPrefs] (`Partial<AppPrefs>`). Only set fields are
/// sent. `locale` needs [clearLocale] because `null` is a real value there
/// (follow the system language).
final class AppPrefsPatch {
  const AppPrefsPatch({
    this.locale,
    this.clearLocale = false,
    this.theme,
    this.font,
    this.fontScale,
    this.compact,
    this.startWithWindows,
    this.replicaSyncEnabled,
  });

  final AppLocale? locale;
  final bool clearLocale;
  final ThemePreference? theme;
  final FontPreference? font;
  final double? fontScale;
  final bool? compact;
  final bool? startWithWindows;
  final bool? replicaSyncEnabled;

  JsonMap toJson() {
    final json = withoutNulls({
      'locale': locale?.name,
      'theme': theme?.name,
      'font': font?.name,
      'fontScale': fontScale,
      'compact': compact,
      'startWithWindows': startWithWindows,
      'replicaSyncEnabled': replicaSyncEnabled,
    });
    if (clearLocale) json['locale'] = null;
    return json;
  }
}
