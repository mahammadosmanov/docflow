// Warning review and signed first-page staging, ported from
// frontend/src/App.tsx: PlanWarnings, SignedStageNavigator,
// SignedWarningReviewSummary and SignedPageComparison.

import 'dart:convert';
import 'dart:ui' show SemanticsRole;

import 'package:flutter/material.dart' hide IconButton, Switch;

import '../../l10n/strings.dart';
import '../../models/models.dart';
import '../../theme/app_icons.dart';
import '../../theme/tokens.dart';
import '../widgets/metrics.dart';
import '../widgets/pressable.dart';
import '../widgets/widgets.dart';
import 'plan_logic.dart';
import 'plan_parts.dart';

/// `PlanWarnings`: the Plan's warnings, searchable and filterable by type once
/// there are five or more. A warning whose subject names a code in
/// [focusableCodes] offers a button that calls [onFocusCode].
class PlanWarnings extends StatefulWidget {
  const PlanWarnings({
    super.key,
    required this.warnings,
    required this.focusableCodes,
    required this.s,
    this.onFocusCode,
  });

  final List<Warning> warnings;
  final List<String> focusableCodes;
  final ValueChanged<String>? onFocusCode;
  final Strings s;

  @override
  State<PlanWarnings> createState() => _PlanWarningsState();
}

class _PlanWarningsState extends State<PlanWarnings> {
  String _query = '';
  String _type = 'all';

  @override
  Widget build(BuildContext context) {
    final s = widget.s;
    final t = DocFlowTokens.of(context);
    final warnings = widget.warnings;
    final types = {for (final warning in warnings) warning.key}.toList();
    final query = localeLowerCase(_query.trim(), s.locale);
    final visible = warnings.where((warning) {
      if (_type != 'all' && warning.key != _type) return false;
      if (query.isEmpty) return true;
      return localeLowerCase(
        '${warning.subject} ${s.t(warning.key)}',
        s.locale,
      ).contains(query);
    }).toList();
    final focusable = widget.focusableCodes.toSet();
    final filtering = warnings.length >= 5;
    return PlanCard(
      semanticsLabel: s.t('ui.warnings'),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          Wrap(
            alignment: WrapAlignment.spaceBetween,
            crossAxisAlignment: WrapCrossAlignment.center,
            spacing: 12,
            runSpacing: 8,
            children: [
              Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  ExcludeSemantics(
                    child: Icon(
                      AppIcons.circleAlert,
                      size: remPx(context, 1),
                      color: t.warning,
                    ),
                  ),
                  const SizedBox(width: 7),
                  Semantics(
                    header: true,
                    child: Text(s.t('ui.warnings'), style: planStrong(context)),
                  ),
                  const SizedBox(width: 7),
                  Container(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 7,
                      vertical: 1,
                    ),
                    decoration: BoxDecoration(
                      color: t.surface3,
                      borderRadius: BorderRadius.circular(999),
                    ),
                    child: Text(
                      '${warnings.length}',
                      style: planStrong(context, rem: 0.72),
                    ),
                  ),
                ],
              ),
              if (filtering)
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    SizedBox(
                      width: 220,
                      child: PlanTextInput(
                        label: s.t('plan.warnings.search'),
                        placeholder: s.t('plan.warnings.search'),
                        search: true,
                        value: _query,
                        onChanged: (value) => setState(() => _query = value),
                      ),
                    ),
                    SizedBox(
                      width: 220,
                      child: PlanSelect(
                        label: s.t('plan.warnings.type'),
                        value: _type,
                        options: [
                          ('all', s.t('plan.warnings.all')),
                          for (final key in types) (key, s.t(key)),
                        ],
                        onChanged: (value) => setState(() => _type = value),
                      ),
                    ),
                  ],
                ),
            ],
          ),
          const SizedBox(height: 10),
          Semantics(
            role: SemanticsRole.list,
            explicitChildNodes: true,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              mainAxisSize: MainAxisSize.min,
              children: [
                for (final warning in visible)
                  _WarningItem(
                    warning: warning,
                    s: s,
                    focusCode: () {
                      final code = canonicalRfiInText(warning.subject);
                      return code != null &&
                              focusable.contains(code) &&
                              widget.onFocusCode != null
                          ? code
                          : null;
                    }(),
                    onFocusCode: widget.onFocusCode,
                  ),
                if (visible.isEmpty)
                  Padding(
                    padding: const EdgeInsets.symmetric(vertical: 6),
                    child: Text(
                      s.t('plan.warnings.none'),
                      style: planMuted(context),
                    ),
                  ),
              ],
            ),
          ),
          if (filtering) ...[
            const SizedBox(height: 8),
            Text(
              s.t('plan.warnings.showing', {
                'shown': visible.length,
                'count': warnings.length,
              }),
              style: planMuted(context, rem: 0.72),
            ),
          ],
        ],
      ),
    );
  }
}

class _WarningItem extends StatelessWidget {
  const _WarningItem({
    required this.warning,
    required this.s,
    required this.focusCode,
    required this.onFocusCode,
  });

  final Warning warning;
  final Strings s;
  final String? focusCode;
  final ValueChanged<String>? onFocusCode;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final code = focusCode;
    return Semantics(
      role: SemanticsRole.listItem,
      container: true,
      child: Container(
        padding: const EdgeInsets.symmetric(vertical: 8),
        decoration: BoxDecoration(
          border: Border(top: BorderSide(color: t.line)),
        ),
        child: Row(
          children: [
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                mainAxisSize: MainAxisSize.min,
                children: [
                  if (warning.subject.isNotEmpty)
                    Text(warning.subject, style: planStrong(context, rem: 0.8)),
                  Text(s.t(warning.key), style: planMuted(context)),
                ],
              ),
            ),
            if (code != null)
              PlanTextButton(
                icon: AppIcons.eye,
                onPressed: () => onFocusCode?.call(code),
                child: Text(s.t('plan.warnings.focus')),
              ),
          ],
        ),
      ),
    );
  }
}

/// `SignedStageNavigator`: jump buttons to the sections of the signed staging
/// preview. [onJump] receives the section id the button targets.
class SignedStageNavigator extends StatelessWidget {
  const SignedStageNavigator({
    super.key,
    required this.plan,
    required this.rows,
    required this.s,
    required this.onJump,
  });

  final Plan plan;
  final List<Map<String, Object?>> rows;
  final Strings s;
  final ValueChanged<String> onJump;

  @override
  Widget build(BuildContext context) {
    final reviewItems = signedWarningReviewItems(plan);
    final approved = rows.where((row) => row['enabled'] != false).length;
    Widget item(
      String section,
      IconData icon,
      String label,
      int count, {
      bool enabled = true,
    }) => _NavButton(
      icon: icon,
      label: label,
      count: count,
      onPressed: enabled ? () => onJump(section) : null,
    );
    return Semantics(
      container: true,
      explicitChildNodes: true,
      label: s.t('rfi.signed.nav.label'),
      child: Wrap(
        spacing: 8,
        runSpacing: 8,
        children: [
          item(
            'signed-approved-records',
            AppIcons.badgeCheck,
            s.t('rfi.signed.nav.approved'),
            approved,
          ),
          item(
            'signed-page-comparison',
            AppIcons.files,
            s.t('rfi.signed.nav.compare'),
            rows.length,
          ),
          item(
            'signed-warning-review',
            AppIcons.circleAlert,
            s.t('rfi.signed.nav.review'),
            reviewItems.length,
            enabled: reviewItems.isNotEmpty,
          ),
          item(
            'signed-warning-list',
            AppIcons.fileSearch,
            s.t('ui.warnings'),
            plan.warnings.length,
            enabled: plan.warnings.isNotEmpty,
          ),
        ],
      ),
    );
  }
}

class _NavButton extends StatelessWidget {
  const _NavButton({
    required this.icon,
    required this.label,
    required this.count,
    required this.onPressed,
  });

  final IconData icon;
  final String label;
  final int count;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final enabled = onPressed != null;
    return Pressable(
      onPressed: onPressed,
      builder: (context, hovered, focused) => FocusRing(
        visible: focused,
        radius: 9,
        child: Opacity(
          opacity: enabled ? 1 : 0.4,
          child: Container(
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 9),
            decoration: BoxDecoration(
              color: hovered && enabled ? t.surface3 : t.surface,
              border: Border.all(color: t.line),
              borderRadius: BorderRadius.circular(9),
            ),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                ExcludeSemantics(
                  child: Icon(icon, size: remPx(context, 1), color: t.text),
                ),
                const SizedBox(width: 7),
                Text(label, style: planBody(context, rem: 0.8)),
                const SizedBox(width: 8),
                Text('$count', style: planStrong(context, rem: 0.85)),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// `SignedWarningReviewSummary`: signed files that need review, with reasons.
class SignedWarningReviewSummary extends StatelessWidget {
  const SignedWarningReviewSummary({
    super.key,
    required this.plan,
    required this.s,
  });

  final Plan plan;
  final Strings s;

  @override
  Widget build(BuildContext context) {
    final items = signedWarningReviewItems(plan);
    if (items.isEmpty) return const SizedBox.shrink();
    final t = DocFlowTokens.of(context);
    return PlanCard(
      semanticsLabel: s.t('rfi.signed.warningReview.title'),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          PlanHeading(
            icon: AppIcons.circleAlert,
            title: s.t('rfi.signed.warningReview.title'),
            subtitle: s.t('rfi.signed.warningReview.help'),
            trailing: Text('${items.length}', style: planStrong(context)),
          ),
          for (final item in items)
            Container(
              margin: const EdgeInsets.only(top: 10),
              padding: const EdgeInsets.all(10),
              decoration: BoxDecoration(
                color: t.surface2,
                borderRadius: BorderRadius.circular(8),
              ),
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
                        item.sourceFile,
                        style: planStrong(context, rem: 0.8),
                      ),
                      Container(
                        padding: const EdgeInsets.symmetric(
                          horizontal: 7,
                          vertical: 1,
                        ),
                        decoration: BoxDecoration(
                          border: Border.all(
                            color: item.status == 'blocked'
                                ? t.danger
                                : item.status == 'exists'
                                ? t.warning
                                : t.line,
                          ),
                          borderRadius: BorderRadius.circular(999),
                        ),
                        child: Text(
                          s.t('rfi.signed.warningReview.${item.status}'),
                          style: planMuted(context, rem: 0.7),
                        ),
                      ),
                    ],
                  ),
                  const SizedBox(height: 3),
                  Text(
                    '${s.t('rfi.signed.warningReview.destination')}: '
                    '${item.destination}',
                    style: planMuted(context, rem: 0.72),
                  ),
                  for (final reason in item.reasons)
                    Padding(
                      padding: const EdgeInsets.only(top: 6),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Text(
                            '${reason.pageNumber != null ? s.t('rfi.signed.warningReview.page', {'page': reason.pageNumber!}) : s.t('rfi.signed.warningReview.file')}'
                            '${reason.code.isNotEmpty ? ' · ${reason.code}' : ''}',
                            style: planMuted(context, rem: 0.72),
                          ),
                          Text(
                            s.t(reason.key),
                            style: planStrong(context, rem: 0.78),
                          ),
                          if (reason.detail.isNotEmpty)
                            Text(
                              reason.detail,
                              style: planMuted(context, rem: 0.72),
                            ),
                        ],
                      ),
                    ),
                ],
              ),
            ),
        ],
      ),
    );
  }
}

/// `SignedPageComparison`: the archived first page beside the signed one for
/// a chosen record. Setting [focusCode] selects that record and scrolls the
/// comparison into view.
class SignedPageComparison extends StatefulWidget {
  const SignedPageComparison({
    super.key,
    required this.plan,
    required this.editorRows,
    required this.focusCode,
    required this.s,
  });

  final Plan plan;
  final List<Map<String, Object?>> editorRows;
  final String focusCode;
  final Strings s;

  @override
  State<SignedPageComparison> createState() => _SignedPageComparisonState();
}

class _SignedPageComparisonState extends State<SignedPageComparison> {
  String _selectedId = '';
  String? _expanded;
  final OverlayPortalController _modal = OverlayPortalController();

  Map<String, Object?> get _raw =>
      recordValue(widget.plan.meta['pageComparisons']) ?? const {};

  List<Map<String, Object?>> _available() =>
      _comparableRows(widget.plan, widget.editorRows);

  List<String> _availableIds(List<Map<String, Object?>> available) => [
    for (final row in available) jsString(row['_rowId']),
  ];

  @override
  void initState() {
    super.initState();
    final ids = _availableIds(_available());
    _selectedId = ids.isEmpty ? '' : ids.first;
    _applyFocus(scroll: true);
  }

  @override
  void didUpdateWidget(SignedPageComparison oldWidget) {
    super.didUpdateWidget(oldWidget);
    final available = _available();
    final ids = _availableIds(available);
    if (!ids.contains(_selectedId)) {
      _selectedId = ids.isEmpty ? '' : ids.first;
    }
    final oldIds = _availableIds(
      _comparableRows(oldWidget.plan, oldWidget.editorRows),
    );
    if (oldWidget.focusCode != widget.focusCode ||
        oldIds.join('\u0000') != ids.join('\u0000')) {
      _applyFocus(scroll: true);
    }
  }

  void _applyFocus({required bool scroll}) {
    final code = widget.focusCode;
    if (code.isEmpty) return;
    for (final row in _available()) {
      if (row['code'] == code) {
        _selectedId = jsString(row['_rowId']);
        if (scroll) {
          WidgetsBinding.instance.addPostFrameCallback((_) {
            if (mounted) context.findRenderObject()?.showOnScreen();
          });
        }
        return;
      }
    }
  }

  void _open(String which) {
    setState(() => _expanded = which);
    _modal.show();
  }

  void _close() {
    setState(() => _expanded = null);
    _modal.hide();
  }

  @override
  Widget build(BuildContext context) {
    final s = widget.s;
    final t = DocFlowTokens.of(context);
    final plan = widget.plan;
    final available = _available();
    final omittedRaw = plan.meta['pagePreviewOmitted'];
    final omitted = omittedRaw is num ? omittedRaw : 0;
    if (available.isEmpty) {
      return omitted > 0
          ? Notice(
              kind: NoticeKind.warning,
              child: Text(s.t('rfi.signed.previewUnavailable')),
            )
          : const SizedBox.shrink();
    }
    final selected = available.firstWhere(
      (row) => row['_rowId'] == _selectedId,
      orElse: () => available.first,
    );
    final selectedRowId = jsString(selected['_rowId']);
    final comparison = recordValue(_raw[selectedRowId]) ?? const {};
    final archives = recordValue(comparison['archives']) ?? const {};
    final archiveToken = selected['archiveFile'] is String
        ? selected['archiveFile']! as String
        : '';
    final archivePreview = safePngThumbnail(archives[archiveToken]);
    final signedPreview = safePngThumbnail(comparison['signed']);
    final code = selected['code'] is String
        ? selected['code']! as String
        : _selectedId;
    final signedFile = selected['signedFile'] is String
        ? selected['signedFile']! as String
        : '';
    final editor = recordValue(plan.meta['editor']);
    final choiceLabels = recordValue(editor?['choiceLabels']);
    final archiveLabels = recordValue(choiceLabels?['archiveFile']) ?? const {};
    final archiveFile = archiveLabels[archiveToken] is String
        ? archiveLabels[archiveToken]! as String
        : '';
    final selectedIndex = available.indexWhere(
      (row) => jsString(row['_rowId']) == selectedRowId,
    );
    void selectOffset(int offset) {
      final index = selectedIndex + offset;
      if (index >= 0 && index < available.length) {
        setState(() => _selectedId = jsString(available[index]['_rowId']));
      }
    }

    final expandedPreview = _expanded == 'archive'
        ? archivePreview
        : _expanded == 'signed'
        ? signedPreview
        : null;

    Widget figure({
      required String before,
      required String title,
      required String file,
      required String? preview,
      required String alt,
      required String which,
      required String emptyText,
    }) => Container(
      padding: const EdgeInsets.all(10),
      decoration: BoxDecoration(
        color: t.surface2,
        borderRadius: BorderRadius.circular(8),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(before, style: planMuted(context, rem: 0.7)),
          Text(title, style: planStrong(context, rem: 0.82)),
          if (file.isNotEmpty)
            planTooltip(
              file,
              Text(
                file,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: planMuted(context, rem: 0.7),
              ),
            ),
          const SizedBox(height: 8),
          if (preview != null)
            Pressable(
              label: '$alt · ${s.t('rfi.signed.previewEnlarge')}',
              onPressed: () => _open(which),
              builder: (context, hovered, focused) => FocusRing(
                visible: focused,
                radius: 7,
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Container(
                      constraints: const BoxConstraints(maxHeight: 360),
                      decoration: BoxDecoration(
                        border: Border.all(
                          color: hovered ? t.lineStrong : t.line,
                        ),
                      ),
                      child: _DataImage(uri: preview, alt: alt),
                    ),
                    const SizedBox(height: 4),
                    Text(
                      s.t('rfi.signed.previewEnlarge'),
                      style: planMuted(context, rem: 0.7),
                    ),
                  ],
                ),
              ),
            )
          else
            Container(
              constraints: const BoxConstraints(minHeight: 120),
              alignment: Alignment.center,
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(
                border: Border.all(color: t.line),
                borderRadius: BorderRadius.circular(7),
              ),
              child: Text(
                emptyText,
                textAlign: TextAlign.center,
                style: planMuted(context),
              ),
            ),
        ],
      ),
    );

    final archiveFigure = figure(
      before: s.t('rfi.signed.previewBefore'),
      title: s.t('rfi.signed.previewArchive'),
      file: archiveFile,
      preview: archivePreview,
      alt: s.t('rfi.signed.previewArchiveAlt', {'code': code}),
      which: 'archive',
      emptyText: archiveToken.isNotEmpty
          ? s.t('rfi.signed.previewUnavailable')
          : s.t('rfi.signed.previewChooseArchive'),
    );
    final signedFigure = figure(
      before: s.t('rfi.signed.previewAfter'),
      title: s.t('rfi.signed.previewSigned'),
      file: signedFile,
      preview: signedPreview,
      alt: s.t('rfi.signed.previewSignedAlt', {'code': code}),
      which: 'signed',
      emptyText: s.t('rfi.signed.previewUnavailable'),
    );

    return OverlayPortal(
      controller: _modal,
      overlayChildBuilder: (context) {
        final which = _expanded;
        if (which == null || expandedPreview == null) {
          return const SizedBox.shrink();
        }
        return Material(
          type: MaterialType.transparency,
          child: Modal(
            title:
                '$code · ${s.t(which == 'archive' ? 'rfi.signed.previewArchive' : 'rfi.signed.previewSigned')}',
            s: s,
            onClose: _close,
            child: _DataImage(
              uri: expandedPreview,
              alt: s.t(
                which == 'archive'
                    ? 'rfi.signed.previewArchiveAlt'
                    : 'rfi.signed.previewSignedAlt',
                {'code': code},
              ),
            ),
          ),
        );
      },
      child: PlanCard(
        semanticsLabel: s.t('rfi.signed.previewTitle'),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          mainAxisSize: MainAxisSize.min,
          children: [
            Wrap(
              alignment: WrapAlignment.spaceBetween,
              crossAxisAlignment: WrapCrossAlignment.center,
              spacing: 12,
              runSpacing: 8,
              children: [
                Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Semantics(
                      header: true,
                      child: Text(
                        s.t('rfi.signed.previewTitle'),
                        style: planStrong(context),
                      ),
                    ),
                    Text(
                      s.t('rfi.signed.previewHelp'),
                      style: planMuted(context),
                    ),
                  ],
                ),
                Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    PlanIconButton(
                      icon: AppIcons.arrowLeft,
                      label: s.t('rfi.signed.previewPrevious'),
                      onPressed: selectedIndex <= 0
                          ? null
                          : () => selectOffset(-1),
                    ),
                    const SizedBox(width: 6),
                    if (available.length > 1)
                      SizedBox(
                        width: 260,
                        child: PlanSelect(
                          label: s.t('rfi.signed.previewRfi'),
                          value: selectedRowId,
                          options: [
                            for (final row in available)
                              (
                                jsString(row['_rowId']),
                                row['code'] is String
                                    ? row['code']! as String
                                    : jsString(row['_rowId']),
                              ),
                          ],
                          onChanged: (value) =>
                              setState(() => _selectedId = value),
                        ),
                      )
                    else
                      Text(code, style: planMono(context)),
                    const SizedBox(width: 6),
                    PlanIconButton(
                      icon: AppIcons.arrowRight,
                      label: s.t('rfi.signed.previewNext'),
                      onPressed: selectedIndex >= available.length - 1
                          ? null
                          : () => selectOffset(1),
                    ),
                  ],
                ),
              ],
            ),
            const SizedBox(height: 10),
            Wrap(
              spacing: 8,
              runSpacing: 4,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: [
                Text(
                  s.t('rfi.signed.previewArchive'),
                  style: planBody(context),
                ),
                ExcludeSemantics(
                  child: Icon(
                    AppIcons.arrowRight,
                    size: remPx(context, 0.9),
                    color: t.muted,
                  ),
                ),
                Text(
                  s.t('rfi.signed.previewSigned'),
                  style: planStrong(context, rem: 0.82),
                ),
                Text(
                  s.t('rfi.signed.previewOnlyPageOne'),
                  style: planMuted(context, rem: 0.72),
                ),
              ],
            ),
            const SizedBox(height: 10),
            LayoutBuilder(
              builder: (context, constraints) => constraints.maxWidth < 560
                  ? Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        archiveFigure,
                        const SizedBox(height: 10),
                        signedFigure,
                      ],
                    )
                  : Row(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Expanded(child: archiveFigure),
                        const SizedBox(width: 10),
                        Expanded(child: signedFigure),
                      ],
                    ),
            ),
            if (omitted > 0) ...[
              const SizedBox(height: 8),
              Text(
                s.t('rfi.signed.previewOmitted', {'count': jsString(omitted)}),
                style: planMuted(context, rem: 0.72),
              ),
            ],
          ],
        ),
      ),
    );
  }
}

/// The editor rows that have at least one usable page thumbnail.
List<Map<String, Object?>> _comparableRows(
  Plan plan,
  List<Map<String, Object?>> rows,
) {
  final raw = recordValue(plan.meta['pageComparisons']) ?? const {};
  return rows.where((row) {
    final rowId = row['_rowId'] is String ? row['_rowId']! as String : '';
    final record = recordValue(raw[rowId]);
    if (record == null) return false;
    final archives = recordValue(record['archives']);
    return safePngThumbnail(record['signed']) != null ||
        (archives?.values.any((value) => safePngThumbnail(value) != null) ??
            false);
  }).toList();
}

/// A PNG data URI that [safePngThumbnail] accepted, with [alt] as its name.
class _DataImage extends StatelessWidget {
  const _DataImage({required this.uri, required this.alt});

  final String uri;
  final String alt;

  @override
  Widget build(BuildContext context) {
    final payload = uri.substring(uri.indexOf(',') + 1);
    return Image.memory(
      base64Decode(payload),
      semanticLabel: alt,
      fit: BoxFit.contain,
      gaplessPlayback: true,
      errorBuilder: (context, error, stack) =>
          Text(alt, style: planMuted(context)),
    );
  }
}
