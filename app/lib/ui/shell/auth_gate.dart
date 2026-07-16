import 'package:flutter/material.dart';

import '../../core/auth/auth_service.dart';
import '../../core/ipc/sidecar_client.dart';
import '../../core/l10n/strings.dart';
import '../../core/settings/settings_store.dart';
import 'login_screen.dart';
import 'operation_list_screen.dart';

/// Uygulama kapisi: oturum yok -> Giris; oturum var ama onaysiz -> Bekliyor;
/// onayli -> uygulama.
///
/// DURUST SINIR: bu bir UI kapisidir. Dosya islemleri yerelde calisir; kararli
/// biri bunu asabilir. GERCEK zorlama sunucuda RLS ile paylasilan history
/// uzerindedir (kim okuyup yazabilir).
class AuthGate extends StatefulWidget {
  const AuthGate({
    super.key,
    required this.auth,
    required this.client,
    required this.settings,
  });

  final AuthService auth;
  final SidecarClient client;
  final SettingsStore settings;

  @override
  State<AuthGate> createState() => _AuthGateState();
}

class _AuthGateState extends State<AuthGate> {
  bool _checking = false;
  String? _error;

  Future<void> _refreshMe() async {
    setState(() {
      _checking = true;
      _error = null;
    });
    try {
      await widget.auth.loadMe();
    } catch (e) {
      _error = e.toString();
    } finally {
      if (mounted) setState(() => _checking = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    // AuthService bir ChangeNotifier: signIn/signOut/loadMe sonrasi kapi
    // kendiliginden yeniden kurulur — cagiran ekranlarin haber vermesi gerekmez.
    return ListenableBuilder(
      listenable: widget.auth,
      builder: (context, _) {
        final auth = widget.auth;
        if (!auth.signedIn) return LoginScreen(auth: auth);
        if (!auth.allowed) return _pending(context);
        return OperationListScreen(
          client: widget.client,
          settings: widget.settings,
          auth: auth,
        );
      },
    );
  }

  Widget _pending(BuildContext context) {
    final s = Strings.of(context);
    return Scaffold(
      body: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 420),
          child: Padding(
            padding: const EdgeInsets.all(24),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                const Icon(Icons.hourglass_top, size: 44, color: Colors.orange),
                const SizedBox(height: 16),
                Text(s.t('auth.pending'),
                    style: const TextStyle(fontSize: 20, fontWeight: FontWeight.w700)),
                const SizedBox(height: 8),
                Text(s.t('auth.pendingHelp'), textAlign: TextAlign.center),
                const SizedBox(height: 8),
                Text(widget.auth.session?.email ?? '',
                    style: TextStyle(color: Colors.grey.shade600, fontSize: 13)),
                if (_error != null) ...[
                  const SizedBox(height: 10),
                  Text(_error!, style: const TextStyle(color: Colors.red, fontSize: 12)),
                ],
                const SizedBox(height: 20),
                Row(
                  mainAxisAlignment: MainAxisAlignment.center,
                  children: [
                    OutlinedButton.icon(
                      icon: const Icon(Icons.logout, size: 18),
                      label: Text(s.t('auth.signOut')),
                      onPressed: widget.auth.signOut,
                    ),
                    const SizedBox(width: 12),
                    FilledButton.icon(
                      icon: _checking
                          ? const SizedBox(
                              height: 14, width: 14, child: CircularProgressIndicator(strokeWidth: 2))
                          : const Icon(Icons.refresh, size: 18),
                      label: Text(s.t('action.retry')),
                      onPressed: _checking ? null : _refreshMe,
                    ),
                  ],
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
