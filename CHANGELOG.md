# Changelog

All notable DocFlow changes are recorded here. The project follows Semantic
Versioning; `releases.json` is the machine-readable release catalog.

## Unreleased

Record user-visible changes here while they are being developed. Move them into
a dated version section as part of the release commit.

## 0.7.0-rc.2 - 2026-09-11

### Added

- Automated cross-compilation pipeline for Windows x64 (`tools/build_windows_cross.sh`) packaging standalone Tesseract OCR binaries and `eng.traineddata` into both NSIS installer and zero-install portable zip.
- HTML5 drag-and-drop on all single and multi-file/folder form inputs with monochrome active drop states.
- Automatic parameter persistence and re-issue of opaque path tokens on operation load.
- 1-click diagnostic report download in Preferences -> Support.
- Robust header discovery across non-standard contractor RFI logs (detecting RFI and Worley status columns across first 10 rows and 64 columns).

### Safety

- Preserved strict non-worksheet sheet relationship checks in OOXML workbook discovery.
- All 321 Rust tests and 39 frontend/tooling tests passing with zero warnings.

## 0.7.0-rc.1 - 2026-08-29

### Added

- Exact, package-preserving RFI Follow Up preview, revision, apply, recovery,
  and targeted undo with bounded authority and no-clobber publication.
- Windows-only Outlook Reply All draft preparation that revalidates the exact
  selected message, preserves the existing quote/signature, displays only an
  unsent draft, and never synthesizes recipients or calls Send.
- A bounded, redacted, local-only diagnostics preview/copy surface with no
  telemetry or user-document scan.
- Commit-bound Windows installer, checksum, smoke, cold-start, peak-memory,
  package-audit, uninstall, and artifact-manifest evidence.

### Safety

- RFI matching is exact whole-value matching; only `Completed → Completed` and
  `Ongoing → In Progress` are automatically selectable.
- Outlook draft undo is available only for the exact unchanged durable draft;
  otherwise it fails closed.

### Changed

- The desktop application authority is fully Rust-owned; the retired application
  sidecar, interpreter runtime, child-process routing, and frozen packaging have
  been removed.
- Version, source/package audit, dependency-license, checksum, and packaged-smoke
  gates now run with Node, Rust, and platform-native tools only.

### Safety

- Final builds fail fast if retired runtime paths or shell/external-binary wiring
  reappear, and package inspection rejects interpreter libraries, bytecode,
  frozen-runtime artifacts, or an application sidecar.
- The packaged binary smoke verifies isolated bootstrap/settings plus real OCR and
  Process Scans apply/journal/undo before an artifact can be accepted.

## 0.6.0 - 2026-08-21

### Added

- Tauri 2 + React/TypeScript desktop shell while preserving the proven Python
  document engine and private JSON-lines sidecar boundary
- Safe signed-RFI first-page staging with read-only archive guarantees,
  duplicate/ambiguous match reporting, and original folder-structure retention
- Persistent, cancellable, resumable mixed-file print queues with per-file,
  group, and all-files configuration
- Compact individual, sheet, RFI-group, and all-row RFI Log editing
- Canonical `VERSION`, machine-readable `releases.json`, repository-wide version
  checks, complete historical changelog, and tag-driven release workflow

### Changed

- The active UI stack moves from Flutter/PySide rollback implementations to a
  lean Tauri webview; legacy settings, journals, trash, and Supabase rows remain
  compatible
- Windows packages are native NSIS installers with the Python sidecar and pinned
  Tesseract bundled without a companion console window

### Safety

- Webview code never receives Supabase access/refresh tokens or arbitrary
  filesystem/network authority
- Every mutation remains preview → approval → apply → journal → undo
- Signed archive PDFs remain read-only; publication never overwrites an existing
  review file and recovers safely from OneDrive/antivirus locks
- Release builds require synchronized Python, Node, Rust, Tauri, and lockfile
  versions before packaging

## 0.5.4 - 2026-08-03

### Fixed

- RFI inspection description metinlerindeki UTF-8 bullet karakterinin Windows
  code-page katmaninda `â€¢` olarak gorunmesi; sidecar JSON hatti artik Unicode'u
  ASCII-safe escape'lerle tasiyor ve istemci dogru `•` karakterini geri kuruyor
- `CIV-5.2.2` ve `CIV-5.2.3` icin description, forms, AZFEN type ve INTEC type
  degerleri uretim workbook'undaki gercek Sheet1 kayitlariyla sabitlendi

### Safety

- Workbook'tan okunan tum lookup metinleri NFC normalize edilir ve yalnizca
  guvenli oldugunda UTF-8/CP1252 mojibake otomatik onarilir
- Tum CIV, MEC, ELE ve PRC presetlerinde bos alan, uyusmayan AZFEN/INTEC item
  baglantisi veya cakisan duplicate tespit edilirse preview artik durur
- 234 uretim presetinin tamami gercek RFI Log uzerinde read-only denetlendi;
  eksik, cakisan veya supheli encoding kaydi bulunmadi

## 0.5.3 - 2026-08-01

### Added

- MEC RFI kayitlarinda `ROW No.` alani artik karma sheet editorunde gorunur ve
  duzenlenebilir; deger Excel'deki gercek H sutununa yazilir ve dogrulanir
- MEC icin Excel tabanli mail onizleme/kopyalama `ROW No.` basligini, degerini ve
  kaynak workbook bicimini sheet'e ozel 15 kolonlu secime dahil eder

### Changed

- Item No. secenekleri UI'da `CIV-`, `MEC-`, `ELE-` ve `PRC-` onekleri olmadan
  gosterilir; plan ve Excel'e yazilan asil deger tam disiplin onekini korur
- Kompakt RFI satir aksiyonlari Material'in guvenli tiklama alanini koruyacak
  genislikte tutulur; uc buton artik hucre disina tasmaz

### Safety

- Karma CIV/MEC editorunde yalniz MEC Row No. hucresinin duzenlenebilir oldugunu,
  MEC H sutununa yazmayi ve workbook-backed kopyaya alinmasini kalici testler
  dogrular

Bu proje [Semantic Versioning](https://semver.org/) kullanır. Kullanıcıya görünen
her feature veya fix, yeni Windows paketi üretilmeden önce burada kaydedilir.

## 0.5.2 - 2026-07-31

### Fixed

- Sheet bazlı Excel kopyalama butonlarının dar pencerelerde Done butonunun
  arkasına taşması; kopyalama aksiyonları artık kendi tam genişlikte, satır
  kırabilen alanında ve alt footer'ın üzerinde duruyor
- Çoklu-sheet RFI işleminde inspection saatlerinin bütün sheet'ler için tek
  ortak zaman çizelgesinden verilmesi; CIV, MEC, ELE ve PRC artık yalnızca kendi
  sheet'indeki mevcut saatleri devam ettiriyor

### Safety

- Dört kopyalama butonunun kompakt pencere genişliğinde Done alanıyla
  çakışmadığını doğrulayan widget testi
- CIV overflow, MEC devam saati ve bağımsız ELE/PRC başlangıç saatlerini aynı
  çoklu-sheet planında doğrulayan kalıcı regresyon testi

## 0.5.1 - 2026-07-30

### Fixed

- Yerel Windows paketinde PyInstaller sidecar klasörünün yanlışlıkla bir seviye
  fazla içeri kopyalanması nedeniyle arka plan servisinin başlayamaması
- Uygulamanın standart düz paket yerleşimine ek olarak iç içe PyInstaller
  yerleşimini de güvenli fallback olarak algılaması

### Safety

- Yerel runner tabanlı paketleyici sidecar klasörünün kendisini değil içeriğini
  kopyalıyor, beklenen `sidecar\docflow_sidecar.exe` yolunu zorunlu tutuyor ve
  paketi teslim etmeden önce gerçek executable ile `ping` çalıştırıyor
- Düz ve iç içe paket yerleşimleri ile geliştirme override'ı Flutter testleriyle
  kalıcı olarak doğrulanıyor

## 0.5.0 - 2026-07-30

### Added

- CIV, MEC, ELE ve PRC için aynı işlem içinde ayrı adetler seçebilen kompakt
  çoklu-sheet RFI seçim alanı
- Sheet bilgisi görünür tek düzenleme tablosunda tüm otomatik alanları değiştirme,
  Area bazlı aynı RFI satırını çoğaltma ve özel değer yazma desteği
- İşlem sonucunda yalnızca kayıt eklenen sheet'ler için ayrı Excel-native
  kopyalama butonları

### Changed

- RFI numarası, sonraki satır ve ITP/Item eşleşmeleri her sheet için bağımsız
  hesaplanırken inspection saatleri tüm sheet'lerde çakışmayı engelleyecek şekilde
  ortak planlanıyor
- Mail okuma CIV/MEC ile sınırlı değil; ELE ve PRC disiplinlerini de algılıyor
- Çoklu-sheet apply/undo işlemi tek workbook yazımı ve tek geri alma kaydıyla
  atomik olarak çalışıyor

### Safety

- CIV, MEC, ELE ve PRC formülleri, farklı sütun yerleşimleri, item/duplicate veri
  doğrulamaları ve dokunulmayan Expiry/Status/Validity alanları kalıcı testlerle
  korunuyor
- 18 binden fazla satırlı gerçek RFI Log güvenli kopyasında dört sheet birlikte
  uygulandı, sheet bazlı mail tabloları tekrar Excel'den okundu ve undo'nun dosyayı
  hash seviyesinde birebir geri getirdiği doğrulandı

## 0.4.4 - 2026-07-30

### Fixed

- Excel penceresinin filtreleme ve native clipboard kopyası tamamlanmadan
  görünmesi; kullanıcının seçim veya hücre değişikliğiyle COM otomasyonunu
  durdurabilmesi
- Excel artık hazırlık boyunca gizli, ekran güncellemesi ve kullanıcı
  etkileşimi kapalı çalışıyor; yalnız kopya hazırlandıktan sonra görünür ve
  etkileşimli hale geliyor

### Safety

- Script sırası için kalıcı test: Excel görünürlüğü ve etkileşimi yalnız
  `Range.Copy()` tamamlandıktan sonra açılabilir
- Gerçek Excel integration testi native Biff12/HTML/RTF biçimlerini ve doğru
  iki RFI satırını gizli hazırlık akışıyla doğruluyor

## 0.4.3 - 2026-07-30

### Fixed

- Birden fazla farklı RFI numarası kopyalanırken bazı Office type-library
  sürümlerinin `AutoFilter(..., 7)` COM çağrısını reddetmesi
- Çoklu seçimde aynı native Excel görünümünü problemli COM overload'u yerine
  hedef satırların görünürlüğünü doğrudan yöneterek oluşturma
- Başarısız Excel otomasyonunun boş `EXCEL.EXE /automation -Embedding`
  süreçleri bırakması; hata sinyali cleanup sonrasına taşındı ve PID tabanlı
  ikinci kapatma güvencesi eklendi

### Safety

- Gerçek Microsoft Excel integration testi iki farklı RFI için genişletildi:
  yalnız hedef satırlar, header ve Subcontractor kopyalanırken eski RFI ve
  Expiry panoya alınmıyor
- Kontrollü hata testinde yeni Excel süreci kalmadığı doğrulandı

## 0.4.2 - 2026-07-30

### Fixed

- 18 binden fazla satırlık RFI Log dosyasını kopyalama öncesinde rastgele hücre
  erişimiyle tekrar tekrar tarayan ve butonu süresiz yükleniyor durumunda
  bırakan okuma yolu tek geçişli akışa çevrildi
- Gerçek boyuttaki workbook'ta mail tablosu verisi saniyeler içinde hazırlanıp
  Excel'in native biçimli panosuna aktarılıyor

### Safety

- Büyük read-only worksheet'lerde veri satırlarına rastgele erişimi reddeden
  kalıcı regresyon testi eklendi
- Üretim boyutundaki güvenli workbook kopyasıyla Excel Biff12/Biff8, HTML ve
  RTF pano formatları uçtan uca doğrulandı

## 0.4.1 - 2026-07-30

### Fixed

- Güncel RFI Log dosyasında K:Q'dan L:R'ye kayan Sheet1 eşleme kolonlarının
  header yapısından dinamik bulunması
- Yeni CIV/MEC satırlarında VLOOKUP formüllerinin gerçek eşleme kolonlarıyla
  yazılması; calculated-column ve data-validation uyumluluğunun korunması
- Excel'deki bozuk ayraçlı ITP adlarının normal mail metniyle eşleşmesi ve
  birleşik MEC Tag değerlerinin kısaltılmadan çıkarılması
- Uygulama içindeki yaklaşık mail tablosu önizlemesi yerine gerçek workbook'un
  Excel'de filtrelenip header dâhil native Office formatlarıyla kopyalanması

### Safety

- RFI yazma testi gerçek üretim workbook kopyasında Excel COM ile açılma,
  formül tutarlılığı ve data-validation sonuçları üzerinden doğrulandı
- Kopyalama yalnız Excel'in kendi panosunu kullanır; Biff, HTML, RTF ve Office
  bağlantı formatları uygulama tarafından yeniden yazılmaz

## 0.4.0 - 2026-07-29

### Added

- Process Scans için varsayılan mevcut klasörlü davranışı koruyan, isteğe bağlı
  klasörsüz çıktı modu
- Klasörsüz modda adlandırılmış PDF'leri doğrudan seçilen hedef klasöre
  taşıma veya orijinalleri koruma seçiliyse kopyalama

### Safety

- Eski kayıtlı ayarlar ve eski istemci istekleri için klasörlü davranışın
  otomatik olarak korunması
- Process Scans klasörlü/klasörsüz hedef, kopyalama, apply ve diğer özellikler
  için genişletilmiş regresyon kontrolleri

## 0.3.0 - 2026-07-29

### Added

- Uygulama sonucunda Excel'den yeniden okunan başlık ve hücrelerle Outlook
  mail tablosu önizleme penceresi
- Excel kolon genişliklerini kullanan yatay/dikey kaydırılabilir, hücre metni
  seçilebilir tablo ve tek tıklamalı biçimli kopyalama

### Fixed

- Excel'in clipboard HTML belgesindeki `<head>` CSS stillerinin korunması
- Unicode içerikte CF_HTML byte offset'lerinin yeniden hesaplanarak Outlook'a
  eksik veya bozuk tablo yapıştırılmasının önlenmesi

## 0.2.4 - 2026-07-28

### Fixed

- Gemini REST `TextResponseFormat.MimeType` alanında metin MIME değeri yerine
  zorunlu `APPLICATION_JSON` enum değerinin gönderilmesi
- Güncel `responseFormat`, modern `responseJsonSchema` ve legacy
  `responseSchema` arasında üç kademeli structured-output uyumluluğu

## 0.2.3 - 2026-07-28

### Fixed

- Gemini Generate Content API'nin güncel `responseFormat` structured-output
  biçimine geçiş
- HTTP 400 uyumsuzluğunda güncel ve legacy istek biçimleri ile desteklenen
  modeller arasında kontrollü otomatik fallback
- `N3_PCS-02_S7/S8` gibi escaped underscore ve slash içeren MEC Tag
  numaralarının bozulmadan çıkarılması
- Kısa MEC Item numaralarının seçilen ITP eşleşmesiyle offline çözülmesi

## 0.2.2 - 2026-07-27

### Fixed

- Yeni kullanıcılara kapatılan `gemini-2.5-flash-lite` varsayılanı yerine
  ücretsiz katmanı bulunan `gemini-3.1-flash-lite` kullanılması
- Seçilen Gemini modeli 404 döndürürse güncel Flash modellerine otomatik geçiş
- Yeni Gemini modellerinde deprecated olan `temperature` parametresinin kaldırılması

## 0.2.1 - 2026-07-27

### Fixed

- Gemini structured-output şemasındaki geçersiz boş discipline enum değeri
- Markdown biçimli Azerice maillerde RFI adedi çıkarımı
- `PCU-14/15/16` gibi kompakt Tag listelerinin ayrı satırlara genişletilmesi
- `2.1` gibi kısa Item numaralarının seçilen ITP eşlemesiyle çözülmesi

## 0.2.0 - 2026-07-27

### Added

- RFI talep e-postalarından offline veya isteğe bağlı Gemini Free ile taslak çıkarma
- Excel'in yerel biçimini koruyan kesin RFI satırı kopyalama
- Uygulamanın sol altında kalıcı sürüm rozeti
- App, sidecar ve build manifesti arasında zorunlu sürüm eşleşme kontrolü
- Küçük ve hızlı Windows UI güncelleme artifact'i

### Fixed

- RFI Log data validation ve calculated-column uyumluluğu
- Process Scans OCR regresyon koruması
- Çoklu dosya yazdırmada kesin sıra ve kullanıcı müdahalesi bekleme

## 0.1.0 - 2026-07-26

- İlk doğrulanmış Windows paketi
- Process Scans, RFI Log ve Batch Print işlemleri
