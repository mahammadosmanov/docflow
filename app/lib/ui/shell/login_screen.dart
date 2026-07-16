import 'dart:async';

import 'package:flutter/material.dart';

import '../../core/auth/auth_service.dart';
import '../../core/l10n/strings.dart';
import '../../core/settings/app_prefs.dart';
import '../theme/responsive.dart';
import 'settings_screen.dart';

enum _Mode { signIn, signUp }

/// Giris / kayit. Parola SADECE kullanici tarafindan girilir ve diske yazilmaz;
/// yalnizca donen oturum token'i saklanir.
///
/// Giriste e-posta VEYA kullanici adi kabul edilir (cozumleme AuthService'te).
class LoginScreen extends StatefulWidget {
  const LoginScreen({super.key, required this.auth, required this.prefs});

  final AuthService auth;
  final AppPrefs prefs;

  @override
  State<LoginScreen> createState() => _LoginScreenState();
}

class _LoginScreenState extends State<LoginScreen> {
  final _identifier = TextEditingController(); // giris: e-posta VEYA kullanici adi
  final _email = TextEditingController(); // kayit: e-posta
  final _username = TextEditingController(); // kayit: kullanici adi
  final _password = TextEditingController();

  _Mode _mode = _Mode.signIn;
  bool _busy = false;
  bool _obscure = true;
  String? _error;
  String? _info;

  // Kullanici adi musaitlik kontrolu (yazarken, gecikmeli).
  Timer? _debounce;
  bool _checking = false;
  bool? _available;

  @override
  void dispose() {
    _debounce?.cancel();
    _identifier.dispose();
    _email.dispose();
    _username.dispose();
    _password.dispose();
    super.dispose();
  }

  bool get _passwordOk => _password.text.length >= 6;

  bool get _valid => switch (_mode) {
        _Mode.signIn => _identifier.text.trim().isNotEmpty && _passwordOk,
        _Mode.signUp => _email.text.trim().contains('@') &&
            UsernameRule.ok(_username.text) &&
            _available != false &&
            _passwordOk,
      };

  void _onUsernameChanged(String v) {
    setState(() => _available = null);
    _debounce?.cancel();
    if (!UsernameRule.ok(v)) return;
    // Her tusa basista sunucuya gitme; kullanici duraklayinca sor.
    _debounce = Timer(const Duration(milliseconds: 450), () async {
      setState(() => _checking = true);
      final ok = await widget.auth.usernameAvailable(v);
      if (mounted && _username.text == v) {
        setState(() {
          _available = ok;
          _checking = false;
        });
      } else if (mounted) {
        setState(() => _checking = false);
      }
    });
  }

  Future<void> _run(Future<void> Function() action, {String? info}) async {
    setState(() {
      _busy = true;
      _error = null;
      _info = null;
    });
    try {
      await action();
      // Basarili signIn -> AuthService dinleyicileri uyarir -> AuthGate bu ekrani
      // sokup uygulamayi acar; buradaki setState calismaz (mounted false).
      if (!mounted) return;
      setState(() => _info = info);
    } catch (e) {
      if (mounted) setState(() => _error = _message(e));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// Sunucu/istisna mesajini i18n anahtarina cevir; bilinmeyeni ham goster.
  ///
  /// Ham GoTrue metni INGILIZCE gelir — kullaniciya oyle gostermek kural 4'u
  /// deler. Bilinen durumlari burada cevir; bilinmeyeni gizleme (sessiz hata,
  /// hic hata gostermemekten beter).
  String _message(Object e) {
    final raw = e.toString();
    final s = Strings.of(context);
    if (raw.contains('invalid_credentials') || raw.contains('Invalid login')) {
      return s.t('auth.err.credentials');
    }
    // E-posta dogrulama acikken: hesap VAR ama giris yapilamaz. Bunu "parola
    // yanlis" diye gostermek kullaniciyi saatlerce yanlis yere baktirir.
    if (raw.contains('email_not_confirmed') || raw.contains('Email not confirmed')) {
      return s.t('auth.err.emailNotConfirmed');
    }
    if (raw.contains('over_email_send_rate_limit') ||
        raw.contains('rate limit') ||
        raw.contains('too many requests')) {
      return s.t('auth.err.rateLimit');
    }
    if (raw.contains('username_taken')) return s.t('auth.err.usernameTaken');
    if (raw.contains('username_invalid')) return s.t('auth.err.usernameInvalid');
    if (raw.contains('User already registered') ||
        raw.contains('user_already_exists')) {
      return s.t('auth.err.emailTaken');
    }
    if (raw.contains('weak_password') || raw.contains('Password should be')) {
      return s.t('auth.err.weakPassword');
    }
    return raw;
  }

  void _submit() {
    if (!_valid || _busy) return;
    if (_mode == _Mode.signIn) {
      _run(() => widget.auth.signIn(_identifier.text, _password.text));
    } else {
      _run(
        () => widget.auth
            .signUp(_email.text.trim(), _password.text, _username.text.trim()),
        info: Strings.of(context).t('auth.signupOk'),
      );
    }
  }

  void _switchMode(_Mode m) => setState(() {
        _mode = m;
        _error = null;
        _info = null;
      });

  @override
  Widget build(BuildContext context) {
    final s = Strings.of(context);
    final t = Theme.of(context);
    return Scaffold(
      appBar: AppBar(
        automaticallyImplyLeading: false,
        title: const SizedBox.shrink(),
        actions: [
          // Dili/temayi giris yapmadan da degistirebilmeli — yanlis dilde
          // kilitli kalan kullanici giris bile yapamaz.
          IconButton(
            icon: const Icon(Icons.tune),
            tooltip: s.t('prefs.title'),
            onPressed: () => Navigator.of(context).push(MaterialPageRoute(
              builder: (_) => SettingsScreen(prefs: widget.prefs),
            )),
          ),
        ],
      ),
      body: Center(
        child: SingleChildScrollView(
          child: Pane(
            maxWidth: 400,
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Icon(Icons.description_outlined,
                    size: 40, color: t.colorScheme.onSurface),
                const SizedBox(height: 14),
                Text(s.t('app.title'),
                    textAlign: TextAlign.center,
                    style: t.textTheme.headlineSmall
                        ?.copyWith(fontWeight: FontWeight.w700, letterSpacing: -0.5)),
                const SizedBox(height: 6),
                Text(s.t(_mode == _Mode.signIn ? 'auth.subtitle' : 'auth.subtitleUp'),
                    textAlign: TextAlign.center,
                    style: t.textTheme.bodySmall
                        ?.copyWith(color: t.colorScheme.onSurfaceVariant)),
                const SizedBox(height: 24),
                SegmentedButton<_Mode>(
                  segments: [
                    ButtonSegment(value: _Mode.signIn, label: Text(s.t('auth.signIn'))),
                    ButtonSegment(value: _Mode.signUp, label: Text(s.t('auth.signUp'))),
                  ],
                  selected: {_mode},
                  onSelectionChanged: _busy ? null : (v) => _switchMode(v.first),
                ),
                const SizedBox(height: 20),
                if (_mode == _Mode.signIn)
                  TextField(
                    controller: _identifier,
                    autofocus: true,
                    enabled: !_busy,
                    textInputAction: TextInputAction.next,
                    decoration: InputDecoration(
                      labelText: s.t('auth.identifier'),
                      helperText: s.t('auth.identifierHelp'),
                      prefixIcon: const Icon(Icons.person_outline, size: 18),
                    ),
                    onChanged: (_) => setState(() {}),
                  )
                else ...[
                  TextField(
                    controller: _email,
                    autofocus: true,
                    enabled: !_busy,
                    keyboardType: TextInputType.emailAddress,
                    textInputAction: TextInputAction.next,
                    decoration: InputDecoration(
                      labelText: s.t('auth.email'),
                      prefixIcon: const Icon(Icons.alternate_email, size: 18),
                    ),
                    onChanged: (_) => setState(() {}),
                  ),
                  const SizedBox(height: 12),
                  TextField(
                    controller: _username,
                    enabled: !_busy,
                    textInputAction: TextInputAction.next,
                    decoration: InputDecoration(
                      labelText: s.t('auth.username'),
                      helperText: s.t('auth.usernameHelp'),
                      prefixIcon: const Icon(Icons.badge_outlined, size: 18),
                      suffixIcon: _UsernameStatus(
                        checking: _checking,
                        available: _available,
                        valid: UsernameRule.ok(_username.text),
                        empty: _username.text.isEmpty,
                      ),
                    ),
                    onChanged: (v) {
                      _onUsernameChanged(v);
                      setState(() {});
                    },
                  ),
                  if (_available == false)
                    Padding(
                      padding: const EdgeInsets.only(top: 6, left: 4),
                      child: Text(s.t('auth.err.usernameTaken'),
                          style: t.textTheme.bodySmall
                              ?.copyWith(fontWeight: FontWeight.w600)),
                    ),
                ],
                const SizedBox(height: 12),
                TextField(
                  controller: _password,
                  obscureText: _obscure,
                  enabled: !_busy,
                  decoration: InputDecoration(
                    labelText: s.t('auth.password'),
                    helperText: s.t('auth.passwordHint'),
                    prefixIcon: const Icon(Icons.lock_outline, size: 18),
                    suffixIcon: IconButton(
                      icon: Icon(
                        _obscure
                            ? Icons.visibility_outlined
                            : Icons.visibility_off_outlined,
                        size: 18,
                      ),
                      tooltip: s.t(_obscure ? 'auth.show' : 'auth.hide'),
                      onPressed: () => setState(() => _obscure = !_obscure),
                    ),
                  ),
                  onChanged: (_) => setState(() {}),
                  onSubmitted: (_) => _submit(),
                ),
                if (_error != null) ...[
                  const SizedBox(height: 14),
                  _Note(icon: Icons.error_outline, message: _error!, strong: true),
                ],
                if (_info != null) ...[
                  const SizedBox(height: 14),
                  _Note(icon: Icons.check_circle_outline, message: _info!),
                ],
                const SizedBox(height: 20),
                FilledButton(
                  onPressed: _busy || !_valid ? null : _submit,
                  child: _busy
                      ? const SizedBox(
                          height: 18,
                          width: 18,
                          child: CircularProgressIndicator(strokeWidth: 2))
                      : Text(s.t(_mode == _Mode.signIn ? 'auth.signIn' : 'auth.signUp')),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// Kullanici adi durumu — monokrom: ikon sekli anlatir, renk degil.
class _UsernameStatus extends StatelessWidget {
  const _UsernameStatus({
    required this.checking,
    required this.available,
    required this.valid,
    required this.empty,
  });

  final bool checking;
  final bool? available;
  final bool valid;
  final bool empty;

  @override
  Widget build(BuildContext context) {
    if (empty) return const SizedBox.shrink();
    if (checking) {
      return const Padding(
        padding: EdgeInsets.all(14),
        child: SizedBox(height: 14, width: 14, child: CircularProgressIndicator(strokeWidth: 2)),
      );
    }
    if (!valid) return const Icon(Icons.error_outline, size: 18);
    if (available == true) return const Icon(Icons.check_circle_outline, size: 18);
    if (available == false) return const Icon(Icons.block, size: 18);
    return const SizedBox.shrink();
  }
}

/// Monokrom bildirim kutusu: anlam ikon + kenarlik kalinligiyla tasinir.
class _Note extends StatelessWidget {
  const _Note({required this.icon, required this.message, this.strong = false});

  final IconData icon;
  final String message;
  final bool strong;

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context);
    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: t.colorScheme.surfaceContainerHigh,
        borderRadius: BorderRadius.circular(8),
        border: Border.all(
          color: strong ? t.colorScheme.onSurface : t.colorScheme.outline,
          width: strong ? 1.5 : 1,
        ),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(icon, size: 18),
          const SizedBox(width: 8),
          Expanded(child: Text(message, style: t.textTheme.bodySmall)),
        ],
      ),
    );
  }
}
