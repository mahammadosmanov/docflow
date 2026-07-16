import 'dart:io';

import 'package:flutter/material.dart';

import '../../core/ipc/sidecar_client.dart';
import '../../core/l10n/strings.dart';
import '../../core/operations/models.dart';

/// _review/ yoneticisi: dosya bazinda Bırak / Hedefe koy / Sil (çöpe).
/// Her sey preview -> apply -> journal -> undo'dan gecer. Silme = geri donuslu
/// cope tasima; Put hedefteki dosyayi cope alip yerine koyar. Uzerine yazma YOK.
class ReviewScreen extends StatefulWidget {
  const ReviewScreen({super.key, required this.client, required this.dest});

  final SidecarClient client;
  final String dest;

  @override
  State<ReviewScreen> createState() => _ReviewScreenState();
}

enum _Stage { list, previewing, preview, applying, result }

class _ReviewScreenState extends State<ReviewScreen> {
  _Stage _stage = _Stage.list;
  late Future<List<ReviewItem>> _items;
  final Map<String, String> _choice = {}; // path -> keep|put|delete
  Plan? _plan;
  ApplyResult? _result;

  @override
  void initState() {
    super.initState();
    _items = widget.client.reviewScan(widget.dest);
  }

  void _openFile(String path) {
    try {
      if (Platform.isMacOS) {
        Process.run('open', [path]);
      } else if (Platform.isWindows) {
        Process.run('cmd', ['/c', 'start', '', path]);
      } else {
        Process.run('xdg-open', [path]);
      }
    } catch (_) {/* acmak kritik degil */}
  }

  Future<void> _preview(List<ReviewItem> items) async {
    final decisions = <Map<String, dynamic>>[];
    for (final it in items) {
      final a = _choice[it.path] ?? 'keep';
      if (a == 'keep') continue;
      decisions.add({'path': it.path, 'action': a, 'code': it.code});
    }
    if (decisions.isEmpty) return;
    setState(() => _stage = _Stage.previewing);
    try {
      final plan = await widget.client.reviewPreview(widget.dest, decisions);
      setState(() {
        _plan = plan;
        _stage = _Stage.preview;
      });
    } catch (e) {
      setState(() => _stage = _Stage.list);
      _snack(e.toString());
    }
  }

  Future<void> _apply() async {
    final plan = _plan;
    if (plan?.planId == null) return;
    setState(() => _stage = _Stage.applying);
    try {
      final r = await widget.client.apply(plan!.planId!);
      setState(() {
        _result = r;
        _stage = _Stage.result;
      });
    } catch (e) {
      setState(() => _stage = _Stage.preview);
      _snack(e.toString());
    }
  }

  Future<void> _undo() async {
    final r = _result;
    if (r == null || r.journalId.isEmpty) return;
    try {
      await widget.client.undo(r.journalId);
      if (mounted) Navigator.of(context).pop(true);
    } catch (e) {
      _snack(e.toString());
    }
  }

  void _snack(String m) {
    if (mounted) ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(m)));
  }

  @override
  Widget build(BuildContext context) {
    final s = Strings.of(context);
    return Scaffold(
      appBar: AppBar(title: Text(s.t('review.title'))),
      body: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 820),
          child: Padding(padding: const EdgeInsets.all(20), child: _body(s)),
        ),
      ),
    );
  }

  Widget _body(Strings s) {
    switch (_stage) {
      case _Stage.previewing:
      case _Stage.applying:
        return const Center(child: CircularProgressIndicator());
      case _Stage.preview:
        return _previewView(s);
      case _Stage.result:
        return _resultView(s);
      case _Stage.list:
        return FutureBuilder<List<ReviewItem>>(
          future: _items,
          builder: (context, snap) {
            if (snap.connectionState == ConnectionState.waiting) {
              return const Center(child: CircularProgressIndicator());
            }
            if (snap.hasError) return Center(child: Text(snap.error.toString()));
            final items = snap.data ?? [];
            if (items.isEmpty) return Center(child: Text(s.t('review.empty')));
            return _listView(s, items);
          },
        );
    }
  }

  Widget _listView(Strings s, List<ReviewItem> items) {
    final anyAction = items.any((it) => (_choice[it.path] ?? 'keep') != 'keep');
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Expanded(
          child: ListView.separated(
            itemCount: items.length,
            separatorBuilder: (_, _) => const Divider(height: 1),
            itemBuilder: (_, i) => _itemTile(s, items[i]),
          ),
        ),
        const SizedBox(height: 12),
        Align(
          alignment: Alignment.centerRight,
          child: FilledButton.icon(
            icon: const Icon(Icons.visibility_outlined),
            label: Text(s.t('action.preview')),
            onPressed: anyAction ? () => _preview(items) : null,
          ),
        ),
      ],
    );
  }

  Widget _itemTile(Strings s, ReviewItem it) {
    final action = _choice[it.path] ?? 'keep';
    final sub = <String>[
      if (it.hasCode) it.code else s.t('review.noCode'),
      if (it.targetExists) s.t('review.willReplace'),
    ].join('  ·  ');
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 8),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(it.name,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: const TextStyle(fontWeight: FontWeight.w600)),
                    Text(sub,
                        style: TextStyle(fontSize: 12, color: Colors.grey.shade600)),
                  ],
                ),
              ),
              TextButton.icon(
                icon: const Icon(Icons.open_in_new, size: 18),
                label: Text(s.t('action.open')),
                onPressed: () => _openFile(it.path),
              ),
            ],
          ),
          const SizedBox(height: 6),
          Wrap(
            spacing: 8,
            children: [
              ChoiceChip(
                label: Text(s.t('review.keep')),
                selected: action == 'keep',
                onSelected: (_) => setState(() => _choice[it.path] = 'keep'),
              ),
              ChoiceChip(
                label: Text(s.t('review.put')),
                selected: action == 'put',
                // Kod okunamayan dosyaya "Put" yapilamaz (hedef bilinmiyor).
                onSelected: it.hasCode
                    ? (_) => setState(() => _choice[it.path] = 'put')
                    : null,
              ),
              ChoiceChip(
                label: Text(s.t('review.delete')),
                selected: action == 'delete',
                selectedColor: Colors.red.shade100,
                onSelected: (_) => setState(() => _choice[it.path] = 'delete'),
              ),
            ],
          ),
        ],
      ),
    );
  }

  Widget _previewView(Strings s) {
    final plan = _plan!;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(s.count('plan.summary', plan.steps.length)),
        const SizedBox(height: 12),
        Expanded(
          child: ListView.separated(
            itemCount: plan.steps.length,
            separatorBuilder: (_, _) => const Divider(height: 1),
            itemBuilder: (_, i) {
              final st = plan.steps[i];
              final toTrash = (st.dst ?? '').contains('/.docflow/trash/') ||
                  (st.dst ?? '').contains(r'\.docflow\trash\');
              return ListTile(
                dense: true,
                leading: Icon(toTrash ? Icons.delete_outline : Icons.east,
                    color: toTrash ? Colors.red : Colors.blue),
                title: Text((st.src ?? '').split(Platform.pathSeparator).last,
                    maxLines: 1, overflow: TextOverflow.ellipsis),
                subtitle: st.noteKey == null ? null : Text(s.t(st.noteKey!)),
              );
            },
          ),
        ),
        const SizedBox(height: 12),
        Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          children: [
            TextButton(
              onPressed: () => setState(() => _stage = _Stage.list),
              child: Text(s.t('action.cancel')),
            ),
            FilledButton.icon(
              icon: const Icon(Icons.play_arrow),
              label: Text(s.t('action.apply')),
              onPressed: plan.steps.isEmpty ? null : _apply,
            ),
          ],
        ),
      ],
    );
  }

  Widget _resultView(Strings s) {
    final r = _result!;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Icon(r.failed == 0 ? Icons.check_circle : Icons.error,
                color: r.failed == 0 ? Colors.green : Colors.orange, size: 28),
            const SizedBox(width: 10),
            Text(s.count('result.applied', r.applied),
                style: const TextStyle(fontSize: 18, fontWeight: FontWeight.w600)),
            if (r.failed > 0) ...[
              const SizedBox(width: 16),
              Text(s.count('result.failed', r.failed),
                  style: const TextStyle(color: Colors.orange)),
            ],
          ],
        ),
        const Spacer(),
        Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          children: [
            OutlinedButton.icon(
              icon: const Icon(Icons.undo),
              label: Text(s.t('action.undo')),
              onPressed: r.journalId.isEmpty ? null : _undo,
            ),
            FilledButton(
              onPressed: () => Navigator.of(context).pop(true),
              child: Text(s.t('action.cancel')),
            ),
          ],
        ),
      ],
    );
  }
}
