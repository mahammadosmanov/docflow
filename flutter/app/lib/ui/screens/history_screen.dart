// HistoryScreen, ported from frontend/src/App.tsx. It lists the recent
// journals from `history/list`, shows whether the list came from the cloud or
// the local journal, and undoes a journal either in place or into a folder the
// user picks.

import 'package:flutter/material.dart' hide IconButton, Switch;

import '../../app/human_error.dart';
import '../../app/routes.dart';
import '../../app/screen_registry.dart';
import '../../bridge/bridge.dart';
import '../../bridge/platform.dart';
import '../../l10n/strings.dart';
import '../../models/models.dart';
import '../../theme/app_icons.dart';
import '../../theme/tokens.dart';
import '../widgets/metrics.dart';
import '../widgets/widgets.dart';
import 'screen_common.dart';

/// Registers the history screen for [ScreenId.history].
void registerHistoryScreen(ScreenRegistry registry) {
  registry.register(ScreenId.history, (ctx) => HistoryScreen(ctx: ctx));
}

/// The title shown for a history row (`operations.find(...)` in App.tsx).
String historyRowTitleKey(HistoryRow row, List<OperationDef> operations) {
  switch (row.opId) {
    case 'rfi.review':
      return 'review.title';
    case 'rfi.process_scans':
      return 'op.rfi.process_scans.title';
  }
  for (final operation in operations) {
    if (operation.id == row.opId) return operation.titleKey;
  }
  return 'history.unknownOperation';
}

class HistoryScreen extends StatefulWidget {
  const HistoryScreen({super.key, required this.ctx});

  final ScreenContext ctx;

  @override
  State<HistoryScreen> createState() => _HistoryScreenState();
}

class _HistoryScreenState extends State<HistoryScreen> {
  HistoryPayload? _payload;
  HistoryRow? _undoRow;
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
    setState(() => _busy = true);
    try {
      final payload = await _bridge.call(
        FrontendCommands.historyList,
        params: (limit: 50),
      );
      if (mounted) setState(() => _payload = payload);
    } catch (reason) {
      if (mounted) setState(() => _error = humanError(reason, _s));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// Undoes [row]. A [destination] restores the files into that folder
  /// instead of their original places.
  Future<void> _undo(HistoryRow row, {PathSelection? destination}) async {
    setState(() {
      _undoRow = null;
      _busy = true;
    });
    try {
      await _bridge.call(
        FrontendCommands.engineUndo,
        params: (
          journalId: row.journalId,
          cloudId: row.cloudId,
          destination: destination,
        ),
      );
      await _load();
    } catch (reason) {
      if (mounted) setState(() => _error = humanError(reason, _s));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _undoToFolder(HistoryRow row) async {
    try {
      final folder = await chooseFolder(_bridge, 'history.undo', 'destination');
      if (folder != null) await _undo(row, destination: folder);
    } catch (reason) {
      if (mounted) setState(() => _error = humanError(reason, _s));
    }
  }

  @override
  Widget build(BuildContext context) {
    final s = _s;
    final payload = _payload;
    final undoRow = _undoRow;
    final body = _buildBody(context, s, payload);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Header(
          title: s.t('history.title'),
          onBack: widget.ctx.goHome,
          backLabel: s.t('action.back'),
          trailing: IconButton(
            label: s.t('action.retry'),
            onPressed: () => _load(),
            child: const Icon(AppIcons.refreshCw),
          ),
        ),
        Expanded(
          child: Stack(
            children: [
              Positioned.fill(child: body),
              if (undoRow != null)
                Modal(
                  title: s.t('action.undo'),
                  s: s,
                  onClose: () => setState(() => _undoRow = null),
                  child: _UndoChoice(
                    s: s,
                    moves: undoRow.moves,
                    onOriginal: () => _undo(undoRow),
                    onFolder: () => _undoToFolder(undoRow),
                  ),
                ),
            ],
          ),
        ),
      ],
    );
  }

  Widget _buildBody(BuildContext context, Strings s, HistoryPayload? payload) {
    if (_busy && payload == null) {
      return BusyState(label: s.t('history.title'), s: s);
    }
    final t = DocFlowTokens.of(context);
    final online = payload?.online ?? false;
    final rows = payload?.rows ?? const <HistoryRow>[];
    return SingleChildScrollView(
      padding: const EdgeInsets.all(16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          PageHeading(
            title: s.t('history.title'),
            subtitle: online
                ? s.t('history.sourceCloud')
                : s.t('history.sourceLocal'),
            trailing: SafeBadge(
              icon: Icon(online ? AppIcons.check : AppIcons.history),
              label: s.t(online ? 'history.cloud' : 'history.local'),
            ),
          ),
          if (rows.isNotEmpty)
            for (final row in rows)
              ScreenRow(
                key: ValueKey('${row.cloudId}-${row.journalId}-${row.when}'),
                icon: const Icon(AppIcons.history),
                title: s.t(
                  historyRowTitleKey(row, widget.ctx.bootstrap.operations),
                ),
                detail: [
                  '${formatScreenDate(context, row.when)} · '
                      '${s.t('history.moves', {'count': row.moves})}',
                  if (row.userEmail.isNotEmpty) row.userEmail,
                ].join(' · '),
                trailing: [_trailingFor(row, s, t)],
              )
          else
            EmptyState(
              icon: const Icon(AppIcons.history),
              title: s.t('history.empty'),
              message: s.t('history.empty.help'),
            ),
          if (_error.isNotEmpty) ...[
            const SizedBox(height: 12),
            Notice(kind: NoticeKind.error, child: Text(_error)),
          ],
        ],
      ),
    );
  }

  /// The right-hand control of a row: undone, not undoable, or undo.
  Widget _trailingFor(HistoryRow row, Strings s, DocFlowTokens t) {
    if (row.undone) {
      return StatusLabel(
        icon: const Icon(AppIcons.check),
        label: s.t('history.undone'),
      );
    }
    if (!row.undoAvailable) {
      return StatusLabel(label: s.t('history.localJournalRequired'));
    }
    return DfButton(
      variant: DfButtonVariant.secondary,
      icon: const Icon(AppIcons.undo),
      onPressed: _busy ? null : () => setState(() => _undoRow = row),
      child: Text(s.t('action.undo')),
    );
  }
}

/// The undo dialog body: the move count and the two ways to undo.
class _UndoChoice extends StatelessWidget {
  const _UndoChoice({
    required this.s,
    required this.moves,
    required this.onOriginal,
    required this.onFolder,
  });

  final Strings s;
  final int moves;
  final VoidCallback onOriginal;
  final VoidCallback onFolder;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        Text(
          s.t('history.moves', {'count': moves}),
          style: TextStyle(fontSize: remPx(context, 0.875), color: t.muted),
        ),
        const SizedBox(height: 14),
        DfButton(
          icon: const Icon(AppIcons.undo),
          onPressed: onOriginal,
          child: Text(s.t('history.undoOriginal')),
        ),
        const SizedBox(height: 8),
        DfButton(
          variant: DfButtonVariant.secondary,
          icon: const Icon(AppIcons.folderOpen),
          onPressed: onFolder,
          child: Text(s.t('history.undoToFolder')),
        ),
      ],
    );
  }
}
