import 'package:flutter/material.dart';

import '../../core/auth/auth_service.dart';
import '../../core/l10n/strings.dart';
import '../theme/responsive.dart';

/// Admin paneli: kim uygulamayi kullanabilir (allowed) ve kim admin.
///
/// Bu ekran sadece bir KOLAYLIK. Asil zorlama sunucuda: app_users uzerindeki
/// RLS policy'si guncellemeyi is_admin() ile sinirlar — admin olmayan biri
/// istegi elle yollasa bile reddedilir.
class AdminPanelScreen extends StatefulWidget {
  const AdminPanelScreen({super.key, required this.auth});

  final AuthService auth;

  @override
  State<AdminPanelScreen> createState() => _AdminPanelScreenState();
}

class _AdminPanelScreenState extends State<AdminPanelScreen> {
  late Future<List<AppUser>> _future;
  bool _busy = false;

  @override
  void initState() {
    super.initState();
    _future = widget.auth.listUsers();
  }

  void _reload() => setState(() => _future = widget.auth.listUsers());

  Future<void> _set(AppUser u, {bool? allowed, bool? isAdmin}) async {
    setState(() => _busy = true);
    try {
      await widget.auth.setFlags(u.id, allowed: allowed, isAdmin: isAdmin);
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text(e.toString())));
      }
    } finally {
      if (mounted) {
        setState(() => _busy = false);
        _reload();
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final s = Strings.of(context);
    return Scaffold(
      appBar: AppBar(
        title: Text(s.t('admin.title')),
        actions: [
          IconButton(
            icon: const Icon(Icons.refresh),
            tooltip: s.t('action.retry'),
            onPressed: _busy ? null : _reload,
          ),
          const SizedBox(width: 4),
        ],
      ),
      body: Stack(
        children: [
          FutureBuilder<List<AppUser>>(
            future: _future,
            builder: (context, snap) {
              if (snap.connectionState == ConnectionState.waiting) {
                return const Center(child: CircularProgressIndicator());
              }
              if (snap.hasError) {
                return EmptyState(
                  icon: Icons.cloud_off_outlined,
                  title: s.t('admin.title'),
                  message: snap.error.toString(),
                  action: FilledButton.icon(
                    icon: const Icon(Icons.refresh, size: 18),
                    label: Text(s.t('action.retry')),
                    onPressed: _reload,
                  ),
                );
              }
              final users = snap.data ?? [];
              if (users.isEmpty) {
                return EmptyState(
                    icon: Icons.person_outline, title: s.t('admin.empty'));
              }
              return ListView(
                children: [
                  Pane(
                    maxWidth: 780,
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        Text(s.t('admin.help'),
                            style: Theme.of(context).textTheme.bodySmall?.copyWith(
                                color: Theme.of(context).colorScheme.onSurfaceVariant)),
                        const SizedBox(height: 14),
                        Card(
                          child: Column(
                            children: [
                              for (var i = 0; i < users.length; i++) ...[
                                if (i > 0) const Divider(height: 1),
                                _tile(s, users[i]),
                              ],
                            ],
                          ),
                        ),
                      ],
                    ),
                  ),
                ],
              );
            },
          ),
          if (_busy)
            Positioned.fill(
              child: ColoredBox(
                color: Theme.of(context).colorScheme.scrim.withValues(alpha: 0.1),
                child: const Center(child: CircularProgressIndicator()),
              ),
            ),
        ],
      ),
    );
  }

  Widget _tile(Strings s, AppUser u) {
    final t = Theme.of(context);
    final isSelf = u.id == widget.auth.session?.userId;
    final compact = FormFactor.of(context).isCompact;

    final switches = [
      _LabeledSwitch(
        label: s.t('admin.allowed'),
        value: u.allowed,
        // Kendi erisimini kazara kapatmani engelle.
        onChanged: _busy || isSelf ? null : (v) => _set(u, allowed: v),
      ),
      const SizedBox(width: 12),
      _LabeledSwitch(
        label: s.t('admin.admin'),
        value: u.isAdmin,
        onChanged: _busy || isSelf ? null : (v) => _set(u, isAdmin: v),
      ),
    ];

    final title = Row(
      children: [
        // Erisim durumu: renk degil, ikon + metin.
        Icon(u.allowed ? Icons.check_circle_outline : Icons.block, size: 18),
        const SizedBox(width: 10),
        Flexible(
          child: Text(u.email,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: t.textTheme.bodyMedium?.copyWith(fontWeight: FontWeight.w600)),
        ),
        if (u.isAdmin) ...[
          const SizedBox(width: 8),
          Chip(label: Text(s.t('admin.admin'))),
        ],
        if (isSelf) ...[
          const SizedBox(width: 6),
          Chip(label: Text(s.t('admin.you'))),
        ],
      ],
    );

    final meta = Padding(
      padding: const EdgeInsets.only(left: 28, top: 2),
      child: Text(
        [
          u.createdAt.split('T').first,
          if (!u.allowed) s.t('admin.noAccess'),
        ].join('  ·  '),
        style: t.textTheme.bodySmall?.copyWith(color: t.colorScheme.onSurfaceVariant),
      ),
    );

    // Dar pencerede switch'ler alta iner — tasma yok.
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 12, 16, 12),
      child: compact
          ? Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                title,
                meta,
                const SizedBox(height: 8),
                Padding(
                  padding: const EdgeInsets.only(left: 28),
                  child: Row(children: switches),
                ),
              ],
            )
          : Row(
              children: [
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [title, meta],
                  ),
                ),
                const SizedBox(width: 12),
                ...switches,
              ],
            ),
    );
  }
}

class _LabeledSwitch extends StatelessWidget {
  const _LabeledSwitch({
    required this.label,
    required this.value,
    required this.onChanged,
  });

  final String label;
  final bool value;
  final ValueChanged<bool>? onChanged;

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context);
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        Text(label,
            style: t.textTheme.labelSmall
                ?.copyWith(color: t.colorScheme.onSurfaceVariant)),
        Switch(value: value, onChanged: onChanged),
      ],
    );
  }
}
