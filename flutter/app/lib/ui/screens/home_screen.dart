// HomeScreen from frontend/src/App.tsx: the operation catalogue with search,
// category filters and the promoted scan operation. Registered under
// ScreenId.home.

import 'dart:async';

import 'package:flutter/material.dart' hide IconButton, Switch;
import 'package:flutter/services.dart';

import '../../app/screen_registry.dart';
import '../../app/routes.dart';
import '../../bridge/bridge.dart';
import '../../l10n/strings.dart';
import '../../models/models.dart';
import '../../theme/app_icons.dart';
import '../../theme/tokens.dart';
import '../../theme/typography.dart';
import '../widgets/metrics.dart';
import '../widgets/widgets.dart';
import 'operation_catalog.dart';

/// Registers the Home screen with the shell (`ScreenId.home`).
void registerHomeScreen(ScreenRegistry registry) {
  registry.register(
    ScreenId.home,
    (screen) => HomeScreen(screen: screen),
  );
}

/// The signed-in workspace home: a searchable, filterable list of operations.
class HomeScreen extends StatefulWidget {
  const HomeScreen({super.key, required this.screen});

  final ScreenContext screen;

  @override
  State<HomeScreen> createState() => _HomeScreenState();
}

class _HomeScreenState extends State<HomeScreen> {
  final TextEditingController _search = TextEditingController();
  String _query = '';
  OperationCategory _category = OperationCategory.all;

  @override
  void initState() {
    super.initState();
    HardwareKeyboard.instance.addHandler(_onKeyEvent);
  }

  @override
  void dispose() {
    HardwareKeyboard.instance.removeHandler(_onKeyEvent);
    _search.dispose();
    super.dispose();
  }

  /// Alt+T cycles the theme, as the window-level listener in App.tsx did.
  bool _onKeyEvent(KeyEvent event) {
    if (event is KeyUpEvent || !mounted) return false;
    if (event.logicalKey == LogicalKeyboardKey.keyT &&
        HardwareKeyboard.instance.isAltPressed) {
      unawaited(_toggleTheme());
      return true;
    }
    return false;
  }

  Future<void> _toggleTheme() async {
    final screen = widget.screen;
    final current = screen.prefs.theme;
    final next = switch (current) {
      ThemePreference.system => ThemePreference.light,
      ThemePreference.light => ThemePreference.dark,
      ThemePreference.dark => ThemePreference.system,
    };
    try {
      final updated = await screen.bridge.call(
        FrontendCommands.settingsUpdate,
        params: AppPrefsPatch(theme: next),
      );
      if (mounted) widget.screen.updatePrefs(updated);
    } catch (_) {
      // Ignore background failure, as the React handler does.
    }
  }

  Future<void> _signOut() async {
    final auth = await widget.screen.bridge.call(FrontendCommands.authSignOut);
    if (mounted) widget.screen.updateAuth(auth);
  }

  List<OperationDef> _filtered(List<OperationDef> operations, Strings s) {
    final query = _query.trim().toLowerCase();
    return [
      for (final operation in operations)
        if ((_category == OperationCategory.all ||
                operationCategoryOf(operation.id) == _category) &&
            (query.isEmpty ||
                s.t(operation.titleKey).toLowerCase().contains(query) ||
                s.t(operation.descriptionKey).toLowerCase().contains(query) ||
                operation.id.toLowerCase().contains(query)))
          operation,
    ];
  }

  void _clearFilters() {
    _search.clear();
    setState(() {
      _query = '';
      _category = OperationCategory.all;
    });
  }

  void _open(OperationDef operation) {
    widget.screen.navigate(OperationRoute(operation: operation));
  }

  IconData _themeIcon(ThemePreference theme) => switch (theme) {
    ThemePreference.dark => AppIcons.moon,
    ThemePreference.light => AppIcons.sun,
    ThemePreference.system => AppIcons.monitor,
  };

  @override
  Widget build(BuildContext context) {
    final screen = widget.screen;
    final s = screen.strings;
    final bootstrap = screen.bootstrap;
    final operations = bootstrap.operations;
    final filtered = _filtered(operations, s);
    final primaryScan = operations
        .where((operation) => operation.id == primaryScanOperationId)
        .firstOrNull;
    final promotePrimaryScan =
        _category == OperationCategory.all && _query.trim().isEmpty;
    final listed = promotePrimaryScan && primaryScan != null
        ? [
            for (final operation in filtered)
              if (operation.id != primaryScan.id) operation,
          ]
        : filtered;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Header(
          trailing: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              IconButton(
                label: s.t('action.toggleTheme'),
                onPressed: () => unawaited(_toggleTheme()),
                child: Icon(_themeIcon(screen.prefs.theme)),
              ),
              IconButton(
                label: s.t('auth.signOut'),
                onPressed: () => unawaited(_signOut()),
                child: const Icon(AppIcons.logOut),
              ),
            ],
          ),
        ),
        Expanded(
          child: SingleChildScrollView(
            padding: const EdgeInsets.all(16),
            child: Center(
              child: ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 960),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    _PageHeading(
                      s: s,
                      version: bootstrap.version,
                    ),
                    const SizedBox(height: 18),
                    if (primaryScan != null && promotePrimaryScan) ...[
                      _PromotedCard(
                        title: s.t(primaryScan.titleKey),
                        description: s.t(primaryScan.descriptionKey),
                        icon: operationIconOf(primaryScan.id),
                        onTap: () => _open(primaryScan),
                      ),
                      const SizedBox(height: 18),
                    ],
                    _SectionHeading(
                      s: s,
                      shown: filtered.length,
                      total: operations.length,
                    ),
                    const SizedBox(height: 12),
                    _SearchBar(
                      s: s,
                      controller: _search,
                      query: _query,
                      onChanged: (value) => setState(() => _query = value),
                      onClear: () {
                        _search.clear();
                        setState(() => _query = '');
                      },
                    ),
                    const SizedBox(height: 10),
                    _CategoryChips(
                      s: s,
                      active: _category,
                      counts: {
                        for (final category in OperationCategory.values)
                          category: category == OperationCategory.all
                              ? operations.length
                              : operations
                                    .where(
                                      (operation) =>
                                          operationCategoryOf(operation.id) ==
                                          category,
                                    )
                                    .length,
                      },
                      onSelected: (category) =>
                          setState(() => _category = category),
                    ),
                    const SizedBox(height: 12),
                    if (filtered.isEmpty)
                      _EmptySearch(
                        s: s,
                        query: _query,
                        onReset: _clearFilters,
                      )
                    else
                      for (final operation in listed)
                        Padding(
                          padding: const EdgeInsets.only(bottom: 8),
                          child: _OperationCard(
                            title: s.t(operation.titleKey),
                            category: s.t(
                              'ops.category.${operationCategoryOf(operation.id).name}',
                            ),
                            description: s.t(operation.descriptionKey),
                            icon: operationIconOf(operation.id),
                            onTap: () => _open(operation),
                          ),
                        ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ],
    );
  }
}

/// The eyebrow, title, subtitle and the "preview first" badge.
class _PageHeading extends StatelessWidget {
  const _PageHeading({required this.s, required this.version});

  final Strings s;
  final String version;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return Wrap(
      alignment: WrapAlignment.spaceBetween,
      crossAxisAlignment: WrapCrossAlignment.end,
      spacing: 16,
      runSpacing: 12,
      children: [
        Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'DOCFLOW',
              style: TextStyle(
                fontSize: remPx(context, DocFlowTypeScale.captionRem),
                fontWeight: FontWeight.w700,
                letterSpacing: 1.2,
                color: t.muted,
              ),
            ),
            const SizedBox(height: 4),
            Text(
              s.t('home.title'),
              style: TextStyle(
                fontSize: remPx(context, DocFlowTypeScale.h1Rem),
                fontWeight: FontWeight.w700,
                height: DocFlowTypeScale.h1Height,
                letterSpacing:
                    DocFlowTypeScale.h1LetterSpacingEm *
                    remPx(context, DocFlowTypeScale.h1Rem),
                color: t.text,
              ),
            ),
            const SizedBox(height: 6),
            Text(
              s.t('home.subtitle'),
              style: TextStyle(
                fontSize: remPx(context, DocFlowTypeScale.bodyRem),
                color: t.muted,
              ),
            ),
          ],
        ),
        Container(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
          decoration: BoxDecoration(
            border: Border.all(color: t.line),
            borderRadius: BorderRadius.circular(t.radius),
          ),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(
                AppIcons.shield,
                size: remPx(context, 0.9),
                color: t.muted,
              ),
              const SizedBox(width: 6),
              Text(
                s.t('home.previewFirst'),
                style: TextStyle(
                  fontSize: remPx(context, DocFlowTypeScale.controlRem),
                  color: t.text,
                ),
              ),
              Text(
                '  ·  ',
                style: TextStyle(color: t.muted),
              ),
              Text(
                'v$version',
                style: TextStyle(
                  fontSize: remPx(context, DocFlowTypeScale.controlRem),
                  color: t.text,
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

/// The section title with the shown / total count.
class _SectionHeading extends StatelessWidget {
  const _SectionHeading({
    required this.s,
    required this.shown,
    required this.total,
  });

  final Strings s;
  final int shown;
  final int total;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return Row(
      crossAxisAlignment: CrossAxisAlignment.end,
      children: [
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                s.t('ops.heading'),
                style: TextStyle(
                  fontSize: remPx(context, DocFlowTypeScale.h2Rem),
                  fontWeight: FontWeight.w700,
                  color: t.text,
                ),
              ),
              const SizedBox(height: 2),
              Text(
                s.t('ops.subheading'),
                style: TextStyle(
                  fontSize: remPx(context, DocFlowTypeScale.captionRem),
                  color: t.muted,
                ),
              ),
            ],
          ),
        ),
        Text(
          '$shown / $total',
          style: TextStyle(
            fontSize: remPx(context, DocFlowTypeScale.captionRem),
            color: t.muted,
          ),
        ),
      ],
    );
  }
}

/// The promoted scan operation shown above the catalogue.
class _PromotedCard extends StatelessWidget {
  const _PromotedCard({
    required this.title,
    required this.description,
    required this.icon,
    required this.onTap,
  });

  final String title;
  final String description;
  final IconData icon;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => _CardSurface(
    onTap: onTap,
    padding: const EdgeInsets.all(16),
    child: Row(
      children: [
        Icon(icon, size: remPx(context, 1.4), color: DocFlowTokens.of(context).text),
        const SizedBox(width: 14),
        Expanded(
          child: _CardCopy(
            title: title,
            description: description,
          ),
        ),
        Icon(AppIcons.chevronRight, color: DocFlowTokens.of(context).muted),
      ],
    ),
  );
}

/// One operation row of the catalogue.
class _OperationCard extends StatelessWidget {
  const _OperationCard({
    required this.title,
    required this.category,
    required this.description,
    required this.icon,
    required this.onTap,
  });

  final String title;
  final String category;
  final String description;
  final IconData icon;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return _CardSurface(
      onTap: onTap,
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
      child: Row(
        children: [
          Icon(icon, size: remPx(context, 1.2), color: t.text),
          const SizedBox(width: 12),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Flexible(
                      child: Text(
                        title,
                        style: TextStyle(
                          fontSize: remPx(context, DocFlowTypeScale.bodyRem),
                          fontWeight: FontWeight.w700,
                          color: t.text,
                        ),
                      ),
                    ),
                    const SizedBox(width: 8),
                    _CategoryTag(label: category),
                  ],
                ),
                const SizedBox(height: 3),
                Text(
                  description,
                  style: TextStyle(
                    fontSize: remPx(context, DocFlowTypeScale.captionRem),
                    height: 1.45,
                    color: t.muted,
                  ),
                ),
              ],
            ),
          ),
          Icon(AppIcons.chevronRight, color: t.muted),
        ],
      ),
    );
  }
}

class _CardCopy extends StatelessWidget {
  const _CardCopy({required this.title, required this.description});

  final String title;
  final String description;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          title,
          style: TextStyle(
            fontSize: remPx(context, DocFlowTypeScale.bodyRem),
            fontWeight: FontWeight.w700,
            color: t.text,
          ),
        ),
        const SizedBox(height: 3),
        Text(
          description,
          style: TextStyle(
            fontSize: remPx(context, DocFlowTypeScale.captionRem),
            height: 1.45,
            color: t.muted,
          ),
        ),
      ],
    );
  }
}

/// A bordered, tappable surface (`.operation-card`, `.primary-scan-card`).
class _CardSurface extends StatelessWidget {
  const _CardSurface({
    required this.onTap,
    required this.padding,
    required this.child,
  });

  final VoidCallback onTap;
  final EdgeInsetsGeometry padding;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return Material(
      color: t.surface,
      borderRadius: BorderRadius.circular(t.radius),
      child: InkWell(
        borderRadius: BorderRadius.circular(t.radius),
        onTap: onTap,
        child: Container(
          width: double.infinity,
          padding: padding,
          decoration: BoxDecoration(
            border: Border.all(color: t.line),
            borderRadius: BorderRadius.circular(t.radius),
            boxShadow: t.panelShadow,
          ),
          child: child,
        ),
      ),
    );
  }
}

class _CategoryTag extends StatelessWidget {
  const _CategoryTag({required this.label});

  final String label;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
      decoration: BoxDecoration(
        border: Border.all(color: t.line),
        borderRadius: BorderRadius.circular(999),
      ),
      child: Text(
        label,
        style: TextStyle(
          fontSize: remPx(context, 0.72),
          fontWeight: FontWeight.w600,
          color: t.muted,
        ),
      ),
    );
  }
}

/// The search field with its clear button (`.operation-search-bar`).
class _SearchBar extends StatelessWidget {
  const _SearchBar({
    required this.s,
    required this.controller,
    required this.query,
    required this.onChanged,
    required this.onClear,
  });

  final Strings s;
  final TextEditingController controller;
  final String query;
  final ValueChanged<String> onChanged;
  final VoidCallback onClear;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final placeholder = s.t('ops.search.placeholder');
    return Container(
      height: 42,
      padding: const EdgeInsets.symmetric(horizontal: 10),
      decoration: BoxDecoration(
        color: t.surface,
        border: Border.all(color: t.line),
        borderRadius: BorderRadius.circular(9),
      ),
      child: Row(
        children: [
          Icon(AppIcons.fileSearch, size: remPx(context, 1.05), color: t.muted),
          const SizedBox(width: 8),
          Expanded(
            child: Semantics(
              label: placeholder,
              textField: true,
              child: TextField(
                controller: controller,
                onChanged: onChanged,
                cursorColor: t.text,
                style: TextStyle(
                  fontSize: remPx(context, DocFlowTypeScale.controlRem),
                  color: t.text,
                ),
                decoration: InputDecoration.collapsed(
                  hintText: placeholder,
                  hintStyle: TextStyle(color: t.muted),
                ),
              ),
            ),
          ),
          if (query.isNotEmpty)
            IconButton(
              label: s.t('action.cancel'),
              onPressed: onClear,
              child: const Icon(AppIcons.close),
            ),
        ],
      ),
    );
  }
}

/// The category filter chips with their counts (`.category-chip`).
class _CategoryChips extends StatelessWidget {
  const _CategoryChips({
    required this.s,
    required this.active,
    required this.counts,
    required this.onSelected,
  });

  final Strings s;
  final OperationCategory active;
  final Map<OperationCategory, int> counts;
  final ValueChanged<OperationCategory> onSelected;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return Wrap(
      spacing: 8,
      runSpacing: 8,
      children: [
        for (final category in OperationCategory.values)
          Semantics(
            button: true,
            selected: category == active,
            label: s.t('ops.category.${category.name}'),
            child: InkWell(
              borderRadius: BorderRadius.circular(999),
              onTap: () => onSelected(category),
              child: Container(
                padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
                decoration: BoxDecoration(
                  color: category == active ? t.inverse : t.surface,
                  border: Border.all(
                    color: category == active ? t.inverse : t.line,
                  ),
                  borderRadius: BorderRadius.circular(999),
                ),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Text(
                      s.t('ops.category.${category.name}'),
                      style: TextStyle(
                        fontSize: remPx(context, DocFlowTypeScale.captionRem),
                        fontWeight: FontWeight.w600,
                        color: category == active ? t.inverseText : t.text,
                      ),
                    ),
                    const SizedBox(width: 6),
                    Text(
                      '${counts[category] ?? 0}',
                      style: TextStyle(
                        fontSize: remPx(context, 0.72),
                        color: category == active ? t.inverseText : t.muted,
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
      ],
    );
  }
}

/// Shown when the filters match nothing (`.empty-search-state`).
class _EmptySearch extends StatelessWidget {
  const _EmptySearch({
    required this.s,
    required this.query,
    required this.onReset,
  });

  final Strings s;
  final String query;
  final VoidCallback onReset;

  @override
  Widget build(BuildContext context) => EmptyState(
    icon: Icon(AppIcons.circleAlert, size: remPx(context, 1.6)),
    title: s.t('ops.emptySearch', {'query': query}),
    action: DfButton(
      variant: DfButtonVariant.secondary,
      onPressed: onReset,
      child: Text(s.t('action.cancel')),
    ),
  );
}
