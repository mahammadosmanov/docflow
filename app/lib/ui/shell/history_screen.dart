import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';

import '../../core/ipc/sidecar_client.dart';
import '../../core/l10n/strings.dart';
import '../../core/operations/models.dart';

/// Gecmis: yapilan her apply/undo bir journal kaydidir. Buradan geri alinabilir.
/// Geri alirken: orijinal konuma VEYA secilen bir klasore. Uzerine yazma yok.
class HistoryScreen extends StatefulWidget {
  const HistoryScreen({super.key, required this.client});

  final SidecarClient client;

  @override
  State<HistoryScreen> createState() => _HistoryScreenState();
}

class _HistoryScreenState extends State<HistoryScreen> {
  late Future<List<HistoryEntry>> _future;
  bool _busy = false;

  @override
  void initState() {
    super.initState();
    _future = widget.client.historyList();
  }

  void _reload() => setState(() => _future = widget.client.historyList());

  static String _fmt(DateTime? d) {
    if (d == null) return '';
    String two(int n) => n.toString().padLeft(2, '0');
    return '${d.year}-${two(d.month)}-${two(d.day)}  ${two(d.hour)}:${two(d.minute)}';
  }

  Future<void> _undo(HistoryEntry e) async {
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
      final r = await widget.client.undo(e.journalId, destFolder: destFolder);
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
      appBar: AppBar(title: Text(s.t('history.title'))),
      body: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 720),
          child: Stack(
            children: [
              FutureBuilder<List<HistoryEntry>>(
                future: _future,
                builder: (context, snap) {
                  if (snap.connectionState == ConnectionState.waiting) {
                    return const Center(child: CircularProgressIndicator());
                  }
                  if (snap.hasError) return Center(child: Text(snap.error.toString()));
                  final entries = snap.data ?? [];
                  if (entries.isEmpty) return Center(child: Text(s.t('history.empty')));
                  return ListView.separated(
                    padding: const EdgeInsets.symmetric(vertical: 16, horizontal: 12),
                    itemCount: entries.length,
                    separatorBuilder: (_, _) => const SizedBox(height: 8),
                    itemBuilder: (_, i) => _tile(s, entries[i]),
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

  Widget _tile(Strings s, HistoryEntry e) {
    return Card(
      elevation: 0,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(12),
        side: BorderSide(color: Colors.grey.shade300),
      ),
      child: ListTile(
        title: Text(_fmt(e.when), style: const TextStyle(fontWeight: FontWeight.w600)),
        subtitle: Text('${e.opId}  ·  ${s.count('history.moves', e.moves)}'),
        trailing: OutlinedButton.icon(
          icon: const Icon(Icons.undo, size: 18),
          label: Text(s.t('action.undo')),
          onPressed: _busy ? null : () => _undo(e),
        ),
      ),
    );
  }
}
