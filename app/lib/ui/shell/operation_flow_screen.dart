import 'dart:async';

import 'package:flutter/material.dart';

import '../../core/auth/auth_service.dart';
import '../../core/cloud/cloud_history.dart';
import '../../core/ipc/sidecar_client.dart';
import '../../core/l10n/strings.dart';
import '../../core/operations/models.dart';
import '../../core/settings/settings_store.dart';
import '../forms/schema_form.dart';
import '../theme/app_theme.dart';
import '../theme/responsive.dart';
import 'review_screen.dart';

enum _Stage { form, previewing, preview, applying, result }

class OperationFlowScreen extends StatefulWidget {
  const OperationFlowScreen({
    super.key,
    required this.client,
    required this.op,
    required this.settings,
    required this.auth,
  });

  final SidecarClient client;
  final OperationDef op;
  final SettingsStore settings;
  final AuthService auth;

  @override
  State<OperationFlowScreen> createState() => _OperationFlowScreenState();
}

class _OperationFlowScreenState extends State<OperationFlowScreen> {
  final Map<String, dynamic> _values = {};
  late final CloudHistory _cloud = CloudHistory(auth: widget.auth);
  _Stage _stage = _Stage.form;
  Plan? _plan;
  ApplyResult? _result;
  double _applyFraction = 0;
  StreamSubscription<Progress>? _sub;

  @override
  void initState() {
    super.initState();
    // Son kez secilen klasorleri/degerleri geri yukle (sadece bu op'un semasi).
    final saved = widget.settings.paramsFor(widget.op.id);
    final known = {for (final p in widget.op.params) p.name};
    saved.forEach((k, v) {
      if (known.contains(k)) _values[k] = v;
    });
  }

  @override
  void dispose() {
    _sub?.cancel();
    super.dispose();
  }

  Future<void> _preview() async {
    // Kullanicinin sectigi klasorleri hatirla (bir sonraki acilista geri gelir).
    await widget.settings.saveParams(widget.op.id, _values);
    setState(() => _stage = _Stage.previewing);
    try {
      final plan = await widget.client.preview(widget.op.id, _values);
      setState(() {
        _plan = plan;
        _stage = _Stage.preview;
      });
    } catch (e) {
      setState(() => _stage = _Stage.form);
      _snack(e.toString());
    }
  }

  Future<void> _apply() async {
    final plan = _plan;
    if (plan?.planId == null) return;
    setState(() {
      _stage = _Stage.applying;
      _applyFraction = 0;
    });
    _sub = widget.client.progress.listen((p) {
      if (p.phase == 'apply') setState(() => _applyFraction = p.fraction);
    });
    try {
      final res = await widget.client.apply(plan!.planId!);
      setState(() {
        _result = res;
        _stage = _Stage.result;
      });
      await _syncCloud(widget.op.id, res);
    } catch (e) {
      setState(() => _stage = _Stage.preview);
      _snack(e.toString());
    } finally {
      await _sub?.cancel();
    }
  }

  /// Gecmisi buluta yolla. Bulut OPSIYONEL: hata uygulamayi etkilemez,
  /// yerel journal her zaman gercek kaynaktir.
  Future<void> _syncCloud(String opId, ApplyResult r) async {
    if (!_cloud.enabled || r.journalId.isEmpty) return;
    try {
      final j = await widget.client.journalGet(r.journalId);
      await _cloud.push(
        opId: opId,
        applied: r.applied,
        failed: r.failed,
        journalId: r.journalId,
        summary: '${r.applied}/${(j['steps'] as List?)?.length ?? r.applied}',
        steps: (j['steps'] as List?) ?? const [],
      );
    } catch (_) {/* bulut opsiyonel */}
  }

  Future<void> _undo() async {
    final res = _result;
    if (res == null || res.journalId.isEmpty) return;
    try {
      await widget.client.undo(res.journalId);
      if (mounted) Navigator.of(context).pop();
    } catch (e) {
      _snack(e.toString());
    }
  }

  void _snack(String msg) {
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(msg)));
  }

  void _openReview() {
    final dest = (_values['dest'] as String?)?.trim();
    if (dest == null || dest.isEmpty) {
      _snack(Strings.of(context).t('error.param.required.dest'));
      return;
    }
    Navigator.of(context).push(MaterialPageRoute(
      builder: (_) => ReviewScreen(
        client: widget.client,
        dest: dest,
        auth: widget.auth,
      ),
    ));
  }

  /// Akisin neresindeyiz? Kullanici her an nerede oldugunu gormeli.
  int get _stepIndex => switch (_stage) {
        _Stage.form => 0,
        _Stage.previewing || _Stage.preview => 1,
        _Stage.applying || _Stage.result => 2,
      };

  @override
  Widget build(BuildContext context) {
    final s = Strings.of(context);
    return Scaffold(
      appBar: AppBar(
        title: Text(s.t(widget.op.titleKey)),
        bottom: PreferredSize(
          preferredSize: const Size.fromHeight(34),
          child: Padding(
            padding: const EdgeInsets.only(bottom: 8),
            child: _Steps(index: _stepIndex),
          ),
        ),
      ),
      body: Pane(maxWidth: 820, child: _body(s)),
    );
  }

  Widget _body(Strings s) {
    switch (_stage) {
      case _Stage.form:
        return _formView(s);
      case _Stage.previewing:
        return const Center(child: CircularProgressIndicator());
      case _Stage.preview:
        return _previewView(s);
      case _Stage.applying:
        return _applyingView(s);
      case _Stage.result:
        return _resultView(s);
    }
  }

  Widget _formView(Strings s) {
    return Column(
      children: [
        Expanded(
          child: ListView(
            padding: const EdgeInsets.only(bottom: 16),
            children: [
              Card(
                child: Padding(
                  padding: const EdgeInsets.all(16),
                  child: SchemaForm(
                    params: widget.op.params,
                    values: _values,
                    onChanged: () {},
                  ),
                ),
              ),
            ],
          ),
        ),
        _Bar(
          left: TextButton.icon(
            icon: const Icon(Icons.folder_open_outlined, size: 18),
            label: Text(s.t('action.review')),
            onPressed: _openReview,
          ),
          right: FilledButton.icon(
            icon: const Icon(Icons.visibility_outlined, size: 18),
            label: Text(s.t('action.preview')),
            onPressed: _preview,
          ),
        ),
      ],
    );
  }

  Widget _previewView(Strings s) {
    final plan = _plan!;
    final canApply = plan.hasActionableSteps;
    final moves = plan.steps.where((e) => e.kind != 'mkdir').length;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (plan.warnings.isNotEmpty) ...[
          _WarningBox(warnings: plan.warnings),
          const SizedBox(height: 16),
        ],
        Row(
          children: [
            Text(s.count('plan.summary', moves),
                style: Theme.of(context)
                    .textTheme
                    .titleSmall
                    ?.copyWith(fontWeight: FontWeight.w600)),
            const Spacer(),
            // Uygulanmadan once oku: bu plan neyi nereye tasiyacak.
            Text(s.t('plan.readOnly'),
                style: Theme.of(context).textTheme.bodySmall?.copyWith(
                    color: Theme.of(context).colorScheme.onSurfaceVariant)),
          ],
        ),
        const SizedBox(height: 10),
        Expanded(
          child: plan.steps.isEmpty
              ? EmptyState(icon: Icons.inbox_outlined, title: s.t('plan.empty'))
              : Card(
                  child: ListView.separated(
                    itemCount: plan.steps.length,
                    separatorBuilder: (_, _) => const Divider(height: 1),
                    itemBuilder: (_, i) => _StepTile(step: plan.steps[i]),
                  ),
                ),
        ),
        const SizedBox(height: 12),
        _Bar(
          left: TextButton(
            onPressed: () => setState(() => _stage = _Stage.form),
            child: Text(s.t('action.cancel')),
          ),
          right: FilledButton.icon(
            icon: const Icon(Icons.play_arrow, size: 18),
            label: Text(s.t('action.apply')),
            // Plan bos veya sadece uyari iceriyorsa PASIF (TASKS 3.2).
            onPressed: canApply ? _apply : null,
          ),
        ),
      ],
    );
  }

  Widget _applyingView(Strings s) {
    return Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          SizedBox(
            width: 280,
            child: ClipRRect(
              borderRadius: BorderRadius.circular(4),
              child: LinearProgressIndicator(
                value: _applyFraction == 0 ? null : _applyFraction,
                minHeight: 6,
              ),
            ),
          ),
          const SizedBox(height: 14),
          Text('${(_applyFraction * 100).round()}%',
              style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 4),
          Text(s.t('plan.applying'),
              style: Theme.of(context).textTheme.bodySmall?.copyWith(
                  color: Theme.of(context).colorScheme.onSurfaceVariant)),
        ],
      ),
    );
  }

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
                // Basarisizlik varsa cerceve kalinlasir — monokromda vurgu bu.
                border: ok ? null : Border.all(color: t.colorScheme.onSurface, width: 1.5),
              ),
              child: Icon(
                  ok ? Severity.ok.icon : Severity.warn.icon,
                  size: 20),
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
        const SizedBox(height: 16),
        if (r.errors.isNotEmpty)
          Expanded(
            child: Card(
              child: ListView.separated(
                itemCount: r.errors.length,
                separatorBuilder: (_, _) => const Divider(height: 1),
                itemBuilder: (_, i) => ListTile(
                  dense: true,
                  leading: Icon(Severity.warn.icon, size: 18),
                  title: Text(s.t(r.errors[i].key)),
                  subtitle:
                      r.errors[i].subject.isEmpty ? null : Text(r.errors[i].subject),
                ),
              ),
            ),
          )
        else
          const Spacer(),
        const SizedBox(height: 12),
        _Bar(
          left: OutlinedButton.icon(
            icon: const Icon(Icons.undo, size: 18),
            label: Text(s.t('action.undo')),
            onPressed: r.journalId.isEmpty ? null : _undo,
          ),
          middle: TextButton.icon(
            icon: const Icon(Icons.folder_open_outlined, size: 18),
            label: Text(s.t('action.review')),
            onPressed: _openReview,
          ),
          right: FilledButton(
            onPressed: () => Navigator.of(context).pop(),
            child: Text(s.t('action.done')),
          ),
        ),
      ],
    );
  }
}

/// Akis gostergesi: Form -> Onizle -> Uygula. Dolu kare = tamamlandi/aktif.
class _Steps extends StatelessWidget {
  const _Steps({required this.index});

  final int index;

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context);
    final s = Strings.of(context);
    final labels = [s.t('step.form'), s.t('step.preview'), s.t('step.apply')];
    return Row(
      mainAxisAlignment: MainAxisAlignment.center,
      children: [
        for (var i = 0; i < labels.length; i++) ...[
          if (i > 0)
            Container(
              width: 20,
              height: 1,
              margin: const EdgeInsets.symmetric(horizontal: 8),
              color: t.colorScheme.outlineVariant,
            ),
          Row(
            children: [
              Container(
                width: 7,
                height: 7,
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  color: i <= index ? t.colorScheme.onSurface : Colors.transparent,
                  border: Border.all(
                    color: i <= index
                        ? t.colorScheme.onSurface
                        : t.colorScheme.outline,
                  ),
                ),
              ),
              const SizedBox(width: 6),
              Text(
                labels[i],
                style: t.textTheme.labelSmall?.copyWith(
                  color: i <= index
                      ? t.colorScheme.onSurface
                      : t.colorScheme.onSurfaceVariant,
                  fontWeight: i == index ? FontWeight.w700 : FontWeight.w400,
                ),
              ),
            ],
          ),
        ],
      ],
    );
  }
}

/// Alt aksiyon cubugu — her adimda ayni yerde, ayni hizada.
class _Bar extends StatelessWidget {
  const _Bar({required this.left, required this.right, this.middle});

  final Widget left;
  final Widget? middle;
  final Widget right;

  @override
  Widget build(BuildContext context) {
    return Row(
      children: [
        left,
        const Spacer(),
        if (middle != null) ...[middle!, const SizedBox(width: 8)],
        right,
      ],
    );
  }
}

class _WarningBox extends StatelessWidget {
  const _WarningBox({required this.warnings});

  final List<PlanWarning> warnings;

  @override
  Widget build(BuildContext context) {
    final s = Strings.of(context);
    final t = Theme.of(context);
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.all(14),
      decoration: BoxDecoration(
        color: t.colorScheme.surfaceContainerHigh,
        borderRadius: BorderRadius.circular(10),
        // Uyari renkle degil, KALIN cerceveyle one cikar (monokrom).
        border: Border.all(color: t.colorScheme.onSurface, width: 1.5),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(Severity.warn.icon, size: 18),
              const SizedBox(width: 8),
              Text(s.t('ui.warnings'),
                  style: t.textTheme.labelLarge?.copyWith(fontWeight: FontWeight.w700)),
            ],
          ),
          const SizedBox(height: 8),
          for (final w in warnings)
            Padding(
              padding: const EdgeInsets.only(top: 2),
              // SECILEBILIR: teshis metnini kullanicinin kopyalayip
              // gonderebilmesi lazim. Kopyalanamayan hata mesaji, destek
              // isteyen kisiyi ekran fotografi cekmeye zorlar.
              child: SelectableText(
                '— ${s.t(w.key)}${w.subject.isEmpty ? '' : '  ${w.subject}'}',
                style: t.textTheme.bodySmall,
              ),
            ),
        ],
      ),
    );
  }
}

class _StepTile extends StatelessWidget {
  const _StepTile({required this.step});

  final PlanStep step;

  @override
  Widget build(BuildContext context) {
    final s = Strings.of(context);
    final t = Theme.of(context);
    final dst = step.dst ?? '';
    final toReview = dst.contains('/_review/') || dst.contains(r'\_review\');
    final sev = step.kind == 'mkdir'
        ? Severity.neutral
        : (toReview ? Severity.warn : Severity.info);
    final name = dst.split(RegExp(r'[/\\]')).last;
    final conf = step.confidence;

    return ListTile(
      dense: true,
      leading: Icon(sev.icon, size: 18),
      title: Text(name,
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          style: t.textTheme.bodyMedium?.copyWith(
            fontWeight: sev.emphasized ? FontWeight.w600 : FontWeight.w400,
          )),
      subtitle: Text(
        [
          if (step.src != null) step.src!.split(RegExp(r'[/\\]')).last,
          if (step.noteKey != null) s.t(step.noteKey!),
        ].join('  ·  '),
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
        style: t.textTheme.bodySmall?.copyWith(color: t.colorScheme.onSurfaceVariant),
      ),
      trailing: conf == null
          ? null
          : Chip(
              visualDensity: VisualDensity.compact,
              label: Text('${(conf * 100).round()}%'),
            ),
    );
  }
}
