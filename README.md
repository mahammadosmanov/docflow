# DocFlow

DocFlow, Windows üzerinde belge işlerini otomatikleştiren Flutter masaüstü
uygulamasıdır. İş mantığı, uygulamayla birlikte paketlenen Python sidecar
tarafından yürütülür.

Güncel sürüm: **0.3.0**. Sürüm geçmişi ve değişiklikler
[`CHANGELOG.md`](CHANGELOG.md) içinde tutulur.

Desteklenen işlemler:

- Taranmış RFI PDF'lerini OCR ile tanıma, adlandırma ve klasörleme
- CIV/MEC kayıtlarını formül, stil ve doğrulamalarını koruyarak RFI Log'a ekleme
- Talep e-postalarından özel/offline veya isteğe bağlı Gemini Free ile doğrulanabilir RFI taslağı çıkarma
- Yeni RFI satırlarını Excel'in yerel görünümüyle, başlıklar dahil Subcontractor'a kadar kopyalama
- PDF, Word, Excel ve görselleri kesin sırayla toplu yazdırma

E-posta analizi varsayılan olarak tamamen çevrimdışıdır. Gemini seçeneği açıkça
seçilirse API anahtarı yalnızca o istek için bellekte tutulur; e-posta metni ve
anahtar ayarlara ya da çalışma kitabına kaydedilmez. Çıkarılan değerler çalışma
kitabındaki ITP/Item eşlemeleriyle doğrulanır ve uygulanmadan önce düzenlenebilir.

## Geliştirme

Windows'ta Python 3.12.10, Flutter 3.44.8 ve Tesseract
5.5.0.20241111 kullanılır. Python bağımlılıkları tam sürüme, Flutter
bağımlılıkları `pubspec.lock` dosyasına sabitlenmiştir.

```powershell
cd sidecar
py -3.12 -m venv .venv
.\.venv\Scripts\python.exe -m pip install -r requirements-dev.txt
.\.venv\Scripts\python.exe -m mypy --config-file mypy.ini docflow_sidecar
.\.venv\Scripts\python.exe -m pytest tests -q
.\.venv\Scripts\python.exe -m PyInstaller --noconfirm --clean docflow_sidecar.spec

cd ..\app
flutter pub get
dart analyze
flutter test
flutter build windows --release
```

Gerçek OCR entegrasyon testini çalıştırmak için Tesseract kurulu olmalıdır:

```powershell
$env:DOCFLOW_RUN_OCR_INTEGRATION = "1"
.\sidecar\.venv\Scripts\python.exe -m pytest `
  sidecar\tests\test_ocr_integration.py -q
```

## Windows paketi

Tek yetkili paketleme hattı `.github/workflows/windows-release.yml` dosyasıdır.
Üretilen `DocFlow-Windows-x64` paketi Flutter uygulamasını, sidecar'ı, Visual
C++ çalışma zamanı dosyalarını, sabit Tesseract sürümünü ve İngilizce OCR dil
verisini içerir. Kullanıcı bilgisayarında ayrı Python veya Tesseract kurulumu
gerekmez.

Paket yayınlanmadan önce:

- Python tip kontrolü ve bütün sidecar testleri
- gerçek, metin katmanı olmayan PDF üzerinde OCR
- Flutter analiz ve testleri
- paketlenmiş sidecar içinde bütün operation'ların varlığı
- paketlenmiş sidecar + gömülü Tesseract ile salt-okunur OCR önizlemesi
- bağımlılık ve araç sürümlerini içeren `BUILD-MANIFEST.json`

kontrol edilir. Ayrıntılı değişiklik ve release kuralları
[`docs/RELEASE_SAFETY.md`](docs/RELEASE_SAFETY.md) dosyasındadır.
