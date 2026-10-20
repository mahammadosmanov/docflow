// DfButton and IconButton, ported from `Button` and `IconButton` in ui.tsx.
// `Button` is named DfButton here so it does not collide with Material's
// button classes.

import 'package:flutter/material.dart' hide IconButton, Switch;

import '../../theme/tokens.dart';
import 'metrics.dart';
import 'pressable.dart';

enum DfButtonVariant { primary, secondary, ghost, danger }

/// A text button in one of four variants (`.button.primary` and friends).
class DfButton extends StatelessWidget {
  const DfButton({
    super.key,
    required this.child,
    this.onPressed,
    this.icon,
    this.variant = DfButtonVariant.primary,
    this.wide = false,
    this.focusNode,
  });

  final Widget child;
  final VoidCallback? onPressed;
  final Widget? icon;
  final DfButtonVariant variant;

  /// `.button.wide`: full width with an 18px top margin.
  final bool wide;
  final FocusNode? focusNode;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final enabled = onPressed != null;
    final (fg, bg, border) = switch (variant) {
      DfButtonVariant.primary => (t.inverseText, t.inverse, Colors.transparent),
      DfButtonVariant.secondary => (t.text, t.surface2, t.line),
      DfButtonVariant.ghost => (
        t.muted,
        Colors.transparent,
        Colors.transparent,
      ),
      // color-mix(in srgb, danger 40%, line)
      DfButtonVariant.danger => (
        t.danger,
        Colors.transparent,
        Color.lerp(t.line, t.danger, 0.4)!,
      ),
    };

    final button = Pressable(
      onPressed: onPressed,
      focusNode: focusNode,
      builder: (context, hovered, focused) {
        // `.button.secondary:hover` and `.button.ghost:hover` use surface-3.
        final hoverBg =
            (variant == DfButtonVariant.secondary ||
                    variant == DfButtonVariant.ghost) &&
                hovered &&
                enabled
            ? t.surface3
            : bg;
        final body = Container(
          constraints: const BoxConstraints(minHeight: 40),
          padding: const EdgeInsets.symmetric(vertical: 8, horizontal: 15),
          alignment: Alignment.center,
          decoration: BoxDecoration(
            color: hoverBg,
            border: Border.all(color: border),
            borderRadius: BorderRadius.circular(9),
          ),
          child: DefaultTextStyle(
            style: TextStyle(
              fontSize: remPx(context, 0.83),
              fontWeight: FontWeight.w700,
              color: fg,
            ),
            child: IconTheme(
              data: IconThemeData(size: remPx(context, 1), color: fg),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  if (icon != null) ...[icon!, const SizedBox(width: 8)],
                  child,
                ],
              ),
            ),
          ),
        );
        final framed = FocusRing(
          visible: focused,
          radius: 9,
          child: Opacity(
            // `.button.primary:hover` drops to opacity 0.86.
            opacity: !enabled
                ? 0.35
                : (variant == DfButtonVariant.primary && hovered ? 0.86 : 1),
            child: body,
          ),
        );
        return framed;
      },
    );

    if (wide) {
      return Padding(
        padding: const EdgeInsets.only(top: 18),
        child: SizedBox(width: double.infinity, child: button),
      );
    }
    return button;
  }
}

/// A square icon-only button (`.icon-button`, 42px). [label] is both the
/// tooltip and the accessible name, as the React `title` and `aria-label` were.
class IconButton extends StatelessWidget {
  const IconButton({
    super.key,
    required this.label,
    required this.child,
    this.onPressed,
    this.focusNode,
  });

  final String label;
  final Widget child;
  final VoidCallback? onPressed;
  final FocusNode? focusNode;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final enabled = onPressed != null;
    return Tooltip(
      message: label,
      child: Pressable(
        onPressed: onPressed,
        label: label,
        focusNode: focusNode,
        builder: (context, hovered, focused) {
          return Opacity(
            opacity: enabled ? 1 : 0.35,
            child: FocusRing(
              visible: focused,
              radius: 7,
              child: Container(
                width: 42,
                height: 42,
                alignment: Alignment.center,
                decoration: BoxDecoration(
                  color: hovered && enabled ? t.surface3 : Colors.transparent,
                  borderRadius: BorderRadius.circular(7),
                ),
                child: IconTheme(
                  data: IconThemeData(size: remPx(context, 1.1), color: t.text),
                  child: child,
                ),
              ),
            ),
          );
        },
      ),
    );
  }
}
