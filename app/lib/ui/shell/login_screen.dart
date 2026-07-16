import 'package:flutter/material.dart';

import '../../core/auth/auth_service.dart';
import '../../core/l10n/strings.dart';

/// Giris / kayit. Parola SADECE kullanici tarafindan girilir ve diske yazilmaz;
/// yalnizca donen oturum token'i saklanir.
class LoginScreen extends StatefulWidget {
  const LoginScreen({super.key, required this.auth});

  final AuthService auth;

  @override
  State<LoginScreen> createState() => _LoginScreenState();
}

class _LoginScreenState extends State<LoginScreen> {
  final _email = TextEditingController();
  final _password = TextEditingController();
  bool _busy = false;
  String? _error;
  String? _info;

  @override
  void dispose() {
    _email.dispose();
    _password.dispose();
    super.dispose();
  }

  bool get _valid =>
      _email.text.trim().contains('@') && _password.text.length >= 6;

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

  @override
  Widget build(BuildContext context) {
    final s = Strings.of(context);
    return Scaffold(
      body: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 380),
          child: Padding(
            padding: const EdgeInsets.all(24),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Text(s.t('app.title'),
                    textAlign: TextAlign.center,
                    style: const TextStyle(fontSize: 26, fontWeight: FontWeight.w700)),
                const SizedBox(height: 28),
                TextField(
                  controller: _email,
                  autofocus: true,
                  keyboardType: TextInputType.emailAddress,
                  decoration: InputDecoration(
                    labelText: s.t('auth.email'),
                    border: const OutlineInputBorder(),
                  ),
                  onChanged: (_) => setState(() {}),
                ),
                const SizedBox(height: 12),
                TextField(
                  controller: _password,
                  obscureText: true,
                  decoration: InputDecoration(
                    labelText: s.t('auth.password'),
                    border: const OutlineInputBorder(),
                  ),
                  onChanged: (_) => setState(() {}),
                  onSubmitted: (_) {
                    if (_valid && !_busy) {
                      _run(() => widget.auth.signIn(_email.text.trim(), _password.text));
                    }
                  },
                ),
                if (_error != null) ...[
                  const SizedBox(height: 12),
                  Text(_error!, style: const TextStyle(color: Colors.red, fontSize: 13)),
                ],
                if (_info != null) ...[
                  const SizedBox(height: 12),
                  Text(_info!, style: const TextStyle(color: Colors.green, fontSize: 13)),
                ],
                const SizedBox(height: 20),
                FilledButton(
                  onPressed: _busy || !_valid
                      ? null
                      : () => _run(
                          () => widget.auth.signIn(_email.text.trim(), _password.text)),
                  child: _busy
                      ? const SizedBox(
                          height: 18, width: 18, child: CircularProgressIndicator(strokeWidth: 2))
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
              ],
            ),
          ),
        ),
      ),
    );
  }
}
