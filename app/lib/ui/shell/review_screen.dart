import 'dart:io';

import 'package:flutter/material.dart';

import '../../core/auth/auth_service.dart';
import '../../core/cloud/cloud_history.dart';
import '../../core/ipc/sidecar_client.dart';
import '../../core/l10n/strings.dart';
import '../../core/operations/models.dart';
import '../theme/app_theme.dart';
import '../theme/responsive.dart';

/// _review/ yoneticisi: dosya bazinda Bırak / Hedefe koy / Sil (çöpe).
/// Her sey preview -> apply -> journal -> undo'dan gecer. Silme = geri donuslu
/// cope tasima; Put hedefteki dosyayi cope alip yerine koyar. Uzerine yazma YOK.
class ReviewScreen extends StatefulWidget {
  const ReviewScreen({
    super.key,
    required this.client,
    required this.dest,
    required this.auth,
  });

  final SidecarClient client;
  final String dest;
  final AuthService auth;

  @override
  State<ReviewScreen> createState() => _ReviewScreenState();
}

enum _Stage { list, previewing, preview, applying, result }

class _ReviewScreenState extends State<ReviewScreen> {
  _Stage _stage = _Stage.list;
  late final CloudHistory _cloud = CloudHistory(auth: widget.auth);
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
      // Bulut gecmisi opsiyonel — hata akisi bozmaz.
      if (_cloud.enabled && r.journalId.isNotEmpty) {
        try {
          final j = await widget.client.journalGet(r.journalId);
          await _cloud.push(
            opId: 'rfi.review',
            applied: r.applied,
            failed: r.failed,
            journalId: r.journalId,
            summary: '${r.applied}/${(j['steps'] as List?)?.length ?? r.applied}',
            steps: (j['steps'] as List?) ?? const [],
          );
        } catch (_) {/* yoksay */}
      }
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
      body: Pane(maxWidth: 860, child: _body(s)),
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
            if (snap.hasError) {
              return EmptyState(
                icon: Icons.error_outline,
                title: s.t('review.title'),
                message: snap.error.toString(),
              );
            }
            final items = snap.data ?? [];
            if (items.isEmpty) {
              return EmptyState(
                icon: Icons.done_all,
                title: s.t('review.empty'),
                message: s.t('review.empty.help'),
              );
            }
            return _listView(s, items);
          },
        );
    }
  }

  Widget _listView(Strings s, List<ReviewItem> items) {
    final pending = items.where((it) => (_choice[it.path] ?? 'keep') != 'keep').length;
    final t = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(s.t('review.help'),
            style: t.textTheme.bodySmall
                ?.copyWith(color: t.colorScheme.onSurfaceVariant)),
        const SizedBox(height: 12),
        Expanded(
          child: Card(
            child: ListView.separated(
              itemCount: items.length,
              separatorBuilder: (_, _) => const Divider(height: 1),
              itemBuilder: (_, i) => _itemTile(s, items[i]),
            ),
          ),
        ),
        const SizedBox(height: 12),
        Row(
          children: [
            Text(
              pending == 0 ? s.t('review.noChanges') : s.count('review.selected', pending),
              style: t.textTheme.bodySmall
                  ?.copyWith(color: t.colorScheme.onSurfaceVariant),
            ),
            const Spacer(),
            FilledButton.icon(
              icon: const Icon(Icons.visibility_outlined, size: 18),
              label: Text(s.t('action.preview')),
              onPressed: pending > 0 ? () => _preview(items) : null,
            ),
          ],
        ),
      ],
    );
  }

  Widget _itemTile(Strings s, ReviewItem it) {
    final t = Theme.of(context);
    final action = _choice[it.path] ?? 'keep';
    final sub = <String>[
      if (it.hasCode) it.code else s.t('review.noCode'),
      if (it.targetExists) s.t('review.willReplace'),
    ].join('  ·  ');
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 12, 12, 12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(it.hasCode ? Severity.info.icon : Severity.warn.icon, size: 18),
              const SizedBox(width: 10),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(it.name,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: t.textTheme.bodyMedium
                            ?.copyWith(fontWeight: FontWeight.w600)),
                    Text(sub,
                        style: t.textTheme.bodySmall
                            ?.copyWith(color: t.colorScheme.onSurfaceVariant)),
                  ],
                ),
              ),
              TextButton.icon(
                icon: const Icon(Icons.open_in_new, size: 16),
                label: Text(s.t('action.open')),
                onPressed: () => _openFile(it.path),
              ),
            ],
          ),
          const SizedBox(height: 8),
          Padding(
            padding: const EdgeInsets.only(left: 28),
            child: Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                _Choice(
                  label: s.t('review.keep'),
                  icon: Icons.remove,
                  selected: action == 'keep',
                  onTap: () => setState(() => _choice[it.path] = 'keep'),
                ),
                _Choice(
                  label: s.t('review.put'),
                  icon: Icons.east,
                  selected: action == 'put',
                  // Kod okunamayan dosyaya "Put" yapilamaz (hedef bilinmiyor).
                  onTap: it.hasCode
                      ? () => setState(() => _choice[it.path] = 'put')
                      : null,
                ),
                _Choice(
                  label: s.t('review.delete'),
                  icon: Icons.delete_outline,
                  selected: action == 'delete',
                  // Yikici secim: secilince kalin cerceve alir (renk yok).
                  danger: true,
                  onTap: () => setState(() => _choice[it.path] = 'delete'),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  Widget _previewView(Strings s) {
    final plan = _plan!;
    final t = Theme.of(context);
    final trashCount = plan.steps.where(_toTrash).length;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Text(s.count('plan.summary', plan.steps.length),
                style: t.textTheme.titleSmall?.copyWith(fontWeight: FontWeight.w600)),
            const Spacer(),
            // Cope giden dosya sayisini ACIKCA soyle — geri alinabilir ama sessiz degil.
            if (trashCount > 0)
              Text(s.count('review.toTrash', trashCount),
                  style: t.textTheme.bodySmall?.copyWith(fontWeight: FontWeight.w600)),
          ],
        ),
        const SizedBox(height: 10),
        Expanded(
          child: Card(
            child: ListView.separated(
              itemCount: plan.steps.length,
              separatorBuilder: (_, _) => const Divider(height: 1),
              itemBuilder: (_, i) {
                final st = plan.steps[i];
                final trash = _toTrash(st);
                return ListTile(
                  dense: true,
                  leading:
                      Icon(trash ? Icons.delete_outline : Severity.info.icon, size: 18),
                  title: Text((st.src ?? '').split(RegExp(r'[/\\]')).last,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: t.textTheme.bodyMedium?.copyWith(
                          fontWeight: trash ? FontWeight.w600 : FontWeight.w400)),
                  subtitle: st.noteKey == null
                      ? null
                      : Text(s.t(st.noteKey!),
                          style: t.textTheme.bodySmall
                              ?.copyWith(color: t.colorScheme.onSurfaceVariant)),
                );
              },
            ),
          ),
        ),
        const SizedBox(height: 12),
        Row(
          children: [
            TextButton(
              onPressed: () => setState(() => _stage = _Stage.list),
              child: Text(s.t('action.cancel')),
            ),
            const Spacer(),
            FilledButton.icon(
              icon: const Icon(Icons.play_arrow, size: 18),
              label: Text(s.t('action.apply')),
              onPressed: plan.steps.isEmpty ? null : _apply,
            ),
          ],
        ),
      ],
    );
  }

  static bool _toTrash(PlanStep st) =>
      (st.dst ?? '').contains('/.docflow/trash/') ||
      (st.dst ?? '').contains(r'\.docflow\trash\');

  Widget _resultView(Strings s) {
    final r = _result!;
    final t = Theme.of(context);
    final ok = r.failed == 0;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Container(
              padding: const EdgeInsets.all(10),
              decoration: BoxDecoration(
                color: t.colorScheme.surfaceContainerHigh,
                shape: BoxShape.circle,
                border: ok ? null : Border.all(color: t.colorScheme.onSurface, width: 1.5),
              ),
              child: Icon(ok ? Severity.ok.icon : Severity.warn.icon, size: 20),
            ),
            const SizedBox(width: 12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(s.count('result.applied', r.applied),
                      style: t.textTheme.titleMedium
                          ?.copyWith(fontWeight: FontWeight.w600)),
                  if (r.failed > 0)
                    Text(s.count('result.failed', r.failed),
                        style: t.textTheme.bodySmall
                            ?.copyWith(fontWeight: FontWeight.w600)),
                ],
              ),
            ),
          ],
        ),
        const SizedBox(height: 12),
        // Silinenler geri alinabilir: kullanici nereye gittigini bilsin.
        Text(s.t('review.trashHint'),
            style: t.textTheme.bodySmall
                ?.copyWith(color: t.colorScheme.onSurfaceVariant)),
        const Spacer(),
        Row(
          children: [
            OutlinedButton.icon(
              icon: const Icon(Icons.undo, size: 18),
              label: Text(s.t('action.undo')),
              onPressed: r.journalId.isEmpty ? null : _undo,
            ),
            const Spacer(),
            FilledButton(
              onPressed: () => Navigator.of(context).pop(true),
              child: Text(s.t('action.done')),
            ),
          ],
        ),
      ],
    );
  }
}

/// Monokrom secim rozeti. Secili = dolu zemin; yikici secim = kalin cerceve.
class _Choice extends StatelessWidget {
  const _Choice({
    required this.label,
    required this.icon,
    required this.selected,
    required this.onTap,
    this.danger = false,
  });

  final String label;
  final IconData icon;
  final bool selected;
  final VoidCallback? onTap;
  final bool danger;

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context);
    final enabled = onTap != null;
    final fg = !enabled
        ? t.colorScheme.onSurfaceVariant.withValues(alpha: 0.5)
        : (selected ? t.colorScheme.onPrimary : t.colorScheme.onSurface);
    return Opacity(
      opacity: enabled ? 1 : 0.55,
      child: Material(
        color: selected ? t.colorScheme.primary : t.colorScheme.surfaceContainer,
        borderRadius: BorderRadius.circular(8),
        child: InkWell(
          borderRadius: BorderRadius.circular(8),
          onTap: onTap,
          child: Container(
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
            decoration: BoxDecoration(
              borderRadius: BorderRadius.circular(8),
              border: Border.all(
                color: selected && danger
                    ? t.colorScheme.onSurface
                    : (selected ? Colors.transparent : t.colorScheme.outline),
                width: selected && danger ? 2 : 1,
              ),
            ),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(icon, size: 15, color: fg),
                const SizedBox(width: 6),
                Text(label,
                    style: t.textTheme.labelMedium?.copyWith(
                      color: fg,
                      fontWeight: selected ? FontWeight.w700 : FontWeight.w500,
                    )),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
