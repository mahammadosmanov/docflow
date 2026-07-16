import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';

/// Kalici ayarlar — kullanicinin son sectigi klasorleri hatirla.
///
/// Operasyona ozel form YOK (mimari): degerleri opId -> {param: value} olarak
/// generic sakliyoruz. Yeni operation eklerken buraya dokunmak gerekmez.
/// Depolama: ~/.docflow/settings.json (sidecar journal'i ile ayni kok).
/// Sadece JSON-serilestirilebilir degerler (String/bool/num/null) saklanir.
class SettingsStore {
  SettingsStore._(this._file, this._data);

  final File _file;
  Map<String, dynamic> _data;

  static File _resolveFile() {
    final env = Platform.environment;
    final home = env['HOME'] ?? env['USERPROFILE'] ?? Directory.systemTemp.path;
    return File('$home${Platform.pathSeparator}.docflow'
        '${Platform.pathSeparator}settings.json');
  }

  static Future<SettingsStore> load({File? file}) async {
    file ??= _resolveFile();
    Map<String, dynamic> data = {};
    try {
      if (await file.exists()) {
        final decoded = json.decode(await file.readAsString());
        if (decoded is Map<String, dynamic>) data = decoded;
      }
    } catch (e) {
      // Bozuk/okunamayan ayar dosyasi uygulamayi engellemez — bos ile devam.
      debugPrint('settings load failed (ignored): $e');
    }
    return SettingsStore._(file, data);
  }

  /// Bir operasyon icin saklanan parametre degerleri (kopya).
  Map<String, dynamic> paramsFor(String opId) {
    final all = _data['params'];
    if (all is Map && all[opId] is Map) {
      return Map<String, dynamic>.from(all[opId] as Map);
    }
    return {};
  }

  /// Operasyonun parametrelerini sakla (sadece serilestirilebilir olanlar).
  Future<void> saveParams(String opId, Map<String, dynamic> values) async {
    final clean = <String, dynamic>{};
    values.forEach((k, v) {
      if (v == null || v is String || v is bool || v is num) clean[k] = v;
    });
    final params = Map<String, dynamic>.from(_data['params'] as Map? ?? {});
    params[opId] = clean;
    _data = {..._data, 'params': params};
    await _write();
  }

  /// Generic bolum oku (or. 'session').
  Map<String, dynamic>? section(String name) {
    final v = _data[name];
    return v is Map ? Map<String, dynamic>.from(v) : null;
  }

  /// Generic bolum yaz; null -> sil (cikis yaparken oturumu temizler).
  Future<void> saveSection(String name, Map<String, dynamic>? value) async {
    final next = Map<String, dynamic>.from(_data);
    if (value == null) {
      next.remove(name);
    } else {
      next[name] = value;
    }
    _data = next;
    await _write();
  }

  Future<void> _write() async {
    try {
      await _file.parent.create(recursive: true);
      await _file.writeAsString(json.encode(_data));
    } catch (e) {
      debugPrint('settings save failed (ignored): $e');
    }
  }
}
