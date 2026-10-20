// Small controls shared by the Plan preview: the bordered card, the icon
// heading, the count list, a compact text button, checkbox, select, text input
// and the `<details>` disclosure. They follow the `.plan-impact`,
// `.editor-card`, `.bulk-editor` and `.warning-review` rules in styles.css.

import 'package:flutter/material.dart' hide IconButton, Switch;
import 'package:flutter/services.dart';

import '../../theme/tokens.dart';
import '../screens/form_controls.dart' show ControlBox;
import '../widgets/metrics.dart';
import '../widgets/pressable.dart';

/// A bordered surface block (`.plan-impact`, `.editor-card`).
class PlanCard extends StatelessWidget {
  const PlanCard({
    super.key,
    required this.child,
    this.padding = const EdgeInsets.all(14),
    this.semanticsLabel,
  });

  final Widget child;
  final EdgeInsetsGeometry padding;

  /// Names the region (the React `aria-labelledby` / `aria-label`).
  final String? semanticsLabel;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final card = Container(
      width: double.infinity,
      padding: padding,
      decoration: BoxDecoration(
        color: t.surface,
        border: Border.all(color: t.line),
        borderRadius: BorderRadius.circular(9),
      ),
      child: child,
    );
    if (semanticsLabel == null) return card;
    return Semantics(
      container: true,
      explicitChildNodes: true,
      label: semanticsLabel,
      child: card,
    );
  }
}

/// An icon followed by a bold title and a muted line (`.plan-impact-heading`).
class PlanHeading extends StatelessWidget {
  const PlanHeading({
    super.key,
    required this.icon,
    required this.title,
    this.subtitle,
    this.trailing,
  });

  final IconData icon;
  final String title;
  final String? subtitle;
  final Widget? trailing;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return Row(
      crossAxisAlignment: CrossAxisAlignment.center,
      children: [
        Container(
          width: 34,
          height: 34,
          alignment: Alignment.center,
          decoration: BoxDecoration(
            color: t.surface3,
            borderRadius: BorderRadius.circular(8),
          ),
          child: ExcludeSemantics(
            child: Icon(icon, size: remPx(context, 1.05), color: t.text),
          ),
        ),
        const SizedBox(width: 10),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisSize: MainAxisSize.min,
            children: [
              Semantics(
                header: true,
                child: Text(title, style: planStrong(context)),
              ),
              if (subtitle != null) ...[
                const SizedBox(height: 2),
                Text(subtitle!, style: planMuted(context)),
              ],
            ],
          ),
        ),
        if (trailing != null) ...[const SizedBox(width: 10), trailing!],
      ],
    );
  }
}

TextStyle planStrong(BuildContext context, {double rem = 0.9}) => TextStyle(
  fontSize: remPx(context, rem),
  fontWeight: FontWeight.w700,
  color: DocFlowTokens.of(context).text,
);

TextStyle planBody(BuildContext context, {double rem = 0.82}) => TextStyle(
  fontSize: remPx(context, rem),
  height: 1.4,
  color: DocFlowTokens.of(context).text,
);

TextStyle planMuted(BuildContext context, {double rem = 0.76}) => TextStyle(
  fontSize: remPx(context, rem),
  height: 1.4,
  color: DocFlowTokens.of(context).muted,
);

TextStyle planMono(BuildContext context, {double rem = 0.78}) => TextStyle(
  fontSize: remPx(context, rem),
  height: 1.45,
  fontFamily: 'SFMono-Regular',
  fontFamilyFallback: const ['Consolas', 'monospace'],
  color: DocFlowTokens.of(context).text,
);

/// The `<dl>` of labelled counts (`.plan-impact-counts`).
class PlanCountList extends StatelessWidget {
  const PlanCountList({super.key, required this.entries});

  final List<(String, int)> entries;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return Wrap(
      spacing: 8,
      runSpacing: 8,
      children: [
        for (final (label, count) in entries)
          MergeSemantics(
            child: Container(
              constraints: const BoxConstraints(minWidth: 110),
              padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 7),
              decoration: BoxDecoration(
                color: t.surface2,
                borderRadius: BorderRadius.circular(8),
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                mainAxisSize: MainAxisSize.min,
                children: [
                  Text(label, style: planMuted(context, rem: 0.7)),
                  Text('$count', style: planStrong(context, rem: 1.05)),
                ],
              ),
            ),
          ),
      ],
    );
  }
}

/// A compact bordered `<button>` (`.bulk-selection button`, `.filter-tab`).
/// [active] draws the selected state.
class PlanTextButton extends StatelessWidget {
  const PlanTextButton({
    super.key,
    required this.child,
    required this.onPressed,
    this.icon,
    this.tooltip,
    this.semanticsLabel,
    this.active = false,
    this.selected,
  });

  final Widget child;
  final VoidCallback? onPressed;
  final IconData? icon;
  final String? tooltip;
  final String? semanticsLabel;
  final bool active;
  final bool? selected;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final enabled = onPressed != null;
    final button = Pressable(
      onPressed: onPressed,
      label: semanticsLabel,
      selected: selected,
      builder: (context, hovered, focused) => FocusRing(
        visible: focused,
        radius: 7,
        child: Opacity(
          opacity: enabled ? 1 : 0.4,
          child: Container(
            constraints: const BoxConstraints(minHeight: 30),
            padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 5),
            decoration: BoxDecoration(
              color: active
                  ? t.inverse
                  : (hovered && enabled ? t.surface3 : t.surface2),
              border: Border.all(color: active ? t.inverse : t.line),
              borderRadius: BorderRadius.circular(7),
            ),
            child: DefaultTextStyle(
              style: TextStyle(
                fontSize: remPx(context, 0.76),
                fontWeight: FontWeight.w600,
                color: active ? t.inverseText : t.text,
              ),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  if (icon != null) ...[
                    Icon(
                      icon,
                      size: remPx(context, 0.9),
                      color: active ? t.inverseText : t.text,
                    ),
                    const SizedBox(width: 5),
                  ],
                  Flexible(child: child),
                ],
              ),
            ),
          ),
        ),
      ),
    );
    if (tooltip == null || tooltip!.isEmpty) return button;
    return Tooltip(message: tooltip, child: button);
  }
}

/// A small square icon button (`.editor-row-actions button`). [label] is the
/// tooltip and accessible name.
class PlanIconButton extends StatelessWidget {
  const PlanIconButton({
    super.key,
    required this.icon,
    required this.label,
    required this.onPressed,
  });

  final IconData icon;
  final String label;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final enabled = onPressed != null;
    return Tooltip(
      message: label,
      child: Pressable(
        onPressed: onPressed,
        label: label,
        builder: (context, hovered, focused) => FocusRing(
          visible: focused,
          radius: 6,
          child: Opacity(
            opacity: enabled ? 1 : 0.35,
            child: Container(
              width: 28,
              height: 28,
              alignment: Alignment.center,
              decoration: BoxDecoration(
                color: hovered && enabled ? t.surface3 : Colors.transparent,
                border: Border.all(color: t.line),
                borderRadius: BorderRadius.circular(6),
              ),
              child: Icon(icon, size: remPx(context, 0.9), color: t.text),
            ),
          ),
        ),
      ),
    );
  }
}

/// A checkbox with an accessible name (`<input type="checkbox">`). A null
/// [onChanged] disables it.
class PlanCheckbox extends StatelessWidget {
  const PlanCheckbox({
    super.key,
    required this.value,
    required this.onChanged,
    required this.label,
  });

  final bool value;
  final ValueChanged<bool>? onChanged;
  final String label;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return Semantics(
      label: label,
      child: SizedBox(
        width: 28,
        height: 28,
        child: Checkbox(
          value: value,
          onChanged: onChanged == null
              ? null
              : (checked) => onChanged!(checked ?? false),
          activeColor: t.inverse,
          checkColor: t.inverseText,
          side: BorderSide(color: t.lineStrong),
          materialTapTargetSize: MaterialTapTargetSize.shrinkWrap,
          visualDensity: VisualDensity.compact,
        ),
      ),
    );
  }
}

/// A compact `<select>`. [options] pairs each value with its label. A [value]
/// that is not among the options shows no selection.
class PlanSelect extends StatelessWidget {
  const PlanSelect({
    super.key,
    required this.label,
    required this.value,
    required this.options,
    required this.onChanged,
    this.tooltip,
  });

  final String label;
  final String value;
  final List<(String, String)> options;
  final ValueChanged<String> onChanged;
  final String? tooltip;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final seen = <String>{};
    final unique = [
      for (final option in options)
        if (seen.add(option.$1)) option,
    ];
    final selected = seen.contains(value) ? value : null;
    final select = Semantics(
      label: label,
      child: ControlBox(
        minHeight: 32,
        child: DropdownButtonHideUnderline(
          child: DropdownButton<String>(
            value: selected,
            isExpanded: true,
            isDense: true,
            dropdownColor: t.surface,
            iconEnabledColor: t.text,
            style: TextStyle(fontSize: remPx(context, 0.8), color: t.text),
            items: [
              for (final (optionValue, optionLabel) in unique)
                DropdownMenuItem(
                  value: optionValue,
                  child: Text(
                    optionLabel,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
            ],
            onChanged: (next) {
              if (next != null) onChanged(next);
            },
          ),
        ),
      ),
    );
    if (tooltip == null || tooltip!.isEmpty) return select;
    return Tooltip(message: tooltip, child: select);
  }
}

/// A controlled text input. The parent owns [value]; every edit is reported
/// through [onChanged]. [suggestions] act as the `<datalist>`.
class PlanTextInput extends StatefulWidget {
  const PlanTextInput({
    super.key,
    required this.label,
    required this.value,
    required this.onChanged,
    this.placeholder,
    this.number = false,
    this.multiline = false,
    this.search = false,
    this.invalid = false,
    this.plain = false,
    this.suggestions = const [],
  });

  final String label;
  final String value;
  final ValueChanged<String> onChanged;
  final String? placeholder;

  /// `type="number"`: only numeric characters are accepted.
  final bool number;

  /// `<textarea rows={2}>`.
  final bool multiline;

  /// `type="search"`: shows the search glyph.
  final bool search;
  final bool invalid;

  /// No autocorrect or suggestions (`spellCheck={false}`, `translate="no"`).
  final bool plain;
  final List<String> suggestions;

  @override
  State<PlanTextInput> createState() => _PlanTextInputState();
}

class _PlanTextInputState extends State<PlanTextInput> {
  late final TextEditingController _controller = TextEditingController(
    text: widget.value,
  );
  final FocusNode _focus = FocusNode();

  @override
  void didUpdateWidget(PlanTextInput oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.value != _controller.text) {
      _controller.value = TextEditingValue(
        text: widget.value,
        selection: TextSelection.collapsed(offset: widget.value.length),
      );
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    _focus.dispose();
    super.dispose();
  }

  Widget _field(
    BuildContext context,
    TextEditingController controller,
    FocusNode focus,
  ) {
    final t = DocFlowTokens.of(context);
    final border = OutlineInputBorder(
      borderRadius: BorderRadius.circular(7),
      borderSide: BorderSide(color: widget.invalid ? t.danger : t.line),
    );
    return TextField(
      controller: controller,
      focusNode: focus,
      onChanged: widget.onChanged,
      minLines: widget.multiline ? 2 : 1,
      maxLines: widget.multiline ? 2 : 1,
      autocorrect: !widget.plain,
      enableSuggestions: !widget.plain,
      keyboardType: widget.number
          ? const TextInputType.numberWithOptions(signed: true, decimal: true)
          : (widget.multiline ? TextInputType.multiline : TextInputType.text),
      inputFormatters: widget.number
          ? [FilteringTextInputFormatter.allow(RegExp(r'[0-9eE+\-.]'))]
          : null,
      style: TextStyle(fontSize: remPx(context, 0.8), color: t.text),
      cursorColor: t.text,
      decoration: InputDecoration(
        isDense: true,
        hintText: widget.placeholder,
        hintStyle: TextStyle(color: t.muted),
        prefixIcon: widget.search
            ? Icon(Icons.search, size: remPx(context, 1), color: t.muted)
            : null,
        prefixIconConstraints: const BoxConstraints(minWidth: 30),
        filled: true,
        fillColor: t.surface,
        contentPadding: const EdgeInsets.symmetric(horizontal: 9, vertical: 9),
        enabledBorder: border,
        border: border,
        focusedBorder: border.copyWith(
          borderSide: BorderSide(color: widget.invalid ? t.danger : t.text),
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final Widget input = widget.suggestions.isEmpty
        ? _field(context, _controller, _focus)
        : RawAutocomplete<String>(
            textEditingController: _controller,
            focusNode: _focus,
            optionsBuilder: (value) {
              final query = value.text.toLowerCase();
              return widget.suggestions.where(
                (option) => option.toLowerCase().contains(query),
              );
            },
            onSelected: widget.onChanged,
            fieldViewBuilder: (context, controller, focus, _) =>
                _field(context, controller, focus),
            optionsViewBuilder: (context, onSelected, options) => Align(
              alignment: AlignmentDirectional.topStart,
              child: Material(
                color: t.surface,
                elevation: 4,
                borderRadius: BorderRadius.circular(7),
                child: ConstrainedBox(
                  constraints: const BoxConstraints(
                    maxHeight: 220,
                    maxWidth: 320,
                  ),
                  child: ListView(
                    padding: EdgeInsets.zero,
                    shrinkWrap: true,
                    children: [
                      for (final option in options)
                        InkWell(
                          onTap: () => onSelected(option),
                          child: Padding(
                            padding: const EdgeInsets.symmetric(
                              horizontal: 10,
                              vertical: 8,
                            ),
                            child: Text(option, style: planBody(context)),
                          ),
                        ),
                    ],
                  ),
                ),
              ),
            ),
          );
    return Semantics(label: widget.label, textField: true, child: input);
  }
}

/// A `<details>` block: [summary] toggles [child], collapsed at first.
class PlanDisclosure extends StatefulWidget {
  const PlanDisclosure({super.key, required this.summary, required this.child});

  final Widget summary;
  final Widget child;

  @override
  State<PlanDisclosure> createState() => _PlanDisclosureState();
}

class _PlanDisclosureState extends State<PlanDisclosure> {
  bool _open = false;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return PlanCard(
      padding: EdgeInsets.zero,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          Pressable(
            onPressed: () => setState(() => _open = !_open),
            toggled: _open,
            builder: (context, hovered, focused) => FocusRing(
              visible: focused,
              radius: 9,
              child: Container(
                padding: const EdgeInsets.symmetric(
                  horizontal: 14,
                  vertical: 11,
                ),
                color: hovered ? t.surface2 : Colors.transparent,
                child: Row(
                  children: [
                    Icon(
                      _open ? Icons.expand_more : Icons.chevron_right,
                      size: remPx(context, 1.1),
                      color: t.muted,
                    ),
                    const SizedBox(width: 6),
                    Expanded(child: widget.summary),
                  ],
                ),
              ),
            ),
          ),
          if (_open)
            Padding(
              padding: const EdgeInsets.fromLTRB(14, 0, 14, 14),
              child: widget.child,
            ),
        ],
      ),
    );
  }
}

/// Wraps [child] in a tooltip (the HTML `title`) unless [message] is empty.
Widget planTooltip(String? message, Widget child) =>
    message == null || message.isEmpty
    ? child
    : Tooltip(message: message, child: child);
