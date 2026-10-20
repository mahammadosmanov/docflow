// BusyState, ported from ui.tsx and the `.busy-state`, `.progress` and
// `.progress-trail` rules.

import 'package:flutter/material.dart' hide IconButton, Switch;

import '../../l10n/strings.dart';
import '../../models/progress.dart';
import '../../theme/app_icons.dart';
import '../../theme/tokens.dart';
import 'metrics.dart';
import 'spinner.dart';

/// A full-area "working" placeholder: spinner, heading, the current phase and
/// a bar, and an optional list of the stages seen so far.
class BusyState extends StatelessWidget {
  const BusyState({
    super.key,
    required this.label,
    required this.s,
    this.progress,
    this.progressTrail = const [],
  });

  final String label;
  final Strings s;
  final ProgressEvent? progress;
  final List<ProgressEvent> progressTrail;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final p = progress;
    // Matches `fraction ? `${fraction * 100}%` : "35%"`: a zero fraction
    // shows the fixed 35% bar.
    final fraction = p != null && p.total > 0 ? p.done / p.total : 0.0;
    final barFraction = fraction != 0 ? fraction : 0.35;

    final detail = p == null
        ? null
        : '${p.total > 0 ? '${p.done} / ${p.total}' : ''}'
              '${p.subject != null && p.subject!.isNotEmpty ? ' · ${p.subject}' : ''}';

    return ConstrainedBox(
      constraints: const BoxConstraints(minHeight: 300),
      child: Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Spinner(size: 28, color: t.text),
            const SizedBox(height: 16),
            Semantics(
              header: true,
              child: Text(
                label,
                textAlign: TextAlign.center,
                style: TextStyle(
                  fontSize: remPx(context, 1.05),
                  fontWeight: FontWeight.w700,
                  letterSpacing: -0.015 * remPx(context, 1.05),
                  color: t.text,
                ),
              ),
            ),
            const SizedBox(height: 16),
            if (p != null) ...[
              Text(
                s.t('progress.phase.${p.phase}'),
                style: TextStyle(
                  fontSize: remPx(context, 0.72),
                  fontWeight: FontWeight.w700,
                  color: t.text,
                ),
              ),
              const SizedBox(height: 9),
            ],
            _ProgressBar(fraction: barFraction),
            if (detail != null) ...[
              const SizedBox(height: 9),
              Text(
                detail,
                textAlign: TextAlign.center,
                style: TextStyle(
                  fontSize: remPx(context, 0.83),
                  color: t.muted,
                ),
              ),
            ],
            if (progressTrail.length > 1) ...[
              const SizedBox(height: 18),
              _ProgressTrail(
                events: progressTrail,
                label: s.t('progress.stages'),
                s: s,
              ),
            ],
          ],
        ),
      ),
    );
  }
}

class _ProgressBar extends StatelessWidget {
  const _ProgressBar({required this.fraction});

  final double fraction;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return ConstrainedBox(
      constraints: const BoxConstraints(maxWidth: 320),
      child: SizedBox(
        width: double.infinity,
        height: 5,
        child: DecoratedBox(
          decoration: BoxDecoration(
            color: t.surface3,
            borderRadius: BorderRadius.circular(99),
          ),
          child: ClipRRect(
            borderRadius: BorderRadius.circular(99),
            child: TweenAnimationBuilder<double>(
              tween: Tween<double>(begin: fraction, end: fraction),
              duration: const Duration(milliseconds: 180),
              builder: (context, value, _) => FractionallySizedBox(
                widthFactor: value.clamp(0.0, 1.0),
                alignment: Alignment.centerLeft,
                child: Container(height: 5, color: t.inverse),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _ProgressTrail extends StatelessWidget {
  const _ProgressTrail({
    required this.events,
    required this.label,
    required this.s,
  });

  final List<ProgressEvent> events;
  final String label;
  final Strings s;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return Semantics(
      container: true,
      explicitChildNodes: true,
      label: label,
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 420),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          mainAxisSize: MainAxisSize.min,
          children: [
            for (var i = 0; i < events.length; i++)
              Padding(
                padding: EdgeInsets.only(
                  bottom: i == events.length - 1 ? 0 : 5,
                ),
                child: _TrailItem(
                  event: events[i],
                  current: i == events.length - 1,
                  s: s,
                  color: t,
                ),
              ),
          ],
        ),
      ),
    );
  }
}

class _TrailItem extends StatelessWidget {
  const _TrailItem({
    required this.event,
    required this.current,
    required this.s,
    required this.color,
  });

  final ProgressEvent event;
  final bool current;
  final Strings s;
  final DocFlowTokens color;

  @override
  Widget build(BuildContext context) {
    final fg = current ? color.text : color.muted;
    final smallSize = remPx(context, 0.67) * 0.83;
    return Container(
      padding: const EdgeInsets.symmetric(vertical: 5, horizontal: 8),
      decoration: BoxDecoration(
        color: current ? color.surface3 : color.surface2,
        borderRadius: BorderRadius.circular(7),
      ),
      child: DefaultTextStyle(
        style: TextStyle(fontSize: remPx(context, 0.67), color: fg),
        child: Row(
          children: [
            ExcludeSemantics(
              child: current
                  ? Spinner(size: 13, color: fg)
                  : Icon(AppIcons.check, size: 13, color: fg),
            ),
            const SizedBox(width: 7),
            Expanded(child: Text(s.t('progress.phase.${event.phase}'))),
            if (event.total > 0) ...[
              const SizedBox(width: 7),
              Text(
                '${event.done}/${event.total}',
                style: TextStyle(fontSize: smallSize, color: fg),
              ),
            ],
          ],
        ),
      ),
    );
  }
}
