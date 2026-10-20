// ReviewScreen, ported from frontend/src/App.tsx. It scans a destination
// folder for RFI files, lets the user keep, put or delete each file, previews
// the resulting Plan, applies it and offers undo.
//
// The Plan preview is PlanView, which the operation screens own. It arrives
// through [PlanPreviewBuilder] so PlanView is never copied here.

import 'package:flutter/material.dart' hide IconButton, Switch;

import '../../app/human_error.dart';
import '../../app/screen_registry.dart';
import '../../app/routes.dart';
import '../../bridge/bridge.dart';
import '../../bridge/platform.dart';
import '../../l10n/strings.dart';
import '../../models/models.dart';
import '../../theme/app_icons.dart';
import '../../theme/tokens.dart';
import '../widgets/widgets.dart';
import '../widgets/metrics.dart';
import 'result_view.dart';
import 'screen_common.dart';

/// Builds the Plan preview with its Cancel and Apply actions (PlanView).
typedef PlanPreviewBuilder = Widget Function({
  required Plan plan,
  required Strings s,
  required String error,
  required VoidCallback onCancel,
  required VoidCallback onApply,
});

/// Registers the review screen for [ScreenId.review]. [planPreview] supplies
/// the PlanView port. Without it, reaching the preview step fails loudly
/// rather than showing an empty page.
void registerReviewScreen(
  ScreenRegistry registry, {
  PlanPreviewBuilder? planPreview,
}) {
  registry.register(
    ScreenId.review,
    (ctx) => ReviewScreen(ctx: ctx, planPreview: planPreview),
  );
}

/// The `rfi.review` review screen (`ReviewScreen`).
class ReviewScreen extends StatefulWidget {
  const ReviewScreen({super.key, required this.ctx, this.planPreview});

  final ScreenContext ctx;
  final PlanPreviewBuilder? planPreview;

  @override
  State<ReviewScreen> createState() => _ReviewScreenState();
}

class _ReviewScreenState extends State<ReviewScreen> {
  List<ReviewItem> _items = const [];
  Map<String, ReviewAction> _actions = {};
  final Map<String, TextEditingController> _codes = {};
  Plan? _plan;
  OperationResult? _result;
  bool _busy = true;
  String _error = '';

  Bridge get _bridge => widget.ctx.bridge;
  Strings get _s => widget.ctx.strings;

  PathSelection get _destination {
    final route = widget.ctx.route;
    if (route is ReviewRoute) return route.destination;
    throw StateError('ReviewScreen needs a ReviewRoute, got $route');
  }

  @override
  void initState() {
    super.initState();
    _scan();
  }

  @override
  void dispose() {
    for (final controller in _codes.values) {
      controller.dispose();
    }
    super.dispose();
  }

  ReviewAction _actionOf(ReviewItem item) =>
      _actions[item.selectionId] ?? ReviewAction.keep;

  String _codeOf(ReviewItem item) => _codes[item.selectionId]?.text ?? '';

  /// Replaces the code controllers with one per scanned file, holding each
  /// file's code from the scan.
  void _resetCodes(List<ReviewItem> rows) {
    for (final controller in _codes.values) {
      controller.dispose();
    }
    _codes
      ..clear()
      ..addAll({
        for (final item in rows)
          item.selectionId: TextEditingController(text: item.code),
      });
  }

  Future<void> _scan() async {
    setState(() {
      _busy = true;
      _error = '';
    });
    try {
      final rows = await _bridge.call(
        FrontendCommands.engineReviewScan,
        params: (destination: _destination),
      );
      if (!mounted) return;
      setState(() {
        _items = rows;
        _actions = {
          for (final item in rows) item.selectionId: ReviewAction.keep,
        };
        _resetCodes(rows);
      });
    } catch (reason) {
      if (mounted) setState(() => _error = humanError(reason, _s));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// The decisions to preview: every file whose action is not keep.
  List<ReviewDecision> get _decisions => [
    for (final item in _items)
      if (_actionOf(item) != ReviewAction.keep)
        ReviewDecision(
          itemId: item.selectionId,
          action: ReviewDecisionAction.fromWire(_actionOf(item).name),
          code: _codeOf(item),
        ),
  ];

  Future<void> _preview() async {
    setState(() {
      _busy = true;
      _error = '';
    });
    try {
      final plan = await _bridge.call(
        FrontendCommands.engineReviewPreview,
        params: (destination: _destination, decisions: _decisions),
      );
      if (mounted) setState(() => _plan = plan);
    } catch (reason) {
      if (mounted) setState(() => _error = humanError(reason, _s));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _apply() async {
    final plan = _plan;
    if (plan == null) return;
    setState(() => _busy = true);
    try {
      final result = await _bridge.call(
        FrontendCommands.engineApply,
        params: (opId: 'rfi.review', planId: plan.planId),
      );
      if (mounted) setState(() => _result = result);
    } catch (reason) {
      if (mounted) setState(() => _error = humanError(reason, _s));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _undo() async {
    final journalId = _result?.journalId ?? '';
    if (journalId.isEmpty) return;
    try {
      await _bridge.call(
        FrontendCommands.engineUndo,
        params: (journalId: journalId, cloudId: null, destination: null),
      );
      if (!mounted) return;
      setState(() {
        _result = null;
        _plan = null;
      });
      await _scan();
    } catch (reason) {
      if (mounted) setState(() => _error = humanError(reason, _s));
    }
  }

  Future<void> _reveal(String selectionId) async {
    try {
      await revealPath(_bridge, selectionId);
    } catch (reason) {
      if (mounted) setState(() => _error = humanError(reason, _s));
    }
  }

  void _setAll(ReviewAction Function(ReviewItem item) actionFor) {
    setState(() {
      _actions = {for (final item in _items) item.selectionId: actionFor(item)};
    });
  }

  @override
  Widget build(BuildContext context) {
    final s = _s;
    final result = _result;
    final plan = _plan;
    final String? error = _error.isEmpty ? null : _error;

    final Widget body;
    if (result != null) {
      body = _ScrollPane(
        child: ResultView(
          result: result,
          s: s,
          error: error,
          onDone: widget.ctx.goHome,
          onUndo: () => _undo(),
          onOpenDestination: () => _reveal(_destination.selectionId),
        ),
      );
    } else if (plan != null) {
      final preview = widget.planPreview;
      if (preview == null) {
        throw StateError(
          'ReviewScreen has no PlanView: pass planPreview to registerReviewScreen',
        );
      }
      body = preview(
        plan: plan,
        s: s,
        error: _error,
        onCancel: () => setState(() => _plan = null),
        onApply: () => _apply(),
      );
    } else if (_busy) {
      body = BusyState(label: s.t('action.review'), s: s);
    } else {
      body = _buildList(s, error);
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Header(
          title: s.t('review.title'),
          onBack: widget.ctx.goHome,
          backLabel: s.t('action.back'),
        ),
        Expanded(child: body),
      ],
    );
  }

  Widget _buildList(Strings s, String? error) {
    final decisions = _decisions;
    final items = _items;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Expanded(
          child: _ScrollPane(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                PageHeading(
                  title: s.t('review.title'),
                  subtitle: s.t('review.help'),
                  trailing: DfButton(
                    variant: DfButtonVariant.secondary,
                    icon: const Icon(AppIcons.refreshCw),
                    onPressed: () => _scan(),
                    child: Text(s.t('action.retry')),
                  ),
                ),
                if (items.isNotEmpty) ...[
                  _BatchBar(
                    s: s,
                    onKeepAll: () => _setAll((_) => ReviewAction.keep),
                    onPutAll: () => _setAll(
                      (item) => item.code.trim().isNotEmpty
                          ? ReviewAction.put
                          : ReviewAction.keep,
                    ),
                    onDeleteAll: () => _setAll((_) => ReviewAction.delete),
                  ),
                  for (final item in items) _buildRow(s, item),
                ] else
                  EmptyState(
                    icon: const Icon(AppIcons.fileCheck),
                    title: s.t('review.empty'),
                    message: s.t('review.empty.help'),
                  ),
                if (error != null) ...[
                  const SizedBox(height: 12),
                  Notice(kind: NoticeKind.error, child: Text(error)),
                ],
              ],
            ),
          ),
        ),
        ActionBar(
          left: [
            Text(
              decisions.isNotEmpty
                  ? s.t('review.selected', {'count': decisions.length})
                  : s.t('review.noChanges'),
              style: TextStyle(
                color: DocFlowTokens.of(context).muted,
                fontSize: remPx(context, 0.875),
              ),
            ),
          ],
          right: [
            DfButton(
              icon: const Icon(AppIcons.eye),
              onPressed: decisions.isEmpty || _busy ? null : () => _preview(),
              child: Text(s.t('action.preview')),
            ),
          ],
        ),
      ],
    );
  }

  Widget _buildRow(Strings s, ReviewItem item) {
    final t = DocFlowTokens.of(context);
    final action = _actionOf(item);
    final controller = _codes[item.selectionId];
    final confidence = formatConfidencePercent(item.confidence) ?? '—';
    return Container(
      key: ValueKey(item.selectionId),
      margin: const EdgeInsets.only(bottom: 8),
      padding: const EdgeInsets.fromLTRB(14, 13, 14, 13),
      decoration: BoxDecoration(
        color: t.surface,
        border: Border.all(color: t.line),
        borderRadius: BorderRadius.circular(8),
      ),
      child: Wrap(
        crossAxisAlignment: WrapCrossAlignment.center,
        spacing: 12,
        runSpacing: 10,
        children: [
          ConstrainedBox(
            constraints: const BoxConstraints(minWidth: 180, maxWidth: 420),
            child: Tooltip(
              message: item.path,
              child: InkWell(
                borderRadius: BorderRadius.circular(6),
                onTap: () => _reveal(item.selectionId),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Icon(AppIcons.fileSearch, color: t.text),
                    const SizedBox(width: 10),
                    Flexible(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Text(
                            item.name,
                            style: TextStyle(
                              fontWeight: FontWeight.w700,
                              color: t.text,
                            ),
                          ),
                          const SizedBox(height: 3),
                          Text(
                            item.path,
                            style: TextStyle(
                              fontSize: remPx(context, 0.78),
                              color: t.muted,
                            ),
                          ),
                          const SizedBox(height: 3),
                          Text(
                            '${s.t('ocr.confidence')}: $confidence',
                            style: TextStyle(
                              fontSize: remPx(context, 0.78),
                              fontWeight: FontWeight.w700,
                              color: t.text,
                            ),
                          ),
                        ],
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
          ConstrainedBox(
            constraints: const BoxConstraints(minWidth: 150, maxWidth: 260),
            child: Semantics(
              label: '${item.name} code',
              textField: true,
              enabled: action == ReviewAction.put,
              child: TextField(
                controller: controller,
                enabled: action == ReviewAction.put,
                onChanged: (_) => setState(() {}),
                style: TextStyle(
                  fontSize: remPx(context, 0.875),
                  color: t.text,
                ),
                decoration: InputDecoration(
                  isDense: true,
                  hintText: s.t('review.noCode'),
                  border: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(7),
                    borderSide: BorderSide(color: t.line),
                  ),
                  enabledBorder: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(7),
                    borderSide: BorderSide(color: t.line),
                  ),
                  disabledBorder: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(7),
                    borderSide: BorderSide(color: t.line),
                  ),
                ),
              ),
            ),
          ),
          Segmented(
            value: action.name,
            small: true,
            options: [
              (ReviewAction.keep.name, s.t('review.keep')),
              (ReviewAction.put.name, s.t('review.put')),
              (ReviewAction.delete.name, s.t('review.delete')),
            ],
            onChanged: (value) => setState(() {
              _actions[item.selectionId] = ReviewAction.fromWire(value);
            }),
          ),
          // The warning follows the file's own action. The React source read
          // the action by path, which never matched, so it never showed.
          if (item.targetExists == true && action == ReviewAction.put)
            Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(AppIcons.circleAlert, size: 14, color: t.warning),
                const SizedBox(width: 5),
                Text(
                  s.t('review.willReplace'),
                  style: TextStyle(
                    fontSize: remPx(context, 0.7),
                    color: t.warning,
                  ),
                ),
              ],
            ),
        ],
      ),
    );
  }
}

/// The `.review-batch-bar`: the help line and the three select-all actions.
class _BatchBar extends StatelessWidget {
  const _BatchBar({
    required this.s,
    required this.onKeepAll,
    required this.onPutAll,
    required this.onDeleteAll,
  });

  final Strings s;
  final VoidCallback onKeepAll;
  final VoidCallback onPutAll;
  final VoidCallback onDeleteAll;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return Container(
      margin: const EdgeInsets.only(bottom: 14),
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
      decoration: BoxDecoration(
        color: t.surface,
        border: Border.all(color: t.line),
        borderRadius: BorderRadius.circular(t.radius),
      ),
      child: Wrap(
        alignment: WrapAlignment.spaceBetween,
        crossAxisAlignment: WrapCrossAlignment.center,
        spacing: 12,
        runSpacing: 10,
        children: [
          Text(
            s.t('review.help'),
            style: TextStyle(fontSize: remPx(context, 0.85), color: t.muted),
          ),
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              DfButton(
                variant: DfButtonVariant.secondary,
                icon: const Icon(AppIcons.rotateCcw),
                onPressed: onKeepAll,
                child: Text(s.t('review.selectAllKeep')),
              ),
              DfButton(
                variant: DfButtonVariant.secondary,
                icon: const Icon(AppIcons.arrowRight),
                onPressed: onPutAll,
                child: Text(s.t('review.selectAllPut')),
              ),
              DfButton(
                variant: DfButtonVariant.secondary,
                icon: const Icon(AppIcons.trash),
                onPressed: onDeleteAll,
                child: Text(s.t('review.selectAllTrash')),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

/// A scrollable body with the screen's padding, used by the review states.
class _ScrollPane extends StatelessWidget {
  const _ScrollPane({required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    return SingleChildScrollView(
      padding: const EdgeInsets.all(16),
      child: child,
    );
  }
}
