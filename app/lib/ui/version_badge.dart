import 'package:flutter/material.dart';

import '../core/app_version.dart';

class AppVersionBadge extends StatelessWidget {
  const AppVersionBadge({super.key});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return IgnorePointer(
      child: SafeArea(
        minimum: const EdgeInsets.only(left: 8, bottom: 6),
        child: DecoratedBox(
          decoration: BoxDecoration(
            color: theme.colorScheme.surface.withValues(alpha: 0.88),
            borderRadius: BorderRadius.circular(5),
            border: Border.all(
              color: theme.colorScheme.outlineVariant.withValues(alpha: 0.7),
            ),
          ),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 3),
            child: Text(
              'v$docFlowVersion',
              key: const ValueKey('app-version'),
              style: theme.textTheme.labelSmall?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
                fontSize: 10,
                height: 1,
                fontFeatures: const [FontFeature.tabularFigures()],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
