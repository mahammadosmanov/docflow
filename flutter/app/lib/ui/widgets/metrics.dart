// Sizing helpers shared by the widget library. The React styles are written in
// rem. The theme's body size already carries the font-size preference (see
// typography.dart), so one rem is the body size in logical pixels.

import 'package:flutter/material.dart' hide IconButton, Switch;

import '../../theme/typography.dart';

/// Converts a CSS rem length to logical pixels under the active theme.
double remPx(BuildContext context, double rem) {
  final root =
      Theme.of(context).textTheme.bodyLarge?.fontSize ??
      DocFlowTypeScale.rootPx;
  return rem * root;
}
