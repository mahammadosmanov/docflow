import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';

/// Supabase gecmis senkronu — duz HTTPS (PostgREST). Yeni bagimlilik YOK.
///
/// Anahtar "publishable" tiptedir: istemciye gomulmek icin tasarlanmistir ve
/// erisim RLS ile sinirlanir. service_role/secret anahtar ASLA buraya konmaz.
/// --dart-define ile ezilebilir (SUPABASE_URL / SUPABASE_KEY).
const _defaultUrl = String.fromEnvironment(
  'SUPABASE_URL',
  defaultValue: 'https://ndpkgbfmshceffwhpqoz.supabase.co',
);
const _defaultKey = String.fromEnvironment(
  'SUPABASE_KEY',
  defaultValue: 'sb_publishable_sK0uEH4t-Hw7LVBpnEaMog_z_2MQwxq',
);

class CloudEntry {
  CloudEntry({
    required this.id,
    required this.createdAt,
    required this.opId,
    required this.applied,
    required this.failed,
    required this.journalId,
    required this.summary,
    required this.steps,
    required this.undone,
  });

  final String id;
  final String createdAt;
  final String opId;
  final int applied;
  final int failed;
  final String journalId;
  final String summary;
  final List<dynamic> steps;
  final bool undone;

  DateTime? get when => DateTime.tryParse(createdAt)?.toLocal();

  factory CloudEntry.fromJson(Map<String, dynamic> j) => CloudEntry(
        id: (j['id'] as String?) ?? '',
        createdAt: (j['created_at'] as String?) ?? '',
        opId: (j['op_id'] as String?) ?? '',
        applied: (j['applied'] as num?)?.toInt() ?? 0,
        failed: (j['failed'] as num?)?.toInt() ?? 0,
        journalId: (j['journal_id'] as String?) ?? '',
        summary: (j['summary'] as String?) ?? '',
        steps: (j['steps'] as List?) ?? const [],
        undone: (j['undone'] as bool?) ?? false,
      );
}

class CloudHistory {
  CloudHistory({String? url, String? key})
      : url = url ?? _defaultUrl,
        key = key ?? _defaultKey;

  final String url;
  final String key;

  bool get enabled => url.isNotEmpty && key.isNotEmpty;

  Future<(int, String)> _send(String method, Uri uri,
      {Object? body, Map<String, String> extra = const {}}) async {
    final c = HttpClient()..connectionTimeout = const Duration(seconds: 10);
    try {
      final req = await c.openUrl(method, uri);
      req.headers.set('apikey', key);
      req.headers.set('Authorization', 'Bearer $key');
      req.headers.set('Content-Type', 'application/json');
      extra.forEach(req.headers.set);
      if (body != null) req.write(json.encode(body));
      final res = await req.close();
      final text = await res.transform(utf8.decoder).join();
      return (res.statusCode, text);
    } finally {
      c.close(force: true);
    }
  }

  /// Bir apply/undo sonrasi gecmis kaydi yolla. Basarisizlik uygulamayi ETKILEMEZ
  /// (bulut opsiyonel; yerel journal her zaman gercek kaynak).
  Future<bool> push({
    required String opId,
    required int applied,
    required int failed,
    required String journalId,
    required String summary,
    required List<dynamic> steps,
  }) async {
    if (!enabled) return false;
    try {
      final (code, text) = await _send(
        'POST',
        Uri.parse('$url/rest/v1/history'),
        body: {
          'device': Platform.localHostname,
          'op_id': opId,
          'applied': applied,
          'failed': failed,
          'journal_id': journalId,
          'summary': summary,
          'steps': steps,
        },
        extra: {'Prefer': 'return=minimal'},
      );
      if (code >= 200 && code < 300) return true;
      debugPrint('cloud push failed [$code]: $text');
      return false;
    } catch (e) {
      debugPrint('cloud push error (ignored): $e');
      return false;
    }
  }

  Future<List<CloudEntry>> list({int limit = 50}) async {
    if (!enabled) return [];
    final (code, text) = await _send(
      'GET',
      Uri.parse('$url/rest/v1/history?select=*&order=created_at.desc&limit=$limit'),
    );
    if (code < 200 || code >= 300) {
      throw Exception('cloud list [$code]: $text');
    }
    final list = (json.decode(text) as List).cast<Map<String, dynamic>>();
    return list.map(CloudEntry.fromJson).toList();
  }

  Future<void> markUndone(String id) async {
    if (!enabled || id.isEmpty) return;
    try {
      await _send(
        'PATCH',
        Uri.parse('$url/rest/v1/history?id=eq.$id'),
        body: {'undone': true},
        extra: {'Prefer': 'return=minimal'},
      );
    } catch (e) {
      debugPrint('cloud markUndone error (ignored): $e');
    }
  }
}
