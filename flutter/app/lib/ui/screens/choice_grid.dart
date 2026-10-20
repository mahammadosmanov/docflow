// ChoiceGrid, ported from `ChoiceGrid` in App.tsx and the `.choice-grid`
// rules in styles.css: a two-column grid of selectable options. The selected
// option carries a check mark, a stronger border and a surface-3 fill.

import 'package:flutter/material.dart' hide IconButton, Switch;

import '../../theme/app_icons.dart';
import '../../theme/tokens.dart';
import '../widgets/metrics.dart';
import '../widgets/pressable.dart';

class ChoiceGrid extends StatelessWidget {
  const ChoiceGrid({
    super.key,
    required this.value,
    required this.options,
    required this.onChanged,
  });

  /// The key of the selected option.
  final String value;

  /// (key, label) pairs, in display order.
  final List<(String, String)> options;

  final ValueChanged<String> onChanged;

  @override
  Widget build(BuildContext context) {
    final rows = <List<(String, String)>>[];
    for (var i = 0; i < options.length; i += 2) {
      rows.add(options.sublist(i, (i + 2).clamp(0, options.length)));
    }
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        for (var r = 0; r < rows.length; r++) ...[
          if (r > 0) const SizedBox(height: 7),
          Row(
            children: [
              for (var c = 0; c < 2; c++) ...[
                if (c > 0) const SizedBox(width: 7),
                Expanded(
                  child: c < rows[r].length
                      ? _ChoiceButton(
                          label: rows[r][c].$2,
                          selected: rows[r][c].$1 == value,
                          onPressed: () => onChanged(rows[r][c].$1),
                        )
                      : const SizedBox.shrink(),
                ),
              ],
            ],
          ),
        ],
      ],
    );
  }
}

class _ChoiceButton extends StatelessWidget {
  const _ChoiceButton({
    required this.label,
    required this.selected,
    required this.onPressed,
  });

  final String label;
  final bool selected;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return Pressable(
      label: label,
      selected: selected,
      onPressed: onPressed,
      builder: (context, hovered, focused) {
        final fill = selected ? t.surface3 : t.surface2;
        return FocusRing(
          visible: focused,
          radius: 8,
          child: Container(
            constraints: const BoxConstraints(minHeight: 40),
            padding: const EdgeInsets.symmetric(vertical: 8, horizontal: 10),
            decoration: BoxDecoration(
              color: hovered && !selected ? t.surface3 : fill,
              borderRadius: BorderRadius.circular(8),
              border: Border.all(
                color: selected ? t.lineStrong : Colors.transparent,
              ),
            ),
            foregroundDecoration: selected
                ? BoxDecoration(
                    // `inset 0 0 0 1px color-mix(text 32%)`.
                    borderRadius: BorderRadius.circular(7),
                    border: Border.all(color: t.text.withValues(alpha: 0.32)),
                  )
                : null,
            child: Row(
              children: [
                SizedBox(
                  width: 16,
                  child: selected
                      ? Icon(
                          AppIcons.check,
                          size: remPx(context, 0.85),
                          color: t.text,
                        )
                      : null,
                ),
                const SizedBox(width: 8),
                Expanded(
                  child: ExcludeSemantics(
                    child: Text(
                      label,
                      style: TextStyle(
                        fontSize: remPx(context, 0.875),
                        height: 1.3,
                        fontWeight: selected
                            ? FontWeight.w600
                            : FontWeight.w400,
                        color: t.text,
                      ),
                    ),
                  ),
                ),
              ],
            ),
          ),
        );
      },
    );
  }
}
