// SettingsGroup, ported from `SettingsGroup` in App.tsx and the `.settings-group`
// rules in styles.css. A card with a title (icon tile, heading and optional
// help) beside its content. Below 700px the title sits above the content.

import 'package:flutter/material.dart' hide IconButton, Switch;

import '../../theme/tokens.dart';
import '../widgets/metrics.dart';
import 'screen_parts.dart';

class SettingsGroup extends StatelessWidget {
  const SettingsGroup({
    super.key,
    required this.title,
    required this.icon,
    required this.child,
    this.help,
  });

  final String title;
  final IconData icon;
  final String? help;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final padding = EdgeInsets.symmetric(
      vertical: 16 * t.spaceScale,
      horizontal: 20,
    );
    final decoration = BoxDecoration(
      color: t.surface,
      borderRadius: BorderRadius.circular(t.radius),
      boxShadow: t.panelShadow,
    );
    final titleBlock = _SettingsTitle(title: title, icon: icon, help: help);

    return LayoutBuilder(
      builder: (context, constraints) {
        final narrow = constraints.maxWidth <= narrowPaneBreakpoint;
        if (narrow) {
          return Container(
            padding: padding,
            decoration: decoration,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [titleBlock, const SizedBox(height: 14), child],
            ),
          );
        }
        // grid-template-columns: minmax(150px, 0.36fr) 1fr; gap 24px.
        final inner = constraints.maxWidth - 40 - 24;
        final titleWidth = (inner * 0.36 / 1.36).clamp(150.0, inner);
        return Container(
          padding: padding,
          decoration: decoration,
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              SizedBox(width: titleWidth, child: titleBlock),
              const SizedBox(width: 24),
              Expanded(child: child),
            ],
          ),
        );
      },
    );
  }
}

class _SettingsTitle extends StatelessWidget {
  const _SettingsTitle({required this.title, required this.icon, this.help});

  final String title;
  final IconData icon;
  final String? help;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Container(
          width: 30,
          height: 30,
          alignment: Alignment.center,
          decoration: BoxDecoration(
            color: t.surface3,
            borderRadius: BorderRadius.circular(8),
          ),
          child: Icon(icon, size: remPx(context, 1), color: t.text),
        ),
        const SizedBox(width: 12),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Semantics(
                header: true,
                child: Text(
                  title,
                  style: Theme.of(context).textTheme.titleMedium,
                ),
              ),
              if (help != null) ...[
                const SizedBox(height: 4),
                Text(
                  help!,
                  style: TextStyle(
                    fontSize: remPx(context, 0.8),
                    height: 1.45,
                    color: t.muted,
                  ),
                ),
              ],
            ],
          ),
        ),
      ],
    );
  }
}
