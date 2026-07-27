// Process gerektirmeyen saf birim testleri: sema ayristirma + i18n cozumleme.
// (Sidecar client / schema_form widget testleri Faz 2.2/2.3'te eklenecek.)

import 'dart:convert';
import 'dart:io';

import 'package:docflow/core/auth/auth_service.dart';
import 'package:docflow/core/l10n/strings.dart';
import 'package:docflow/core/operations/models.dart';
import 'package:docflow/core/settings/app_prefs.dart';
import 'package:docflow/core/settings/settings_store.dart';
import 'package:docflow/ui/theme/app_theme.dart';
import 'package:docflow/ui/shell/operation_flow_screen.dart';
import 'package:flutter/material.dart';
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

  test('mail ve API key operasyon ayarlarina yazilmaz', () {
    final params = [
      ParamDef.fromJson({
        'name': 'workbook',
        'kind': 'file',
        'labelKey': 'x',
        'required': true,
      }),
      ParamDef.fromJson({
        'name': 'email_text',
        'kind': 'longText',
        'labelKey': 'x',
        'required': false,
      }),
      ParamDef.fromJson({
        'name': 'gemini_api_key',
        'kind': 'secret',
        'labelKey': 'x',
        'required': false,
        'visibleWhen': {'email_provider': 'gemini'},
      }),
    ];

    final saved = persistentOperationValues(params, {
      'workbook': r'C:\RFI Log.xlsx',
      'email_text': 'confidential mail',
      'gemini_api_key': 'secret',
    });

    expect(saved, {'workbook': r'C:\RFI Log.xlsx'});
    expect(params[1].kind, ParamKind.longText);
    expect(params[2].kind, ParamKind.secret);
    expect(params[2].visibleWhen, {'email_provider': 'gemini'});
  });

  test('Plan.hasActionableSteps mkdir-only plani PASIF sayar', () {
    final onlyMkdir = Plan.fromJson({
      'planId': 'x',
      'opId': 'rfi.process_scans',
      'steps': [
        {
          'kind': 'mkdir',
          'src': null,
          'dst': '/out/A',
          'noteKey': null,
          'meta': {},
        },
      ],
      'warnings': [],
    });
    expect(onlyMkdir.hasActionableSteps, isFalse);

    final withMove = Plan.fromJson({
      'planId': 'x',
      'opId': 'rfi.process_scans',
      'steps': [
        {
          'kind': 'mkdir',
          'src': null,
          'dst': '/out/A',
          'noteKey': null,
          'meta': {},
        },
        {
          'kind': 'move',
          'src': '/in/a.pdf',
          'dst': '/out/A/a.pdf',
          'noteKey': 'note.renamed',
          'meta': {'confidence': 1.0},
        },
      ],
      'warnings': [],
    });
    expect(withMove.hasActionableSteps, isTrue);
    expect(withMove.steps[1].confidence, 1.0);
  });

  test('Plan workbook_update adimini uygulanabilir ve editorlu sayar', () {
    final plan = Plan.fromJson({
      'planId': 'rfi-plan',
      'opId': 'rfi.log_add',
      'steps': [
        {
          'kind': 'workbook_update',
          'src': 'RFI Log.xlsx',
          'dst': 'backup.xlsx',
          'noteKey': 'note.rfi.workbookUpdate',
          'meta': {},
        },
      ],
      'warnings': [],
      'meta': {
        'editor': {
          'kind': 'recordTable',
          'rows': [
            {'rfiNo': 'AZE-SUN-INT-QMS-RFI-CIV-0860'},
          ],
        },
      },
    });
    expect(plan.hasActionableSteps, isTrue);
    expect(plan.editor?['kind'], 'recordTable');
  });

  test('Strings cozulemeyen anahtari sessizce yutmaz, anahtari doner', () {
    final s = Strings({'action.apply': 'Apply'});
    expect(s.t('action.apply'), 'Apply');
    expect(s.t('yok.olan.anahtar'), 'yok.olan.anahtar');
    expect(
      Strings({'plan.summary': '{count} files'}).count('plan.summary', 3),
      '3 files',
    );
  });

  test(
    'SettingsStore son secilen klasorleri hatirlar (disk round-trip)',
    () async {
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
    },
  );

  // CLAUDE.md kural 4: her metin DORT ARB'de de olmak zorunda. gen-l10n noktali
  // anahtarlari reddettigi icin build-time kontrol yok — kontrol burada.
  test('i18n: dort ARB ayni anahtar kumesine sahip', () {
    Set<String> keysOf(String loc) {
      final j =
          json.decode(File('l10n/app_$loc.arb').readAsStringSync()) as Map;
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
      expect(
        az[k],
        isNot(equals(tr[k])),
        reason: '$k: az, tr kopyasi olmamali',
      );
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
    expect(
      AuthSession.fromJson({'accessToken': 'x'}),
      isNull,
      reason: 'refresh yok',
    );
  });

  test('AppUser: eksik alanlar erisim ACMAZ (guvenli varsayilan)', () {
    final u = AppUser.fromJson({'id': 'x', 'email': 'e@x.com'});
    expect(u.allowed, isFalse);
    expect(u.isAdmin, isFalse);
    expect(u.username, isNull);
  });

  test('AppUser.display: kullanici adi varsa o, yoksa e-posta', () {
    final withName = AppUser.fromJson({
      'id': 'x',
      'email': 'e@x.com',
      'username': 'ali_k',
    });
    expect(withName.display, 'ali_k');
    // username null VEYA bos -> e-postaya dus (bos string ekranda bosluk birakirdi)
    expect(
      AppUser.fromJson({'id': 'x', 'email': 'e@x.com'}).display,
      'e@x.com',
    );
    expect(
      AppUser.fromJson({'id': 'x', 'email': 'e@x.com', 'username': ''}).display,
      'e@x.com',
    );
  });

  // Bu kural sunucudaki public.username_ok ile AYNI olmali; ikisi ayrilirsa
  // istemci kabul edip sunucu reddeder ve kullanici anlamsiz hata gorur.
  test('UsernameRule: sunucu kuralini birebir yansitir', () {
    expect(UsernameRule.ok('ali'), isTrue); // min 3
    expect(UsernameRule.ok('ali_k.2-x'), isTrue);
    expect(UsernameRule.ok('a' * 24), isTrue); // max 24
    expect(UsernameRule.ok('  ali  '), isTrue); // trim edilir

    expect(UsernameRule.ok('al'), isFalse); // kisa
    expect(UsernameRule.ok('a' * 25), isFalse); // uzun
    expect(UsernameRule.ok(''), isFalse);
    expect(UsernameRule.ok('ali k'), isFalse); // bosluk
    expect(UsernameRule.ok('ali@k'), isFalse); // '@' YASAK: yoksa giriste
    expect(UsernameRule.ok('alı'), isFalse); // ASCII disi (Turkce 'ı')
  });

  // Kullanicinin istegi: MONOKROM. Bunu yorumla degil, olcerek koru — ileride
  // biri "su uyariyi kirmizi yapalim" derse test patlar.
  for (final brightness in [Brightness.light, Brightness.dark]) {
    test('tema (${brightness.name}) tamamen gri: her renkte R==G==B', () {
      final cs = AppTheme.build(
        brightness: brightness,
        font: AppFont.system,
        compact: false,
      ).colorScheme;

      final colors = <String, Color>{
        'primary': cs.primary,
        'onPrimary': cs.onPrimary,
        'primaryContainer': cs.primaryContainer,
        'secondary': cs.secondary,
        'surface': cs.surface,
        'onSurface': cs.onSurface,
        'onSurfaceVariant': cs.onSurfaceVariant,
        'surfaceContainer': cs.surfaceContainer,
        'surfaceContainerHigh': cs.surfaceContainerHigh,
        'error': cs.error,
        'onError': cs.onError,
        'errorContainer': cs.errorContainer,
        'outline': cs.outline,
        'outlineVariant': cs.outlineVariant,
        'inverseSurface': cs.inverseSurface,
      };

      colors.forEach((name, c) {
        expect(c.r, closeTo(c.g, 0.001), reason: '$name gri degil (r!=g)');
        expect(c.g, closeTo(c.b, 0.001), reason: '$name gri degil (g!=b)');
      });
    });
  }

  test('tema: metin ile zemin arasinda gercek kontrast var', () {
    for (final b in [Brightness.light, Brightness.dark]) {
      final cs = AppTheme.build(
        brightness: b,
        font: AppFont.system,
        compact: false,
      ).colorScheme;
      // Monokromda okunabilirligin TEK guvencesi bu fark; kaybolursa ekran okunmaz.
      final diff =
          (cs.onSurface.computeLuminance() - cs.surface.computeLuminance())
              .abs();
      expect(diff, greaterThan(0.5), reason: '${b.name}: kontrast yetersiz');
    }
  });

  test('Severity: her seviye FARKLI ikon — anlam renkte degil sekilde', () {
    final icons = Severity.values.map((s) => s.icon).toSet();
    expect(icons.length, Severity.values.length);
    expect(Severity.warn.emphasized, isTrue);
    expect(Severity.danger.emphasized, isTrue);
    expect(Severity.info.emphasized, isFalse);
  });

  // GERILEME TESTI. Onceki kod olcegi textTheme.apply(fontSizeFactor:) ile
  // uyguluyordu ve 1.0 disinda HER deger cokuyordu (TextStyle.apply assert'i:
  // Material 3'te fontSize'i null olan stiller var). Testlerin hepsi 1.0
  // geciyordu — yani hatayi ureten tek degeri hic denememisler. Kullanici font
  // degistirince cokme olarak geldi. Tema artik olcek BILMIYOR; yine de her
  // font/parlaklik kombinasyonunun kurulabildigini burada sabitliyoruz.
  test('tema: her font x parlaklik kombinasyonu cokmeden kurulur', () {
    for (final b in [Brightness.light, Brightness.dark]) {
      for (final f in AppFont.values) {
        for (final compact in [false, true]) {
          expect(
            () => AppTheme.build(brightness: b, font: f, compact: compact),
            returnsNormally,
            reason: '${b.name}/${f.id}/compact=$compact',
          );
        }
      }
    }
  });

  testWidgets('yazi olcegi uygulanir ve OS olcegiyle CARPILIR', (tester) async {
    late double scaled;
    await tester.pumpWidget(
      MediaQuery(
        // OS'un erisilebilirlik olcegi 2.0 diyelim.
        data: const MediaQueryData(textScaler: TextScaler.linear(2.0)),
        child: Builder(
          builder: (outer) => AppTheme.applyTextScale(
            outer,
            Builder(
              builder: (inner) {
                scaled = MediaQuery.of(inner).textScaler.scale(10);
                return const SizedBox();
              },
            ),
            1.5, // kullanici tercihi
          ),
        ),
      ),
    );
    // 10 * 1.5 * 2.0 — kullanici tercihi OS ayarini EZMEZ, uzerine biner.
    expect(scaled, 30.0);
  });

  testWidgets('yazi olcegi: null child cokme uretmez', (tester) async {
    // MaterialApp.builder child'i null verebilir; SizedBox'a dusmeli.
    await tester.pumpWidget(
      MediaQuery(
        data: const MediaQueryData(),
        child: Builder(builder: (c) => AppTheme.applyTextScale(c, null, 1.2)),
      ),
    );
    expect(tester.takeException(), isNull);
  });

  test('AppFont: sistem disi secenekler fallback zinciri tasir', () {
    for (final f in AppFont.values.where((f) => f != AppFont.system)) {
      expect(f.family, isNotNull);
      expect(
        f.fallback,
        isNotEmpty,
        reason: '${f.id}: her PC\'de bulunmayabilir',
      );
    }
    // Sistem varsayilani: aile YOK -> Flutter platform fontunu secer.
    expect(AppFont.system.family, isNull);
  });

  test('AppPrefs: diske yazar, geri okur, olcegi sinirlar', () async {
    final tmp = await Directory.systemTemp.createTemp('docflow_prefs');
    final f = File('${tmp.path}/settings.json');
    try {
      final p1 = AppPrefs(await SettingsStore.load(file: f));
      expect(p1.themeMode, ThemeMode.system);
      expect(p1.localeCode, isNull);
      expect(p1.fontScale, 1.0);

      await p1.setThemeMode(ThemeMode.dark);
      await p1.setLocale('az');
      await p1.setFont(AppFont.mono);
      await p1.setCompact(true);
      await p1.setFontScale(9.0); // sacma deger -> sinirlanmali

      expect(p1.fontScale, AppPrefs.maxScale);

      final p2 = AppPrefs(await SettingsStore.load(file: f)); // yeniden okundu
      expect(p2.themeMode, ThemeMode.dark);
      expect(p2.localeCode, 'az');
      expect(p2.locale, const Locale('az'));
      expect(p2.font, AppFont.mono);
      expect(p2.compact, isTrue);
      expect(p2.fontScale, AppPrefs.maxScale);

      await p2.reset();
      final p3 = AppPrefs(await SettingsStore.load(file: f));
      expect(p3.themeMode, ThemeMode.system);
      expect(p3.localeCode, isNull);
      expect(p3.font, AppFont.system);
      expect(p3.fontScale, 1.0);
    } finally {
      await tmp.delete(recursive: true);
    }
  });

  test(
    'AppPrefs: bozuk/bilinmeyen degerler cokme degil varsayilan verir',
    () async {
      final tmp = await Directory.systemTemp.createTemp('docflow_prefs_bad');
      final f = File('${tmp.path}/settings.json');
      try {
        await f.writeAsString(
          json.encode({
            'prefs': {
              'theme': 'neon',
              'font': 'comic',
              'fontScale': 'big',
              'locale': 42,
            },
          }),
        );
        final p = AppPrefs(await SettingsStore.load(file: f));
        expect(p.themeMode, ThemeMode.system);
        expect(p.font, AppFont.system);
        expect(p.fontScale, 1.0);
        expect(p.localeCode, isNull);
      } finally {
        await tmp.delete(recursive: true);
      }
    },
  );

  test('AppPrefs degisiklikte dinleyicileri uyarir (canli tema/dil)', () async {
    final tmp = await Directory.systemTemp.createTemp('docflow_prefs_notify');
    try {
      final p = AppPrefs(
        await SettingsStore.load(file: File('${tmp.path}/s.json')),
      );
      var n = 0;
      p.addListener(() => n++);
      await p.setThemeMode(ThemeMode.dark);
      expect(n, 1);
      await p.setThemeMode(
        ThemeMode.dark,
      ); // ayni deger -> gereksiz rebuild yok
      expect(n, 1);
      await p.setFontScale(1.2);
      expect(n, 2);
    } finally {
      await tmp.delete(recursive: true);
    }
  });
}
