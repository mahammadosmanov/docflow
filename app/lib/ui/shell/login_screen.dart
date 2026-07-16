import 'package:flutter/material.dart';

import '../../core/auth/auth_service.dart';
import '../../core/l10n/strings.dart';
import '../../core/settings/app_prefs.dart';
import '../theme/responsive.dart';
import 'settings_screen.dart';

/// Giris / kayit. Parola SADECE kullanici tarafindan girilir ve diske yazilmaz;
/// yalnizca donen oturum token'i saklanir.
class LoginScreen extends StatefulWidget {
  const LoginScreen({super.key, required this.auth, required this.prefs});

  final AuthService auth;
  final AppPrefs prefs;

  @override
  State<LoginScreen> createState() => _LoginScreenState();
}

class _LoginScreenState extends State<LoginScreen> {
  final _email = TextEditingController();
  final _password = TextEditingController();
  bool _busy = false;
  bool _obscure = true;
  String? _error;
  String? _info;

  @override
  void dispose() {
    _email.dispose();
    _password.dispose();
    super.dispose();
  }

  bool get _valid => _email.text.trim().contains('@') && _password.text.length >= 6;

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
      if (mounted) setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  void _submit() {
    if (_valid && !_busy) {
      _run(() => widget.auth.signIn(_email.text.trim(), _password.text));
    }
  }

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
                Text(s.t('auth.subtitle'),
                    textAlign: TextAlign.center,
                    style: t.textTheme.bodySmall
                        ?.copyWith(color: t.colorScheme.onSurfaceVariant)),
                const SizedBox(height: 32),
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
                  controller: _password,
                  obscureText: _obscure,
                  enabled: !_busy,
                  decoration: InputDecoration(
                    labelText: s.t('auth.password'),
                    prefixIcon: const Icon(Icons.lock_outline, size: 18),
                    suffixIcon: IconButton(
                      icon: Icon(
                        _obscure ? Icons.visibility_outlined : Icons.visibility_off_outlined,
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
                      : Text(s.t('auth.signIn')),
                ),
                const SizedBox(height: 8),
                TextButton(
                  onPressed: _busy || !_valid
                      ? null
                      : () => _run(
                            () => widget.auth.signUp(_email.text.trim(), _password.text),
                            info: s.t('auth.signupOk'),
                          ),
                  child: Text(s.t('auth.signUp')),
                ),
                const SizedBox(height: 4),
                Text(s.t('auth.passwordHint'),
                    textAlign: TextAlign.center,
                    style: t.textTheme.bodySmall
                        ?.copyWith(color: t.colorScheme.onSurfaceVariant)),
              ],
            ),
          ),
        ),
      ),
    );
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
