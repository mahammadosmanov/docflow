// Form chrome shared by SchemaField, PresetToolbar and CompactDateInput: the
// `.field` label, the bordered control box, a select, and the help line. The
// styling follows `.field`, `.input-shell` and `.dropzone-field` in styles.css.

import 'package:flutter/material.dart' hide IconButton, Switch;

import '../../theme/tokens.dart';
import '../widgets/metrics.dart';

/// Label, control and optional help text of one form field (`.field`).
class FormFieldShell extends StatelessWidget {
  const FormFieldShell({
    super.key,
    required this.label,
    required this.child,
    this.help,
  });

  final String label;
  final Widget child;
  final String? help;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        ExcludeSemantics(
          child: Text(
            label,
            style: TextStyle(
              fontSize: remPx(context, 0.875),
              fontWeight: FontWeight.w600,
              height: 1.4,
              color: t.text,
            ),
          ),
        ),
        const SizedBox(height: 7),
        child,
        if (help != null) ...[
          const SizedBox(height: 7),
          ExcludeSemantics(
            child: Text(
              help!,
              style: TextStyle(
                fontSize: remPx(context, 0.8),
                height: 1.45,
                color: t.muted,
              ),
            ),
          ),
        ],
      ],
    );
  }
}

/// The bordered control surface of a field (`.input-shell`). [focused] draws
/// the focus border, and [highlighted] draws the heavier drop-target border.
class ControlBox extends StatelessWidget {
  const ControlBox({
    super.key,
    required this.child,
    this.focused = false,
    this.highlighted = false,
    this.minHeight = 38,
  });

  final Widget child;
  final bool focused;
  final bool highlighted;
  final double minHeight;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final active = focused || highlighted;
    return Container(
      constraints: BoxConstraints(minHeight: minHeight),
      padding: const EdgeInsets.symmetric(horizontal: 10),
      decoration: BoxDecoration(
        color: t.surface,
        border: Border.all(
          color: active ? t.text : t.line,
          width: highlighted ? 2 : 1,
        ),
        borderRadius: BorderRadius.circular(9),
        boxShadow: focused
            ? [
                BoxShadow(
                  color: t.text.withValues(alpha: 0.12),
                  spreadRadius: 2,
                ),
              ]
            : null,
      ),
      child: child,
    );
  }
}

/// A labelled select (`<select>` inside `.field`). [items] carries the options,
/// so a caller can add a placeholder or an unavailable current value.
class SelectField<T> extends StatefulWidget {
  const SelectField({
    super.key,
    required this.label,
    required this.value,
    required this.items,
    required this.onChanged,
    this.help,
    this.enabled = true,
  });

  final String label;
  final T? value;
  final List<DropdownMenuItem<T>> items;
  final ValueChanged<T?> onChanged;
  final String? help;
  final bool enabled;

  @override
  State<SelectField<T>> createState() => _SelectFieldState<T>();
}

class _SelectFieldState<T> extends State<SelectField<T>> {
  final FocusNode _focus = FocusNode();

  @override
  void initState() {
    super.initState();
    _focus.addListener(_onFocus);
  }

  @override
  void dispose() {
    _focus.removeListener(_onFocus);
    _focus.dispose();
    super.dispose();
  }

  void _onFocus() {
    if (mounted) setState(() {});
  }

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return FormFieldShell(
      label: widget.label,
      help: widget.help,
      child: Semantics(
        label: widget.label,
        child: ControlBox(
          focused: _focus.hasFocus,
          child: DropdownButtonHideUnderline(
            child: DropdownButton<T>(
              value: widget.value,
              items: widget.items,
              onChanged: widget.enabled ? widget.onChanged : null,
              focusNode: _focus,
              isExpanded: true,
              dropdownColor: t.surface,
              iconEnabledColor: t.text,
              style: TextStyle(fontSize: remPx(context, 0.875), color: t.text),
            ),
          ),
        ),
      ),
    );
  }
}
