# Changelog

Bu proje [Semantic Versioning](https://semver.org/) kullanır. Kullanıcıya görünen
her feature veya fix, yeni Windows paketi üretilmeden önce burada kaydedilir.

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
