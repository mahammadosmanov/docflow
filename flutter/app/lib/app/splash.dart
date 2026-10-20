// Startup views: the splash while bootstrap runs and the failure view when it
// cannot run (`Splash` and the fatal branch of `App` in App.tsx).

import 'package:flutter/material.dart';

import '../l10n/strings.dart';
import '../theme/app_icons.dart';
import '../theme/tokens.dart';

/// Brand mark plus spinner shown until bootstrap completes.
final class Splash extends StatelessWidget {
  const Splash({super.key});

  @override
  Widget build(BuildContext context) {
    final tokens = DocFlowTokens.of(context);
    final textTheme = Theme.of(context).textTheme;
    return ColoredBox(
      color: tokens.bg,
      child: Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          spacing: 12,
          children: [
            _BrandMark(tokens: tokens),
            Text(
              'DocFlow',
              style: textTheme.bodyLarge?.copyWith(
                color: tokens.text,
                fontWeight: FontWeight.w700,
              ),
            ),
            SpinningIcon(
              icon: AppIcons.loaderCircle,
              size: DocFlowMetrics.spinnerSize,
              color: tokens.muted,
            ),
          ],
        ),
      ),
    );
  }
}

/// The square brand mark (`.brand-mark`): inverse background, inverse glyph.
final class BrandMark extends StatelessWidget {
  const BrandMark({super.key, this.size = DocFlowMetrics.splashMark});

  final double size;

  @override
  Widget build(BuildContext context) =>
      _BrandMark(tokens: DocFlowTokens.of(context), size: size);
}

final class _BrandMark extends StatelessWidget {
  const _BrandMark({
    required this.tokens,
    this.size = DocFlowMetrics.splashMark,
  });

  final DocFlowTokens tokens;
  final double size;

  @override
  Widget build(BuildContext context) => Container(
    width: size,
    height: size,
    alignment: Alignment.center,
    decoration: BoxDecoration(
      color: tokens.inverse,
      borderRadius: BorderRadius.circular(13),
    ),
    child: Icon(
      AppIcons.files,
      size: DocFlowMetrics.brandMarkIcon,
      color: tokens.inverseText,
    ),
  );
}

/// An icon that rotates continuously (`.spin`: 900ms linear). It stops when the
/// platform asks for reduced motion (`prefers-reduced-motion`).
final class SpinningIcon extends StatefulWidget {
  const SpinningIcon({
    super.key,
    required this.icon,
    required this.size,
    required this.color,
  });

  final IconData icon;
  final double size;
  final Color color;

  @override
  State<SpinningIcon> createState() => _SpinningIconState();
}

final class _SpinningIconState extends State<SpinningIcon>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 900),
  );

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final reduce = MediaQuery.maybeDisableAnimationsOf(context) ?? false;
    if (reduce) {
      _controller.stop();
    } else if (!_controller.isAnimating) {
      _controller.repeat();
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => RotationTransition(
    turns: _controller,
    child: Icon(widget.icon, size: widget.size, color: widget.color),
  );
}

/// Fatal bootstrap state (`EmptyState` with the retry action). Shown when the
/// engine could not return the bootstrap payload.
final class BootstrapFailure extends StatelessWidget {
  const BootstrapFailure({
    super.key,
    required this.strings,
    required this.message,
    required this.onRetry,
  });

  final Strings strings;
  final String message;
  final VoidCallback onRetry;

  @override
  Widget build(BuildContext context) {
    final tokens = DocFlowTokens.of(context);
    final textTheme = Theme.of(context).textTheme;
    return ColoredBox(
      color: tokens.bg,
      child: Center(
        child: SingleChildScrollView(
          padding: const EdgeInsets.all(12),
          child: ConstrainedBox(
            constraints: const BoxConstraints(
              minHeight: DocFlowMetrics.emptyStateMinHeight,
            ),
            child: Padding(
              padding: const EdgeInsets.all(30),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Container(
                    width: DocFlowMetrics.emptyStateIconBox,
                    height: DocFlowMetrics.emptyStateIconBox,
                    alignment: Alignment.center,
                    decoration: BoxDecoration(
                      color: tokens.surface3,
                      borderRadius: BorderRadius.circular(12),
                    ),
                    child: Icon(
                      AppIcons.xCircle,
                      size: DocFlowMetrics.emptyStateIcon,
                      color: tokens.text,
                    ),
                  ),
                  const SizedBox(height: 15),
                  Text(
                    strings.t('bootstrap.fatalTitle'),
                    textAlign: TextAlign.center,
                    style: textTheme.titleMedium?.copyWith(color: tokens.text),
                  ),
                  const SizedBox(height: 8),
                  ConstrainedBox(
                    constraints: const BoxConstraints(maxWidth: 460),
                    child: Text(
                      message,
                      textAlign: TextAlign.center,
                      style: textTheme.bodyMedium?.copyWith(
                        color: tokens.muted,
                      ),
                    ),
                  ),
                  const SizedBox(height: 16),
                  FilledButton.icon(
                    style: FilledButton.styleFrom(
                      backgroundColor: tokens.inverse,
                      foregroundColor: tokens.inverseText,
                    ),
                    onPressed: onRetry,
                    icon: Icon(AppIcons.refreshCw, size: 18),
                    label: Text(strings.t('bootstrap.retry')),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
