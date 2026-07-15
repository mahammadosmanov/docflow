# Mimari

Tek cümle: **her özellik bir `Operation`'dır, UI şemasından üretilir, her dosya
işlemi preview → apply → journal'dan geçer.**

Geri kalan her şey bu üç cümlenin sonucu.

## Neden bu şekilde

Sahibinin beyanı: *"özellikler zaman geçtikçe eklenecek ve/veya düzenlenecek."*
O halde mimarinin optimize edeceği tek metrik: **yeni özellik eklemenin maliyeti.**

Hedef: yeni özellik = 1 Python dosyası + 4 ARB girdisi. UI'a dokunma yok,
IPC'ye dokunma yok, Flutter derlemesi yok.

Bu hedefe üç kararla ulaşılıyor:

### 1. Tek sözleşme (`contracts.py`)

Her operation `preview()` ve `apply()` implement eder, `ParamSchema` bildirir.
Sidecar `list_operations` ile bunları JSON olarak yayınlar. Flutter tarafı
operation'ların **ne yaptığını bilmez** — sadece şemayı okur.

### 2. Şema-güdümlü UI

`ParamKind` → widget eşlemesi tek yerde (`lib/ui/forms/schema_form.dart`).
Operation parametrelerini bildirir, form kendiliğinden çıkar.

Sızıntı testi: `schema_form.dart` içinde herhangi bir operation id'si geçiyorsa
mimari bozulmuştur.

### 3. preview / apply ayrımı

```
preview(params) -> Plan     # dosya sistemine DOKUNMAZ, sadece okur
apply(plan)     -> Result   # SADECE plan.steps'i uygular, hesap YAPMAZ
```

Bu ayrım pazarlık konusu değil. `apply()` içinde yeniden hesaplama yapmak,
kullanıcının **onayladığı şey** ile **yapılan şey** arasında sessiz bir fark açar.
Bu uygulama kullanıcının belgelerini taşıyor; o fark veri kaybıdır.

Yan fayda: dry-run bedava. Undo bedava.

## Journal

Her `apply()` ters çevrilebilir bir kayıt yazar (`~/.docflow/journal/`).
Undo = adımları **ters sırada** ters çevir.

Journal, adım uygulanmadan **önce** yazılır. Çökme anında yarım iş kalırsa
journal'da fazladan kayıt olur — zararsız (undo o adımı atlar). Tersi veri kaybı.

Undo kuralları:
- `move` → geri taşı
- `copy` → hedefi sil
- `mkdir` → **sadece boş klasörü** sil. Kullanıcı içine bir şey koyduysa dokunma.

## Veri akışı

```
Flutter                          Sidecar (Python)
───────                          ────────────────
list_operations  ─────────────►  registry.all_operations()
                 ◄─────────────  [{id, titleKey, schema}]

şemadan form üret
kullanıcı doldurur

preview(op, params) ──────────►  op.preview(params)
                                   ├─ extract.first_page_text()  ── metin katmanı? yoksa OCR
                                   ├─ extract.find()             ── strict → repaired → fuzzy
                                   └─ Plan(steps, warnings)
                 ◄─────────────  Plan

planı tablo olarak göster
kullanıcı ONAYLAR                 ◄── tek insan kararı burada

apply(planId)   ──────────────►  fs.apply_steps(plan)
                                   ├─ journal yaz (ÖNCE)
                                   └─ adımı uygula (SONRA)
                 ◄─────────────  Result(applied, failed, journalId)
```

## Katman kuralı

Flutter **hiçbir belge işi yapmaz.** Parametre toplar, planı gösterir, onay alır,
sonucu gösterir. PDF açmaz, OCR yapmaz, dosya taşımaz.

Gerekçe: Dart'ta OCR yok, docx/pptx manipülasyonu yok. Bugün bir istisna yaparsan
yarın belge mantığı iki dile bölünür ve hangi tarafta olduğunu kimse bilmez.

## Genişletme örneği

`pdf.merge` eklemek:

1. `sidecar/docflow_sidecar/ops/pdf_merge.py` yaz — `@register("pdf.merge")`,
   şema bildir, `preview`/`apply` yaz
2. `ops/__init__.py`'ye tek satır import ekle
3. Dört ARB dosyasına `op.pdf.merge.title` / `.desc` ekle

Bitti. Flutter'a dokunmadın. Yeniden derlemedin. UI kendiliğinden çıktı.

Bu adımlardan fazlasını yapman gerekiyorsa **dur ve mimariyi konuş** —
sızıntı var demektir.
