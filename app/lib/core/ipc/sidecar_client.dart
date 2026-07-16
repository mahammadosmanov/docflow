import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';

import '../operations/models.dart';

/// docs/IPC_PROTOCOL.md sozlesmesi: stdin/stdout uzerinde satir-ayrimli JSON.
///
/// - Her istek bir `id` alir, Future ile eslesir.
/// - `id`'siz mesaj = notification (progress) -> Stream.
/// - stderr = uygulama logu, ASLA parse edilmez.
/// - Sidecar olurse yeniden baslat; ust uste 3 kez olurse `error.sidecar_down`.
class SidecarClient {
  SidecarClient({
    required this.executable,
    required this.args,
    required this.workingDir,
    this.environment,
  });

  /// Sidecar'i baslatan calistirilabilir + argumanlar.
  ///   dev  : venv python + ['-m', 'docflow_sidecar']
  ///   paket: docflow_sidecar.exe + []
  final String executable;
  final List<String> args;
  final String workingDir;

  /// Cocuk process'e gecen ek ortam (or. DOCFLOW_TESSERACT = bundle tesseract yolu).
  final Map<String, String>? environment;

  Process? _proc;
  int _nextId = 1;
  final Map<int, Completer<Map<String, dynamic>>> _pending = {};
  final StreamController<Progress> _progress = StreamController.broadcast();
  int _consecutiveDeaths = 0;
  bool _down = false;
  bool _stopping = false;

  Stream<Progress> get progress => _progress.stream;

  /// Ust uste 3 olumden sonra true; UI `error.sidecar_down` gosterir.
  bool get isDown => _down;

  Future<void> start() async {
    if (_proc != null) return;
    _stopping = false;
    final proc = await Process.start(
      executable,
      args,
      workingDirectory: workingDir,
      environment: environment,
    );
    _proc = proc;

    proc.stdout
        .transform(utf8.decoder)
        .transform(const LineSplitter())
        .listen(_onLine, onError: (e) => debugPrint('sidecar stdout err: $e'));

    proc.stderr
        .transform(utf8.decoder)
        .transform(const LineSplitter())
        .listen((l) => debugPrint('[sidecar] $l')); // ASLA parse etme

    proc.exitCode.then(_onExit);

    // Hazir mi? ping ile dogrula. Basarili olursa olum sayaci sifirlanir.
    await ping();
    _consecutiveDeaths = 0;
    _down = false;
  }

  void _onLine(String line) {
    line = line.trim();
    if (line.isEmpty) return;
    Map<String, dynamic> msg;
    try {
      msg = json.decode(line) as Map<String, dynamic>;
    } catch (e) {
      debugPrint('sidecar: JSON degil, atlaniyor: $line');
      return;
    }

    if (!msg.containsKey('id') || msg['id'] == null) {
      // notification (progress)
      if (msg['method'] == 'progress') {
        final p = (msg['params'] as Map).cast<String, dynamic>();
        _progress.add(Progress(
          phase: p['phase'] as String? ?? '',
          done: (p['done'] as num?)?.toInt() ?? 0,
          total: (p['total'] as num?)?.toInt() ?? 0,
        ));
      }
      return;
    }

    final id = (msg['id'] as num).toInt();
    final c = _pending.remove(id);
    if (c == null) return;
    if (msg.containsKey('error')) {
      final err = (msg['error'] as Map).cast<String, dynamic>();
      c.completeError(SidecarError(
        code: (err['code'] as num?)?.toInt() ?? 0,
        message: err['message'] as String? ?? 'unknown',
      ));
    } else {
      c.complete(_asMap(msg['result']));
    }
  }

  Map<String, dynamic> _asMap(Object? result) {
    if (result is Map) return result.cast<String, dynamic>();
    return {'result': result}; // list_operations list dondurur; ayrica ele alinir
  }

  Future<void> _onExit(int code) async {
    _proc = null;
    // Bekleyen tum istekleri hataya dusur.
    for (final c in _pending.values) {
      c.completeError(SidecarError(code: -1, message: 'sidecar exited ($code)'));
    }
    _pending.clear();
    if (_stopping) return;

    _consecutiveDeaths++;
    debugPrint('sidecar oldu (code=$code), olum #$_consecutiveDeaths');
    if (_consecutiveDeaths >= 3) {
      _down = true;
      return;
    }
    try {
      await start();
    } catch (e) {
      debugPrint('yeniden baslatma basarisiz: $e');
    }
  }

  Future<Object?> _callRaw(String method, Map<String, dynamic> params) {
    final proc = _proc;
    if (proc == null) {
      return Future.error(SidecarError(code: -1, message: 'sidecar not running'));
    }
    final id = _nextId++;
    final c = Completer<Map<String, dynamic>>();
    _pending[id] = c;
    proc.stdin.writeln(json.encode({'id': id, 'method': method, 'params': params}));
    return c.future.then((m) => m.containsKey('result') ? m['result'] : m);
  }

  Future<Map<String, dynamic>> ping() async =>
      (await _callRaw('ping', {})) as Map<String, dynamic>;

  Future<List<OperationDef>> listOperations() async {
    final res = await _callRaw('list_operations', {});
    final list = (res as List).cast<Map<String, dynamic>>();
    return list.map(OperationDef.fromJson).toList();
  }

  Future<Plan> preview(String opId, Map<String, dynamic> params) async {
    final res = await _callRaw('preview', {'op': opId, 'params': params});
    return Plan.fromJson((res as Map).cast<String, dynamic>());
  }

  Future<ApplyResult> apply(String planId) async {
    final res = await _callRaw('apply', {'planId': planId});
    return ApplyResult.fromJson((res as Map).cast<String, dynamic>());
  }

  Future<ApplyResult> undo(String journalId) async {
    final res = await _callRaw('undo', {'journalId': journalId});
    return ApplyResult.fromJson((res as Map).cast<String, dynamic>());
  }

  Future<List<ReviewItem>> reviewScan(String dest) async {
    final res = await _callRaw('review_scan', {'dest': dest});
    return (res as List)
        .cast<Map<String, dynamic>>()
        .map(ReviewItem.fromJson)
        .toList();
  }

  Future<Plan> reviewPreview(String dest, List<Map<String, dynamic>> decisions) async {
    final res = await _callRaw('review_preview', {'dest': dest, 'decisions': decisions});
    return Plan.fromJson((res as Map).cast<String, dynamic>());
  }

  Future<void> dispose() async {
    _stopping = true;
    _proc?.stdin.close();
    _proc?.kill();
    await _progress.close();
  }
}

class SidecarError implements Exception {
  SidecarError({required this.code, required this.message});
  final int code;
  final String message;
  @override
  String toString() => 'SidecarError($code): $message';
}
