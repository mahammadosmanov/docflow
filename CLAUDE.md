# DocFlow

> Proje adı geçici. Değiştirmek için: bu dosya, `app/pubspec.yaml`, `sidecar/pyproject.toml`, klasör adı.

Masaüstü belge otomasyon uygulaması. Kullanıcının elle yaptığı PDF/Word/Excel/PowerPoint
işlerini otomatikleştirir. Özellikler zaman içinde sürekli eklenecek — bu yüzden mimarinin
tek işi **yeni özellik eklemeyi ucuzlatmak**.

MVP: taranmış RFI formlarını ilk sayfadaki RFI numarasına göre yeniden adlandır ve
her RFI için klasör aç.

---

## Stack

| Katman | Teknoloji | Neden |
|---|---|---|
| UI | Flutter Desktop (macOS / Windows / Linux) | Sahibi Flutter geliştiricisi; i18n (ARB) çözülmüş |
| İş mantığı | Python sidecar | PyMuPDF / Tesseract / python-docx / openpyxl / python-pptx — Dart'ta karşılığı yok |
| IPC | stdin/stdout üzerinde satır-ayrımlı JSON-RPC | HTTP kurumsal Windows'ta firewall pop-up'ı yer |

Flutter **hiçbir belge işi yapmaz**. Sadece: parametre topla, sidecar'a gönder, planı göster,
onay al, sonucu göster. Belge mantığı Python'da, istisnasız.

---

## Değişmez kurallar

Bunlar tartışmaya açık değil. Bir tanesini bile ihlal eden PR reddedilir.

1. **Preview olmadan apply yok.** Her dosya işlemi önce `preview()` ile bir `Plan` üretir.
   Kullanıcı planı görür ve onaylar. `apply()` sadece onaylanmış bir `Plan` alır ve
   *başka hiçbir şey hesaplamaz*.
2. **Journal olmadan apply yok.** Her `apply()` ters çevrilebilir bir journal kaydı yazar.
   Undo bir özellik değil, mimarinin sonucu.
3. **Sidecar stdout'a JSON-RPC dışında hiçbir şey yazmaz.** Tek bir `print()` protokolü bozar.
   Log → `stderr`. Bu, hata ayıklarken en çok zaman kaybettiren tuzak.
4. **Kullanıcıya görünen hiçbir string koda gömülmez.** Her metin bir i18n anahtarıdır ve
   **dört ARB dosyasının hepsinde** karşılığı olmak zorunda: `tr`, `en`, `ru`, `az`.
   Anahtarlar noktalı (`op.rfi.title`), `gen-l10n` bunu reddediyor → ARB'ler **runtime**
   yükleniyor (`core/l10n/strings.dart`) ve `l10n.yaml` devre dışı. Yani build seni
   yakalamaz; **anahtar eşitliğini `flutter test` içindeki i18n testi zorlar**. Çözülemeyen
   anahtar ekranda anahtarın kendisi olarak görünür (sessiz boşluk yok).
5. **`az` ve `tr` çevirilerini paylaştırma.** Yakınlar, aynı değiller.
6. **Her Operation için elle form yazma.** UI, `ParamSchema`'dan üretilir. Yeni bir operation
   eklerken `lib/ui/` altında hiçbir dosyaya dokunmuyor olman lazım. Dokunuyorsan şema
   eksik demektir — şemayı genişlet, form yazma.
7. **RFI kodunu asla küçült.** `NFC + UPPER` canonical forma bir kez çevir, bir daha case
   dönüşümü uygulama. Gerekçe: Python locale-aware casing yapmaz; Türkçe I/ı ikilisi ve
   `"İ".lower()` → `i̇` (i + U+0307 combining dot) dosya adı karşılaştırmasını sessizce bozar.
8. **Yeni Operation = tek Python dosyası + 4 ARB girdisi.** Başka hiçbir yere dokunma.
   Dokunman gerekiyorsa mimari bozulmuş, önce onu konuş.
9. **Arayüz monokrom.** Palet tek gri skalasıdır (`ui/theme/app_theme.dart`) ve her tonda
   `R == G == B`. "Neredeyse gri" paletler (Tailwind `zinc` gibi) maviye çalar — bu gerçekten
   oldu, testi yazana kadar fark edilmedi. `tema tamamen gri` testi bunu ölçer.
10. **Anlamı renk taşımaz, ikon/kenarlık taşır.** Monokromda hue yok: uyarı ≠ hata ayrımı
   `Severity` ikonuyla, kalın çerçeveyle ve metinle yapılır. Bu bir estetik tercih değil —
   uygulama kullanıcının belgelerini taşıyor, "kırmızıya boyayıp" geçmek yok. Yan fayda:
   renk körü kullanıcı için de aynı derecede okunur.
11. **Ekranlarda çıplak sabit yok.** Renk → `Theme.of(context).colorScheme`; yazı boyutu →
   `textTheme` (kullanıcı ölçeği ayarlardan gelir, `fontSize: 13` yazarsan onu ezer);
   genişlik/boşluk → `ui/theme/responsive.dart` (`FormFactor`, `Pane`). Dar pencere de
   desteklenen bir durumdur, 1366×768 ofis dizüstüyle 4K masaüstü aynı kodu paylaşır.

---

## Mimari: Operation sözleşmesi

Her özellik aynı sözleşmeyi implement eder. Uygulamanın tamamı bu soyutlamanın üstünde durur.

```python
@register("rfi.process_scans")
class ProcessScans(Operation):
    title_key = "op.rfi.title"
    description_key = "op.rfi.desc"
    schema = ParamSchema([
        FolderParam("source", "param.source"),
        FolderParam("dest",   "param.dest"),
        BoolParam("dry_run",  "param.dry_run", default=True),
    ])

    def preview(self, params: dict) -> Plan: ...   # HİÇBİR ŞEYİ DEĞİŞTİRMEZ
    def apply(self, plan: Plan) -> Result: ...     # SADECE plan.steps'i uygular
```

Flutter tarafı operation'ları `list_operations` ile sidecar'dan çeker. Yani yeni özellik
eklerken **Flutter'ı yeniden derlemek gerekmez** — sadece çeviri metinleri için.

Ayrıntı: `docs/ARCHITECTURE.md`. IPC: `docs/IPC_PROTOCOL.md`.

---

## OCR: ölçülmüş gerçekler

Aşağıdakiler tahmin değil, gerçek belge fotoğrafı üzerinde ölçüldü. **Bunları yeniden
keşfetmeye çalışma, sorgulamadan uygula.**

| Denenen | Sonuç |
|---|---|
| Tam sayfa OCR | **MISS** — çalışmıyor |
| Üst bant crop + global Otsu | **MISS** — çalışmıyor |
| Üst bant crop (%35) + adaptive threshold | **HIT**, conf 1.00 |

- **Global Otsu kullanma.** Tek eşik seçer; sayfadaki ışık gradyanı basılı gri metni beyaza
  düzler. `cv2.adaptiveThreshold(..., ADAPTIVE_THRESH_GAUSSIAN_C, ...)` kullan.
- **Üst bant crop opsiyonel değil.** Hız optimizasyonu değil, çalışma koşulu. Tesseract
  ~30px cap height ister; tam sayfada kod metni ~12px ve okunamıyor.
- **`RFI No` etiketine çapa atma.** Etiket OCR'da `REI N0` oluyor — etiket çürüyor, kod
  hayatta kalıyor. Sadece desen-çapalı eşleştirme:
  `AZE-SUN-INT-QMS-RFI-([A-Z]{3})-(\d{4})`
- **Sayfada tuzak kodlar var:** `AZE-SUN-INT-QMS-ITP-MEC-0001`, `AZE-SUN-INT-ENG-STR-DRG-0901`.
  Aynı şekiller. `-RFI-` çapası ikisini de eliyor. Regex'i gevşetme.
- **Tarih ve Rev'i OCR'dan çekme.** Bu kalitede güvenilmez — tarih yanlış okundu (18 ← 19),
  Rev tamamen çözülemedi.
- **DPI kalibre edilmemiş.** `tools/rfi_probe.py --sweep` gerçek taramalar üzerinde çalıştırılıp
  sonuç `sidecar/docflow_sidecar/extract/ocr.py` içindeki `DPI`/`BLOCK` sabitlerine yazılmalı.
  Sahte çözünürlük (düşük kaliteli kaynağı 300 DPI'da render etmek) doğruluğu **düşürür**.
- **Metin katmanını her zaman önce dene.** Tarayıcı "searchable PDF" üretiyorsa OCR'a hiç
  girme — daha hızlı ve %100 doğru.

Referans implementasyon (çalışıyor, test edilmiş): `sidecar/docflow_sidecar/extract/`.

---

## RFI adlandırma kararı

Aynı RFI numarası birden fazla revizyonda var olabilir (`Rev: 0`, `Rev: 1`).

```
{dest}/AZE-SUN-INT-QMS-RFI-MEC-0107/          ← klasör: rev YOK
       ├── AZE-SUN-INT-QMS-RFI-MEC-0107_R0.pdf   ← dosya: rev VAR
       └── AZE-SUN-INT-QMS-RFI-MEC-0107_R1.pdf
```

Bir RFI'ın tüm revizyonları tek klasörde. Rev OCR'dan gelmiyor (güvenilmez) — çakışma
sırasına göre `_R0`, `_R1` diye artır ve planı kullanıcıya göster, düzeltsin.

**Kod bulunamayan dosyalar `{dest}/_review/` altına gider.** Asla sessizce atlanmaz,
asla tahmini bir isimle adlandırılmaz.

---

## Dizin yapısı

```
app/                        Flutter
  lib/core/ipc/             sidecar client, JSON-RPC
  lib/core/operations/      Operation contract, registry, ParamSchema
  lib/core/journal/         undo
  lib/core/settings/        kalıcı ayarlar
  lib/ui/shell/             navigasyon, layout
  lib/ui/forms/             ŞEMADAN ÜRETİLEN widget'lar — operation'a özel form YOK
  l10n/                     app_tr.arb  app_en.arb  app_ru.arb  app_az.arb

sidecar/docflow_sidecar/
  rpc.py                    stdio JSON-RPC döngüsü
  contracts.py              Operation, Plan, Result, ParamSchema
  registry.py               @register
  ops/                      operation başına BİR dosya
  extract/                  pdf_text.py  ocr.py  rfi_code.py
  fs/                       plan.py  apply.py (atomik + journal)

tools/rfi_probe.py          salt-okunur kalibrasyon aracı
```

---

## Doğrulama

Kod yazdıktan sonra bunları çalıştır. Hepsi geçmeden iş bitmiş sayılmaz.

```bash
# sidecar
cd sidecar && python -m pytest -q && python -m mypy docflow_sidecar

# protokol dumanı: sidecar sadece JSON konuşuyor mu?
echo '{"id":1,"method":"list_operations","params":{}}' | python -m docflow_sidecar | head -1

# flutter
cd app && dart analyze && flutter test

# i18n: eksik çeviri var mı?
cd app && flutter gen-l10n && test ! -s l10n/untranslated.json
```

---

## Yazarken

- Küçük commit, tek konu.
- Yeni bağımlılık eklemeden önce sor.
- `# TODO` bırakma; ya yap ya `docs/TASKS.md`'ye taşı.
- Testler önce. Özellikle `extract/rfi_code.py` — saf fonksiyon, test etmemek için mazeret yok.
- Emin değilsen dosya sistemine dokunma. Bu uygulama kullanıcının belgelerini taşıyor;
  bir veri kaybı projeyi öldürür.
