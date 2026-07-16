import 'package:flutter/material.dart';

import '../../core/auth/auth_service.dart';
import '../../core/l10n/strings.dart';
import '../../core/settings/app_prefs.dart';
import '../theme/responsive.dart';

/// Ayarlar: hesap, dil, tema, yazi tipi, yazi boyutu, yogunluk.
///
/// Tercihler ANINDA uygulanir ve diske yazilir — "Kaydet" butonu yok, kullanici
/// sonucu canli gorur. Kullanici adi istisna: sunucuya gider, dogrulanir ve
/// reddedilebilir; o yuzden acik bir Kaydet butonu var.
class SettingsScreen extends StatelessWidget {
  const SettingsScreen({super.key, required this.prefs, this.auth});

  final AppPrefs prefs;

  /// Giris ekranindan acilinca null — o an hesap yok, Hesap bolumu gizlenir.
  final AuthService? auth;

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
                  if (auth != null && auth!.signedIn)
                    Section(
                      title: s.t('prefs.account'),
                      child: _AccountSection(auth: auth!),
                    ),
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

/// Hesap: kullanici adini degistir; e-posta salt-okunur.
///
/// E-posta neden degistirilemiyor: degistirmek dogrulama akisi (eski+yeni adrese
/// mail) gerektirir ve yanlis yapilirsa hesabi erisilmez birakir. Ihtiyac
/// olursa ayri is.
class _AccountSection extends StatefulWidget {
  const _AccountSection({required this.auth});

  final AuthService auth;

  @override
  State<_AccountSection> createState() => _AccountSectionState();
}

class _AccountSectionState extends State<_AccountSection> {
  late final TextEditingController _c =
      TextEditingController(text: widget.auth.me?.username ?? '');
  bool _busy = false;
  String? _error;
  bool _saved = false;

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  bool get _dirty => _c.text.trim() != (widget.auth.me?.username ?? '');
  bool get _canSave => _dirty && UsernameRule.ok(_c.text) && !_busy;

  Future<void> _save() async {
    setState(() {
      _busy = true;
      _error = null;
      _saved = false;
    });
    try {
      await widget.auth.setUsername(_c.text);
      if (mounted) setState(() => _saved = true);
    } catch (e) {
      if (!mounted) return; // await sonrasi context kullanmadan ONCE kontrol et
      final raw = e.toString();
      final s = Strings.of(context);
      setState(() => _error = raw.contains('username_taken')
          ? s.t('auth.err.usernameTaken')
          : raw.contains('username_invalid')
              ? s.t('auth.err.usernameInvalid')
              : raw);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final s = Strings.of(context);
    final t = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 14, 16, 14),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              const Icon(Icons.alternate_email, size: 16),
              const SizedBox(width: 8),
              Expanded(
                child: Text(widget.auth.session?.email ?? '',
                    style: t.textTheme.bodySmall
                        ?.copyWith(color: t.colorScheme.onSurfaceVariant)),
              ),
              Chip(label: Text(s.t('prefs.account.emailFixed'))),
            ],
          ),
          const SizedBox(height: 14),
          TextField(
            controller: _c,
            enabled: !_busy,
            decoration: InputDecoration(
              labelText: s.t('auth.username'),
              helperText: s.t('auth.usernameHelp'),
              prefixIcon: const Icon(Icons.badge_outlined, size: 18),
            ),
            onChanged: (_) => setState(() => _saved = false),
            onSubmitted: (_) => _canSave ? _save() : null,
          ),
          if (_error != null) ...[
            const SizedBox(height: 8),
            Row(
              children: [
                const Icon(Icons.error_outline, size: 16),
                const SizedBox(width: 6),
                Expanded(
                  child: Text(_error!,
                      style: t.textTheme.bodySmall
                          ?.copyWith(fontWeight: FontWeight.w600)),
                ),
              ],
            ),
          ],
          const SizedBox(height: 12),
          Row(
            children: [
              if (_saved && !_dirty)
                Row(
                  children: [
                    const Icon(Icons.check_circle_outline, size: 16),
                    const SizedBox(width: 6),
                    Text(s.t('prefs.account.saved'), style: t.textTheme.bodySmall),
                  ],
                ),
              const Spacer(),
              FilledButton(
                onPressed: _canSave ? _save : null,
                child: _busy
                    ? const SizedBox(
                        height: 16,
                        width: 16,
                        child: CircularProgressIndicator(strokeWidth: 2))
                    : Text(s.t('action.save')),
              ),
            ],
          ),
        ],
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
