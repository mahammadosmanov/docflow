// PendingScreen, ported from `PendingScreen` in App.tsx. Shown to a signed-in
// account that has not been allowed yet. It offers settings, sign-out and a
// retry that reloads the account state.

import 'package:flutter/material.dart' hide IconButton, Switch;

import '../../app/human_error.dart';
import '../../app/routes.dart';
import '../../app/screen_registry.dart';
import '../../bridge/bridge.dart';
import '../../l10n/strings.dart';
import '../../models/auth.dart';
import '../../theme/app_icons.dart';
import '../widgets/widgets.dart';
import 'screen_parts.dart';

/// Registers the pending screen (signed in, but not yet allowed).
void registerPendingScreen(ScreenRegistry registry) {
  registry.register(
    ScreenId.pending,
    (context) => PendingScreen(
      bridge: context.bridge,
      auth: context.auth,
      s: context.strings,
      onAuth: context.updateAuth,
      onSettings: () =>
          context.navigate(const SettingsRoute(back: SettingsBack.pending)),
    ),
  );
}

class PendingScreen extends StatefulWidget {
  const PendingScreen({
    super.key,
    required this.bridge,
    required this.auth,
    required this.s,
    required this.onAuth,
    required this.onSettings,
  });

  final Bridge bridge;
  final AuthState auth;
  final Strings s;
  final ValueChanged<AuthState> onAuth;
  final VoidCallback onSettings;

  @override
  State<PendingScreen> createState() => _PendingScreenState();
}

class _PendingScreenState extends State<PendingScreen> {
  bool _busy = false;
  String _error = '';

  /// Runs an account request and hands its result to [PendingScreen.onAuth].
  Future<void> _run(Future<AuthState> Function() request) async {
    setState(() {
      _busy = true;
      _error = '';
    });
    try {
      widget.onAuth(await request());
    } catch (reason) {
      if (mounted) setState(() => _error = humanError(reason, widget.s));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final s = widget.s;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Header(
          title: s.t('app.title'),
          trailing: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              IconButton(
                label: s.t('prefs.title'),
                onPressed: widget.onSettings,
                child: Icon(AppIcons.settings),
              ),
              IconButton(
                label: s.t('auth.signOut'),
                onPressed: () => _run(
                  () => widget.bridge.call(FrontendCommands.authSignOut),
                ),
                child: Icon(AppIcons.logOut),
              ),
            ],
          ),
        ),
        Expanded(
          child: Centered(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                EmptyState(
                  icon: Icon(AppIcons.shield),
                  title: s.t('auth.pending'),
                  message:
                      '${s.t('auth.pendingHelp')}\n'
                      '${widget.auth.session?.email ?? ''}',
                  action: DfButton(
                    onPressed: _busy
                        ? null
                        : () => _run(
                            () =>
                                widget.bridge.call(FrontendCommands.authLoadMe),
                          ),
                    icon: SpinningIcon(
                      icon: AppIcons.refreshCw,
                      spinning: _busy,
                    ),
                    child: Text(s.t('action.retry')),
                  ),
                ),
                if (_error.isNotEmpty)
                  Padding(
                    padding: const EdgeInsets.only(top: 12),
                    child: ConstrainedBox(
                      constraints: const BoxConstraints(maxWidth: 460),
                      child: Notice(
                        kind: NoticeKind.error,
                        child: Text(_error),
                      ),
                    ),
                  ),
              ],
            ),
          ),
        ),
      ],
    );
  }
}
