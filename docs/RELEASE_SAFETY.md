# Release Safety

Bu belge, bir özellik geliştirilirken başka bir özelliğin sessizce bozulmasını
önleyen zorunlu kontrolleri tanımlar.

## Temel kurallar

1. Yalnızca `.github/workflows/windows-release.yml` dağıtılabilir Windows paketi
   üretir. İkinci veya elle farklılaşmış bir paketleme hattı oluşturulmaz.
2. Python, Flutter, Tesseract ve bütün Python paketleri sabit sürümdedir.
   Bağımlılık yükseltmesi ayrı bir değişiklik olarak yapılır ve bütün release
   kapılarından geçer.
3. Tesseract paket içine alınır. Sistem `PATH` değerine veya kullanıcının yerel
   kurulumuna güvenilmez.
4. `preview` hiçbir dosyayı değiştiremez. CI'daki paketli OCR testi, kaynak ve
   hedef durumunu işlemden sonra ayrıca doğrular.
5. `ping` tek başına yeterli değildir. Dağıtım, desteklenen bütün operation
   kimliklerini ve gerçek iş akışını kontrol eder.

## Sürümleme politikası

DocFlow, `MAJOR.MINOR.PATCH+BUILD` biçimini kullanır:

- `MAJOR`: geriye uyumsuz veri, protokol veya kullanıcı akışı değişikliği
- `MINOR`: kullanıcıya görünen yeni feature
- `PATCH`: geriye uyumlu hata düzeltmesi
- `BUILD`: üretilen her yeni Windows paketi için artan tam sayı

Örnek: `0.2.0+2`. Kullanıcı arayüzü `v0.2.0` gösterir; build numarası paket
manifestinde ve `pubspec.yaml` içinde tutulur. `pubspec.yaml`, Flutter sürüm
sabitleri ve sidecar sürümü eşleşmezse release contract testi build'i durdurur.
Yeni feature/fix commit edilmeden önce ilgili SemVer parçası ve build numarası
birlikte artırılır.

## Regresyon matrisi

| Alan | Zorunlu kapı | Koruduğu risk |
|---|---|---|
| Process Scans | OCR birim testleri | OCR config, hata ve fallback davranışları |
| Process Scans | Görsel-only PDF ile gerçek Tesseract testi | Eksik motor/dil verisi, PyMuPDF/OpenCV/pytesseract uyumsuzluğu |
| Process Scans | Paketlenmiş sidecar ile OCR preview | PyInstaller veya paket içeriği farkı |
| RFI Log | RFI ve OOXML testleri | Şifre, formül, tablo, validation, stil ve Excel repair uyarıları |
| Batch Print | Yazıcı/spooler testleri | Sıra ihlali, timeout ve kullanıcı müdahalesi gerektiren işler |
| Operation sözleşmesi | Tam ID kümesi ve JSON serileştirme | Import unutulması veya pakette özelliğin kaybolması |
| Localization | Dört ARB anahtar kümesi ve schema anahtarları | Eksik çeviri yüzünden UI'da ham anahtar görünmesi |
| Paket | Flutter analiz/test + tam paket smoke | UI/sidecar protokolü ve eksik native runtime |

## Bağımlılık güncelleme politikası

Dependabot ayda bir toplu güncelleme önerir; öneri otomatik olarak release
değildir. Güncelleme kabul edilmeden önce:

1. Değişen paketlerin release notları incelenir.
2. `requirements*.txt` yine `==` ile tam sürüme sabit kalır.
3. Sidecar ve Flutter testleri eksiksiz geçer.
4. Gerçek OCR ve paketlenmiş OCR smoke testi geçer.
5. RFI Log için örnek workbook önizlemesi salt-okunur çalıştırılır; testler
   Excel dosyasına onay olmadan apply yapmaz.
6. Batch Print testi gerçek yazıcıya iş göndermeden spooler davranışını mock ile
   doğrular.

## Teşhis ve geri dönüş

Her paket kökünde `BUILD-MANIFEST.json` bulunur. Hata bildiriminde bu dosya ile
uygulama sürümü, commit'i, Python/Flutter/Tesseract ve Python paket sürümleri
kesin olarak belirlenebilir.

Yeni build bir release kapısını geçmezse dağıtılmaz. Çalışan son artifact
korunur; uygulama dosyalarının üzerine kısmi kopyalama yapılmaz. Yerel denemede
de yeni build ayrı klasöre hazırlanır, smoke test geçtikten sonra çalıştırılır.
