// SettingsScreen, ported from `SettingsScreen` in App.tsx (the `settings`
// route). Account name, language, theme, font, font size, density, automation,
// support diagnostics and reset. Every preference change goes through the
// engine (`settings/update`) and the answer replaces the shell's preferences.

import 'dart:async';

import 'package:flutter/material.dart' hide IconButton, Switch;

import '../../app/human_error.dart';
import '../../app/routes.dart';
import '../../app/screen_registry.dart';
import '../../bridge/bridge.dart';
import '../../l10n/strings.dart';
import '../../models/auth.dart';
import '../../models/prefs.dart';
import '../../models/replica.dart';
import '../../models/support.dart';
import '../../theme/app_icons.dart';
import '../../theme/tokens.dart';
import '../widgets/metrics.dart';
import '../widgets/widgets.dart';
import '../widgets/spinner.dart';
import 'choice_grid.dart';
import 'diagnostics_saver.dart';
import 'screen_parts.dart';
import 'settings_group.dart';

/// How often the replica automation status is re-read while settings is open.
const Duration replicaStatusInterval = Duration(seconds: 5);

/// Registers the settings screen. [saver] writes downloaded diagnostics; see
/// diagnostics_saver.dart.
void registerSettingsScreen(
  ScreenRegistry registry, {
  DiagnosticsSaver saver = const FileDiagnosticsSaver(),
}) {
  registry.register(
    ScreenId.settings,
    (context) => SettingsScreen(
      bridge: context.bridge,
      auth: context.auth,
      prefs: context.prefs,
      s: context.strings,
      onPrefs: context.updatePrefs,
      onAuth: context.updateAuth,
      onBack: context.goHome,
      diagnosticsSaver: saver,
    ),
  );
}

class SettingsScreen extends StatefulWidget {
  const SettingsScreen({
    super.key,
    required this.bridge,
    required this.auth,
    required this.prefs,
    required this.s,
    required this.onPrefs,
    required this.onAuth,
    required this.onBack,
    this.diagnosticsSaver = const FileDiagnosticsSaver(),
  });

  final Bridge bridge;
  final AuthState auth;
  final AppPrefs prefs;
  final Strings s;
  final ValueChanged<AppPrefs> onPrefs;
  final ValueChanged<AuthState> onAuth;
  final VoidCallback onBack;
  final DiagnosticsSaver diagnosticsSaver;

  @override
  State<SettingsScreen> createState() => _SettingsScreenState();
}

class _SettingsScreenState extends State<SettingsScreen> {
  late final TextEditingController _username = TextEditingController(
    text: widget.auth.user?.username ?? '',
  );
  String _saved = '';
  String _error = '';

  SupportBundlePreview? _diagnostics;
  bool _diagnosticsBusy = false;
  String _diagnosticsStatus = '';

  ReplicaAutomationStatus? _replicaStatus;
  Timer? _replicaTimer;

  @override
  void initState() {
    super.initState();
    unawaited(_refreshReplicaStatus());
    _replicaTimer = Timer.periodic(
      replicaStatusInterval,
      (_) => unawaited(_refreshReplicaStatus()),
    );
  }

  @override
  void dispose() {
    _replicaTimer?.cancel();
    _username.dispose();
    super.dispose();
  }

  Strings get _s => widget.s;

  /// Re-reads the replica status. A failed read keeps the last status.
  Future<void> _refreshReplicaStatus() async {
    try {
      final status = await widget.bridge.call(
        FrontendCommands.settingsReplicaStatus,
      );
      if (mounted) setState(() => _replicaStatus = status);
    } catch (_) {
      // The preference stays usable when the read-only refresh is unavailable.
    }
  }

  Future<void> _update(AppPrefsPatch patch) async {
    try {
      final next = await widget.bridge.call(
        FrontendCommands.settingsUpdate,
        params: patch,
      );
      widget.onPrefs(next);
      unawaited(_refreshReplicaStatus());
    } catch (reason) {
      if (mounted) setState(() => _error = humanError(reason, _s));
    }
  }

  Future<void> _saveUsername() async {
    setState(() => _error = '');
    try {
      final auth = await widget.bridge.call(
        FrontendCommands.authSetUsername,
        params: (username: _username.text),
      );
      widget.onAuth(auth);
      if (mounted) setState(() => _saved = _s.t('prefs.account.saved'));
    } catch (reason) {
      if (mounted) setState(() => _error = humanError(reason, _s));
    }
  }

  Future<void> _previewDiagnostics() async {
    setState(() {
      _error = '';
      _diagnosticsStatus = '';
      _diagnosticsBusy = true;
    });
    try {
      final preview = await widget.bridge.call(
        FrontendCommands.supportDiagnostics,
      );
      if (mounted) setState(() => _diagnostics = preview);
    } catch (reason) {
      if (mounted) setState(() => _error = humanError(reason, _s));
    } finally {
      if (mounted) setState(() => _diagnosticsBusy = false);
    }
  }

  Future<void> _copyDiagnostics() async {
    final preview = _diagnostics;
    if (preview == null) return;
    setState(() {
      _error = '';
      _diagnosticsStatus = '';
      _diagnosticsBusy = true;
    });
    try {
      await widget.bridge.call(
        FrontendCommands.supportCopyDiagnostics,
        params: (content: preview.content),
      );
      if (mounted) {
        setState(() => _diagnosticsStatus = _s.t('prefs.support.copied'));
      }
    } catch (reason) {
      if (mounted) setState(() => _error = humanError(reason, _s));
    } finally {
      if (mounted) setState(() => _diagnosticsBusy = false);
    }
  }

  Future<void> _downloadDiagnostics() async {
    final preview = _diagnostics;
    if (preview == null) return;
    setState(() {
      _error = '';
      _diagnosticsStatus = '';
    });
    try {
      final path = await widget.diagnosticsSaver.save(preview);
      if (mounted) {
        setState(() {
          _diagnosticsStatus = '${_s.t('prefs.support.downloaded')} ($path)';
        });
      }
    } catch (reason) {
      if (mounted) setState(() => _error = humanError(reason, _s));
    }
  }

  Future<void> _reset() async {
    try {
      final next = await widget.bridge.call(FrontendCommands.settingsReset);
      widget.onPrefs(next);
    } catch (reason) {
      if (mounted) setState(() => _error = humanError(reason, _s));
    }
  }

  @override
  Widget build(BuildContext context) {
    final s = _s;
    final prefs = widget.prefs;
    final t = DocFlowTokens.of(context);
    final gap = SizedBox(height: 14 * t.spaceScale);
    final status = _replicaStatus;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Header(
          title: s.t('prefs.title'),
          onBack: widget.onBack,
          backLabel: s.t('action.back'),
        ),
        Expanded(
          child: PaneScroll(
            maxWidth: 1100,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                if (widget.auth.signedIn) ...[
                  SettingsGroup(
                    title: s.t('prefs.account'),
                    icon: AppIcons.user,
                    child: _accountContent(context),
                  ),
                  gap,
                ],
                SettingsGroup(
                  title: s.t('prefs.language'),
                  icon: AppIcons.badgeCheck,
                  child: ChoiceGrid(
                    value: prefs.locale?.name ?? 'system',
                    options: [
                      ('system', s.t('prefs.lang.system')),
                      ('tr', s.t('prefs.lang.tr')),
                      ('en', s.t('prefs.lang.en')),
                      ('ru', s.t('prefs.lang.ru')),
                      ('az', s.t('prefs.lang.az')),
                    ],
                    onChanged: (locale) => _update(
                      locale == 'system'
                          ? const AppPrefsPatch(clearLocale: true)
                          : AppPrefsPatch(locale: AppLocale.fromWire(locale)),
                    ),
                  ),
                ),
                gap,
                SettingsGroup(
                  title: s.t('prefs.theme'),
                  icon: AppIcons.slidersHorizontal,
                  child: Segmented(
                    value: prefs.theme.name,
                    options: [
                      for (final theme in ThemePreference.values)
                        (theme.name, s.t('prefs.theme.${theme.name}')),
                    ],
                    onChanged: (theme) => _update(
                      AppPrefsPatch(theme: ThemePreference.fromWire(theme)),
                    ),
                  ),
                ),
                gap,
                SettingsGroup(
                  title: s.t('prefs.font'),
                  icon: AppIcons.fileSearch,
                  help: s.t('prefs.font.help'),
                  child: ChoiceGrid(
                    value: prefs.font.name,
                    options: [
                      for (final font in FontPreference.values)
                        (font.name, s.t('prefs.font.${font.name}')),
                    ],
                    onChanged: (font) => _update(
                      AppPrefsPatch(font: FontPreference.fromWire(font)),
                    ),
                  ),
                ),
                gap,
                SettingsGroup(
                  title: s.t('prefs.fontSize'),
                  icon: AppIcons.slidersHorizontal,
                  child: _fontSizeContent(context),
                ),
                gap,
                SettingsGroup(
                  title: s.t('prefs.density'),
                  icon: AppIcons.slidersHorizontal,
                  help: s.t('prefs.density.help'),
                  child: _ToggleRow(
                    label: s.t('prefs.density.compact'),
                    checked: prefs.compact,
                    onChanged: (value) =>
                        _update(AppPrefsPatch(compact: value)),
                  ),
                ),
                gap,
                SettingsGroup(
                  title: s.t('prefs.automation'),
                  icon: AppIcons.monitor,
                  help: s.t('prefs.automation.help'),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      _ToggleRow(
                        label: s.t('prefs.startWithWindows'),
                        checked: prefs.startWithWindows,
                        onChanged: (value) =>
                            _update(AppPrefsPatch(startWithWindows: value)),
                      ),
                      const SizedBox(height: 12),
                      _ToggleRow(
                        label: s.t('prefs.replicaSyncEnabled'),
                        help: s.t('prefs.replicaSyncEnabled.help'),
                        extra: [
                          if (status != null)
                            s.t('prefs.replica.status.${status.status.wire}'),
                          if (status?.lastErrorCode != null)
                            s.t(status!.lastErrorCode!),
                        ],
                        checked: status?.enabled ?? prefs.replicaSyncEnabled,
                        onChanged: (value) =>
                            _update(AppPrefsPatch(replicaSyncEnabled: value)),
                      ),
                    ],
                  ),
                ),
                gap,
                SettingsGroup(
                  title: s.t('prefs.support.title'),
                  icon: AppIcons.shield,
                  help: s.t('prefs.support.help'),
                  child: _supportContent(context),
                ),
                if (_error.isNotEmpty) ...[
                  gap,
                  Notice(kind: NoticeKind.error, child: Text(_error)),
                ],
                gap,
                Align(
                  alignment: AlignmentDirectional.centerStart,
                  child: DfButton(
                    variant: DfButtonVariant.danger,
                    icon: Icon(AppIcons.rotateCcw),
                    onPressed: _reset,
                    child: Text(s.t('prefs.reset')),
                  ),
                ),
              ],
            ),
          ),
        ),
      ],
    );
  }

  Widget _accountContent(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final s = _s;
    final valid = usernamePattern.hasMatch(_username.text);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              widget.auth.session?.email ?? '',
              style: const TextStyle(fontWeight: FontWeight.w700),
            ),
            const SizedBox(height: 3),
            Text(
              s.t('prefs.account.emailFixed'),
              style: TextStyle(fontSize: remPx(context, 0.83), color: t.muted),
            ),
          ],
        ),
        const SizedBox(height: 9),
        _BareInput(
          controller: _username,
          hint: s.t('auth.username'),
          onChanged: (_) => setState(() => _saved = ''),
        ),
        const SizedBox(height: 9),
        Align(
          alignment: AlignmentDirectional.centerStart,
          child: DfButton(
            variant: DfButtonVariant.secondary,
            icon: Icon(AppIcons.save),
            onPressed: valid ? _saveUsername : null,
            child: Text(s.t('action.save')),
          ),
        ),
        if (_saved.isNotEmpty) ...[
          const SizedBox(height: 12),
          Notice(kind: NoticeKind.info, child: Text(_saved)),
        ],
      ],
    );
  }

  Widget _fontSizeContent(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final prefs = widget.prefs;
    final scale = prefs.fontScale.clamp(0.8, 1.6).toDouble();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            Text(
              '80%',
              style: TextStyle(fontSize: remPx(context, 0.72), color: t.muted),
            ),
            Expanded(
              child: SliderTheme(
                data: SliderThemeData(
                  activeTrackColor: t.inverse,
                  inactiveTrackColor: t.line,
                  thumbColor: t.inverse,
                  overlayColor: t.inverse.withValues(alpha: 0.12),
                ),
                child: Slider(
                  min: 0.8,
                  max: 1.6,
                  // step 0.05 over 0.8 to 1.6 is 16 divisions.
                  divisions: 16,
                  value: scale,
                  label: '${(scale * 100).round()}%',
                  onChanged: (value) => _update(
                    AppPrefsPatch(
                      fontScale: double.parse(value.toStringAsFixed(2)),
                    ),
                  ),
                ),
              ),
            ),
            const SizedBox(width: 10),
            Text(
              '${(prefs.fontScale * 100).round()}%',
              style: TextStyle(
                fontSize: remPx(context, 0.72),
                fontWeight: FontWeight.w700,
                color: t.muted,
              ),
            ),
          ],
        ),
        const SizedBox(height: 12),
        Container(
          padding: const EdgeInsets.all(13),
          decoration: BoxDecoration(
            color: t.surface2,
            border: Border.all(color: t.line),
            borderRadius: BorderRadius.circular(8),
          ),
          child: Text(
            _s.t('prefs.fontSize.sample'),
            style: TextStyle(
              fontSize: remPx(context, 1),
              height: 1.45,
              color: t.text,
            ),
          ),
        ),
      ],
    );
  }

  Widget _supportContent(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final s = _s;
    final preview = _diagnostics;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          s.t('prefs.support.privacy'),
          style: TextStyle(
            fontSize: remPx(context, 0.8),
            height: 1.5,
            color: t.muted,
          ),
        ),
        const SizedBox(height: 12),
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: [
            DfButton(
              variant: DfButtonVariant.secondary,
              icon: _diagnosticsBusy
                  ? Spinner(size: remPx(context, 1), color: t.text)
                  : Icon(AppIcons.eye),
              onPressed: _diagnosticsBusy ? null : _previewDiagnostics,
              child: Text(s.t('prefs.support.preview')),
            ),
            if (preview != null) ...[
              DfButton(
                variant: DfButtonVariant.secondary,
                icon: Icon(AppIcons.copy),
                onPressed: _diagnosticsBusy ? null : _copyDiagnostics,
                child: Text(s.t('prefs.support.copy')),
              ),
              DfButton(
                variant: DfButtonVariant.secondary,
                icon: Icon(AppIcons.download),
                onPressed: _diagnosticsBusy ? null : _downloadDiagnostics,
                child: Text(s.t('prefs.support.download')),
              ),
            ],
          ],
        ),
        if (preview != null) ...[
          const SizedBox(height: 12),
          Text(
            '${preview.fileName} · '
            '${s.t('prefs.support.size', {'count': preview.byteLength})}',
            style: TextStyle(fontSize: remPx(context, 0.83), color: t.muted),
          ),
          const SizedBox(height: 7),
          Semantics(
            label: s.t('prefs.support.previewLabel'),
            child: Container(
              width: double.infinity,
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(
                color: t.surface2,
                border: Border.all(color: t.line),
                borderRadius: BorderRadius.circular(8),
              ),
              child: SelectableText(
                preview.content,
                style: TextStyle(
                  fontFamily: 'monospace',
                  fontFamilyFallback: const [
                    'Menlo',
                    'Consolas',
                    'Courier New',
                  ],
                  fontSize: remPx(context, 0.75),
                  height: 1.5,
                  color: t.text,
                ),
              ),
            ),
          ),
        ],
        if (_diagnosticsStatus.isNotEmpty) ...[
          const SizedBox(height: 12),
          Notice(kind: NoticeKind.info, child: Text(_diagnosticsStatus)),
        ],
      ],
    );
  }
}

/// A `.toggle-row`: the label (with optional help and status lines) and a
/// switch. Tapping the label toggles the switch, as the React `<label>` did.
class _ToggleRow extends StatelessWidget {
  const _ToggleRow({
    required this.label,
    required this.checked,
    required this.onChanged,
    this.help,
    this.extra = const [],
  });

  final String label;
  final String? help;
  final List<String> extra;
  final bool checked;
  final ValueChanged<bool> onChanged;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final small = TextStyle(
      fontSize: remPx(context, 0.83),
      height: 1.4,
      fontWeight: FontWeight.w400,
      color: t.muted,
    );
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 3),
      child: Row(
        children: [
          Expanded(
            child: GestureDetector(
              behavior: HitTestBehavior.opaque,
              onTap: () => onChanged(!checked),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    label,
                    style: const TextStyle(fontWeight: FontWeight.w700),
                  ),
                  if (help != null) ...[
                    const SizedBox(height: 4),
                    Text(help!, style: small),
                  ],
                  for (final line in extra) ...[
                    const SizedBox(height: 4),
                    Text(line, style: small),
                  ],
                ],
              ),
            ),
          ),
          const SizedBox(width: 20),
          Switch(checked: checked, semanticsLabel: label, onChanged: onChanged),
        ],
      ),
    );
  }
}

/// The bare username input of the account group: a bordered box with a
/// placeholder and no label, as the `.settings-account input` rule drew it.
class _BareInput extends StatelessWidget {
  const _BareInput({
    required this.controller,
    required this.hint,
    required this.onChanged,
  });

  final TextEditingController controller;
  final String hint;
  final ValueChanged<String> onChanged;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return Semantics(
      textField: true,
      label: hint,
      child: Container(
        height: 38,
        padding: const EdgeInsets.symmetric(horizontal: 10),
        alignment: Alignment.centerLeft,
        decoration: BoxDecoration(
          color: t.surface,
          border: Border.all(color: t.line),
          borderRadius: BorderRadius.circular(9),
        ),
        child: TextField(
          controller: controller,
          onChanged: onChanged,
          cursorColor: t.text,
          style: TextStyle(fontSize: remPx(context, 0.875), color: t.text),
          decoration: InputDecoration.collapsed(
            hintText: hint,
            hintStyle: TextStyle(
              fontSize: remPx(context, 0.875),
              color: t.muted,
            ),
          ),
        ),
      ),
    );
  }
}
