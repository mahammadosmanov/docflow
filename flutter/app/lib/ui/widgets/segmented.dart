// Segmented, ported from `Segmented` in ui.tsx and the `.segmented` rules.

import 'package:flutter/material.dart' hide IconButton, Switch;

import '../../theme/tokens.dart';
import 'metrics.dart';
import 'pressable.dart';

/// A row of mutually exclusive options. [options] are (key, label) pairs.
class Segmented extends StatelessWidget {
  const Segmented({
    super.key,
    required this.value,
    required this.options,
    required this.onChanged,
    this.small = false,
  });

  final String value;
  final List<(String, String)> options;
  final ValueChanged<String> onChanged;
  final bool small;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return Container(
      padding: const EdgeInsets.all(3),
      decoration: BoxDecoration(
        color: t.surface3,
        borderRadius: BorderRadius.circular(10),
      ),
      child: Row(
        children: [
          for (final (key, label) in options)
            Expanded(
              child: _SegmentOption(
                label: label,
                selected: key == value,
                small: small,
                onPressed: () => onChanged(key),
              ),
            ),
        ],
      ),
    );
  }
}

class _SegmentOption extends StatelessWidget {
  const _SegmentOption({
    required this.label,
    required this.selected,
    required this.small,
    required this.onPressed,
  });

  final String label;
  final bool selected;
  final bool small;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return Pressable(
      selected: selected,
      label: label,
      onPressed: onPressed,
      builder: (context, hovered, focused) {
        return FocusRing(
          visible: focused,
          radius: 7,
          child: Container(
            constraints: BoxConstraints(minHeight: small ? 31 : 35),
            padding: small
                ? const EdgeInsets.symmetric(vertical: 5, horizontal: 9)
                : const EdgeInsets.symmetric(vertical: 7, horizontal: 10),
            alignment: Alignment.center,
            decoration: BoxDecoration(
              color: selected ? t.surface : Colors.transparent,
              borderRadius: BorderRadius.circular(7),
              boxShadow: selected
                  ? const [
                      BoxShadow(
                        color: Color(0x24000000),
                        offset: Offset(0, 1),
                        blurRadius: 5,
                      ),
                    ]
                  : null,
            ),
            child: ExcludeSemantics(
              child: Text(
                label,
                textAlign: TextAlign.center,
                style: TextStyle(
                  fontSize: remPx(context, small ? 0.7 : 0.8),
                  height: 1.3,
                  fontWeight: selected ? FontWeight.w700 : FontWeight.w400,
                  color: t.text,
                ),
              ),
            ),
          ),
        );
      },
    );
  }
}
