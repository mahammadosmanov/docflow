// PresetToolbar from frontend/src/App.tsx: lists, applies, saves and deletes
// the named parameter sets of an operation. Presets are stored by the engine;
// this widget only asks it through the bridge.

import 'package:flutter/material.dart' hide IconButton, Switch;

import '../../app/human_error.dart';
import '../../bridge/bridge.dart';
import '../../l10n/strings.dart';
import '../../models/models.dart';
import '../../theme/app_icons.dart';
import '../../theme/tokens.dart';
import '../widgets/metrics.dart';
import '../widgets/widgets.dart';
import 'form_controls.dart';
import 'operation_catalog.dart';

/// `presetCommandError`: a settings write failure has its own message.
String presetCommandError(Object error, Strings s) {
  if (asBridgeError(error).code == 'settings_write_failed') {
    return s.t('error.preset.write');
  }
  return humanError(error, s);
}

/// The browser's `window.confirm`, as a modal with cancel and [confirmLabel].
Future<bool> confirmPresetAction(
  BuildContext context, {
  required Strings s,
  required String message,
  required String confirmLabel,
}) async {
  final confirmed = await showDialog<bool>(
    context: context,
    builder: (dialogContext) => AlertDialog(
      content: Text(message),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(dialogContext).pop(false),
          child: Text(s.t('action.cancel')),
        ),
        TextButton(
          onPressed: () => Navigator.of(dialogContext).pop(true),
          child: Text(confirmLabel),
        ),
      ],
    ),
  );
  return confirmed ?? false;
}

/// The preset toolbar of an operation form (`.preset-toolbar`).
class PresetToolbar extends StatefulWidget {
  const PresetToolbar({
    super.key,
    required this.bridge,
    required this.operation,
    required this.values,
    required this.s,
    required this.onApply,
    required this.onError,
  });

  final Bridge bridge;
  final OperationDef operation;

  /// The form values the toolbar saves from.
  final Map<String, Object?> values;
  final Strings s;

  /// Receives the preset to apply to the form.
  final ValueChanged<OperationPreset> onApply;

  /// Receives the error message shown on the screen ('' clears it).
  final ValueChanged<String> onError;

  @override
  State<PresetToolbar> createState() => _PresetToolbarState();
}

class _PresetToolbarState extends State<PresetToolbar> {
  final TextEditingController _name = TextEditingController();
  List<OperationPreset> _presets = const [];
  String _selectedId = '';
  bool _loading = true;
  bool _busy = false;
  String _status = '';

  @override
  void initState() {
    super.initState();
    if (widget.operation.supportsPresets) _loadPresets();
  }

  @override
  void dispose() {
    _name.dispose();
    super.dispose();
  }

  Future<List<OperationPreset>?> _loadPresets() async {
    try {
      final next = await widget.bridge.call(
        FrontendCommands.settingsPresets,
        params: (opId: widget.operation.id),
      );
      if (mounted) setState(() => _presets = next);
      return next;
    } catch (reason) {
      if (mounted) widget.onError(presetCommandError(reason, widget.s));
      return null;
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  OperationPreset? get _selected {
    for (final preset in _presets) {
      if (preset.id == _selectedId) return preset;
    }
    return null;
  }

  void _applyPreset() {
    final selected = _selected;
    if (selected == null) return;
    widget.onApply(
      OperationPreset(
        id: selected.id,
        name: selected.name,
        values: presetCapableValues(widget.operation, selected.values),
      ),
    );
  }

  Future<void> _savePreset() async {
    final s = widget.s;
    final trimmedName = _name.text.trim();
    if (trimmedName.isEmpty || _busy) return;
    setState(() {
      _busy = true;
      _status = '';
    });
    widget.onError('');
    try {
      final presetValues = presetCapableValues(widget.operation, widget.values);
      OperationPreset saved;
      try {
        saved = await widget.bridge.call(
          FrontendCommands.settingsSavePreset,
          params: (
            opId: widget.operation.id,
            name: trimmedName,
            values: presetValues,
            replace: null,
          ),
        );
      } catch (reason) {
        if (!isDuplicatePresetError(reason)) throw reason;
        final confirmed =
            mounted &&
            await confirmPresetAction(
              context,
              s: s,
              message: s.t('preset.confirm.replace', {'name': trimmedName}),
              confirmLabel: s.t('preset.replace'),
            );
        if (!confirmed) throw reason;
        saved = await widget.bridge.call(
          FrontendCommands.settingsSavePreset,
          params: (
            opId: widget.operation.id,
            name: trimmedName,
            values: presetValues,
            replace: true,
          ),
        );
      }
      final next = await _loadPresets();
      if (next != null && mounted) {
        setState(() {
          _selectedId = saved.id;
          _name.clear();
          _status = s.t('preset.saved', {'name': saved.name});
        });
      }
    } catch (reason) {
      if (mounted) widget.onError(presetCommandError(reason, s));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _deletePreset() async {
    final s = widget.s;
    final selected = _selected;
    if (selected == null || _busy) return;
    final confirmed = await confirmPresetAction(
      context,
      s: s,
      message: s.t('preset.confirm.delete', {'name': selected.name}),
      confirmLabel: s.t('preset.delete'),
    );
    if (!confirmed || !mounted) return;
    setState(() {
      _busy = true;
      _status = '';
    });
    widget.onError('');
    try {
      await widget.bridge.call(
        FrontendCommands.settingsDeletePreset,
        params: (opId: widget.operation.id, presetId: selected.id),
      );
      final next = await _loadPresets();
      if (next != null && mounted) {
        setState(() {
          _selectedId = '';
          _status = s.t('preset.deleted', {'name': selected.name});
        });
      }
    } catch (reason) {
      if (mounted) widget.onError(presetCommandError(reason, s));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final s = widget.s;
    final selected = _selected;
    final presetItems = <DropdownMenuItem<String>>[
      DropdownMenuItem<String>(
        value: '',
        child: Text(s.t('preset.select.placeholder')),
      ),
      for (final preset in _presets)
        DropdownMenuItem<String>(value: preset.id, child: Text(preset.name)),
    ];
    final presetIds = {for (final preset in _presets) preset.id};
    final selectedValue = _selectedId.isEmpty || presetIds.contains(_selectedId)
        ? _selectedId
        : '';

    return Semantics(
      container: true,
      label: s.t('preset.title'),
      child: Container(
        padding: const EdgeInsets.all(14),
        decoration: BoxDecoration(
          color: t.surface2,
          border: Border.all(color: t.line),
          borderRadius: BorderRadius.circular(t.radius),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              crossAxisAlignment: CrossAxisAlignment.baseline,
              textBaseline: TextBaseline.alphabetic,
              children: [
                Text(
                  s.t('preset.title'),
                  style: TextStyle(
                    fontSize: remPx(context, 1.0),
                    fontWeight: FontWeight.w700,
                    color: t.text,
                  ),
                ),
                if (!_loading && _presets.isEmpty) ...[
                  const SizedBox(width: 10),
                  Flexible(
                    child: Text(
                      s.t('preset.empty'),
                      style: TextStyle(
                        fontSize: remPx(context, 0.8),
                        color: t.muted,
                      ),
                    ),
                  ),
                ],
              ],
            ),
            const SizedBox(height: 10),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              crossAxisAlignment: WrapCrossAlignment.end,
              children: [
                SizedBox(
                  width: 260,
                  child: SelectField<String>(
                    label: s.t('preset.select'),
                    value: selectedValue,
                    items: presetItems,
                    enabled: !_loading && !_busy,
                    onChanged: (value) =>
                        setState(() => _selectedId = value ?? ''),
                  ),
                ),
                DfButton(
                  variant: DfButtonVariant.secondary,
                  onPressed: selected == null || _busy ? null : _applyPreset,
                  child: Text(s.t('preset.apply')),
                ),
                DfButton(
                  variant: DfButtonVariant.danger,
                  icon: Icon(AppIcons.trash, size: remPx(context, 0.95)),
                  onPressed: selected == null || _busy ? null : _deletePreset,
                  child: Text(s.t('preset.delete')),
                ),
              ],
            ),
            const SizedBox(height: 10),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              crossAxisAlignment: WrapCrossAlignment.end,
              children: [
                SizedBox(
                  width: 260,
                  child: Field(
                    label: s.t('preset.name'),
                    controller: _name,
                    enabled: !_busy,
                    onChanged: (_) => setState(() {}),
                  ),
                ),
                DfButton(
                  icon: _busy
                      ? Icon(AppIcons.loaderCircle, size: remPx(context, 0.95))
                      : Icon(AppIcons.save, size: remPx(context, 0.95)),
                  onPressed: _name.text.trim().isEmpty || _busy
                      ? null
                      : _savePreset,
                  child: Text(s.t('preset.save')),
                ),
              ],
            ),
            if (_status.isNotEmpty) ...[
              const SizedBox(height: 8),
              Text(
                _status,
                style: TextStyle(fontSize: remPx(context, 0.8), color: t.muted),
              ),
            ],
          ],
        ),
      ),
    );
  }
}
