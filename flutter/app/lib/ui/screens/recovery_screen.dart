// RecoveryScreen, ported from frontend/src/App.tsx. It lists journals that
// were interrupted and need a decision, and the print queue that can be
// resumed or discarded. Restoring an original is the only undo offered here.

import 'package:flutter/material.dart' hide IconButton, Switch;

import '../../app/human_error.dart';
import '../../app/routes.dart';
import '../../app/screen_registry.dart';
import '../../bridge/bridge.dart';
import '../../l10n/strings.dart';
import '../../models/models.dart';
import '../../theme/app_icons.dart';
import '../../theme/tokens.dart';
import '../widgets/metrics.dart';
import '../widgets/widgets.dart';
import 'screen_common.dart';

/// Registers the recovery screen for [ScreenId.recovery].
void registerRecoveryScreen(ScreenRegistry registry) {
  registry.register(ScreenId.recovery, (ctx) => RecoveryScreen(ctx: ctx));
}

class RecoveryScreen extends StatefulWidget {
  const RecoveryScreen({super.key, required this.ctx});

  final ScreenContext ctx;

  @override
  State<RecoveryScreen> createState() => _RecoveryScreenState();
}

class _RecoveryScreenState extends State<RecoveryScreen> {
  RecoveryPayload? _payload;
  bool _busy = false;
  String _error = '';

  Bridge get _bridge => widget.ctx.bridge;
  Strings get _s => widget.ctx.strings;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    setState(() {
      _busy = true;
      _error = '';
    });
    try {
      final payload = await _bridge.call(FrontendCommands.recoveryList);
      if (mounted) setState(() => _payload = payload);
    } catch (reason) {
      if (mounted) setState(() => _error = humanError(reason, _s));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _discardQueue() async {
    final queue = _payload?.printQueue;
    if (queue == null) return;
    setState(() {
      _busy = true;
      _error = '';
    });
    try {
      await _bridge.call(
        FrontendCommands.enginePrintQueueDiscard,
        params: (queueId: queue.queueId),
      );
      await _load();
    } catch (reason) {
      if (mounted) {
        setState(() {
          _error = humanError(reason, _s);
          _busy = false;
        });
      }
    }
  }

  Future<void> _restoreOriginal(String journalId) async {
    setState(() {
      _busy = true;
      _error = '';
    });
    try {
      final result = await _bridge.call(
        FrontendCommands.engineUndo,
        params: (journalId: journalId, cloudId: null, destination: null),
      );
      final payload = await _bridge.call(FrontendCommands.recoveryList);
      if (!mounted) return;
      setState(() => _payload = payload);
      if (result.failed > 0) {
        setState(() => _error = _s.t('error.rfi.followUp.recoveryRequired'));
      }
    } catch (reason) {
      if (mounted) setState(() => _error = humanError(reason, _s));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// Opens the batch print operation so its queue can be reviewed.
  void _openPrintQueue() {
    for (final operation in widget.ctx.bootstrap.operations) {
      if (operation.id == 'files.batch_print') {
        widget.ctx.navigate(OperationRoute(operation: operation));
        return;
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final s = _s;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Header(
          title: s.t('recovery.title'),
          onBack: widget.ctx.goHome,
          backLabel: s.t('action.back'),
          trailing: IconButton(
            label: s.t('action.retry'),
            onPressed: () => _load(),
            child: const Icon(AppIcons.refreshCw),
          ),
        ),
        Expanded(child: _buildBody(context, s)),
      ],
    );
  }

  Widget _buildBody(BuildContext context, Strings s) {
    final payload = _payload;
    if (_busy && payload == null) {
      return BusyState(label: s.t('recovery.title'), s: s);
    }
    final t = DocFlowTokens.of(context);
    final journals = payload?.journals ?? const <RecoveryEntry>[];
    final queue = payload?.printQueue;
    final showEmpty = payload != null && journals.isEmpty && queue == null;
    return SingleChildScrollView(
      padding: const EdgeInsets.all(16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          PageHeading(
            title: s.t('recovery.title'),
            subtitle: s.t('recovery.help'),
          ),
          for (final entry in journals)
            ScreenRow(
              key: ValueKey(entry.journalId),
              icon: const Icon(AppIcons.circleAlert),
              title: s.t(
                entry.disposition == RecoveryDisposition.noEffect
                    ? 'recovery.noEffect'
                    : 'recovery.outcomeUnknown',
              ),
              detail: entry.journalId,
              trailing: [
                StatusLabel(label: s.t('recovery.manual')),
                if (entry.restoreOriginalAvailable == true)
                  DfButton(
                    variant: DfButtonVariant.secondary,
                    icon: const Icon(AppIcons.undo),
                    onPressed: _busy
                        ? null
                        : () => _restoreOriginal(entry.journalId),
                    child: Text(s.t('recovery.restoreOriginal')),
                  ),
              ],
            ),
          if (queue != null)
            Container(
              margin: const EdgeInsets.only(top: 14),
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(
                color: t.surface,
                border: Border.all(color: t.lineStrong),
                borderRadius: BorderRadius.circular(10),
              ),
              child: Wrap(
                alignment: WrapAlignment.spaceBetween,
                crossAxisAlignment: WrapCrossAlignment.center,
                spacing: 12,
                runSpacing: 10,
                children: [
                  Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Text(
                        s.t('recovery.printQueue'),
                        style: TextStyle(
                          fontWeight: FontWeight.w700,
                          color: t.text,
                        ),
                      ),
                      const SizedBox(height: 3),
                      Text(
                        s.t('print.resume.summary', {'count': queue.resumable}),
                        style: TextStyle(
                          fontSize: remPx(context, 0.76),
                          color: t.muted,
                        ),
                      ),
                    ],
                  ),
                  Wrap(
                    spacing: 7,
                    runSpacing: 7,
                    crossAxisAlignment: WrapCrossAlignment.center,
                    children: [
                      DfButton(
                        variant: DfButtonVariant.ghost,
                        onPressed: _busy ? null : () => _discardQueue(),
                        child: Text(s.t('print.resume.discard')),
                      ),
                      DfButton(
                        variant: DfButtonVariant.secondary,
                        icon: const Icon(AppIcons.rotateCcw),
                        onPressed: _busy ? null : _openPrintQueue,
                        child: Text(s.t('print.resume.review')),
                      ),
                    ],
                  ),
                ],
              ),
            ),
          if (showEmpty)
            EmptyState(
              icon: const Icon(AppIcons.checkCircle),
              title: s.t('recovery.empty'),
              message: s.t('recovery.empty.help'),
            ),
          if (_error.isNotEmpty) ...[
            const SizedBox(height: 12),
            Notice(kind: NoticeKind.error, child: Text(_error)),
          ],
        ],
      ),
    );
  }
}
