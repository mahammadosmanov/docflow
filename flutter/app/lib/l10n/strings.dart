// Runtime string lookup, mirroring frontend/src/i18n.ts.
//
// Engine and frontend code refer to text by dotted key (`op.rfi.title`) and
// interpolate `{name}` placeholders. [Strings.t] does the same as `t()` there:
// a missing key renders as the key itself, so text is never silently blank.
//
// Typed access from Flutter code uses the gen-l10n class `AppLocalizations`
// (lib/l10n/app_localizations.dart). Both read the catalogs generated from
// frontend/src/locales/*.arb (see generate_catalog.dart).

import 'dart:ui' show PlatformDispatcher;

import 'package:flutter/widgets.dart' show Locale;

import '../models/prefs.dart' show AppLocale;
import 'message_catalog.dart';

/// Mirrors `systemLocale()`: the system language when it is az, en or ru, and
/// Turkish otherwise. [language] overrides the platform language, for tests.
AppLocale systemLocale({String? language}) {
  final code = (language ?? PlatformDispatcher.instance.locale.languageCode)
      .toLowerCase()
      .split('-')
      .first;
  return switch (code) {
    'az' => AppLocale.az,
    'en' => AppLocale.en,
    'ru' => AppLocale.ru,
    _ => AppLocale.tr,
  };
}

/// Translation handle for one locale (the `Strings` type in i18n.ts).
final class Strings {
  Strings._(this.locale, this._messages);

  /// The locale actually in use: the preference, or the system locale.
  final AppLocale locale;

  final Map<String, String> _messages;

  /// Returns the message for [key] with `{name}` placeholders replaced by
  /// [values]. Returns [key] when no message exists.
  String t(String key, [Map<String, Object> values = const {}]) {
    var result = _messages[key] ?? key;
    for (final entry in values.entries) {
      result = result.replaceAll('{${entry.key}}', '${entry.value}');
    }
    return result;
  }
}

/// Mirrors `makeStrings(preference)`. A null [preference] follows the system
/// language. Locales with no catalog fall back to Turkish, as in i18n.ts.
Strings makeStrings(AppLocale? preference, {String? systemLanguage}) {
  final locale = preference ?? systemLocale(language: systemLanguage);
  final messages = messageCatalog[locale.name] ?? messageCatalog['tr']!;
  return Strings._(locale, messages);
}

/// The Flutter [Locale] for an app locale. Material and Cupertino widgets use
/// it through the gen-l10n delegates.
Locale flutterLocaleFor(AppLocale locale) => Locale(locale.name);
