// Helpers shared by the review, result, history and recovery screens. They
// port small functions from frontend/src/App.tsx (`formatConfidencePercent`,
// `formatDate`) and the `.page-heading`, `.history-row` and `.safe-badge`
// rules from styles.css.

import 'package:flutter/material.dart' hide IconButton, Switch;

import '../../theme/tokens.dart';
import '../widgets/metrics.dart';

/// `formatConfidencePercent` from App.tsx. [value] is a fraction (0 to 1) by
/// default, or a percentage when [percentScale] is true. Returns null for a
/// value that is not a finite number, which the screens show as an em dash.
String? formatConfidencePercent(Object? value, {bool percentScale = false}) {
  final numeric = switch (value) {
    final num n => n.toDouble(),
    final String text when text.trim().isNotEmpty => double.tryParse(text),
    _ => null,
  };
  if (numeric == null || !numeric.isFinite) return null;
  final percent = percentScale ? numeric : numeric * 100;
  final clamped = percent.clamp(0.0, 100.0);
  return '${clamped.round()}%';
}

/// `formatDate` from App.tsx: a medium date and a short time. The text comes
/// from the Material localizations of the active locale, which need no intl
/// date data to be loaded. A value that is not a date is returned unchanged.
String formatScreenDate(BuildContext context, String value) {
  final parsed = DateTime.tryParse(value);
  if (parsed == null) return value;
  final local = parsed.toLocal();
  final localizations = MaterialLocalizations.of(context);
  final time = localizations.formatTimeOfDay(
    TimeOfDay.fromDateTime(local),
    alwaysUse24HourFormat: MediaQuery.alwaysUse24HourFormatOf(context),
  );
  return '${localizations.formatMediumDate(local)}, $time';
}

/// Reads a JSON-style number from engine metadata, as `Number(value ?? 0)`
/// does in App.tsx. Anything that is not numeric reads as zero.
int screenInt(Object? value) {
  if (value is num) return value.toInt();
  if (value is String) return int.tryParse(value.trim()) ?? 0;
  return 0;
}

/// `.page-heading.compact-heading`: a title and a line of help, with an
/// optional widget (a badge or a button) on the trailing side.
class PageHeading extends StatelessWidget {
  const PageHeading({
    super.key,
    required this.title,
    required this.subtitle,
    this.trailing,
  });

  final String title;
  final String subtitle;
  final Widget? trailing;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final titleSize = remPx(context, 1.5);
    return Padding(
      padding: const EdgeInsets.only(bottom: 16),
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
              Semantics(
                header: true,
                child: Text(
                  title,
                  style: TextStyle(
                    fontSize: titleSize,
                    fontWeight: FontWeight.w700,
                    letterSpacing: -0.02 * titleSize,
                    color: t.text,
                  ),
                ),
              ),
              const SizedBox(height: 3),
              Text(
                subtitle,
                style: TextStyle(
                  fontSize: remPx(context, 0.875),
                  height: 1.5,
                  color: t.muted,
                ),
              ),
            ],
          ),
          ?trailing,
        ],
      ),
    );
  }
}

/// `.safe-badge`: a bordered label with an icon, used for the cloud or local
/// history source.
class SafeBadge extends StatelessWidget {
  const SafeBadge({super.key, required this.icon, required this.label});

  final Widget icon;
  final String label;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
      decoration: BoxDecoration(
        border: Border.all(color: t.line),
        borderRadius: BorderRadius.circular(999),
        color: t.surface,
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          IconTheme(
            data: IconThemeData(size: remPx(context, 0.9), color: t.text),
            child: icon,
          ),
          const SizedBox(width: 6),
          Flexible(
            child: Text(
              label,
              style: TextStyle(fontSize: remPx(context, 0.8), color: t.text),
            ),
          ),
        ],
      ),
    );
  }
}

/// `.status-label`: a quiet inline label with an optional leading icon.
class StatusLabel extends StatelessWidget {
  const StatusLabel({super.key, required this.label, this.icon});

  final String label;
  final Widget? icon;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        if (icon != null) ...[
          IconTheme(
            data: IconThemeData(size: remPx(context, 0.85), color: t.muted),
            child: icon!,
          ),
          const SizedBox(width: 5),
        ],
        Flexible(
          child: Text(
            label,
            style: TextStyle(fontSize: remPx(context, 0.8), color: t.muted),
          ),
        ),
      ],
    );
  }
}

/// `.history-row`: a bordered row with a leading icon, a title with a detail
/// line, and trailing controls. Used by the history and recovery lists.
class ScreenRow extends StatelessWidget {
  const ScreenRow({
    super.key,
    required this.icon,
    required this.title,
    required this.detail,
    this.trailing = const [],
  });

  final Widget icon;
  final String title;
  final String detail;
  final List<Widget> trailing;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return Container(
      margin: const EdgeInsets.only(bottom: 8),
      padding: const EdgeInsets.all(14),
      decoration: BoxDecoration(
        color: t.surface,
        border: Border.all(color: t.line),
        borderRadius: BorderRadius.circular(8),
      ),
      child: LayoutBuilder(
        builder: (context, constraints) {
          final iconAndText = Row(
            children: [
              IconTheme(
                data: IconThemeData(size: remPx(context, 1.1), color: t.text),
                child: icon,
              ),
              const SizedBox(width: 12),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Text(
                      title,
                      style: TextStyle(
                        fontSize: remPx(context, 0.9),
                        fontWeight: FontWeight.w700,
                        color: t.text,
                      ),
                    ),
                    const SizedBox(height: 3),
                    Text(
                      detail,
                      style: TextStyle(
                        fontSize: remPx(context, 0.78),
                        color: t.muted,
                      ),
                    ),
                  ],
                ),
              ),
            ],
          );
          if (trailing.isEmpty) return iconAndText;
          final controls = Wrap(
            alignment: WrapAlignment.end,
            crossAxisAlignment: WrapCrossAlignment.center,
            spacing: 7,
            runSpacing: 7,
            children: trailing,
          );
          // Wide rows keep the controls at their natural width beside the text,
          // which takes the remaining space. Narrow rows stack the controls under
          // the text, so a long button label has the full row width.
          if (constraints.maxWidth < 640) {
            return Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                iconAndText,
                const SizedBox(height: 10),
                Align(alignment: Alignment.centerRight, child: controls),
              ],
            );
          }
          return Row(
            children: [
              Expanded(child: iconAndText),
              const SizedBox(width: 12),
              controls,
            ],
          );
        },
      ),
    );
  }
}
