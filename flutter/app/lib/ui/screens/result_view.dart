// ResultView, ported from frontend/src/App.tsx. It shows the outcome of an
// applied Plan with its effect state, warnings, resume summary and actions.
// Operation screens and the review screen both show it.

import 'package:flutter/material.dart' hide IconButton, Switch;

import '../../l10n/strings.dart';
import '../../models/models.dart';
import '../../theme/app_icons.dart';
import '../../theme/tokens.dart';
import '../widgets/widgets.dart';
import '../widgets/metrics.dart';
import '../widgets/spinner.dart';
import 'screen_common.dart';

/// The effect state of a result (`effectState` in ResultView). The engine's
/// `meta.effectState` wins; otherwise it is derived from the counts.
String resultEffectState(OperationResult result) {
  final meta = result.meta;
  final declared = meta?['effectState'];
  if (declared is String) return declared;
  if (meta?['resumeAvailable'] == true) return 'partial_committed';
  if (result.failed > 0) {
    return result.applied > 0 ? 'partial_committed' : 'no_effect_retryable';
  }
  return 'committed';
}

/// Whether the result counts as a clean success (`success` in ResultView).
bool resultIsSuccess(OperationResult result) =>
    resultEffectState(result) == 'committed' &&
    result.meta?['resumeAvailable'] != true;

/// The outcome card. [onCopy] receives a sheet name when the copy buttons are
/// shown per sheet, and null for the single mail copy.
class ResultView extends StatelessWidget {
  const ResultView({
    super.key,
    required this.result,
    required this.s,
    this.error,
    required this.onDone,
    required this.onUndo,
    this.copySheets = const [],
    this.copyEnabled = false,
    this.copyingSheet,
    this.onCopy,
    this.onResumePrint,
    this.onPrepareRfiReply,
    this.onOpenDestination,
  });

  final OperationResult result;
  final Strings s;
  final String? error;
  final VoidCallback onDone;
  final VoidCallback onUndo;
  final List<String> copySheets;
  final bool copyEnabled;

  /// The sheet whose copy is running; disables every copy button while set.
  final String? copyingSheet;
  final void Function(String? sheet)? onCopy;
  final VoidCallback? onResumePrint;
  final VoidCallback? onPrepareRfiReply;
  final VoidCallback? onOpenDestination;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final effectState = resultEffectState(result);
    final success = resultIsSuccess(result);
    final meta = result.meta;
    final resumeAvailable = meta?['resumeAvailable'] == true;
    final cloudSynced = result.cloudSynced;

    final copyBusy = copyingSheet != null;
    final copyButtons = <Widget>[];
    if (onCopy != null) {
      if (copySheets.isNotEmpty) {
        for (final sheet in copySheets) {
          copyButtons.add(
            DfButton(
              variant: DfButtonVariant.secondary,
              icon: copyingSheet == sheet
                  ? Spinner(size: remPx(context, 1), color: t.text)
                  : const Icon(AppIcons.tableProperties),
              onPressed: copyBusy ? null : () => onCopy!(sheet),
              child: Text('$sheet · ${s.t('rfi.action.copySheet')}'),
            ),
          );
        }
      } else if (copyEnabled) {
        copyButtons.add(
          DfButton(
            variant: DfButtonVariant.secondary,
            icon: copyBusy
                ? Spinner(size: remPx(context, 1), color: t.text)
                : const Icon(AppIcons.tableProperties),
            onPressed: copyBusy ? null : () => onCopy!(null),
            child: Text(s.t('rfi.action.copyMail')),
          ),
        );
      }
    }

    final errors = result.errors;
    final journalMissing = result.journalId.isEmpty;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.center,
      mainAxisSize: MainAxisSize.min,
      children: [
        Container(
          width: 60,
          height: 60,
          margin: const EdgeInsets.only(bottom: 18),
          alignment: Alignment.center,
          decoration: BoxDecoration(
            color: t.surface3,
            borderRadius: BorderRadius.circular(16),
          ),
          child: Icon(
            success ? AppIcons.checkCircle : AppIcons.circleAlert,
            size: 28,
            color: success ? t.success : t.danger,
          ),
        ),
        Text(
          success ? 'DOCFLOW' : s.t('ui.warnings'),
          textAlign: TextAlign.center,
          style: TextStyle(
            fontSize: remPx(context, 0.68),
            fontWeight: FontWeight.w700,
            letterSpacing: 0.16 * remPx(context, 0.68),
            color: t.muted,
          ),
        ),
        const SizedBox(height: 7),
        Semantics(
          header: true,
          child: Text(
            s.t('result.applied', {'count': result.applied}),
            textAlign: TextAlign.center,
            style: TextStyle(
              fontSize: remPx(context, 1.75),
              fontWeight: FontWeight.w700,
              color: t.text,
            ),
          ),
        ),
        const SizedBox(height: 4),
        Text(
          s.t('result.failed', {'count': result.failed}),
          textAlign: TextAlign.center,
          style: TextStyle(fontSize: remPx(context, 1), color: t.muted),
        ),
        const SizedBox(height: 10),
        StatusLabel(label: s.t('effect.$effectState')),
        if (cloudSynced != null) ...[
          const SizedBox(height: 10),
          SafeBadge(
            icon: Icon(cloudSynced ? AppIcons.check : AppIcons.history),
            label: s.t(cloudSynced ? 'history.cloud' : 'history.local'),
          ),
        ],
        if (errors.isNotEmpty) ...[
          const SizedBox(height: 16),
          SizedBox(
            width: _maxWidth(context, 520),
            child: Notice(
              kind: NoticeKind.warning,
              child: Text(
                [for (final item in errors) '${item.subject} ${s.t(item.key)}']
                    .join('\n'),
                style: TextStyle(
                  fontSize: remPx(context, 0.875),
                  color: t.text,
                ),
              ),
            ),
          ),
        ],
        if (resumeAvailable) ...[
          const SizedBox(height: 14),
          SizedBox(
            width: _maxWidth(context, 480),
            child: Container(
              padding: const EdgeInsets.symmetric(horizontal: 13, vertical: 11),
              decoration: BoxDecoration(
                color: t.surface2,
                border: Border.all(color: t.line),
                borderRadius: BorderRadius.circular(9),
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                mainAxisSize: MainAxisSize.min,
                children: [
                  Text(
                    s.t('print.resume.incomplete'),
                    style: TextStyle(
                      fontSize: remPx(context, 0.875),
                      fontWeight: FontWeight.w700,
                      color: t.text,
                    ),
                  ),
                  const SizedBox(height: 3),
                  Text(
                    s.t('print.resume.summary', {
                      'count': screenInt(meta?['resumable']),
                    }),
                    style: TextStyle(
                      fontSize: remPx(context, 0.76),
                      color: t.muted,
                    ),
                  ),
                ],
              ),
            ),
          ),
        ],
        if (error != null && error!.isNotEmpty) ...[
          const SizedBox(height: 16),
          SizedBox(
            width: _maxWidth(context, 520),
            child: Notice(kind: NoticeKind.error, child: Text(error!)),
          ),
        ],
        if (copyButtons.isNotEmpty) ...[
          const SizedBox(height: 18),
          Container(
            width: _maxWidth(context, 720),
            padding: const EdgeInsets.all(10),
            decoration: BoxDecoration(
              color: t.surface2,
              borderRadius: BorderRadius.circular(10),
            ),
            child: Wrap(
              alignment: WrapAlignment.center,
              spacing: 8,
              runSpacing: 8,
              children: copyButtons,
            ),
          ),
        ],
        const SizedBox(height: 24),
        Wrap(
          alignment: WrapAlignment.center,
          spacing: 10,
          runSpacing: 10,
          children: [
            if (onPrepareRfiReply != null)
              DfButton(
                variant: DfButtonVariant.secondary,
                icon: const Icon(AppIcons.mail),
                onPressed: onPrepareRfiReply,
                child: Text(s.t('rfi.action.prepareReply')),
              ),
            if (onResumePrint != null)
              DfButton(
                variant: DfButtonVariant.secondary,
                icon: const Icon(AppIcons.rotateCcw),
                onPressed: onResumePrint,
                child: Text(s.t('print.resume.review')),
              ),
            if (onOpenDestination != null)
              DfButton(
                variant: DfButtonVariant.secondary,
                icon: const Icon(AppIcons.folderOpen),
                onPressed: onOpenDestination,
                child: Text(s.t('action.openDestination')),
              ),
            DfButton(
              variant: DfButtonVariant.secondary,
              icon: const Icon(AppIcons.undo),
              onPressed: journalMissing || meta?['nonUndoable'] == true
                  ? null
                  : onUndo,
              child: Text(s.t('action.undo')),
            ),
            DfButton(
              icon: const Icon(AppIcons.check),
              onPressed: onDone,
              child: Text(s.t('action.done')),
            ),
          ],
        ),
      ],
    );
  }
}

/// `min(100%, [px])`: the width the React CSS caps the card parts at.
double _maxWidth(BuildContext context, double px) {
  final available = MediaQuery.sizeOf(context).width - 32;
  return available < px ? available : px;
}
