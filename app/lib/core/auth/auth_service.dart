import 'dart:convert';

import 'package:flutter/foundation.dart';

import '../cloud/rest.dart';
import '../settings/settings_store.dart';

/// Supabase Auth (email + parola) — duz HTTPS, yeni bagimlilik yok.
///
/// GUVENLIK SINIRI (durustce): bu bir MASAUSTU uygulamasi. Giris ekrani UI
/// kapisidir; asil zorlama sunucuda RLS ile yapilir (paylasilan history'ye kim
/// yazip okuyabilir). Dosya islemleri yerel kalir.
///
/// Oturum ~/.docflow/settings.json icinde DUZ METIN saklanir (OS keychain degil).
/// Makineye erisimi olan biri token'i okuyabilir — kisisel arac icin kabul.
class AuthSession {
  AuthSession({
    required this.accessToken,
    required this.refreshToken,
    required this.userId,
    required this.email,
    required this.expiresAt,
  });

  final String accessToken;
  final String refreshToken;
  final String userId;
  final String email;
  final int expiresAt; // epoch saniye

  /// 60 sn pay birak — istek ortasinda sonmesin.
  bool get expired =>
      DateTime.now().millisecondsSinceEpoch ~/ 1000 >= (expiresAt - 60);

  Map<String, dynamic> toJson() => {
        'accessToken': accessToken,
        'refreshToken': refreshToken,
        'userId': userId,
        'email': email,
        'expiresAt': expiresAt,
      };

  static AuthSession? fromJson(Map<String, dynamic>? j) {
    if (j == null) return null;
    final at = j['accessToken'] as String?;
    final rt = j['refreshToken'] as String?;
    if (at == null || rt == null) return null;
    return AuthSession(
      accessToken: at,
      refreshToken: rt,
      userId: (j['userId'] as String?) ?? '',
      email: (j['email'] as String?) ?? '',
      expiresAt: (j['expiresAt'] as num?)?.toInt() ?? 0,
    );
  }

  static AuthSession fromTokenResponse(Map<String, dynamic> j) {
    final user = (j['user'] as Map?)?.cast<String, dynamic>() ?? const {};
    final expiresIn = (j['expires_in'] as num?)?.toInt() ?? 3600;
    final expiresAt = (j['expires_at'] as num?)?.toInt() ??
        (DateTime.now().millisecondsSinceEpoch ~/ 1000) + expiresIn;
    return AuthSession(
      accessToken: (j['access_token'] as String?) ?? '',
      refreshToken: (j['refresh_token'] as String?) ?? '',
      userId: (user['id'] as String?) ?? '',
      email: (user['email'] as String?) ?? '',
      expiresAt: expiresAt,
    );
  }
}

/// public.app_users satiri — kimin kullanabilecegini belirler.
class AppUser {
  AppUser({
    required this.id,
    required this.email,
    required this.allowed,
    required this.isAdmin,
    required this.createdAt,
    this.username,
  });

  final String id;
  final String email;
  final bool allowed;
  final bool isAdmin;
  final String createdAt;

  /// Bos olabilir: eski kayitlar ve signup yarisini kaybedenler (bkz. SQL).
  final String? username;

  /// Ekranda gosterilecek ad — kullanici adi varsa o, yoksa e-posta.
  String get display => (username != null && username!.isNotEmpty) ? username! : email;

  factory AppUser.fromJson(Map<String, dynamic> j) => AppUser(
        id: (j['id'] as String?) ?? '',
        email: (j['email'] as String?) ?? '',
        allowed: (j['allowed'] as bool?) ?? false,
        isAdmin: (j['is_admin'] as bool?) ?? false,
        createdAt: (j['created_at'] as String?) ?? '',
        username: j['username'] as String?,
      );
}

/// Kullanici adi kurallari — sunucudaki public.username_ok ile AYNI olmali.
/// Buradaki kontrol bir kolayliktir; zorlayan sunucudur.
abstract final class UsernameRule {
  static const int minLength = 3;
  static const int maxLength = 24;
  static final RegExp charset = RegExp(r'^[A-Za-z0-9._-]+$');

  static bool ok(String s) {
    final v = s.trim();
    return v.length >= minLength && v.length <= maxLength && charset.hasMatch(v);
  }
}

class AuthException implements Exception {
  AuthException(this.message);
  final String message;
  @override
  String toString() => message;
}

class AuthService extends ChangeNotifier {
  AuthService({required this.url, required this.key, required this.settings});

  final String url;
  final String key;
  final SettingsStore settings;

  AuthSession? _session;
  AppUser? _me;

  AuthSession? get session => _session;
  AppUser? get me => _me;
  bool get signedIn => _session != null;

  /// Onaylanmadan uygulama kullanilamaz (admin app_users.allowed=true yapmali).
  bool get allowed => _me?.allowed ?? false;
  bool get isAdmin => _me?.isAdmin ?? false;

  Map<String, String> get _anonHeaders => {'apikey': key};
  Map<String, String> get _authHeaders => {
        'apikey': key,
        if (_session != null) 'Authorization': 'Bearer ${_session!.accessToken}',
      };

  /// Diskteki oturumu yukle; suresi dolmussa yenile.
  Future<void> restore() async {
    _session = AuthSession.fromJson(settings.section('session'));
    if (_session == null) return;
    if (_session!.expired) {
      final ok = await _refresh();
      if (!ok) {
        await signOut();
        return;
      }
    }
    try {
      await loadMe();
    } catch (_) {/* ag yoksa sessiz — gate tekrar dener */}
  }

  String _errorFrom(String body, int code) {
    try {
      final j = json.decode(body);
      if (j is Map) {
        final m = j['error_description'] ?? j['msg'] ?? j['message'] ?? j['error'];
        if (m is String && m.isNotEmpty) return m;
      }
    } catch (_) {}
    return 'HTTP $code';
  }

  /// Postgres fonksiyonu cagir (PostgREST /rpc). Donus: cozulmus JSON.
  Future<dynamic> _rpc(
    String fn,
    Map<String, dynamic> args, {
    bool authed = true,
  }) async {
    final (code, body) = await restSend(
      'POST',
      Uri.parse('$url/rest/v1/rpc/$fn'),
      headers: authed ? _authHeaders : _anonHeaders,
      body: args,
    );
    if (code < 200 || code >= 300) throw AuthException(_errorFrom(body, code));
    return body.isEmpty ? null : json.decode(body);
  }

  /// Kayit ekrani icin: bu kullanici adi alinabilir mi?
  /// Ag hatasinda TRUE doner — kullaniciyi bos yere engelleme; son sozu
  /// sunucudaki unique index soyler.
  Future<bool> usernameAvailable(String username) async {
    if (!UsernameRule.ok(username)) return false;
    try {
      final r = await _rpc('username_available', {'p_username': username.trim()},
          authed: false);
      return r == true;
    } catch (_) {
      return true;
    }
  }

  Future<void> signUp(String email, String password, String username) async {
    final (code, body) = await restSend(
      'POST',
      Uri.parse('$url/auth/v1/signup'),
      headers: _anonHeaders,
      // data -> raw_user_meta_data; handle_new_user trigger'i buradan okur.
      body: {
        'email': email,
        'password': password,
        'data': {'username': username.trim()},
      },
    );
    if (code < 200 || code >= 300) throw AuthException(_errorFrom(body, code));
    // E-posta dogrulama aciksa oturum donmez — kullanici yine de giris denemeli.
  }

  /// E-posta VEYA kullanici adi ile giris.
  ///
  /// Supabase e-posta ile dogrular; kullanici adi girildiyse once sunucuda
  /// e-postaya cevrilir (email_for_login parolayi da dogrular — bkz. SQL).
  /// Cozulemezse "kullanici adi yok" ile "parola yanlis" AYNI hatayi verir:
  /// disaridan hangi adlarin var oldugu anlasilmasin.
  Future<void> signIn(String identifier, String password) async {
    final id = identifier.trim();
    var email = id;

    if (!id.contains('@')) {
      dynamic resolved;
      try {
        resolved = await _rpc('email_for_login',
            {'p_username': id, 'p_password': password},
            authed: false);
      } on AuthException {
        // Fonksiyon yoksa/bozuksa (SQL calistirilmamis olabilir) e-posta ile
        // giris hala calisir; kullaniciya anlamli mesaj ver.
        throw AuthException('invalid_credentials');
      }
      if (resolved is! String || resolved.isEmpty) {
        throw AuthException('invalid_credentials');
      }
      email = resolved;
    }

    final (code, body) = await restSend(
      'POST',
      Uri.parse('$url/auth/v1/token?grant_type=password'),
      headers: _anonHeaders,
      body: {'email': email, 'password': password},
    );
    if (code < 200 || code >= 300) throw AuthException(_errorFrom(body, code));
    _session = AuthSession.fromTokenResponse(
        (json.decode(body) as Map).cast<String, dynamic>());
    await settings.saveSection('session', _session!.toJson());
    await loadMe();
    notifyListeners();
  }

  /// Kendi kullanici adini degistir. Sunucu dogrular (uzunluk/karakter/benzersiz).
  Future<void> setUsername(String username) async {
    await validToken();
    await _rpc('set_username', {'p_username': username.trim()});
    await loadMe(); // _me tazelensin -> UI hemen yeni adi gosterir
  }

  Future<bool> _refresh() async {
    final rt = _session?.refreshToken;
    if (rt == null) return false;
    try {
      final (code, body) = await restSend(
        'POST',
        Uri.parse('$url/auth/v1/token?grant_type=refresh_token'),
        headers: _anonHeaders,
        body: {'refresh_token': rt},
      );
      if (code < 200 || code >= 300) return false;
      _session = AuthSession.fromTokenResponse(
          (json.decode(body) as Map).cast<String, dynamic>());
      await settings.saveSection('session', _session!.toJson());
      return true;
    } catch (e) {
      debugPrint('refresh failed: $e');
      return false;
    }
  }

  /// Gecerli access token dondur (gerekirse yenile). Bulut istekleri bunu kullanir.
  Future<String?> validToken() async {
    if (_session == null) return null;
    if (_session!.expired && !await _refresh()) return null;
    return _session!.accessToken;
  }

  Future<void> signOut() async {
    try {
      if (_session != null) {
        await restSend('POST', Uri.parse('$url/auth/v1/logout'),
            headers: _authHeaders);
      }
    } catch (_) {/* yerel temizlik yeter */}
    _session = null;
    _me = null;
    await settings.saveSection('session', null);
    notifyListeners();
  }

  /// Kendi app_users satirini oku (allowed / is_admin). RLS: kendi satirin.
  Future<AppUser?> loadMe() async {
    final token = await validToken();
    if (token == null || _session == null) return null;
    final (code, body) = await restSend(
      'GET',
      Uri.parse('$url/rest/v1/app_users?select=*&id=eq.${_session!.userId}'),
      headers: _authHeaders,
    );
    if (code < 200 || code >= 300) throw AuthException(_errorFrom(body, code));
    final list = (json.decode(body) as List).cast<Map<String, dynamic>>();
    _me = list.isEmpty ? null : AppUser.fromJson(list.first);
    notifyListeners();
    return _me;
  }

  // ---------------- admin ----------------
  // RLS zorlar: admin degilsen bu cagrilar bos doner / reddedilir.

  Future<List<AppUser>> listUsers() async {
    await validToken();
    final (code, body) = await restSend(
      'GET',
      Uri.parse('$url/rest/v1/app_users?select=*&order=created_at.desc'),
      headers: _authHeaders,
    );
    if (code < 200 || code >= 300) throw AuthException(_errorFrom(body, code));
    return (json.decode(body) as List)
        .cast<Map<String, dynamic>>()
        .map(AppUser.fromJson)
        .toList();
  }

  Future<void> setFlags(String userId, {bool? allowed, bool? isAdmin}) async {
    await validToken();
    final patch = <String, dynamic>{
      'allowed': ?allowed,
      'is_admin': ?isAdmin,
    };
    if (patch.isEmpty) return;
    final (code, body) = await restSend(
      'PATCH',
      Uri.parse('$url/rest/v1/app_users?id=eq.$userId'),
      headers: {..._authHeaders, 'Prefer': 'return=minimal'},
      body: patch,
    );
    if (code < 200 || code >= 300) throw AuthException(_errorFrom(body, code));
  }
}
