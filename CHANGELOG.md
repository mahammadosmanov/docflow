# Changelog

Bu proje [Semantic Versioning](https://semver.org/) kullanır. Kullanıcıya görünen
her feature veya fix, yeni Windows paketi üretilmeden önce burada kaydedilir.

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
