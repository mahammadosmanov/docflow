// AdminScreen, ported from `AdminScreen` in App.tsx (the `admin` route). Lists
// every account and lets an administrator switch access and admin rights. The
// administrator's own switches are locked.

import 'package:flutter/material.dart' hide IconButton, Switch;

import '../../app/human_error.dart';
import '../../app/routes.dart';
import '../../app/screen_registry.dart';
import '../../bridge/bridge.dart';
import '../../l10n/strings.dart';
import '../../models/auth.dart';
import '../../theme/app_icons.dart';
import '../../theme/tokens.dart';
import '../widgets/metrics.dart';
import '../widgets/widgets.dart';
import 'screen_parts.dart';

/// Registers the admin screen (`ScreenId.admin`).
void registerAdminScreen(ScreenRegistry registry) {
  registry.register(
    ScreenId.admin,
    (context) => AdminScreen(
      bridge: context.bridge,
      auth: context.auth,
      s: context.strings,
      onBack: context.goHome,
    ),
  );
}

class AdminScreen extends StatefulWidget {
  const AdminScreen({
    super.key,
    required this.bridge,
    required this.auth,
    required this.s,
    required this.onBack,
  });

  final Bridge bridge;
  final AuthState auth;
  final Strings s;
  final VoidCallback onBack;

  @override
  State<AdminScreen> createState() => _AdminScreenState();
}

class _AdminScreenState extends State<AdminScreen> {
  List<AppUser> _users = const [];

  /// The account whose switches are being saved, or empty.
  String _busyId = '';
  String _error = '';

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    try {
      final users = await widget.bridge.call(FrontendCommands.authListUsers);
      if (mounted) setState(() => _users = users);
    } catch (reason) {
      if (mounted) setState(() => _error = humanError(reason, widget.s));
    }
  }

  Future<void> _update(AppUser user, {bool? allowed, bool? isAdmin}) async {
    setState(() => _busyId = user.id);
    try {
      await widget.bridge.call(
        FrontendCommands.authSetFlags,
        params: (userId: user.id, allowed: allowed, isAdmin: isAdmin),
      );
      await _load();
    } catch (reason) {
      if (mounted) setState(() => _error = humanError(reason, widget.s));
    } finally {
      if (mounted) setState(() => _busyId = '');
    }
  }

  @override
  Widget build(BuildContext context) {
    final s = widget.s;
    final t = DocFlowTokens.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Header(
          title: s.t('admin.title'),
          onBack: widget.onBack,
          backLabel: s.t('action.back'),
          trailing: IconButton(
            label: s.t('action.retry'),
            onPressed: _load,
            child: Icon(AppIcons.refreshCw),
          ),
        ),
        Expanded(
          child: PaneScroll(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                PageHeading(
                  title: s.t('admin.title'),
                  help: s.t('admin.help'),
                  compact: true,
                  trailing: SafeBadge(
                    icon: AppIcons.shield,
                    label: s.t('admin.admin'),
                  ),
                ),
                if (_users.isNotEmpty)
                  Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      for (final user in _users)
                        Padding(
                          padding: const EdgeInsets.only(bottom: 8),
                          child: _AdminRow(
                            user: user,
                            self: user.id == widget.auth.session?.userId,
                            busy: _busyId == user.id,
                            s: s,
                            onAllowed: (value) => _update(user, allowed: value),
                            onAdmin: (value) => _update(user, isAdmin: value),
                          ),
                        ),
                    ],
                  )
                else
                  EmptyState(
                    icon: Icon(AppIcons.userRoundCog),
                    title: s.t('admin.empty'),
                  ),
                if (_error.isNotEmpty)
                  Padding(
                    padding: EdgeInsets.only(top: 14 * t.spaceScale),
                    child: Notice(kind: NoticeKind.error, child: Text(_error)),
                  ),
              ],
            ),
          ),
        ),
      ],
    );
  }
}

/// One `.admin-row`: avatar, name, email and the two switches.
class _AdminRow extends StatelessWidget {
  const _AdminRow({
    required this.user,
    required this.self,
    required this.busy,
    required this.s,
    required this.onAllowed,
    required this.onAdmin,
  });

  final AppUser user;
  final bool self;
  final bool busy;
  final Strings s;
  final ValueChanged<bool> onAllowed;
  final ValueChanged<bool> onAdmin;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final disabled = self || busy;
    final initial = user.display.isEmpty
        ? ''
        : user.display.substring(0, 1).toUpperCase();
    final small = TextStyle(
      fontSize: remPx(context, 0.83),
      height: 1.4,
      color: t.muted,
    );
    final copy = Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        Wrap(
          spacing: 8,
          runSpacing: 4,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: [
            Text(
              user.display,
              style: const TextStyle(fontWeight: FontWeight.w700),
            ),
            if (self)
              Container(
                padding: const EdgeInsets.symmetric(vertical: 2, horizontal: 6),
                decoration: BoxDecoration(
                  color: t.surface3,
                  borderRadius: BorderRadius.circular(99),
                ),
                child: Text(
                  s.t('admin.you'),
                  style: TextStyle(
                    fontSize: remPx(context, 0.65),
                    color: t.muted,
                  ),
                ),
              ),
          ],
        ),
        const SizedBox(height: 4),
        Text(
          user.allowed
              ? user.email
              : '${user.email} · ${s.t('admin.noAccess')}',
          style: small,
        ),
      ],
    );
    final avatar = Container(
      width: 42,
      height: 42,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: t.surface3,
        borderRadius: BorderRadius.circular(10),
      ),
      child: ExcludeSemantics(
        child: Text(
          initial,
          style: const TextStyle(fontWeight: FontWeight.w700),
        ),
      ),
    );
    final toggles = Wrap(
      spacing: 16,
      runSpacing: 8,
      crossAxisAlignment: WrapCrossAlignment.center,
      children: [
        _MiniToggle(
          label: s.t('admin.allowed'),
          checked: user.allowed,
          disabled: disabled,
          onChanged: onAllowed,
        ),
        _MiniToggle(
          label: s.t('admin.admin'),
          checked: user.isAdmin,
          disabled: disabled,
          onChanged: onAdmin,
        ),
      ],
    );
    final narrow = MediaQuery.sizeOf(context).width <= narrowPaneBreakpoint;

    return Container(
      padding: EdgeInsets.all(14 * t.spaceScale),
      decoration: BoxDecoration(
        color: t.surface,
        borderRadius: BorderRadius.circular(10),
        boxShadow: t.panelShadow,
      ),
      child: narrow
          ? Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  crossAxisAlignment: CrossAxisAlignment.center,
                  children: [
                    avatar,
                    const SizedBox(width: 12),
                    Expanded(child: copy),
                  ],
                ),
                const SizedBox(height: 10),
                toggles,
              ],
            )
          : Row(
              children: [
                avatar,
                const SizedBox(width: 12),
                Expanded(child: copy),
                const SizedBox(width: 12),
                toggles,
              ],
            ),
    );
  }
}

/// `.mini-toggle`: a small caption beside a switch.
class _MiniToggle extends StatelessWidget {
  const _MiniToggle({
    required this.label,
    required this.checked,
    required this.disabled,
    required this.onChanged,
  });

  final String label;
  final bool checked;
  final bool disabled;
  final ValueChanged<bool> onChanged;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Text(
          label,
          style: TextStyle(fontSize: remPx(context, 0.72), color: t.muted),
        ),
        const SizedBox(width: 8),
        Switch(
          checked: checked,
          disabled: disabled,
          semanticsLabel: label,
          onChanged: onChanged,
        ),
      ],
    );
  }
}
