// Header and StepIndicator, ported from ui.tsx and the `.app-header`,
// `.step-indicator` rules in styles.css.

import 'dart:math' as math;

import 'package:flutter/material.dart' hide IconButton, Switch;

import '../../theme/app_icons.dart';
import '../../theme/tokens.dart';
import 'buttons.dart';
import 'metrics.dart';

/// The top bar: an optional back button, the title, trailing actions, and an
/// optional row beneath it ([bottom], typically a [StepIndicator]).
class Header extends StatelessWidget {
  const Header({
    super.key,
    this.title,
    this.onBack,
    this.backLabel,
    this.trailing,
    this.bottom,
  });

  final String? title;
  final VoidCallback? onBack;

  /// Accessible name of the back button. Defaults to a left arrow, as the
  /// React `backLabel ?? "←"` did.
  final String? backLabel;
  final Widget? trailing;
  final Widget? bottom;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final width = MediaQuery.sizeOf(context).width;
    final narrow = width <= DocFlowMetrics.breakpointBottomBar;
    final phone = width <= 700;
    final inset = phone ? 10.0 : (width * 0.024).clamp(16.0, 32.0);

    final titleText = title == null
        ? null
        : Semantics(
            header: true,
            child: Text(
              title!,
              maxLines: narrow ? null : 1,
              overflow: narrow ? TextOverflow.visible : TextOverflow.ellipsis,
              style: TextStyle(
                fontSize: remPx(context, 0.95),
                fontWeight: FontWeight.w700,
                height: 1.2,
                letterSpacing: 0,
                color: t.text,
              ),
            ),
          );

    final left = Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        if (onBack != null)
          IconButton(
            label: backLabel ?? '←',
            onPressed: onBack,
            child: Icon(AppIcons.arrowLeft),
          ),
        if (onBack != null && titleText != null) const SizedBox(width: 8),
        if (titleText != null) Flexible(child: titleText),
      ],
    );

    final main = Container(
      constraints: BoxConstraints(minHeight: phone ? 56 : 62),
      padding: EdgeInsets.symmetric(horizontal: inset),
      child: Row(
        crossAxisAlignment: narrow
            ? CrossAxisAlignment.start
            : CrossAxisAlignment.center,
        children: [
          Expanded(
            child: Align(
              alignment: AlignmentDirectional.centerStart,
              child: left,
            ),
          ),
          const SizedBox(width: 12),
          if (trailing != null)
            Row(mainAxisSize: MainAxisSize.min, children: [trailing!]),
        ],
      ),
    );

    return DecoratedBox(
      decoration: BoxDecoration(
        color: t.surface,
        border: Border(bottom: BorderSide(color: t.line)),
      ),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          main,
          if (bottom != null) _HeaderDensity(child: bottom!),
        ],
      ),
    );
  }
}

/// Marks the content of a [Header]'s bottom row, which gets the compact
/// `.with-steps` step-indicator sizes.
class _HeaderDensity extends InheritedWidget {
  const _HeaderDensity({required super.child});

  static bool isDense(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<_HeaderDensity>() != null;

  @override
  bool updateShouldNotify(_HeaderDensity oldWidget) => false;
}

/// A row of numbered steps. Steps before [current] show a check mark and are
/// marked done, the step at [current] is selected, and connectors between
/// steps are drawn in the line colour (done connectors are darker).
class StepIndicator extends StatelessWidget {
  const StepIndicator({super.key, required this.current, required this.labels});

  final int current;
  final List<String> labels;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final dense = _HeaderDensity.isDense(context);
    final viewportWidth = MediaQuery.sizeOf(context).width;
    final viewportHeight = MediaQuery.sizeOf(context).height;
    final compact = viewportWidth <= DocFlowMetrics.breakpointBottomBar;
    final shortViewport = viewportHeight <= 620;
    final height = dense ? (shortViewport ? 40.0 : 42.0) : 50.0;
    final connectorTop = dense ? 10.0 : 12.0;
    final circle = dense ? 20.0 : 22.0;

    return LayoutBuilder(
      builder: (context, constraints) {
        final available = constraints.hasBoundedWidth
            ? constraints.maxWidth
            : viewportWidth;
        // width: min(100% - 20px, 480px) on compact widths, else min(390px, 100% - 40px).
        final width = compact
            ? math.min(available - 20, 480.0)
            : math.min(390.0, available - 40);
        return Center(
          child: SizedBox(
            width: width,
            height: height,
            child: Semantics(
              container: true,
              explicitChildNodes: true,
              child: Stack(
                children: [
                  Positioned.fill(
                    child: CustomPaint(
                      painter: _ConnectorPainter(
                        count: labels.length,
                        doneCount: current,
                        line: t.line,
                        lineDone: t.lineStrong,
                        top: connectorTop,
                      ),
                    ),
                  ),
                  Row(
                    children: [
                      for (var i = 0; i < labels.length; i++)
                        Expanded(
                          child: _StepItem(
                            index: i,
                            label: labels[i],
                            current: i == current,
                            done: i < current,
                            circle: circle,
                            compact: compact,
                          ),
                        ),
                    ],
                  ),
                ],
              ),
            ),
          ),
        );
      },
    );
  }
}

class _StepItem extends StatelessWidget {
  const _StepItem({
    required this.index,
    required this.label,
    required this.current,
    required this.done,
    required this.circle,
    required this.compact,
  });

  final int index;
  final String label;
  final bool current;
  final bool done;
  final double circle;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final active = current || done;
    final color = active ? t.text : t.muted;
    final numberColor = active ? t.inverseText : color;

    return MergeSemantics(
      child: Semantics(
        label: label,
        selected: current,
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            ExcludeSemantics(
              child: Container(
                width: circle,
                height: circle,
                alignment: Alignment.center,
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  color: active ? t.inverse : t.bg,
                  border: Border.all(color: active ? t.inverse : t.line),
                ),
                child: done
                    ? Icon(
                        AppIcons.check,
                        size: remPx(context, 0.75),
                        color: numberColor,
                      )
                    : Text(
                        '${index + 1}',
                        style: TextStyle(
                          fontSize: remPx(context, 0.7),
                          color: numberColor,
                        ),
                      ),
              ),
            ),
            const SizedBox(height: 4),
            ExcludeSemantics(
              child: Text(
                label,
                textAlign: TextAlign.center,
                style: TextStyle(
                  fontSize: remPx(context, compact ? 0.65 : 0.68),
                  height: 1.2,
                  color: color,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// Draws the 1px connectors between neighbouring steps. Each connector runs
/// from 18px right of one step's centre to 18px left of the next centre.
class _ConnectorPainter extends CustomPainter {
  _ConnectorPainter({
    required this.count,
    required this.doneCount,
    required this.line,
    required this.lineDone,
    required this.top,
  });

  final int count;
  final int doneCount;
  final Color line;
  final Color lineDone;
  final double top;

  @override
  void paint(Canvas canvas, Size size) {
    if (count < 2) return;
    final stepWidth = size.width / count;
    for (var i = 0; i < count - 1; i++) {
      final start = (i + 0.5) * stepWidth + 18;
      final end = (i + 1.5) * stepWidth - 18;
      final paint = Paint()..color = i < doneCount ? lineDone : line;
      canvas.drawRect(Rect.fromLTWH(start, top, end - start, 1), paint);
    }
  }

  @override
  bool shouldRepaint(_ConnectorPainter old) =>
      old.count != count ||
      old.doneCount != doneCount ||
      old.line != line ||
      old.lineDone != lineDone ||
      old.top != top;
}
