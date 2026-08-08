import 'package:flutter/material.dart';

import '../../core/l10n/strings.dart';
import '../../core/operations/models.dart';

class GuidedSessionPanel extends StatelessWidget {
  const GuidedSessionPanel({
    super.key,
    required this.session,
    required this.busy,
    required this.onComplete,
    required this.onStop,
    required this.onContinue,
    required this.onRestart,
    required this.onDone,
  });

  final GuidedSession session;
  final bool busy;
  final VoidCallback onComplete;
  final VoidCallback onStop;
  final VoidCallback onContinue;
  final VoidCallback onRestart;
  final VoidCallback onDone;

  @override
  Widget build(BuildContext context) {
    final strings = Strings.of(context);
    final theme = Theme.of(context);
    if (session.isComplete) {
      return Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Icon(Icons.task_alt_outlined),
            const SizedBox(height: 12),
            Text(
              strings.t('print.session.completed'),
              style: theme.textTheme.titleMedium?.copyWith(
                fontWeight: FontWeight.w700,
              ),
            ),
            const SizedBox(height: 16),
            FilledButton(
              key: const ValueKey('guided-done'),
              onPressed: busy ? null : onDone,
              child: Text(strings.t('action.done')),
            ),
          ],
        ),
      );
    }

    final job = session.currentJob ?? const <String, dynamic>{};
    final path = job['path']?.toString() ?? '';
    final name = path.split(RegExp(r'[/\\]')).last;
    final paper = job['paperSize']?.toString() ?? '';
    final rawCopies = job['copies'];
    final copies = rawCopies is num ? rawCopies.toInt() : 1;
    final paused = session.isPaused;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        LinearProgressIndicator(
          value: session.total == 0 ? 0 : session.current / session.total,
        ),
        const SizedBox(height: 8),
        Text(
          '${session.currentNumber}/${session.total}',
          textAlign: TextAlign.end,
          style: theme.textTheme.labelMedium,
        ),
        const SizedBox(height: 16),
        Card(
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Icon(
                      paused
                          ? Icons.pause_circle_outline
                          : Icons.print_outlined,
                    ),
                    const SizedBox(width: 10),
                    Expanded(
                      child: Text(
                        name,
                        style: theme.textTheme.titleMedium?.copyWith(
                          fontWeight: FontWeight.w700,
                        ),
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: 12),
                SelectableText(path, style: theme.textTheme.bodySmall),
                const SizedBox(height: 12),
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    Chip(
                      label: Text(
                        '${strings.t('print.session.paper')}: $paper',
                      ),
                    ),
                    Chip(
                      label: Text(
                        '${strings.t('print.session.copies')}: $copies',
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: 12),
                Text(
                  strings.t(
                    paused
                        ? 'print.session.paused'
                        : 'print.session.instruction',
                  ),
                  style: theme.textTheme.bodyMedium,
                ),
                if (session.lastError.isNotEmpty) ...[
                  const SizedBox(height: 8),
                  SelectableText(
                    session.lastError,
                    style: theme.textTheme.bodySmall?.copyWith(
                      fontWeight: FontWeight.w700,
                    ),
                  ),
                ],
              ],
            ),
          ),
        ),
        const Spacer(),
        if (paused)
          Wrap(
            alignment: WrapAlignment.end,
            spacing: 8,
            runSpacing: 8,
            children: [
              OutlinedButton.icon(
                key: const ValueKey('guided-restart'),
                onPressed: busy ? null : onRestart,
                icon: const Icon(Icons.replay_outlined),
                label: Text(strings.t('action.restartAll')),
              ),
              FilledButton.icon(
                key: const ValueKey('guided-continue'),
                onPressed: busy ? null : onContinue,
                icon: const Icon(Icons.play_arrow),
                label: Text(strings.t('action.continue')),
              ),
            ],
          )
        else
          Wrap(
            alignment: WrapAlignment.spaceBetween,
            spacing: 8,
            runSpacing: 8,
            children: [
              OutlinedButton.icon(
                key: const ValueKey('guided-stop'),
                onPressed: busy ? null : onStop,
                icon: const Icon(Icons.stop_circle_outlined),
                label: Text(strings.t('action.stop')),
              ),
              FilledButton.icon(
                key: const ValueKey('guided-complete'),
                onPressed: busy ? null : onComplete,
                icon: const Icon(Icons.navigate_next),
                label: Text(strings.t('action.printedNext')),
              ),
            ],
          ),
      ],
    );
  }
}
