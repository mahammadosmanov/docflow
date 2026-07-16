import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';

import '../auth/auth_service.dart';
import 'rest.dart';

/// Supabase gecmis senkronu — duz HTTPS (PostgREST). Yeni bagimlilik YOK.
///
/// history artik SADECE giris yapmis + onaylanmis (allowed) kullanicilara acik;
/// zorlama sunucuda RLS ile yapilir, istemcideki kontrol sadece UI kolayligi.
/// Anahtar "publishable" tiptedir (istemciye gomulmek icin tasarlanmis);
/// service_role/secret anahtar ASLA buraya konmaz.
const supabaseUrl = String.fromEnvironment(
  'SUPABASE_URL',
  defaultValue: 'https://ndpkgbfmshceffwhpqoz.supabase.co',
);
const supabaseKey = String.fromEnvironment(
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
    required this.userEmail,
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
  final String userEmail;

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
        userEmail: (j['user_email'] as String?) ?? '',
      );
}

class CloudHistory {
  CloudHistory({required this.auth, String? url, String? key})
      : url = url ?? supabaseUrl,
        key = key ?? supabaseKey;

  final AuthService auth;
  final String url;
  final String key;

  /// Bulut ancak giris yapilmis VE onaylanmissa kullanilir.
  bool get enabled =>
      url.isNotEmpty && key.isNotEmpty && auth.signedIn && auth.allowed;

  Future<Map<String, String>> _headers() async {
    final token = await auth.validToken();
    return {
      'apikey': key,
      if (token != null) 'Authorization': 'Bearer $token',
    };
  }

  /// Apply/undo sonrasi gecmis kaydi. Bulut OPSIYONEL: hata uygulamayi ETKILEMEZ,
  /// yerel journal her zaman gercek kaynaktir.
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
      final (code, text) = await restSend(
        'POST',
        Uri.parse('$url/rest/v1/history'),
        headers: {...await _headers(), 'Prefer': 'return=minimal'},
        body: {
          'device': Platform.localHostname,
          'op_id': opId,
          'applied': applied,
          'failed': failed,
          'journal_id': journalId,
          'summary': summary,
          'steps': steps,
          'user_id': auth.session?.userId,
          'user_email': auth.session?.email,
        },
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
    final (code, text) = await restSend(
      'GET',
      Uri.parse('$url/rest/v1/history?select=*&order=created_at.desc&limit=$limit'),
      headers: await _headers(),
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
      await restSend(
        'PATCH',
        Uri.parse('$url/rest/v1/history?id=eq.$id'),
        headers: {...await _headers(), 'Prefer': 'return=minimal'},
        body: {'undone': true},
      );
    } catch (e) {
      debugPrint('cloud markUndone error (ignored): $e');
    }
  }
}
