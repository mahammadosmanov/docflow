import 'package:flutter/material.dart';

import '../../core/l10n/strings.dart';
import '../../core/settings/app_prefs.dart';
import '../theme/responsive.dart';

/// Ayarlar: dil, tema, yazi tipi, yazi boyutu, yogunluk.
///
/// Her degisiklik ANINDA uygulanir ve diske yazilir — "Kaydet" butonu yok,
/// kullanici sonucu canli gorur. Geri donus icin "Varsayilanlara don" var.
class SettingsScreen extends StatelessWidget {
  const SettingsScreen({super.key, required this.prefs});

  final AppPrefs prefs;

  static const _locales = <String?, String>{
    null: 'prefs.lang.system',
    'tr': 'prefs.lang.tr',
    'en': 'prefs.lang.en',
    'ru': 'prefs.lang.ru',
    'az': 'prefs.lang.az',
  };

  @override
  Widget build(BuildContext context) {
    final s = Strings.of(context);
    return Scaffold(
      appBar: AppBar(title: Text(s.t('prefs.title'))),
      body: ListenableBuilder(
        listenable: prefs,
        builder: (context, _) => ListView(
          children: [
            Pane(
              maxWidth: 640,
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Section(
                    title: s.t('prefs.language'),
                    child: RadioGroup<String?>(
                      groupValue: prefs.localeCode,
                      onChanged: (v) => prefs.setLocale(v),
                      child: Column(
                        children: [
                          for (final e in _locales.entries)
                            RadioListTile<String?>(
                              value: e.key,
                              title: Text(s.t(e.value)),
                              dense: true,
                            ),
                        ],
                      ),
                    ),
                  ),
                  Section(
                    title: s.t('prefs.theme'),
                    child: Padding(
                      padding: const EdgeInsets.all(12),
                      child: SegmentedButton<ThemeMode>(
                        segments: [
                          ButtonSegment(
                            value: ThemeMode.system,
                            icon: const Icon(Icons.brightness_auto_outlined),
                            label: Text(s.t('prefs.theme.system')),
                          ),
                          ButtonSegment(
                            value: ThemeMode.light,
                            icon: const Icon(Icons.light_mode_outlined),
                            label: Text(s.t('prefs.theme.light')),
                          ),
                          ButtonSegment(
                            value: ThemeMode.dark,
                            icon: const Icon(Icons.dark_mode_outlined),
                            label: Text(s.t('prefs.theme.dark')),
                          ),
                        ],
                        selected: {prefs.themeMode},
                        onSelectionChanged: (v) => prefs.setThemeMode(v.first),
                      ),
                    ),
                  ),
                  Section(
                    title: s.t('prefs.font'),
                    subtitle: s.t('prefs.font.help'),
                    child: RadioGroup<AppFont>(
                      groupValue: prefs.font,
                      onChanged: (v) => prefs.setFont(v!),
                      child: Column(
                        children: [
                          for (final f in AppFont.values)
                            RadioListTile<AppFont>(
                              value: f,
                              dense: true,
                              // Etiketi kendi fontuyla goster — secmeden once gor.
                              title: Text(
                                s.t(f.labelKey),
                                style: TextStyle(
                                  fontFamily: f.family,
                                  fontFamilyFallback: f.fallback,
                                ),
                              ),
                            ),
                        ],
                      ),
                    ),
                  ),
                  Section(
                    title: s.t('prefs.fontSize'),
                    child: _FontSizeControl(prefs: prefs, s: s),
                  ),
                  Section(
                    title: s.t('prefs.density'),
                    child: SwitchListTile(
                      value: prefs.compact,
                      onChanged: (v) => prefs.setCompact(v),
                      title: Text(s.t('prefs.density.compact')),
                      subtitle: Text(s.t('prefs.density.help')),
                    ),
                  ),
                  Align(
                    alignment: Alignment.centerLeft,
                    child: OutlinedButton.icon(
                      icon: const Icon(Icons.restart_alt, size: 18),
                      label: Text(s.t('prefs.reset')),
                      onPressed: prefs.reset,
                    ),
                  ),
                  const SizedBox(height: 24),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _FontSizeControl extends StatelessWidget {
  const _FontSizeControl({required this.prefs, required this.s});

  final AppPrefs prefs;
  final Strings s;

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 12, 16, 8),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              const Icon(Icons.text_fields, size: 16),
              const Spacer(),
              Text('${(prefs.fontScale * 100).round()}%',
                  style: t.textTheme.labelLarge),
              const Spacer(),
              const Icon(Icons.text_fields, size: 26),
            ],
          ),
          Slider(
            value: prefs.fontScale,
            min: AppPrefs.minScale,
            max: AppPrefs.maxScale,
            // 0.05 adim: 0.8..1.6 arasi 16 durak.
            divisions: 16,
            label: '${(prefs.fontScale * 100).round()}%',
            onChanged: (v) => prefs.setFontScale(v),
          ),
          // Canli ornek: kullanici uygulamadan cikmadan sonucu gorsun.
          Container(
            width: double.infinity,
            margin: const EdgeInsets.only(top: 4, bottom: 8),
            padding: const EdgeInsets.all(12),
            decoration: BoxDecoration(
              color: t.colorScheme.surfaceContainer,
              borderRadius: BorderRadius.circular(8),
            ),
            child: Text(s.t('prefs.fontSize.sample'), style: t.textTheme.bodyMedium),
          ),
        ],
      ),
    );
  }
}
