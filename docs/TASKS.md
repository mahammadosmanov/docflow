# Görev sırası

Claude Code'a **tek tek** ver. Her görevin sonunda doğrulama komutu var — geçmeden
sonrakine geçme. Sıra tesadüfi değil: her adım öncekinin üstünde duruyor ve
riskli olanlar (dosya sistemi) en sona bırakıldı.

Her oturuma şununla başla:

```
CLAUDE.md ve docs/ARCHITECTURE.md oku. Sonra Görev N'i yap.
Değişmez kuralları ihlal etme. Bitince doğrulama komutlarını çalıştır.
```

---

## Faz 0 — Kalibrasyon (kod yazmadan önce)

### Görev 0.1 — DPI sabitini ölç
**Claude Code'a değil, sana.**

```bash
python3 tools/rfi_probe.py /gercek/taramalar --sweep
```

Kazanan `(dpi, blok)` çiftini `sidecar/docflow_sidecar/extract/ocr.py` içindeki
`DPI` / `BLOCK` sabitlerine yaz.

İsabet oranı **%95+** → devam et.
**%70 civarı** → dur. Mimari değil, tarayıcı ayarları kazandırır. Tarayıcıda
"searchable PDF" / OCR seçeneğini aç, 300 DPI'a çık, tekrar ölç. Yazılım kötü
girdiyi düzeltemez.

### Görev 0.2 — Disiplin kodlarını topla
Gerçek taramalarda gördüğün tüm disiplin kodlarını
`extract/rfi_code.py` → `DISCIPLINES` kümesine ekle. Bu küme fuzzy yolun
yanlış eşleşmesini engelliyor — ne kadar tamsa o kadar güvenli.

---

## Faz 1 — Sidecar (hazır, sadece doğrula)

### Görev 1.1 — Ortamı kur ve testleri geçir
```
sidecar/ klasöründe sanal ortam kur, requirements-dev.txt yükle,
testleri çalıştır. Tesseract binary'si yoksa kurulum komutunu söyle.
```
**Doğrulama:** `cd sidecar && python -m pytest -q` → 15 passed

### Görev 1.2 — mypy temiz
```
sidecar/ üzerinde mypy'ı temiz geçir. Type ignore ekleyerek değil,
tipleri düzelterek. rpc.py içindeki stdout guard'daki ignore kalabilir.
```
**Doğrulama:** `cd sidecar && python -m mypy docflow_sidecar`

---

## Faz 2 — Flutter iskeleti

### Görev 2.1 — Proje + i18n
```
app/ içinde Flutter desktop projesi oluştur (macOS, Windows, Linux).
l10n/ altındaki dört ARB dosyası ve l10n.yaml zaten var — bunları KULLAN,
yeniden üretme. flutter gen-l10n çalışsın ve untranslated.json boş çıksın.

Uygulama dili sistem dilinden gelsin, ayarlardan değiştirilebilsin.
Desteklenen: tr, en, ru, az. Fallback: en.
```
**Doğrulama:** `cd app && flutter gen-l10n && test ! -s l10n/untranslated.json && dart analyze`

### Görev 2.2 — Sidecar client
```
lib/core/ipc/sidecar_client.dart yaz. docs/IPC_PROTOCOL.md'deki sözleşmeyi
birebir uygula.

- Process.start ile sidecar'ı başlat, stdin/stdout'a bağlan
- Satır-ayrımlı JSON. Her istek bir id alır, Future ile eşleşir
- id'siz mesajlar = notification (progress) → Stream'e ver
- stderr'i uygulama log'una yaz, ASLA parse etme
- Sidecar ölürse yeniden başlat, üst üste 3 kez ölürse error.sidecar_down

Test: sahte bir sidecar process (echo script) ile unit test yaz.
```
**Doğrulama:** `cd app && flutter test test/ipc/`

### Görev 2.3 — Şemadan form üretimi
```
lib/ui/forms/schema_form.dart yaz.

ParamSchema JSON'ını alır, Widget üretir. ParamKind → widget eşlemesi:
  folder  → klasör seçici (file_picker)
  file    → dosya seçici
  text    → TextField
  bool    → Switch
  choice  → DropdownButton
  int     → sayısal TextField

labelKey ve helpKey i18n'den çözülür. Düz metin YOK.

>>> Bu dosya operation'a özel HİÇBİR ŞEY bilmemeli. <<<
"if (opId == 'rfi.process_scans')" yazıyorsan mimariyi bozuyorsun.
```
**Doğrulama:** `cd app && flutter test test/ui/schema_form_test.dart`

---

## Faz 3 — Akış

### Görev 3.1 — Operation listesi ekranı
```
Açılışta list_operations çağır, operation'ları kart olarak listele
(titleKey + descriptionKey). Tıklanınca şemadan üretilmiş form açılsın.

Minimalist: tek sütun, bol boşluk, çerçeve yok. Süsleme yok.
```

### Görev 3.2 — Preview ekranı
```
Form doldurulup Önizle'ye basınca preview çağır, Plan'ı tablo olarak göster:

  kaynak dosya → hedef yol | not | güven

Renk kodu: mavi = taşınacak, sarı = _review'a gidecek, kırmızı = uyarı.
Uyarılar üstte özet olarak.

>>> Uygula butonu, plan boşsa veya sadece uyarı içeriyorsa PASİF. <<<
```

### Görev 3.3 — Apply + ilerleme + sonuç
```
Uygula'ya basınca apply çağır. progress notification'larını dinle,
determinate progress bar göster.

Sonuçta: N uygulandı, M başarısız, journalId. Geri al butonu.
Başarısız adımlar listelenir, sessizce yutulmaz.
```

### Görev 3.4 — Undo
```
sidecar'a "undo" method'u ekle (fs/apply.py'de undo() zaten yazılı,
sadece __main__.py'ye handler ekle). Flutter'da Geri al butonuna bağla.
Son 20 journal kaydını gösteren bir geçmiş ekranı ekle.
```
**Doğrulama:** gerçek bir klasörde: uygula → geri al → klasör başlangıç durumunda mı?

---

## Faz 4 — Paketleme

### Görev 4.1 — Sidecar'ı bundle et
```
PyInstaller ile sidecar'ı tek binary yap. Flutter'ın bundle'ına asset
olarak koy, çalışma anında çıkar ve çalıştır.

Platform matrisi: macOS (arm64), Windows (x64), Linux (x64).
Tesseract binary'si de bundle'a girmeli — sistemde kurulu olduğunu varsayma.

Bu görev tahmin edilenden uzun sürer. Beklentini ona göre ayarla.
```

---

## Sonraki özellikler (mimariyi doğrulayan test)

Aşağıdakilerin her biri **tek Python dosyası + 4 ARB girdisi** olmalı.
Başka bir yere dokunman gerekiyorsa mimari sızdırıyor demektir — durup onu konuş.

- `rfi.split_multi` — bir taramada birden çok RFI varsa PDF'i böl
- `rfi.extract_status` — Passed/Failed kutusunu oku, duruma göre klasörle
- `xlsx.rfi_register` — işlenen RFI'lardan Excel kayıt defteri üret
- `pdf.merge` / `pdf.split`
- `docx.find_replace`

---

## Bilinen açıklar

Bunlar bilinçli olarak bırakıldı, unutulmadı:

1. **Çeviriler ham.** ARB dosyaları benim yazdığım çevirilerle dolu.
   `az` ve `ru`'yu anadili konuşan biri gözden geçirsin. `az` ve `tr` yakın
   ama aynı değil — kopyala-yapıştır yapma.
2. **Rev OCR'dan gelmiyor.** Görülme sırasına göre atanıyor. Kullanıcı planda
   düzeltebilmeli — Görev 3.2'de düzenlenebilir hedef yolu düşün.
3. **`_PLANS` bellekte.** Sidecar restart'ında planlar kaybolur. MVP için kabul;
   plan üretimi ucuz.
4. **Windows 260 karakter path limiti.** Hedef kök derinse patlar.
   Görev 3.2'de preview sırasında uzunluk kontrolü ekle.
5. **Concurrent apply koruması yok.** İki apply aynı anda çalışırsa journal
   karışır. Tek kullanıcılı masaüstünde düşük risk, ama not düşüldü.
6. **Undo boş `_review/` klasörünü bırakıyor.** E2E testte görüldü. Sebep:
   `_review` dizini `apply` sırasında `dst.parent.mkdir()` ile örtük oluşuyor,
   açık bir `mkdir` adımı yok, dolayısıyla journal'da karşılığı yok.
   Kozmetik (veri kaybı değil). Düzeltme: `preview()` `_review` için de açık bir
   `mkdir` adımı üretsin. **Isınma görevi olarak iyi** — mimariyi bozmadan
   dokunmayı öğretir.
