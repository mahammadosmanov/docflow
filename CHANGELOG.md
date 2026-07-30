# Changelog

Bu proje [Semantic Versioning](https://semver.org/) kullanır. Kullanıcıya görünen
her feature veya fix, yeni Windows paketi üretilmeden önce burada kaydedilir.

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
