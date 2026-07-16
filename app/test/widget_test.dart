// Process gerektirmeyen saf birim testleri: sema ayristirma + i18n cozumleme.
// (Sidecar client / schema_form widget testleri Faz 2.2/2.3'te eklenecek.)

import 'dart:convert';
import 'dart:io';

import 'package:docflow/core/auth/auth_service.dart';
import 'package:docflow/core/l10n/strings.dart';
import 'package:docflow/core/operations/models.dart';
import 'package:docflow/core/settings/settings_store.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('OperationDef list_operations JSON semasini ayristirir', () {
    final op = OperationDef.fromJson({
      'id': 'rfi.process_scans',
      'titleKey': 'op.rfi.process_scans.title',
      'descriptionKey': 'op.rfi.process_scans.desc',
      'schema': [
        {
          'name': 'source',
          'kind': 'folder',
          'labelKey': 'param.source',
          'default': null,
          'required': true,
          'choices': [],
          'helpKey': 'param.source.help',
        },
        {
          'name': 'keep_original',
          'kind': 'bool',
          'labelKey': 'param.keep_original',
          'default': false,
          'required': false,
          'choices': [],
          'helpKey': null,
        },
      ],
    });

    expect(op.id, 'rfi.process_scans');
    expect(op.params.first.kind, ParamKind.folder);
    expect(op.params.first.required, isTrue);
    expect(op.params[1].kind, ParamKind.bool);
    expect(op.params[1].required, isFalse);
  });

  test('Plan.hasActionableSteps mkdir-only plani PASIF sayar', () {
    final onlyMkdir = Plan.fromJson({
      'planId': 'x',
      'opId': 'rfi.process_scans',
      'steps': [
        {'kind': 'mkdir', 'src': null, 'dst': '/out/A', 'noteKey': null, 'meta': {}},
      ],
      'warnings': [],
    });
    expect(onlyMkdir.hasActionableSteps, isFalse);

    final withMove = Plan.fromJson({
      'planId': 'x',
      'opId': 'rfi.process_scans',
      'steps': [
        {'kind': 'mkdir', 'src': null, 'dst': '/out/A', 'noteKey': null, 'meta': {}},
        {'kind': 'move', 'src': '/in/a.pdf', 'dst': '/out/A/a.pdf', 'noteKey': 'note.renamed', 'meta': {'confidence': 1.0}},
      ],
      'warnings': [],
    });
    expect(withMove.hasActionableSteps, isTrue);
    expect(withMove.steps[1].confidence, 1.0);
  });

  test('Strings cozulemeyen anahtari sessizce yutmaz, anahtari doner', () {
    final s = Strings({'action.apply': 'Apply'});
    expect(s.t('action.apply'), 'Apply');
    expect(s.t('yok.olan.anahtar'), 'yok.olan.anahtar');
    expect(Strings({'plan.summary': '{count} files'}).count('plan.summary', 3), '3 files');
  });

  test('SettingsStore son secilen klasorleri hatirlar (disk round-trip)', () async {
    final tmp = await Directory.systemTemp.createTemp('docflow_settings');
    final f = File('${tmp.path}/settings.json');
    try {
      final s1 = await SettingsStore.load(file: f);
      await s1.saveParams('rfi.process_scans', {
        'source': '/a/in',
        'dest': '/a/out',
        'keep_original': true,
        'not_serializable': [1, 2, 3], // filtrelenmeli
      });

      final s2 = await SettingsStore.load(file: f); // yeniden okundu
      final got = s2.paramsFor('rfi.process_scans');
      expect(got['source'], '/a/in');
      expect(got['dest'], '/a/out');
      expect(got['keep_original'], true);
      expect(got.containsKey('not_serializable'), isFalse);
      expect(s2.paramsFor('unknown.op'), isEmpty);
    } finally {
      await tmp.delete(recursive: true);
    }
  });

  // CLAUDE.md kural 4: her metin DORT ARB'de de olmak zorunda. gen-l10n noktali
  // anahtarlari reddettigi icin build-time kontrol yok — kontrol burada.
  test('i18n: dort ARB ayni anahtar kumesine sahip', () {
    Set<String> keysOf(String loc) {
      final j = json.decode(File('l10n/app_$loc.arb').readAsStringSync()) as Map;
      // '@@locale' ve '@key' meta girdileri cevrilebilir metin degil.
      return j.keys.cast<String>().where((k) => !k.startsWith('@')).toSet();
    }

    final en = keysOf('en');
    expect(en, isNotEmpty);
    for (final loc in ['tr', 'ru', 'az']) {
      expect(keysOf(loc), en, reason: '$loc anahtar kumesi en ile ayni degil');
    }
  });

  // Kural 5: az ve tr paylasilmaz. Ayni dizeyi tasiyorlarsa biri kopyalanmis
  // demektir — bu testin amaci o kaymayi erken yakalamak.
  test('i18n: az ve tr auth metinleri ayri cevrilmis', () {
    Map<String, dynamic> load(String loc) =>
        (json.decode(File('l10n/app_$loc.arb').readAsStringSync()) as Map)
            .cast<String, dynamic>();
    final tr = load('tr');
    final az = load('az');
    for (final k in ['auth.signIn', 'auth.pending', 'admin.title']) {
      expect(az[k], isNot(equals(tr[k])), reason: '$k: az, tr kopyasi olmamali');
    }
  });

  test('AuthSession suresi dolmayi 60sn payla bildirir', () {
    final now = DateTime.now().millisecondsSinceEpoch ~/ 1000;
    AuthSession at(int expiresAt) => AuthSession(
          accessToken: 'a',
          refreshToken: 'r',
          userId: 'u',
          email: 'e@x.com',
          expiresAt: expiresAt,
        );
    expect(at(now + 3600).expired, isFalse);
    expect(at(now + 30).expired, isTrue, reason: 'pay icinde: yenilenmeli');
    expect(at(now - 1).expired, isTrue);
  });

  test('AuthSession disk round-trip: token korunur', () {
    final s = AuthSession(
      accessToken: 'tok',
      refreshToken: 'ref',
      userId: 'uid',
      email: 'e@x.com',
      expiresAt: 123,
    );
    final back = AuthSession.fromJson(json.decode(json.encode(s.toJson())));
    expect(back!.accessToken, 'tok');
    expect(back.refreshToken, 'ref');
    expect(back.userId, 'uid');
    expect(AuthSession.fromJson(null), isNull);
    expect(AuthSession.fromJson({'accessToken': 'x'}), isNull, reason: 'refresh yok');
  });

  test('AppUser: eksik alanlar erisim ACMAZ (guvenli varsayilan)', () {
    final u = AppUser.fromJson({'id': 'x', 'email': 'e@x.com'});
    expect(u.allowed, isFalse);
    expect(u.isAdmin, isFalse);
  });
}
