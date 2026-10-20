// Switch, ported from `Switch` in ui.tsx and the `.switch` rules.

import 'package:flutter/material.dart' hide IconButton, Switch;

import '../../theme/tokens.dart';
import 'pressable.dart';

/// An on/off control with switch semantics (`role="switch"`).
class Switch extends StatelessWidget {
  const Switch({
    super.key,
    required this.checked,
    required this.onChanged,
    this.disabled = false,
    this.semanticsLabel,
    this.focusNode,
  });

  final bool checked;
  final ValueChanged<bool> onChanged;
  final bool disabled;

  /// The React `ariaLabel`.
  final String? semanticsLabel;
  final FocusNode? focusNode;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final enabled = !disabled;
    return Pressable(
      button: false,
      toggled: checked,
      label: semanticsLabel,
      focusNode: focusNode,
      onPressed: enabled ? () => onChanged(!checked) : null,
      builder: (context, hovered, focused) {
        return Opacity(
          opacity: enabled ? 1 : 0.35,
          child: FocusRing(
            visible: focused,
            radius: 999,
            child: AnimatedContainer(
              duration: const Duration(milliseconds: 130),
              width: 40,
              height: 23,
              decoration: BoxDecoration(
                color: checked ? t.inverse : t.surface3,
                border: Border.all(color: checked ? t.inverse : t.lineStrong),
                borderRadius: BorderRadius.circular(999),
              ),
              child: Stack(
                children: [
                  AnimatedPositioned(
                    duration: const Duration(milliseconds: 130),
                    left: checked ? 20 : 3,
                    top: 3,
                    child: Container(
                      width: 17,
                      height: 17,
                      decoration: BoxDecoration(
                        shape: BoxShape.circle,
                        color: checked ? t.inverseText : t.muted,
                      ),
                    ),
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
