import 'package:flutter/material.dart';

import '../../core/auth/auth_service.dart';
import '../../core/l10n/strings.dart';

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
      appBar: AppBar(title: Text(s.t('admin.title'))),
      body: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 760),
          child: Stack(
            children: [
              FutureBuilder<List<AppUser>>(
                future: _future,
                builder: (context, snap) {
                  if (snap.connectionState == ConnectionState.waiting) {
                    return const Center(child: CircularProgressIndicator());
                  }
                  if (snap.hasError) {
                    return Center(child: Text(snap.error.toString()));
                  }
                  final users = snap.data ?? [];
                  if (users.isEmpty) return Center(child: Text(s.t('admin.empty')));
                  return ListView.separated(
                    padding: const EdgeInsets.symmetric(vertical: 16, horizontal: 12),
                    itemCount: users.length,
                    separatorBuilder: (_, _) => const Divider(height: 1),
                    itemBuilder: (_, i) => _tile(s, users[i]),
                  );
                },
              ),
              if (_busy)
                const Positioned.fill(
                  child: ColoredBox(
                    color: Color(0x22000000),
                    child: Center(child: CircularProgressIndicator()),
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _tile(Strings s, AppUser u) {
    final isSelf = u.id == widget.auth.session?.userId;
    return ListTile(
      leading: Icon(
        u.allowed ? Icons.check_circle : Icons.block,
        color: u.allowed ? Colors.green : Colors.grey,
      ),
      title: Row(
        children: [
          Flexible(
            child: Text(u.email,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(fontWeight: FontWeight.w600)),
          ),
          if (u.isAdmin)
            Padding(
              padding: const EdgeInsets.only(left: 8),
              child: Chip(
                visualDensity: VisualDensity.compact,
                label: Text(s.t('admin.admin'), style: const TextStyle(fontSize: 11)),
              ),
            ),
        ],
      ),
      subtitle: Text(u.createdAt.split('T').first),
      trailing: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(s.t('admin.allowed'), style: const TextStyle(fontSize: 12)),
          Switch(
            value: u.allowed,
            // Kendi erisimini kazara kapatmani engelle.
            onChanged: _busy || isSelf ? null : (v) => _set(u, allowed: v),
          ),
          const SizedBox(width: 8),
          Text(s.t('admin.admin'), style: const TextStyle(fontSize: 12)),
          Switch(
            value: u.isAdmin,
            onChanged: _busy || isSelf ? null : (v) => _set(u, isAdmin: v),
          ),
        ],
      ),
    );
  }
}
