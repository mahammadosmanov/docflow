import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';

import '../../core/l10n/strings.dart';
import '../../core/operations/models.dart';
import '../theme/responsive.dart';

bool isParamVisible(ParamDef param, Map<String, dynamic> values) {
  return param.visibleWhen.entries.every(
    (condition) => values[condition.key] == condition.value,
  );
}

/// Semadan uretilen form. Bu dosya operation'a ozel HICBIR SEY bilmez.
/// `if (opId == '...')` yazmak mimariyi bozar (docs/ARCHITECTURE.md sizinti testi).
/// Yeni ParamKind eklemek = buraya tek `case`; operation dosyalarina dokunma yok.
class SchemaForm extends StatefulWidget {
  const SchemaForm({
    super.key,
    required this.params,
    required this.values,
    required this.onChanged,
  });

  final List<ParamDef> params;
  final Map<String, dynamic> values;
  final VoidCallback onChanged;

  @override
  State<SchemaForm> createState() => _SchemaFormState();
}

class _SchemaFormState extends State<SchemaForm> {
  @override
  void initState() {
    super.initState();
    for (final p in widget.params) {
      widget.values.putIfAbsent(p.name, () => p.defaultValue);
    }
  }

  void _set(String name, dynamic value) {
    widget.values[name] = value;
    widget.onChanged();
    setState(() {});
  }

  @override
  Widget build(BuildContext context) {
    final s = Strings.of(context);
    final visible = widget.params
        .where((p) => isParamVisible(p, widget.values))
        .toList();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        for (var i = 0; i < visible.length; i++) ...[
          if (i > 0) const SizedBox(height: 18),
          _field(context, s, visible[i]),
        ],
      ],
    );
  }

  Widget _field(BuildContext context, Strings s, ParamDef p) {
    switch (p.kind) {
      case ParamKind.folder:
        return _pathField(s, p, pickFolder: true);
      case ParamKind.file:
        return _pathField(s, p, pickFolder: false);
      case ParamKind.files:
        return _filesField(s, p);
      case ParamKind.bool:
        final t = Theme.of(context);
        return Material(
          color: t.colorScheme.surfaceContainerLow,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(10),
            side: BorderSide(color: t.colorScheme.outlineVariant),
          ),
          clipBehavior: Clip.antiAlias,
          child: SwitchListTile(
            contentPadding: const EdgeInsets.symmetric(
              horizontal: 14,
              vertical: 4,
            ),
            shape: RoundedRectangleBorder(
              borderRadius: BorderRadius.circular(10),
            ),
            title: Text(
              s.t(p.labelKey),
              style: t.textTheme.bodyMedium?.copyWith(
                fontWeight: FontWeight.w600,
              ),
            ),
            subtitle: p.helpKey == null
                ? null
                : Text(
                    s.t(p.helpKey!),
                    style: t.textTheme.bodySmall?.copyWith(
                      color: t.colorScheme.onSurfaceVariant,
                    ),
                  ),
            value: (widget.values[p.name] as bool?) ?? false,
            onChanged: (v) => _set(p.name, v),
          ),
        );
      case ParamKind.choice:
        return _labeled(
          s,
          p,
          DropdownButtonFormField<String>(
            key: ValueKey(p.name),
            initialValue: widget.values[p.name] as String?,
            items: [
              for (final c in p.choices)
                DropdownMenuItem(
                  value: c,
                  child: Text(
                    p.choiceLabels[c] == null ? c : s.t(p.choiceLabels[c]!),
                  ),
                ),
            ],
            onChanged: (v) => _set(p.name, v),
          ),
        );
      case ParamKind.int:
        return _labeled(
          s,
          p,
          TextFormField(
            key: ValueKey(p.name),
            initialValue: widget.values[p.name]?.toString() ?? '',
            keyboardType: TextInputType.number,
            onChanged: (v) => _set(p.name, int.tryParse(v)),
          ),
        );
      case ParamKind.longText:
        return _labeled(
          s,
          p,
          TextFormField(
            key: ValueKey(p.name),
            initialValue: widget.values[p.name]?.toString() ?? '',
            minLines: 7,
            maxLines: 14,
            keyboardType: TextInputType.multiline,
            decoration: const InputDecoration(alignLabelWithHint: true),
            onChanged: (v) => _set(p.name, v),
          ),
        );
      case ParamKind.secret:
        return _labeled(
          s,
          p,
          TextFormField(
            key: ValueKey(p.name),
            initialValue: widget.values[p.name]?.toString() ?? '',
            obscureText: true,
            enableSuggestions: false,
            autocorrect: false,
            onChanged: (v) => _set(p.name, v),
          ),
        );
      case ParamKind.sheetCounts:
        return _sheetCountsField(s, p);
      case ParamKind.text:
      case ParamKind.unknown:
        return _labeled(
          s,
          p,
          TextFormField(
            initialValue: widget.values[p.name]?.toString() ?? '',
            onChanged: (v) => _set(p.name, v),
          ),
        );
    }
  }

  Widget _sheetCountsField(Strings s, ParamDef p) {
    final raw = widget.values[p.name];
    final counts = <String, int>{
      for (final choice in p.choices)
        choice:
            ((raw is Map ? raw[choice] : null) as num?)?.toInt().clamp(0, 100) ??
            0,
    };
    final total = counts.values.fold<int>(0, (sum, value) => sum + value);
    final t = Theme.of(context);

    void update(String sheet, int count) {
      final next = <String, int>{...counts};
      next[sheet] = count.clamp(0, 100);
      _set(p.name, next);
    }

    return _labeled(
      s,
      p,
      Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              for (final sheet in p.choices)
                SizedBox(
                  width: 164,
                  child: Material(
                    key: ValueKey('sheet-count-$sheet'),
                    color: counts[sheet]! > 0
                        ? t.colorScheme.surfaceContainerHigh
                        : t.colorScheme.surfaceContainerLowest,
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(10),
                      side: BorderSide(
                        color: counts[sheet]! > 0
                            ? t.colorScheme.onSurface
                            : t.colorScheme.outlineVariant,
                        width: counts[sheet]! > 0 ? 1.3 : 1,
                      ),
                    ),
                    child: Padding(
                      padding: const EdgeInsets.fromLTRB(8, 6, 6, 6),
                      child: Row(
                        children: [
                          Checkbox(
                            key: ValueKey('sheet-enabled-$sheet'),
                            visualDensity: VisualDensity.compact,
                            value: counts[sheet]! > 0,
                            onChanged: (enabled) =>
                                update(sheet, enabled == true ? 1 : 0),
                          ),
                          Expanded(
                            child: Text(
                              sheet,
                              style: t.textTheme.labelLarge?.copyWith(
                                fontWeight: FontWeight.w800,
                              ),
                            ),
                          ),
                          IconButton(
                            key: ValueKey('sheet-minus-$sheet'),
                            tooltip: s.t('action.decrease'),
                            visualDensity: VisualDensity.compact,
                            constraints: const BoxConstraints.tightFor(
                              width: 28,
                              height: 30,
                            ),
                            padding: EdgeInsets.zero,
                            onPressed: counts[sheet]! > 0
                                ? () => update(sheet, counts[sheet]! - 1)
                                : null,
                            icon: const Icon(Icons.remove, size: 16),
                          ),
                          SizedBox(
                            width: 25,
                            child: Text(
                              '${counts[sheet]}',
                              key: ValueKey('sheet-value-$sheet'),
                              textAlign: TextAlign.center,
                              style: t.textTheme.labelLarge?.copyWith(
                                fontFeatures: const [
                                  FontFeature.tabularFigures(),
                                ],
                                fontWeight: FontWeight.w700,
                              ),
                            ),
                          ),
                          IconButton(
                            key: ValueKey('sheet-plus-$sheet'),
                            tooltip: s.t('action.increase'),
                            visualDensity: VisualDensity.compact,
                            constraints: const BoxConstraints.tightFor(
                              width: 28,
                              height: 30,
                            ),
                            padding: EdgeInsets.zero,
                            onPressed: counts[sheet]! < 100
                                ? () => update(sheet, counts[sheet]! + 1)
                                : null,
                            icon: const Icon(Icons.add, size: 16),
                          ),
                        ],
                      ),
                    ),
                  ),
                ),
            ],
          ),
          const SizedBox(height: 7),
          Text(
            s.count('rfi.sheetCounts.total', total),
            textAlign: TextAlign.end,
            style: t.textTheme.bodySmall?.copyWith(
              color: t.colorScheme.onSurfaceVariant,
              fontWeight: FontWeight.w600,
            ),
          ),
        ],
      ),
    );
  }

  Widget _labeled(Strings s, ParamDef p, Widget field) {
    final t = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Text(
              s.t(p.labelKey),
              style: t.textTheme.bodyMedium?.copyWith(
                fontWeight: FontWeight.w600,
              ),
            ),
            // Zorunlu alan isareti — sema soyluyor, form gosteriyor.
            if (p.required)
              Padding(
                padding: const EdgeInsets.only(left: 4),
                child: Text(
                  '*',
                  style: t.textTheme.bodyMedium?.copyWith(
                    color: t.colorScheme.onSurfaceVariant,
                  ),
                ),
              ),
          ],
        ),
        if (p.helpKey != null)
          Padding(
            padding: const EdgeInsets.only(top: 2),
            child: Text(
              s.t(p.helpKey!),
              style: t.textTheme.bodySmall?.copyWith(
                color: t.colorScheme.onSurfaceVariant,
              ),
            ),
          ),
        const SizedBox(height: 8),
        field,
      ],
    );
  }

  Widget _pathField(Strings s, ParamDef p, {required bool pickFolder}) {
    final value = widget.values[p.name] as String?;
    final ff = FormFactor.of(context);
    final field = TextFormField(
      key: ValueKey('${p.name}:$value'),
      initialValue: value ?? '',
      decoration: InputDecoration(
        isDense: true,
        prefixIcon: Icon(
          pickFolder ? Icons.folder_outlined : Icons.description_outlined,
          size: 18,
        ),
        hintText: s.t(pickFolder ? 'param.hint.folder' : 'param.hint.file'),
      ),
      onChanged: (v) => widget.values[p.name] = v,
    );
    final browse = OutlinedButton.icon(
      icon: Icon(pickFolder ? Icons.folder_open : Icons.attach_file, size: 18),
      label: Text(s.t('action.browse')),
      onPressed: () async {
        if (pickFolder) {
          final dir = await FilePicker.platform.getDirectoryPath();
          if (dir != null) _set(p.name, dir);
        } else {
          final res = await FilePicker.platform.pickFiles();
          if (res != null && res.files.single.path != null) {
            _set(p.name, res.files.single.path);
          }
        }
      },
    );

    // Dar pencerede yol alani ile Gozat alt alta — yol kirpilmasin.
    return _labeled(
      s,
      p,
      ff.isCompact
          ? Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [field, const SizedBox(height: 8), browse],
            )
          : Row(
              children: [
                Expanded(child: field),
                const SizedBox(width: 8),
                browse,
              ],
            ),
    );
  }

  Widget _filesField(Strings s, ParamDef p) {
    final paths = (widget.values[p.name] as List? ?? const [])
        .map((item) => item.toString())
        .toList();
    final t = Theme.of(context);

    void update(List<String> next) => _set(p.name, next);

    return _labeled(
      s,
      p,
      Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Expanded(
                child: OutlinedButton.icon(
                  icon: const Icon(Icons.playlist_add, size: 18),
                  label: Text(s.t('action.addFiles')),
                  onPressed: () async {
                    final result = await FilePicker.platform.pickFiles(
                      allowMultiple: true,
                    );
                    if (result == null) return;
                    final next = [...paths];
                    for (final file in result.files) {
                      final path = file.path;
                      if (path != null && !next.contains(path)) next.add(path);
                    }
                    update(next);
                  },
                ),
              ),
              if (paths.isNotEmpty) ...[
                const SizedBox(width: 8),
                TextButton(
                  onPressed: () => update([]),
                  child: Text(s.t('action.clear')),
                ),
              ],
            ],
          ),
          const SizedBox(height: 8),
          Container(
            constraints: const BoxConstraints(maxHeight: 280),
            decoration: BoxDecoration(
              border: Border.all(color: t.colorScheme.outlineVariant),
              borderRadius: BorderRadius.circular(10),
              color: t.colorScheme.surfaceContainerLowest,
            ),
            child: paths.isEmpty
                ? Padding(
                    padding: const EdgeInsets.all(18),
                    child: Text(
                      s.t('param.hint.files'),
                      textAlign: TextAlign.center,
                      style: t.textTheme.bodySmall?.copyWith(
                        color: t.colorScheme.onSurfaceVariant,
                      ),
                    ),
                  )
                : ListView.separated(
                    shrinkWrap: true,
                    itemCount: paths.length,
                    separatorBuilder: (_, _) => const Divider(height: 1),
                    itemBuilder: (_, index) {
                      final path = paths[index];
                      final name = path.split(RegExp(r'[/\\]')).last;
                      return ListTile(
                        dense: true,
                        leading: SizedBox(
                          width: 28,
                          child: Text(
                            '${index + 1}',
                            textAlign: TextAlign.center,
                            style: t.textTheme.labelMedium?.copyWith(
                              fontWeight: FontWeight.w700,
                            ),
                          ),
                        ),
                        title: Text(
                          name,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                        ),
                        subtitle: Text(
                          path,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                        ),
                        trailing: Row(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            IconButton(
                              tooltip: s.t('action.moveUp'),
                              onPressed: index == 0
                                  ? null
                                  : () {
                                      final next = [...paths];
                                      final item = next.removeAt(index);
                                      next.insert(index - 1, item);
                                      update(next);
                                    },
                              icon: const Icon(Icons.arrow_upward, size: 17),
                            ),
                            IconButton(
                              tooltip: s.t('action.moveDown'),
                              onPressed: index == paths.length - 1
                                  ? null
                                  : () {
                                      final next = [...paths];
                                      final item = next.removeAt(index);
                                      next.insert(index + 1, item);
                                      update(next);
                                    },
                              icon: const Icon(Icons.arrow_downward, size: 17),
                            ),
                            IconButton(
                              tooltip: s.t('action.remove'),
                              onPressed: () {
                                final next = [...paths]..removeAt(index);
                                update(next);
                              },
                              icon: const Icon(Icons.close, size: 17),
                            ),
                          ],
                        ),
                      );
                    },
                  ),
          ),
          if (paths.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(top: 6),
              child: Text(
                s.count('print.filesSelected', paths.length),
                textAlign: TextAlign.end,
                style: t.textTheme.labelSmall?.copyWith(
                  color: t.colorScheme.onSurfaceVariant,
                ),
              ),
            ),
        ],
      ),
    );
  }
}
