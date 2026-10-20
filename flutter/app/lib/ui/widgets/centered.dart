// Centered, ported from `Centered` in ui.tsx and the `.centered` rule. The
// content is centred on both axes and scrolls when it does not fit.

import 'dart:math' as math;

import 'package:flutter/material.dart' hide IconButton, Switch;

class Centered extends StatelessWidget {
  const Centered({super.key, required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final minHeight = constraints.hasBoundedHeight
            ? math.max(0.0, constraints.maxHeight - 24)
            : 0.0;
        return SingleChildScrollView(
          padding: const EdgeInsets.all(12),
          child: ConstrainedBox(
            constraints: BoxConstraints(minHeight: minHeight),
            child: Center(child: child),
          ),
        );
      },
    );
  }
}
