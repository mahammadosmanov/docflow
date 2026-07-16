import 'package:flutter/material.dart';

import '../../core/auth/auth_service.dart';
import '../../core/ipc/sidecar_client.dart';
import '../../core/l10n/strings.dart';
import '../../core/operations/models.dart';
import '../../core/settings/app_prefs.dart';
import '../../core/settings/settings_store.dart';
import '../theme/responsive.dart';
import 'admin_panel_screen.dart';
import 'history_screen.dart';
import 'operation_flow_screen.dart';
import 'settings_screen.dart';

/// Acilis ekrani: list_operations cagirir, operation'lari kart olarak listeler.
class OperationListScreen extends StatefulWidget {
  const OperationListScreen({
    super.key,
    required this.client,
    required this.settings,
    required this.auth,
    required this.prefs,
  });

  final SidecarClient client;
  final SettingsStore settings;
  final AuthService auth;
  final AppPrefs prefs;

  @override
  State<OperationListScreen> createState() => _OperationListScreenState();
}

class _OperationListScreenState extends State<OperationListScreen> {
  late Future<List<OperationDef>> _future;

  @override
  void initState() {
    super.initState();
    _future = _load();
  }

  Future<List<OperationDef>> _load() async {
    await widget.client.start();
    return widget.client.listOperations();
  }

  void _retry() => setState(() => _future = _load());

  void _push(Widget screen) =>
      Navigator.of(context).push(MaterialPageRoute(builder: (_) => screen));

  @override
  Widget build(BuildContext context) {
    final s = Strings.of(context);
    final t = Theme.of(context);
    final ff = FormFactor.of(context);
    final email = widget.auth.session?.email ?? '';

    return Scaffold(
      appBar: AppBar(
        title: Text(s.t('app.title')),
        actions: [
          // Dar pencerede e-posta rozetini gizle — aksiyon ikonlari onceliklidir.
          if (!ff.isCompact && email.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(right: 8),
              child: Chip(
                avatar: const Icon(Icons.person_outline, size: 14),
                label: Text(email),
              ),
            ),
          // Admin butonu sadece admin'e gorunur; sunucuda RLS zaten zorlar.
          if (widget.auth.isAdmin)
            IconButton(
              icon: const Icon(Icons.shield_outlined),
              tooltip: s.t('action.admin'),
              onPressed: () => _push(AdminPanelScreen(auth: widget.auth)),
            ),
          IconButton(
            icon: const Icon(Icons.history),
            tooltip: s.t('action.history'),
            onPressed: () =>
                _push(HistoryScreen(client: widget.client, auth: widget.auth)),
          ),
          IconButton(
            icon: const Icon(Icons.tune),
            tooltip: s.t('prefs.title'),
            onPressed: () => _push(SettingsScreen(prefs: widget.prefs)),
          ),
          IconButton(
            icon: const Icon(Icons.logout),
            tooltip: '${s.t('auth.signOut')} — $email',
            // AuthGate dinliyor: signOut -> kapi Login'e doner.
            onPressed: widget.auth.signOut,
          ),
          const SizedBox(width: 4),
        ],
      ),
      body: FutureBuilder<List<OperationDef>>(
        future: _future,
        builder: (context, snap) {
          if (snap.connectionState == ConnectionState.waiting) {
            return const Center(child: CircularProgressIndicator());
          }
          if (snap.hasError || widget.client.isDown) {
            return EmptyState(
              icon: Icons.power_off_outlined,
              title: s.t('error.sidecar_down'),
              message: s.t('error.sidecar_down.help'),
              action: FilledButton.icon(
                icon: const Icon(Icons.refresh, size: 18),
                label: Text(s.t('action.retry')),
                onPressed: _retry,
              ),
            );
          }
          final ops = snap.data ?? [];
          if (ops.isEmpty) {
            return EmptyState(icon: Icons.inbox_outlined, title: s.t('plan.empty'));
          }
          return ListView(
            children: [
              Pane(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    Text(s.t('ops.heading'),
                        style: t.textTheme.headlineSmall
                            ?.copyWith(fontWeight: FontWeight.w700, letterSpacing: -0.5)),
                    const SizedBox(height: 4),
                    Text(s.t('ops.subheading'),
                        style: t.textTheme.bodyMedium
                            ?.copyWith(color: t.colorScheme.onSurfaceVariant)),
                    const SizedBox(height: 20),
                    for (final op in ops) ...[
                      _OpCard(
                        op: op,
                        onTap: () => _push(OperationFlowScreen(
                          client: widget.client,
                          op: op,
                          settings: widget.settings,
                          auth: widget.auth,
                        )),
                      ),
                      const SizedBox(height: 12),
                    ],
                  ],
                ),
              ),
            ],
          );
        },
      ),
    );
  }
}

class _OpCard extends StatefulWidget {
  const _OpCard({required this.op, required this.onTap});

  final OperationDef op;
  final VoidCallback onTap;

  @override
  State<_OpCard> createState() => _OpCardState();
}

class _OpCardState extends State<_OpCard> {
  bool _hover = false;

  @override
  Widget build(BuildContext context) {
    final s = Strings.of(context);
    final t = Theme.of(context);
    // Monokromda hover'i renk degil, kenarlik + zemin tonu belli eder.
    return MouseRegion(
      onEnter: (_) => setState(() => _hover = true),
      onExit: (_) => setState(() => _hover = false),
      child: AnimatedContainer(
        duration: const Duration(milliseconds: 120),
        decoration: BoxDecoration(
          color: _hover
              ? t.colorScheme.surfaceContainer
              : t.colorScheme.surfaceContainerLowest,
          borderRadius: BorderRadius.circular(12),
          border: Border.all(
            color: _hover ? t.colorScheme.onSurface : t.colorScheme.outlineVariant,
          ),
        ),
        child: Material(
          color: Colors.transparent,
          child: InkWell(
            borderRadius: BorderRadius.circular(12),
            onTap: widget.onTap,
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 16),
              child: Row(
                children: [
                  Container(
                    padding: const EdgeInsets.all(10),
                    decoration: BoxDecoration(
                      color: t.colorScheme.surfaceContainerHigh,
                      borderRadius: BorderRadius.circular(8),
                    ),
                    child: const Icon(Icons.auto_awesome_motion_outlined, size: 20),
                  ),
                  const SizedBox(width: 14),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(s.t(widget.op.titleKey),
                            style: t.textTheme.titleSmall
                                ?.copyWith(fontWeight: FontWeight.w600)),
                        const SizedBox(height: 3),
                        Text(s.t(widget.op.descriptionKey),
                            style: t.textTheme.bodySmall
                                ?.copyWith(color: t.colorScheme.onSurfaceVariant)),
                      ],
                    ),
                  ),
                  const SizedBox(width: 8),
                  Icon(Icons.arrow_forward,
                      size: 18, color: t.colorScheme.onSurfaceVariant),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
