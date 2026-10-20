// EditablePlanTable, ported from frontend/src/App.tsx. The engine describes
// the editor in `plan.meta.editor` (columns, choices, autofill rules, bulk and
// selection options); the rows themselves are owned by the operation screen,
// and every edit goes back through [EditablePlanTable.onChange] as a new list.

import 'package:flutter/material.dart' hide IconButton, Switch;
import 'package:flutter/services.dart';

import '../../l10n/strings.dart';
import '../../theme/app_icons.dart';
import '../../theme/tokens.dart';
import '../screens/compact_date_input.dart';
import '../widgets/metrics.dart';
import '../widgets/widgets.dart';
import 'plan_logic.dart';
import 'plan_parts.dart';

typedef EditorRow = Map<String, Object?>;

enum _FilterMode { all, changes, review }

const double _selectionWidth = 52;
const double _actionsWidth = 132;

/// The editable record table of a Plan preview (`EditablePlanTable`).
class EditablePlanTable extends StatefulWidget {
  const EditablePlanTable({
    super.key,
    required this.editor,
    required this.rows,
    required this.s,
    required this.onChange,
  });

  final Map<String, Object?> editor;
  final List<EditorRow> rows;
  final Strings s;
  final ValueChanged<List<EditorRow>> onChange;

  @override
  State<EditablePlanTable> createState() => _EditablePlanTableState();
}

class _EditablePlanTableState extends State<EditablePlanTable> {
  final Map<String, double> _columnWidths = {};
  Set<String> _selectedIds = {};
  late bool _detailsVisible = widget.editor['detailsDefaultVisible'] == true;
  String _bulkField = '';
  String _bulkValue = '';
  String _searchTerm = '';
  _FilterMode _filterMode = _FilterMode.all;

  Map<String, Object?> get _editor => widget.editor;
  List<EditorRow> get _rows => widget.rows;
  Strings get s => widget.s;

  List<EditorColumn> get _columns => editorColumns(_editor);

  String get _groupField =>
      _editor['groupField'] == null ? 'rfiNo' : jsString(_editor['groupField']);

  String get _widthScope =>
      jsString(_editor['titleKey'] ?? _editor['kind'] ?? 'plan-editor');

  Map<String, Object?> _object(String key) =>
      recordValue(_editor[key]) ?? const {};

  bool _flag(String key) => _editor[key] == true;

  String _rowId(EditorRow row, int index) {
    final id = row['_rowId'];
    if (id != null) return jsString(id);
    final group = row[_groupField];
    return '${group == null ? 'row' : jsString(group)}-$index';
  }

  List<EditorColumn> get _bulkColumns => [
    for (final column in _columns)
      if (column.bulk &&
          column.kind != 'readonly' &&
          (!column.detail || _detailsVisible))
        column,
  ];

  List<EditorColumn> get _visibleColumns => [
    for (final column in _columns)
      if (!column.detail || _detailsVisible) column,
  ];

  double _columnWidth(EditorColumn column) => clampColumnWidth(
    _columnWidths['$_widthScope:${column.key}'] ??
        editorColumnDefaultWidth(column),
    minWidth: editorColumnMinWidth(column),
  );

  @override
  void initState() {
    super.initState();
    final bulk = _bulkColumns;
    _bulkField = bulk.isEmpty ? '' : bulk.first.key;
  }

  @override
  void didUpdateWidget(EditablePlanTable oldWidget) {
    super.didUpdateWidget(oldWidget);
    final valid = {for (var i = 0; i < _rows.length; i++) _rowId(_rows[i], i)};
    final next = _selectedIds.where(valid.contains).toSet();
    if (next.length != _selectedIds.length) _selectedIds = next;
    if (!identical(oldWidget.editor['columns'], _editor['columns'])) {
      _syncBulkField();
    }
  }

  /// The React effect that resets the bulk field when it is no longer offered.
  void _syncBulkField() {
    final bulk = _bulkColumns;
    if (bulk.any((column) => column.key == _bulkField)) return;
    _bulkField = bulk.isEmpty ? '' : bulk.first.key;
    _bulkValue = '';
  }

  List<int> _filteredRowIndices() {
    final search = _searchTerm.trim().toLowerCase();
    final result = <int>[];
    for (var index = 0; index < _rows.length; index++) {
      final row = _rows[index];
      if (_filterMode == _FilterMode.changes) {
        final isChange =
            (jsTruthy(row['proposedStatus']) &&
                row['proposedStatus'] != row['currentStatus']) ||
            row['enabled'] == true;
        if (!isChange) continue;
      } else if (_filterMode == _FilterMode.review) {
        final isReview =
            row['disposition'] == 'manual_review' ||
            jsTruthy(row['reasonKey']) ||
            row['selectable'] == false ||
            row['enabled'] == false;
        if (!isReview) continue;
      }
      if (search.isNotEmpty) {
        final matches = row.values.any(
          (value) =>
              (value is String || value is num) &&
              jsString(value).toLowerCase().contains(search),
        );
        if (!matches) continue;
      }
      result.add(index);
    }
    return result;
  }

  String _choiceLabel(String field, String value, [EditorColumn? column]) {
    final fieldLabels = recordValue(_object('choiceLabels')[field]);
    final key = fieldLabels?[value];
    if (key is String) return s.t(key);
    return column != null ? displayEditorValue(column, value) : value;
  }

  List<String> _choicesFor(EditorColumn column, EditorRow row) {
    final direct = _object('choices')[column.key];
    if (direct is List) return [for (final value in direct) jsString(value)];
    final config = recordValue(_object('choicesByValue')[column.key]);
    if (config == null) return const [];
    final values = recordValue(config['values']);
    final dependency = config['dependsOn'] == null
        ? ''
        : jsString(config['dependsOn']);
    if (values == null) return const [];
    final dependencyValue = row[dependency];
    final options =
        values[dependencyValue == null ? '' : jsString(dependencyValue)];
    return options is List ? [for (final o in options) jsString(o)] : const [];
  }

  List<String> _suggestions(String field) {
    final raw = _object('autocomplete')[field];
    return raw is List ? [for (final value in raw) jsString(value)] : const [];
  }

  static String _ruleText(Map<String, Object?> rule, String key) =>
      rule.containsKey(key) ? jsString(rule[key]) : 'undefined';

  void _applyValue(EditorRow row, String key, Object value) {
    row[key] = value;
    final autoFill = recordValue(_editor['autoFill']);
    if (autoFill != null && autoFill['field'] == key) {
      final values = recordValue(autoFill['values']);
      final preset = recordValue(values?[jsString(value)]);
      if (preset != null) row.addAll(preset);
    }
    final regexFill = _editor['regexFill'];
    if (regexFill is List) {
      for (final rawRule in regexFill) {
        final rule = recordValue(rawRule);
        if (rule == null || rule['source'] != key) continue;
        try {
          final pattern = RegExp(
            _ruleText(rule, 'pattern'),
            caseSensitive: false,
          );
          if (pattern.hasMatch(jsString(value))) {
            row[_ruleText(rule, 'target')] = rule['value'] == null
                ? ''
                : jsString(rule['value']);
          }
        } on FormatException {
          // Invalid server-side presentation hints never block row editing.
        }
      }
    }
  }

  void _update(int index, String key, Object value) {
    final next = [for (final row in _rows) EditorRow.of(row)];
    _applyValue(next[index], key, value);
    widget.onChange(next);
  }

  void _applyBulk() {
    if (_bulkField.isEmpty || _selectedIds.isEmpty) return;
    final isBool = _bulkColumns.any(
      (column) => column.key == _bulkField && column.kind == 'bool',
    );
    final Object nextValue = isBool ? _bulkValue == 'true' : _bulkValue;
    final next = <EditorRow>[];
    for (var index = 0; index < _rows.length; index++) {
      final clone = EditorRow.of(_rows[index]);
      if (_selectedIds.contains(_rowId(_rows[index], index))) {
        _applyValue(clone, _bulkField, nextValue);
      }
      next.add(clone);
    }
    widget.onChange(next);
  }

  void _selectAll(List<int> filtered) => setState(
    () => _selectedIds = {for (final i in filtered) _rowId(_rows[i], i)},
  );

  void _selectNone() => setState(() => _selectedIds = {});

  void _toggleRow(String id) => setState(() {
    final next = {..._selectedIds};
    if (!next.remove(id)) next.add(id);
    _selectedIds = next;
  });

  void _selectGroup(String field, Object? value) => setState(() {
    _selectedIds = {
      for (var i = 0; i < _rows.length; i++)
        if (_rows[i][field] == value) _rowId(_rows[i], i),
    };
  });

  void _duplicate(int index) {
    final source = _rows[index];
    final group = source[_groupField];
    var insertAt = index + 1;
    while (insertAt < _rows.length && _rows[insertAt][_groupField] == group) {
      insertAt += 1;
    }
    final base = source['_rowId'] ?? group;
    final clone = EditorRow.of(source)
      ..['_rowId'] =
          '${base == null ? 'undefined' : jsString(base)}-copy-'
          '${DateTime.now().millisecondsSinceEpoch}';
    widget.onChange([..._rows]..insert(insertAt, clone));
  }

  void _remove(int index) {
    final group = _rows[index][_groupField];
    if (_rows.where((row) => row[_groupField] == group).length <= 1) return;
    widget.onChange([
      for (var i = 0; i < _rows.length; i++)
        if (i != index) _rows[i],
    ]);
  }

  void _move(int index, int direction) {
    final target = index + direction;
    if (target < 0 || target >= _rows.length) return;
    final next = [..._rows];
    final swap = next[index];
    next[index] = next[target];
    next[target] = swap;
    widget.onChange(next);
  }

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final rows = _rows;
    final compact = _editor['layout'] == 'compact';
    final groupField = _groupField;
    final filtered = _filteredRowIndices();
    final bulkColumns = _bulkColumns;
    final visibleColumns = _visibleColumns;
    final selectionEnabled = _flag('selectionEnabled');
    final hasRowActions = _flag('reorderable') || _flag('duplicateEnabled');
    final tableWidth =
        (selectionEnabled ? _selectionWidth : 0) +
        visibleColumns.fold<double>(0, (sum, c) => sum + _columnWidth(c)) +
        (hasRowActions ? _actionsWidth : 0);
    final selectedRows = [
      for (var i = 0; i < rows.length; i++)
        if (_selectedIds.contains(_rowId(rows[i], i))) rows[i],
    ];

    final rawGroupFields = _editor['selectionGroupFields'];
    final selectionGroupFields = rawGroupFields is List
        ? [for (final f in rawGroupFields) jsString(f)]
        : const <String>[];
    final groupChoices = <(String, String)>[];
    for (final field in selectionGroupFields) {
      final values = {
        for (final row in rows) row[field] == null ? '' : jsString(row[field]),
      }.where((value) => value.isNotEmpty).toList();
      if (values.length <= 12) {
        groupChoices.addAll([for (final value in values) (field, value)]);
      }
    }

    EditorColumn? bulkColumn;
    for (final column in bulkColumns) {
      if (column.key == _bulkField) bulkColumn = column;
    }
    final choiceRows = selectedRows.isNotEmpty
        ? selectedRows
        : (rows.isNotEmpty ? [rows.first] : const <EditorRow>[]);
    var bulkChoiceValues = <String>[];
    if (bulkColumn != null) {
      for (var i = 0; i < choiceRows.length; i++) {
        final values = _choicesFor(bulkColumn, choiceRows[i]);
        bulkChoiceValues = i == 0
            ? values
            : bulkChoiceValues.where(values.contains).toList();
      }
    }
    final title = s.t(
      _editor['titleKey'] == null
          ? 'rfi.editor.title'
          : jsString(_editor['titleKey']),
    );

    final heading = Row(
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
            child: Icon(
              AppIcons.tableProperties,
              size: remPx(context, 1.05),
              color: t.text,
            ),
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
              Text(
                s.t(
                  _editor['rowCountKey'] == null
                      ? 'rfi.editor.rowCount'
                      : jsString(_editor['rowCountKey']),
                  {'count': rows.length},
                ),
                style: planMuted(context),
              ),
            ],
          ),
        ),
        if (_flag('detailsEnabled'))
          DfButton(
            variant: DfButtonVariant.ghost,
            icon: Icon(_detailsVisible ? AppIcons.eyeOff : AppIcons.eye),
            onPressed: () => setState(() {
              _detailsVisible = !_detailsVisible;
              _syncBulkField();
            }),
            child: Text(
              s.t(
                _detailsVisible
                    ? 'editor.action.hideDetails'
                    : 'editor.action.showDetails',
              ),
            ),
          ),
      ],
    );

    Widget filterTab(_FilterMode mode, String key) => PlanTextButton(
      active: _filterMode == mode,
      selected: _filterMode == mode,
      onPressed: () => setState(() => _filterMode = mode),
      child: Text(s.t(key)),
    );

    final filterBar = Wrap(
      spacing: 10,
      runSpacing: 8,
      crossAxisAlignment: WrapCrossAlignment.center,
      children: [
        SizedBox(
          width: 260,
          child: PlanTextInput(
            label: s.t('filter.searchPlaceholder'),
            placeholder: s.t('filter.searchPlaceholder'),
            search: true,
            value: _searchTerm,
            onChanged: (value) => setState(() => _searchTerm = value),
          ),
        ),
        Wrap(
          spacing: 6,
          runSpacing: 6,
          children: [
            filterTab(_FilterMode.all, 'filter.all'),
            filterTab(_FilterMode.changes, 'filter.changes'),
            filterTab(_FilterMode.review, 'filter.manualReview'),
          ],
        ),
        if (_searchTerm.isNotEmpty || _filterMode != _FilterMode.all)
          Text(
            s.t('table.filterCount', {
              'filtered': filtered.length,
              'total': rows.length,
            }),
            style: planMuted(context),
          ),
      ],
    );

    Widget? bulkEditor;
    if (_flag('bulkEnabled')) {
      final Widget valueControl;
      if (bulkColumn?.kind == 'bool') {
        valueControl = PlanSelect(
          label: s.t('editor.bulk.value'),
          value: _bulkValue,
          options: [
            ('', s.t('editor.bulk.choose')),
            ('true', s.t('editor.bool.true')),
            ('false', s.t('editor.bool.false')),
          ],
          onChanged: (value) => setState(() => _bulkValue = value),
        );
      } else if (bulkColumn?.kind == 'choice') {
        valueControl = PlanSelect(
          label: s.t('editor.bulk.value'),
          value: _bulkValue,
          options: [
            ('', s.t('editor.bulk.choose')),
            for (final choice in bulkChoiceValues)
              (choice, _choiceLabel(_bulkField, choice, bulkColumn)),
          ],
          onChanged: (value) => setState(() => _bulkValue = value),
        );
      } else if (bulkColumn?.kind == 'compactDate') {
        valueControl = CompactDateInput(
          key: ValueKey(_bulkField),
          value: _bulkValue,
          label: s.t(bulkColumn!.labelKey),
          s: s,
          onChange: (value) => setState(() => _bulkValue = value),
        );
      } else {
        valueControl = PlanTextInput(
          key: ValueKey('bulk-$_bulkField'),
          label: s.t('editor.bulk.value'),
          value: _bulkValue,
          number: bulkColumn?.kind == 'int',
          suggestions: _suggestions(_bulkField),
          onChanged: (value) => setState(() => _bulkValue = value),
        );
      }
      bulkEditor = Semantics(
        container: true,
        explicitChildNodes: true,
        label: s.t('editor.bulk.title'),
        child: Container(
          padding: const EdgeInsets.all(10),
          decoration: BoxDecoration(
            color: t.surface2,
            borderRadius: BorderRadius.circular(8),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisSize: MainAxisSize.min,
            children: [
              Wrap(
                spacing: 6,
                runSpacing: 6,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  Text(
                    s.t('editor.selectedCount', {'count': _selectedIds.length}),
                    style: planStrong(context, rem: 0.8),
                  ),
                  PlanTextButton(
                    onPressed: () => _selectAll(filtered),
                    child: Text(s.t('editor.selectAll')),
                  ),
                  PlanTextButton(
                    onPressed: _selectNone,
                    child: Text(s.t('editor.selectNone')),
                  ),
                  for (final (field, value) in groupChoices)
                    PlanTextButton(
                      tooltip: compact && field == groupField ? value : null,
                      onPressed: () => _selectGroup(field, value),
                      child: Text(
                        compact && field == groupField
                            ? (trailingDigits(value) ?? value)
                            : value,
                      ),
                    ),
                ],
              ),
              const SizedBox(height: 8),
              Wrap(
                spacing: 8,
                runSpacing: 8,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  SizedBox(
                    width: 220,
                    child: PlanSelect(
                      label: s.t('editor.bulk.field'),
                      value: _bulkField,
                      options: [
                        for (final column in bulkColumns)
                          (column.key, s.t(column.labelKey)),
                      ],
                      onChanged: (value) => setState(() {
                        _bulkField = value;
                        _bulkValue = '';
                      }),
                    ),
                  ),
                  SizedBox(width: 260, child: valueControl),
                  DfButton(
                    variant: DfButtonVariant.secondary,
                    onPressed:
                        _selectedIds.isEmpty ||
                            _bulkField.isEmpty ||
                            _bulkValue.isEmpty
                        ? null
                        : _applyBulk,
                    child: Text(s.t('editor.bulk.apply')),
                  ),
                ],
              ),
            ],
          ),
        ),
      );
    }

    final allSelected =
        filtered.isNotEmpty &&
        filtered.every((i) => _selectedIds.contains(_rowId(rows[i], i)));
    final header = Container(
      decoration: BoxDecoration(
        color: t.surface2,
        border: Border(bottom: BorderSide(color: t.line)),
      ),
      child: Row(
        children: [
          if (selectionEnabled)
            SizedBox(
              width: _selectionWidth,
              height: 36,
              child: Center(
                child: PlanCheckbox(
                  label: s.t('editor.selectAll'),
                  value: allSelected,
                  onChanged: (checked) =>
                      checked ? _selectAll(filtered) : _selectNone(),
                ),
              ),
            ),
          for (final column in visibleColumns)
            _headerCell(context, column, _columnWidth(column)),
          if (hasRowActions)
            Semantics(
              label: s.t('editor.rowActions'),
              child: const SizedBox(width: _actionsWidth, height: 36),
            ),
        ],
      ),
    );

    final body = filtered.isEmpty
        ? Padding(
            padding: const EdgeInsets.all(18),
            child: Text(
              s.t('plan.empty'),
              textAlign: TextAlign.center,
              style: planMuted(context),
            ),
          )
        : ListView.builder(
            itemCount: filtered.length,
            itemBuilder: (context, position) => _buildRow(
              context,
              filtered[position],
              visibleColumns,
              selectionEnabled,
              hasRowActions,
            ),
          );

    return PlanCard(
      semanticsLabel: title,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          heading,
          const SizedBox(height: 12),
          filterBar,
          if (bulkEditor != null) ...[const SizedBox(height: 10), bulkEditor],
          const SizedBox(height: 10),
          AdjustableDataRegion(
            s: s,
            label: title,
            child: Scrollbar(
              child: SingleChildScrollView(
                scrollDirection: Axis.horizontal,
                child: SizedBox(
                  width: tableWidth,
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      header,
                      Expanded(child: body),
                    ],
                  ),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }

  Widget _headerCell(BuildContext context, EditorColumn column, double width) {
    final label = s.t(column.labelKey);
    void setWidth(double next) =>
        setState(() => _columnWidths['$_widthScope:${column.key}'] = next);
    return SizedBox(
      width: width,
      height: 36,
      child: Stack(
        children: [
          Positioned.fill(
            child: Tooltip(
              message: label,
              child: Padding(
                padding: const EdgeInsets.fromLTRB(8, 9, 14, 9),
                child: Semantics(
                  header: true,
                  child: Text(
                    label,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: planStrong(context, rem: 0.74),
                  ),
                ),
              ),
            ),
          ),
          Positioned(
            top: 0,
            bottom: 0,
            right: 0,
            child: ColumnResizeHandle(
              width: width,
              label: label,
              minWidth: editorColumnMinWidth(column),
              s: s,
              onWidthPreview: setWidth,
              onWidthChange: setWidth,
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildRow(
    BuildContext context,
    int rowIndex,
    List<EditorColumn> visibleColumns,
    bool selectionEnabled,
    bool hasRowActions,
  ) {
    final t = DocFlowTokens.of(context);
    final rows = _rows;
    final row = rows[rowIndex];
    final groupField = _groupField;
    final sheet = row['_sheet'] == null ? '' : jsString(row['_sheet']);
    final groupCount = rows
        .where((item) => item[groupField] == row[groupField])
        .length;
    final id = _rowId(row, rowIndex);
    final selected = _selectedIds.contains(id);

    final cells = <Widget>[
      if (selectionEnabled)
        SizedBox(
          width: _selectionWidth,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              PlanCheckbox(
                label: s.t('editor.selectRow', {'row': rowIndex + 1}),
                value: selected,
                onChanged: (_) => _toggleRow(id),
              ),
              if (groupCount > 1)
                PlanTextButton(
                  tooltip: s.t('editor.selectGroup'),
                  semanticsLabel: s.t('editor.selectGroup'),
                  onPressed: () => _selectGroup(groupField, row[groupField]),
                  child: Text('$groupCount'),
                ),
            ],
          ),
        ),
      for (final column in visibleColumns)
        Container(
          width: _columnWidth(column),
          padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 5),
          alignment: column.kind == 'bool'
              ? Alignment.center
              : AlignmentDirectional.centerStart,
          child: _cell(context, column, row, rowIndex, sheet),
        ),
      if (hasRowActions)
        SizedBox(
          width: _actionsWidth,
          child: Wrap(
            spacing: 4,
            runSpacing: 4,
            alignment: WrapAlignment.center,
            children: [
              if (_flag('reorderable')) ...[
                PlanIconButton(
                  icon: AppIcons.arrowUp,
                  label: s.t('action.moveUp'),
                  onPressed: rowIndex == 0 ? null : () => _move(rowIndex, -1),
                ),
                PlanIconButton(
                  icon: AppIcons.arrowDown,
                  label: s.t('action.moveDown'),
                  onPressed: rowIndex == rows.length - 1
                      ? null
                      : () => _move(rowIndex, 1),
                ),
              ],
              if (_flag('duplicateEnabled')) ...[
                PlanIconButton(
                  icon: AppIcons.copy,
                  label: s.t('rfi.action.duplicate'),
                  onPressed: () => _duplicate(rowIndex),
                ),
                PlanIconButton(
                  icon: AppIcons.trash,
                  label: s.t('rfi.action.removeDuplicate'),
                  onPressed: groupCount <= 1 ? null : () => _remove(rowIndex),
                ),
              ],
            ],
          ),
        ),
    ];

    return _EditorRowFrame(
      key: ValueKey(id),
      selected: selected,
      onSpace: () {
        if (_flag('selectionEnabled')) {
          _toggleRow(id);
        } else if (row['enabled'] is bool && row['selectable'] != false) {
          _update(rowIndex, 'enabled', !(row['enabled']! as bool));
        }
      },
      child: Container(
        decoration: BoxDecoration(
          color: selected ? t.surface2 : null,
          border: Border(bottom: BorderSide(color: t.line)),
        ),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.center,
          children: cells,
        ),
      ),
    );
  }

  Widget _cell(
    BuildContext context,
    EditorColumn column,
    EditorRow row,
    int rowIndex,
    String sheet,
  ) {
    final t = DocFlowTokens.of(context);
    final sheets = column.sheets;
    if (sheets != null && !sheets.contains(sheet)) {
      return Text('—', style: planMuted(context));
    }
    final raw = row[column.key];
    final text = raw == null ? '' : jsString(raw);
    final String displayText;
    if (column.display == 'localizedKey') {
      displayText = s.t(text);
    } else if (column.key == 'status' && text.isNotEmpty) {
      displayText = s.t('print.status.$text');
    } else if (column.key == 'worksheetVisibility' && text.isNotEmpty) {
      displayText = s.t('print.worksheet.visibility.$text');
    } else {
      displayText = displayEditorValue(column, text);
    }
    final label = s.t(column.labelKey);

    switch (column.kind) {
      case 'readonly':
        return planTooltip(
          displayText.isNotEmpty ? displayText : text,
          Text(
            displayText,
            maxLines: 2,
            overflow: TextOverflow.ellipsis,
            style: planBody(context, rem: 0.78),
          ),
        );
      case 'bool':
        final disabledField = column.disabledWhenField;
        final disabled =
            disabledField != null &&
            row[disabledField] == column.disabledWhenEquals;
        return PlanCheckbox(
          label: label,
          value: jsTruthy(raw),
          onChanged: disabled
              ? null
              : (checked) => _update(rowIndex, column.key, checked),
        );
      case 'choice':
        return PlanSelect(
          label: label,
          tooltip: text,
          value: text,
          options: [
            for (final choice in _choicesFor(column, row))
              (choice, _choiceLabel(column.key, choice, column)),
          ],
          onChanged: (value) => _update(rowIndex, column.key, value),
        );
      case 'compactDate':
        return CompactDateInput(
          value: text,
          label: label,
          s: s,
          onChange: (value) => _update(rowIndex, column.key, value),
        );
      case 'multiline':
        return PlanTextInput(
          label: label,
          value: text,
          multiline: true,
          onChanged: (value) => _update(rowIndex, column.key, value),
        );
    }

    final isScanCode = _editor['kind'] == 'rfiScan' && column.key == 'code';
    String? errorKey;
    if (isScanCode) {
      final duplicate = [
        for (var i = 0; i < _rows.length; i++)
          if (i != rowIndex && _rows[i]['code'] == row['code']) i,
      ].isNotEmpty;
      if (!isRfiCodeCandidate(row['code'])) {
        errorKey = 'error.rfi.fastScan.codeInvalid';
      } else if (duplicate) {
        errorKey = 'error.rfi.fastScan.targetExists';
      }
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        PlanTextInput(
          label: label,
          value: text,
          number: column.kind == 'int',
          plain: isScanCode,
          invalid: errorKey != null,
          suggestions: _suggestions(column.key),
          onChanged: (value) => _update(rowIndex, column.key, value),
        ),
        if (errorKey != null)
          Semantics(
            liveRegion: true,
            child: Padding(
              padding: const EdgeInsets.only(top: 3),
              child: Text(
                s.t(errorKey),
                style: TextStyle(
                  fontSize: remPx(context, 0.7),
                  color: t.danger,
                ),
              ),
            ),
          ),
      ],
    );
  }
}

/// A focusable table row: Space on the row itself (not on a control inside
/// it) calls [onSpace], as the React `onKeyDown` did.
class _EditorRowFrame extends StatefulWidget {
  const _EditorRowFrame({
    super.key,
    required this.selected,
    required this.onSpace,
    required this.child,
  });

  final bool selected;
  final VoidCallback onSpace;
  final Widget child;

  @override
  State<_EditorRowFrame> createState() => _EditorRowFrameState();
}

class _EditorRowFrameState extends State<_EditorRowFrame> {
  final FocusNode _node = FocusNode();

  @override
  void dispose() {
    _node.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return Focus(
      focusNode: _node,
      onFocusChange: (_) => setState(() {}),
      onKeyEvent: (node, event) {
        if (event is KeyDownEvent &&
            event.logicalKey == LogicalKeyboardKey.space &&
            node.hasPrimaryFocus) {
          widget.onSpace();
          return KeyEventResult.handled;
        }
        return KeyEventResult.ignored;
      },
      child: Semantics(
        selected: widget.selected,
        child: DecoratedBox(
          position: DecorationPosition.foreground,
          decoration: BoxDecoration(
            border: _node.hasPrimaryFocus
                ? Border.all(color: t.text, width: 2)
                : null,
          ),
          child: widget.child,
        ),
      ),
    );
  }
}
