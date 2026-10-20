// Notice and EmptyState, ported from ui.tsx and the `.notice` / `.empty-state`
// rules.

import 'package:flutter/material.dart' hide IconButton, Switch;

import '../../theme/app_icons.dart';
import '../../theme/tokens.dart';
import 'metrics.dart';

enum NoticeKind { error, warning, info }

/// An inline message box. The icon is a check for [NoticeKind.info] and an
/// alert otherwise.
class Notice extends StatelessWidget {
  const Notice({
    super.key,
    required this.kind,
    required this.child,
    this.title,
  });

  final NoticeKind kind;
  final String? title;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final (color, border) = switch (kind) {
      NoticeKind.error => (t.danger, Color.lerp(t.line, t.danger, 0.4)!),
      NoticeKind.warning => (t.warning, Color.lerp(t.line, t.warning, 0.4)!),
      NoticeKind.info => (t.text, t.line),
    };
    final icon = kind == NoticeKind.info
        ? AppIcons.checkCircle
        : AppIcons.circleAlert;

    return Container(
      padding: const EdgeInsets.symmetric(vertical: 12, horizontal: 14),
      decoration: BoxDecoration(
        color: t.surface2,
        border: Border.all(color: border),
        borderRadius: BorderRadius.circular(9),
      ),
      child: DefaultTextStyle(
        style: TextStyle(
          fontSize: remPx(context, 0.8),
          height: 1.5,
          color: color,
        ),
        child: IconTheme(
          data: IconThemeData(size: remPx(context, 1.1), color: color),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Padding(
                padding: const EdgeInsets.only(top: 2),
                child: Icon(icon),
              ),
              const SizedBox(width: 10),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    if (title != null) ...[
                      Text(
                        title!,
                        style: const TextStyle(fontWeight: FontWeight.w700),
                      ),
                      const SizedBox(height: 2),
                    ],
                    child,
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// A centred placeholder with an icon tile, a heading, an optional message and
/// an optional action.
class EmptyState extends StatelessWidget {
  const EmptyState({
    super.key,
    required this.icon,
    required this.title,
    this.message,
    this.action,
  });

  final Widget icon;
  final String title;
  final String? message;
  final Widget? action;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final headingSize = remPx(context, 1.05);
    return ConstrainedBox(
      constraints: const BoxConstraints(minHeight: 230),
      child: Padding(
        padding: const EdgeInsets.all(30),
        child: Center(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Container(
                width: DocFlowMetrics.emptyStateIconBox,
                height: DocFlowMetrics.emptyStateIconBox,
                margin: const EdgeInsets.only(bottom: 15),
                alignment: Alignment.center,
                decoration: BoxDecoration(
                  color: t.surface3,
                  borderRadius: BorderRadius.circular(12),
                ),
                child: IconTheme(
                  data: IconThemeData(
                    size: DocFlowMetrics.emptyStateIcon,
                    color: t.text,
                  ),
                  child: icon,
                ),
              ),
              Semantics(
                header: true,
                child: Text(
                  title,
                  textAlign: TextAlign.center,
                  style: TextStyle(
                    fontSize: headingSize,
                    fontWeight: FontWeight.w700,
                    letterSpacing: -0.015 * headingSize,
                    color: t.text,
                  ),
                ),
              ),
              const SizedBox(height: 4),
              if (message != null)
                ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 460),
                  child: Text(
                    message!,
                    textAlign: TextAlign.center,
                    style: TextStyle(
                      fontSize: remPx(context, 1),
                      height: 1.5,
                      color: t.muted,
                    ),
                  ),
                ),
              if (action != null) ...[const SizedBox(height: 16), action!],
            ],
          ),
        ),
      ),
    );
  }
}
