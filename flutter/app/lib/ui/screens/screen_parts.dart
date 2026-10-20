// Layout pieces shared by the account and settings screens: the large brand
// mark of the login card (`.brand-mark.large`), the pane that holds a
// settings-style page (`.pane` / `.settings-pane`), the page heading, the
// spinning refresh icon, and the username pattern from App.tsx.

import 'dart:math' as math;

import 'package:flutter/material.dart' hide IconButton, Switch;

import '../../theme/app_icons.dart';
import '../../theme/tokens.dart';
import '../widgets/metrics.dart';

/// `usernamePattern` in App.tsx: 3 to 24 letters, digits, dots, dashes or
/// underscores.
final RegExp usernamePattern = RegExp(r'^[a-zA-Z0-9._-]{3,24}$');

/// Width below which the `(max-width: 700px)` rules of styles.css apply.
const double narrowPaneBreakpoint = 700;

/// The square brand mark at login (`.brand-mark.large`): 54px, inverse
/// background, 25px glyph, centred with a 14px gap below.
class LargeBrandMark extends StatelessWidget {
  const LargeBrandMark({super.key});

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return Center(
      child: Container(
        width: 54,
        height: 54,
        margin: const EdgeInsets.only(bottom: 14),
        alignment: Alignment.center,
        decoration: BoxDecoration(
          color: t.inverse,
          borderRadius: BorderRadius.circular(13),
        ),
        child: Icon(AppIcons.files, size: 25, color: t.inverseText),
      ),
    );
  }
}

/// A scrolling page column of `.pane` width: `min(100% - clamp(24px, 4vw,
/// 64px), maxWidth)`, horizontally padded by `clamp(12px, 2vw, 24px)`.
class PaneScroll extends StatelessWidget {
  const PaneScroll({super.key, required this.child, this.maxWidth = 1160});

  final Widget child;
  final double maxWidth;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return LayoutBuilder(
      builder: (context, constraints) {
        final viewport = constraints.maxWidth;
        final narrow = viewport <= narrowPaneBreakpoint;
        final outer = math.min(
          viewport - (viewport * 0.04).clamp(24.0, 64.0),
          maxWidth,
        );
        final side = narrow
            ? (viewport * 0.02).clamp(10.0, 14.0)
            : (viewport * 0.02).clamp(12.0, 24.0);
        final top = (narrow ? 14.0 : 28.0) * t.spaceScale;
        final bottom = (narrow ? 14.0 : 20.0) * t.spaceScale;
        return SingleChildScrollView(
          padding: EdgeInsets.only(top: top, bottom: bottom),
          child: Center(
            child: SizedBox(
              width: outer,
              child: Padding(
                padding: EdgeInsets.symmetric(horizontal: side),
                child: child,
              ),
            ),
          ),
        );
      },
    );
  }
}

/// `.page-heading`: the title with its help text on the left and a trailing
/// widget on the right, aligned to the bottom (or centre when [compact]).
class PageHeading extends StatelessWidget {
  const PageHeading({
    super.key,
    required this.title,
    required this.help,
    this.trailing,
    this.compact = false,
  });

  final String title;
  final String help;
  final Widget? trailing;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final heading = Theme.of(context).textTheme.headlineMedium;
    return Padding(
      padding: EdgeInsets.only(bottom: 16 * t.spaceScale),
      child: Row(
        crossAxisAlignment: compact
            ? CrossAxisAlignment.center
            : CrossAxisAlignment.end,
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Semantics(header: true, child: Text(title, style: heading)),
                const SizedBox(height: 5),
                ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 720),
                  child: Text(
                    help,
                    style: TextStyle(
                      fontSize: remPx(context, 0.9),
                      height: 1.5,
                      color: t.muted,
                    ),
                  ),
                ),
              ],
            ),
          ),
          if (trailing != null) ...[const SizedBox(width: 24), trailing!],
        ],
      ),
    );
  }
}

/// The `.safe-badge` pill: a small bordered label with a leading glyph.
class SafeBadge extends StatelessWidget {
  const SafeBadge({super.key, required this.icon, required this.label});

  final IconData icon;
  final String label;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return Container(
      constraints: const BoxConstraints(minHeight: 30, maxWidth: 220),
      padding: const EdgeInsets.symmetric(vertical: 5, horizontal: 10),
      alignment: Alignment.center,
      decoration: BoxDecoration(
        border: Border.all(color: t.line),
        borderRadius: BorderRadius.circular(999),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(icon, size: remPx(context, 0.9), color: t.muted),
          const SizedBox(width: 6),
          Flexible(
            child: Text(
              label,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(fontSize: remPx(context, 0.75), color: t.muted),
            ),
          ),
        ],
      ),
    );
  }
}

/// A refresh-style glyph that turns while [spinning] (the React `spin` class
/// on an icon). Holds still when the platform asks for reduced motion.
class SpinningIcon extends StatefulWidget {
  const SpinningIcon({super.key, required this.icon, this.spinning = false});

  final IconData icon;
  final bool spinning;

  @override
  State<SpinningIcon> createState() => _SpinningIconState();
}

class _SpinningIconState extends State<SpinningIcon>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 900),
  );

  @override
  void initState() {
    super.initState();
    _sync();
  }

  @override
  void didUpdateWidget(SpinningIcon oldWidget) {
    super.didUpdateWidget(oldWidget);
    _sync();
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _sync();
  }

  void _sync() {
    final run = widget.spinning && !MediaQuery.disableAnimationsOf(context);
    if (run) {
      if (!_controller.isAnimating) _controller.repeat();
    } else {
      _controller.stop();
      _controller.value = 0;
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) =>
      RotationTransition(turns: _controller, child: Icon(widget.icon));
}
