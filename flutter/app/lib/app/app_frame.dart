// The signed-in shell around every workspace screen (`AppFrame` in App.tsx and
// the `.app-frame` rules in styles.css). Desktop and tablet widths get a side
// rail. Narrow phones get a bottom bar, as in the 620px CSS breakpoint.

import 'package:flutter/material.dart';

import '../bridge/bridge.dart';
import '../bridge/platform.dart';
import '../l10n/strings.dart';
import '../models/models.dart';
import '../theme/app_icons.dart';
import '../theme/tokens.dart';
import 'human_error.dart';
import 'routes.dart';

/// The shell chrome around a workspace screen.
final class AppFrame extends StatefulWidget {
  const AppFrame({
    super.key,
    required this.bridge,
    required this.routeName,
    required this.isAdmin,
    required this.user,
    required this.strings,
    required this.onNavigate,
    required this.child,
  });

  final Bridge bridge;

  /// The screen currently shown. Highlights its navigation entry.
  final ScreenId routeName;
  final bool isAdmin;
  final AppUser? user;
  final Strings strings;
  final void Function(AppRoute route) onNavigate;
  final Widget child;

  @override
  State<AppFrame> createState() => _AppFrameState();
}

final class _AppFrameState extends State<AppFrame> {
  String _reviewError = '';
  bool _openingReview = false;

  bool get _workspaceActive =>
      widget.routeName == ScreenId.home ||
      widget.routeName == ScreenId.operation;

  void _navigate(AppRoute route) {
    setState(() => _reviewError = '');
    widget.onNavigate(route);
  }

  /// Opens the folder picker for the review destination, then navigates to the
  /// review screen with the chosen folder (`openReview` in App.tsx).
  Future<void> _openReview() async {
    setState(() {
      _openingReview = true;
      _reviewError = '';
    });
    try {
      final destination = await chooseFolder(
        widget.bridge,
        'rfi.review',
        'destination',
      );
      if (destination != null) {
        _navigate(ReviewRoute(destination: destination));
      }
    } catch (error) {
      if (mounted) {
        setState(() => _reviewError = humanError(error, widget.strings));
      }
    } finally {
      if (mounted) setState(() => _openingReview = false);
    }
  }

  List<_NavEntry> _entries() {
    final s = widget.strings;
    return [
      _NavEntry(
        label: s.t('home.title'),
        icon: AppIcons.files,
        active: _workspaceActive,
        onTap: () => _navigate(const HomeRoute()),
        group: 0,
      ),
      _NavEntry(
        label: s.t('action.review'),
        icon: AppIcons.fileCheck,
        active: widget.routeName == ScreenId.review,
        enabled: !_openingReview,
        onTap: _openReview,
        group: 0,
      ),
      _NavEntry(
        label: s.t('action.history'),
        icon: AppIcons.history,
        active: widget.routeName == ScreenId.history,
        onTap: () => _navigate(const HistoryRoute()),
        group: 0,
      ),
      _NavEntry(
        label: s.t('recovery.title'),
        icon: AppIcons.rotateCcw,
        active: widget.routeName == ScreenId.recovery,
        onTap: () => _navigate(const RecoveryRoute()),
        group: 0,
      ),
      _NavEntry(
        label: s.t('prefs.title'),
        icon: AppIcons.settings,
        active: widget.routeName == ScreenId.settings,
        onTap: () => _navigate(const SettingsRoute(back: SettingsBack.home)),
        group: 1,
      ),
      if (widget.isAdmin)
        _NavEntry(
          label: s.t('action.admin'),
          icon: AppIcons.userRoundCog,
          active: widget.routeName == ScreenId.admin,
          onTap: () => _navigate(const AdminRoute()),
          group: 1,
        ),
    ];
  }

  @override
  Widget build(BuildContext context) {
    final width = MediaQuery.sizeOf(context).width;
    final tokens = DocFlowTokens.of(context);
    final entries = _entries();

    if (width <= DocFlowMetrics.breakpointBottomBar) {
      return _BottomBarLayout(
        tokens: tokens,
        strings: widget.strings,
        entries: entries,
        reviewError: _reviewError,
        child: widget.child,
      );
    }

    final compact = width <= DocFlowMetrics.breakpointRailCompact;
    final railWidth = compact
        ? DocFlowMetrics.railCompact
        : width <= DocFlowMetrics.breakpointRailMedium
        ? DocFlowMetrics.railMedium
        : DocFlowMetrics.railWide;

    return ColoredBox(
      color: tokens.bg,
      child: Row(
        children: [
          SizedBox(
            width: railWidth,
            child: _Rail(
              tokens: tokens,
              strings: widget.strings,
              compact: compact,
              entries: entries,
              user: widget.user,
              reviewError: _reviewError,
            ),
          ),
          Expanded(child: ClipRect(child: widget.child)),
        ],
      ),
    );
  }
}

final class _NavEntry {
  const _NavEntry({
    required this.label,
    required this.icon,
    required this.active,
    required this.onTap,
    required this.group,
    this.enabled = true,
  });

  final String label;
  final IconData icon;
  final bool active;
  final bool enabled;
  final void Function() onTap;

  /// 0 for workspace destinations, 1 for settings and admin (separated by a
  /// divider on the rail).
  final int group;
}

final class _Rail extends StatelessWidget {
  const _Rail({
    required this.tokens,
    required this.strings,
    required this.compact,
    required this.entries,
    required this.user,
    required this.reviewError,
  });

  final DocFlowTokens tokens;
  final Strings strings;
  final bool compact;
  final List<_NavEntry> entries;
  final AppUser? user;
  final String reviewError;

  @override
  Widget build(BuildContext context) {
    final padding = compact
        ? const EdgeInsets.fromLTRB(8, 16, 8, 12)
        : const EdgeInsets.fromLTRB(14, 22, 14, 16);
    final workspace = entries.where((e) => e.group == 0).toList();
    final settings = entries.where((e) => e.group == 1).toList();
    final profileName = _profileName(user);

    return DecoratedBox(
      decoration: BoxDecoration(
        color: tokens.surface,
        border: Border(right: BorderSide(color: tokens.line)),
      ),
      child: Padding(
        padding: padding,
        child: LayoutBuilder(
          builder: (context, constraints) => SingleChildScrollView(
            child: ConstrainedBox(
              constraints: BoxConstraints(minHeight: constraints.maxHeight),
              child: IntrinsicHeight(
                child: Column(
                  crossAxisAlignment: compact
                      ? CrossAxisAlignment.center
                      : CrossAxisAlignment.stretch,
                  children: [
                    _Brand(tokens: tokens, compact: compact),
                    const SizedBox(height: 24),
                    for (final entry in workspace)
                      _NavItemTile(
                        entry: entry,
                        tokens: tokens,
                        compact: compact,
                        strings: strings,
                      ),
                    const SizedBox(height: 16),
                    _Divider(tokens: tokens),
                    const SizedBox(height: 16),
                    for (final entry in settings)
                      _NavItemTile(
                        entry: entry,
                        tokens: tokens,
                        compact: compact,
                        strings: strings,
                      ),
                    if (reviewError.isNotEmpty) ...[
                      const SizedBox(height: 16),
                      _ErrorBox(tokens: tokens, message: reviewError),
                    ],
                    const Spacer(),
                    _Profile(
                      tokens: tokens,
                      compact: compact,
                      name: profileName,
                      email: user?.email ?? '',
                    ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

final class _BottomBarLayout extends StatelessWidget {
  const _BottomBarLayout({
    required this.tokens,
    required this.strings,
    required this.entries,
    required this.reviewError,
    required this.child,
  });

  final DocFlowTokens tokens;
  final Strings strings;
  final List<_NavEntry> entries;
  final String reviewError;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final safeBottom = MediaQuery.paddingOf(context).bottom;
    final barHeight = DocFlowMetrics.bottomBarHeight + safeBottom;
    return ColoredBox(
      color: tokens.bg,
      child: Stack(
        children: [
          Positioned.fill(
            child: Padding(
              padding: EdgeInsets.only(bottom: barHeight),
              child: ClipRect(child: child),
            ),
          ),
          Positioned(
            left: 0,
            right: 0,
            bottom: 0,
            height: barHeight,
            child: DecoratedBox(
              decoration: BoxDecoration(
                color: tokens.surface,
                border: Border(top: BorderSide(color: tokens.line)),
              ),
              child: Padding(
                padding: EdgeInsets.fromLTRB(8, 4, 8, 4 + safeBottom),
                child: Center(
                  child: ConstrainedBox(
                    constraints: const BoxConstraints(maxWidth: 460),
                    child: Row(
                      mainAxisAlignment: MainAxisAlignment.spaceAround,
                      children: [
                        for (final entry in entries)
                          _NavItemTile(
                            entry: entry,
                            tokens: tokens,
                            compact: true,
                            strings: strings,
                          ),
                      ],
                    ),
                  ),
                ),
              ),
            ),
          ),
          if (reviewError.isNotEmpty)
            Positioned(
              left: 12,
              right: 12,
              bottom: barHeight + 10,
              child: _ErrorBox(tokens: tokens, message: reviewError),
            ),
        ],
      ),
    );
  }
}

final class _Brand extends StatelessWidget {
  const _Brand({required this.tokens, required this.compact});

  final DocFlowTokens tokens;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final mark = Container(
      width: DocFlowMetrics.brandMark,
      height: DocFlowMetrics.brandMark,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: tokens.inverse,
        borderRadius: BorderRadius.circular(7),
      ),
      child: Icon(
        AppIcons.files,
        size: DocFlowMetrics.brandMarkIcon,
        color: tokens.inverseText,
      ),
    );
    if (compact) return mark;
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 6),
      child: Row(
        children: [
          mark,
          const SizedBox(width: 11),
          Text(
            'DocFlow',
            style: Theme.of(context).textTheme.titleLarge
                ?.copyWith(color: tokens.text),
          ),
        ],
      ),
    );
  }
}

final class _Divider extends StatelessWidget {
  const _Divider({required this.tokens});

  final DocFlowTokens tokens;

  @override
  Widget build(BuildContext context) => Container(
    height: 1,
    margin: const EdgeInsets.symmetric(horizontal: 8),
    color: tokens.line,
  );
}

final class _ErrorBox extends StatelessWidget {
  const _ErrorBox({required this.tokens, required this.message});

  final DocFlowTokens tokens;
  final String message;

  @override
  Widget build(BuildContext context) => Semantics(
    liveRegion: true,
    child: Container(
      padding: const EdgeInsets.all(10),
      decoration: BoxDecoration(
        color: tokens.surface,
        border: Border.all(color: tokens.lineStrong),
        borderRadius: BorderRadius.circular(tokens.radius),
      ),
      child: Text(
        message,
        style: Theme.of(context).textTheme.bodyMedium
            ?.copyWith(color: tokens.danger, height: 1.45),
      ),
    ),
  );
}

final class _Profile extends StatelessWidget {
  const _Profile({
    required this.tokens,
    required this.compact,
    required this.name,
    required this.email,
  });

  final DocFlowTokens tokens;
  final bool compact;
  final String name;
  final String email;

  @override
  Widget build(BuildContext context) {
    final initial = name.isEmpty ? '' : name.substring(0, 1).toUpperCase();
    final textTheme = Theme.of(context).textTheme;
    final avatar = Container(
      width: DocFlowMetrics.profileAvatar,
      height: DocFlowMetrics.profileAvatar,
      alignment: Alignment.center,
      decoration: BoxDecoration(color: tokens.surface2, shape: BoxShape.circle),
      child: Text(
        initial,
        style: textTheme.bodyMedium?.copyWith(
          color: tokens.text,
          fontWeight: FontWeight.w700,
        ),
      ),
    );
    return Container(
      margin: const EdgeInsets.only(top: 12),
      padding: compact
          ? EdgeInsets.zero
          : const EdgeInsets.fromLTRB(5, 12, 5, 0),
      decoration: compact
          ? null
          : BoxDecoration(
              border: Border(top: BorderSide(color: tokens.line)),
            ),
      child: compact
          ? Center(child: avatar)
          : Row(
              children: [
                avatar,
                const SizedBox(width: 9),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    spacing: 3,
                    children: [
                      Text(
                        name,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: textTheme.bodyMedium?.copyWith(
                          color: tokens.text,
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                      Text(
                        email,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: textTheme.bodySmall?.copyWith(
                          color: tokens.muted,
                        ),
                      ),
                    ],
                  ),
                ),
              ],
            ),
    );
  }
}

final class _NavItemTile extends StatelessWidget {
  const _NavItemTile({
    required this.entry,
    required this.tokens,
    required this.compact,
    required this.strings,
  });

  final _NavEntry entry;
  final DocFlowTokens tokens;
  final bool compact;
  final Strings strings;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final active = entry.active;
    final color = active ? tokens.text : tokens.muted;
    final labelStyle =
        (active ? theme.textTheme.labelLarge : theme.textTheme.bodyMedium)
            ?.copyWith(color: color);
    final iconWidget = Icon(
      entry.icon,
      size: DocFlowMetrics.navIcon,
      color: color,
    );
    final content = Row(
      mainAxisAlignment: compact
          ? MainAxisAlignment.center
          : MainAxisAlignment.start,
      children: [
        iconWidget,
        if (!compact) ...[
          const SizedBox(width: 11),
          Flexible(
            child: Text(
              entry.label,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: labelStyle,
            ),
          ),
        ],
      ],
    );

    return Tooltip(
      message: entry.label,
      child: Semantics(
        button: true,
        enabled: entry.enabled,
        selected: active,
        label: entry.label,
        child: Opacity(
          opacity: entry.enabled ? 1 : 0.58,
          child: Material(
            type: MaterialType.transparency,
            child: InkWell(
              borderRadius: BorderRadius.circular(tokens.radius - 1),
              hoverColor: tokens.surface2,
              onTap: entry.enabled ? () => entry.onTap() : null,
              child: Container(
                width: compact ? DocFlowMetrics.navItemCompactSize : null,
                constraints: const BoxConstraints(
                  minHeight: DocFlowMetrics.navItemMinHeight,
                ),
                padding: EdgeInsets.symmetric(horizontal: compact ? 0 : 11),
                alignment: compact ? Alignment.center : Alignment.centerLeft,
                decoration: BoxDecoration(
                  color: active ? tokens.surface2 : null,
                  borderRadius: BorderRadius.circular(tokens.radius - 1),
                  border: active && !compact
                      ? Border(left: BorderSide(color: tokens.text, width: 2))
                      : null,
                ),
                child: content,
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// `profileName` in App.tsx: display name, else email.
String _profileName(AppUser? user) {
  if (user == null) return '';
  return user.display.isNotEmpty ? user.display : user.email;
}
