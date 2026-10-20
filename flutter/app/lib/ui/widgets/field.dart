// Field, ported from ui.tsx and the `.field` / `.input-shell` rules.

import 'package:flutter/material.dart' hide IconButton, Switch;
import 'package:flutter/services.dart';

import '../../theme/tokens.dart';
import 'metrics.dart';

/// A labelled text input. [controller] owns the value (the React `value` prop);
/// [onChanged] receives every edit (the React `onChange`).
class Field extends StatefulWidget {
  const Field({
    super.key,
    required this.label,
    required this.controller,
    required this.onChanged,
    this.help,
    this.icon,
    this.suffix,
    this.invalid = false,
    this.focusNode,
    this.enabled = true,
    this.autofocus = false,
    this.obscureText = false,
    this.keyboardType,
    this.textInputAction,
    this.onSubmitted,
    this.inputFormatters,
    this.autofillHints,
  });

  final String label;
  final TextEditingController controller;
  final ValueChanged<String> onChanged;
  final String? help;
  final Widget? icon;
  final Widget? suffix;
  final bool invalid;
  final FocusNode? focusNode;
  final bool enabled;
  final bool autofocus;
  final bool obscureText;
  final TextInputType? keyboardType;
  final TextInputAction? textInputAction;
  final ValueChanged<String>? onSubmitted;
  final List<TextInputFormatter>? inputFormatters;
  final Iterable<String>? autofillHints;

  @override
  State<Field> createState() => _FieldState();
}

class _FieldState extends State<Field> {
  FocusNode? _ownNode;

  FocusNode get _node => widget.focusNode ?? (_ownNode ??= FocusNode());

  FocusNode? _listened;

  @override
  void initState() {
    super.initState();
    _listen();
  }

  @override
  void didUpdateWidget(Field oldWidget) {
    super.didUpdateWidget(oldWidget);
    _listen();
  }

  void _listen() {
    if (_listened == _node) return;
    _listened?.removeListener(_onFocusChange);
    _node.addListener(_onFocusChange);
    _listened = _node;
  }

  void _onFocusChange() {
    if (mounted) setState(() {});
  }

  @override
  void dispose() {
    _listened?.removeListener(_onFocusChange);
    _ownNode?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final focused = _node.hasFocus;
    final shellChildren = <Widget>[
      if (widget.icon != null)
        IconTheme(
          data: IconThemeData(size: remPx(context, 1.1), color: t.muted),
          child: widget.icon!,
        ),
      if (widget.icon != null) const SizedBox(width: 8),
      Expanded(
        child: MergeSemantics(
          child: Semantics(
            label: widget.label,
            hint: widget.help,
            textField: true,
            child: TextField(
              controller: widget.controller,
              focusNode: _node,
              enabled: widget.enabled,
              autofocus: widget.autofocus,
              obscureText: widget.obscureText,
              keyboardType: widget.keyboardType,
              textInputAction: widget.textInputAction,
              inputFormatters: widget.inputFormatters,
              autofillHints: widget.autofillHints,
              onChanged: widget.onChanged,
              onSubmitted: widget.onSubmitted,
              cursorColor: t.text,
              style: TextStyle(fontSize: remPx(context, 0.875), color: t.text),
              decoration: const InputDecoration.collapsed(hintText: null),
            ),
          ),
        ),
      ),
      if (widget.suffix != null) ...[const SizedBox(width: 8), widget.suffix!],
    ];

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        // The visible label is excluded from semantics: the input carries it.
        ExcludeSemantics(
          child: Text(
            widget.label,
            style: TextStyle(
              fontSize: remPx(context, 0.875),
              fontWeight: FontWeight.w600,
              height: 1.4,
              color: t.text,
            ),
          ),
        ),
        const SizedBox(height: 7),
        Container(
          height: 38,
          padding: const EdgeInsets.symmetric(horizontal: 10),
          decoration: BoxDecoration(
            color: t.surface,
            border: Border.all(color: focused ? t.text : t.line),
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
          child: Row(children: shellChildren),
        ),
        if (widget.help != null) ...[
          const SizedBox(height: 7),
          ExcludeSemantics(
            child: Text(
              widget.help!,
              style: TextStyle(
                fontSize: remPx(context, 0.8),
                height: 1.45,
                color: widget.invalid ? t.danger : t.muted,
              ),
            ),
          ),
        ],
      ],
    );
  }
}
