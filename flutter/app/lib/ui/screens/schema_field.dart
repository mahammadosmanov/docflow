// SchemaField from frontend/src/App.tsx: one generic form control for every
// ParamSchema kind. Forms are generated from the engine schema, so nothing in
// here is specific to an operation.

import 'dart:math' as math;

import 'package:desktop_drop/desktop_drop.dart';
import 'package:flutter/material.dart' hide IconButton, Switch;

import '../../app/app_utils.dart';
import '../../app/human_error.dart';
import '../../bridge/bridge.dart';
import '../../bridge/platform.dart';
import '../../l10n/strings.dart';
import '../../models/models.dart';
import '../../theme/app_icons.dart';
import '../../theme/tokens.dart';
import '../widgets/widgets.dart';
import '../widgets/metrics.dart';
import 'form_controls.dart';

/// Upper bound of a sheet count (`Math.min(100, ...)`).
const int sheetCountMax = 100;

/// Renders one parameter of an operation form (`SchemaField`).
class SchemaField extends StatefulWidget {
  const SchemaField({
    super.key,
    required this.bridge,
    required this.operationId,
    required this.param,
    required this.value,
    required this.values,
    required this.s,
    required this.onChange,
    required this.onError,
  });

  final Bridge bridge;
  final String operationId;
  final ParamDef param;
  final Object? value;

  /// Every value of the form, for `visibleWhen`.
  final Map<String, Object?> values;
  final Strings s;
  final ValueChanged<Object?> onChange;

  /// Receives a message when a file dialog or drop fails.
  final ValueChanged<String> onError;

  @override
  State<SchemaField> createState() => _SchemaFieldState();
}

/// `String(value ?? "")` as the browser showed it: whole numbers without a
/// fraction.
String _jsString(Object? value) {
  if (value == null) return '';
  if (value is num && value.isFinite && value == value.roundToDouble()) {
    return value.toInt().toString();
  }
  return value.toString();
}

/// The text the control shows for [value]: the display path for file and
/// folder parameters, the value itself otherwise.
String _displayFor(ParamDef param, Object? value) {
  if (param.kind == ParamKind.folder || param.kind == ParamKind.file) {
    return asPathSelection(value)?.displayPath ?? '';
  }
  return _jsString(value);
}

/// Paths held in a `files` value, skipping anything that is not a selection.
List<PathSelection> _pathsOf(Object? value) {
  if (value is! List) return const [];
  return [
    for (final item in value)
      if (asPathSelection(item) case final path?) path,
  ];
}

/// The paths a drop delivered, as `file.path || file.name` read them in the
/// browser. String items carry no file and are skipped.
List<String> _droppedPaths(DropDetails details) {
  final paths = <String>[];
  for (final item in details.items) {
    final String? path = switch (item) {
      DropFileItem file => file.localPath ?? file.name,
      DropDirectoryItem directory => directory.localPath ?? directory.name,
      DropStringItem _ => null,
    };
    if (path != null && path.isNotEmpty) paths.add(path);
  }
  return paths;
}

/// Count of one sheet, as `Number(counts[choice] ?? 0)`.
int _countOf(Object? value) {
  if (value is num) return value.toInt();
  if (value is String) return int.tryParse(value) ?? 0;
  return 0;
}

class _SchemaFieldState extends State<SchemaField> {
  late final TextEditingController _text;
  bool _filesDragOver = false;
  bool _browseDragOver = false;

  @override
  void initState() {
    super.initState();
    _text = TextEditingController(
      text: _displayFor(widget.param, widget.value),
    );
  }

  @override
  void didUpdateWidget(SchemaField oldWidget) {
    super.didUpdateWidget(oldWidget);
    final next = _displayFor(widget.param, widget.value);
    final previous = _displayFor(oldWidget.param, oldWidget.value);
    if (next != previous && _text.text != next) {
      _text.value = TextEditingValue(
        text: next,
        selection: TextSelection.collapsed(offset: next.length),
      );
    }
  }

  @override
  void dispose() {
    _text.dispose();
    super.dispose();
  }

  bool get _visible => widget.param.visibleWhen.entries.every(
    (entry) => widget.values[entry.key] == entry.value,
  );

  void _fail(Object error) {
    if (mounted) widget.onError(humanError(error, widget.s));
  }

  @override
  Widget build(BuildContext context) {
    if (!_visible) return const SizedBox.shrink();
    final param = widget.param;
    return switch (param.kind) {
      ParamKind.bool => _buildToggle(context),
      ParamKind.choice => _buildChoice(context),
      ParamKind.files => _buildFiles(context),
      ParamKind.sheetCounts => _buildSheetCounts(context),
      ParamKind.longText => _buildLongText(context),
      ParamKind.folder || ParamKind.file => _buildBrowse(context),
      ParamKind.text || ParamKind.int || ParamKind.secret => _buildText(),
    };
  }

  String _label() => widget.s.t(widget.param.labelKey);

  String? _help() {
    final key = widget.param.helpKey;
    return key == null ? null : widget.s.t(key);
  }

  Widget _buildToggle(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final checked = widget.value == true;
    final label = _label();
    final help = _help();
    return InkWell(
      onTap: () => widget.onChange(!checked),
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 4),
        child: Row(
          children: [
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    label,
                    style: TextStyle(
                      fontSize: remPx(context, 0.875),
                      fontWeight: FontWeight.w700,
                      color: t.text,
                    ),
                  ),
                  if (help != null)
                    Text(
                      help,
                      style: TextStyle(
                        fontSize: remPx(context, 0.8),
                        height: 1.45,
                        color: t.muted,
                      ),
                    ),
                ],
              ),
            ),
            const SizedBox(width: 12),
            Switch(
              checked: checked,
              onChanged: (value) => widget.onChange(value),
              semanticsLabel: label,
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildChoice(BuildContext context) {
    final current = _jsString(widget.value);
    final unavailable =
        current.isNotEmpty && !widget.param.choices.contains(current);
    final labels = widget.param.choiceLabels;
    final items = <DropdownMenuItem<String>>[
      if (unavailable)
        DropdownMenuItem<String>(value: current, child: Text(current)),
      for (final choice in widget.param.choices)
        DropdownMenuItem<String>(
          value: choice,
          child: Text(
            labels[choice] != null ? widget.s.t(labels[choice]!) : choice,
          ),
        ),
    ];
    final selected = current.isEmpty ? null : current;
    return SelectField<String>(
      label: _label(),
      value: items.any((item) => item.value == selected) ? selected : null,
      items: items,
      help: _help(),
      onChanged: (value) {
        if (value != null) widget.onChange(value);
      },
    );
  }

  void _setPaths(List<PathSelection> merged) {
    widget.onChange([for (final path in merged) path.toJson()]);
  }

  /// Merges [selected] into [current] by selection id, keeping the first
  /// position of each selection (`new Map([...paths, ...selected])`).
  void _mergeInto(List<PathSelection> current, List<PathSelection> selected) {
    final merged = <String, PathSelection>{
      for (final path in [...current, ...selected]) path.selectionId: path,
    };
    _setPaths(merged.values.toList());
  }

  Future<void> _addFiles(List<PathSelection> paths) async {
    try {
      final selected = await chooseFiles(
        widget.bridge,
        widget.operationId,
        widget.param.name,
      );
      if (mounted) _mergeInto(paths, selected);
    } catch (error) {
      _fail(error);
    }
  }

  Future<void> _dropFiles(
    List<PathSelection> paths,
    DropDetails details,
  ) async {
    final dropped = _droppedPaths(details);
    if (dropped.isEmpty) return;
    try {
      final selected = await dropPaths(
        widget.bridge,
        widget.operationId,
        widget.param.name,
        dropped,
      );
      if (mounted) _mergeInto(paths, selected);
    } catch (error) {
      _fail(error);
    }
  }

  Widget _buildFiles(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final paths = _pathsOf(widget.value);
    final s = widget.s;
    return DropTarget(
      onEnter: (_) => setState(() => _filesDragOver = true),
      onLeave: () => setState(() => _filesDragOver = false),
      onDrop: (details) {
        setState(() => _filesDragOver = false);
        return _dropFiles(paths, details);
      },
      child: FormFieldShell(
        label: _label(),
        help: _help(),
        child: ControlBox(
          highlighted: _filesDragOver,
          minHeight: 0,
          child: Padding(
            padding: const EdgeInsets.symmetric(vertical: 8),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    DfButton(
                      variant: DfButtonVariant.secondary,
                      icon: Icon(AppIcons.plus, size: remPx(context, 0.95)),
                      onPressed: () => _addFiles(paths),
                      child: Text(s.t('action.addFiles')),
                    ),
                    if (paths.isNotEmpty)
                      DfButton(
                        variant: DfButtonVariant.ghost,
                        onPressed: () => widget.onChange(<Object?>[]),
                        child: Text(s.t('action.clear')),
                      ),
                  ],
                ),
                if (paths.isNotEmpty) ...[
                  const SizedBox(height: 8),
                  for (var index = 0; index < paths.length; index++)
                    _SelectedFile(
                      path: paths[index],
                      removeLabel: s.t('action.remove'),
                      color: t.text,
                      onRemove: () => widget.onChange([
                        for (var i = 0; i < paths.length; i++)
                          if (i != index) paths[i].toJson(),
                      ]),
                    ),
                ],
              ],
            ),
          ),
        ),
      ),
    );
  }

  Widget _buildSheetCounts(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final s = widget.s;
    final value = widget.value;
    final Map<String, Object?> counts = value is Map
        ? value.cast<String, Object?>()
        : const <String, Object?>{};
    final choices = widget.param.choices;
    final total = [for (final choice in choices) _countOf(counts[choice])]
        .fold<int>(0, (sum, count) => sum + count);

    void update(String choice, int next) {
      final clamped = math.min(math.max(next, 0), sheetCountMax);
      widget.onChange({...counts, choice: clamped});
    }

    return FormFieldShell(
      label: _label(),
      help: _help(),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              for (final choice in choices)
                _SheetCountCell(
                  choice: choice,
                  count: _countOf(counts[choice]),
                  active: _countOf(counts[choice]) > 0,
                  activeColor: t.text,
                  decreaseLabel: '$choice ${s.t('action.decrease')}',
                  increaseLabel: '$choice ${s.t('action.increase')}',
                  onDecrease: () =>
                      update(choice, _countOf(counts[choice]) - 1),
                  onIncrease: () =>
                      update(choice, _countOf(counts[choice]) + 1),
                ),
            ],
          ),
          const SizedBox(height: 7),
          Text(
            s.t('rfi.sheetCounts.total', {'count': total}),
            style: TextStyle(
              fontSize: remPx(context, 0.8),
              height: 1.45,
              color: t.muted,
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildLongText(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final label = _label();
    return FormFieldShell(
      label: label,
      help: _help(),
      child: ControlBox(
        minHeight: 0,
        child: Padding(
          padding: const EdgeInsets.symmetric(vertical: 8),
          child: Semantics(
            label: label,
            textField: true,
            child: TextField(
              controller: _text,
              minLines: 8,
              maxLines: 8,
              cursorColor: t.text,
              style: TextStyle(fontSize: remPx(context, 0.875), color: t.text),
              decoration: const InputDecoration.collapsed(hintText: null),
              onChanged: (value) => widget.onChange(value),
            ),
          ),
        ),
      ),
    );
  }

  Widget _buildText() {
    final param = widget.param;
    return Field(
      label: _label(),
      help: _help(),
      controller: _text,
      obscureText: param.kind == ParamKind.secret,
      keyboardType: param.kind == ParamKind.int
          ? const TextInputType.numberWithOptions(decimal: true, signed: true)
          : null,
      onChanged: (value) => widget.onChange(
        param.kind == ParamKind.int ? _parseInt(value) : value,
      ),
    );
  }

  /// `Number(event.target.value)`: an empty field is 0.
  num _parseInt(String text) {
    final trimmed = text.trim();
    if (trimmed.isEmpty) return 0;
    return num.tryParse(trimmed) ?? double.nan;
  }

  Widget _buildBrowse(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final s = widget.s;
    final param = widget.param;
    final path = asPathSelection(widget.value);
    final hasPath = path != null;
    final label = _label();

    Future<void> browse() async {
      try {
        final selected = param.kind == ParamKind.folder
            ? await chooseFolder(widget.bridge, widget.operationId, param.name)
            : await chooseFile(widget.bridge, widget.operationId, param.name);
        if (mounted && selected != null) widget.onChange(selected.toJson());
      } catch (error) {
        _fail(error);
      }
    }

    Future<void> dropOne(DropDetails details) async {
      final dropped = _droppedPaths(details);
      if (dropped.isEmpty) return;
      try {
        final selected = await dropPaths(
          widget.bridge,
          widget.operationId,
          param.name,
          dropped,
        );
        if (mounted && selected.isNotEmpty) {
          widget.onChange(selected.first.toJson());
        }
      } catch (error) {
        _fail(error);
      }
    }

    return DropTarget(
      onEnter: (_) => setState(() => _browseDragOver = true),
      onLeave: () => setState(() => _browseDragOver = false),
      onDrop: (details) {
        setState(() => _browseDragOver = false);
        return dropOne(details);
      },
      child: FormFieldShell(
        label: label,
        help: _help(),
        child: ControlBox(
          highlighted: _browseDragOver,
          child: Row(
            children: [
              Expanded(
                child: Semantics(
                  label: label,
                  textField: true,
                  child: TextField(
                    controller: _text,
                    readOnly: true,
                    cursorColor: t.text,
                    style: TextStyle(
                      fontSize: remPx(context, 0.875),
                      color: t.text,
                    ),
                    decoration: InputDecoration.collapsed(
                      hintText: s.t('param.hint.${param.kind.name}'),
                    ),
                  ),
                ),
              ),
              const SizedBox(width: 8),
              DfButton(
                variant: DfButtonVariant.secondary,
                icon: Icon(AppIcons.folderOpen, size: remPx(context, 0.95)),
                onPressed: browse,
                child: Text(s.t('action.browse')),
              ),
              if (hasPath)
                Tooltip(
                  message: s.t('action.clear'),
                  child: Semantics(
                    button: true,
                    label: s.t('action.clear'),
                    child: InkWell(
                      borderRadius: BorderRadius.circular(7),
                      onTap: () => widget.onChange(null),
                      child: Padding(
                        padding: const EdgeInsets.all(6),
                        child: Icon(
                          AppIcons.close,
                          size: remPx(context, 0.95),
                          color: t.muted,
                        ),
                      ),
                    ),
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

/// One selected file row of a `files` parameter.
class _SelectedFile extends StatelessWidget {
  const _SelectedFile({
    required this.path,
    required this.removeLabel,
    required this.color,
    required this.onRemove,
  });

  final PathSelection path;
  final String removeLabel;
  final Color color;
  final VoidCallback onRemove;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.symmetric(vertical: 2),
    child: Row(
      children: [
        Expanded(
          child: Tooltip(
            message: path.displayPath,
            child: Text(
              path.name,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(fontSize: remPx(context, 0.875), color: color),
            ),
          ),
        ),
        Tooltip(
          message: removeLabel,
          child: Semantics(
            button: true,
            label: removeLabel,
            child: InkWell(
              borderRadius: BorderRadius.circular(7),
              onTap: onRemove,
              child: Padding(
                padding: const EdgeInsets.all(6),
                child: Icon(
                  AppIcons.close,
                  size: remPx(context, 0.9),
                  color: color,
                ),
              ),
            ),
          ),
        ),
      ],
    ),
  );
}

/// One sheet of a `sheetCounts` parameter with its decrease and increase
/// steps (`.sheet-count`).
class _SheetCountCell extends StatelessWidget {
  const _SheetCountCell({
    required this.choice,
    required this.count,
    required this.active,
    required this.activeColor,
    required this.decreaseLabel,
    required this.increaseLabel,
    required this.onDecrease,
    required this.onIncrease,
  });

  final String choice;
  final int count;
  final bool active;
  final Color activeColor;
  final String decreaseLabel;
  final String increaseLabel;
  final VoidCallback onDecrease;
  final VoidCallback onIncrease;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return Container(
      width: 132,
      padding: const EdgeInsets.all(10),
      decoration: BoxDecoration(
        color: t.surface,
        border: Border.all(color: active ? activeColor : t.line),
        borderRadius: BorderRadius.circular(9),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            choice,
            style: TextStyle(
              fontSize: remPx(context, 0.875),
              fontWeight: FontWeight.w700,
              color: t.text,
            ),
          ),
          const SizedBox(height: 6),
          Row(
            children: [
              _StepButton(
                label: decreaseLabel,
                glyph: '−',
                enabled: count > 0,
                onPressed: onDecrease,
              ),
              Expanded(
                child: Text(
                  '$count',
                  textAlign: TextAlign.center,
                  style: TextStyle(
                    fontSize: remPx(context, 0.875),
                    color: t.text,
                  ),
                ),
              ),
              _StepButton(
                label: increaseLabel,
                glyph: '+',
                enabled: count < sheetCountMax,
                onPressed: onIncrease,
              ),
            ],
          ),
        ],
      ),
    );
  }
}

class _StepButton extends StatelessWidget {
  const _StepButton({
    required this.label,
    required this.glyph,
    required this.enabled,
    required this.onPressed,
  });

  final String label;
  final String glyph;
  final bool enabled;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return Tooltip(
      message: label,
      child: Semantics(
        button: true,
        enabled: enabled,
        label: label,
        child: InkWell(
          borderRadius: BorderRadius.circular(7),
          onTap: enabled ? onPressed : null,
          child: Container(
            width: 28,
            height: 28,
            alignment: Alignment.center,
            decoration: BoxDecoration(
              border: Border.all(color: t.line),
              borderRadius: BorderRadius.circular(7),
            ),
            child: Text(
              glyph,
              style: TextStyle(
                fontSize: remPx(context, 0.95),
                color: enabled ? t.text : t.muted,
              ),
            ),
          ),
        ),
      ),
    );
  }
}
