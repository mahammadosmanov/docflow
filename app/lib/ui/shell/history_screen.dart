import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';

import '../../core/auth/auth_service.dart';
import '../../core/cloud/cloud_history.dart';
import '../../core/ipc/sidecar_client.dart';
import '../../core/l10n/strings.dart';
import '../theme/responsive.dart';

/// Gecmis: yapilan her apply/undo bir journal kaydidir.
/// Kaynak: bulut (Supabase) varsa oradan — makineler arasi gorunur; yoksa/hata
/// halinde yerel journal. Geri alirken orijinal konuma VEYA secilen klasore.
class HistoryScreen extends StatefulWidget {
  const HistoryScreen({super.key, required this.client, required this.auth});

  final SidecarClient client;
  final AuthService auth;

  @override
  State<HistoryScreen> createState() => _HistoryScreenState();
}

/// Bulut ve yerel kayitlari tek sekle indirger.
class _Row {
  _Row({
    required this.when,
    required this.opId,
    required this.moves,
    required this.journalId,
    this.steps,
    this.cloudId,
    this.undone = false,
    this.fromCloud = false,
  });

  final DateTime? when;
  final String opId;
  final int moves;
  final String journalId;
  final List<dynamic>? steps; // bulut kaydinda dolu -> yerel journal gerekmez
  final String? cloudId;
  final bool undone;
  final bool fromCloud;
}

class _HistoryScreenState extends State<HistoryScreen> {
  late final CloudHistory _cloud = CloudHistory(auth: widget.auth);
  late Future<List<_Row>> _future;
  bool _busy = false;
  bool _cloudFailed = false;

  @override
  void initState() {
    super.initState();
    _future = _load();
  }

  Future<List<_Row>> _load() async {
    if (_cloud.enabled) {
      try {
        final entries = await _cloud.list();
        _cloudFailed = false;
        return entries
            .map((e) => _Row(
                  when: e.when,
                  opId: e.opId,
                  moves: e.applied,
                  journalId: e.journalId,
                  steps: e.steps.isEmpty ? null : e.steps,
                  cloudId: e.id,
                  undone: e.undone,
                  fromCloud: true,
                ))
            .toList();
      } catch (_) {
        _cloudFailed = true; // buluta ulasilamadi -> yerele dus
      }
    }
    final local = await widget.client.historyList();
    return local
        .map((e) => _Row(
              when: e.when,
              opId: e.opId,
              moves: e.moves,
              journalId: e.journalId,
            ))
        .toList();
  }

  void _reload() => setState(() => _future = _load());

  static String _fmt(DateTime? d) {
    if (d == null) return '';
    String two(int n) => n.toString().padLeft(2, '0');
    return '${d.year}-${two(d.month)}-${two(d.day)}  ${two(d.hour)}:${two(d.minute)}';
  }

  Future<void> _undo(_Row row) async {
    final s = Strings.of(context);
    final choice = await showModalBottomSheet<String>(
      context: context,
      builder: (ctx) => SafeArea(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            ListTile(
              leading: const Icon(Icons.undo),
              title: Text(s.t('history.undoOriginal')),
              onTap: () => Navigator.pop(ctx, 'original'),
            ),
            ListTile(
              leading: const Icon(Icons.folder_open),
              title: Text(s.t('history.undoToFolder')),
              onTap: () => Navigator.pop(ctx, 'folder'),
            ),
            ListTile(
              leading: const Icon(Icons.close),
              title: Text(s.t('action.cancel')),
              onTap: () => Navigator.pop(ctx, null),
            ),
          ],
        ),
      ),
    );
    if (choice == null || !mounted) return;

    String? destFolder;
    if (choice == 'folder') {
      destFolder = await FilePicker.platform.getDirectoryPath();
      if (destFolder == null) return; // iptal
    }

    setState(() => _busy = true);
    try {
      final r = await widget.client.undo(
        row.journalId,
        destFolder: destFolder,
        steps: row.steps, // bulut kaydi -> yerel journal olmasa da calisir
      );
      if (r.failed == 0 && row.cloudId != null) {
        await _cloud.markUndone(row.cloudId!);
      }
      if (!mounted) return;
      final msg = r.failed == 0
          ? s.count('result.applied', r.applied)
          : '${s.count('result.applied', r.applied)} · ${s.count('result.failed', r.failed)}';
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(msg)));
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(e.toString())));
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
    final online = _cloud.enabled && !_cloudFailed;
    return Scaffold(
      appBar: AppBar(
        title: Text(s.t('history.title')),
        actions: [
          // Kayitlar buluttan mi yerelden mi geliyor? Kullanici bilsin.
          Tooltip(
            message: s.t(online ? 'history.sourceCloud' : 'history.sourceLocal'),
            child: Chip(
              avatar: Icon(
                  online ? Icons.cloud_outlined : Icons.sd_storage_outlined,
                  size: 14),
              label: Text(s.t(online ? 'history.cloud' : 'history.local')),
            ),
          ),
          const SizedBox(width: 8),
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
          FutureBuilder<List<_Row>>(
            future: _future,
            builder: (context, snap) {
              if (snap.connectionState == ConnectionState.waiting) {
                return const Center(child: CircularProgressIndicator());
              }
              if (snap.hasError) {
                return EmptyState(
                  icon: Icons.error_outline,
                  title: s.t('history.title'),
                  message: snap.error.toString(),
                  action: FilledButton.icon(
                    icon: const Icon(Icons.refresh, size: 18),
                    label: Text(s.t('action.retry')),
                    onPressed: _reload,
                  ),
                );
              }
              final rows = snap.data ?? [];
              if (rows.isEmpty) {
                return EmptyState(
                  icon: Icons.history_toggle_off,
                  title: s.t('history.empty'),
                  message: s.t('history.empty.help'),
                );
              }
              return ListView(
                children: [
                  Pane(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        for (var i = 0; i < rows.length; i++) ...[
                          _tile(s, rows[i]),
                          if (i < rows.length - 1) const SizedBox(height: 8),
                        ],
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

  Widget _tile(Strings s, _Row e) {
    final t = Theme.of(context);
    final compact = FormFactor.of(context).isCompact;
    final undoBtn = OutlinedButton.icon(
      icon: const Icon(Icons.undo, size: 16),
      label: Text(s.t(e.undone ? 'history.undone' : 'action.undo')),
      onPressed: _busy || e.undone ? null : () => _undo(e),
    );

    final info = Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Icon(e.fromCloud ? Icons.cloud_outlined : Icons.sd_storage_outlined,
                size: 16),
            const SizedBox(width: 8),
            Text(_fmt(e.when),
                style: t.textTheme.bodyMedium?.copyWith(fontWeight: FontWeight.w600)),
            if (e.undone) ...[
              const SizedBox(width: 8),
              Chip(label: Text(s.t('history.undone'))),
            ],
          ],
        ),
        const SizedBox(height: 3),
        Padding(
          padding: const EdgeInsets.only(left: 24),
          child: Text('${e.opId}  ·  ${s.count('history.moves', e.moves)}',
              style: t.textTheme.bodySmall
                  ?.copyWith(color: t.colorScheme.onSurfaceVariant)),
        ),
      ],
    );

    return Card(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(14, 12, 14, 12),
        child: compact
            ? Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  info,
                  const SizedBox(height: 10),
                  Align(alignment: Alignment.centerRight, child: undoBtn),
                ],
              )
            : Row(
                children: [
                  Expanded(child: info),
                  const SizedBox(width: 12),
                  undoBtn,
                ],
              ),
      ),
    );
  }
}
