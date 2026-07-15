import 'package:flutter/material.dart';

import '../../core/ipc/sidecar_client.dart';
import '../../core/l10n/strings.dart';
import '../../core/operations/models.dart';
import 'operation_flow_screen.dart';

/// Acilis ekrani: list_operations cagirir, operation'lari kart olarak listeler.
/// Minimalist: tek sutun, bol bosluk (docs/TASKS.md 3.1).
class OperationListScreen extends StatefulWidget {
  const OperationListScreen({super.key, required this.client});

  final SidecarClient client;

  @override
  State<OperationListScreen> createState() => _OperationListScreenState();
}

class _OperationListScreenState extends State<OperationListScreen> {
  late Future<List<OperationDef>> _future;

  @override
  void initState() {
    super.initState();
    _future = _load();
  }

  Future<List<OperationDef>> _load() async {
    await widget.client.start();
    return widget.client.listOperations();
  }

  void _retry() => setState(() => _future = _load());

  @override
  Widget build(BuildContext context) {
    final s = Strings.of(context);
    return Scaffold(
      appBar: AppBar(title: Text(s.t('app.title'))),
      body: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 640),
          child: FutureBuilder<List<OperationDef>>(
            future: _future,
            builder: (context, snap) {
              if (snap.connectionState == ConnectionState.waiting) {
                return const CircularProgressIndicator();
              }
              if (snap.hasError || widget.client.isDown) {
                return _error(s);
              }
              final ops = snap.data ?? [];
              if (ops.isEmpty) {
                return Text(s.t('plan.empty'));
              }
              return ListView(
                padding: const EdgeInsets.symmetric(vertical: 24, horizontal: 16),
                children: [
                  for (final op in ops) _card(context, s, op),
                ],
              );
            },
          ),
        ),
      ),
    );
  }

  Widget _error(Strings s) => Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          const Icon(Icons.cloud_off, size: 40, color: Colors.redAccent),
          const SizedBox(height: 12),
          Text(s.t('error.sidecar_down')),
          const SizedBox(height: 12),
          FilledButton(onPressed: _retry, child: Text(s.t('action.preview'))),
        ],
      );

  Widget _card(BuildContext context, Strings s, OperationDef op) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 16),
      child: Card(
        elevation: 0,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(14),
          side: BorderSide(color: Colors.grey.shade300),
        ),
        child: ListTile(
          contentPadding: const EdgeInsets.symmetric(horizontal: 20, vertical: 12),
          title: Text(s.t(op.titleKey),
              style: const TextStyle(fontWeight: FontWeight.w600, fontSize: 16)),
          subtitle: Padding(
            padding: const EdgeInsets.only(top: 6),
            child: Text(s.t(op.descriptionKey)),
          ),
          trailing: const Icon(Icons.chevron_right),
          onTap: () => Navigator.of(context).push(
            MaterialPageRoute(
              builder: (_) => OperationFlowScreen(client: widget.client, op: op),
            ),
          ),
        ),
      ),
    );
  }
}
