// PlanView, ported from frontend/src/App.tsx: the approved preview of an
// operation with its impact summary, warnings, optional record editor, steps
// and the Cancel / Apply bar. Operation screens show it through
// [OperationStageViews]; the review screen shows it through [planPreview].

import 'package:flutter/material.dart' hide IconButton, Switch;

import '../../l10n/strings.dart';
import '../../models/models.dart';
import '../../theme/app_icons.dart';
import '../../theme/tokens.dart';
import '../screens/operation_screen.dart' show PlanStageProps;
import '../widgets/metrics.dart';
import '../widgets/widgets.dart';
import 'editable_plan_table.dart';
import 'plan_logic.dart';
import 'plan_parts.dart';
import 'plan_sections.dart';
import 'plan_warnings.dart';

/// The Plan preview. Without [editorRows] and [onEditorRowsChanged] the
/// Plan's editor is not offered and the steps are listed read-only.
class PlanView extends StatefulWidget {
  const PlanView({
    super.key,
    required this.plan,
    required this.s,
    required this.error,
    required this.onCancel,
    required this.onApply,
    this.editorRows,
    this.onEditorRowsChanged,
    this.editorDirty = false,
    this.onUpdatePreview,
    this.isUpdatingPreview = false,
    this.scanPreviewUpdated = false,
  });

  /// The operation screen's preview stage.
  PlanView.fromProps(PlanStageProps props, {Key? key})
    : this(
        key: key,
        plan: props.plan,
        s: props.s,
        error: props.error,
        onCancel: props.onCancel,
        onApply: props.onApply,
        editorRows: props.editorRows,
        onEditorRowsChanged: props.onEditorRowsChanged,
        editorDirty: props.editorDirty,
        onUpdatePreview: props.onUpdatePreview,
        isUpdatingPreview: props.isUpdatingPreview,
        scanPreviewUpdated: props.scanPreviewUpdated,
      );

  /// A preview without the record editor (the review screen's use).
  const PlanView.preview({
    super.key,
    required this.plan,
    required this.s,
    required this.error,
    required this.onCancel,
    required this.onApply,
  }) : editorRows = null,
       onEditorRowsChanged = null,
       editorDirty = false,
       onUpdatePreview = null,
       isUpdatingPreview = false,
       scanPreviewUpdated = false;

  final Plan plan;
  final Strings s;
  final String error;
  final VoidCallback onCancel;
  final VoidCallback onApply;
  final List<Map<String, Object?>>? editorRows;
  final ValueChanged<List<Map<String, Object?>>>? onEditorRowsChanged;
  final bool editorDirty;
  final VoidCallback? onUpdatePreview;
  final bool isUpdatingPreview;
  final bool scanPreviewUpdated;

  @override
  State<PlanView> createState() => _PlanViewState();
}

/// [PlanPreviewBuilder] for the review screen: PlanView without the editor.
Widget planPreview({
  required Plan plan,
  required Strings s,
  required String error,
  required VoidCallback onCancel,
  required VoidCallback onApply,
}) => PlanView.preview(
  plan: plan,
  s: s,
  error: error,
  onCancel: onCancel,
  onApply: onApply,
);

class _PlanViewState extends State<PlanView> {
  String _warningFocusCode = '';

  /// Section anchors of the signed staging preview (the React element ids).
  final Map<String, GlobalKey> _sections = {
    for (final id in const [
      'signed-approved-records',
      'signed-page-comparison',
      'signed-warning-review',
      'signed-warning-list',
    ])
      id: GlobalKey(debugLabel: id),
  };

  void _scrollToSection(String id) {
    final context = _sections[id]?.currentContext;
    if (context != null) Scrollable.ensureVisible(context);
  }

  Widget _anchor(String id, Widget child) =>
      KeyedSubtree(key: _sections[id], child: child);

  @override
  Widget build(BuildContext context) {
    final plan = widget.plan;
    final s = widget.s;
    final t = DocFlowTokens.of(context);
    final editorRows = widget.editorRows;
    final onRowsChanged = widget.onEditorRowsChanged;
    final moves = plan.steps
        .where((step) => step.kind != StepKind.mkdir)
        .length;
    final editor = plan.meta['editor'] is List
        ? null
        : recordValue(plan.meta['editor']);
    final editorReady =
        editor == null ||
        editorRows == null ||
        isPlanEditorReady(editor, editorRows);
    final requiredFields = editor?['requiredFields'];
    final editorCanMaterializeStep =
        (editorRows?.any((row) => row['enabled'] != false) ?? false) &&
        ((requiredFields is List && requiredFields.isNotEmpty) ||
            (plan.opId == 'files.batch_print' &&
                editor?['kind'] == 'recordTable'));
    final canApply =
        editorReady && (plan.steps.isNotEmpty || editorCanMaterializeStep);
    final isScanEditor = editor?['kind'] == 'rfiScan';
    final shouldUpdatePreview = isScanEditor && widget.editorDirty;
    final replyPreview = outlookReplyPreview(plan.meta['replyPreview']);
    final signedStage = plan.opId == 'rfi.stage_signed_first_pages';
    final focusableWarningCodes = signedStage
        ? [
            for (final row in editorRows ?? const <Map<String, Object?>>[])
              if (row['code'] is String && (row['code']! as String).isNotEmpty)
                row['code']! as String,
          ]
        : const <String>[];

    final children = <Widget>[
      PlanImpactSummary(plan: plan, s: s),
      if (widget.scanPreviewUpdated && !widget.editorDirty)
        Semantics(
          liveRegion: true,
          child: Notice(
            kind: NoticeKind.info,
            child: Text(s.t('rfi.fastScan.editor.previewUpdated')),
          ),
        ),
      ScanIndexSummary(plan: plan, s: s),
    ];

    if (editor != null && editorRows != null && onRowsChanged != null) {
      final table = EditablePlanTable(
        editor: editor,
        rows: editorRows,
        s: s,
        onChange: onRowsChanged,
      );
      children.addAll([
        if (signedStage)
          SignedStageNavigator(
            plan: plan,
            rows: editorRows,
            s: s,
            onJump: _scrollToSection,
          ),
        if (!signedStage && plan.warnings.isNotEmpty)
          PlanWarnings(warnings: plan.warnings, focusableCodes: const [], s: s),
        WorkbookChangePreview(plan: plan, s: s),
        if (replyPreview != null)
          OutlookReplyPreviewSection(preview: replyPreview, s: s),
        if (signedStage) ...[
          _anchor('signed-approved-records', table),
          _anchor(
            'signed-page-comparison',
            SignedPageComparison(
              plan: plan,
              editorRows: editorRows,
              focusCode: _warningFocusCode,
              s: s,
            ),
          ),
          _anchor(
            'signed-warning-review',
            SignedWarningReviewSummary(plan: plan, s: s),
          ),
          if (plan.warnings.isNotEmpty)
            _anchor(
              'signed-warning-list',
              PlanWarnings(
                warnings: plan.warnings,
                focusableCodes: focusableWarningCodes,
                onFocusCode: (code) => setState(() => _warningFocusCode = code),
                s: s,
              ),
            ),
        ] else
          table,
        if (plan.steps.isNotEmpty) PlanExactEffects(plan: plan, s: s),
      ]);
    } else {
      children.addAll([
        if (plan.warnings.isNotEmpty)
          PlanWarnings(warnings: plan.warnings, focusableCodes: const [], s: s),
        WorkbookChangePreview(plan: plan, s: s),
        if (replyPreview != null)
          OutlookReplyPreviewSection(preview: replyPreview, s: s),
        PlanCard(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            mainAxisSize: MainAxisSize.min,
            children: [
              Wrap(
                alignment: WrapAlignment.spaceBetween,
                crossAxisAlignment: WrapCrossAlignment.center,
                spacing: 12,
                runSpacing: 6,
                children: [
                  Semantics(
                    header: true,
                    child: Text(
                      s.t('plan.summary', {'count': moves}),
                      style: planStrong(context),
                    ),
                  ),
                  Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Icon(
                        AppIcons.eye,
                        size: remPx(context, 0.9),
                        color: t.muted,
                      ),
                      const SizedBox(width: 5),
                      Text(s.t('plan.readOnly'), style: planMuted(context)),
                    ],
                  ),
                ],
              ),
              const SizedBox(height: 6),
              if (plan.steps.isNotEmpty)
                for (final step in plan.steps) StepRow(step: step, s: s)
              else
                EmptyState(
                  icon: const Icon(AppIcons.fileCheck),
                  title: s.t('plan.empty'),
                ),
            ],
          ),
        ),
      ]);
    }
    if (widget.error.isNotEmpty) {
      children.add(Notice(kind: NoticeKind.error, child: Text(widget.error)));
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Expanded(
          child: SingleChildScrollView(
            padding: const EdgeInsets.only(bottom: 16),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                for (var i = 0; i < children.length; i++) ...[
                  if (i > 0) const SizedBox(height: 12),
                  children[i],
                ],
              ],
            ),
          ),
        ),
        ActionBar(
          left: [
            DfButton(
              variant: DfButtonVariant.ghost,
              onPressed: widget.isUpdatingPreview ? null : widget.onCancel,
              child: Text(s.t('action.cancel')),
            ),
          ],
          right: [
            DfButton(
              icon: Icon(
                shouldUpdatePreview ? AppIcons.refreshCw : AppIcons.play,
              ),
              onPressed: !canApply || widget.isUpdatingPreview
                  ? null
                  : (shouldUpdatePreview
                        ? (widget.onUpdatePreview ?? widget.onApply)
                        : widget.onApply),
              child: Text(
                widget.isUpdatingPreview
                    ? s.t('rfi.fastScan.editor.updatingPreview')
                    : shouldUpdatePreview
                    ? s.t('rfi.fastScan.editor.updatePreview')
                    : s.t('action.apply'),
              ),
            ),
          ],
        ),
      ],
    );
  }
}
