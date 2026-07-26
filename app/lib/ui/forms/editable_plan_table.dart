import 'package:flutter/material.dart';

import '../../core/l10n/strings.dart';

/// Bir kaynak alan degistiginde schema'daki regex kurallarini uygular.
/// Otomatik uretilen hedef sonradan elle degistirildiyse kullanici degerini korur.
bool applyEditorRegexFill(
  Map<String, dynamic> editor,
  Map<String, dynamic> row,
  String source,
) {
  final rules = editor['regexFill'];
  if (rules is! List) return false;
  var changed = false;
  final processedTargets = <String>{};
  for (final candidate in rules) {
    if (candidate is! Map || candidate['source'] != source) continue;
    final target = candidate['target']?.toString() ?? '';
    if (target.isEmpty || !processedTargets.add(target)) continue;
    final marker = '_inferred:$target';
    final current = row[target]?.toString() ?? '';
    final previous = row[marker]?.toString() ?? '';
    if (current.isNotEmpty && current != previous) continue;

    var inferred = '';
    for (final raw in rules) {
      if (raw is! Map ||
          raw['source'] != source ||
          raw['target']?.toString() != target) {
        continue;
      }
      final pattern = raw['pattern']?.toString() ?? '';
      if (pattern.isNotEmpty &&
          RegExp(
            pattern,
            caseSensitive: false,
          ).hasMatch(row[source]?.toString() ?? '')) {
        inferred = raw['value']?.toString() ?? '';
        break;
      }
    }
    if (current != inferred) {
      row[target] = inferred;
      row['_version:$target'] = ((row['_version:$target'] as int?) ?? 0) + 1;
      changed = true;
    }
    if (inferred.isEmpty) {
      row.remove(marker);
    } else {
      row[marker] = inferred;
    }
  }
  return changed;
}

String planTableDisplayValue(Map<String, dynamic> column, String value) {
  if (column['display'] == 'suffix' && value.contains('-')) {
    return value.split('-').last;
  }
  return value;
}

/// Plan.meta.editor tarafindan tarif edilen genel, duzenlenebilir kayit tablosu.
/// Operation kimligi bilmez; kolon, secenek, bagimli secenek ve otomatik doldurma
/// kurallarinin tamamini sidecar planindan alir.
class EditablePlanTable extends StatefulWidget {
  const EditablePlanTable({
    super.key,
    required this.editor,
    required this.rows,
    required this.onChanged,
  });

  final Map<String, dynamic> editor;
  final List<Map<String, dynamic>> rows;
  final VoidCallback onChanged;

  @override
  State<EditablePlanTable> createState() => _EditablePlanTableState();
}

class _EditablePlanTableState extends State<EditablePlanTable> {
  final ScrollController _horizontal = ScrollController();
  final ScrollController _vertical = ScrollController();
  final Set<String> _expandedRows = {};

  List<Map<String, dynamic>> get _columns =>
      (widget.editor['columns'] as List? ?? const [])
          .map((e) => (e as Map).cast<String, dynamic>())
          .toList();

  bool get _compact => widget.editor['layout'] == 'compact';

  @override
  void dispose() {
    _horizontal.dispose();
    _vertical.dispose();
    super.dispose();
  }

  List<String> _choices(Map<String, dynamic> column, Map<String, dynamic> row) {
    final key = column['key'] as String;
    final direct = (widget.editor['choices'] as Map?)?[key];
    if (direct is List) return direct.map((e) => e.toString()).toList();

    final byValue = (widget.editor['choicesByValue'] as Map?)?[key];
    if (byValue is! Map) return const [];
    final dependsOn = byValue['dependsOn']?.toString() ?? '';
    final values = byValue['values'];
    if (values is! Map) return const [];
    final selected = row[dependsOn]?.toString() ?? '';
    final raw = values[selected];
    return raw is List ? raw.map((e) => e.toString()).toList() : const [];
  }

  void _applyAutoFill(Map<String, dynamic> row, String field, String value) {
    final autoFill = widget.editor['autoFill'];
    if (autoFill is! Map || autoFill['field'] != field) return;
    final values = autoFill['values'];
    final fill = values is Map ? values[value] : null;
    if (fill is! Map) return;
    fill.forEach(
      (key, dynamic item) => row[key.toString()] = item?.toString() ?? '',
    );
    row['_version'] = ((row['_version'] as int?) ?? 0) + 1;
  }

  void _applyDependentDefault(Map<String, dynamic> row, String changedField) {
    final all = widget.editor['choicesByValue'];
    if (all is! Map) return;
    for (final entry in all.entries) {
      final config = entry.value;
      if (config is! Map || config['dependsOn'] != changedField) continue;
      final target = entry.key.toString();
      final values = config['values'];
      final options = values is Map
          ? values[row[changedField]?.toString()]
          : null;
      if (options is List && options.isNotEmpty) {
        row[target] = options.first.toString();
        _applyAutoFill(row, target, row[target].toString());
      }
    }
  }

  void _set(
    Map<String, dynamic> row,
    String key,
    String value, {
    bool rebuild = false,
    bool infer = false,
  }) {
    row[key] = value;
    final inferenceMarker = '_inferred:$key';
    if (row.containsKey(inferenceMarker) && row[inferenceMarker] != value) {
      row.remove(inferenceMarker);
    }
    if (rebuild) {
      _applyDependentDefault(row, key);
      _applyAutoFill(row, key, value);
    }
    final inferred = infer && applyEditorRegexFill(widget.editor, row, key);
    widget.onChanged();
    if (rebuild || inferred) setState(() {});
  }

  void _duplicate(int index) {
    final original = widget.rows[index];
    final rfi = original[widget.editor['groupField'] ?? 'rfiNo'];
    var insertAt = index + 1;
    while (insertAt < widget.rows.length &&
        widget.rows[insertAt][widget.editor['groupField'] ?? 'rfiNo'] == rfi) {
      insertAt++;
    }
    final clone = Map<String, dynamic>.from(original)
      ..['_rowId'] =
          '${original['_rowId']}-copy-${DateTime.now().microsecondsSinceEpoch}'
      ..remove('_version');
    widget.rows.insert(insertAt, clone);
    widget.onChanged();
    setState(() {});
  }

  void _remove(int index) {
    final groupField = widget.editor['groupField']?.toString() ?? 'rfiNo';
    final group = widget.rows[index][groupField];
    final count = widget.rows.where((row) => row[groupField] == group).length;
    if (count <= 1) return;
    widget.rows.removeAt(index);
    widget.onChanged();
    setState(() {});
  }

  Widget _field(Map<String, dynamic> column, Map<String, dynamic> row) {
    final key = column['key'] as String;
    final kind = column['kind']?.toString() ?? 'text';
    final value = row[key]?.toString() ?? '';
    final version = row['_version:$key'] ?? row['_version'] ?? 0;
    final inputDecoration = InputDecoration(
      isDense: true,
      contentPadding: EdgeInsets.symmetric(
        horizontal: _compact ? 6 : 9,
        vertical: _compact ? 7 : 9,
      ),
      border: const OutlineInputBorder(),
    );

    if (kind == 'readonly') {
      return Tooltip(
        message: value,
        child: SelectableText(
          planTableDisplayValue(column, value),
          maxLines: 1,
          style: Theme.of(
            context,
          ).textTheme.bodySmall?.copyWith(fontWeight: FontWeight.w700),
        ),
      );
    }
    if (kind == 'choice') {
      final options = _choices(column, row);
      final selected = options.contains(value) ? value : null;
      return DropdownButtonFormField<String>(
        key: ValueKey('${row['_rowId']}:$key:$value'),
        initialValue: selected,
        isExpanded: true,
        decoration: inputDecoration,
        items: [
          for (final option in options)
            DropdownMenuItem(
              value: option,
              child: Tooltip(
                message: option,
                child: Text(
                  planTableDisplayValue(column, option),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                ),
              ),
            ),
        ],
        onChanged: (choice) {
          if (choice != null) _set(row, key, choice, rebuild: true);
        },
      );
    }
    if (kind == 'autocomplete') {
      final raw = (widget.editor['autocomplete'] as Map?)?[key];
      final options = raw is List
          ? raw.map((e) => e.toString()).toList()
          : <String>[];
      return _AutocompleteCell(
        key: ValueKey('${row['_rowId']}:$key:$version'),
        value: value,
        options: options,
        compact: _compact,
        onChanged: (text) => _set(row, key, text, infer: true),
        onSelected: (text) => _set(row, key, text, infer: true),
      );
    }
    return TextFormField(
      key: ValueKey('${row['_rowId']}:$key:$version'),
      initialValue: value,
      minLines: kind == 'multiline' ? 2 : 1,
      maxLines: kind == 'multiline' ? 4 : 1,
      decoration: inputDecoration,
      onChanged: (text) => _set(row, key, text),
    );
  }

  @override
  Widget build(BuildContext context) {
    if (_compact) return _buildCompact(context);

    final s = Strings.of(context);
    final t = Theme.of(context);
    final columns = _columns;
    final widths = [
      for (final column in columns)
        (column['width'] as num?)?.toDouble() ?? 140,
    ];
    const actionsWidth = 114.0;
    final tableWidth = widths.fold<double>(
      actionsWidth,
      (total, width) => total + width,
    );

    Widget cell({
      required double width,
      required Widget child,
      required bool header,
    }) {
      return Container(
        width: width,
        constraints: BoxConstraints(minHeight: header ? 46 : 100),
        padding: const EdgeInsets.all(8),
        alignment: Alignment.centerLeft,
        decoration: BoxDecoration(
          color: header
              ? t.colorScheme.surfaceContainerHigh
              : t.colorScheme.surfaceContainerLowest,
          border: Border(
            right: BorderSide(color: t.colorScheme.outlineVariant),
            bottom: BorderSide(color: t.colorScheme.outlineVariant),
          ),
        ),
        child: child,
      );
    }

    return Card(
      clipBehavior: Clip.antiAlias,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
            child: Row(
              children: [
                const Icon(Icons.table_rows_outlined, size: 18),
                const SizedBox(width: 8),
                Text(
                  s.count('rfi.editor.rowCount', widget.rows.length),
                  style: t.textTheme.labelLarge?.copyWith(
                    fontWeight: FontWeight.w700,
                  ),
                ),
                const Spacer(),
                Text(
                  s.t('rfi.editor.scrollHint'),
                  style: t.textTheme.bodySmall?.copyWith(
                    color: t.colorScheme.onSurfaceVariant,
                  ),
                ),
              ],
            ),
          ),
          const Divider(height: 1),
          Expanded(
            child: LayoutBuilder(
              builder: (context, constraints) => Scrollbar(
                controller: _horizontal,
                thumbVisibility: true,
                child: SingleChildScrollView(
                  controller: _horizontal,
                  scrollDirection: Axis.horizontal,
                  child: SizedBox(
                    width: tableWidth,
                    height: constraints.maxHeight,
                    child: Column(
                      children: [
                        Row(
                          children: [
                            for (var i = 0; i < columns.length; i++)
                              cell(
                                width: widths[i],
                                header: true,
                                child: Text(
                                  s.t(columns[i]['labelKey'] as String),
                                  style: t.textTheme.labelSmall?.copyWith(
                                    fontWeight: FontWeight.w700,
                                  ),
                                ),
                              ),
                            cell(
                              width: actionsWidth,
                              header: true,
                              child: Text(
                                s.t('rfi.col.actions'),
                                style: t.textTheme.labelSmall?.copyWith(
                                  fontWeight: FontWeight.w700,
                                ),
                              ),
                            ),
                          ],
                        ),
                        Expanded(
                          child: Scrollbar(
                            controller: _vertical,
                            thumbVisibility: true,
                            child: ListView.builder(
                              controller: _vertical,
                              itemCount: widget.rows.length,
                              itemExtent: 100,
                              itemBuilder: (context, rowIndex) {
                                final row = widget.rows[rowIndex];
                                final groupField =
                                    widget.editor['groupField']?.toString() ??
                                    'rfiNo';
                                final groupCount = widget.rows
                                    .where(
                                      (item) =>
                                          item[groupField] == row[groupField],
                                    )
                                    .length;
                                return Row(
                                  children: [
                                    for (var i = 0; i < columns.length; i++)
                                      cell(
                                        width: widths[i],
                                        header: false,
                                        child: _field(columns[i], row),
                                      ),
                                    cell(
                                      width: actionsWidth,
                                      header: false,
                                      child: Row(
                                        mainAxisSize: MainAxisSize.min,
                                        children: [
                                          IconButton(
                                            icon: const Icon(
                                              Icons.add_to_photos_outlined,
                                              size: 18,
                                            ),
                                            tooltip: s.t(
                                              'rfi.action.duplicate',
                                            ),
                                            onPressed: () =>
                                                _duplicate(rowIndex),
                                          ),
                                          IconButton(
                                            icon: const Icon(
                                              Icons.remove_circle_outline,
                                              size: 18,
                                            ),
                                            tooltip: s.t(
                                              'rfi.action.removeDuplicate',
                                            ),
                                            onPressed: groupCount > 1
                                                ? () => _remove(rowIndex)
                                                : null,
                                          ),
                                        ],
                                      ),
                                    ),
                                  ],
                                );
                              },
                            ),
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildCompact(BuildContext context) {
    final s = Strings.of(context);
    final t = Theme.of(context);
    final columns = _columns.where((column) => column['compact'] == true).toList();
    final detailColumns = _columns
        .where((column) => column['detail'] == true)
        .toList();
    final baseWidths = [
      for (final column in columns)
        (column['width'] as num?)?.toDouble() ?? 100,
    ];
    const actionsWidth = 104.0;
    final baseTableWidth = baseWidths.fold<double>(
      actionsWidth,
      (total, width) => total + width,
    );

    return Card(
      clipBehavior: Clip.antiAlias,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 7),
            child: Row(
              children: [
                const Icon(Icons.view_compact_outlined, size: 17),
                const SizedBox(width: 7),
                Text(
                  s.count('rfi.editor.rowCount', widget.rows.length),
                  style: t.textTheme.labelLarge?.copyWith(
                    fontWeight: FontWeight.w700,
                  ),
                ),
                const Spacer(),
                Flexible(
                  child: Text(
                    s.t('rfi.editor.compactHint'),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: t.textTheme.bodySmall?.copyWith(
                      color: t.colorScheme.onSurfaceVariant,
                    ),
                  ),
                ),
              ],
            ),
          ),
          const Divider(height: 1),
          Expanded(
            child: LayoutBuilder(
              builder: (context, constraints) {
                final tableWidth = constraints.maxWidth > baseTableWidth
                    ? constraints.maxWidth
                    : baseTableWidth;
                final extra = constraints.maxWidth > baseTableWidth
                    ? (constraints.maxWidth - baseTableWidth) / columns.length
                    : 0.0;
                final widths = [
                  for (final width in baseWidths) width + extra,
                ];

                Widget cell({
                  required double width,
                  required Widget child,
                  required bool header,
                }) {
                  return Container(
                    width: width,
                    height: header ? 38 : 54,
                    padding: EdgeInsets.symmetric(
                      horizontal: header ? 6 : 4,
                      vertical: 4,
                    ),
                    alignment: Alignment.centerLeft,
                    decoration: BoxDecoration(
                      color: header
                          ? t.colorScheme.surfaceContainerHigh
                          : t.colorScheme.surfaceContainerLowest,
                      border: Border(
                        right: BorderSide(color: t.colorScheme.outlineVariant),
                        bottom: BorderSide(color: t.colorScheme.outlineVariant),
                      ),
                    ),
                    child: child,
                  );
                }

                return Scrollbar(
                  controller: _horizontal,
                  thumbVisibility: tableWidth > constraints.maxWidth,
                  child: SingleChildScrollView(
                    controller: _horizontal,
                    scrollDirection: Axis.horizontal,
                    child: SizedBox(
                      width: tableWidth,
                      height: constraints.maxHeight,
                      child: Column(
                        children: [
                          Row(
                            children: [
                              for (var i = 0; i < columns.length; i++)
                                cell(
                                  width: widths[i],
                                  header: true,
                                  child: Text(
                                    s.t(columns[i]['labelKey'] as String),
                                    maxLines: 2,
                                    overflow: TextOverflow.ellipsis,
                                    style: t.textTheme.labelSmall?.copyWith(
                                      fontWeight: FontWeight.w700,
                                      height: 1.05,
                                    ),
                                  ),
                                ),
                              cell(
                                width: actionsWidth,
                                header: true,
                                child: Text(
                                  s.t('rfi.col.actions'),
                                  style: t.textTheme.labelSmall?.copyWith(
                                    fontWeight: FontWeight.w700,
                                  ),
                                ),
                              ),
                            ],
                          ),
                          Expanded(
                            child: Scrollbar(
                              controller: _vertical,
                              thumbVisibility: true,
                              child: ListView.builder(
                                controller: _vertical,
                                itemCount: widget.rows.length,
                                itemBuilder: (context, rowIndex) {
                                  final row = widget.rows[rowIndex];
                                  final rowId = row['_rowId']?.toString() ?? '$rowIndex';
                                  final expanded = _expandedRows.contains(rowId);
                                  final groupField =
                                      widget.editor['groupField']?.toString() ??
                                      'rfiNo';
                                  final groupCount = widget.rows
                                      .where(
                                        (item) =>
                                            item[groupField] == row[groupField],
                                      )
                                      .length;
                                  return Column(
                                    mainAxisSize: MainAxisSize.min,
                                    children: [
                                      Row(
                                        children: [
                                          for (var i = 0; i < columns.length; i++)
                                            cell(
                                              width: widths[i],
                                              header: false,
                                              child: _field(columns[i], row),
                                            ),
                                          cell(
                                            width: actionsWidth,
                                            header: false,
                                            child: Row(
                                              mainAxisSize: MainAxisSize.min,
                                              children: [
                                                _compactAction(
                                                  icon: expanded
                                                      ? Icons.expand_less
                                                      : Icons.tune_outlined,
                                                  tooltip: s.t(
                                                    'rfi.action.details',
                                                  ),
                                                  onPressed: () => setState(() {
                                                    if (expanded) {
                                                      _expandedRows.remove(rowId);
                                                    } else {
                                                      _expandedRows.add(rowId);
                                                    }
                                                  }),
                                                ),
                                                _compactAction(
                                                  icon: Icons
                                                      .add_to_photos_outlined,
                                                  tooltip: s.t(
                                                    'rfi.action.duplicate',
                                                  ),
                                                  onPressed: () =>
                                                      _duplicate(rowIndex),
                                                ),
                                                _compactAction(
                                                  icon:
                                                      Icons.remove_circle_outline,
                                                  tooltip: s.t(
                                                    'rfi.action.removeDuplicate',
                                                  ),
                                                  onPressed: groupCount > 1
                                                      ? () => _remove(rowIndex)
                                                      : null,
                                                ),
                                              ],
                                            ),
                                          ),
                                        ],
                                      ),
                                      if (expanded)
                                        Container(
                                          width: tableWidth,
                                          padding: const EdgeInsets.fromLTRB(
                                            8,
                                            7,
                                            8,
                                            9,
                                          ),
                                          decoration: BoxDecoration(
                                            color: t.colorScheme
                                                .surfaceContainerLowest,
                                            border: Border(
                                              bottom: BorderSide(
                                                color:
                                                    t.colorScheme.outlineVariant,
                                              ),
                                            ),
                                          ),
                                          child: Row(
                                            crossAxisAlignment:
                                                CrossAxisAlignment.start,
                                            children: [
                                              for (
                                                var i = 0;
                                                i < detailColumns.length;
                                                i++
                                              ) ...[
                                                if (i > 0)
                                                  const SizedBox(width: 8),
                                                Expanded(
                                                  flex: i < 2 ? 3 : 1,
                                                  child: Column(
                                                    crossAxisAlignment:
                                                        CrossAxisAlignment.start,
                                                    children: [
                                                      Padding(
                                                        padding:
                                                            const EdgeInsets.only(
                                                              left: 2,
                                                              bottom: 3,
                                                            ),
                                                        child: Text(
                                                          s.t(
                                                            detailColumns[i]['labelKey']
                                                                as String,
                                                          ),
                                                          style: t
                                                              .textTheme
                                                              .labelSmall
                                                              ?.copyWith(
                                                                fontWeight:
                                                                    FontWeight
                                                                        .w700,
                                                              ),
                                                        ),
                                                      ),
                                                      _field(
                                                        detailColumns[i],
                                                        row,
                                                      ),
                                                    ],
                                                  ),
                                                ),
                                              ],
                                            ],
                                          ),
                                        ),
                                    ],
                                  );
                                },
                              ),
                            ),
                          ),
                        ],
                      ),
                    ),
                  ),
                );
              },
            ),
          ),
        ],
      ),
    );
  }

  Widget _compactAction({
    required IconData icon,
    required String tooltip,
    required VoidCallback? onPressed,
  }) {
    return IconButton(
      icon: Icon(icon, size: 17),
      tooltip: tooltip,
      visualDensity: VisualDensity.compact,
      padding: EdgeInsets.zero,
      constraints: const BoxConstraints.tightFor(width: 32, height: 34),
      onPressed: onPressed,
    );
  }
}

class _AutocompleteCell extends StatefulWidget {
  const _AutocompleteCell({
    super.key,
    required this.value,
    required this.options,
    required this.compact,
    required this.onChanged,
    required this.onSelected,
  });

  final String value;
  final List<String> options;
  final bool compact;
  final ValueChanged<String> onChanged;
  final ValueChanged<String> onSelected;

  @override
  State<_AutocompleteCell> createState() => _AutocompleteCellState();
}

class _AutocompleteCellState extends State<_AutocompleteCell> {
  late final TextEditingController _controller = TextEditingController(
    text: widget.value,
  );
  late final FocusNode _focus = FocusNode();

  @override
  void initState() {
    super.initState();
    _controller.addListener(_changed);
  }

  void _changed() => widget.onChanged(_controller.text);

  @override
  void didUpdateWidget(covariant _AutocompleteCell oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!_focus.hasFocus && widget.value != _controller.text) {
      _controller.value = TextEditingValue(
        text: widget.value,
        selection: TextSelection.collapsed(offset: widget.value.length),
      );
    }
  }

  @override
  void dispose() {
    _controller.removeListener(_changed);
    _controller.dispose();
    _focus.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return RawAutocomplete<String>(
      textEditingController: _controller,
      focusNode: _focus,
      optionsBuilder: (value) {
        final query = value.text.trim().toLowerCase();
        if (query.isEmpty) return widget.options.take(20);
        return widget.options
            .where((option) => option.toLowerCase().contains(query))
            .take(20);
      },
      displayStringForOption: (option) => option,
      onSelected: widget.onSelected,
      fieldViewBuilder: (context, controller, focusNode, onSubmitted) =>
          TextField(
            controller: controller,
            focusNode: focusNode,
            decoration: InputDecoration(
              isDense: true,
              contentPadding: EdgeInsets.symmetric(
                horizontal: widget.compact ? 6 : 9,
                vertical: widget.compact ? 7 : 9,
              ),
              border: const OutlineInputBorder(),
            ),
            onSubmitted: (_) => onSubmitted(),
          ),
      optionsViewBuilder: (context, onSelected, options) => Align(
        alignment: Alignment.topLeft,
        child: Material(
          elevation: 8,
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxHeight: 240, maxWidth: 280),
            child: ListView.builder(
              padding: EdgeInsets.zero,
              shrinkWrap: true,
              itemCount: options.length,
              itemBuilder: (context, index) {
                final option = options.elementAt(index);
                return ListTile(
                  dense: true,
                  title: Text(
                    option,
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                  ),
                  onTap: () => onSelected(option),
                );
              },
            ),
          ),
        ),
      ),
    );
  }
}
