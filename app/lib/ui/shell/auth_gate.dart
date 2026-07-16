import 'package:flutter/material.dart';

import '../../core/auth/auth_service.dart';
import '../../core/ipc/sidecar_client.dart';
import '../../core/l10n/strings.dart';
import '../../core/settings/app_prefs.dart';
import '../../core/settings/settings_store.dart';
import '../theme/responsive.dart';
import 'login_screen.dart';
import 'operation_list_screen.dart';
import 'settings_screen.dart';

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
    required this.prefs,
  });

  final AuthService auth;
  final SidecarClient client;
  final SettingsStore settings;
  final AppPrefs prefs;

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
        if (!auth.signedIn) return LoginScreen(auth: auth, prefs: widget.prefs);
        if (!auth.allowed) return _pending(context);
        return OperationListScreen(
          client: widget.client,
          settings: widget.settings,
          auth: auth,
          prefs: widget.prefs,
        );
      },
    );
  }

  Widget _pending(BuildContext context) {
    final s = Strings.of(context);
    final t = Theme.of(context);
    return Scaffold(
      appBar: AppBar(
        title: Text(s.t('app.title')),
        actions: [
          // Onay beklerken de dil/tema degistirilebilmeli — kilitli kalma.
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
        child: Pane(
          maxWidth: 460,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Container(
                padding: const EdgeInsets.all(16),
                decoration: BoxDecoration(
                  color: t.colorScheme.surfaceContainer,
                  shape: BoxShape.circle,
                ),
                child: const Icon(Icons.hourglass_empty, size: 28),
              ),
              const SizedBox(height: 20),
              Text(s.t('auth.pending'),
                  textAlign: TextAlign.center,
                  style: t.textTheme.titleLarge?.copyWith(fontWeight: FontWeight.w600)),
              const SizedBox(height: 8),
              Text(s.t('auth.pendingHelp'),
                  textAlign: TextAlign.center,
                  style: t.textTheme.bodyMedium
                      ?.copyWith(color: t.colorScheme.onSurfaceVariant)),
              const SizedBox(height: 14),
              Chip(label: Text(widget.auth.session?.email ?? '')),
              if (_error != null) ...[
                const SizedBox(height: 14),
                _ErrorNote(message: _error!),
              ],
              const SizedBox(height: 24),
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
                            height: 14,
                            width: 14,
                            child: CircularProgressIndicator(strokeWidth: 2))
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
    );
  }
}

/// Hata notu — monokromda anlam ikon + kenarlikla tasinir, renkle degil.
class _ErrorNote extends StatelessWidget {
  const _ErrorNote({required this.message});

  final String message;

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context);
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: t.colorScheme.surfaceContainerHigh,
        borderRadius: BorderRadius.circular(8),
        border: Border.all(color: t.colorScheme.onSurface, width: 1.5),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Icon(Icons.error_outline, size: 18),
          const SizedBox(width: 8),
          Expanded(child: Text(message, style: t.textTheme.bodySmall)),
        ],
      ),
    );
  }
}
