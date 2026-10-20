// ignore: unused_import
import 'package:intl/intl.dart' as intl;

import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for Turkish (`tr`).
class AppLocalizationsTr extends AppLocalizations {
  AppLocalizationsTr([String locale = 'tr']) : super(locale);

  @override
  String get appTitle => 'DocFlow';

  @override
  String get actionPreview => 'Önizle';

  @override
  String get actionApply => 'Uygula';

  @override
  String get actionUndo => 'Geri al';

  @override
  String get actionCancel => 'İptal';

  @override
  String get actionBack => 'Geri';

  @override
  String get actionBrowse => 'Gözat';

  @override
  String get uiWarnings => 'Uyarılar';

  @override
  String resultApplied(int count) {
    return '$count uygulandı';
  }

  @override
  String resultFailed(int count) {
    return '$count başarısız';
  }

  @override
  String get actionOpen => 'Aç';

  @override
  String get actionReview => 'İnceleme klasörü';

  @override
  String get reviewTitle => 'Dosya İnceleme';

  @override
  String get reviewEmpty => 'İnceleme klasörü boş';

  @override
  String get reviewKeep => 'Olduğu gibi bırak';

  @override
  String get reviewPut => 'Hedef klasöre taşı';

  @override
  String get reviewDelete => 'Çöp kutusuna taşı';

  @override
  String get reviewNoCode => 'RFI kodu yok';

  @override
  String get reviewWillReplace => 'mevcut dosyayı değiştirir';

  @override
  String get noteDeleted => 'Silindi (çöpe)';

  @override
  String get noteDisplaced => 'Değiştirildi — çöpe taşındı';

  @override
  String get actionHistory => 'Geçmiş';

  @override
  String get historyTitle => 'Geçmiş';

  @override
  String get historyUnknownOperation => 'Bilinmeyen işlem';

  @override
  String get historyEmpty => 'Henüz geçmiş yok';

  @override
  String historyMoves(int count) {
    return '$count dosya';
  }

  @override
  String get historyUndoOriginal => 'Orijinal konuma geri al';

  @override
  String get historyUndoToFolder => 'Bir klasöre geri al…';

  @override
  String get planEmpty => 'Yapılacak bir şey yok';

  @override
  String planSummary(int count) {
    return '$count dosya işlenecek';
  }

  @override
  String get planImpactTitle => 'Uygulamadan önce';

  @override
  String get planImpactPreviewOnly =>
      'Henüz hiçbir şey değişmedi. Uygula yalnız bu onaylı önizlemeyi yürütür.';

  @override
  String get planImpactOutputs => 'Yeni çıktılar';

  @override
  String get planImpactUpdates => 'Güncellenecek dosyalar';

  @override
  String get planImpactMoves => 'Taşıma / ad değiştirme';

  @override
  String get planImpactPrintJobs => 'Yazdırma işleri';

  @override
  String get planImpactDrafts => 'Açılacak taslaklar';

  @override
  String get planImpactCleanup => 'Temizlik';

  @override
  String get planImpactWarnings => 'Uyarılar';

  @override
  String get scanIndexTitle => 'Artımlı tarama indeksi';

  @override
  String get scanIndexHelp =>
      'Değişmeyen dosyalar kalıcı yerel indeksten yeniden kullanıldı.';

  @override
  String get scanIndexForceApplied =>
      'Bu işlem için tam tarama özellikle istendi.';

  @override
  String get scanIndexDiscovered => 'Bulunan';

  @override
  String get scanIndexReused => 'İndeksten';

  @override
  String get scanIndexAnalyzed => 'Şimdi taranan';

  @override
  String get scanIndexFailed => 'Başarısız';

  @override
  String get scanIndexInterrupted => 'Önceki yarım kalanlar';

  @override
  String get planWarningsSearch => 'Uyarılarda ara';

  @override
  String get planWarningsType => 'Uyarı türü';

  @override
  String get planWarningsAll => 'Tüm uyarı türleri';

  @override
  String get planWarningsFocus => 'İlgili sayfayı göster';

  @override
  String get planWarningsNone => 'Bu filtrelerle eşleşen uyarı yok.';

  @override
  String planWarningsShowing(Object count, Object shown) {
    return '$count uyarının $shown tanesi gösteriliyor';
  }

  @override
  String get planWorkbookChangesTitle => 'Kesin Excel hücre değişiklikleri';

  @override
  String planWorkbookChangesCount(Object count) {
    return '$count onaylı hücre değişikliği';
  }

  @override
  String get planWorkbookChangesHelp =>
      'Mevcut değer ile Uygula\'nın yazacağı kesin değeri karşılaştırmak için açın.';

  @override
  String get planWorkbookChangesWorkbook => 'Çalışma kitabı';

  @override
  String get planWorkbookChangesSheetCell => 'Sayfa ve hücre';

  @override
  String get planWorkbookChangesRfi => 'RFI';

  @override
  String get planWorkbookChangesBefore => 'Mevcut';

  @override
  String get planWorkbookChangesAfter => 'Uygulama sonrası';

  @override
  String get planWorkbookChangesBlank => '(boş)';

  @override
  String get planWorkbookChangesKindRfiLog => 'RFI Log';

  @override
  String get planWorkbookChangesKindFollowUp => 'Follow Up';

  @override
  String get planWorkbookChangesKindWorkbook => 'Çalışma kitabı';

  @override
  String planWorkbookChangesLimit(Object count) {
    return 'Önizleme ilk $count onaylı hücre değişikliğiyle sınırlıdır.';
  }

  @override
  String planExactTitle(Object count) {
    return '$count onaylı kesin etki';
  }

  @override
  String get planExactHelp => 'Tam kaynak ve hedef yollarını incelemek için aç';

  @override
  String get opRfiProcessScansTitle => 'Taramaları İşle';

  @override
  String get opRfiProcessScansFastCoversTitle => 'Hızlı RFI Taraması';

  @override
  String get opRfiProcessScansFastCoversDesc =>
      'Çok sayfalı taramalarda RFI kapaklarını bulur. Birleştirilmiş taramalar için Çoklu RFI, yalnızca ilk sayfayı okuyup PDF\'nin tamamını korumak için Tek RFI modunu seçin. OCR\'da okunan CLV, CIV olarak düzeltilir. Belirsiz taramalar _review klasörüne gider.';

  @override
  String get paramRfiProcessScansScanMode => 'Tarama modu';

  @override
  String get paramRfiProcessScansScanModeHelp =>
      'Çoklu RFI, birleştirilmiş taramayı ayırır. Tek RFI yalnızca ilk sayfayı okur ve PDF\'nin tamamını olduğu gibi korur.';

  @override
  String get choiceRfiProcessScansScanModeMultiRfi =>
      'Birden fazla RFI\'yi ayır';

  @override
  String get choiceRfiProcessScansScanModeSingleRfi =>
      'Tek RFI — yalnızca ilk sayfayı oku';

  @override
  String get rfiFastScanEditorTitle => 'RFI numaralarını incele ve düzelt';

  @override
  String rfiFastScanEditorRowCount(int count) {
    return '$count tarama dosyası';
  }

  @override
  String get rfiFastScanColumnSourceFile => 'Kaynak dosya';

  @override
  String get rfiFastScanColumnEvidence => 'Tanıma';

  @override
  String get rfiFastScanColumnDetectedCode => 'Algılanan RFI';

  @override
  String get rfiFastScanColumnCode => 'RFI numarası';

  @override
  String get rfiFastScanColumnTarget => 'Hedef';

  @override
  String get rfiFastScanEvidenceClvToCiv => 'OCR eşlemesi: CLV → CIV';

  @override
  String get rfiFastScanEvidenceConfirmed => 'Doğrulandı';

  @override
  String get rfiFastScanEditorUpdatePreview => 'Önizlemeyi güncelle';

  @override
  String get rfiFastScanEditorUpdatingPreview => 'Önizleme güncelleniyor…';

  @override
  String get rfiFastScanEditorPreviewUpdated =>
      'Önizleme güncellendi. Uygulamadan önce hedef yollarını kontrol edin.';

  @override
  String get errorRfiFastScanCodeInvalid =>
      'Desteklenen tam RFI kodunu 1–4 ASCII rakamlı bir sonekle girin; kod dört rakamla kaydedilir.';

  @override
  String get errorRfiFastScanTargetExists =>
      'Başka bir tarama aynı RFI numarasını kullanıyor veya hedef zaten mevcut.';

  @override
  String get errorRfiFastScanRevisionInvalid =>
      'Düzenlenen tarama önizlemesi geçersiz. RFI numaralarını kontrol edip tekrar deneyin.';

  @override
  String get opRfiGroupFilesTitle => 'RFI Dosyalarını Grupla';

  @override
  String get opRfiGroupFilesDesc =>
      'Dosya adı ve içerikteki kesin RFI numaralarını bulur; onaylanan dosyaları <disiplin>/<tam RFI kodu> klasörlerine kopyalar.';

  @override
  String get paramRfiFileGroupSource => 'Gelen dosyalar klasörü';

  @override
  String get paramRfiFileGroupSourceHelp =>
      'Yalnız üst düzey dosyalar taranır; alt klasörler değişmeden atlanır.';

  @override
  String get paramRfiFileGroupDestination => 'Gruplanmış çıktı klasörü';

  @override
  String get paramRfiFileGroupDestinationHelp =>
      'Kaynak dışında boş veya ayrı bir hedef seçin. Mevcut dosyaların üzerine yazılmaz.';

  @override
  String get rfiFileGroupEditorTitle => 'Bulunan RFI eşleşmeleri';

  @override
  String rfiFileGroupEditorRowCount(Object count) {
    return '$count kopyalama eşleşmesi';
  }

  @override
  String get rfiFileGroupColumnEnabled => 'Kopyala';

  @override
  String get rfiFileGroupColumnRfiCode => 'RFI numarası';

  @override
  String get rfiFileGroupColumnSourceFile => 'Kaynak dosya';

  @override
  String get rfiFileGroupColumnMethod => 'Bulma yöntemi';

  @override
  String get rfiFileGroupMethodFilename => 'Dosya adı';

  @override
  String get rfiFileGroupMethodPdf => 'PDF / OCR';

  @override
  String get rfiFileGroupMethodMail => 'Mail içeriği';

  @override
  String get rfiFileGroupMethodOffice => 'Office içeriği';

  @override
  String get rfiFileGroupMethodContent => 'Dosya içeriği';

  @override
  String get noteRfiFileGroupFolderCreated => 'RFI klasörü oluşturulacak';

  @override
  String get noteRfiFileGroupCopy =>
      'Dosya, disiplin ve tam RFI kodu klasörüne kopyalanacak';

  @override
  String get noteRfiFileGroupReviewCopy =>
      'Dosya inceleme klasörüne kopyalanacak';

  @override
  String get warnRfiFileGroupNoCode =>
      'RFI numarası bulunamadı; dosya incelemeye ayrılacak';

  @override
  String get warnRfiFileGroupUnreadable =>
      'Dosya tam okunamadı; incelemeye ayrılacak';

  @override
  String get warnRfiFileGroupPartialInspection =>
      'Dosya adındaki eşleşme kullanılabilir ancak içerik tam incelenemedi';

  @override
  String get warnRfiFileGroupExists =>
      'Hedef dosya zaten var; üzerine yazılmayacak';

  @override
  String get warnRfiFileGroupNoFiles =>
      'Kaynak klasörde üst düzey dosya bulunamadı';

  @override
  String get warnRfiFileGroupMissingEmail =>
      'Bu RFI için EML veya MSG e-posta dosyası bulunamadı; ilgili dosyalar yerleştirilmeyecek';

  @override
  String get errorParamRequiredDestination => 'Hedef klasör gerekli';

  @override
  String get paramSource => 'Kaynak klasör';

  @override
  String get paramSourceHelp => 'Taranmış PDF\'lerin bulunduğu klasör';

  @override
  String get paramDest => 'Hedef klasör';

  @override
  String get paramDestHelp => 'İşlenen RFI PDF\'lerinin kaydedileceği yer';

  @override
  String get paramKeepOriginal => 'Orijinalleri koru (taşımak yerine kopyala)';

  @override
  String get noteRenamed => 'Adlandırıldı';

  @override
  String get noteNeedsReview => 'İncelemeye taşınacak';

  @override
  String get noteUnreadable => 'Okunamadı — incelemeye taşınacak';

  @override
  String get warnNoCode => 'RFI numarası bulunamadı';

  @override
  String get warnLowConfidence => 'Düşük güven — kontrol gerekli';

  @override
  String get warnUnknownRfiFolder =>
      'Algılanan RFI klasörü mevcut değil — incelemeye taşındı';

  @override
  String get warnUnreadable => 'Dosya okunamadı';

  @override
  String get warnRevisionGuessed => 'Revizyon numarası tahmin edildi';

  @override
  String get warnDuplicate => 'Yinelenen RFI numarası — incelemeye taşındı';

  @override
  String get warnExists =>
      'Hedef zaten var — üzerine yazılmadı, incelemeye taşındı';

  @override
  String get warnNoPdfs => 'Klasörde PDF yok';

  @override
  String get errorStepFailed => 'Adım başarısız';

  @override
  String get errorUndoFailed => 'Geri alma başarısız';

  @override
  String get errorParamRequiredSource => 'Kaynak klasör zorunlu';

  @override
  String get errorParamRequiredDest => 'Hedef klasör zorunlu';

  @override
  String get actionAdmin => 'Yönetim paneli';

  @override
  String get actionRetry => 'Tekrar denetle';

  @override
  String get bootstrapFatalTitle => 'DocFlow başlatılamadı';

  @override
  String get bootstrapRetry => 'Tekrar dene';

  @override
  String get authEmail => 'E-posta';

  @override
  String get authPassword => 'Parola';

  @override
  String get authSignIn => 'Giriş yap';

  @override
  String get authSignUp => 'Hesap oluştur';

  @override
  String get authSignOut => 'Çıkış yap';

  @override
  String get authSignupOk =>
      'Hesap oluşturuldu. Yöneticiden onaylamasını isteyin, sonra giriş yapın.';

  @override
  String get authPending => 'Onay bekleniyor';

  @override
  String get authPendingHelp =>
      'Hesabınız var ama henüz onaylanmadı. Yöneticinin erişim vermesi gerekiyor.';

  @override
  String get adminTitle => 'Kullanıcılar';

  @override
  String get adminAllowed => 'Erişim';

  @override
  String get adminAdmin => 'Yönetici';

  @override
  String get adminEmpty => 'Henüz kullanıcı yok';

  @override
  String get actionDone => 'Bitti';

  @override
  String get adminHelp =>
      'Birinin uygulamayı kullanabilmesi için Erişim\'i aç. Kendi anahtarların kilitli — kendini dışarıda bırakamazsın.';

  @override
  String get adminNoAccess => 'erişim yok';

  @override
  String get adminYou => 'sen';

  @override
  String get authSubtitle => 'Devam etmek için giriş yap';

  @override
  String get authShow => 'Parolayı göster';

  @override
  String get authHide => 'Parolayı gizle';

  @override
  String get authPasswordHint => 'En az 6 karakter';

  @override
  String get historyEmptyHelp =>
      'Uyguladığın işlemler burada listelenir ve geri alabilirsin.';

  @override
  String get historyUndone => 'Geri alındı';

  @override
  String get historyLocalJournalRequired =>
      'Geri almak için bu cihazdaki yerel günlük gerekli.';

  @override
  String get historyCloud => 'Bulut';

  @override
  String get historyLocal => 'Yerel';

  @override
  String get historySourceCloud =>
      'Paylaşılan geçmiş, tüm makinelerinde görünür';

  @override
  String get historySourceLocal => 'Sadece yerel geçmiş — buluta ulaşılamıyor';

  @override
  String get opsHeading => 'İşlemler';

  @override
  String get opsSubheading =>
      'Bir işlem seçin. Siz inceleyip onaylamadan hiçbir dosyanız değiştirilmez.';

  @override
  String get homeTitle => 'Çalışma alanı';

  @override
  String get homeSubtitle =>
      'Belge işlemleri ve tüm yardımcı araçlar tek yerde.';

  @override
  String get homePreviewFirst => 'Önizleme öncelikli';

  @override
  String get ocrConfidence => 'OCR kanıt skoru';

  @override
  String get ocrConfidenceShort => 'Kanıt';

  @override
  String get ocrConfidenceHelp =>
      '100 üzerinden sezgisel OCR tutarlılık skoru. Doğruluk olasılığı, kesin onay veya otomatik onay değildir.';

  @override
  String get homeFeatures => 'Diğer özellikler';

  @override
  String get homeFeaturesHelp =>
      'İnceleme, geçmiş, hesap ve uygulama yönetimi.';

  @override
  String get homeReviewDesc =>
      'Bekleyen dosyaları tek tek kontrol edin ve güvenle yerleştirin.';

  @override
  String get homeHistoryDesc => 'Yapılan işlemleri görün ve güvenle geri alın.';

  @override
  String get homeSettingsDesc =>
      'Dil, görünüm, yazı ve hesap tercihlerinizi yönetin.';

  @override
  String get homeAdminDesc =>
      'Kullanıcı erişimlerini ve yönetici yetkilerini yönetin.';

  @override
  String get paramHintFolder => 'Bir klasör seç';

  @override
  String get paramHintFile => 'Bir dosya seç';

  @override
  String get planApplying => 'Uygulanıyor';

  @override
  String get planReadOnly => 'Henüz hiçbir şey değişmedi';

  @override
  String get prefsTitle => 'Ayarlar';

  @override
  String get prefsLanguage => 'Dil';

  @override
  String get prefsLangSystem => 'Sistem dili';

  @override
  String get prefsLangTr => 'Türkçe';

  @override
  String get prefsLangEn => 'English';

  @override
  String get prefsLangRu => 'Русский';

  @override
  String get prefsLangAz => 'Azərbaycanca';

  @override
  String get prefsTheme => 'Tema';

  @override
  String get prefsThemeSystem => 'Sistem';

  @override
  String get prefsThemeLight => 'Açık';

  @override
  String get prefsThemeDark => 'Koyu';

  @override
  String get prefsFont => 'Yazı tipi';

  @override
  String get prefsFontHelp =>
      'Bu bilgisayarda zaten kurulu yazı tiplerini kullanır — hiçbir şey indirilmez.';

  @override
  String get prefsFontSystem => 'Sistem varsayılanı';

  @override
  String get prefsFontSans => 'Sans serif';

  @override
  String get prefsFontSerif => 'Serif';

  @override
  String get prefsFontMono => 'Eş aralıklı';

  @override
  String get prefsFontSize => 'Yazı boyutu';

  @override
  String get prefsFontSizeSample =>
      'AZE-SUN-INT-QMS-RFI-CIV-0064 — bu boyutta örnek metin.';

  @override
  String get prefsDensity => 'Aralık';

  @override
  String get prefsDensityCompact => 'Sıkışık yerleşim';

  @override
  String get prefsDensityHelp =>
      'Ekrana daha çok şey sığdırır. Küçük ekranlarda işe yarar.';

  @override
  String get prefsSupportTitle => 'Gelişmiş destek';

  @override
  String get prefsSupportHelp =>
      'Salt okunur ve redakte edilmiş bir yerel destek paketi oluşturun.';

  @override
  String get prefsSupportPrivacy =>
      'Sürüm, platform, sidecar/OCR durumu, son hata kodları ve yol içermeyen son printer kuyruk sonucunu içerir; belge içeriği, kimlik bilgileri, tokenlar ve özel yollar dahil edilmez. Hiçbir yere gönderilmez.';

  @override
  String get prefsSupportPreview => 'Destek paketini önizle';

  @override
  String get prefsSupportPreviewLabel => 'Destek paketi önizlemesi';

  @override
  String prefsSupportSize(Object count) {
    return '$count bayt';
  }

  @override
  String get prefsSupportCopy => 'Paketi panoya kopyala';

  @override
  String get prefsSupportCopied => 'Destek paketi panoya kopyalandı.';

  @override
  String get prefsSupportDownload => 'Destek paketini indir';

  @override
  String get prefsSupportDownloaded => 'Destek paketi indirildi.';

  @override
  String get paramDropFiles =>
      'Dosyaları buraya sürükleyin veya eklemek için tıklayın';

  @override
  String get paramDropFolder => 'Klasörü buraya sürükleyin veya göz atın';

  @override
  String get paramDropFile => 'Dosyayı buraya sürükleyin veya göz atın';

  @override
  String get actionRecent => 'Son seçimler';

  @override
  String get actionToggleTheme => 'Temayı değiştir (Açık / Koyu / Sistem)';

  @override
  String get filterSearchPlaceholder => 'Ara (RFI no, durum, açıklama)...';

  @override
  String get filterAll => 'Tümü';

  @override
  String get filterChanges => 'Değişiklikler';

  @override
  String get filterManualReview => 'Manuel İnceleme';

  @override
  String tableFilterCount(Object filtered, Object total) {
    return '$filtered / $total satır gösteriliyor';
  }

  @override
  String get prefsReset => 'Varsayılanlara dön';

  @override
  String get reviewHelp =>
      'Her dosyaya ne olacağına karar ver. Sen planı onaylamadan hiçbir şey değişmez.';

  @override
  String get reviewEmptyHelp => 'İnceleme bekleyen dosya yok.';

  @override
  String get reviewNoChanges => 'Hiçbir şey seçilmedi';

  @override
  String reviewSelected(Object count) {
    return '$count seçildi';
  }

  @override
  String reviewToTrash(Object count) {
    return '$count çöpe';
  }

  @override
  String get reviewTrashHint =>
      'Silinen dosyalar geri alınabilir çöpe gider, kalıcı olarak silinmez.';

  @override
  String get stepForm => 'Yapılandır';

  @override
  String get stepPreview => 'Önizle';

  @override
  String get stepApply => 'Uygula';

  @override
  String get actionSave => 'Kaydet';

  @override
  String get authUsername => 'Kullanıcı adı';

  @override
  String get authUsernameHelp =>
      '3-24 karakter: harf, rakam, nokta, tire, alt çizgi';

  @override
  String get authIdentifier => 'Kullanıcı adı veya e-posta';

  @override
  String get authIdentifierHelp => 'İkisi de olur';

  @override
  String get authEmailSignInHelp =>
      'Kullanıcı adıyla giriş kullanılamıyor. Hesabının e-posta adresini kullan.';

  @override
  String get authSubtitleUp => 'Başlamak için hesap oluştur';

  @override
  String get authErrCredentials => 'Kullanıcı adı/e-posta veya parola yanlış.';

  @override
  String get authErrUsernameTaken => 'Bu kullanıcı adı alınmış.';

  @override
  String get authErrUsernameInvalid =>
      'Kullanıcı adı 3-24 karakter olmalı: harf, rakam, nokta, tire, alt çizgi.';

  @override
  String get authErrEmailTaken => 'Bu e-posta ile bir hesap zaten var.';

  @override
  String get prefsAccount => 'Hesap';

  @override
  String get prefsAccountEmailFixed => 'değiştirilemez';

  @override
  String get prefsAccountSaved => 'Kaydedildi';

  @override
  String get authErrEmailNotConfirmed =>
      'Bu hesap var ama e-posta henüz doğrulanmamış. Yönetici e-posta doğrulamasını kapatabilir ya da gelen kutuna gönderilen bağlantıyı aç.';

  @override
  String get authErrRateLimit =>
      'Çok fazla deneme. Birkaç dakika bekleyip tekrar dene.';

  @override
  String get authErrWeakPassword =>
      'Parola çok zayıf. En az 6 karakter kullan.';

  @override
  String get warnNoOcr =>
      'OCR motoru (Tesseract) bulunamadı — hiçbir dosyaya dokunulmadı. Kur ya da uygulama paketini yeniden yükle.';

  @override
  String get noteFolderCreated => 'Yeni klasör';

  @override
  String get warnOcrBroken =>
      'OCR her dosyada başarısız oldu — bu bir kurulum sorunu, dosya sorunu değil. Hiçbir şeye dokunulmadı. Ayrıntı:';

  @override
  String get opFilesBatchPrintTitle => 'Birden fazla dosya yazdır';

  @override
  String get opFilesBatchPrintDesc =>
      'PDF, Office belgeleri ve desteklenen görsellerden sıralı tek bir yerel yazdırma kuyruğu oluşturur.';

  @override
  String get paramPrintFiles => 'Yazdırılacak dosyalar';

  @override
  String get paramPrintFilesHelp =>
      'Farklı dosya türleri ekleyin; ardından sıralayın veya kaldırın. Ekrandaki sıra yazdırma sırasıdır.';

  @override
  String get paramPrintRfiRoot => 'RFI Sistemi klasörü';

  @override
  String get paramPrintRfiRootHelp =>
      'Bu klasörü ve alt klasörlerini kaynak dosyalara dokunmadan arar.';

  @override
  String get paramPrintRfiNumbers => 'RFI numaraları';

  @override
  String get paramPrintRfiNumbersHelp =>
      'Her satıra tam RFI kodu, CIV-1319 gibi disiplin-numara kodu veya 1–4 basamaklı bir sayı girin; kısa numaralar dört basamağa tamamlanır.';

  @override
  String get paramPrintIncludeFirstPage => 'İlk Sayfa belgeleri';

  @override
  String get paramPrintIncludeSketch => 'Sketch belgeleri';

  @override
  String get printRfiTitle => 'RFI belgelerini bul';

  @override
  String get printRfiInputs => 'Bu RFI isteklerini dahil et';

  @override
  String get printRfiRootRequired => 'Önce bir RFI Sistemi klasörü seçin.';

  @override
  String get printRfiInputRequired =>
      'Aramadan önce en az bir RFI isteği seçin.';

  @override
  String get printRfiTypesRequired =>
      'Aramadan önce İlk Sayfa, Sketch veya ikisini seçin.';

  @override
  String get printRfiSearch => 'RFI Sisteminde ara';

  @override
  String get printRfiSearching => 'Aranıyor…';

  @override
  String printRfiFound(Object count) {
    return '$count eşleşen belge';
  }

  @override
  String get printRfiAmbiguous =>
      'Bu numara birden fazla disiplinde var. İstenen dosyaları açıkça seçin.';

  @override
  String get printRfiMissingFirstPage => 'İlk Sayfa belgesi bulunamadı.';

  @override
  String get printRfiMissingSketch => 'Sketch belgesi bulunamadı.';

  @override
  String get printRfiToggleRequest => 'Bu RFI\'yi değiştir';

  @override
  String get printRfiSelectTypeFirstPage => 'İlk Sayfaları seç';

  @override
  String get printRfiSelectTypeSketch => 'Sketch\'leri seç';

  @override
  String get printRfiClearTypeFirstPage => 'İlk Sayfaları temizle';

  @override
  String printRfiSelectDiscipline(Object discipline) {
    return '$discipline disiplinini seç';
  }

  @override
  String get printRfiClearTypeSketch => 'Sketch\'leri temizle';

  @override
  String get printRfiTypeFirstPage => 'İlk Sayfa';

  @override
  String get printRfiTypeSketch => 'Sketch';

  @override
  String get printRfiNoMatches => 'Eşleşen belge bulunamadı.';

  @override
  String get printRfiTruncated =>
      'Arama güvenli tarama sınırına ulaştı. Bazı klasörler aranmamış olabilir.';

  @override
  String get actionSelectAll => 'Tümünü seç';

  @override
  String get actionSelectNone => 'Hiçbirini seçme';

  @override
  String get errorPrintRfiInvalidRequest =>
      'RFI isteklerini ve seçilen belge türlerini kontrol edin.';

  @override
  String get errorPrintRfiRootEscape =>
      'Bulunan dosya seçilen RFI Sistemi dışında değişti. Yeniden arayın.';

  @override
  String get errorPrintRfiChanged =>
      'Bulunan dosya arama sırasında değişti. Yeniden arayın.';

  @override
  String get paramPrintPrinter => 'Yazıcı';

  @override
  String get paramPrintPrinterHelp =>
      'Bu bilgisayarda kurulu olan yazıcıları kullanır.';

  @override
  String get paramPrintCopies => 'Kopya sayısı';

  @override
  String get paramPrintCopiesHelp => 'Kuyruğun 1 ile 99 arasında kopyası.';

  @override
  String get paramPrintCollate => 'Kopyaları harmanla';

  @override
  String get paramPrintCollateHelp =>
      'Sonraki kopyaya başlamadan önce sıralı dosya setinin tamamını yazdırır.';

  @override
  String get errorParamRequiredFiles => 'En az bir dosya seçin';

  @override
  String get errorParamRequiredPrinter => 'Bir yazıcı seçin';

  @override
  String get errorParamRequiredCopies => 'Kopya sayısı zorunludur';

  @override
  String get actionAddFiles => 'Dosya ekle';

  @override
  String get actionClear => 'Temizle';

  @override
  String get actionMoveUp => 'Yukarı taşı';

  @override
  String get actionMoveDown => 'Aşağı taşı';

  @override
  String get actionRemove => 'Kaldır';

  @override
  String get actionDecrease => 'Azalt';

  @override
  String get actionIncrease => 'Artır';

  @override
  String get paramHintFiles => 'Dosya seçilmedi';

  @override
  String printFilesSelected(Object count) {
    return '$count dosya seçildi';
  }

  @override
  String get notePrintQueued => 'Yazdırmaya hazır';

  @override
  String get warnPrintNoPrinters => 'Yazıcı bulunamadı';

  @override
  String get warnPrintPrinterUnavailable =>
      'Seçilen yazıcı artık kullanılamıyor';

  @override
  String get warnPrintInvalidCopies =>
      'Kopya sayısı 1 ile 99 arasında olmalıdır';

  @override
  String get warnPrintTooManyFiles =>
      'Bir kuyruk en fazla 250 dosya içerebilir';

  @override
  String get warnPrintFileMissing => 'Dosya bulunamadı ve atlanacak';

  @override
  String get warnPrintWorkbookDiscoveryFailed =>
      'Çalışma kitabı güvenle incelenemedi ve yazdırma sırasına eklenmedi';

  @override
  String get warnPrintWorkbookTooManyWorksheets =>
      'Çalışma kitabı çok fazla çalışma sayfası içeriyor';

  @override
  String get warnPrintExpandedOrderTooLarge =>
      'Genişletilmiş yazdırma sırası en fazla 250 iş içerebilir';

  @override
  String get warnPrintWorkbookUnsafeXlm =>
      'Çalışma kitabı Excel 4.0 makro sayfası içerdiği için reddedildi';

  @override
  String get warnPrintWorkbookUnsafeProtection =>
      'Çalışma kitabı yapısı korumalı olduğu için reddedildi';

  @override
  String get warnPrintWorkbookUnsafeExternalLinks =>
      'Çalışma kitabı dış bağlantılar içerdiği için reddedildi';

  @override
  String get warnPrintWorkbookUnsafeConnections =>
      'Çalışma kitabı dış veri bağlantıları içerdiği için reddedildi';

  @override
  String get warnPrintWorkbookUnsafeQueryTables =>
      'Çalışma kitabı sorgu tabloları içerdiği için reddedildi';

  @override
  String get warnPrintNoHandler =>
      'Bu dosya türü için yerel bir yazdırma uygulaması bulunamadı; yazdırabilen bir varsayılan uygulama kurun veya seçin';

  @override
  String get warnPrintUnsupportedOptions =>
      'Seçilen yazıcı veya yerel uygulama bu yazdırma seçeneklerini desteklemiyor';

  @override
  String get warnPrintNonUndoable =>
      'Yazdırma geri alınamaz. Dosyalar kesin olarak tek tek çalışır; bir uygulama doğrulama isterse sonraki dosya başlatılmadan önce kuyruk bekler.';

  @override
  String get errorPrintInvalidPlan =>
      'Yazdırma planı geçersiz. Kuyruğu yeniden önizleyin.';

  @override
  String get errorPrintFailed => 'Yazdırma gönderimi başarısız';

  @override
  String get errorPrintQueueStopped =>
      'Kesin yazdırma sırası korundu; kalan dosyalar gönderilmedi';

  @override
  String get errorPrintPageRangeInvalid =>
      'all ya da 1-3,5 gibi virgülle ayrılmış sayfa ve aralıklar girin.';

  @override
  String get errorPrintPageRangeBounds =>
      'Seçilen sayfa aralığı bu PDF\'in sayfa sayısını aşıyor.';

  @override
  String get errorPrintPageRangeImageUnsupported =>
      'Görseller yalnızca all veya 1. sayfayı kabul eder.';

  @override
  String get errorPrintPageRangeNativeUnsupported =>
      'Yerel bir uygulamayla yazdırılan dosyalar yalnızca tüm sayfaları kabul eder.';

  @override
  String get errorPrintWorksheetPageRangeAllOnly =>
      'Seçili çalışma sayfaları yalnızca all değerini kabul eder.';

  @override
  String get errorPrintWorkbookChanged =>
      'Çalışma kitabı önizlemeden sonra değişti. Yazdırma sırasını yeniden önizleyin.';

  @override
  String get errorPrintWorksheetChanged =>
      'Çalışma sayfası yapısı önizlemeden sonra değişti. Yazdırma sırasını yeniden önizleyin.';

  @override
  String get errorPrintExcelUnavailable =>
      'Excel çalışma sayfası dışa aktarımı bu bilgisayarda kullanılamıyor.';

  @override
  String get errorPrintExcelExportTimedOut =>
      'Excel çalışma sayfası dışa aktarımı yazdırma başlamadan zaman aşımına uğradı.';

  @override
  String get errorPrintExcelExportCancelled =>
      'Excel çalışma sayfası dışa aktarımı yazdırma başlamadan iptal edildi.';

  @override
  String get errorPrintExcelExportQuarantined =>
      'Önceki Excel çalışanı güvenle kapatılamadığı için DocFlow yeniden başlatılana kadar Excel dışa aktarımı devre dışıdır.';

  @override
  String get errorPrintExportCleanupRequiresReview =>
      'Özel Excel dışa aktarım temizliği doğrulanamadı. Yeniden denemeden önce DocFlow\'u yeniden başlatıp geçici dışa aktarım durumunu inceleyin.';

  @override
  String get errorPrintExcelSourceMotwBlocked =>
      'Çalışma kitabı, Windows indirme bölgesi verileri güvensiz, bozuk, aşırı büyük veya okunamaz olduğu için engellendi.';

  @override
  String get errorPrintExcelWorkbookUnsafe =>
      'Çalışma kitabı güvensiz koruma, bağlantı, sorgu veya Excel 4.0 makro işlemi gerektirdiği için açılmadı.';

  @override
  String get errorPrintExcelComContractInvalid =>
      'Onaylı Excel otomasyon isteği geçersiz olduğu için çalışma kitabı açılmadı.';

  @override
  String get errorPrintExcelComFailed =>
      'Excel otomasyonu yazdırma başlamadan başarısız oldu.';

  @override
  String get errorPrintExcelExportDestinationExists =>
      'Özel çalışma sayfası PDF hedefi zaten var olduğu için dışa aktarım durduruldu.';

  @override
  String get errorPrintExcelExportFailed =>
      'Excel seçilen çalışma sayfasını PDF olarak dışa aktaramadı.';

  @override
  String get errorPrintExcelExportMalformedPdf =>
      'Excel boş veya geçersiz bir çalışma sayfası PDF\'i oluşturdu; hiçbir şey yazdırılmadı.';

  @override
  String get errorPrintExcelPrinterContextUnavailable =>
      'Windows, Excel çalışma sayfası için onaylanan yazıcı bağlamını oluşturamadı.';

  @override
  String get errorPrintExcelPrintJobRejected =>
      'Windows, kontrollü Excel çalışma sayfası yazdırma işini gönderim başlamadan reddetti.';

  @override
  String get errorPrintExcelPrintPreparationFailed =>
      'Windows, Excel çalışma sayfasını kontrollü yazdırmaya hazırlayamadı.';

  @override
  String get errorPrintExcelPrintOptionsRejected =>
      'Windows yazıcısı, Excel çalışma sayfası için onaylanan yazdırma ayarlarını reddetti.';

  @override
  String get errorPrintExcelPrintCancelledUnconfirmed =>
      'Excel çalışma sayfası yazdırma işi kuyruğa girdikten sonra iptal doğrulanamadı. Yeniden denemeden önce yazıcıyı kontrol edin.';

  @override
  String get errorPrintExcelPrintCompletionUnconfirmed =>
      'Excel çalışma sayfası işi kuyruk sistemine girdikten sonra Windows tamamlandığını doğrulayamadı. Yeniden denemeden önce yazıcıyı kontrol et.';

  @override
  String get errorPrintExcelControlledPdfUnavailable =>
      'Excel çalışma sayfaları için kontrollü PDF yazdırma bu yapıda kullanılamıyor.';

  @override
  String get errorPrintExcelWorkerStartFailed =>
      'Yalıtılmış Excel dışa aktarma çalışanı başlatılamadı.';

  @override
  String get errorPrintExcelDeadlineInvalid =>
      'Excel dışa aktarma zaman aşımı geçersiz.';

  @override
  String get errorPrintExcelWorksheetMetadataMissing =>
      'Onaylanmış çalışma sayfası bilgileri eksik. Yazdırma sırasını yeniden önizleyin.';

  @override
  String get errorPrintExcelSafetyMetadataMissing =>
      'Onaylanmış çalışma kitabı güvenlik bilgileri eksik. Yazdırma sırasını yeniden önizleyin.';

  @override
  String get errorPrintExcelGenerationExhausted =>
      'DocFlow yeniden başlatılana kadar Excel dışa aktarma güvenle devam edemez.';

  @override
  String get errorPrintExcelWorkerStopped =>
      'Yalıtılmış Excel dışa aktarma çalışanı beklenmedik şekilde durdu.';

  @override
  String get errorPrintExcelResponseStale =>
      'Eski bir Excel dışa aktarma yanıtı yazdırmadan önce reddedildi.';

  @override
  String get errorPrintExcelStagingUnavailable =>
      'Özel çalışma kitabı hazırlama alanı kullanılamıyor.';

  @override
  String get errorPrintExcelSourceNameUnavailable =>
      'Onaylanmış çalışma kitabı dosya adı kullanılamıyor.';

  @override
  String get errorPrintExcelStagingFailed =>
      'Çalışma kitabı özel hazırlama alanına kopyalanamadı.';

  @override
  String get errorPrintExcelStagingVerificationFailed =>
      'Özel çalışma kitabı kopyası onaylanmış kaynakla doğrulanamadı.';

  @override
  String get errorPrintExcelPrivatePdfUnavailable =>
      'Özel çalışma sayfası PDF\'i kullanılamıyor.';

  @override
  String get errorPrintExcelPrivatePdfEmpty =>
      'Excel boş bir çalışma sayfası PDF\'i üretti; hiçbir şey yazdırılmadı.';

  @override
  String get printWaitingForCurrent =>
      'Sonraki dosya başlatılmadan önce bu dosyanın tamamlanması bekleniyor';

  @override
  String get paramPrintPaper => 'Kağıt boyutu';

  @override
  String get paramPrintPaperHelp =>
      'Tüm dosyalar için başlangıç ayarıdır; Önizleme’de dosya bazında değiştirebilirsin.';

  @override
  String get paramPrintOrientation => 'Yön';

  @override
  String get paramPrintScaling => 'Sayfa ölçeği';

  @override
  String get paramPrintColor => 'Renk modu';

  @override
  String get paramPrintDuplex => 'Çift taraflı yazdırma';

  @override
  String get paramPrintRetries => 'Otomatik yeniden deneme';

  @override
  String get paramPrintRetriesHelp =>
      'Kuyruk hata kuralını uygulamadan önce başarısız işi 0-3 kez yeniden dener.';

  @override
  String get paramPrintOnError => 'İş yine başarısız olursa';

  @override
  String get paramPrintOnErrorHelp =>
      'Duraklat sıralamayı korur; Atla devam eder ve başarısız işi yeniden denemek için saklar.';

  @override
  String get paramPrintTimeout => 'İş zaman aşımı (dakika)';

  @override
  String get paramPrintTimeoutHelp =>
      'Yazıcı bloke, çevrimdışı veya yanıtsız kalırsa beklemeyi durdurur.';

  @override
  String get choicePrintPaperAuto => 'Yazıcı varsayılanı';

  @override
  String get choicePrintPaperA4 => 'A4';

  @override
  String get choicePrintPaperA3 => 'A3';

  @override
  String get choicePrintPaperA5 => 'A5';

  @override
  String get choicePrintPaperLetter => 'Letter';

  @override
  String get choicePrintPaperLegal => 'Legal';

  @override
  String get choicePrintOrientationAuto => 'Sayfadan otomatik';

  @override
  String get choicePrintOrientationPortrait => 'Dikey';

  @override
  String get choicePrintOrientationLandscape => 'Yatay';

  @override
  String get choicePrintScalingFit => 'Yazdırılabilir alana sığdır';

  @override
  String get choicePrintScalingActual => 'Gerçek boyut';

  @override
  String get choicePrintScalingShrink => 'Yalnız büyük sayfaları küçült';

  @override
  String get choicePrintColorPrinter => 'Yazıcı varsayılanı';

  @override
  String get choicePrintColorColor => 'Renkli';

  @override
  String get choicePrintColorGrayscale => 'Gri tonlama';

  @override
  String get choicePrintDuplexPrinter => 'Yazıcı varsayılanı';

  @override
  String get choicePrintDuplexSimplex => 'Tek taraflı';

  @override
  String get choicePrintDuplexLong => 'Çift taraflı, uzun kenar';

  @override
  String get choicePrintDuplexShort => 'Çift taraflı, kısa kenar';

  @override
  String get choicePrintOnErrorPause => 'Kuyruğu duraklat';

  @override
  String get choicePrintOnErrorSkip => 'Atla ve devam et';

  @override
  String get printEditorTitle => 'Yazdırma kuyruğunu kontrol et ve özelleştir';

  @override
  String printEditorRowCount(Object count) {
    return '$count dosya/iş';
  }

  @override
  String get printColInclude => 'Yazdır';

  @override
  String get printColStatus => 'Durum';

  @override
  String get printColFile => 'Dosya';

  @override
  String get printColFormat => 'Format';

  @override
  String get printColWorksheet => 'Çalışma sayfası';

  @override
  String get printColWorksheetVisibility => 'Görünürlük';

  @override
  String get printWorksheetVisibilityVisible => 'Görünür';

  @override
  String get printWorksheetVisibilityHidden => 'Gizli';

  @override
  String get printWorksheetVisibilityVeryHidden => 'Çok gizli';

  @override
  String get printColPrinter => 'Yazıcı';

  @override
  String get printColPaper => 'Kağıt';

  @override
  String get printColOrientation => 'Yön';

  @override
  String get printColScaling => 'Ölçek';

  @override
  String get printColColor => 'Renk';

  @override
  String get printColDuplex => 'Çift taraf';

  @override
  String get printColCopies => 'Kopya';

  @override
  String get printColPageRange => 'Sayfa aralığı';

  @override
  String get printColPageRangeHelp =>
      'all, tek bir sayfa veya virgülle ayrılmış aralıklar kullanın. Görseller all ya da 1; yerel uygulama dosyaları yalnızca all kabul eder.';

  @override
  String get printStatusReady => 'Hazır';

  @override
  String get printStatusPending => 'Bekliyor';

  @override
  String get printStatusPrinting => 'Yazdırılıyor';

  @override
  String get printStatusCompleted => 'Tamamlandı';

  @override
  String get printStatusCompletedUnconfirmed => 'Gönderildi; makbuz alınamadı';

  @override
  String get printStatusFailed => 'Başarısız';

  @override
  String get printStatusCancelled => 'İptal edildi';

  @override
  String get printStatusUncertain => 'Yazıcıyı kontrol et';

  @override
  String get warnPrintSettingsLimited =>
      'Bu format kendi yerel uygulamasıyla yazdırılır; kağıt ve yerleşim desteği o uygulamaya bağlıdır';

  @override
  String get warnPrintNoJobsSelected => 'Yazdırılacak satır seçilmedi';

  @override
  String get warnPrintInvalidRetries =>
      'Otomatik yeniden deneme 0 ile 3 arasında olmalıdır';

  @override
  String get warnPrintInvalidTimeout =>
      'İş zaman aşımı 1 ile 240 dakika arasında olmalıdır';

  @override
  String get warnPrintResumingQueue =>
      'Tamamlanmamış kayıtlı yazdırma kuyruğu inceleniyor';

  @override
  String get warnPrintCancelled =>
      'Yazdırma kuyruğu kullanıcı tarafından duraklatıldı';

  @override
  String get warnPrintCancelledUncertain =>
      'İptal geldiğinde sistem mevcut spool işini henüz tanımamıştı. Bu işi tekrar seçmeden önce yazıcıyı kontrol et.';

  @override
  String get warnPrintStateUncertain =>
      'Sistem bu işin yazdırılıp yazdırılmadığını doğrulayamadı. Otomatik tekrar yapılmadı; yeniden seçmeden önce çıktıyı kontrol et.';

  @override
  String get warnPrintSubmittedUnconfirmed =>
      'Windows bu işi kabul etti ancak son doğrulama kaydını tutmadı. İş tekrar gönderilmedi ve kuyruk sıradaki dosyayla devam etti.';

  @override
  String get errorPrintQueuePaused =>
      'Kuyruk duraklatıldı; kalan işler Devam Et için saklandı';

  @override
  String get errorPrintQueueSourceReselectionRequired =>
      'Bu eski yazdırma kuyruğu güvenle devam ettirilemez. Kuyruğu silip dosyaları yeniden seç.';

  @override
  String get printResumeTitle => 'Tamamlanmamış yazdırma kuyruğu bulundu';

  @override
  String printResumeSummary(Object count) {
    return '$count iş incelenebilir veya devam ettirilebilir';
  }

  @override
  String get printResumeReview => 'Yalnız basılmayanları yeniden dene';

  @override
  String get printResumeDiscard => 'Kuyruğu bırak';

  @override
  String get paramsPreviousTitle => 'Önceki yazdırma ayarları hazır';

  @override
  String get paramsPreviousHelp =>
      'Dosyalar güvenlik için saklanmaz. Önceki yazıcı, kağıt, yön, renk ve kuyruk ayarlarını yalnız istersen getir.';

  @override
  String get paramsPreviousApply => 'Önceki ayarları getir';

  @override
  String get printResumeIncomplete => 'Kuyruk güvenli biçimde duraklatıldı';

  @override
  String get printCancelQueue => 'Kuyruğu duraklat';

  @override
  String get printCancelling => 'Duraklatılıyor…';

  @override
  String get editorActionShowDetails => 'Otomatik alanları göster';

  @override
  String get editorActionHideDetails => 'Otomatik alanları gizle';

  @override
  String get layoutDataResetHeight => 'Yüksekliği sıfırla';

  @override
  String get layoutDataFit => 'Kullanılabilir alana sığdır';

  @override
  String layoutDataResizeHeight(Object label) {
    return '$label yüksekliğini ayarla';
  }

  @override
  String layoutColumnResize(Object column) {
    return '$column sütununun genişliğini ayarla';
  }

  @override
  String layoutColumnWidth(Object width) {
    return '$width piksel genişliğinde';
  }

  @override
  String get editorBulkTitle => 'Toplu düzenleme';

  @override
  String editorSelectedCount(Object count) {
    return '$count seçili';
  }

  @override
  String get editorSelectAll => 'Tümünü seç';

  @override
  String get editorSelectNone => 'Seçimi temizle';

  @override
  String editorSelectRow(Object row) {
    return '$row. satırı seç';
  }

  @override
  String get editorSelectGroup => 'Bu grubu seç';

  @override
  String get editorBulkField => 'Değiştirilecek alan';

  @override
  String get editorBulkValue => 'Yeni değer';

  @override
  String get editorBulkChoose => 'Bir değer seç';

  @override
  String get editorBulkApply => 'Seçilenlere uygula';

  @override
  String get editorBoolTrue => 'Evet';

  @override
  String get editorBoolFalse => 'Hayır';

  @override
  String get operationCancel => 'Mevcut öğeden sonra durdur';

  @override
  String get operationCancelling => 'Durduruluyor…';

  @override
  String get warnOperationCancelled =>
      'İşlem güvenli biçimde durduruldu; tamamlananlar geçmişte tutuldu ve kalanlar başlatılmadı';

  @override
  String get opRfiLogAddTitle => 'RFI Günlüğüne Çoklu Kayıt Ekle';

  @override
  String get opRfiFollowUpSyncTitle => 'RFI Log\'u Follow Up\'a eşitle';

  @override
  String get opRfiFollowUpSyncDesc =>
      'CIV/MEC yaşam döngüsü satırlarını ve FINAL Worley Signed/Not Signed satırlarını RFI Log\'dan güvenle eşitler; hazırlanmış eksik kayıtları, çıkarmaları ve grup durumlarını da işler.';

  @override
  String get paramRfiFollowUpRfiLogWorkbook => 'Kaynak RFI Log çalışma kitabı';

  @override
  String get paramRfiFollowUpRfiLogWorkbookHelp =>
      'Durumların okunacağı .xlsm veya .xlsx RFI Log dosyasını seç.';

  @override
  String get paramRfiFollowUpFollowUpWorkbook =>
      'Hedef Follow Up çalışma kitabı';

  @override
  String get paramRfiFollowUpFollowUpWorkbookHelp =>
      'Yalnız önizlenen RFI ve durum hücrelerinin değiştirilebileceği .xlsm veya .xlsx dosyasını seç.';

  @override
  String get errorParamRequiredRfiLogWorkbook =>
      'RFI Log çalışma kitabı zorunlu';

  @override
  String get errorParamRequiredFollowUpWorkbook =>
      'Follow Up çalışma kitabı zorunlu';

  @override
  String get errorRfiFollowUpWorkbookInvalid =>
      'Geçerli RFI Log ve Follow Up .xlsm veya .xlsx çalışma kitapları seç.';

  @override
  String get errorRfiFollowUpWorkbookLocked =>
      'Seçili çalışma kitabını Excel veya başka bir uygulamada kapatıp yeniden dene.';

  @override
  String get errorRfiFollowUpWorkbookChanged =>
      'Seçili çalışma kitaplarından biri önizlemeden sonra değişti. Uygulamadan önce yeni önizleme oluştur.';

  @override
  String get errorRfiFollowUpLayoutAmbiguous =>
      'Birden fazla güvenli Follow Up tablosu bulundu. Tek bir exact RFI NO./STATUS tablosu bırak.';

  @override
  String get errorRfiFollowUpLayoutMissing =>
      'Exact RFI NO. ve STATUS başlıklarına sahip güvenli Follow Up tablosu bulunamadı.';

  @override
  String get errorRfiFollowUpPatchVerification =>
      'DocFlow yalnız onaylanan durum hücrelerinin değiştiğini doğrulayamadı. Hiçbir çıktı kabul edilmedi.';

  @override
  String get errorRfiFollowUpProtected =>
      'Çalışma kitabı, hedef sheet veya gerekli hücreler korumalı ya da güvenli düzenlemeye uygun değil.';

  @override
  String get errorRfiFollowUpSignedPackage =>
      'Çalışma kitabında dijital imza var ve otomasyonla güvenli biçimde değiştirilemez.';

  @override
  String get errorRfiFollowUpLimitExceeded =>
      'Çalışma kitabı bu işlem için güvenli işleme sınırlarını aşıyor.';

  @override
  String get errorRfiFollowUpNoRowsSelected =>
      'Uygulamadan önce en az bir güvenli Follow Up satırı seç.';

  @override
  String get errorRfiFollowUpRetainedStorageFull =>
      'Sınırlı kurtarma depolaması dolu. Yeniden denemeden önce eski DocFlow kanıtlarını incele veya kaldır.';

  @override
  String get errorRfiFollowUpBackupExists =>
      'Ayrılmış adda bir yedek zaten var. Hiçbir çalışma kitabının üzerine yazılmadı.';

  @override
  String get errorRfiFollowUpBackupVerify =>
      'DocFlow çalışma kitabı yedeğini doğrulayamadı. İşlem kurtarma için durduruldu.';

  @override
  String get errorRfiFollowUpRecoveryRequired =>
      'İşlem kurtarma gerektiriyor; yeniden uygulamayın.';

  @override
  String get rfiFollowUpEditorTitle => 'RFI Follow Up durum adaylarını incele';

  @override
  String rfiFollowUpEditorRowCount(Object count) {
    return '$count Follow Up satırı';
  }

  @override
  String get rfiFollowUpColumnEnabled => 'Uygula';

  @override
  String get rfiFollowUpColumnRfiNo => 'RFI numarası';

  @override
  String get rfiFollowUpColumnSourceStatuses => 'RFI Log kaynak değeri';

  @override
  String get rfiFollowUpColumnCurrentStatus => 'Mevcut Follow Up durumu';

  @override
  String get rfiFollowUpColumnProposedRfiNo => 'Önerilen RFI grubu';

  @override
  String get rfiFollowUpColumnProposedStatus => 'Önerilen durum';

  @override
  String get rfiFollowUpColumnDisposition => 'Karar';

  @override
  String get rfiFollowUpColumnReason => 'Neden';

  @override
  String get rfiFollowUpColumnSheet => 'Sheet';

  @override
  String get rfiFollowUpColumnRow => 'Satır';

  @override
  String get rfiFollowUpColumnMappingDetails => 'Eşleme ayrıntıları';

  @override
  String get rfiFollowUpDispositionReady => 'Güncellenmeye hazır';

  @override
  String get rfiFollowUpDispositionAlreadySynced => 'Zaten eşitlenmiş';

  @override
  String get rfiFollowUpDispositionManualReview => 'Yalnız manuel inceleme';

  @override
  String get warnRfiFollowUpManualRows =>
      'satır manuel inceleme gerektiriyor ve otomatik seçilemez.';

  @override
  String get warnRfiFollowUpNoRowsSelected =>
      'Güvenle uygulanabilecek satır yok. Kapalı kutular manuel satırlardır; Neden sütununu incele.';

  @override
  String get warnRfiFollowUpUnmappedRecords =>
      'Desteklenmeyen rotası, eksik hazırlanmış hedef satırı veya belirsiz eşleşmesi olan RFI Log kayıtları değiştirilmedi. Kesin ITP/item ve kaynak/hedef bağlamı için Neden ve Eşleme ayrıntıları sütunlarını inceleyin.';

  @override
  String get warnRfiFollowUpReasonTargetRfiFormula =>
      'Hedef RFI numarası formül içeriyor.';

  @override
  String get warnRfiFollowUpReasonTargetRfiMerged =>
      'Hedef RFI numarası birleştirilmiş hücrede.';

  @override
  String get warnRfiFollowUpReasonBlankRfi => 'Hedef RFI numarası boş.';

  @override
  String get warnRfiFollowUpReasonInvalidRfi =>
      'Hedef RFI numarası CIV-0170 gibi exact kısa formatta olmalıdır.';

  @override
  String get warnRfiFollowUpReasonTargetDuplicate =>
      'Aynı tam RFI numarası hedef tabloda birden fazla kez bulunuyor.';

  @override
  String get warnRfiFollowUpReasonSourceMissing =>
      'RFI Log içinde exact tam-değer eşleşmesi yok.';

  @override
  String get warnRfiFollowUpReasonSourceFormula =>
      'Eşleşen kaynak RFI veya durum formül içeriyor.';

  @override
  String get warnRfiFollowUpReasonSourceMerged =>
      'Eşleşen kaynak RFI veya durum birleştirilmiş hücrede.';

  @override
  String get warnRfiFollowUpReasonSourceStatusBlank =>
      'Eşleşen RFI Log durumu boş.';

  @override
  String get warnRfiFollowUpReasonSourceStatusUnknown =>
      'Eşleşen RFI Log durumu exact Completed veya Ongoing değil.';

  @override
  String get warnRfiFollowUpReasonSourceConflict =>
      'RFI Log bu exact RFI numarası için çelişen durumlar içeriyor.';

  @override
  String get warnRfiFollowUpReasonTargetCellMissing =>
      'Hedef durum hücresi bulunmuyor.';

  @override
  String get warnRfiFollowUpReasonTargetFormula =>
      'Hedef durum hücresi formül içeriyor.';

  @override
  String get warnRfiFollowUpReasonTargetMerged =>
      'Hedef durum hücresi birleştirilmiş.';

  @override
  String get warnRfiFollowUpReasonTargetStatusUnknown =>
      'Mevcut hedef durumu boş, Signed veya Not Signed değil; üzerine yazılmayacak.';

  @override
  String get warnRfiFollowUpReasonAlreadySynced =>
      'Hedef zaten önerilen duruma sahip.';

  @override
  String get warnRfiFollowUpReasonReadySigned =>
      'RFI Log remark alanındaki exact Signed by Worley kanıtı hedefe Signed olarak uygulanabilir.';

  @override
  String get warnRfiFollowUpReasonReadyLogSync =>
      'RFI grubu ve ortak durum, onaylı RFI Log eşlemesinden eşitlenebilir.';

  @override
  String get warnRfiFollowUpReasonReadyWorleySync =>
      'FINAL Worley durumu, REMARKS alanındaki exact Signed by Worley satırından eşitlenebilir.';

  @override
  String get warnRfiFollowUpReasonReadyRemove =>
      'Cancelled veya Rejected RFI kayıtları çıkarılacak ve kalan grubun ortak durumu yeniden hesaplanacak.';

  @override
  String get warnRfiFollowUpReasonMappingMissing =>
      'Desteklenen bir Follow Up rotası var, ancak bu RFI için ITP, item ve bağlamı tam eşleşen hazır bir hedef satırı yok. Değişiklik yapılmadı.';

  @override
  String get warnRfiFollowUpReasonPartialContext =>
      'Bazı kaynak bağlamları hazır Follow Up satırlarıyla eşleşiyor, bazıları eşleşmiyor. Eşleşmeyen Tag No, Area ve Inspection Location değerlerini RFI Log\'da kontrol edin. DocFlow kaynak kaydı değiştirmez.';

  @override
  String get warnRfiFollowUpReasonMappingUnsupported =>
      'Bu ITP/item birleşimi geçerli şablonda bir Follow Up rotasına sahip değil. Otomatik eşitlenemez.';

  @override
  String get warnRfiFollowUpReasonMappingAmbiguous =>
      'Bu kayıtla birden fazla hedef satır eşleşiyor; tahmin yürütülmeden değiştirilmeden bırakıldı.';

  @override
  String get warnRfiFollowUpReasonReadyNotSigned =>
      'Exact Signed by Worley remark kanıtı bulunmadı; hedefe Not Signed uygulanabilir.';

  @override
  String get opRfiLogAddDesc =>
      'CIV, MEC, ELE ve PRC RFI numaralarını birlikte üretir, saatleri tüm çalışma sayfalarına göre planlar ve her sayfa için ayrı Excel tablosu hazırlar.';

  @override
  String get paramRfiWorkbook => 'RFI Log Excel dosyası';

  @override
  String get paramRfiWorkbookHelp =>
      'Makro etkin .xlsm veya standart .xlsx RFI Log dosyasını güvenli biçimde okumak ve güncellemek için seç.';

  @override
  String get dialogFilterExcelWorkbook => 'Excel (.xlsm, .xlsx)';

  @override
  String get errorRfiWorkbookInvalid =>
      'Geçerli bir makro etkin .xlsm veya standart .xlsx RFI Log dosyası seç.';

  @override
  String get paramRfiEmailText => 'Talep mailini buraya yapıştır';

  @override
  String get paramRfiEmailTextHelp =>
      'Mail gövdesini yapıştır. Yalnız bu önizlemede kullanılır; ayarlara veya Excel\'e kaydedilmez.';

  @override
  String get paramRfiEmailProvider => 'Mail çıkarma yöntemi';

  @override
  String get paramRfiEmailProviderHelp =>
      'Offline yöntem ücretsiz ve özeldir. Gemini Free isteğe bağlıdır ve maili Google\'a gönderir.';

  @override
  String get choiceRfiEmailProviderOffline => 'Offline — özel (önerilen)';

  @override
  String get choiceRfiEmailProviderGemini => 'Gemini Free — harici AI';

  @override
  String get paramRfiGeminiApiKey => 'Gemini API anahtarı (yalnız Gemini için)';

  @override
  String get paramRfiGeminiApiKeyHelp =>
      'Kaydedilmez. Windows\'ta GEMINI_API_KEY de tanımlayabilirsin. Gizli şirket mailleri için Gemini Free kullanma.';

  @override
  String get errorParamRequiredEmailProvider => 'Bir mail çıkarma yöntemi seç';

  @override
  String get warnRfiEmailExtracted =>
      'Mail alanları düzenlenebilir önizlemeye çıkarıldı';

  @override
  String get warnRfiEmailNeedsReview =>
      'Maildeki bazı bilgiler belirsiz veya eksik; satırları kontrol et';

  @override
  String get warnRfiAiFallback =>
      'Gemini kullanılamadı; özel offline çıkarma kullanıldı';

  @override
  String get warnRfiGeminiPrivacy =>
      'Gemini Free bu maili Google\'a gönderir. Ücretsiz katman içeriği ürün geliştirmede kullanılabilir; gizli bilgi gönderme.';

  @override
  String get paramRfiDiscipline => 'Disiplin';

  @override
  String get paramRfiCount => 'RFI numarası adedi';

  @override
  String get paramRfiCountHelp =>
      'Benzersiz RFI numaralarını sayar; aynı RFI\'nin ek satırları yeni numara veya saat tüketmez.';

  @override
  String get paramRfiSheetCounts => 'Sheet\'ler ve RFI adetleri';

  @override
  String get paramRfiSheetCountsHelp =>
      'Bir veya daha fazla sheet seçip her biri için benzersiz RFI numarası adedini belirle. Hepsi tek işlemde düzenlenip uygulanır.';

  @override
  String rfiSheetCountsTotal(Object count) {
    return 'Bu işlemde $count RFI numarası';
  }

  @override
  String get paramRfiOverflowNextDay =>
      'Taşan saatleri sonraki takvim gününe geçir';

  @override
  String get paramRfiOverflowNextDayHelp =>
      '17:30\'dan sonra ertesi gün 09:00\'da devam eder. Kapatılırsa geç saatler aynı günde kalır.';

  @override
  String get errorParamRequiredWorkbook => 'RFI Log Excel dosyası zorunlu';

  @override
  String get errorParamRequiredDiscipline => 'Disiplin zorunlu';

  @override
  String get errorParamRequiredCount => 'RFI adedi zorunlu';

  @override
  String get errorParamRequiredSheetCounts =>
      'En az bir sheet ve RFI numarası seç';

  @override
  String get noteRfiWorkbookUpdate => 'RFI Log satırları eklenecek';

  @override
  String get noteRfiRestoreBackup => 'Excel yedeğini geri yükle';

  @override
  String get warnRfiOverflowMoved =>
      'RFI saatleri 17:30\'u geçti ve sonraki takvim gününe taşındı';

  @override
  String get warnRfiOverflowKept =>
      'RFI saatleri 17:30\'u geçiyor ve aynı günde tutulacak';

  @override
  String get warnRfiGroupScheduleMismatch =>
      'Aynı RFI numarasındaki satırların inspection tarihi veya saati farklı';

  @override
  String get rfiEditorTitle => 'RFI kayıtlarını kontrol et ve düzenle';

  @override
  String rfiEditorRowCount(Object count) {
    return '$count Excel satırı';
  }

  @override
  String get rfiEditorScrollHint =>
      'Bütün alanları düzenlemek için yatay kaydır';

  @override
  String get rfiEditorCompactHint =>
      'Tam numara için üzerine gelin; otomatik alanlar Detaylar’da';

  @override
  String get rfiEditorReviewWarnings =>
      'Düzenlenen satırlar doğrulandı. Yeni uyarıları kontrol edip tekrar Uygula\'ya bas.';

  @override
  String rfiEditorCopied(Object count) {
    return 'Excel açıldı; $count satır filtrelendi ve Excel biçimiyle kopyalandı';
  }

  @override
  String get rfiActionDuplicate => 'Aynı RFI numarasıyla ek satır oluştur';

  @override
  String get rfiActionRemoveDuplicate => 'Bu ek satırı kaldır';

  @override
  String get rfiActionCopyMail => 'Excel’de filtrele ve birebir kopyala';

  @override
  String get rfiActionPrepareReply => 'Outlook yanıtını hazırla';

  @override
  String get rfiActionCopySheet => 'Excel\'den kopyala';

  @override
  String get rfiMailPreviewTitle => 'Outlook için Excel tablosu';

  @override
  String get rfiMailPreviewSourceWorkbook => 'Excel’den güncel veri';

  @override
  String get rfiMailPreviewSourcePlan => 'Onaylı plan verisi';

  @override
  String get rfiMailPreviewHint =>
      'Bu görünüm kopyalanacak kolonları ve değerleri gösterir. Kopyaladıktan sonra Outlook’a normal şekilde yapıştır; Excel biçimi ve hücre kenarlıkları korunur.';

  @override
  String get rfiMailPreviewCopy => 'Biçimli tabloyu kopyala';

  @override
  String get rfiActionDetails => 'Otomatik alanlar ve detaylar';

  @override
  String get rfiColSheet => 'Sheet';

  @override
  String get rfiColRfiNo => 'RFI';

  @override
  String get rfiColItpNo => 'ITP';

  @override
  String get rfiColItemNo => 'Item';

  @override
  String get rfiColDescription => 'Inspection Description';

  @override
  String get rfiColForms => 'Forms';

  @override
  String get rfiColTagNo => 'Tag No.';

  @override
  String get rfiColArea => 'Area';

  @override
  String get rfiColRowNo => 'Row No.';

  @override
  String get rfiColIssueDate => 'Veriliş';

  @override
  String get rfiColInspectionDate => 'Kontrol';

  @override
  String get rfiColStartTime => 'Saat';

  @override
  String get rfiColAzfenType => 'Insp. Type AZFEN';

  @override
  String get rfiColIntecType => 'Insp. Type INTEC';

  @override
  String get rfiColLocation => 'Konum';

  @override
  String get rfiColSubcontractor => 'Alt yüklenici';

  @override
  String get rfiDateDay => 'Gün';

  @override
  String get rfiDateMonth => 'Ay';

  @override
  String get rfiDateChangeYear => 'Yılı değiştir';

  @override
  String get rfiDateYear => 'Yıl';

  @override
  String get rfiColActions => 'Satırlar';

  @override
  String get actionClose => 'Kapat';

  @override
  String get paramOrganizeIntoRfiFolders => 'Her RFI için ayrı klasör oluştur';

  @override
  String get paramOrganizeIntoRfiFoldersHelp =>
      'Adlandırılmış PDF\'leri doğrudan hedefe kaydetmek için kapatın; okunamayan veya yinelenen dosyalar güvenlik için yine _review\'a gider';

  @override
  String get errorRfiWorkbookLocked =>
      'RFI Log açık veya yazmaya kapalı. Excel’de kaydedip kapatın ve dosyanın salt okunur olmadığını kontrol edin.';

  @override
  String get opRfiStageSignedFirstPagesTitle =>
      'İmzalı RFI ilk sayfalarını hazırla';

  @override
  String get opRfiNormalizePdfOrientationTitle => 'RFI PDF sayfalarını döndür';

  @override
  String get opRfiNormalizePdfOrientationDesc =>
      'Fast RFI Taraması\'ndan ayrı, CIV, MEC, ELE ve PRC altındaki PDF sayfa yönü ve dönüşünü düzeltme işlemidir. Yalnızca güvenle belirlenen düzeltmeler ayrı bir inceleme kopyasına yazılır; herhangi bir sayfa belirsizse PDF manuel inceleme için değiştirilmeden bırakılır.';

  @override
  String get paramRfiOrientationRfiSystemRoot => 'RFI sistemi kökü';

  @override
  String get paramRfiOrientationRfiSystemRootHelp =>
      'CIV, MEC, ELE ve PRC klasörlerini içeren salt okunur kaynak. Kaynak dosyalar değiştirilmez.';

  @override
  String get paramRfiOrientationReviewRoot => 'Yön düzeltme review klasörü';

  @override
  String get paramRfiOrientationReviewRootHelp =>
      'Düzeltilmiş kopyaların aynı klasör yapısıyla oluşturulacağı ayrı ve boş olmayan seçilebilir klasör.';

  @override
  String get paramScanIndexForceRescan => 'Bu sefer tam taramayı zorla';

  @override
  String get paramScanIndexForceRescanHelp =>
      'Bu işlemde önceki tarama indeksini kullanma. Görevden çıkınca seçenek otomatik kapanır.';

  @override
  String get errorRfiOrientationReviewRootMustBeSeparate =>
      'Review klasörü RFI sistemiyle çakışamaz';

  @override
  String get warnRfiOrientationNoPdfs =>
      'CIV, MEC, ELE veya PRC altında PDF bulunamadı';

  @override
  String get warnRfiOrientationUnsupported => 'PDF güvenle analiz edilemedi';

  @override
  String get warnRfiOrientationNeedsReview =>
      'En az bir sayfanın yönü yeterli güvenle belirlenemedi; dosya uygulanmadı';

  @override
  String get warnRfiOrientationReviewExists =>
      'Review hedefi zaten var; üzerine yazılmadı';

  @override
  String get noteRfiPdfOrientationNormalized =>
      'Onaylanan PDF sayfa yönlerini ayrı review kopyasında düzelt';

  @override
  String get rfiOrientationPages => 'Sayfa dönüşleri';

  @override
  String get opRfiStageSignedFirstPagesDesc =>
      'İmzalı taramaları tanır, RFI Sistem klasörlerinde hızlı ve güvenli arama yapar; arşive dokunmadan ayrı inceleme kopyalarında yalnızca ilk sayfayı yeniler.';

  @override
  String get paramRfiSignedScans => 'Yeni imzalı taramalar klasörü';

  @override
  String get paramRfiSignedScansHelp =>
      'İmza eklenmiş yeni RFI PDF\'leri. Birleşik taramadaki her sayfa bağımsız olarak tanınır.';

  @override
  String get paramRfiArchiveRoot => 'Mevcut RFI arşivinin kökü';

  @override
  String get paramRfiArchiveRootHelp =>
      'Salt-okunur RFI System kökü. Tüm alt klasörler sınırlı multi-core keşifle aranır; yalnız exact RFI PDF dosya adları eşleşebilir.';

  @override
  String get paramRfiReviewRoot => 'Review çıktı klasörü';

  @override
  String get paramRfiReviewRootHelp =>
      'Hazırlanan kopyaların arşivdeki aynı göreli klasör düzeniyle yazılacağı ayrı ve boş olması önerilen klasör.';

  @override
  String get paramRfiScanSubfolders =>
      'Yeni taramaların alt klasörlerini de tara';

  @override
  String get paramRfiScanSubfoldersHelp =>
      'Varsayılan olarak açıktır. Yeni taramalar klasörünün her derinliğindeki PDF\'leri tanır; yalnız bilinçli üst-seviye tarama için kapatın.';

  @override
  String get errorParamRequiredSignedScans =>
      'Yeni imzalı taramalar klasörü gerekli';

  @override
  String get errorParamRequiredArchiveRoot =>
      'Mevcut RFI arşivinin kökü gerekli';

  @override
  String get errorParamRequiredReviewRoot => 'Review çıktı klasörü gerekli';

  @override
  String get errorRfiReviewRootMustBeSeparate =>
      'Review çıktı klasörü hem arşivden hem de yeni taramalar klasöründen tamamen ayrı olmalıdır. Hiçbir dosyaya dokunulmadı.';

  @override
  String get noteRfiSignedFirstPageStaged =>
      'İmzalı ilk sayfa ile review kopyası hazırlanacak';

  @override
  String get noteRfiCleanupStaleTemp =>
      'Tamamlanmış review PDF\'sinden kalan geçici bağlantıyı temizle';

  @override
  String get warnRfiNoSignedPdfs => 'Yeni taramalar klasöründe PDF bulunamadı';

  @override
  String get warnRfiSignedUnreadable =>
      'PDF okunamadı; bu RFI için çıktı hazırlanmadı';

  @override
  String get warnRfiSignedDuplicate =>
      'Aynı RFI numarası için birden çok yeni tarama bulundu; otomatik seçim yapılmadı';

  @override
  String get warnRfiArchiveUnreadable =>
      'Arşivin tamamı güvenle taranamadı; eksik veya yanlış eşleşmeyi önlemek için hiçbir çıktı hazırlanmadı';

  @override
  String get warnRfiArchiveMissing =>
      'Arşivde bu RFI numarasına ait dosya bulunamadı';

  @override
  String get warnRfiArchiveAmbiguous =>
      'Arşivde bu RFI numarasına ait birden çok dosya bulundu; satırdaki arşiv yolundan bir PDF seçin';

  @override
  String get warnRfiReviewExists =>
      'Review hedefinde dosya zaten var; üzerine yazılmadı';

  @override
  String get warnRfiStaleTempFound =>
      'Review PDF zaten tamamlanmış; önceki Windows kilidinden kalan geçici dosya güvenle temizlenecek';

  @override
  String get warnRfiReviewPathBlocked =>
      'Review içindeki hedef yol bir dosya veya yönlendirilmiş klasör tarafından engelleniyor; çıktı hazırlanmadı';

  @override
  String get warnRfiSignedExtraPages =>
      'Yeni PDF birden çok sayfa içeriyor; yalnız imzalı ilk sayfa kullanılacak';

  @override
  String get warnRfiSignedMultiPage =>
      'Çok sayfalı imzalı tarama algılandı; her sayfa bağımsız olarak analiz edildi';

  @override
  String get warnRfiFirstPageLayoutDiffers =>
      'Yeni ilk sayfanın boyutu veya yönü eskisinden farklı; uygulamadan önce önizlemeyi kontrol edin';

  @override
  String get warnRfiPdfFidelityUnsupported =>
      'Bu PDF, belge bütünlüğü riske atılmadan hazırlanamaz; nedeni inceleyip işlemi elle tamamlayın';

  @override
  String get rfiSignedEditorTitle => 'İmzalı ilk sayfa incelemesi';

  @override
  String rfiSignedEditorRowCount(Object count) {
    return 'Eşleşen $count RFI PDF\'i';
  }

  @override
  String get rfiSignedColumnEnabled => 'Hazırla';

  @override
  String get rfiSignedColumnCode => 'RFI numarası';

  @override
  String get rfiSignedColumnScan => 'İmzalı tarama';

  @override
  String get rfiSignedColumnArchive => 'Arşiv yolu';

  @override
  String get rfiSignedArchiveChoose => 'Arşiv PDF\'ini seçin';

  @override
  String get rfiSignedPreviewTitle => 'İlk sayfa karşılaştırması';

  @override
  String get rfiSignedPreviewHelp =>
      'Arşivdeki ilk sayfayı onaylanan planın seçtiği yeni imzalı sayfayla karşılaştırın.';

  @override
  String get rfiSignedPreviewRfi => 'Önizlenecek RFI';

  @override
  String get rfiSignedPreviewArchive => 'Mevcut arşiv sayfası';

  @override
  String get rfiSignedPreviewSigned => 'Yeni imzalı sayfa';

  @override
  String rfiSignedPreviewArchiveAlt(Object code) {
    return '$code için arşivdeki mevcut ilk sayfa';
  }

  @override
  String rfiSignedPreviewSignedAlt(Object code) {
    return '$code için yeni imzalı sayfa';
  }

  @override
  String get rfiSignedPreviewChooseArchive =>
      'İlk sayfasını göstermek için bir arşiv PDF\'i seçin.';

  @override
  String get rfiSignedPreviewUnavailable =>
      'Sınırlı sayfa önizlemesi hazırlanamadı; uygulamadan önce PDF\'i elle inceleyin.';

  @override
  String rfiSignedPreviewOmitted(Object count) {
    return 'Güvenli önizleme sınırına ulaşıldığı için $count ek RFI satırının önizlemesi gösterilemiyor.';
  }

  @override
  String get rfiSignedPreviewPrevious => 'Önceki RFI';

  @override
  String get rfiSignedPreviewNext => 'Sonraki RFI';

  @override
  String get rfiSignedPreviewOnlyPageOne =>
      'Yalnızca 1. sayfa değiştirilecek; arşivdeki diğer sayfalar aynı kalacak.';

  @override
  String get rfiSignedPreviewBefore => 'ÖNCE';

  @override
  String get rfiSignedPreviewAfter => 'SONRA';

  @override
  String get rfiSignedPreviewEnlarge => 'Büyüt';

  @override
  String get rfiSignedNavLabel => 'İmzalı sayfa önizleme bölümleri';

  @override
  String get rfiSignedNavApproved => 'Onaylanan RFI\'lar';

  @override
  String get rfiSignedNavCompare => 'Sayfaları karşılaştır';

  @override
  String get rfiSignedNavReview => 'Review kopyaları';

  @override
  String get rfiSignedWarningReviewTitle =>
      'Warning veren PDF\'ler Review\'a gönderilecek';

  @override
  String get rfiSignedWarningReviewHelp =>
      'Apply, etkilenen her kaynak PDF\'i değiştirmeden kopyalar ve alt klasör yolunu _review altında korur.';

  @override
  String get rfiSignedWarningReviewPlanned => 'Kopyalanacak';

  @override
  String get rfiSignedWarningReviewExists => 'Zaten var';

  @override
  String get rfiSignedWarningReviewBlocked => 'Engellendi';

  @override
  String get rfiSignedWarningReviewDestination => 'Review hedefi';

  @override
  String rfiSignedWarningReviewPage(Object page) {
    return 'Sayfa $page';
  }

  @override
  String get rfiSignedWarningReviewFile => 'Tüm dosya';

  @override
  String get noteRfiSignedWarningReviewCopy =>
      'Warning PDF\'ini Review\'a kopyala';

  @override
  String get warnRfiWarningReviewExists =>
      'Warning PDF\'i Review\'da zaten var ve üzerine yazılmayacak';

  @override
  String get warnRfiWarningReviewBlocked =>
      'Hedef yolu engelli olduğu için warning PDF\'i Review\'a kopyalanamıyor';

  @override
  String get progressStages => 'İşlem aşamaları';

  @override
  String get progressPhasePreparing => 'Önizleme hazırlanıyor';

  @override
  String get progressPhasePreview => 'Önizleme oluşturuluyor';

  @override
  String get progressPhaseApply => 'Onaylanan plan uygulanıyor';

  @override
  String get progressPhasePrintWaiting => 'Yazıcı bekleniyor';

  @override
  String get progressPhaseScan => 'Dosyalar taranıyor';

  @override
  String get progressPhaseScanDiscovery => 'Taranacak PDF\'ler bulunuyor';

  @override
  String get progressPhaseFileGroupDiscovery =>
      'Gruplanacak dosyalar bulunuyor';

  @override
  String get progressPhaseFileGroupAnalysis => 'RFI numaraları okunuyor';

  @override
  String get progressPhaseFileGroupPlanning => 'RFI klasör planı hazırlanıyor';

  @override
  String get progressPhaseScanOcr => 'PDF\'ler OCR ile okunuyor';

  @override
  String get progressPhaseScanPlanning => 'Dosya planı hazırlanıyor';

  @override
  String get progressPhaseWorkbookReading => 'Çalışma kitabı okunuyor';

  @override
  String get progressPhaseRfiRequestAnalysis => 'RFI talebi analiz ediliyor';

  @override
  String get progressPhaseRfiPlanPreparation => 'RFI satırları hazırlanıyor';

  @override
  String get progressPhaseFollowUpAnalysis =>
      'Follow Up eşleştirmeleri hesaplanıyor';

  @override
  String get progressPhaseSystemDiscovery => 'RFI sistem kayıtları bulunuyor';

  @override
  String get progressPhaseSystemSignatureAnalysis =>
      'Worley imzaları analiz ediliyor';

  @override
  String get progressPhaseSystemWorkbookRead =>
      'RFI çalışma kitapları okunuyor';

  @override
  String get progressPhaseSystemMapping =>
      'İmzalar çalışma kitabı kayıtlarıyla eşleştiriliyor';

  @override
  String get progressPhaseOrientationDiscovery =>
      'Yönü denetlenecek PDF\'ler bulunuyor';

  @override
  String get progressPhaseOrientationInspection =>
      'PDF yapısı ve tarama önbelleği denetleniyor';

  @override
  String get progressPhaseOrientationAnalysis =>
      'PDF sayfa yönleri analiz ediliyor';

  @override
  String get progressPhasePrinterCheck => 'Yazıcı hazırlığı denetleniyor';

  @override
  String get progressPhasePrintFileInspection =>
      'Baskı dosyaları ve ayarları inceleniyor';

  @override
  String get progressPhaseOutlookSelection => 'Outlook seçimi doğrulanıyor';

  @override
  String get progressPhaseSignedDiscovery => 'İmzalı PDF\'ler bulunuyor';

  @override
  String get progressPhaseSignedInspection => 'PDF sayfaları inceleniyor';

  @override
  String get progressPhaseSignedOcr => 'RFI numaraları okunuyor';

  @override
  String get progressPhaseSignedClassification =>
      'İmzalı sayfalar eşleştiriliyor';

  @override
  String get progressPhaseArchiveScan => 'RFI arşivinde aranıyor';

  @override
  String get progressPhaseComparisonPreparation =>
      'Sayfa karşılaştırmaları hazırlanıyor';

  @override
  String get progressPhasePreviewReady => 'Önizleme hazır';

  @override
  String get recoveryTitle => 'Kurtarma merkezi';

  @override
  String get homeRecoveryDesc =>
      'Bekleyen günlükleri ve belirsiz yazdırma işlerini güvenle inceleyin.';

  @override
  String get recoveryHelp =>
      'Yalnız yerel günlükler ve kaydedilmiş yazdırma kuyrukları kurtarma yetkisi verir.';

  @override
  String get recoveryNoEffect =>
      'Hiçbir değişiklik olmadı — yeniden önizleme güvenli';

  @override
  String get recoveryOutcomeUnknown => 'Sonuç bilinmiyor — otomatik tekrar yok';

  @override
  String get recoveryManual => 'Elle inceleme gerekli';

  @override
  String get recoveryRestoreOriginal =>
      'Follow Up dosyasının aslını geri yükle';

  @override
  String get recoveryPrintQueue => 'Belirsiz yazdırma kuyruğu';

  @override
  String get recoveryEmpty => 'Kurtarılacak öğe yok';

  @override
  String get recoveryEmptyHelp =>
      'Bekleyen günlük veya yarım kalmış yazdırma kuyruğu bulunamadı.';

  @override
  String get effectCommitted => 'Tamamlandı';

  @override
  String get effectNoEffectRetryable =>
      'Değişiklik olmadı — yeniden denemek güvenli';

  @override
  String get effectPartialCommitted =>
      'Kısmen tamamlandı — kurtarmayı inceleyin';

  @override
  String get effectOutcomeUnknown => 'Sonuç bilinmiyor — otomatik tekrar yok';

  @override
  String get effectCancelledClean => 'Temiz iptal — değişiklik olmadı';

  @override
  String get effectCancelledPartial => 'Kısmi iptal — kurtarmayı inceleyin';

  @override
  String get presetTitle => 'Ön ayarlar';

  @override
  String get presetSelect => 'Kayıtlı ön ayar';

  @override
  String get presetSelectPlaceholder => 'Bir ön ayar seçin';

  @override
  String get presetName => 'Yeni ön ayar adı';

  @override
  String get presetApply => 'Ön ayarı uygula';

  @override
  String get presetSave => 'Ön ayarı kaydet';

  @override
  String get presetReplace => 'Ön ayarı değiştir';

  @override
  String get presetDelete => 'Ön ayarı sil';

  @override
  String presetConfirmReplace(Object name) {
    return '$name adlı ön ayar zaten var. Değiştirilsin mi?';
  }

  @override
  String presetConfirmDelete(Object name) {
    return '$name adlı ön ayar silinsin mi?';
  }

  @override
  String get presetEmpty => 'Henüz kayıtlı ön ayar yok.';

  @override
  String presetSaved(Object name) {
    return '$name ön ayarı kaydedildi.';
  }

  @override
  String presetDeleted(Object name) {
    return '$name ön ayarı silindi.';
  }

  @override
  String get errorPresetNameInvalid => 'Ön ayar adı 1–64 karakter olmalıdır.';

  @override
  String get errorPresetDuplicate => 'Bu adda bir ön ayar zaten var.';

  @override
  String get errorPresetLimit =>
      'Bu işlem için en fazla 20 ön ayar kaydedilebilir.';

  @override
  String get errorPresetStale =>
      'Bu ön ayar artık mevcut değil. Listeyi yenileyip tekrar deneyin.';

  @override
  String get errorPresetWrite =>
      'Ön ayar ayarları kaydedilemedi. Tekrar deneyin.';

  @override
  String get opRfiOutlookReplyDraftTitle =>
      'Outlook Tümünü Yanıtla taslağı hazırla';

  @override
  String get opRfiOutlookReplyDraftDesc =>
      'Yalnızca Outlook\'ta seçili olan tek e-postayı kullanır, Tümünü Yanıtla yanıtını hazırlar ve inceleme için GÖNDERİLMEMİŞ taslak olarak gösterir.';

  @override
  String get paramRfiOutlookReplyDraftReplyText => 'Yanıt metni';

  @override
  String get paramRfiOutlookReplyDraftReplyTextHelp =>
      'Başa eklenecek metni gir. Önizleme HTML\'yi kaçışlar; uygulama iletiyi asla göndermez veya alıcı eklemez.';

  @override
  String get errorParamRequiredReplyText => 'Yanıt metni gerekli';

  @override
  String get stepRfiOutlookReplyDraft =>
      'Gönderilmemiş Tümünü Yanıtla taslağını göster';

  @override
  String get outlookDraftPreviewTitle =>
      'Gönderilmemiş Outlook yanıtı önizlemesi';

  @override
  String get outlookDraftSubject => 'Seçili ileti konusu';

  @override
  String get outlookDraftPlainText => 'Düz metin';

  @override
  String get outlookDraftHtml => 'Kaçışlanmış HTML';

  @override
  String get warnRfiOutlookDraftUnsent =>
      'Uygula, GÖNDERİLMEMİŞ bir Tümünü Yanıtla taslağı gösterir; DocFlow bunu asla göndermez.';

  @override
  String get warnRfiOutlookDraftUndoConditional =>
      'Geri alma yalnızca Outlook kalıcı bir taslağı otomatik kaydederse ve aynı taslak değişmeden kalırsa kullanılabilir.';

  @override
  String get warnRfiOutlookDraftUndoUnavailable =>
      'Outlook kalıcı otomatik kaydedilmiş bir taslak sağlamadı; geri alma kullanılamaz.';

  @override
  String get errorRfiOutlookDraftUnsupportedPlatform =>
      'Outlook taslak otomasyonu yalnızca Windows\'ta kullanılabilir.';

  @override
  String get errorRfiOutlookDraftReplyRequired =>
      'Önizlemeden önce yanıt metni gir.';

  @override
  String get errorRfiOutlookDraftRfiLogNotCommitted =>
      'Outlook yanıtı yalnız aynı tamamlanmış RFI Log işleminden hazırlanabilir.';

  @override
  String get errorRfiOutlookDraftReplyTooLong =>
      'Yanıt metni 10.000 karakter sınırını aşıyor.';

  @override
  String get errorRfiOutlookDraftReplyInvalid =>
      'Yanıt metni desteklenmeyen bir denetim karakteri içeriyor.';

  @override
  String get errorRfiOutlookDraftSelectionRequired =>
      'Etkin Outlook Explorer penceresinde tam olarak bir e-posta seç.';

  @override
  String get errorRfiOutlookDraftSingleSelectionRequired =>
      'Tam olarak bir Outlook e-postası seç.';

  @override
  String get errorRfiOutlookDraftMailItemRequired =>
      'Geçerli Outlook seçimi bir e-posta iletisi değil.';

  @override
  String get errorRfiOutlookDraftSelectionInvalid =>
      'Seçili Outlook e-postasının kararlı bir kimliği yok.';

  @override
  String get errorRfiOutlookDraftSubjectTooLong =>
      'Seçili Outlook konusu önizleme sınırını aşıyor.';

  @override
  String get errorRfiOutlookDraftSelectionChanged =>
      'Seçili Outlook e-postası önizlemeden sonra değişti; yeniden önizle.';

  @override
  String get errorRfiOutlookDraftOutlookUnavailable =>
      'Microsoft Outlook masaüstü otomasyonu kullanılamıyor.';

  @override
  String get errorRfiOutlookDraftOutlookNotRunning =>
      'Önizlemeden önce Outlook masaüstünü aç ve bir e-posta seç.';

  @override
  String get errorRfiOutlookDraftComUnavailable =>
      'Windows Outlook otomasyon dairesi kullanılamıyor.';

  @override
  String get errorRfiOutlookDraftComFailed =>
      'Outlook gerekli sınırlı ileti özelliğini sağlayamadı.';

  @override
  String get errorRfiOutlookDraftReplyAllFailed =>
      'Outlook Tümünü Yanıtla taslağının oluşturulduğunu doğrulayamadı; Outlook\'u elle incele.';

  @override
  String get errorRfiOutlookDraftBodyPrependFailed =>
      'Outlook yanıt gövdesi güncellemesini doğrulayamadı; Outlook\'u elle incele.';

  @override
  String get errorRfiOutlookDraftBodyFormatUnsupported =>
      'Outlook yanıtı desteklenmeyen zengin metin biçimini kullanıyor; Outlook\'u elle incele.';

  @override
  String get errorRfiOutlookDraftContentTooLarge =>
      'Outlook taslak gövdesi güvenli otomasyon sınırını aşıyor.';

  @override
  String get errorRfiOutlookDraftDisplayUnknown =>
      'Outlook gönderilmemiş taslağın gösterildiğini doğrulayamadı; Outlook\'u elle incele.';

  @override
  String get errorRfiOutlookDraftDraftMissingOrChanged =>
      'Tam otomatik kaydedilmiş taslak bulunamadığı veya değiştiği için geri alma reddedildi.';

  @override
  String get errorRfiOutlookDraftDeleteUnknown =>
      'Outlook değişmemiş taslağın silindiğini doğrulayamadı; Outlook\'u elle incele.';

  @override
  String get errorRfiOutlookDraftUndoUnavailable =>
      'Outlook kalıcı bir taslağı otomatik kaydetmediği için geri alma kullanılamaz.';

  @override
  String get opRfiOutlookArchiveSelectedTitle =>
      'Seçili Outlook e-postasını arşivle';

  @override
  String get opRfiOutlookArchiveSelectedDesc =>
      'Seçili tek Outlook e-postasını göndermeden, taşımadan veya silmeden yerel Unicode MSG kopyası olarak kaydeder.';

  @override
  String get paramRfiOutlookArchiveDestination => 'Mail arşiv klasörü';

  @override
  String get paramRfiOutlookArchiveDestinationHelp =>
      'Ayrı bir hazırlık klasörü seç. RFİ\'ye göre dosyaları grupla daha sonra bu MSG\'yi ilgili tüm exact RFI klasörlerine kopyalayabilir.';

  @override
  String get stepRfiOutlookArchive => 'Korumalı yerel MSG kopyası oluştur';

  @override
  String get warnRfiOutlookArchiveLocalCopy =>
      'Uygula bir yerel MSG kopyası oluşturur; seçili Outlook e-postası ve posta kutusu değişmez.';

  @override
  String get errorRfiOutlookArchiveDestinationExists =>
      'Planlanan MSG dosya adı arşiv klasöründe zaten var; dosyayı yeniden adlandır veya taşı, sonra yeniden önizle.';

  @override
  String get errorRfiOutlookArchivePlanUnavailable =>
      'Onaylanan Outlook arşiv planı artık kullanılamıyor; yeniden önizle.';

  @override
  String get errorRfiOutlookArchiveSelectionChanged =>
      'Seçili Outlook e-postası önizlemeden sonra değişti; asıl e-postayı seçip yeniden önizle.';

  @override
  String get errorRfiOutlookArchiveExportFailed =>
      'Outlook özel MSG çıktısını oluşturamadı; arşiv dosyası yayınlanmadı.';

  @override
  String get errorRfiOutlookArchiveInvalidMsg =>
      'Outlook boş, aşırı büyük veya geçersiz MSG çıktısı oluşturdu; arşiv dosyası yayınlanmadı.';

  @override
  String get errorRfiOutlookArchiveWriteFailed =>
      'MSG arşiv dosyası oluşturulamadı; mevcut dosyanın üzerine yazılmadı.';

  @override
  String get errorRfiOutlookArchiveWriteUnknown =>
      'MSG arşiv yazımı doğrulanamadı; yeniden denemeden önce hedefi incele.';

  @override
  String get opRfiSystemSyncTitle =>
      'RFI Sistemini Analiz Et ve Worley İmzalarını Eşitle';

  @override
  String get opRfiSystemSyncDesc =>
      'Disiplinlere göre RFI sistem klasörlerini tarar, 1. sayfadaki Others bölümünde Worley imzalarını inceler ve RFI Log ile Follow Up çalışma kitaplarını eşitler.';

  @override
  String get paramRfiSystemSyncRfiSystemRoot => 'RFI Sistem Kök Klasörü';

  @override
  String get paramRfiSystemSyncRfiSystemRootHelp =>
      'CIV, MEC, ELE ve PRC disiplin klasörlerini içeren kök klasörü seçin.';

  @override
  String get paramRfiSystemSyncRfiLogWorkbook => 'RFI Log Çalışma Kitabı';

  @override
  String get paramRfiSystemSyncRfiLogWorkbookHelp =>
      'Açıklamalar (REMARKS) sütununa \'Signed by Worley\' yazılacak RFI Log (.xlsm veya .xlsx) dosyasını seçin.';

  @override
  String get paramRfiSystemSyncFollowUpWorkbook => 'Follow Up Çalışma Kitabı';

  @override
  String get paramRfiSystemSyncFollowUpWorkbookHelp =>
      'Signed By Worley CIV ve Signed By Worley MEC kontrol tablolarını içeren güncel Follow Up dosyasını seçin.';

  @override
  String get errorParamRequiredRfiSystemRoot =>
      'RFI Sistem kök klasörü zorunludur';

  @override
  String get errorRfiSystemSyncWorkbookInvalid =>
      'Geçerli RFI Log ve Follow Up .xlsm veya .xlsx çalışma kitaplarını seçin.';

  @override
  String get errorRfiSystemSyncWorkbookLocked =>
      'Seçilen çalışma kitabını Excel veya başka bir uygulamada kapatıp tekrar deneyin.';

  @override
  String get errorRfiSystemSyncWorkbookLayoutInvalid =>
      'RFI Log içindeki CIV, MEC, ELE ve PRC sayfalarının her birinde tek bir RFI No. ve REMARKS başlığı bulunmalıdır.';

  @override
  String get errorRfiSystemSyncFollowUpLayoutInvalid =>
      'Follow Up dosyasında bir Signed By Worley CIV tablosu ve bir Signed By Worley MEC kontrol tablosu bulunmalıdır.';

  @override
  String get errorRfiSystemSyncApprovedUpdateInvalid =>
      'Onaylanan çalışma kitabı değişikliği geçersiz; yeniden önizleme yapın.';

  @override
  String get errorRfiSystemSyncWorkbookChanged =>
      'Seçili çalışma kitabı önizlemeden sonra değişti; hiçbir değişiklik uygulanmadı. Yeniden önizleme yapın.';

  @override
  String get warnRfiSystemSyncNoChangesNeeded =>
      'Algılanan tüm RFI\'lar zaten eşitlenmiş veya imzasız.';

  @override
  String get warnRfiSystemSyncNoRowsSelected =>
      'Eşitlemek için en az bir güvenli imzalı RFI seçin.';

  @override
  String get noteRfiSystemSyncRfiLog =>
      'Worley imzalarını RFI Log açıklamalarına eşitle';

  @override
  String get noteRfiSystemSyncFollowUp =>
      'Final ITP imzalı RFI\'ları Follow Up çalışma kitabına eşitle';

  @override
  String get noteRfiSystemSyncBackup => 'Eşitleme öncesi yedek oluştur';

  @override
  String get noteRfiSystemSyncRestore =>
      'Çalışma kitabını eşitleme öncesi yedekten geri yükle';

  @override
  String get rfiSystemSyncEditorTitle =>
      'RFI Sistemi ve Worley İmza Eşitlemesini İncele';

  @override
  String rfiSystemSyncEditorRowCount(Object count) {
    return '$count taranan RFI';
  }

  @override
  String get rfiSystemSyncColumnEnabled => 'Uygula';

  @override
  String get rfiSystemSyncColumnDiscipline => 'Disiplin';

  @override
  String get rfiSystemSyncColumnRfiNo => 'RFI No.';

  @override
  String get rfiSystemSyncColumnWorleyStatus => 'Worley İmzası';

  @override
  String get rfiSystemSyncColumnRfiLogAction => 'RFI Log İşlemi';

  @override
  String get rfiSystemSyncColumnFollowUpAction => 'Follow Up İşlemi';

  @override
  String get rfiSystemSyncColumnDisposition => 'Durum';

  @override
  String get rfiSystemSyncColumnReason => 'Kanıt / Detaylar';

  @override
  String get rfiSystemSyncDispositionReady => 'Eşitlemeye hazır';

  @override
  String get rfiSystemSyncDispositionAlreadySynced => 'Zaten eşitlenmiş';

  @override
  String get rfiSystemSyncDispositionUnsigned => 'İmzasız';

  @override
  String get rfiSystemSyncDispositionManualReview => 'Manuel inceleme';

  @override
  String get opRfiCompleteStatusSyncTitle =>
      'RFI Sisteminden tamamlanan RFI\'ları işaretle';

  @override
  String get opRfiCompleteStatusSyncDesc =>
      'RFI Sistem alt klasörlerindeki PDF\'leri tarar ve RFI Günlüğündeki denetim durumunu \'Tamamlandı\' (Completed) olarak güncellemek için önizleme hazırlar.';

  @override
  String get paramRfiCompleteStatusRfiSystemRoot => 'RFI Sistem Kök Klasörü';

  @override
  String get paramRfiCompleteStatusRfiSystemRootHelp =>
      'RFI Sistem kökünü seçin. Bütün alt klasörler salt okunur taranır; sembolik bağlantılar izlenmez.';

  @override
  String get paramRfiCompleteStatusRfiLogWorkbook => 'RFI Log Çalışma Kitabı';

  @override
  String get paramRfiCompleteStatusRfiLogWorkbookHelp =>
      'İncelemeden sonra güvenli Inspection Status hücreleri Completed yapılabilecek .xlsm veya .xlsx RFI Log dosyasını seçin.';

  @override
  String get errorRfiCompleteStatusSystemRootInvalid =>
      'Okunabilir bir RFI Sistem kök klasörü seçin.';

  @override
  String get errorRfiCompleteStatusWorkbookInvalid =>
      'Geçerli bir RFI Log .xlsm veya .xlsx dosyası seçin.';

  @override
  String get errorRfiCompleteStatusWorkbookLocked =>
      'Seçili RFI Log dosyasını Excel veya başka bir uygulamada kapatıp tekrar deneyin.';

  @override
  String get errorRfiCompleteStatusWorkbookLayoutInvalid =>
      'CIV, MEC, ELE ve PRC sayfalarının her birinde bir exact RFI No. ve Inspection Status başlığı bulunmalıdır.';

  @override
  String get errorRfiCompleteStatusApprovedUpdateInvalid =>
      'Onaylanan tamamlanma durumu değişikliği geçersiz; yeniden önizleme yapın.';

  @override
  String get warnRfiCompleteStatusInvalidPdfName =>
      'Bir RFI No. PDF\'sinin adı canonical değil; kullanılmadı.';

  @override
  String get warnRfiCompleteStatusInvalidPdfNameOverflow =>
      'Diğer canonical olmayan RFI No. PDF adları uyarı listesinden çıkarıldı.';

  @override
  String get warnRfiCompleteStatusNoChangesNeeded =>
      'Güvenle uygulanabilecek RFI Log tamamlanma değişikliği yok.';

  @override
  String get warnRfiCompleteStatusNoRowsSelected =>
      'Eşitlemek için en az bir güvenli tamamlanmış RFI seçin.';

  @override
  String get noteRfiCompleteStatusRfiLog =>
      'Onaylanan RFI Log Inspection Status hücrelerini Completed yap';

  @override
  String get rfiCompleteStatusEditorTitle =>
      'Tamamlanan RFI kanıtlarını ve durum değişikliklerini incele';

  @override
  String rfiCompleteStatusEditorRowCount(Object count) {
    return 'Exact tamamlanmış PDF bulunan $count RFI';
  }

  @override
  String get rfiCompleteStatusColumnEnabled => 'Uygula';

  @override
  String get rfiCompleteStatusColumnDiscipline => 'Disiplin';

  @override
  String get rfiCompleteStatusColumnRfiNo => 'RFI No.';

  @override
  String get rfiCompleteStatusColumnPdfPath => 'Tamamlanmış PDF';

  @override
  String get rfiCompleteStatusColumnPdfCount => 'PDF sayısı';

  @override
  String get rfiCompleteStatusColumnCurrentStatus => 'Mevcut durum';

  @override
  String get rfiCompleteStatusColumnProposedStatus => 'Yeni durum';

  @override
  String get rfiCompleteStatusColumnDisposition => 'Karar';

  @override
  String get rfiCompleteStatusColumnAction => 'İşlem';

  @override
  String get rfiCompleteStatusColumnPdfPaths => 'Eşleşen bütün PDF\'ler';

  @override
  String get rfiCompleteStatusColumnReason => 'Kanıt / Detaylar';

  @override
  String get rfiCompleteStatusDispositionReady => 'Hazır';

  @override
  String get rfiCompleteStatusDispositionAlreadyCompleted => 'Zaten Completed';

  @override
  String get rfiCompleteStatusDispositionManualReview => 'Manuel inceleme';

  @override
  String get progressPhaseCompletionPdfDiscovery =>
      'Bütün RFI Sistem klasörlerinde tamamlanmış PDF\'ler taranıyor';

  @override
  String get progressPhaseCompletionWorkbookRead =>
      'RFI Log durum hücreleri okunuyor';

  @override
  String get progressPhaseCompletionMapping =>
      'Tamamlanmış PDF\'ler RFI Log satırlarıyla eşleştiriliyor';

  @override
  String get opFilesReplicaPublishTitle => 'RFI Sistemlerini Yayınla';

  @override
  String get opFilesReplicaPublishDesc =>
      'Seçilen kaynak RFI Sistemini iki hedefe yedekli, hash doğrulamalı ve geri alınabilir biçimde yayınlar.';

  @override
  String get paramReplicaRootA => 'Sistem A klasörü';

  @override
  String get paramReplicaRootAHelp =>
      'SharePoint, ağ veya yerel RFI Sistem köklerinden biri.';

  @override
  String get paramReplicaRootB => 'Sistem B klasörü';

  @override
  String get paramReplicaRootBHelp => 'İkinci RFI Sistem kökü.';

  @override
  String get paramReplicaRootC => 'Sistem C klasörü';

  @override
  String get paramReplicaRootCHelp => 'Üçüncü RFI Sistem kökü.';

  @override
  String get paramReplicaSourceRoot => 'Yetkili kaynak';

  @override
  String get paramReplicaSourceRootHelp =>
      'Bu sistem diğer iki hedefe yayınlanır. Hedef değişiklikleri kaynağa geri alınmaz.';

  @override
  String get choiceReplicaSourceA => 'Sistem A';

  @override
  String get choiceReplicaSourceB => 'Sistem B';

  @override
  String get choiceReplicaSourceC => 'Sistem C';

  @override
  String get paramReplicaBackupRoot => 'Sürüm yedekleri klasörü';

  @override
  String get paramReplicaBackupRootHelp =>
      'Değiştirilecek her hedef dosya önce ayrı ve kalıcı bir yedeğe kopyalanır.';

  @override
  String get paramReplicaRootFiles => 'Ek kök dosyaları';

  @override
  String get paramReplicaRootFilesHelp =>
      'CIV, MEC, ELE ve PRC dışında eşitlenecek kök dosya adlarını noktalı virgül veya yeni satırla ayırın.';

  @override
  String get paramReplicaUnattended => 'Onaysız otomatik modu etkinleştir';

  @override
  String get paramReplicaUnattendedHelp =>
      'Bu önizlemeyi bir kez uygulamak sabit profil için kalıcı yetki verir. Belirsizlik otomasyonu durdurur.';

  @override
  String get paramReplicaIntervalMinutes => 'Kontrol aralığı (dakika)';

  @override
  String get paramReplicaIntervalMinutesHelp =>
      '1 ile 1440 dakika arasında. DocFlow açıkken çalışır.';

  @override
  String get paramReplicaSettleSeconds => 'Dosya sakinleşme süresi (saniye)';

  @override
  String get paramReplicaSettleSecondsHelp =>
      'Otomatik yayın, en yeni kaynak değişikliğinden sonra bu kadar bekler. 5 ile 3600 arasında.';

  @override
  String get paramReplicaMaxChangedFiles =>
      'Otomatik çalışmada en fazla dosya etkisi';

  @override
  String get paramReplicaMaxChangedFilesHelp =>
      'Bir otomatik çalışmada daha fazla hedef dosya değişecekse sistem duraklatılır.';

  @override
  String get paramReplicaMaxChangedMib => 'Otomatik çalışmada en fazla MiB';

  @override
  String get paramReplicaMaxChangedMibHelp =>
      'Bir otomatik çalışmanın toplam yayın boyutu bu sınırı aşarsa sistem duraklatılır.';

  @override
  String get replicaEditorTitle => 'Kesin yayın etkilerini incele';

  @override
  String replicaEditorRowCount(Object count) {
    return '$count dosya etkisi';
  }

  @override
  String get replicaColumnTarget => 'Hedef';

  @override
  String get replicaColumnPath => 'Göreli yol';

  @override
  String get replicaColumnAction => 'İşlem';

  @override
  String get replicaColumnBeforeHash => 'Önceki hash';

  @override
  String get replicaColumnAfterHash => 'Kaynak hash';

  @override
  String get replicaActionCopy => 'Yeni dosyayı kopyala';

  @override
  String get replicaActionReplace => 'Yedekle ve değiştir';

  @override
  String get noteReplicaMkdir => 'Gerekli klasörü oluştur';

  @override
  String get noteReplicaCopy => 'Eksik hedef dosyayı kopyala';

  @override
  String get noteReplicaReplace =>
      'Hedefi yedekle, doğrula ve kaynaktan değiştir';

  @override
  String get noteReplicaRestore => 'Hedefi yayın öncesi yedekten geri yükle';

  @override
  String get warnReplicaTargetExtra =>
      'Hedefte kaynakta olmayan dosya var; silinmedi.';

  @override
  String get warnReplicaTargetExtraOverflow =>
      'Listelenemeyen ek hedef dosyaları da var; hiçbiri silinmedi.';

  @override
  String get warnReplicaNoChanges =>
      'Üç sistem zaten seçilen kaynakla eşleşiyor.';

  @override
  String get errorReplicaRootInvalid => 'Üç okunabilir RFI Sistem kökü seçin.';

  @override
  String get errorReplicaBackupInvalid =>
      'Okunabilir ve ayrı bir yedek klasörü seçin.';

  @override
  String get errorReplicaSourceInvalid => 'Yetkili kaynak seçimi geçersiz.';

  @override
  String get errorReplicaRootsOverlap =>
      'RFI Sistem kökleri aynı veya iç içe olamaz.';

  @override
  String get errorReplicaBackupOverlap =>
      'Yedek klasörü hiçbir RFI Sistem köküyle aynı veya iç içe olamaz.';

  @override
  String get errorReplicaRootFileInvalid => 'Ek kök dosya adı geçersiz.';

  @override
  String get errorReplicaSourceRootFileMissing =>
      'Seçilen ek kök dosyası kaynak sistemde yok.';

  @override
  String get errorReplicaTooManyRootFiles =>
      'En fazla 64 ek kök dosyası seçilebilir.';

  @override
  String get errorReplicaIntervalInvalid =>
      'Kontrol aralığı 1 ile 1440 dakika arasında olmalıdır.';

  @override
  String get errorReplicaSettleInvalid =>
      'Dosya sakinleşme süresi 5 ile 3600 saniye arasında olmalıdır.';

  @override
  String get errorReplicaChangeLimitInvalid =>
      'Otomatik değişiklik sınırları sıfırdan büyük ve desteklenen aralıkta olmalıdır.';

  @override
  String get errorReplicaSourceSettling =>
      'Kaynakta yeni değişmiş dosya var; otomatik yayın dosya sakinleşene kadar ertelendi.';

  @override
  String get errorReplicaChangeLimitExceeded =>
      'Otomatik yayın onaylanan dosya veya boyut sınırını aşıyor; güvenlik için duraklatıldı.';

  @override
  String get errorReplicaDisciplineMissing =>
      'Kaynakta CIV, MEC, ELE ve PRC klasörlerinin tamamı bulunmalıdır.';

  @override
  String get errorReplicaScanIncomplete =>
      'Klasör taraması tamamlanamadı; hiçbir dosya yayınlanmadı.';

  @override
  String get errorReplicaEntryLimit =>
      'RFI Sistemi 500000 kayıt güvenlik sınırını aşıyor.';

  @override
  String get errorReplicaSymlinkRefused =>
      'Sembolik bağlantı bulundu; güvenli yayın durduruldu.';

  @override
  String get errorReplicaPathCollision =>
      'Aynı göreli yola çözülen birden fazla dosya bulundu.';

  @override
  String get errorReplicaFileUnstable =>
      'Bir dosya tarama sırasında değişti; yeniden önizleyin.';

  @override
  String get errorReplicaApprovedStepInvalid =>
      'Onaylanan yayın adımı geçersiz; yeniden önizleyin.';

  @override
  String get errorReplicaSourceChanged =>
      'Kaynak dosya önizlemeden sonra değişti; yayın durduruldu.';

  @override
  String get errorReplicaTargetChanged =>
      'Hedef dosya önizlemeden sonra değişti; yayın durduruldu.';

  @override
  String get errorReplicaBackupExists =>
      'Bu yayın yedeği zaten var; üzerine yazılmadı.';

  @override
  String get errorReplicaBackupVerificationFailed =>
      'Yedek hash doğrulaması başarısız; otomasyon durduruldu.';

  @override
  String get errorReplicaCopyVerificationFailed =>
      'Geçici kopya hash doğrulaması başarısız.';

  @override
  String get errorReplicaPublishVerificationFailed =>
      'Yayın sonrası hash doğrulaması başarısız; recovery gerekli.';

  @override
  String get errorReplicaProfileMissing =>
      'Etkinleştirilecek onaylı otomatik sync profili yok.';

  @override
  String get errorReplicaProfileInvalid =>
      'Kaydedilmiş otomatik sync profili geçersiz.';

  @override
  String get errorReplicaActivationFailed =>
      'Dosyalar yayınlandı ancak otomatik profil etkinleştirilemedi.';

  @override
  String get errorReplicaAutomaticApplyFailed =>
      'Otomatik yayın tamamlanamadı ve duraklatıldı.';

  @override
  String get progressPhaseReplicaDiscovery => 'Üç RFI Sistemi taranıyor';

  @override
  String get progressPhaseReplicaSourceHash =>
      'Kaynak dosyalar çok çekirdekli hash kontrolünden geçiyor';

  @override
  String get progressPhaseReplicaTargetHash =>
      'Hedef dosyalar çok çekirdekli hash kontrolünden geçiyor';

  @override
  String get prefsAutomation => 'Windows ve otomasyon';

  @override
  String get prefsAutomationHelp =>
      'Başlangıç davranışını ve daha önce onaylanmış arka plan sync profilini yönetin.';

  @override
  String get prefsStartWithWindows => 'Windows açılışında DocFlow\'u başlat';

  @override
  String get prefsReplicaSyncEnabled => 'Onaysız RFI Sistem sync aktif';

  @override
  String get prefsReplicaSyncEnabledHelp =>
      'Hata veya belirsizlik bu anahtarı otomatik kapatır. Yedekler otomatik silinmez.';

  @override
  String get prefsReplicaStatusUnconfigured =>
      'Durum: Henüz onaylanmış profil yok';

  @override
  String get prefsReplicaStatusDisabled => 'Durum: Kapalı';

  @override
  String get prefsReplicaStatusIdle => 'Durum: Sonraki çalışma bekleniyor';

  @override
  String get prefsReplicaStatusRunning => 'Durum: Çalışıyor';

  @override
  String get prefsReplicaStatusWaitingForSignIn =>
      'Durum: Oturum açılması bekleniyor';

  @override
  String get prefsReplicaStatusDeferred =>
      'Durum: Başka işlem bitene kadar ertelendi';

  @override
  String get prefsReplicaStatusPaused =>
      'Durum: Güvenlik nedeniyle duraklatıldı';

  @override
  String get paramRfiProcessScansCorrectOrientation => 'Sayfa dönüşünü düzelt';

  @override
  String get paramRfiProcessScansCorrectOrientationHelp =>
      'Etkinleştirildiğinde yalnızca yönü güvenle belirlenen sayfalar döndürülür. Herhangi bir sayfa belirsizse PDF\'nin tamamı için düzeltme atlanır ve dosya olduğu gibi _review\'a gönderilir.';

  @override
  String get warnRfiProcessScansOrientationReview =>
      'Sayfa yönü güvenle belirlenemedi; PDF _review alanına alındı';

  @override
  String get progressPhaseScanOrientation => 'PDF sayfa yönleri denetleniyor';

  @override
  String get rfiExtractionPages => 'Çıkarılan sayfalar';

  @override
  String get reviewSelectAllKeep => 'Tümünü Bırak';

  @override
  String get reviewSelectAllPut => 'Uygun Olanları Taşı';

  @override
  String get reviewSelectAllTrash => 'Tümünü Çöpe Taşı';

  @override
  String get opsSearchPlaceholder => 'İşlem ara…';

  @override
  String get opsCategoryAll => 'Tümü';

  @override
  String get opsCategoryScans => 'Taramalar ve PDF';

  @override
  String get opsCategoryExcel => 'Excel ve Günlükler';

  @override
  String get opsCategoryMail => 'Outlook ve E-posta';

  @override
  String get opsCategorySystem => 'Yayınlama ve Yazdırma';

  @override
  String opsEmptySearch(Object query) {
    return '\'$query\' ile eşleşen işlem bulunamadı';
  }

  @override
  String get actionOpenDestination => 'Hedef Klasörü Aç';

  @override
  String get opRfiGenerateConcreteDocumentsTitle => 'Beton Belgeleri Oluştur';

  @override
  String get opRfiGenerateConcreteDocumentsDesc =>
      'Seçilen RFI türleri ve şablonlarla RFI Log ve Follow Up çalışma kitaplarından toplu beton belgesi oluşturur.';

  @override
  String get paramRfiDocumentsRfiLog => 'RFI Log çalışma kitabı';

  @override
  String get paramRfiDocumentsFollowUp => 'Follow Up çalışma kitabı';

  @override
  String get paramRfiDocumentsTemplate010 =>
      '010 şablonu — Follow Up alanları; ortam = santral taze sıcaklığı eksi rastgele 0,5–0,7';

  @override
  String get paramRfiDocumentsTemplate011 =>
      '011 şablonu — rapor/konum ve ayarlanabilir rastgele ortam aralığı (varsayılan 19–24)';

  @override
  String get paramRfiDocumentsTemplate013 =>
      '013 şablonu — yalnızca rapor/konum';

  @override
  String get paramRfiDocumentsTemplate014 =>
      '014 şablonu — yalnızca rapor/konum';

  @override
  String get paramRfiDocumentsOutput => 'Çıktı klasörü';

  @override
  String get paramRfiDocumentsRfis010 => '010 şablonu için RFI numaraları';

  @override
  String get paramRfiDocumentsRfis011 => '011 şablonu için RFI numaraları';

  @override
  String get paramRfiDocumentsRfis013 => '013 şablonu için RFI numaraları';

  @override
  String get paramRfiDocumentsRfis014 => '014 şablonu için RFI numaraları';

  @override
  String get paramRfiDocumentsWaterRatios =>
      'İsteğe bağlı RFI bazında su/çimento oranları (yoksa boş bırakılır)';

  @override
  String get paramRfiDocumentsAmbientMin => 'En düşük ortam sıcaklığı';

  @override
  String get paramRfiDocumentsAmbientMax => 'En yüksek ortam sıcaklığı';

  @override
  String get paramRfiDocumentsAmbientGroups =>
      'RFI\'ye özel ortam sıcaklığı aralıkları';

  @override
  String get errorParamRequiredOutputDirectory => 'Çıktı klasörü gerekli';

  @override
  String get errorRfiDocumentsInvalidInput =>
      'Beton belgesi girdileri geçersiz';

  @override
  String get warnRfiDocumentsOutputExists =>
      'Çıktı dosyası zaten var; üzerine yazılmayacak';

  @override
  String get warnRfiDocumentsFollowUpMissing =>
      'Bu RFI için Concrete Specimen Test Follow-Up satırı bulunamadı. Bu RFI atlanacak. Uygula yalnızca mevcut belgeleri oluşturur; çalışma kitabını düzeltip yeniden önizlemek için İptal\'i seçin.';

  @override
  String get warnRfiDocumentsFormNotRequired =>
      'RFI Log Forms alanında bu RFI için CON-CIV-010 listelenmiyor; bu nedenle CON-CIV-010 oluşturulmayacak. İstenen belge türünü veya RFI Log\'u kontrol edip yeniden önizleyin.';

  @override
  String get paramRfiDocumentsRfiLogHelp =>
      'Civil sayfası, istenen her RFI numarasını ve konumunu sağlar.';

  @override
  String get paramRfiDocumentsFollowUpHelp =>
      'Yalnızca CON-CIV-010 oluşturulurken gerekir; beton döküm alanlarını sağlar.';

  @override
  String get paramRfiDocumentsOutputHelp =>
      'Oluşturulan dosyalar buraya yazılır; mevcut dosyaların üzerine yazılmaz.';

  @override
  String get paramRfiDocumentsRfisHelp =>
      'Tam RFI numaralarını veya CIV seri numaralarını virgülle ya da yeni satırlarla girin.';

  @override
  String get paramRfiDocumentsWaterRatiosHelp =>
      'İsteğe bağlı olarak her satıra RFI numarası = oran girin. Yazılmayan RFI\'nin hücresi boş kalır.';

  @override
  String get paramRfiDocumentsAmbientGroupsHelp =>
      'İsteğe bağlı; her grup için bir satır: RFI[, RFI...]=MIN-MAX. Aralık uçları dahildir; listelenmeyen RFI\'ler varsayılan En düşük/En yüksek ortam sıcaklıklarını kullanır.';

  @override
  String get stepRfiGeneratedConcreteDocument => 'Beton belgesi oluştur';
}
