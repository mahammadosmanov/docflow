import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';

import '../../core/l10n/strings.dart';
import '../../core/operations/models.dart';

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
        for (final p in widget.params)
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 10),
            child: _field(context, s, p),
          ),
      ],
    );
  }

  Widget _field(BuildContext context, Strings s, ParamDef p) {
    switch (p.kind) {
      case ParamKind.folder:
        return _pathField(s, p, pickFolder: true);
      case ParamKind.file:
        return _pathField(s, p, pickFolder: false);
      case ParamKind.bool:
        return SwitchListTile(
          contentPadding: EdgeInsets.zero,
          title: Text(s.t(p.labelKey)),
          subtitle: p.helpKey == null ? null : Text(s.t(p.helpKey!)),
          value: (widget.values[p.name] as bool?) ?? false,
          onChanged: (v) => _set(p.name, v),
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
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(s.t(p.labelKey), style: const TextStyle(fontWeight: FontWeight.w600)),
        if (p.helpKey != null)
          Padding(
            padding: const EdgeInsets.only(top: 2, bottom: 6),
            child: Text(s.t(p.helpKey!),
                style: TextStyle(fontSize: 12, color: Colors.grey.shade600)),
          ),
        field,
      ],
    );
  }

  Widget _pathField(Strings s, ParamDef p, {required bool pickFolder}) {
    final value = widget.values[p.name] as String?;
    return _labeled(
      s,
      p,
      Row(
        children: [
          Expanded(
            child: TextFormField(
              key: ValueKey('${p.name}:$value'),
              initialValue: value ?? '',
              decoration: const InputDecoration(isDense: true),
              onChanged: (v) => widget.values[p.name] = v,
            ),
          ),
          const SizedBox(width: 8),
          OutlinedButton.icon(
            icon: Icon(pickFolder ? Icons.folder_open : Icons.description_outlined),
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
          ),
        ],
      ),
    );
  }
}
