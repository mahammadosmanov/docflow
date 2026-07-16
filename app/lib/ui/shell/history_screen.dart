import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';

import '../../core/auth/auth_service.dart';
import '../../core/cloud/cloud_history.dart';
import '../../core/ipc/sidecar_client.dart';
import '../../core/l10n/strings.dart';

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
    return Scaffold(
      appBar: AppBar(
        title: Text(s.t('history.title')),
        actions: [
          Padding(
            padding: const EdgeInsets.only(right: 12),
            child: Icon(
              _cloud.enabled && !_cloudFailed ? Icons.cloud_done_outlined : Icons.cloud_off_outlined,
              size: 20,
              color: _cloud.enabled && !_cloudFailed ? Colors.green : Colors.grey,
            ),
          ),
        ],
      ),
      body: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 720),
          child: Stack(
            children: [
              FutureBuilder<List<_Row>>(
                future: _future,
                builder: (context, snap) {
                  if (snap.connectionState == ConnectionState.waiting) {
                    return const Center(child: CircularProgressIndicator());
                  }
                  if (snap.hasError) return Center(child: Text(snap.error.toString()));
                  final rows = snap.data ?? [];
                  if (rows.isEmpty) return Center(child: Text(s.t('history.empty')));
                  return ListView.separated(
                    padding: const EdgeInsets.symmetric(vertical: 16, horizontal: 12),
                    itemCount: rows.length,
                    separatorBuilder: (_, _) => const SizedBox(height: 8),
                    itemBuilder: (_, i) => _tile(s, rows[i]),
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

  Widget _tile(Strings s, _Row e) {
    return Card(
      elevation: 0,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(12),
        side: BorderSide(color: Colors.grey.shade300),
      ),
      child: ListTile(
        leading: e.fromCloud
            ? const Icon(Icons.cloud_outlined, size: 18)
            : const Icon(Icons.sd_storage_outlined, size: 18),
        title: Text(_fmt(e.when), style: const TextStyle(fontWeight: FontWeight.w600)),
        subtitle: Text('${e.opId}  ·  ${s.count('history.moves', e.moves)}'
            '${e.undone ? '  ·  ↩' : ''}'),
        trailing: OutlinedButton.icon(
          icon: const Icon(Icons.undo, size: 18),
          label: Text(s.t('action.undo')),
          onPressed: _busy || e.undone ? null : () => _undo(e),
        ),
      ),
    );
  }
}
