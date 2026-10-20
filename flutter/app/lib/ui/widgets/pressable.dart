// Internal interaction surface shared by the widgets in this library. A tap, or
// Enter and Space while focused, activates the control. It can take keyboard
// focus, and it reports hover and keyboard-focus state so the visuals can match
// the React `:hover` and `:focus-visible` rules.

import 'package:flutter/material.dart' hide IconButton, Switch;

import '../../theme/tokens.dart';

typedef PressableBuilder = Widget Function(
  BuildContext context,
  bool hovered,
  bool focused,
);

/// A focusable, tappable region with button-like semantics.
class Pressable extends StatefulWidget {
  const Pressable({
    super.key,
    required this.builder,
    this.onPressed,
    this.label,
    this.value,
    this.button = true,
    this.selected,
    this.toggled,
    this.focusNode,
  });

  final PressableBuilder builder;

  /// Null disables the control: it is not tappable, focusable or announced
  /// as enabled.
  final VoidCallback? onPressed;
  final String? label;
  final String? value;
  final bool button;
  final bool? selected;
  final bool? toggled;
  final FocusNode? focusNode;

  @override
  State<Pressable> createState() => _PressableState();
}

class _PressableState extends State<Pressable> {
  bool _hovered = false;
  bool _focused = false;

  void _activate() => widget.onPressed?.call();

  @override
  Widget build(BuildContext context) {
    final enabled = widget.onPressed != null;
    return MergeSemantics(
      child: Semantics(
        button: widget.button,
        enabled: enabled,
        selected: widget.selected,
        toggled: widget.toggled,
        label: widget.label,
        value: widget.value,
        child: FocusableActionDetector(
          focusNode: widget.focusNode,
          enabled: enabled,
          mouseCursor: enabled
              ? SystemMouseCursors.click
              : SystemMouseCursors.basic,
          onShowHoverHighlight: (value) => setState(() => _hovered = value),
          onShowFocusHighlight: (value) => setState(() => _focused = value),
          actions: <Type, Action<Intent>>{
            ActivateIntent: CallbackAction<ActivateIntent>(
              onInvoke: (_) {
                _activate();
                return null;
              },
            ),
          },
          child: GestureDetector(
            behavior: HitTestBehavior.opaque,
            onTap: enabled ? _activate : null,
            child: widget.builder(context, _hovered, _focused),
          ),
        ),
      ),
    );
  }
}

/// The `:focus-visible` outline: 2px in the text colour, offset 2px outside
/// the box. Shown only while [visible] is true.
class FocusRing extends StatelessWidget {
  const FocusRing({
    super.key,
    required this.visible,
    required this.radius,
    required this.child,
  });

  final bool visible;
  final double radius;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    if (!visible) return child;
    final color = DocFlowTokens.of(context).text;
    return Stack(
      clipBehavior: Clip.none,
      children: [
        child,
        Positioned(
          left: -4,
          top: -4,
          right: -4,
          bottom: -4,
          child: IgnorePointer(
            child: DecoratedBox(
              decoration: BoxDecoration(
                border: Border.all(color: color, width: 2),
                borderRadius: BorderRadius.circular(radius + 4),
              ),
            ),
          ),
        ),
      ],
    );
  }
}
