import 'dart:async';

import 'package:flutter/material.dart';

import '../../core/cloud/cloud_history.dart';
import '../../core/ipc/sidecar_client.dart';
import '../../core/l10n/strings.dart';
import '../../core/operations/models.dart';
import '../../core/settings/settings_store.dart';
import '../forms/schema_form.dart';
import 'review_screen.dart';

enum _Stage { form, previewing, preview, applying, result }

class OperationFlowScreen extends StatefulWidget {
  const OperationFlowScreen({
    super.key,
    required this.client,
    required this.op,
    required this.settings,
  });

  final SidecarClient client;
  final OperationDef op;
  final SettingsStore settings;

  @override
  State<OperationFlowScreen> createState() => _OperationFlowScreenState();
}

class _OperationFlowScreenState extends State<OperationFlowScreen> {
  final Map<String, dynamic> _values = {};
  final CloudHistory _cloud = CloudHistory();
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
      builder: (_) => ReviewScreen(client: widget.client, dest: dest),
    ));
  }

  @override
  Widget build(BuildContext context) {
    final s = Strings.of(context);
    return Scaffold(
      appBar: AppBar(title: Text(s.t(widget.op.titleKey))),
      body: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 760),
          child: Padding(
            padding: const EdgeInsets.all(20),
            child: _body(s),
          ),
        ),
      ),
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
    return ListView(
      children: [
        SchemaForm(
          params: widget.op.params,
          values: _values,
          onChanged: () {},
        ),
        const SizedBox(height: 24),
        Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          children: [
            TextButton.icon(
              icon: const Icon(Icons.rate_review_outlined),
              label: Text(s.t('action.review')),
              onPressed: _openReview,
            ),
            FilledButton.icon(
              icon: const Icon(Icons.visibility_outlined),
              label: Text(s.t('action.preview')),
              onPressed: _preview,
            ),
          ],
        ),
      ],
    );
  }

  Widget _previewView(Strings s) {
    final plan = _plan!;
    final canApply = plan.hasActionableSteps;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (plan.warnings.isNotEmpty) _warnings(s, plan.warnings),
        Text(s.count('plan.summary',
            plan.steps.where((e) => e.kind != 'mkdir').length)),
        const SizedBox(height: 12),
        Expanded(
          child: plan.steps.isEmpty
              ? Center(child: Text(s.t('plan.empty')))
              : ListView.separated(
                  itemCount: plan.steps.length,
                  separatorBuilder: (_, _) => const Divider(height: 1),
                  itemBuilder: (_, i) => _stepTile(s, plan.steps[i]),
                ),
        ),
        const SizedBox(height: 12),
        Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          children: [
            TextButton(
              onPressed: () => setState(() => _stage = _Stage.form),
              child: Text(s.t('action.cancel')),
            ),
            FilledButton.icon(
              icon: const Icon(Icons.play_arrow),
              label: Text(s.t('action.apply')),
              // Plan bos veya sadece uyari iceriyorsa PASIF (TASKS 3.2).
              onPressed: canApply ? _apply : null,
            ),
          ],
        ),
      ],
    );
  }

  Widget _warnings(Strings s, List<PlanWarning> warnings) {
    return Container(
      width: double.infinity,
      margin: const EdgeInsets.only(bottom: 16),
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: Colors.amber.shade50,
        borderRadius: BorderRadius.circular(10),
        border: Border.all(color: Colors.amber.shade300),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(s.t('ui.warnings'),
              style: const TextStyle(fontWeight: FontWeight.w700)),
          const SizedBox(height: 6),
          for (final w in warnings)
            Text('• ${s.t(w.key)}${w.subject.isEmpty ? '' : ' — ${w.subject}'}',
                style: const TextStyle(fontSize: 13)),
        ],
      ),
    );
  }

  Widget _stepTile(Strings s, PlanStep step) {
    final toReview = (step.dst ?? '').contains('/_review/');
    final color = step.kind == 'mkdir'
        ? Colors.grey
        : (toReview ? Colors.amber.shade800 : Colors.blue);
    final icon = step.kind == 'mkdir'
        ? Icons.create_new_folder_outlined
        : (toReview ? Icons.rate_review_outlined : Icons.east);
    final dst = step.dst ?? '';
    final name = dst.split('/').isEmpty ? dst : dst.split('/').last;
    return ListTile(
      dense: true,
      leading: Icon(icon, color: color),
      title: Text(name, maxLines: 1, overflow: TextOverflow.ellipsis),
      subtitle: Text(
        [
          if (step.src != null) step.src!.split('/').last,
          if (step.noteKey != null) s.t(step.noteKey!),
        ].join('  ·  '),
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
      ),
      trailing: step.confidence == null
          ? null
          : Chip(
              visualDensity: VisualDensity.compact,
              label: Text(step.confidence!.toStringAsFixed(2)),
            ),
    );
  }

  Widget _applyingView(Strings s) {
    return Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          SizedBox(
            width: 320,
            child: LinearProgressIndicator(value: _applyFraction == 0 ? null : _applyFraction),
          ),
          const SizedBox(height: 12),
          Text('${(_applyFraction * 100).round()}%'),
        ],
      ),
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
        const SizedBox(height: 12),
        if (r.errors.isNotEmpty)
          Expanded(
            child: ListView(
              children: [
                for (final e in r.errors)
                  ListTile(
                    dense: true,
                    leading: const Icon(Icons.warning_amber, color: Colors.orange),
                    title: Text(s.t(e.key)),
                    subtitle: e.subject.isEmpty ? null : Text(e.subject),
                  ),
              ],
            ),
          )
        else
          const Spacer(),
        Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          children: [
            OutlinedButton.icon(
              icon: const Icon(Icons.undo),
              label: Text(s.t('action.undo')),
              onPressed: r.journalId.isEmpty ? null : _undo,
            ),
            TextButton.icon(
              icon: const Icon(Icons.rate_review_outlined),
              label: Text(s.t('action.review')),
              onPressed: _openReview,
            ),
            FilledButton(
              onPressed: () => Navigator.of(context).pop(),
              child: Text(s.t('action.cancel')),
            ),
          ],
        ),
      ],
    );
  }
}
