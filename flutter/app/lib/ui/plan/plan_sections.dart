// Read-only blocks of the Plan preview, ported from frontend/src/App.tsx:
// PlanImpactSummary, ScanIndexSummary, PlanExactEffects,
// WorkbookChangePreview, StepRow and the Outlook reply draft preview.

import 'package:flutter/material.dart' hide IconButton, Step, Switch;

import '../../l10n/strings.dart';
import '../../models/models.dart';
import '../../theme/app_icons.dart';
import '../../theme/tokens.dart';
import '../screens/screen_common.dart' show formatConfidencePercent;
import '../widgets/metrics.dart';
import '../widgets/widgets.dart';
import 'plan_logic.dart';
import 'plan_parts.dart';

/// `PlanImpactSummary`: what applying the Plan will do, by kind.
class PlanImpactSummary extends StatelessWidget {
  const PlanImpactSummary({super.key, required this.plan, required this.s});

  final Plan plan;
  final Strings s;

  @override
  Widget build(BuildContext context) {
    final impact = summarizePlanImpact(plan);
    final entries = [
      ('plan.impact.outputs', impact.outputs),
      ('plan.impact.updates', impact.updates),
      ('plan.impact.moves', impact.moves),
      ('plan.impact.printJobs', impact.printJobs),
      ('plan.impact.drafts', impact.drafts),
      ('plan.impact.cleanup', impact.cleanup),
      ('plan.impact.warnings', impact.warnings),
    ].where((entry) => entry.$2 > 0).toList();
    return PlanCard(
      semanticsLabel: s.t('plan.impact.title'),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          PlanHeading(
            icon: AppIcons.shield,
            title: s.t('plan.impact.title'),
            subtitle: s.t('plan.impact.previewOnly'),
          ),
          if (entries.isNotEmpty) ...[
            const SizedBox(height: 12),
            PlanCountList(
              entries: [for (final (key, count) in entries) (s.t(key), count)],
            ),
          ],
        ],
      ),
    );
  }
}

/// `ScanIndexSummary`: the scan index reuse counts of `plan.meta.scanIndex`.
class ScanIndexSummary extends StatelessWidget {
  const ScanIndexSummary({super.key, required this.plan, required this.s});

  final Plan plan;
  final Strings s;

  @override
  Widget build(BuildContext context) {
    final raw = plan.meta['scanIndex'];
    if (raw is List) return const SizedBox.shrink();
    final data = recordValue(raw);
    if (data == null) return const SizedBox.shrink();
    int count(String key) {
      final value = data[key];
      if (value is! num || !value.isFinite) return 0;
      final truncated = value.truncate();
      return truncated < 0 ? 0 : truncated;
    }

    final entries = [
      ('scanIndex.discovered', count('discovered')),
      ('scanIndex.reused', count('reused')),
      ('scanIndex.analyzed', count('analyzed')),
      ('scanIndex.failed', count('failed')),
      ('scanIndex.interrupted', count('priorInterruptedRuns')),
    ].where((entry) => entry.$1 != 'scanIndex.interrupted' || entry.$2 > 0);
    return PlanCard(
      semanticsLabel: s.t('scanIndex.title'),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          PlanHeading(
            icon: AppIcons.database,
            title: s.t('scanIndex.title'),
            subtitle: data['forceRescan'] == true
                ? s.t('scanIndex.forceApplied')
                : s.t('scanIndex.help'),
          ),
          const SizedBox(height: 12),
          PlanCountList(
            entries: [for (final (key, value) in entries) (s.t(key), value)],
          ),
        ],
      ),
    );
  }
}

/// `PlanExactEffects`: every non-mkdir step with full paths, collapsed.
class PlanExactEffects extends StatelessWidget {
  const PlanExactEffects({super.key, required this.plan, required this.s});

  final Plan plan;
  final Strings s;

  @override
  Widget build(BuildContext context) {
    final effects = [
      for (final step in plan.steps)
        if (step.kind != StepKind.mkdir) step,
    ];
    if (effects.isEmpty) return const SizedBox.shrink();
    final t = DocFlowTokens.of(context);
    return PlanDisclosure(
      summary: Wrap(
        spacing: 8,
        runSpacing: 2,
        crossAxisAlignment: WrapCrossAlignment.center,
        children: [
          ExcludeSemantics(
            child: Icon(AppIcons.eye, size: remPx(context, 1), color: t.text),
          ),
          Text(
            s.t('plan.exact.title', {'count': effects.length}),
            style: planStrong(context, rem: 0.85),
          ),
          Text(s.t('plan.exact.help'), style: planMuted(context)),
        ],
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (final step in effects) StepRow(step: step, s: s, fullPath: true),
        ],
      ),
    );
  }
}

const _workbookColumns = [
  'plan.workbookChanges.workbook',
  'plan.workbookChanges.sheetCell',
  'plan.workbookChanges.rfi',
  'plan.workbookChanges.before',
  'plan.workbookChanges.after',
];

/// `WorkbookChangePreview`: the planned workbook cell edits in a resizable
/// table, collapsed.
class WorkbookChangePreview extends StatefulWidget {
  const WorkbookChangePreview({super.key, required this.plan, required this.s});

  final Plan plan;
  final Strings s;

  @override
  State<WorkbookChangePreview> createState() => _WorkbookChangePreviewState();
}

class _WorkbookChangePreviewState extends State<WorkbookChangePreview> {
  final List<double> _widths = [170, 150, 220, 260, 260];

  @override
  Widget build(BuildContext context) {
    final s = widget.s;
    final t = DocFlowTokens.of(context);
    final changes = summarizeWorkbookCellChanges(widget.plan);
    if (changes.isEmpty) return const SizedBox.shrink();
    final labels = [for (final key in _workbookColumns) s.t(key)];
    final tableWidth = _widths.fold<double>(0, (total, w) => total + w);
    Widget cell(int index, Widget child) => Container(
      width: _widths[index],
      padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 7),
      child: child,
    );
    final header = Container(
      decoration: BoxDecoration(
        color: t.surface2,
        border: Border(bottom: BorderSide(color: t.line)),
      ),
      child: Row(
        children: [
          for (var i = 0; i < labels.length; i++)
            SizedBox(
              width: _widths[i],
              height: 34,
              child: Stack(
                children: [
                  Positioned.fill(
                    child: Tooltip(
                      message: labels[i],
                      child: Padding(
                        padding: const EdgeInsets.fromLTRB(9, 8, 14, 8),
                        child: Semantics(
                          header: true,
                          child: Text(
                            labels[i],
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: planStrong(context, rem: 0.74),
                          ),
                        ),
                      ),
                    ),
                  ),
                  Positioned(
                    top: 0,
                    bottom: 0,
                    right: 0,
                    child: ColumnResizeHandle(
                      width: _widths[i],
                      label: labels[i],
                      s: s,
                      onWidthPreview: (w) => setState(() => _widths[i] = w),
                      onWidthChange: (w) => setState(() => _widths[i] = w),
                    ),
                  ),
                ],
              ),
            ),
        ],
      ),
    );
    final blank = s.t('plan.workbookChanges.blank');
    return PlanDisclosure(
      summary: Wrap(
        spacing: 8,
        runSpacing: 2,
        crossAxisAlignment: WrapCrossAlignment.center,
        children: [
          ExcludeSemantics(
            child: Icon(
              AppIcons.tableProperties,
              size: remPx(context, 1),
              color: t.text,
            ),
          ),
          Text(
            s.t('plan.workbookChanges.title'),
            style: planStrong(context, rem: 0.85),
          ),
          Text(
            s.t('plan.workbookChanges.count', {'count': changes.length}),
            style: planMuted(context),
          ),
        ],
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(s.t('plan.workbookChanges.help'), style: planMuted(context)),
          const SizedBox(height: 10),
          AdjustableDataRegion(
            s: s,
            label: s.t('plan.workbookChanges.title'),
            child: Scrollbar(
              child: SingleChildScrollView(
                scrollDirection: Axis.horizontal,
                child: SizedBox(
                  width: tableWidth,
                  child: Column(
                    children: [
                      header,
                      Expanded(
                        child: ListView.builder(
                          itemCount: changes.length,
                          itemBuilder: (context, index) {
                            final change = changes[index];
                            return Container(
                              decoration: BoxDecoration(
                                border: Border(
                                  bottom: BorderSide(color: t.line),
                                ),
                              ),
                              child: Row(
                                crossAxisAlignment: CrossAxisAlignment.start,
                                children: [
                                  cell(
                                    0,
                                    Text(
                                      s.t(
                                        'plan.workbookChanges.kind.${change.workbook}',
                                      ),
                                      style: planBody(context),
                                    ),
                                  ),
                                  cell(
                                    1,
                                    Text(
                                      '${change.sheet}!${change.cell}',
                                      style: planMono(context),
                                    ),
                                  ),
                                  cell(
                                    2,
                                    Text(
                                      change.rfiCode.isEmpty
                                          ? '—'
                                          : change.rfiCode,
                                      style: planBody(context),
                                    ),
                                  ),
                                  cell(
                                    3,
                                    Text(
                                      change.before.isEmpty
                                          ? blank
                                          : change.before,
                                      style: planBody(context),
                                    ),
                                  ),
                                  cell(
                                    4,
                                    Text(
                                      change.after.isEmpty
                                          ? blank
                                          : change.after,
                                      style: planBody(context),
                                    ),
                                  ),
                                ],
                              ),
                            );
                          },
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ),
          ),
          if (changes.length == maxWorkbookCellPreviews) ...[
            const SizedBox(height: 8),
            Text(
              s.t('plan.workbookChanges.limit', {
                'count': maxWorkbookCellPreviews,
              }),
              style: planMuted(context),
            ),
          ],
        ],
      ),
    );
  }
}

/// The Outlook reply draft a Plan will create (`.outlook-reply-preview`).
class OutlookReplyPreviewSection extends StatelessWidget {
  const OutlookReplyPreviewSection({
    super.key,
    required this.preview,
    required this.s,
  });

  final OutlookReplyPreview preview;
  final Strings s;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    Widget part(String labelKey, String text) => Padding(
      padding: const EdgeInsets.only(top: 10),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(s.t(labelKey), style: planStrong(context, rem: 0.8)),
          const SizedBox(height: 4),
          Container(
            padding: const EdgeInsets.all(10),
            decoration: BoxDecoration(
              color: t.surface2,
              borderRadius: BorderRadius.circular(7),
            ),
            child: SelectableText(text, style: planMono(context)),
          ),
        ],
      ),
    );
    return PlanCard(
      semanticsLabel: s.t('outlookDraft.previewTitle'),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          Semantics(
            header: true,
            child: Text(
              s.t('outlookDraft.previewTitle'),
              style: planStrong(context, rem: 0.95),
            ),
          ),
          part('outlookDraft.subject', preview.subject),
          part('outlookDraft.plainText', preview.plainText),
          part('outlookDraft.html', preview.html),
        ],
      ),
    );
  }
}

final RegExp _pathSeparator = RegExp(r'[\\/]');

IconData _stepIcon(StepKind kind) => switch (kind) {
  StepKind.print => AppIcons.printer,
  StepKind.workbookUpdate => AppIcons.tableProperties,
  StepKind.replicaPublish => AppIcons.database,
  StepKind.pdfFirstPageReplace => AppIcons.files,
  StepKind.pdfPageRotate => AppIcons.rotateCcw,
  StepKind.pdfPagesExtract => AppIcons.files,
  StepKind.cleanupTemp => AppIcons.trash,
  StepKind.copy => AppIcons.copy,
  StepKind.mkdir => AppIcons.folderOpen,
  _ => AppIcons.arrowRight,
};

/// `StepRow`: one planned step with its source, destination, confidence and
/// page details. [fullPath] shows whole paths instead of file names.
class StepRow extends StatelessWidget {
  const StepRow({
    super.key,
    required this.step,
    required this.s,
    this.fullPath = false,
  });

  final Step step;
  final Strings s;
  final bool fullPath;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final src = step.src;
    final dst = step.dst;
    final source = fullPath ? src : src?.split(_pathSeparator).last;
    final destination = fullPath ? dst : dst?.split(_pathSeparator).last;
    final confidence = formatConfidencePercent(step.meta['confidence']);
    final rotations = step.meta['rotations'];
    final rotationSummary =
        (step.kind == StepKind.pdfPageRotate ||
                step.kind == StepKind.pdfPagesExtract) &&
            rotations is List
        ? rotations
              .map((entry) {
                final record = recordValue(entry);
                if (record == null) return '';
                final page = jsInteger(record['pageIndex']);
                final target = jsInteger(record['targetRotation']);
                final pageConfidence = formatConfidencePercent(
                  record['confidence'],
                  percentScale: true,
                );
                if (page == null || target == null) return '';
                final suffix = pageConfidence == null
                    ? ''
                    : ' · ${s.t('ocr.confidence.short')} $pageConfidence';
                return '${page + 1} → $target°$suffix';
              })
              .where((text) => text.isNotEmpty)
              .join(', ')
        : '';
    final pageIndexes = step.meta['pageIndexes'];
    final extractionPages =
        step.kind == StepKind.pdfPagesExtract && pageIndexes is List
        ? pageIndexes
              .map(jsInteger)
              .whereType<int>()
              .map((page) => page + 1)
              .join(', ')
        : '';
    final noteKey = step.noteKey;
    return Container(
      padding: const EdgeInsets.symmetric(vertical: 9),
      decoration: BoxDecoration(
        border: Border(bottom: BorderSide(color: t.line)),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Container(
            width: 30,
            height: 30,
            alignment: Alignment.center,
            decoration: BoxDecoration(
              color: t.surface3,
              borderRadius: BorderRadius.circular(7),
            ),
            child: ExcludeSemantics(
              child: Icon(
                _stepIcon(step.kind),
                size: remPx(context, 0.95),
                color: t.text,
              ),
            ),
          ),
          const SizedBox(width: 10),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                Wrap(
                  spacing: 8,
                  runSpacing: 4,
                  crossAxisAlignment: WrapCrossAlignment.center,
                  children: [
                    Text(
                      noteKey != null && noteKey.isNotEmpty
                          ? s.t(noteKey)
                          : step.kind.wire,
                      style: planStrong(context, rem: 0.85),
                    ),
                    if (confidence != null)
                      Tooltip(
                        message: s.t('ocr.confidence.help'),
                        child: Container(
                          padding: const EdgeInsets.symmetric(
                            horizontal: 7,
                            vertical: 2,
                          ),
                          decoration: BoxDecoration(
                            border: Border.all(color: t.line),
                            borderRadius: BorderRadius.circular(999),
                          ),
                          child: Text(
                            '${s.t('ocr.confidence.short')} $confidence',
                            style: planMuted(context, rem: 0.7),
                          ),
                        ),
                      ),
                  ],
                ),
                const SizedBox(height: 2),
                planTooltip(
                  src,
                  Text(
                    source == null || source.isEmpty ? '—' : source,
                    style: planMuted(context),
                  ),
                ),
                if (dst != null && dst.isNotEmpty)
                  Tooltip(
                    message: dst,
                    child: Text(
                      '→ $destination',
                      style: planBody(context, rem: 0.78),
                    ),
                  ),
                if (rotationSummary.isNotEmpty)
                  Text(
                    '${s.t('rfi.orientation.pages')}: $rotationSummary',
                    style: planMuted(context),
                  ),
                if (extractionPages.isNotEmpty)
                  Text(
                    '${s.t('rfi.extraction.pages')}: $extractionPages',
                    style: planMuted(context),
                  ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}
