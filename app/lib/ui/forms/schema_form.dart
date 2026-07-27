import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';

import '../../core/l10n/strings.dart';
import '../../core/operations/models.dart';
import '../theme/responsive.dart';

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
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        for (var i = 0; i < widget.params.length; i++) ...[
          if (i > 0) const SizedBox(height: 18),
          _field(context, s, widget.params[i]),
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
        return Container(
          decoration: BoxDecoration(
            color: t.colorScheme.surfaceContainerLow,
            borderRadius: BorderRadius.circular(10),
            border: Border.all(color: t.colorScheme.outlineVariant),
          ),
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
            initialValue: widget.values[p.name] as String?,
            items: [
              for (final c in p.choices)
                DropdownMenuItem(value: c, child: Text(c)),
            ],
            onChanged: (v) => _set(p.name, v),
          ),
        );
      case ParamKind.int:
        return _labeled(
          s,
          p,
          TextFormField(
            initialValue: widget.values[p.name]?.toString() ?? '',
            keyboardType: TextInputType.number,
            onChanged: (v) => _set(p.name, int.tryParse(v)),
          ),
        );
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
