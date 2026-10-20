// ActionBar, ported from `ActionBar` in ui.tsx and the `.action-bar` rules.

import 'package:flutter/material.dart' hide IconButton, Switch;

import '../../theme/tokens.dart';

/// The bottom bar of a flow: [left] actions on one side, [right] on the other.
/// Below the bottom-bar breakpoint the two groups stack, and each button
/// stretches to share the row.
class ActionBar extends StatelessWidget {
  const ActionBar({super.key, required this.left, required this.right});

  final List<Widget> left;
  final List<Widget> right;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final narrow =
        MediaQuery.sizeOf(context).width <= DocFlowMetrics.breakpointBottomBar;

    final content = narrow
        ? Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            mainAxisSize: MainAxisSize.min,
            children: [
              _Group(stretch: true, children: left),
              const SizedBox(height: 8),
              _Group(stretch: true, children: right),
            ],
          )
        : Row(
            children: [
              Flexible(child: _Group(children: left)),
              Flexible(
                child: Align(
                  alignment: AlignmentDirectional.centerEnd,
                  child: _Group(alignEnd: true, children: right),
                ),
              ),
            ],
          );

    return Container(
      constraints: const BoxConstraints(minHeight: 52),
      padding: const EdgeInsets.only(top: 10, bottom: 6),
      decoration: BoxDecoration(
        color: t.bg,
        border: Border(top: BorderSide(color: t.line)),
      ),
      child: content,
    );
  }
}

class _Group extends StatelessWidget {
  const _Group({
    required this.children,
    this.stretch = false,
    this.alignEnd = false,
  });

  final List<Widget> children;
  final bool stretch;
  final bool alignEnd;

  @override
  Widget build(BuildContext context) {
    if (stretch) {
      return Row(
        children: [
          for (var i = 0; i < children.length; i++) ...[
            if (i > 0) const SizedBox(width: 8),
            Expanded(child: children[i]),
          ],
        ],
      );
    }
    return Wrap(
      spacing: 8,
      runSpacing: 8,
      crossAxisAlignment: WrapCrossAlignment.center,
      alignment: alignEnd ? WrapAlignment.end : WrapAlignment.start,
      children: children,
    );
  }
}
