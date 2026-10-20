// ignore: unused_import
import 'package:intl/intl.dart' as intl;

import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for Azerbaijani (`az`).
class AppLocalizationsAz extends AppLocalizations {
  AppLocalizationsAz([String locale = 'az']) : super(locale);

  @override
  String get appTitle => 'DocFlow';

  @override
  String get actionPreview => 'Öncədən bax';

  @override
  String get actionApply => 'Tətbiq et';

  @override
  String get actionUndo => 'Geri qaytar';

  @override
  String get actionCancel => 'Ləğv et';

  @override
  String get actionBack => 'Geri';

  @override
  String get actionBrowse => 'Bax';

  @override
  String get uiWarnings => 'Xəbərdarlıqlar';

  @override
  String resultApplied(int count) {
    return '$count tətbiq edildi';
  }

  @override
  String resultFailed(int count) {
    return '$count uğursuz';
  }

  @override
  String get actionOpen => 'Aç';

  @override
  String get actionReview => 'Yoxlama qovluğu';

  @override
  String get reviewTitle => 'Faylları yoxla';

  @override
  String get reviewEmpty => 'Yoxlama qovluğu boşdur';

  @override
  String get reviewKeep => 'Olduğu kimi saxla';

  @override
  String get reviewPut => 'Hədəf qovluğa köçür';

  @override
  String get reviewDelete => 'Zibil qutusuna köçür';

  @override
  String get reviewNoCode => 'RFI kodu yoxdur';

  @override
  String get reviewWillReplace => 'mövcud faylı əvəz edir';

  @override
  String get noteDeleted => 'Silindi (səbətə)';

  @override
  String get noteDisplaced => 'Əvəz edildi — səbətə köçürüldü';

  @override
  String get actionHistory => 'Tarixçə';

  @override
  String get historyTitle => 'Tarixçə';

  @override
  String get historyUnknownOperation => 'Naməlum əməliyyat';

  @override
  String get historyEmpty => 'Hələ tarixçə yoxdur';

  @override
  String historyMoves(int count) {
    return '$count fayl';
  }

  @override
  String get historyUndoOriginal => 'Orijinal yerə geri qaytar';

  @override
  String get historyUndoToFolder => 'Bir qovluğa geri qaytar…';

  @override
  String get planEmpty => 'Ediləcək bir şey yoxdur';

  @override
  String planSummary(int count) {
    return '$count fayl emal ediləcək';
  }

  @override
  String get planImpactTitle => 'Tətbiqdən əvvəl';

  @override
  String get planImpactPreviewOnly =>
      'Hələ heç nə dəyişməyib. Tətbiq yalnız bu təsdiqlənmiş önizləməni icra edir.';

  @override
  String get planImpactOutputs => 'Yeni çıxışlar';

  @override
  String get planImpactUpdates => 'Yenilənəcək fayllar';

  @override
  String get planImpactMoves => 'Köçürmə / ad dəyişmə';

  @override
  String get planImpactPrintJobs => 'Çap işləri';

  @override
  String get planImpactDrafts => 'Açılacaq qaralamalar';

  @override
  String get planImpactCleanup => 'Təmizləmə';

  @override
  String get planImpactWarnings => 'Xəbərdarlıqlar';

  @override
  String get scanIndexTitle => 'Artımlı skan indeksi';

  @override
  String get scanIndexHelp =>
      'Dəyişməyən fayllar qalıcı yerli indeksdən yenidən istifadə edildi.';

  @override
  String get scanIndexForceApplied =>
      'Bu əməliyyat üçün tam skan xüsusi olaraq seçildi.';

  @override
  String get scanIndexDiscovered => 'Tapılan';

  @override
  String get scanIndexReused => 'İndeksdən';

  @override
  String get scanIndexAnalyzed => 'İndi skan edilən';

  @override
  String get scanIndexFailed => 'Uğursuz';

  @override
  String get scanIndexInterrupted => 'Əvvəlki yarımçıq qalanlar';

  @override
  String get planWarningsSearch => 'Xəbərdarlıqlarda axtar';

  @override
  String get planWarningsType => 'Xəbərdarlıq növü';

  @override
  String get planWarningsAll => 'Bütün xəbərdarlıq növləri';

  @override
  String get planWarningsFocus => 'Əlaqəli səhifəni göstər';

  @override
  String get planWarningsNone => 'Bu filtrlərə uyğun xəbərdarlıq yoxdur.';

  @override
  String planWarningsShowing(Object count, Object shown) {
    return '$count xəbərdarlıqdan $shown dənəsi göstərilir';
  }

  @override
  String get planWorkbookChangesTitle => 'Dəqiq Excel xana dəyişiklikləri';

  @override
  String planWorkbookChangesCount(Object count) {
    return '$count təsdiqlənmiş xana dəyişikliyi';
  }

  @override
  String get planWorkbookChangesHelp =>
      'Cari dəyəri Tətbiq et əməliyyatının yazacağı dəqiq dəyərlə müqayisə etmək üçün açın.';

  @override
  String get planWorkbookChangesWorkbook => 'İş kitabı';

  @override
  String get planWorkbookChangesSheetCell => 'Səhifə və xana';

  @override
  String get planWorkbookChangesRfi => 'RFI';

  @override
  String get planWorkbookChangesBefore => 'Cari';

  @override
  String get planWorkbookChangesAfter => 'Tətbiqdən sonra';

  @override
  String get planWorkbookChangesBlank => '(boş)';

  @override
  String get planWorkbookChangesKindRfiLog => 'RFI Log';

  @override
  String get planWorkbookChangesKindFollowUp => 'Follow Up';

  @override
  String get planWorkbookChangesKindWorkbook => 'İş kitabı';

  @override
  String planWorkbookChangesLimit(Object count) {
    return 'Önizləmə ilk $count təsdiqlənmiş xana dəyişikliyi ilə məhdudlaşır.';
  }

  @override
  String planExactTitle(Object count) {
    return '$count təsdiqlənmiş dəqiq təsir';
  }

  @override
  String get planExactHelp => 'Tam mənbə və hədəf yollarını yoxlamaq üçün açın';

  @override
  String get opRfiProcessScansTitle => 'Skanları emal et';

  @override
  String get opRfiProcessScansFastCoversTitle => 'RFI-ni sürətli skan et';

  @override
  String get opRfiProcessScansFastCoversDesc =>
      'Çoxsəhifəli skanlarda RFI üz qabıqlarını tapır. Birləşdirilmiş skanı ayırmaq üçün Çoxlu RFI, yalnız ilk səhifəni oxuyub tam PDF-ni saxlamaq üçün Tək RFI rejimini seçin. OCR ilə oxunan CLV CIV olaraq düzəldilir. Qeyri-müəyyən skanlar _review qovluğuna göndərilir.';

  @override
  String get paramRfiProcessScansScanMode => 'Skan rejimi';

  @override
  String get paramRfiProcessScansScanModeHelp =>
      'Çoxlu RFI birləşdirilmiş skanı ayırır. Tək RFI yalnız ilk səhifəni oxuyur və tam PDF-ni olduğu kimi saxlayır.';

  @override
  String get choiceRfiProcessScansScanModeMultiRfi => 'Bir neçə RFI-ni ayır';

  @override
  String get choiceRfiProcessScansScanModeSingleRfi =>
      'Tək RFI — yalnız ilk səhifəni oxu';

  @override
  String get rfiFastScanEditorTitle => 'RFI nömrələrini yoxla və düzəlt';

  @override
  String rfiFastScanEditorRowCount(int count) {
    return '$count skan faylı';
  }

  @override
  String get rfiFastScanColumnSourceFile => 'Mənbə faylı';

  @override
  String get rfiFastScanColumnEvidence => 'Tanıma';

  @override
  String get rfiFastScanColumnDetectedCode => 'Aşkarlanan RFI';

  @override
  String get rfiFastScanColumnCode => 'RFI nömrəsi';

  @override
  String get rfiFastScanColumnTarget => 'Hədəf';

  @override
  String get rfiFastScanEvidenceClvToCiv => 'OCR uyğunlaşdırması: CLV → CIV';

  @override
  String get rfiFastScanEvidenceConfirmed => 'Təsdiqlənib';

  @override
  String get rfiFastScanEditorUpdatePreview => 'Önizləməni yenilə';

  @override
  String get rfiFastScanEditorUpdatingPreview => 'Önizləmə yenilənir…';

  @override
  String get rfiFastScanEditorPreviewUpdated =>
      'Önizləmə yeniləndi. Tətbiq etməzdən əvvəl hədəf yollarını yoxlayın.';

  @override
  String get errorRfiFastScanCodeInvalid =>
      'Dəstəklənən tam RFI kodunu 1–4 ASCII rəqəmli sonluqla daxil edin; kod dörd rəqəmlə saxlanılır.';

  @override
  String get errorRfiFastScanTargetExists =>
      'Başqa skan artıq bu RFI nömrəsindən istifadə edir və ya hədəf mövcuddur.';

  @override
  String get errorRfiFastScanRevisionInvalid =>
      'Düzəliş edilmiş skan önizləməsi yanlışdır. RFI nömrələrini yoxlayıb yenidən cəhd edin.';

  @override
  String get opRfiGroupFilesTitle => 'RFI fayllarını qrupla';

  @override
  String get opRfiGroupFilesDesc =>
      'Fayl adında və məzmununda dəqiq RFI nömrələrini tapır, təsdiqlənmiş faylları <ixtisas>/<tam RFI kodu> qovluqlarına kopyalayır.';

  @override
  String get paramRfiFileGroupSource => 'Gələn fayllar qovluğu';

  @override
  String get paramRfiFileGroupSourceHelp =>
      'Yalnız üst səviyyədəki fayllar skan edilir; alt qovluqlar dəyişdirilmədən ötürülür.';

  @override
  String get paramRfiFileGroupDestination => 'Qruplaşdırılmış çıxış qovluğu';

  @override
  String get paramRfiFileGroupDestinationHelp =>
      'Mənbədən kənarda boş və ya ayrı hədəf seçin. Mövcud faylların üzərinə yazılmır.';

  @override
  String get rfiFileGroupEditorTitle => 'Tapılmış RFI uyğunluqları';

  @override
  String rfiFileGroupEditorRowCount(Object count) {
    return '$count kopyalama uyğunluğu';
  }

  @override
  String get rfiFileGroupColumnEnabled => 'Kopyala';

  @override
  String get rfiFileGroupColumnRfiCode => 'RFI nömrəsi';

  @override
  String get rfiFileGroupColumnSourceFile => 'Mənbə faylı';

  @override
  String get rfiFileGroupColumnMethod => 'Tapılma üsulu';

  @override
  String get rfiFileGroupMethodFilename => 'Fayl adı';

  @override
  String get rfiFileGroupMethodPdf => 'PDF / OCR';

  @override
  String get rfiFileGroupMethodMail => 'Poçt məzmunu';

  @override
  String get rfiFileGroupMethodOffice => 'Office məzmunu';

  @override
  String get rfiFileGroupMethodContent => 'Fayl məzmunu';

  @override
  String get noteRfiFileGroupFolderCreated => 'RFI qovluğu yaradılacaq';

  @override
  String get noteRfiFileGroupCopy =>
      'Fayl ixtisas və tam RFI kodu qovluğuna kopyalanacaq';

  @override
  String get noteRfiFileGroupReviewCopy =>
      'Fayl yoxlama qovluğuna kopyalanacaq';

  @override
  String get warnRfiFileGroupNoCode =>
      'RFI nömrəsi tapılmadı; fayl yoxlamaya ayrılacaq';

  @override
  String get warnRfiFileGroupUnreadable =>
      'Fayl tam oxunmadı; yoxlamaya ayrılacaq';

  @override
  String get warnRfiFileGroupPartialInspection =>
      'Fayl adındakı uyğunluq istifadə edilə bilər, lakin məzmun tam yoxlanmadı';

  @override
  String get warnRfiFileGroupExists =>
      'Hədəf fayl artıq mövcuddur; üzərinə yazılmayacaq';

  @override
  String get warnRfiFileGroupNoFiles =>
      'Mənbə qovluğunda üst səviyyəli fayl tapılmadı';

  @override
  String get warnRfiFileGroupMissingEmail =>
      'Bu RFI üçün EML və ya MSG e-poçt faylı tapılmadı; əlaqəli fayllar yerləşdirilməyəcək';

  @override
  String get errorParamRequiredDestination => 'Hədəf qovluğu tələb olunur';

  @override
  String get paramSource => 'Mənbə qovluğu';

  @override
  String get paramSourceHelp => 'Skan edilmiş PDF-lərin olduğu qovluq';

  @override
  String get paramDest => 'Hədəf qovluğu';

  @override
  String get paramDestHelp => 'Emal edilmiş RFI PDF-lərinin saxlanacağı yer';

  @override
  String get paramKeepOriginal =>
      'Orijinalları saxla (köçürmək əvəzinə kopyala)';

  @override
  String get noteRenamed => 'Adlandırıldı';

  @override
  String get noteNeedsReview => 'Yoxlamaya köçürüləcək';

  @override
  String get noteUnreadable => 'Oxunmur — yoxlamaya köçürüləcək';

  @override
  String get warnNoCode => 'RFI nömrəsi tapılmadı';

  @override
  String get warnLowConfidence => 'Aşağı etibarlılıq — yoxlama lazımdır';

  @override
  String get warnUnknownRfiFolder =>
      'Aşkarlanan RFI qovluğu mövcud deyil — yoxlamaya köçürüldü';

  @override
  String get warnUnreadable => 'Fayl oxuna bilmədi';

  @override
  String get warnRevisionGuessed => 'Reviziya nömrəsi təxmin edildi';

  @override
  String get warnDuplicate => 'Təkrarlanan RFI nömrəsi — yoxlamaya köçürüldü';

  @override
  String get warnExists =>
      'Hədəf artıq mövcuddur — üzərinə yazılmadı, yoxlamaya köçürüldü';

  @override
  String get warnNoPdfs => 'Qovluqda PDF yoxdur';

  @override
  String get errorStepFailed => 'Addım uğursuz oldu';

  @override
  String get errorUndoFailed => 'Geri qaytarma uğursuz oldu';

  @override
  String get errorParamRequiredSource => 'Mənbə qovluğu mütləqdir';

  @override
  String get errorParamRequiredDest => 'Hədəf qovluğu mütləqdir';

  @override
  String get actionAdmin => 'İdarəetmə paneli';

  @override
  String get actionRetry => 'Yenidən yoxla';

  @override
  String get bootstrapFatalTitle => 'DocFlow başladıla bilmədi';

  @override
  String get bootstrapRetry => 'Yenidən cəhd et';

  @override
  String get authEmail => 'E-poçt';

  @override
  String get authPassword => 'Şifrə';

  @override
  String get authSignIn => 'Daxil ol';

  @override
  String get authSignUp => 'Hesab yarat';

  @override
  String get authSignOut => 'Çıxış';

  @override
  String get authSignupOk =>
      'Hesab yaradıldı. Administratordan təsdiqləməsini xahiş edin, sonra daxil olun.';

  @override
  String get authPending => 'Təsdiq gözlənilir';

  @override
  String get authPendingHelp =>
      'Hesabınız var, lakin hələ təsdiqlənməyib. Administrator giriş icazəsi verməlidir.';

  @override
  String get adminTitle => 'İstifadəçilər';

  @override
  String get adminAllowed => 'Giriş';

  @override
  String get adminAdmin => 'Administrator';

  @override
  String get adminEmpty => 'Hələ istifadəçi yoxdur';

  @override
  String get actionDone => 'Hazırdır';

  @override
  String get adminHelp =>
      'Kiminsə tətbiqdən istifadə etməsi üçün Giriş-i aktiv edin. Öz açarlarınız kilidlidir — özünüzü kənarda qoya bilməzsiniz.';

  @override
  String get adminNoAccess => 'giriş yoxdur';

  @override
  String get adminYou => 'siz';

  @override
  String get authSubtitle => 'Davam etmək üçün daxil olun';

  @override
  String get authShow => 'Şifrəni göstər';

  @override
  String get authHide => 'Şifrəni gizlət';

  @override
  String get authPasswordHint => 'Ən azı 6 simvol';

  @override
  String get historyEmptyHelp =>
      'Tətbiq etdiyiniz əməliyyatlar burada görünəcək və geri qaytara biləcəksiniz.';

  @override
  String get historyUndone => 'Geri qaytarıldı';

  @override
  String get historyLocalJournalRequired =>
      'Geri qaytarmaq üçün bu cihazdakı yerli jurnal tələb olunur.';

  @override
  String get historyCloud => 'Bulud';

  @override
  String get historyLocal => 'Yerli';

  @override
  String get historySourceCloud =>
      'Paylaşılan tarixçə, bütün kompüterlərinizdə görünür';

  @override
  String get historySourceLocal => 'Yalnız yerli tarixçə — buluda çıxış yoxdur';

  @override
  String get opsHeading => 'Əməliyyatlar';

  @override
  String get opsSubheading =>
      'Bir əməliyyat seçin. Siz yoxlayıb təsdiqləməyincə heç bir faylınız dəyişdirilmir.';

  @override
  String get homeTitle => 'İş sahəsi';

  @override
  String get homeSubtitle =>
      'Sənəd əməliyyatları və bütün köməkçi alətlər bir yerdə.';

  @override
  String get homePreviewFirst => 'Öncə baxış';

  @override
  String get ocrConfidence => 'OCR sübut balı';

  @override
  String get ocrConfidenceShort => 'Sübut';

  @override
  String get ocrConfidenceHelp =>
      '100 üzərindən evristik OCR uyğunluq balı. Dəqiqlik ehtimalı, qəti təsdiq və ya avtomatik təsdiq deyil.';

  @override
  String get homeFeatures => 'Digər funksiyalar';

  @override
  String get homeFeaturesHelp => 'Baxış, tarixçə, hesab və tətbiq idarəetməsi.';

  @override
  String get homeReviewDesc =>
      'Gözləyən faylları ayrı-ayrılıqda yoxlayın və təhlükəsiz yerləşdirin.';

  @override
  String get homeHistoryDesc =>
      'Tamamlanmış əməliyyatlara baxın və təhlükəsiz geri qaytarın.';

  @override
  String get homeSettingsDesc =>
      'Dil, görünüş, mətn və hesab seçimlərinizi idarə edin.';

  @override
  String get homeAdminDesc =>
      'İstifadəçi girişini və administrator icazələrini idarə edin.';

  @override
  String get paramHintFolder => 'Qovluq seçin';

  @override
  String get paramHintFile => 'Fayl seçin';

  @override
  String get planApplying => 'Tətbiq olunur';

  @override
  String get planReadOnly => 'Hələ heç nə dəyişməyib';

  @override
  String get prefsTitle => 'Tənzimləmələr';

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
  String get prefsTheme => 'Mövzu';

  @override
  String get prefsThemeSystem => 'Sistem';

  @override
  String get prefsThemeLight => 'İşıqlı';

  @override
  String get prefsThemeDark => 'Qaranlıq';

  @override
  String get prefsFont => 'Şrift';

  @override
  String get prefsFontHelp =>
      'Bu kompüterdə artıq quraşdırılmış şriftlərdən istifadə edir — heç nə yüklənmir.';

  @override
  String get prefsFontSystem => 'Sistem standartı';

  @override
  String get prefsFontSans => 'Sans serif';

  @override
  String get prefsFontSerif => 'Serif';

  @override
  String get prefsFontMono => 'Bərabər aralıqlı';

  @override
  String get prefsFontSize => 'Mətn ölçüsü';

  @override
  String get prefsFontSizeSample =>
      'AZE-SUN-INT-QMS-RFI-CIV-0064 — bu ölçüdə nümunə mətn.';

  @override
  String get prefsDensity => 'Aralıq';

  @override
  String get prefsDensityCompact => 'Yığcam görünüş';

  @override
  String get prefsDensityHelp =>
      'Ekrana daha çox şey sığdırır. Kiçik ekranlarda faydalıdır.';

  @override
  String get prefsSupportTitle => 'Ətraflı dəstək';

  @override
  String get prefsSupportHelp =>
      'Yalnız oxunan, redaktə edilmiş yerli dəstək paketi yaradın.';

  @override
  String get prefsSupportPrivacy =>
      'Versiya, platforma, sidecar/OCR vəziyyəti, son xəta kodları və fayl yolu olmayan son printer növbəsi nəticəsi daxil edilir; sənəd məzmunu, giriş məlumatları, tokenlər və şəxsi yollar çıxarılır. Heç yerə göndərilmir.';

  @override
  String get prefsSupportPreview => 'Dəstək paketinə önbaxış';

  @override
  String get prefsSupportPreviewLabel => 'Dəstək paketinin önbaxışı';

  @override
  String prefsSupportSize(Object count) {
    return '$count bayt';
  }

  @override
  String get prefsSupportCopy => 'Paketi mübadilə buferinə köçür';

  @override
  String get prefsSupportCopied => 'Dəstək paketi mübadilə buferinə köçürüldü.';

  @override
  String get prefsSupportDownload => 'Dəstək paketini yüklə';

  @override
  String get prefsSupportDownloaded => 'Dəstək paketi yükləndi.';

  @override
  String get paramDropFiles =>
      'Faylları buraya sürükləyin və ya əlavə etmək üçün vurun';

  @override
  String get paramDropFolder => 'Qovluğu buraya sürükləyin və ya baxın';

  @override
  String get paramDropFile => 'Faylı buraya sürükləyin və ya baxın';

  @override
  String get actionRecent => 'Son seçimlər';

  @override
  String get actionToggleTheme => 'Mövzunu dəyiş (İşıqlı / Qaranlıq / Sistem)';

  @override
  String get filterSearchPlaceholder =>
      'Axtarış (RFI nömrəsi, status, təsvir)...';

  @override
  String get filterAll => 'Hamısı';

  @override
  String get filterChanges => 'Dəyişikliklər';

  @override
  String get filterManualReview => 'Əl ilə yoxlama';

  @override
  String tableFilterCount(Object filtered, Object total) {
    return '$filtered / $total sətir göstərilir';
  }

  @override
  String get prefsReset => 'Standartlara qaytar';

  @override
  String get reviewHelp =>
      'Hər fayla nə olacağını qərarlaşdırın. Siz planı təsdiqləməyincə heç nə dəyişmir.';

  @override
  String get reviewEmptyHelp => 'Baxış gözləyən fayl yoxdur.';

  @override
  String get reviewNoChanges => 'Heç nə seçilməyib';

  @override
  String reviewSelected(Object count) {
    return '$count seçildi';
  }

  @override
  String reviewToTrash(Object count) {
    return '$count zibilə';
  }

  @override
  String get reviewTrashHint =>
      'Silinən fayllar həmişəlik yox, bərpa oluna bilən zibil qutusuna gedir.';

  @override
  String get stepForm => 'Quraşdır';

  @override
  String get stepPreview => 'Öncədən bax';

  @override
  String get stepApply => 'Tətbiq et';

  @override
  String get actionSave => 'Yadda saxla';

  @override
  String get authUsername => 'İstifadəçi adı';

  @override
  String get authUsernameHelp =>
      '3-24 simvol: hərf, rəqəm, nöqtə, tire, alt xətt';

  @override
  String get authIdentifier => 'İstifadəçi adı və ya e-poçt';

  @override
  String get authIdentifierHelp => 'Hər ikisi işləyir';

  @override
  String get authEmailSignInHelp =>
      'İstifadəçi adı ilə giriş mümkün deyil. Hesabınızın e-poçt ünvanından istifadə edin.';

  @override
  String get authSubtitleUp => 'Başlamaq üçün hesab yaradın';

  @override
  String get authErrCredentials =>
      'İstifadəçi adı/e-poçt və ya şifrə yanlışdır.';

  @override
  String get authErrUsernameTaken => 'Bu istifadəçi adı tutulub.';

  @override
  String get authErrUsernameInvalid =>
      'İstifadəçi adı 3-24 simvol olmalıdır: hərf, rəqəm, nöqtə, tire, alt xətt.';

  @override
  String get authErrEmailTaken => 'Bu e-poçt ilə hesab artıq mövcuddur.';

  @override
  String get prefsAccount => 'Hesab';

  @override
  String get prefsAccountEmailFixed => 'dəyişdirilə bilməz';

  @override
  String get prefsAccountSaved => 'Yadda saxlanıldı';

  @override
  String get authErrEmailNotConfirmed =>
      'Bu hesab mövcuddur, lakin e-poçt hələ təsdiqlənməyib. Administrator e-poçt təsdiqini söndürə bilər və ya poçtunuza gələn keçidi açın.';

  @override
  String get authErrRateLimit =>
      'Çox sayda cəhd. Bir neçə dəqiqə gözləyib yenidən yoxlayın.';

  @override
  String get authErrWeakPassword =>
      'Şifrə çox zəifdir. Ən azı 6 simvol istifadə edin.';

  @override
  String get warnNoOcr =>
      'OCR mühərriki (Tesseract) tapılmadı — heç bir fayla toxunulmadı. Quraşdırın və ya tətbiq paketini yenidən yükləyin.';

  @override
  String get noteFolderCreated => 'Yeni qovluq';

  @override
  String get warnOcrBroken =>
      'OCR bütün fayllarda uğursuz oldu — bu quraşdırma problemidir, fayl problemi deyil. Heç nəyə toxunulmadı. Təfərrüat:';

  @override
  String get opFilesBatchPrintTitle => 'Birdən çox faylı çap et';

  @override
  String get opFilesBatchPrintDesc =>
      'PDF, Office sənədləri və dəstəklənən şəkillərdən sıralı bir yerli çap növbəsi yaradır.';

  @override
  String get paramPrintFiles => 'Çap ediləcək fayllar';

  @override
  String get paramPrintFilesHelp =>
      'Müxtəlif fayl növləri əlavə edin, sonra sıralayın və ya silin. Ekrandakı sıra çap sırasıdır.';

  @override
  String get paramPrintRfiRoot => 'RFI Sistemi qovluğu';

  @override
  String get paramPrintRfiRootHelp =>
      'Mənbə fayllarını dəyişmədən bu qovluğu və alt qovluqları axtarır.';

  @override
  String get paramPrintRfiNumbers => 'RFI nömrələri';

  @override
  String get paramPrintRfiNumbersHelp =>
      'Hər sətrə tam RFI kodu, CIV-1319 kimi ixtisas-nömrə kodu və ya 1–4 rəqəmli nömrə daxil edin; qısa nömrələr dörd rəqəmə tamamlanır.';

  @override
  String get paramPrintIncludeFirstPage => 'First Page sənədləri';

  @override
  String get paramPrintIncludeSketch => 'Sketch sənədləri';

  @override
  String get printRfiTitle => 'RFI sənədlərini tap';

  @override
  String get printRfiInputs => 'Bu RFI sorğularını daxil et';

  @override
  String get printRfiRootRequired => 'Əvvəlcə RFI Sistemi qovluğunu seçin.';

  @override
  String get printRfiInputRequired =>
      'Axtarışdan əvvəl ən azı bir RFI sorğusu seçin.';

  @override
  String get printRfiTypesRequired =>
      'Axtarışdan əvvəl First Page, Sketch və ya hər ikisini seçin.';

  @override
  String get printRfiSearch => 'RFI Sistemində axtar';

  @override
  String get printRfiSearching => 'Axtarılır…';

  @override
  String printRfiFound(Object count) {
    return '$count uyğun sənəd';
  }

  @override
  String get printRfiAmbiguous =>
      'Bu nömrə bir neçə intizamda mövcuddur. İstədiyiniz faylları açıq şəkildə seçin.';

  @override
  String get printRfiMissingFirstPage => 'First Page sənədi tapılmadı.';

  @override
  String get printRfiMissingSketch => 'Sketch sənədi tapılmadı.';

  @override
  String get printRfiToggleRequest => 'Bu RFI-ni dəyiş';

  @override
  String get printRfiSelectTypeFirstPage => 'First Page sənədlərini seç';

  @override
  String get printRfiSelectTypeSketch => 'Sketch sənədlərini seç';

  @override
  String get printRfiClearTypeFirstPage => 'First Page seçimini sil';

  @override
  String printRfiSelectDiscipline(Object discipline) {
    return '$discipline intizamını seç';
  }

  @override
  String get printRfiClearTypeSketch => 'Sketch seçimini sil';

  @override
  String get printRfiTypeFirstPage => 'First Page';

  @override
  String get printRfiTypeSketch => 'Sketch';

  @override
  String get printRfiNoMatches => 'Uyğun sənəd tapılmadı.';

  @override
  String get printRfiTruncated =>
      'Axtarış təhlükəsiz skan həddinə çatdı. Bəzi qovluqlar axtarılmamış ola bilər.';

  @override
  String get actionSelectAll => 'Hamısını seç';

  @override
  String get actionSelectNone => 'Heç birini seçmə';

  @override
  String get errorPrintRfiInvalidRequest =>
      'RFI sorğularını və seçilmiş sənəd növlərini yoxlayın.';

  @override
  String get errorPrintRfiRootEscape =>
      'Tapılmış fayl seçilmiş RFI Sistemindən kənarda dəyişdi. Yenidən axtarın.';

  @override
  String get errorPrintRfiChanged =>
      'Tapılmış fayl axtarış zamanı dəyişdi. Yenidən axtarın.';

  @override
  String get paramPrintPrinter => 'Printer';

  @override
  String get paramPrintPrinterHelp =>
      'Bu kompüterdə quraşdırılmış printerlərdən istifadə edir.';

  @override
  String get paramPrintCopies => 'Nüsxə sayı';

  @override
  String get paramPrintCopiesHelp => 'Növbənin 1-dən 99-a qədər nüsxəsi.';

  @override
  String get paramPrintCollate => 'Nüsxələri dəstlə';

  @override
  String get paramPrintCollateHelp =>
      'Növbəti nüsxəyə keçməzdən əvvəl bütün sıralı dəsti çap edir.';

  @override
  String get errorParamRequiredFiles => 'Ən azı bir fayl seçin';

  @override
  String get errorParamRequiredPrinter => 'Printer seçin';

  @override
  String get errorParamRequiredCopies => 'Nüsxə sayı tələb olunur';

  @override
  String get actionAddFiles => 'Fayl əlavə et';

  @override
  String get actionClear => 'Təmizlə';

  @override
  String get actionMoveUp => 'Yuxarı daşı';

  @override
  String get actionMoveDown => 'Aşağı daşı';

  @override
  String get actionRemove => 'Sil';

  @override
  String get actionDecrease => 'Azalt';

  @override
  String get actionIncrease => 'Artır';

  @override
  String get paramHintFiles => 'Fayl seçilməyib';

  @override
  String printFilesSelected(Object count) {
    return '$count fayl seçildi';
  }

  @override
  String get notePrintQueued => 'Çapa hazırdır';

  @override
  String get warnPrintNoPrinters => 'Printer tapılmadı';

  @override
  String get warnPrintPrinterUnavailable =>
      'Seçilmiş printer artıq mövcud deyil';

  @override
  String get warnPrintInvalidCopies => 'Nüsxə sayı 1 ilə 99 arasında olmalıdır';

  @override
  String get warnPrintTooManyFiles => 'Bir növbədə ən çox 250 fayl ola bilər';

  @override
  String get warnPrintFileMissing => 'Fayl tapılmadı və ötürüləcək';

  @override
  String get warnPrintWorkbookDiscoveryFailed =>
      'İş kitabı təhlükəsiz şəkildə yoxlanıla bilmədi və çap sırasına əlavə edilmədi';

  @override
  String get warnPrintWorkbookTooManyWorksheets =>
      'İş kitabında həddindən artıq iş vərəqi var';

  @override
  String get warnPrintExpandedOrderTooLarge =>
      'Genişləndirilmiş çap sırası ən çox 250 işdən ibarət ola bilər';

  @override
  String get warnPrintWorkbookUnsafeXlm =>
      'İş kitabında Excel 4.0 makro vərəqi olduğu üçün rədd edildi';

  @override
  String get warnPrintWorkbookUnsafeProtection =>
      'İş kitabının strukturu qorunduğu üçün rədd edildi';

  @override
  String get warnPrintWorkbookUnsafeExternalLinks =>
      'İş kitabında xarici bağlantılar olduğu üçün rədd edildi';

  @override
  String get warnPrintWorkbookUnsafeConnections =>
      'İş kitabında xarici məlumat bağlantıları olduğu üçün rədd edildi';

  @override
  String get warnPrintWorkbookUnsafeQueryTables =>
      'İş kitabında sorğu cədvəlləri olduğu üçün rədd edildi';

  @override
  String get warnPrintNoHandler =>
      'Bu fayl növü üçün yerli çap tətbiqi tapılmadı; çap edə bilən standart tətbiq quraşdırın və ya seçin';

  @override
  String get warnPrintUnsupportedOptions =>
      'Seçilmiş printer və ya yerli tətbiq bu çap seçimlərini dəstəkləmir';

  @override
  String get warnPrintNonUndoable =>
      'Çap geri qaytarıla bilməz. Fayllar dəqiq şəkildə bir-bir işləyir; tətbiq təsdiq istəsə növbəti fayl başlamazdan əvvəl növbə gözləyir.';

  @override
  String get errorPrintInvalidPlan =>
      'Çap planı etibarsızdır. Növbəni yenidən önizləyin.';

  @override
  String get errorPrintFailed => 'Çap göndərişi alınmadı';

  @override
  String get errorPrintQueueStopped =>
      'Dəqiq çap sırası qorundu; qalan fayllar göndərilmədi';

  @override
  String get errorPrintPageRangeInvalid =>
      'all və ya 1-3,5 kimi vergüllə ayrılmış səhifə və aralıqlar daxil edin.';

  @override
  String get errorPrintPageRangeBounds =>
      'Seçilmiş səhifə aralığı bu PDF-in səhifə sayını aşır.';

  @override
  String get errorPrintPageRangeImageUnsupported =>
      'Şəkillər yalnız all və ya 1-ci səhifəni qəbul edir.';

  @override
  String get errorPrintPageRangeNativeUnsupported =>
      'Yerli tətbiqlə çap olunan fayllar yalnız bütün səhifələri qəbul edir.';

  @override
  String get errorPrintWorksheetPageRangeAllOnly =>
      'Seçilmiş iş vərəqləri yalnız all dəyərini qəbul edir.';

  @override
  String get errorPrintWorkbookChanged =>
      'İş kitabı ön baxışdan sonra dəyişib. Çap sırasını yenidən önizləyin.';

  @override
  String get errorPrintWorksheetChanged =>
      'İş vərəqi quruluşu ön baxışdan sonra dəyişib. Çap sırasını yenidən önizləyin.';

  @override
  String get errorPrintExcelUnavailable =>
      'Excel iş vərəqinin ixracı bu kompüterdə əlçatan deyil.';

  @override
  String get errorPrintExcelExportTimedOut =>
      'Excel iş vərəqinin ixrac vaxtı çap başlamazdan əvvəl bitdi.';

  @override
  String get errorPrintExcelExportCancelled =>
      'Excel iş vərəqinin ixracı çap başlamazdan əvvəl ləğv edildi.';

  @override
  String get errorPrintExcelExportQuarantined =>
      'Əvvəlki Excel işçisi təhlükəsiz bağlanmadığı üçün DocFlow yenidən başladılanadək Excel ixracı söndürülüb.';

  @override
  String get errorPrintExportCleanupRequiresReview =>
      'Məxfi Excel ixracının təmizlənməsi təsdiqlənmədi. Yenidən cəhd etməzdən əvvəl DocFlow-u yenidən başladın və müvəqqəti ixrac vəziyyətini yoxlayın.';

  @override
  String get errorPrintExcelSourceMotwBlocked =>
      'İş kitabı Windows yükləmə zonası məlumatı təhlükəli, pozulmuş, həddindən böyük və ya oxunmaz olduğu üçün bloklandı.';

  @override
  String get errorPrintExcelWorkbookUnsafe =>
      'İş kitabı təhlükəli qoruma, bağlantı, sorğu və ya Excel 4.0 makro işlənməsi tələb etdiyi üçün açılmadı.';

  @override
  String get errorPrintExcelComContractInvalid =>
      'Təsdiqlənmiş Excel avtomatlaşdırma sorğusu etibarsızdır və iş kitabı açılmadı.';

  @override
  String get errorPrintExcelComFailed =>
      'Excel avtomatlaşdırması çap başlamazdan əvvəl uğursuz oldu.';

  @override
  String get errorPrintExcelExportDestinationExists =>
      'Məxfi iş vərəqi PDF hədəfi artıq mövcuddur, buna görə ixrac dayandırıldı.';

  @override
  String get errorPrintExcelExportFailed =>
      'Excel seçilmiş iş vərəqini PDF-ə ixrac edə bilmədi.';

  @override
  String get errorPrintExcelExportMalformedPdf =>
      'Excel boş və ya etibarsız iş vərəqi PDF-i yaratdı; heç nə çap edilmədi.';

  @override
  String get errorPrintExcelPrinterContextUnavailable =>
      'Windows Excel iş vərəqi üçün təsdiqlənmiş printer kontekstini yarada bilmədi.';

  @override
  String get errorPrintExcelPrintJobRejected =>
      'Windows idarə olunan Excel iş vərəqi çap işini göndərmə başlamazdan əvvəl rədd etdi.';

  @override
  String get errorPrintExcelPrintPreparationFailed =>
      'Windows Excel iş vərəqini idarə olunan çapa hazırlaya bilmədi.';

  @override
  String get errorPrintExcelPrintOptionsRejected =>
      'Windows printeri Excel iş vərəqi üçün təsdiqlənmiş çap parametrlərini rədd etdi.';

  @override
  String get errorPrintExcelPrintCancelledUnconfirmed =>
      'Excel iş vərəqi çap işi növbəyə daxil olduqdan sonra ləğvetmə təsdiqlənmədi. Yenidən cəhd etməzdən əvvəl printeri yoxlayın.';

  @override
  String get errorPrintExcelPrintCompletionUnconfirmed =>
      'Excel iş vərəqi işi çap növbəsinə daxil olduqdan sonra Windows tamamlanmanı təsdiqləyə bilmədi. Yenidən cəhd etməzdən əvvəl printeri yoxlayın.';

  @override
  String get errorPrintExcelControlledPdfUnavailable =>
      'Excel iş vərəqləri üçün idarə olunan PDF çapı bu yığımda mövcud deyil.';

  @override
  String get errorPrintExcelWorkerStartFailed =>
      'Təcrid olunmuş Excel ixrac işçisi başladılmadı.';

  @override
  String get errorPrintExcelDeadlineInvalid =>
      'Excel ixracının vaxt limiti etibarsızdır.';

  @override
  String get errorPrintExcelWorksheetMetadataMissing =>
      'Təsdiqlənmiş iş vərəqi məlumatları yoxdur. Çap sırasına yenidən önbaxış edin.';

  @override
  String get errorPrintExcelSafetyMetadataMissing =>
      'Təsdiqlənmiş iş kitabı təhlükəsizlik məlumatları yoxdur. Çap sırasına yenidən önbaxış edin.';

  @override
  String get errorPrintExcelGenerationExhausted =>
      'DocFlow yenidən başladılana qədər Excel ixracı təhlükəsiz davam edə bilməz.';

  @override
  String get errorPrintExcelWorkerStopped =>
      'Təcrid olunmuş Excel ixrac işçisi gözlənilmədən dayandı.';

  @override
  String get errorPrintExcelResponseStale =>
      'Köhnəlmiş Excel ixrac cavabı çapdan əvvəl rədd edildi.';

  @override
  String get errorPrintExcelStagingUnavailable =>
      'Şəxsi iş kitabı hazırlama sahəsi mövcud deyil.';

  @override
  String get errorPrintExcelSourceNameUnavailable =>
      'Təsdiqlənmiş iş kitabı faylının adı mövcud deyil.';

  @override
  String get errorPrintExcelStagingFailed =>
      'İş kitabı şəxsi hazırlama sahəsinə kopyalana bilmədi.';

  @override
  String get errorPrintExcelStagingVerificationFailed =>
      'Şəxsi iş kitabı nüsxəsi təsdiqlənmiş mənbə ilə doğrulana bilmədi.';

  @override
  String get errorPrintExcelPrivatePdfUnavailable =>
      'Şəxsi iş vərəqi PDF-i mövcud deyil.';

  @override
  String get errorPrintExcelPrivatePdfEmpty =>
      'Excel boş iş vərəqi PDF-i yaratdı; heç nə çap edilmədi.';

  @override
  String get printWaitingForCurrent =>
      'Növbəti fayl başlamazdan əvvəl bu faylın tamamlanması gözlənilir';

  @override
  String get paramPrintPaper => 'Kağız ölçüsü';

  @override
  String get paramPrintPaperHelp =>
      'Bütün fayllar üçün ilkin ayardır; Ön baxışda hər fayl üçün dəyişdirilə bilər.';

  @override
  String get paramPrintOrientation => 'İstiqamət';

  @override
  String get paramPrintScaling => 'Səhifə miqyası';

  @override
  String get paramPrintColor => 'Rəng rejimi';

  @override
  String get paramPrintDuplex => 'İkitərəfli çap';

  @override
  String get paramPrintRetries => 'Avtomatik təkrar cəhd';

  @override
  String get paramPrintRetriesHelp =>
      'Növbə qaydasından əvvəl uğursuz işi 0-3 dəfə yenidən sınayır.';

  @override
  String get paramPrintOnError => 'İş yenə uğursuz olarsa';

  @override
  String get paramPrintOnErrorHelp =>
      'Dayandırmaq sıranı qoruyur; Keçmək davam edir və işi təkrar üçün saxlayır.';

  @override
  String get paramPrintTimeout => 'İş vaxt limiti (dəqiqə)';

  @override
  String get paramPrintTimeoutHelp =>
      'Printer bloklanmış, oflayn və ya cavabsız qalarsa gözləməni dayandırır.';

  @override
  String get choicePrintPaperAuto => 'Printer standartı';

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
  String get choicePrintOrientationAuto => 'Səhifəyə görə avtomatik';

  @override
  String get choicePrintOrientationPortrait => 'Şaquli';

  @override
  String get choicePrintOrientationLandscape => 'Üfüqi';

  @override
  String get choicePrintScalingFit => 'Çap sahəsinə sığdır';

  @override
  String get choicePrintScalingActual => 'Həqiqi ölçü';

  @override
  String get choicePrintScalingShrink => 'Yalnız böyük səhifələri kiçilt';

  @override
  String get choicePrintColorPrinter => 'Printer standartı';

  @override
  String get choicePrintColorColor => 'Rəngli';

  @override
  String get choicePrintColorGrayscale => 'Boz çalar';

  @override
  String get choicePrintDuplexPrinter => 'Printer standartı';

  @override
  String get choicePrintDuplexSimplex => 'Birtərəfli';

  @override
  String get choicePrintDuplexLong => 'İkitərəfli, uzun kənar';

  @override
  String get choicePrintDuplexShort => 'İkitərəfli, qısa kənar';

  @override
  String get choicePrintOnErrorPause => 'Növbəni dayandır';

  @override
  String get choicePrintOnErrorSkip => 'Keç və davam et';

  @override
  String get printEditorTitle => 'Çap növbəsini yoxla və fərdiləşdir';

  @override
  String printEditorRowCount(Object count) {
    return '$count fayl/iş';
  }

  @override
  String get printColInclude => 'Çap et';

  @override
  String get printColStatus => 'Status';

  @override
  String get printColFile => 'Fayl';

  @override
  String get printColFormat => 'Format';

  @override
  String get printColWorksheet => 'İş vərəqi';

  @override
  String get printColWorksheetVisibility => 'Görünmə';

  @override
  String get printWorksheetVisibilityVisible => 'Görünən';

  @override
  String get printWorksheetVisibilityHidden => 'Gizli';

  @override
  String get printWorksheetVisibilityVeryHidden => 'Çox gizli';

  @override
  String get printColPrinter => 'Printer';

  @override
  String get printColPaper => 'Kağız';

  @override
  String get printColOrientation => 'İstiqamət';

  @override
  String get printColScaling => 'Miqyas';

  @override
  String get printColColor => 'Rəng';

  @override
  String get printColDuplex => 'İki tərəf';

  @override
  String get printColCopies => 'Nüsxə';

  @override
  String get printColPageRange => 'Səhifə aralığı';

  @override
  String get printColPageRangeHelp =>
      'all, bir səhifə və ya vergüllə ayrılmış aralıqlardan istifadə edin. Şəkillər all və ya 1-i, yerli tətbiq faylları isə yalnız all dəyərini qəbul edir.';

  @override
  String get printStatusReady => 'Hazır';

  @override
  String get printStatusPending => 'Gözləyir';

  @override
  String get printStatusPrinting => 'Çap olunur';

  @override
  String get printStatusCompleted => 'Tamamlandı';

  @override
  String get printStatusCompletedUnconfirmed => 'Göndərildi; qəbz alınmadı';

  @override
  String get printStatusFailed => 'Uğursuz';

  @override
  String get printStatusCancelled => 'Ləğv edildi';

  @override
  String get printStatusUncertain => 'Printeri yoxla';

  @override
  String get warnPrintSettingsLimited =>
      'Bu format yerli tətbiqi ilə çap edilir; kağız və yerləşdirmə dəstəyi həmin tətbiqdən asılıdır';

  @override
  String get warnPrintNoJobsSelected => 'Çap üçün sətir seçilməyib';

  @override
  String get warnPrintInvalidRetries =>
      'Təkrar cəhd 0 ilə 3 arasında olmalıdır';

  @override
  String get warnPrintInvalidTimeout =>
      'Vaxt limiti 1 ilə 240 dəqiqə arasında olmalıdır';

  @override
  String get warnPrintResumingQueue =>
      'Tamamlanmamış çap növbəsi nəzərdən keçirilir';

  @override
  String get warnPrintCancelled =>
      'Çap növbəsi istifadəçi tərəfindən dayandırıldı';

  @override
  String get warnPrintCancelledUncertain =>
      'Ləğv zamanı sistem cari spool işini hələ tanımamışdı. Yenidən seçməzdən əvvəl printeri yoxlayın.';

  @override
  String get warnPrintStateUncertain =>
      'Sistem bu işin çap olunub-olunmadığını təsdiqləyə bilmədi. Avtomatik təkrar edilmədi; yenidən seçməzdən əvvəl çıxışı yoxlayın.';

  @override
  String get warnPrintSubmittedUnconfirmed =>
      'Windows bu işi qəbul etdi, lakin son təsdiq qeydini saxlamadı. İş yenidən göndərilmədi və növbə növbəti faylla davam etdi.';

  @override
  String get errorPrintQueuePaused =>
      'Növbə dayandırıldı; qalan işlər davam üçün saxlanıldı';

  @override
  String get errorPrintQueueSourceReselectionRequired =>
      'Bu köhnə çap növbəsini təhlükəsiz davam etdirmək olmur. Növbəni silin, sonra faylları yenidən seçin.';

  @override
  String get printResumeTitle => 'Tamamlanmamış çap növbəsi tapıldı';

  @override
  String printResumeSummary(Object count) {
    return '$count işi yoxlamaq və ya davam etdirmək olar';
  }

  @override
  String get printResumeReview => 'Yalnız çap olunmayanları yenidən sına';

  @override
  String get printResumeDiscard => 'Növbəni burax';

  @override
  String get paramsPreviousTitle => 'Əvvəlki çap parametrləri hazırdır';

  @override
  String get paramsPreviousHelp =>
      'Fayllar təhlükəsizlik üçün saxlanmır. Əvvəlki printer, kağız, istiqamət, rəng və növbə parametrlərini yalnız istədikdə bərpa edin.';

  @override
  String get paramsPreviousApply => 'Əvvəlki parametrləri gətir';

  @override
  String get printResumeIncomplete => 'Növbə təhlükəsiz dayandırılıb';

  @override
  String get printCancelQueue => 'Növbəni dayandır';

  @override
  String get printCancelling => 'Dayandırılır…';

  @override
  String get editorActionShowDetails => 'Avtomatik sahələri göstər';

  @override
  String get editorActionHideDetails => 'Avtomatik sahələri gizlət';

  @override
  String get layoutDataResetHeight => 'Hündürlüyü sıfırla';

  @override
  String get layoutDataFit => 'Mövcud sahəyə uyğunlaşdır';

  @override
  String layoutDataResizeHeight(Object label) {
    return '$label hündürlüyünü dəyiş';
  }

  @override
  String layoutColumnResize(Object column) {
    return '$column sütununun enini dəyiş';
  }

  @override
  String layoutColumnWidth(Object width) {
    return 'Eni $width piksel';
  }

  @override
  String get editorBulkTitle => 'Toplu redaktə';

  @override
  String editorSelectedCount(Object count) {
    return '$count seçilib';
  }

  @override
  String get editorSelectAll => 'Hamısını seç';

  @override
  String get editorSelectNone => 'Seçimi təmizlə';

  @override
  String editorSelectRow(Object row) {
    return '$row-ci sətri seç';
  }

  @override
  String get editorSelectGroup => 'Bu qrupu seç';

  @override
  String get editorBulkField => 'Dəyişdiriləcək sahə';

  @override
  String get editorBulkValue => 'Yeni dəyər';

  @override
  String get editorBulkChoose => 'Dəyər seç';

  @override
  String get editorBulkApply => 'Seçilənlərə tətbiq et';

  @override
  String get editorBoolTrue => 'Bəli';

  @override
  String get editorBoolFalse => 'Xeyr';

  @override
  String get operationCancel => 'Cari elementdən sonra dayandır';

  @override
  String get operationCancelling => 'Dayandırılır…';

  @override
  String get warnOperationCancelled =>
      'Əməliyyat təhlükəsiz dayandırıldı; tamamlananlar tarixçədə saxlandı və qalanlar başladılmadı';

  @override
  String get opRfiLogAddTitle => 'RFI jurnalına çoxvərəqli qeyd əlavə et';

  @override
  String get opRfiFollowUpSyncTitle => 'RFI Log-u Follow Up-a sinxronlaşdır';

  @override
  String get opRfiFollowUpSyncDesc =>
      'CIV/MEC həyat dövrü sətirlərini və FINAL Worley Signed/Not Signed sətirlərini RFI Log-dan təhlükəsiz sinxronlaşdırır; hazırlanmış çatışmayan qeydləri, silmələri və qrup statuslarını da işləyir.';

  @override
  String get paramRfiFollowUpRfiLogWorkbook => 'Mənbə RFI Log iş kitabı';

  @override
  String get paramRfiFollowUpRfiLogWorkbookHelp =>
      'Statusların oxunacağı .xlsm və ya .xlsx RFI Log faylını seçin.';

  @override
  String get paramRfiFollowUpFollowUpWorkbook => 'Hədəf Follow Up iş kitabı';

  @override
  String get paramRfiFollowUpFollowUpWorkbookHelp =>
      'Yalnız önizlənmiş RFI və status xanalarının dəyişə biləcəyi .xlsm və ya .xlsx faylını seçin.';

  @override
  String get errorParamRequiredRfiLogWorkbook =>
      'RFI Log iş kitabı tələb olunur';

  @override
  String get errorParamRequiredFollowUpWorkbook =>
      'Follow Up iş kitabı tələb olunur';

  @override
  String get errorRfiFollowUpWorkbookInvalid =>
      'Etibarlı RFI Log və Follow Up .xlsm və ya .xlsx iş kitabları seçin.';

  @override
  String get errorRfiFollowUpWorkbookLocked =>
      'Seçilmiş iş kitabını Excel-də və ya başqa tətbiqdə bağlayıb yenidən cəhd edin.';

  @override
  String get errorRfiFollowUpWorkbookChanged =>
      'Seçilmiş iş kitablarından biri önizləmədən sonra dəyişib. Tətbiqdən əvvəl yeni önizləmə yaradın.';

  @override
  String get errorRfiFollowUpLayoutAmbiguous =>
      'Birdən çox təhlükəsiz Follow Up cədvəli tapıldı. Yalnız bir dəqiq RFI NO./STATUS cədvəli saxlayın.';

  @override
  String get errorRfiFollowUpLayoutMissing =>
      'Dəqiq RFI NO. və STATUS başlıqları olan təhlükəsiz Follow Up cədvəli tapılmadı.';

  @override
  String get errorRfiFollowUpPatchVerification =>
      'DocFlow yalnız təsdiqlənmiş status xanalarının dəyişdiyini doğrulaya bilmədi. Heç bir çıxış qəbul edilmədi.';

  @override
  String get errorRfiFollowUpProtected =>
      'İş kitabı, hədəf sheet və ya tələb olunan xanalar qorunur və ya təhlükəsiz redaktə üçün uyğun deyil.';

  @override
  String get errorRfiFollowUpSignedPackage =>
      'İş kitabında rəqəmsal imza var və avtomatlaşdırma ilə təhlükəsiz dəyişdirilə bilməz.';

  @override
  String get errorRfiFollowUpLimitExceeded =>
      'İş kitabı bu əməliyyat üçün təhlükəsiz emal limitlərini aşır.';

  @override
  String get errorRfiFollowUpNoRowsSelected =>
      'Tətbiqdən əvvəl ən az bir təhlükəsiz Follow Up sətri seçin.';

  @override
  String get errorRfiFollowUpRetainedStorageFull =>
      'Məhdud bərpa yaddaşı doludur. Yenidən cəhd etməzdən əvvəl köhnə DocFlow sübutlarını yoxlayın və ya silin.';

  @override
  String get errorRfiFollowUpBackupExists =>
      'Ayrılmış adda ehtiyat nüsxə artıq mövcuddur. Heç bir iş kitabının üzərinə yazılmadı.';

  @override
  String get errorRfiFollowUpBackupVerify =>
      'DocFlow iş kitabı ehtiyat nüsxəsini doğrulaya bilmədi. Əməliyyat bərpa üçün dayandırıldı.';

  @override
  String get errorRfiFollowUpRecoveryRequired =>
      'Əməliyyat bərpa tələb edir; onu yenidən tətbiq etməyin.';

  @override
  String get rfiFollowUpEditorTitle =>
      'RFI Follow Up status namizədlərini yoxlayın';

  @override
  String rfiFollowUpEditorRowCount(Object count) {
    return '$count Follow Up sətri';
  }

  @override
  String get rfiFollowUpColumnEnabled => 'Tətbiq et';

  @override
  String get rfiFollowUpColumnRfiNo => 'RFI nömrəsi';

  @override
  String get rfiFollowUpColumnSourceStatuses => 'RFI Log mənbə dəyəri';

  @override
  String get rfiFollowUpColumnCurrentStatus => 'Cari Follow Up statusu';

  @override
  String get rfiFollowUpColumnProposedRfiNo => 'Təklif olunan RFI qrupu';

  @override
  String get rfiFollowUpColumnProposedStatus => 'Təklif olunan status';

  @override
  String get rfiFollowUpColumnDisposition => 'Qərar';

  @override
  String get rfiFollowUpColumnReason => 'Səbəb';

  @override
  String get rfiFollowUpColumnSheet => 'Sheet';

  @override
  String get rfiFollowUpColumnRow => 'Sətir';

  @override
  String get rfiFollowUpColumnMappingDetails => 'Uyğunlaşdırma təfərrüatları';

  @override
  String get rfiFollowUpDispositionReady => 'Yenilənməyə hazır';

  @override
  String get rfiFollowUpDispositionAlreadySynced => 'Artıq sinxronlaşdırılıb';

  @override
  String get rfiFollowUpDispositionManualReview => 'Yalnız manual yoxlama';

  @override
  String get warnRfiFollowUpManualRows =>
      'sətir manual yoxlama tələb edir və avtomatik seçilə bilməz.';

  @override
  String get warnRfiFollowUpNoRowsSelected =>
      'Təhlükəsiz tətbiq edilə bilən sətir yoxdur. Bağlı seçimlər manual sətirlərdir; Səbəb sütununa baxın.';

  @override
  String get warnRfiFollowUpUnmappedRecords =>
      'Dəstəklənməyən marşrutu, hazırlanmış hədəf sətri çatışmayan və ya qeyri-müəyyən uyğunluğu olan RFI Log qeydləri dəyişdirilmədi. Dəqiq ITP/item və mənbə/hədəf konteksti üçün Səbəb və Uyğunlaşdırma təfərrüatları sütunlarına baxın.';

  @override
  String get warnRfiFollowUpReasonTargetRfiFormula =>
      'Hədəf RFI nömrəsi formuladır.';

  @override
  String get warnRfiFollowUpReasonTargetRfiMerged =>
      'Hədəf RFI nömrəsi birləşdirilmiş xanadadır.';

  @override
  String get warnRfiFollowUpReasonBlankRfi => 'Hədəf RFI nömrəsi boşdur.';

  @override
  String get warnRfiFollowUpReasonInvalidRfi =>
      'Hədəf RFI nömrəsi CIV-0170 kimi dəqiq qısa formatda olmalıdır.';

  @override
  String get warnRfiFollowUpReasonTargetDuplicate =>
      'Eyni tam RFI nömrəsi hədəf cədvəldə bir neçə dəfə var.';

  @override
  String get warnRfiFollowUpReasonSourceMissing =>
      'RFI Log-da tam dəyər üzrə dəqiq uyğunluq yoxdur.';

  @override
  String get warnRfiFollowUpReasonSourceFormula =>
      'Uyğun mənbə RFI və ya status formuladır.';

  @override
  String get warnRfiFollowUpReasonSourceMerged =>
      'Uyğun mənbə RFI və ya status birləşdirilmiş xanadadır.';

  @override
  String get warnRfiFollowUpReasonSourceStatusBlank =>
      'Uyğun RFI Log statusu boşdur.';

  @override
  String get warnRfiFollowUpReasonSourceStatusUnknown =>
      'Uyğun RFI Log statusu dəqiq Completed və ya Ongoing deyil.';

  @override
  String get warnRfiFollowUpReasonSourceConflict =>
      'RFI Log bu dəqiq RFI nömrəsi üçün ziddiyyətli statuslar saxlayır.';

  @override
  String get warnRfiFollowUpReasonTargetCellMissing =>
      'Hədəf status xanası yoxdur.';

  @override
  String get warnRfiFollowUpReasonTargetFormula =>
      'Hədəf status xanası formuladır.';

  @override
  String get warnRfiFollowUpReasonTargetMerged =>
      'Hədəf status xanası birləşdirilib.';

  @override
  String get warnRfiFollowUpReasonTargetStatusUnknown =>
      'Cari hədəf statusu boş, Signed və ya Not Signed deyil; üzərinə yazılmayacaq.';

  @override
  String get warnRfiFollowUpReasonAlreadySynced =>
      'Hədəfdə təklif olunan status artıq var.';

  @override
  String get warnRfiFollowUpReasonReadySigned =>
      'RFI Log remark sahəsindəki dəqiq Signed by Worley sübutu hədəfə Signed kimi tətbiq edilə bilər.';

  @override
  String get warnRfiFollowUpReasonReadyLogSync =>
      'RFI qrupu və ortaq status təsdiqlənmiş RFI Log uyğunlaşdırmasından sinxronlaşdırıla bilər.';

  @override
  String get warnRfiFollowUpReasonReadyWorleySync =>
      'FINAL Worley statusu REMARKS sahəsindəki dəqiq Signed by Worley sətrindən sinxronlaşdırıla bilər.';

  @override
  String get warnRfiFollowUpReasonReadyRemove =>
      'Cancelled və ya Rejected RFI qeydləri çıxarılacaq və qalan qrupun ortaq statusu yenidən hesablanacaq.';

  @override
  String get warnRfiFollowUpReasonMappingMissing =>
      'Dəstəklənən Follow Up marşrutu mövcuddur, lakin bu RFI üçün ITP, item və konteksti tam uyğun olan hazır hədəf sətri yoxdur. Dəyişiklik edilmədi.';

  @override
  String get warnRfiFollowUpReasonPartialContext =>
      'Bəzi mənbə kontekstləri hazır Follow Up sətirləri ilə uyğunlaşır, bəziləri uyğunlaşmır. Uyğunlaşmayan kontekstlərdə Tag No, Area və Inspection Location dəyərlərini RFI Log-da yoxlayın. DocFlow mənbə jurnalını dəyişmir.';

  @override
  String get warnRfiFollowUpReasonMappingUnsupported =>
      'Cari şablonda bu ITP/item birləşməsi üçün Follow Up marşrutu yoxdur. Avtomatik sinxronlaşdırıla bilməz.';

  @override
  String get warnRfiFollowUpReasonMappingAmbiguous =>
      'Bu qeydə birdən çox hədəf sətri uyğundur; təxmin edilmədən dəyişdirilmədi.';

  @override
  String get warnRfiFollowUpReasonReadyNotSigned =>
      'Dəqiq Signed by Worley remark sübutu tapılmadı; hədəfə Not Signed tətbiq edilə bilər.';

  @override
  String get opRfiLogAddDesc =>
      'CIV, MEC, ELE və PRC RFI nömrələrini birlikdə yaradır, vaxtları bütün iş vərəqləri üzrə planlayır və hər vərəq üçün ayrı Excel cədvəli hazırlayır.';

  @override
  String get paramRfiWorkbook => 'RFI Log Excel faylı';

  @override
  String get paramRfiWorkbookHelp =>
      'Təhlükəsiz oxumaq və yeniləmək üçün makro aktiv .xlsm və ya standart .xlsx RFI Log faylını seçin.';

  @override
  String get dialogFilterExcelWorkbook => 'Excel (.xlsm, .xlsx)';

  @override
  String get errorRfiWorkbookInvalid =>
      'Etibarlı makro aktiv .xlsm və ya standart .xlsx RFI Log faylı seçin.';

  @override
  String get paramRfiEmailText => 'Sorğu e-poçtunu bura yapışdırın';

  @override
  String get paramRfiEmailTextHelp =>
      'E-poçt mətnini yapışdırın. Yalnız bu önizləmədə istifadə olunur; ayarlara və ya Excel-ə yazılmır.';

  @override
  String get paramRfiEmailProvider => 'E-poçt çıxarma üsulu';

  @override
  String get paramRfiEmailProviderHelp =>
      'Offline üsul pulsuz və məxfidir. Gemini Free istəyə bağlıdır və mətni Google-a göndərir.';

  @override
  String get choiceRfiEmailProviderOffline =>
      'Offline — məxfi (tövsiyə edilir)';

  @override
  String get choiceRfiEmailProviderGemini => 'Gemini Free — xarici AI';

  @override
  String get paramRfiGeminiApiKey => 'Gemini API açarı (yalnız Gemini üçün)';

  @override
  String get paramRfiGeminiApiKeyHelp =>
      'Yadda saxlanmır. Windows-da GEMINI_API_KEY də təyin edə bilərsiniz. Məxfi şirkət e-poçtları üçün Gemini Free istifadə etməyin.';

  @override
  String get errorParamRequiredEmailProvider => 'E-poçt çıxarma üsulunu seçin';

  @override
  String get warnRfiEmailExtracted =>
      'E-poçt sahələri redaktə edilən önizləməyə çıxarıldı';

  @override
  String get warnRfiEmailNeedsReview =>
      'Bəzi məlumatlar qeyri-müəyyən və ya əskikdir; sətirləri yoxlayın';

  @override
  String get warnRfiAiFallback =>
      'Gemini əlçatan olmadı; məxfi offline çıxarma istifadə edildi';

  @override
  String get warnRfiGeminiPrivacy =>
      'Gemini Free bu e-poçtu Google-a göndərir. Pulsuz səviyyə məzmunu məhsul inkişafında istifadə oluna bilər; məxfi məlumat göndərməyin.';

  @override
  String get paramRfiDiscipline => 'İntizam';

  @override
  String get paramRfiCount => 'RFI nömrələrinin sayı';

  @override
  String get paramRfiCountHelp =>
      'Unikal RFI nömrələrini sayır; eyni RFI üçün əlavə sətir yeni nömrə və vaxt intervalı işlətmir.';

  @override
  String get paramRfiSheetCounts => 'Sheet-lər və RFI sayları';

  @override
  String get paramRfiSheetCountsHelp =>
      'Bir və ya bir neçə sheet seçin və hər biri üçün unikal RFI sayını təyin edin. Hamısı bir əməliyyatda tətbiq olunur.';

  @override
  String rfiSheetCountsTotal(Object count) {
    return 'Bu əməliyyatda $count RFI nömrəsi';
  }

  @override
  String get paramRfiOverflowNextDay =>
      'Daşan vaxtları növbəti təqvim gününə keçir';

  @override
  String get paramRfiOverflowNextDayHelp =>
      '17:30-dan sonra növbəti gün 09:00-da davam edir. Söndürülərsə gec vaxtlar həmin gündə qalır.';

  @override
  String get errorParamRequiredWorkbook => 'RFI Log Excel faylı mütləqdir';

  @override
  String get errorParamRequiredDiscipline => 'İntizam mütləqdir';

  @override
  String get errorParamRequiredCount => 'RFI sayı mütləqdir';

  @override
  String get errorParamRequiredSheetCounts =>
      'Ən azı bir sheet və RFI nömrəsi seçin';

  @override
  String get noteRfiWorkbookUpdate => 'RFI Log sətirləri əlavə ediləcək';

  @override
  String get noteRfiRestoreBackup => 'Excel ehtiyat nüsxəsini bərpa et';

  @override
  String get warnRfiOverflowMoved =>
      'RFI vaxtları 17:30-u keçdi və növbəti təqvim gününə keçirildi';

  @override
  String get warnRfiOverflowKept =>
      'RFI vaxtları 17:30-u keçir və həmin gündə saxlanılacaq';

  @override
  String get warnRfiGroupScheduleMismatch =>
      'Eyni RFI nömrəli sətirlərin yoxlama tarixi və ya vaxtı fərqlidir';

  @override
  String get rfiEditorTitle => 'RFI qeydlərini yoxla və redaktə et';

  @override
  String rfiEditorRowCount(Object count) {
    return '$count Excel sətri';
  }

  @override
  String get rfiEditorScrollHint =>
      'Bütün xanaları redaktə etmək üçün üfüqi sürüşdürün';

  @override
  String get rfiEditorCompactHint =>
      'Tam nömrə üçün üzərinə gəlin; avtomatik xanalar Təfərrüatlardadır';

  @override
  String get rfiEditorReviewWarnings =>
      'Redaktə olunmuş sətirlər yoxlanıldı. Yeni xəbərdarlıqları nəzərdən keçirib yenidən Tətbiq et düyməsinə basın.';

  @override
  String rfiEditorCopied(Object count) {
    return 'Excel açıldı; $count sətir filtrləndi və Excel formatı ilə kopyalandı';
  }

  @override
  String get rfiActionDuplicate => 'Eyni RFI nömrəsi ilə əlavə sətir yarat';

  @override
  String get rfiActionRemoveDuplicate => 'Bu əlavə sətri sil';

  @override
  String get rfiActionCopyMail => 'Excel-də filtrlə və eynilə kopyala';

  @override
  String get rfiActionPrepareReply => 'Outlook cavabını hazırla';

  @override
  String get rfiActionCopySheet => 'Excel-dən kopyala';

  @override
  String get rfiMailPreviewTitle => 'Outlook üçün Excel cədvəli';

  @override
  String get rfiMailPreviewSourceWorkbook => 'Excel-dən aktual məlumat';

  @override
  String get rfiMailPreviewSourcePlan => 'Təsdiqlənmiş plan məlumatı';

  @override
  String get rfiMailPreviewHint =>
      'Bu önbaxış kopyalanacaq sütun və dəyərləri göstərir. Outlook-a adi qaydada yapışdırın; Excel formatı və xana çərçivələri qorunur.';

  @override
  String get rfiMailPreviewCopy => 'Formatlı cədvəli kopyala';

  @override
  String get rfiActionDetails => 'Avtomatik xanalar və təfərrüatlar';

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
  String get rfiColInspectionDate => 'Yoxlama';

  @override
  String get rfiColStartTime => 'Saat';

  @override
  String get rfiColAzfenType => 'Insp. Type AZFEN';

  @override
  String get rfiColIntecType => 'Insp. Type INTEC';

  @override
  String get rfiColLocation => 'Yer';

  @override
  String get rfiColSubcontractor => 'Subpodratçı';

  @override
  String get rfiDateDay => 'Gün';

  @override
  String get rfiDateMonth => 'Ay';

  @override
  String get rfiDateChangeYear => 'İli dəyişdir';

  @override
  String get rfiDateYear => 'İl';

  @override
  String get rfiColActions => 'Sətirlər';

  @override
  String get actionClose => 'Bağla';

  @override
  String get paramOrganizeIntoRfiFolders => 'Hər RFI üçün ayrıca qovluq yarat';

  @override
  String get paramOrganizeIntoRfiFoldersHelp =>
      'Adlandırılmış PDF-ləri birbaşa hədəfdə saxlamaq üçün söndürün; oxunmayan və ya təkrarlanan fayllar təhlükəsizlik üçün yenə _review-a gedir';

  @override
  String get errorRfiWorkbookLocked =>
      'RFI Log açıqdır və ya yazmaq üçün bağlıdır. Excel-də yadda saxlayıb bağlayın və faylın yalnız oxuma rejimində olmadığını yoxlayın.';

  @override
  String get opRfiStageSignedFirstPagesTitle =>
      'İmzalı RFI ilk səhifələrini hazırla';

  @override
  String get opRfiNormalizePdfOrientationTitle => 'RFI PDF səhifələrini döndər';

  @override
  String get opRfiNormalizePdfOrientationDesc =>
      'Fast RFI Skanından ayrı olaraq CIV, MEC, ELE və PRC altındakı PDF səhifələrinin istiqamət və dönüşünü düzəldir. Yalnız etibarlı müəyyən edilmiş düzəlişlər ayrıca yoxlama nüsxəsinə yazılır; hər hansı səhifə qeyri-müəyyəndirsə, PDF əl ilə yoxlama üçün dəyişdirilmədən saxlanılır.';

  @override
  String get paramRfiOrientationRfiSystemRoot => 'RFI sistemi kökü';

  @override
  String get paramRfiOrientationRfiSystemRootHelp =>
      'CIV, MEC, ELE və PRC qovluqlarını saxlayan yalnız oxunan mənbə. Mənbə faylları dəyişdirilmir.';

  @override
  String get paramRfiOrientationReviewRoot => 'İstiqamət review qovluğu';

  @override
  String get paramRfiOrientationReviewRootHelp =>
      'Düzəldilmiş nüsxələrin eyni qovluq quruluşu ilə yaradılacağı ayrıca qovluq.';

  @override
  String get paramScanIndexForceRescan => 'Bu dəfə tam skanı məcbur et';

  @override
  String get paramScanIndexForceRescanHelp =>
      'Bu əməliyyatda əvvəlki skan indeksini istifadə etmə. Tapşırıqdan çıxanda seçim avtomatik sönür.';

  @override
  String get errorRfiOrientationReviewRootMustBeSeparate =>
      'Review qovluğu RFI sistemi ilə üst-üstə düşməməlidir';

  @override
  String get warnRfiOrientationNoPdfs =>
      'CIV, MEC, ELE və ya PRC altında PDF tapılmadı';

  @override
  String get warnRfiOrientationUnsupported =>
      'PDF təhlükəsiz analiz edilə bilmədi';

  @override
  String get warnRfiOrientationNeedsReview =>
      'Ən azı bir səhifənin istiqaməti kifayət qədər etibarlı deyil; fayl tətbiq edilmədi';

  @override
  String get warnRfiOrientationReviewExists =>
      'Review hədəfi artıq mövcuddur və üzərinə yazılmadı';

  @override
  String get noteRfiPdfOrientationNormalized =>
      'Təsdiqlənmiş PDF səhifə istiqamətlərini ayrıca review nüsxəsində düzəlt';

  @override
  String get rfiOrientationPages => 'Səhifə dönüşləri';

  @override
  String get opRfiStageSignedFirstPagesDesc =>
      'İmzalı skanları tanıyır, RFI Sistem qovluqlarında sürətli və təhlükəsiz axtarış edir; arxivə toxunmadan ayrı yoxlama nüsxələrində yalnız ilk səhifəni yeniləyir.';

  @override
  String get paramRfiSignedScans => 'Yeni imzalı skanlar qovluğu';

  @override
  String get paramRfiSignedScansHelp =>
      'İmza əlavə edilmiş yeni RFI PDF-ləri. Birləşdirilmiş skanın hər səhifəsi ayrıca tanınır.';

  @override
  String get paramRfiArchiveRoot => 'Mövcud RFI arxivinin kökü';

  @override
  String get paramRfiArchiveRootHelp =>
      'Yalnız oxunan RFI System kökü. Bütün alt qovluqlar məhdud multi-core axtarışla yoxlanır; yalnız exact RFI PDF fayl adları uyğunlaşa bilər.';

  @override
  String get paramRfiReviewRoot => 'Review çıxış qovluğu';

  @override
  String get paramRfiReviewRootHelp =>
      'Nüsxələrin arxivdəki nisbi qovluq quruluşu ilə yazılacağı ayrıca, tercihen boş qovluq.';

  @override
  String get paramRfiScanSubfolders =>
      'Yeni skanların alt qovluqlarını da yoxla';

  @override
  String get paramRfiScanSubfoldersHelp =>
      'Standart olaraq aktivdir. Yeni skanlar qovluğunun hər dərinliyindəki PDF-ləri tanıyır; yalnız qəsdən üst-səviyyə yoxlaması üçün söndürün.';

  @override
  String get errorParamRequiredSignedScans =>
      'Yeni imzalı skanlar qovluğu tələb olunur';

  @override
  String get errorParamRequiredArchiveRoot =>
      'Mövcud RFI arxivinin kökü tələb olunur';

  @override
  String get errorParamRequiredReviewRoot =>
      'Review çıxış qovluğu tələb olunur';

  @override
  String get errorRfiReviewRootMustBeSeparate =>
      'Review çıxış qovluğu həm arxivdən, həm də yeni skanlar qovluğundan tam ayrı olmalıdır. Heç bir fayla toxunulmadı.';

  @override
  String get noteRfiSignedFirstPageStaged =>
      'İmzalı ilk səhifə ilə review nüsxəsi hazırlanacaq';

  @override
  String get noteRfiCleanupStaleTemp =>
      'Tamamlanmış review PDF-indən qalan müvəqqəti bağlantını təmizlə';

  @override
  String get warnRfiNoSignedPdfs => 'Yeni skanlar qovluğunda PDF tapılmadı';

  @override
  String get warnRfiSignedUnreadable =>
      'PDF oxuna bilmədi; bu RFI üçün çıxış hazırlanmadı';

  @override
  String get warnRfiSignedDuplicate =>
      'Eyni RFI nömrəsi üçün bir neçə yeni skan tapıldı; avtomatik seçim edilmədi';

  @override
  String get warnRfiArchiveUnreadable =>
      'Arxivin hamısı təhlükəsiz yoxlanıla bilmədi; əskik və ya yanlış uyğunluğun qarşısını almaq üçün çıxış hazırlanmadı';

  @override
  String get warnRfiArchiveMissing =>
      'Arxivdə bu RFI nömrəsinə uyğun fayl tapılmadı';

  @override
  String get warnRfiArchiveAmbiguous =>
      'Arxivdə bu RFI nömrəsinə uyğun bir neçə fayl tapıldı; arxiv yolu sahəsindən bir PDF seçin';

  @override
  String get warnRfiReviewExists =>
      'Review hədəfində fayl artıq mövcuddur; üzərinə yazılmadı';

  @override
  String get warnRfiStaleTempFound =>
      'Review PDF artıq tamamlanıb; əvvəlki Windows kilidindən qalan müvəqqəti fayl təhlükəsiz silinəcək';

  @override
  String get warnRfiReviewPathBlocked =>
      'Review daxilindəki hədəf yol fayl və ya yönləndirilmiş qovluq tərəfindən bloklanıb; çıxış hazırlanmadı';

  @override
  String get warnRfiSignedExtraPages =>
      'Yeni PDF bir neçə səhifədən ibarətdir; yalnız imzalı ilk səhifə istifadə olunacaq';

  @override
  String get warnRfiSignedMultiPage =>
      'Çoxsəhifəli imzalı skan aşkarlandı; hər səhifə ayrıca təhlil edildi';

  @override
  String get warnRfiFirstPageLayoutDiffers =>
      'Yeni ilk səhifənin ölçüsü və ya istiqaməti köhnədən fərqlidir; tətbiqdən əvvəl önizləməni yoxlayın';

  @override
  String get warnRfiPdfFidelityUnsupported =>
      'Bu PDF sənəd bütövlüyü riskə atılmadan hazırlana bilmir; səbəbi yoxlayın və faylı əl ilə emal edin';

  @override
  String get rfiSignedEditorTitle => 'İmzalı ilk səhifələrin yoxlanması';

  @override
  String rfiSignedEditorRowCount(Object count) {
    return 'Uyğun gələn $count RFI PDF-i';
  }

  @override
  String get rfiSignedColumnEnabled => 'Hazırla';

  @override
  String get rfiSignedColumnCode => 'RFI nömrəsi';

  @override
  String get rfiSignedColumnScan => 'İmzalı skan';

  @override
  String get rfiSignedColumnArchive => 'Arxiv yolu';

  @override
  String get rfiSignedArchiveChoose => 'Arxiv PDF-ni seçin';

  @override
  String get rfiSignedPreviewTitle => 'İlk səhifələrin müqayisəsi';

  @override
  String get rfiSignedPreviewHelp =>
      'Arxivdəki ilk səhifəni təsdiqlənmiş planın seçdiyi yeni imzalı səhifə ilə müqayisə edin.';

  @override
  String get rfiSignedPreviewRfi => 'Önizlənəcək RFI';

  @override
  String get rfiSignedPreviewArchive => 'Cari arxiv səhifəsi';

  @override
  String get rfiSignedPreviewSigned => 'Yeni imzalı səhifə';

  @override
  String rfiSignedPreviewArchiveAlt(Object code) {
    return '$code üçün arxivdəki cari ilk səhifə';
  }

  @override
  String rfiSignedPreviewSignedAlt(Object code) {
    return '$code üçün yeni imzalı səhifə';
  }

  @override
  String get rfiSignedPreviewChooseArchive =>
      'İlk səhifəsini göstərmək üçün arxiv PDF-ni seçin.';

  @override
  String get rfiSignedPreviewUnavailable =>
      'Məhdud səhifə önizləməsi hazırlana bilmədi; tətbiq etməzdən əvvəl PDF-ni əl ilə yoxlayın.';

  @override
  String rfiSignedPreviewOmitted(Object count) {
    return 'Təhlükəsiz önizləmə həddinə çatıldığı üçün əlavə $count RFI sətrinin önizləməsi göstərilmir.';
  }

  @override
  String get rfiSignedPreviewPrevious => 'Əvvəlki RFI';

  @override
  String get rfiSignedPreviewNext => 'Növbəti RFI';

  @override
  String get rfiSignedPreviewOnlyPageOne =>
      'Yalnız 1-ci səhifə dəyişdiriləcək; arxivdəki digər səhifələr dəyişməz qalacaq.';

  @override
  String get rfiSignedPreviewBefore => 'ƏVVƏL';

  @override
  String get rfiSignedPreviewAfter => 'SONRA';

  @override
  String get rfiSignedPreviewEnlarge => 'Böyüt';

  @override
  String get rfiSignedNavLabel => 'İmzalı səhifə önizləmə bölmələri';

  @override
  String get rfiSignedNavApproved => 'Təsdiqlənmiş RFI-lər';

  @override
  String get rfiSignedNavCompare => 'Səhifələri müqayisə et';

  @override
  String get rfiSignedNavReview => 'Review nüsxələri';

  @override
  String get rfiSignedWarningReviewTitle =>
      'Xəbərdarlıq verilən PDF-lər Review-a göndəriləcək';

  @override
  String get rfiSignedWarningReviewHelp =>
      'Apply təsirlənən hər mənbə PDF-i dəyişdirmədən kopyalayır və alt qovluq yolunu _review daxilində saxlayır.';

  @override
  String get rfiSignedWarningReviewPlanned => 'Kopyalanacaq';

  @override
  String get rfiSignedWarningReviewExists => 'Artıq mövcuddur';

  @override
  String get rfiSignedWarningReviewBlocked => 'Bloklanıb';

  @override
  String get rfiSignedWarningReviewDestination => 'Review təyinatı';

  @override
  String rfiSignedWarningReviewPage(Object page) {
    return 'Səhifə $page';
  }

  @override
  String get rfiSignedWarningReviewFile => 'Bütün fayl';

  @override
  String get noteRfiSignedWarningReviewCopy =>
      'Xəbərdarlıq PDF-ini Review-a kopyala';

  @override
  String get warnRfiWarningReviewExists =>
      'Xəbərdarlıq PDF-i Review-da artıq mövcuddur və üzərinə yazılmayacaq';

  @override
  String get warnRfiWarningReviewBlocked =>
      'Təyinat yolu bloklandığı üçün xəbərdarlıq PDF-i Review-a kopyalana bilmir';

  @override
  String get progressStages => 'İşləmə mərhələləri';

  @override
  String get progressPhasePreparing => 'Önizləmə hazırlanır';

  @override
  String get progressPhasePreview => 'Önizləmə yaradılır';

  @override
  String get progressPhaseApply => 'Təsdiqlənmiş plan tətbiq edilir';

  @override
  String get progressPhasePrintWaiting => 'Printer gözlənilir';

  @override
  String get progressPhaseScan => 'Fayllar skan edilir';

  @override
  String get progressPhaseScanDiscovery => 'Skan ediləcək PDF-lər tapılır';

  @override
  String get progressPhaseFileGroupDiscovery =>
      'Qruplaşdırılacaq fayllar tapılır';

  @override
  String get progressPhaseFileGroupAnalysis => 'RFI nömrələri oxunur';

  @override
  String get progressPhaseFileGroupPlanning => 'RFI qovluq planı hazırlanır';

  @override
  String get progressPhaseScanOcr => 'PDF-lər OCR ilə oxunur';

  @override
  String get progressPhaseScanPlanning => 'Fayl planı hazırlanır';

  @override
  String get progressPhaseWorkbookReading => 'İş kitabı oxunur';

  @override
  String get progressPhaseRfiRequestAnalysis => 'RFI sorğusu təhlil edilir';

  @override
  String get progressPhaseRfiPlanPreparation => 'RFI sətirləri hazırlanır';

  @override
  String get progressPhaseFollowUpAnalysis =>
      'Follow Up uyğunluqları hesablanır';

  @override
  String get progressPhaseSystemDiscovery => 'RFI sistem qeydləri tapılır';

  @override
  String get progressPhaseSystemSignatureAnalysis =>
      'Worley imzaları təhlil edilir';

  @override
  String get progressPhaseSystemWorkbookRead => 'RFI iş kitabları oxunur';

  @override
  String get progressPhaseSystemMapping =>
      'İmzalar iş kitabı qeydləri ilə uyğunlaşdırılır';

  @override
  String get progressPhaseOrientationDiscovery =>
      'İstiqaməti yoxlanacaq PDF-lər tapılır';

  @override
  String get progressPhaseOrientationInspection =>
      'PDF quruluşu və skan keşi yoxlanılır';

  @override
  String get progressPhaseOrientationAnalysis =>
      'PDF səhifə istiqamətləri təhlil edilir';

  @override
  String get progressPhasePrinterCheck => 'Printer hazırlığı yoxlanılır';

  @override
  String get progressPhasePrintFileInspection =>
      'Çap faylları və parametrləri yoxlanılır';

  @override
  String get progressPhaseOutlookSelection => 'Outlook seçimi yoxlanılır';

  @override
  String get progressPhaseSignedDiscovery => 'İmzalı PDF-lər tapılır';

  @override
  String get progressPhaseSignedInspection => 'PDF səhifələri yoxlanılır';

  @override
  String get progressPhaseSignedOcr => 'RFI nömrələri oxunur';

  @override
  String get progressPhaseSignedClassification =>
      'İmzalı səhifələr uyğunlaşdırılır';

  @override
  String get progressPhaseArchiveScan => 'RFI arxivində axtarılır';

  @override
  String get progressPhaseComparisonPreparation =>
      'Səhifə müqayisələri hazırlanır';

  @override
  String get progressPhasePreviewReady => 'Önizləmə hazırdır';

  @override
  String get recoveryTitle => 'Bərpa mərkəzi';

  @override
  String get homeRecoveryDesc =>
      'Gözləyən jurnalları və qeyri-müəyyən çap işlərini təhlükəsiz yoxlayın.';

  @override
  String get recoveryHelp =>
      'Bərpaya yalnız yerli jurnallar və saxlanmış çap növbələri icazə verir.';

  @override
  String get recoveryNoEffect =>
      'Dəyişiklik olmayıb — yenidən önbaxış təhlükəsizdir';

  @override
  String get recoveryOutcomeUnknown =>
      'Nəticə naməlumdur — avtomatik təkrar yoxdur';

  @override
  String get recoveryManual => 'Əl ilə yoxlama tələb olunur';

  @override
  String get recoveryRestoreOriginal =>
      'Follow Up faylının orijinalını bərpa et';

  @override
  String get recoveryPrintQueue => 'Qeyri-müəyyən çap növbəsi';

  @override
  String get recoveryEmpty => 'Bərpa ediləcək heç nə yoxdur';

  @override
  String get recoveryEmptyHelp =>
      'Gözləyən jurnal və ya yarımçıq çap növbəsi tapılmadı.';

  @override
  String get effectCommitted => 'Tamamlandı';

  @override
  String get effectNoEffectRetryable =>
      'Dəyişiklik olmayıb — təkrar təhlükəsizdir';

  @override
  String get effectPartialCommitted => 'Qismən tamamlandı — bərpanı yoxlayın';

  @override
  String get effectOutcomeUnknown =>
      'Nəticə naməlumdur — avtomatik təkrar yoxdur';

  @override
  String get effectCancelledClean => 'Təmiz ləğv edildi — dəyişiklik olmayıb';

  @override
  String get effectCancelledPartial => 'Qismən ləğv edildi — bərpanı yoxlayın';

  @override
  String get presetTitle => 'Önayarlar';

  @override
  String get presetSelect => 'Saxlanmış önayar';

  @override
  String get presetSelectPlaceholder => 'Önayar seçin';

  @override
  String get presetName => 'Yeni önayar adı';

  @override
  String get presetApply => 'Önayarı tətbiq et';

  @override
  String get presetSave => 'Önayarı saxla';

  @override
  String get presetReplace => 'Önayarı əvəz et';

  @override
  String get presetDelete => 'Önayarı sil';

  @override
  String presetConfirmReplace(Object name) {
    return '$name adlı önayar artıq mövcuddur. Əvəz edilsin?';
  }

  @override
  String presetConfirmDelete(Object name) {
    return '$name adlı önayar silinsin?';
  }

  @override
  String get presetEmpty => 'Hələ saxlanmış önayar yoxdur.';

  @override
  String presetSaved(Object name) {
    return '$name önayarı saxlanıldı.';
  }

  @override
  String presetDeleted(Object name) {
    return '$name önayarı silindi.';
  }

  @override
  String get errorPresetNameInvalid =>
      'Önayar adı 1–64 simvoldan ibarət olmalıdır.';

  @override
  String get errorPresetDuplicate => 'Bu adda önayar artıq mövcuddur.';

  @override
  String get errorPresetLimit =>
      'Bu əməliyyat üçün ən çox 20 önayar saxlamaq olar.';

  @override
  String get errorPresetStale =>
      'Bu önayar artıq mövcud deyil. Siyahını yeniləyib təkrar cəhd edin.';

  @override
  String get errorPresetWrite =>
      'Önayar parametrlərini saxlamaq mümkün olmadı. Təkrar cəhd edin.';

  @override
  String get opRfiOutlookReplyDraftTitle =>
      'Outlook Hamısına Cavab qaralaması hazırla';

  @override
  String get opRfiOutlookReplyDraftDesc =>
      'Yalnız Outlook-da seçilmiş bir e-poçtdan istifadə edir, Hamısına Cavab hazırlayır və yoxlama üçün GÖNDƏRİLMƏMİŞ qaralama kimi göstərir.';

  @override
  String get paramRfiOutlookReplyDraftReplyText => 'Cavab mətni';

  @override
  String get paramRfiOutlookReplyDraftReplyTextHelp =>
      'Əvvələ əlavə ediləcək mətni daxil edin. Önbaxış HTML-i qaçışlayır; tətbiq məktubu heç vaxt göndərmir və alıcı əlavə etmir.';

  @override
  String get errorParamRequiredReplyText => 'Cavab mətni tələb olunur';

  @override
  String get stepRfiOutlookReplyDraft =>
      'Göndərilməmiş Hamısına Cavab qaralamasını göstər';

  @override
  String get outlookDraftPreviewTitle =>
      'Göndərilməmiş Outlook cavabının önbaxışı';

  @override
  String get outlookDraftSubject => 'Seçilmiş məktubun mövzusu';

  @override
  String get outlookDraftPlainText => 'Sadə mətn';

  @override
  String get outlookDraftHtml => 'Qaçışlanmış HTML';

  @override
  String get warnRfiOutlookDraftUnsent =>
      'Tətbiq GÖNDƏRİLMƏMİŞ Hamısına Cavab qaralamasını göstərəcək; DocFlow onu heç vaxt göndərmir.';

  @override
  String get warnRfiOutlookDraftUndoConditional =>
      'Geri qaytarma yalnız Outlook davamlı qaralamanı avtomatik saxladıqda və həmin qaralama dəyişmədikdə mümkündür.';

  @override
  String get warnRfiOutlookDraftUndoUnavailable =>
      'Outlook davamlı avtomatik saxlanmış qaralama təqdim etmədi; geri qaytarma mümkün deyil.';

  @override
  String get errorRfiOutlookDraftUnsupportedPlatform =>
      'Outlook qaralama avtomatlaşdırması yalnız Windows-da mövcuddur.';

  @override
  String get errorRfiOutlookDraftReplyRequired =>
      'Önbaxışdan əvvəl cavab mətnini daxil edin.';

  @override
  String get errorRfiOutlookDraftRfiLogNotCommitted =>
      'Outlook cavabı yalnız eyni tamamlanmış RFI Log əməliyyatından hazırlana bilər.';

  @override
  String get errorRfiOutlookDraftReplyTooLong =>
      'Cavab mətni 10.000 simvol həddini aşır.';

  @override
  String get errorRfiOutlookDraftReplyInvalid =>
      'Cavab mətnində dəstəklənməyən idarəetmə simvolu var.';

  @override
  String get errorRfiOutlookDraftSelectionRequired =>
      'Aktiv Outlook Explorer pəncərəsində dəqiq bir e-poçt seçin.';

  @override
  String get errorRfiOutlookDraftSingleSelectionRequired =>
      'Dəqiq bir Outlook e-poçtu seçin.';

  @override
  String get errorRfiOutlookDraftMailItemRequired =>
      'Cari Outlook seçimi e-poçt məktubu deyil.';

  @override
  String get errorRfiOutlookDraftSelectionInvalid =>
      'Seçilmiş Outlook e-poçtunun sabit identifikatoru yoxdur.';

  @override
  String get errorRfiOutlookDraftSubjectTooLong =>
      'Seçilmiş Outlook mövzusu önbaxış həddini aşır.';

  @override
  String get errorRfiOutlookDraftSelectionChanged =>
      'Seçilmiş Outlook e-poçtu önbaxışdan sonra dəyişib; yenidən önbaxış edin.';

  @override
  String get errorRfiOutlookDraftOutlookUnavailable =>
      'Microsoft Outlook masaüstü avtomatlaşdırması mövcud deyil.';

  @override
  String get errorRfiOutlookDraftOutlookNotRunning =>
      'Önbaxışdan əvvəl Outlook masaüstünü açın və bir e-poçt seçin.';

  @override
  String get errorRfiOutlookDraftComUnavailable =>
      'Windows Outlook avtomatlaşdırma apartamenti mövcud deyil.';

  @override
  String get errorRfiOutlookDraftComFailed =>
      'Outlook tələb olunan məhdud məktub xüsusiyyətini təqdim edə bilmədi.';

  @override
  String get errorRfiOutlookDraftReplyAllFailed =>
      'Outlook Hamısına Cavab qaralamasının yaradılmasını təsdiqləmədi; Outlook-u əl ilə yoxlayın.';

  @override
  String get errorRfiOutlookDraftBodyPrependFailed =>
      'Outlook cavab gövdəsinin yenilənməsini təsdiqləmədi; Outlook-u əl ilə yoxlayın.';

  @override
  String get errorRfiOutlookDraftBodyFormatUnsupported =>
      'Outlook cavabı dəstəklənməyən zəngin mətn formatındadır; Outlook-u əl ilə yoxlayın.';

  @override
  String get errorRfiOutlookDraftContentTooLarge =>
      'Outlook qaralama gövdəsi təhlükəsiz avtomatlaşdırma həddini aşır.';

  @override
  String get errorRfiOutlookDraftDisplayUnknown =>
      'Outlook göndərilməmiş qaralamanın göstərilməsini təsdiqləmədi; Outlook-u əl ilə yoxlayın.';

  @override
  String get errorRfiOutlookDraftDraftMissingOrChanged =>
      'Dəqiq avtomatik saxlanmış qaralama olmadığı və ya dəyişdiyi üçün geri qaytarma rədd edildi.';

  @override
  String get errorRfiOutlookDraftDeleteUnknown =>
      'Outlook dəyişməmiş qaralamanın silinməsini təsdiqləmədi; Outlook-u əl ilə yoxlayın.';

  @override
  String get errorRfiOutlookDraftUndoUnavailable =>
      'Outlook davamlı qaralamanı avtomatik saxlamadığı üçün geri qaytarma mümkün deyil.';

  @override
  String get opRfiOutlookArchiveSelectedTitle =>
      'Seçilmiş Outlook e-poçtunu arxivlə';

  @override
  String get opRfiOutlookArchiveSelectedDesc =>
      'Bir seçilmiş Outlook e-poçtunu göndərmədən, köçürmədən və ya silmədən yerli Unicode MSG surəti kimi saxlayır.';

  @override
  String get paramRfiOutlookArchiveDestination => 'Poçt arxiv qovluğu';

  @override
  String get paramRfiOutlookArchiveDestinationHelp =>
      'Ayrı hazırlıq qovluğu seçin. RFI üzrə Faylları Qrupla sonra bu MSG-ni bütün dəqiq əlaqəli RFI qovluqlarına kopyalaya bilər.';

  @override
  String get stepRfiOutlookArchive => 'Qorunan yerli MSG surəti yarat';

  @override
  String get warnRfiOutlookArchiveLocalCopy =>
      'Tətbiq bir yerli MSG surəti yaradır; seçilmiş Outlook e-poçtu və poçt qutusu dəyişmir.';

  @override
  String get errorRfiOutlookArchiveDestinationExists =>
      'Planlaşdırılan MSG fayl adı arxiv qovluğunda artıq var; faylı adlandırın və ya köçürün, sonra yenidən önbaxış edin.';

  @override
  String get errorRfiOutlookArchivePlanUnavailable =>
      'Təsdiqlənmiş Outlook arxiv planı artıq mövcud deyil; yenidən önbaxış edin.';

  @override
  String get errorRfiOutlookArchiveSelectionChanged =>
      'Seçilmiş Outlook e-poçtu önbaxışdan sonra dəyişib; ilkin e-poçtu seçin və yenidən önbaxış edin.';

  @override
  String get errorRfiOutlookArchiveExportFailed =>
      'Outlook özəl MSG ixracını yarada bilmədi; arxiv faylı dərc edilmədi.';

  @override
  String get errorRfiOutlookArchiveInvalidMsg =>
      'Outlook boş, həddən artıq böyük və ya etibarsız MSG ixracı yaratdı; arxiv faylı dərc edilmədi.';

  @override
  String get errorRfiOutlookArchiveWriteFailed =>
      'MSG arxiv faylı yaradıla bilmədi; mövcud faylın üzərinə yazılmadı.';

  @override
  String get errorRfiOutlookArchiveWriteUnknown =>
      'MSG arxiv yazısı təsdiqlənmədi; yenidən cəhddən əvvəl hədəfi yoxlayın.';

  @override
  String get opRfiSystemSyncTitle =>
      'RFI Sistemini Analiz Et və Worley İmzalarını Sinxronlaşdır';

  @override
  String get opRfiSystemSyncDesc =>
      'Mühəndislik sahələri üzrə RFI sistem qovluqlarını skan edir, 1-ci səhifədəki Others bölməsində Worley imzalarını yoxlayır və RFI Log ilə Follow Up kitablarını sinxronlaşdırır.';

  @override
  String get paramRfiSystemSyncRfiSystemRoot => 'RFI Sistem Əsas Qovluğu';

  @override
  String get paramRfiSystemSyncRfiSystemRootHelp =>
      'CIV, MEC, ELE və PRC fənn qovluqlarını ehtiva edən əsas qovluğu seçin.';

  @override
  String get paramRfiSystemSyncRfiLogWorkbook => 'RFI Log İş Kitabı';

  @override
  String get paramRfiSystemSyncRfiLogWorkbookHelp =>
      'Qeydlər (REMARKS) sütununa \'Signed by Worley\' yazılacaq RFI Log (.xlsm və ya .xlsx) faylını seçin.';

  @override
  String get paramRfiSystemSyncFollowUpWorkbook => 'Follow Up İş Kitabı';

  @override
  String get paramRfiSystemSyncFollowUpWorkbookHelp =>
      'Signed By Worley CIV və Signed By Worley MEC nəzarət cədvəllərini ehtiva edən cari Follow Up faylını seçin.';

  @override
  String get errorParamRequiredRfiSystemRoot =>
      'RFI Sistem əsas qovluğu mütləqdir';

  @override
  String get errorRfiSystemSyncWorkbookInvalid =>
      'Düzgün RFI Log və Follow Up .xlsm və ya .xlsx iş kitablarını seçin.';

  @override
  String get errorRfiSystemSyncWorkbookLocked =>
      'Seçilmiş iş kitabını Excel və ya digər tətbiqdə bağlayıb yenidən cəhd edin.';

  @override
  String get errorRfiSystemSyncWorkbookLayoutInvalid =>
      'RFI Log-dakı CIV, MEC, ELE və PRC səhifələrinin hər birində bir dəqiq RFI No. və REMARKS başlığı olmalıdır.';

  @override
  String get errorRfiSystemSyncFollowUpLayoutInvalid =>
      'Follow Up faylında bir Signed By Worley CIV cədvəli və bir Signed By Worley MEC nəzarət cədvəli olmalıdır.';

  @override
  String get errorRfiSystemSyncApprovedUpdateInvalid =>
      'Təsdiqlənmiş iş kitabı yeniləməsi etibarsızdır; yenidən önizləyin.';

  @override
  String get errorRfiSystemSyncWorkbookChanged =>
      'Seçilmiş iş kitabı önizləmədən sonra dəyişib; heç bir yeniləmə tətbiq edilmədi. Yenidən önizləyin.';

  @override
  String get warnRfiSystemSyncNoChangesNeeded =>
      'Bütün aşkar edilmiş RFI-lar artıq sinxronlaşdırılıb və ya imzasızdır.';

  @override
  String get warnRfiSystemSyncNoRowsSelected =>
      'Sinxronlaşdırmaq üçün ən azı bir təhlükəsiz imzalı RFI seçin.';

  @override
  String get noteRfiSystemSyncRfiLog =>
      'Worley imzalarını RFI Log qeydlərinə sinxronlaşdır';

  @override
  String get noteRfiSystemSyncFollowUp =>
      'Final ITP imzalı RFI-ları Follow Up iş kitabına sinxronlaşdır';

  @override
  String get noteRfiSystemSyncBackup => 'Sinxronizasiyadan əvvəl nüsxə yarat';

  @override
  String get noteRfiSystemSyncRestore =>
      'İş kitabını əvvəlki nüsxədən bərpa et';

  @override
  String get rfiSystemSyncEditorTitle =>
      'RFI Sistemi və Worley İmza Sinxronizasiyasını İncələ';

  @override
  String rfiSystemSyncEditorRowCount(Object count) {
    return '$count skan edilmiş RFI';
  }

  @override
  String get rfiSystemSyncColumnEnabled => 'Tətbiq et';

  @override
  String get rfiSystemSyncColumnDiscipline => 'Fənn';

  @override
  String get rfiSystemSyncColumnRfiNo => 'RFI №';

  @override
  String get rfiSystemSyncColumnWorleyStatus => 'Worley İmzası';

  @override
  String get rfiSystemSyncColumnRfiLogAction => 'RFI Log Əməliyyatı';

  @override
  String get rfiSystemSyncColumnFollowUpAction => 'Follow Up Əməliyyatı';

  @override
  String get rfiSystemSyncColumnDisposition => 'Status';

  @override
  String get rfiSystemSyncColumnReason => 'Sübut / Təfərrüatlar';

  @override
  String get rfiSystemSyncDispositionReady => 'Sinxronizasiyaya hazırdır';

  @override
  String get rfiSystemSyncDispositionAlreadySynced => 'Artıq sinxronlaşdırılıb';

  @override
  String get rfiSystemSyncDispositionUnsigned => 'İmzasızdır';

  @override
  String get rfiSystemSyncDispositionManualReview => 'Əl ilə yoxlama';

  @override
  String get opRfiCompleteStatusSyncTitle =>
      'RFI Sistemindən tamamlanmış RFI-ları işarələ';

  @override
  String get opRfiCompleteStatusSyncDesc =>
      'RFI Sistem alt qovluqlarındakı PDF-ləri skan edir və RFI Jurnalındakı yoxlama statusunu \'Tamamlandı\' (Completed) olaraq yeniləmək üçün önbaxış hazırlayır.';

  @override
  String get paramRfiCompleteStatusRfiSystemRoot => 'RFI Sistem Kök Qovluğu';

  @override
  String get paramRfiCompleteStatusRfiSystemRootHelp =>
      'RFI Sistem kökünü seçin. Bütün alt qovluqlar yalnız oxuma rejimində yoxlanır; simvolik keçidlər izlənmir.';

  @override
  String get paramRfiCompleteStatusRfiLogWorkbook => 'RFI Log İş Kitabı';

  @override
  String get paramRfiCompleteStatusRfiLogWorkbookHelp =>
      'Yoxlamadan sonra təhlükəsiz Inspection Status xanaları Completed edilə bilən .xlsm və ya .xlsx RFI Log faylını seçin.';

  @override
  String get errorRfiCompleteStatusSystemRootInvalid =>
      'Oxuna bilən RFI Sistem kök qovluğu seçin.';

  @override
  String get errorRfiCompleteStatusWorkbookInvalid =>
      'Etibarlı RFI Log .xlsm və ya .xlsx faylı seçin.';

  @override
  String get errorRfiCompleteStatusWorkbookLocked =>
      'Seçilmiş RFI Log faylını Excel və ya digər tətbiqdə bağlayıb yenidən cəhd edin.';

  @override
  String get errorRfiCompleteStatusWorkbookLayoutInvalid =>
      'CIV, MEC, ELE və PRC vərəqlərinin hər birində bir dəqiq RFI No. və Inspection Status başlığı olmalıdır.';

  @override
  String get errorRfiCompleteStatusApprovedUpdateInvalid =>
      'Təsdiqlənmiş tamamlanma statusu dəyişikliyi etibarsızdır; yenidən önbaxış edin.';

  @override
  String get warnRfiCompleteStatusInvalidPdfName =>
      'RFI No. PDF faylının adı canonical deyil və istifadə edilmədi.';

  @override
  String get warnRfiCompleteStatusInvalidPdfNameOverflow =>
      'Digər canonical olmayan RFI No. PDF adları xəbərdarlıq siyahısından çıxarıldı.';

  @override
  String get warnRfiCompleteStatusNoChangesNeeded =>
      'Təhlükəsiz tətbiq ediləcək RFI Log tamamlanma dəyişikliyi yoxdur.';

  @override
  String get warnRfiCompleteStatusNoRowsSelected =>
      'Sinxronlaşdırmaq üçün ən az bir təhlükəsiz tamamlanmış RFI seçin.';

  @override
  String get noteRfiCompleteStatusRfiLog =>
      'Təsdiqlənmiş RFI Log Inspection Status xanalarını Completed et';

  @override
  String get rfiCompleteStatusEditorTitle =>
      'Tamamlanmış RFI sübutlarını və status dəyişikliklərini yoxla';

  @override
  String rfiCompleteStatusEditorRowCount(Object count) {
    return 'Dəqiq tamamlanmış PDF-i olan $count RFI';
  }

  @override
  String get rfiCompleteStatusColumnEnabled => 'Tətbiq et';

  @override
  String get rfiCompleteStatusColumnDiscipline => 'Fənn';

  @override
  String get rfiCompleteStatusColumnRfiNo => 'RFI No.';

  @override
  String get rfiCompleteStatusColumnPdfPath => 'Tamamlanmış PDF';

  @override
  String get rfiCompleteStatusColumnPdfCount => 'PDF sayı';

  @override
  String get rfiCompleteStatusColumnCurrentStatus => 'Cari status';

  @override
  String get rfiCompleteStatusColumnProposedStatus => 'Yeni status';

  @override
  String get rfiCompleteStatusColumnDisposition => 'Qərar';

  @override
  String get rfiCompleteStatusColumnAction => 'Əməliyyat';

  @override
  String get rfiCompleteStatusColumnPdfPaths => 'Bütün uyğun PDF-lər';

  @override
  String get rfiCompleteStatusColumnReason => 'Sübut / Detallar';

  @override
  String get rfiCompleteStatusDispositionReady => 'Hazır';

  @override
  String get rfiCompleteStatusDispositionAlreadyCompleted => 'Artıq Completed';

  @override
  String get rfiCompleteStatusDispositionManualReview => 'Əl ilə yoxlama';

  @override
  String get progressPhaseCompletionPdfDiscovery =>
      'Bütün RFI Sistem qovluqlarında tamamlanmış PDF-lər skan edilir';

  @override
  String get progressPhaseCompletionWorkbookRead =>
      'RFI Log status xanaları oxunur';

  @override
  String get progressPhaseCompletionMapping =>
      'Tamamlanmış PDF-lər RFI Log sətirləri ilə uyğunlaşdırılır';

  @override
  String get opFilesReplicaPublishTitle => 'RFI sistemlərini yayımla';

  @override
  String get opFilesReplicaPublishDesc =>
      'Seçilmiş mənbə RFI Sistemini saxlanılan yedəklər, hash yoxlaması və Geri al ilə iki hədəfə yayımlayır.';

  @override
  String get paramReplicaRootA => 'Sistem A qovluğu';

  @override
  String get paramReplicaRootAHelp =>
      'SharePoint, şəbəkə və ya lokal RFI Sistem köklərindən biri.';

  @override
  String get paramReplicaRootB => 'Sistem B qovluğu';

  @override
  String get paramReplicaRootBHelp => 'İkinci RFI Sistem kökü.';

  @override
  String get paramReplicaRootC => 'Sistem C qovluğu';

  @override
  String get paramReplicaRootCHelp => 'Üçüncü RFI Sistem kökü.';

  @override
  String get paramReplicaSourceRoot => 'Səlahiyyətli mənbə';

  @override
  String get paramReplicaSourceRootHelp =>
      'Bu sistem digər iki hədəfə yayımlanır. Hədəf dəyişiklikləri mənbəyə qaytarılmır.';

  @override
  String get choiceReplicaSourceA => 'Sistem A';

  @override
  String get choiceReplicaSourceB => 'Sistem B';

  @override
  String get choiceReplicaSourceC => 'Sistem C';

  @override
  String get paramReplicaBackupRoot => 'Versiya yedəkləri qovluğu';

  @override
  String get paramReplicaBackupRootHelp =>
      'Dəyişdirilən hər hədəf fayl əvvəlcə ayrı saxlanılan yedəyə kopyalanır.';

  @override
  String get paramReplicaRootFiles => 'Əlavə kök faylları';

  @override
  String get paramReplicaRootFilesHelp =>
      'CIV, MEC, ELE və PRC xaricində sync ediləcək kök fayl adlarını nöqtəli vergül və ya yeni sətirlə ayırın.';

  @override
  String get paramReplicaUnattended => 'Təsdiqsiz avtomatik rejimi aktiv et';

  @override
  String get paramReplicaUnattendedHelp =>
      'Bu önizləməni bir dəfə tətbiq etmək yalnız sabit profilə daimi icazə verir. Qeyri-müəyyənlik avtomatlaşdırmanı dayandırır.';

  @override
  String get paramReplicaIntervalMinutes => 'Yoxlama intervalı (dəqiqə)';

  @override
  String get paramReplicaIntervalMinutesHelp =>
      '1 ilə 1440 dəqiqə arasında. DocFlow açıq olduqda işləyir.';

  @override
  String get paramReplicaSettleSeconds => 'Faylın sabitləşmə müddəti (saniyə)';

  @override
  String get paramReplicaSettleSecondsHelp =>
      'Avtomatik dərc ən yeni mənbə dəyişikliyindən sonra bu qədər gözləyir. 5 ilə 3600 arasında.';

  @override
  String get paramReplicaMaxChangedFiles =>
      'Avtomatik iş üçün maksimum fayl təsiri';

  @override
  String get paramReplicaMaxChangedFilesHelp =>
      'Bir iş daha çox hədəf faylı dəyişərsə avtomatlaşdırma dayandırılır.';

  @override
  String get paramReplicaMaxChangedMib => 'Avtomatik iş üçün maksimum MiB';

  @override
  String get paramReplicaMaxChangedMibHelp =>
      'Bir işin ümumi dərc ölçüsü həddi aşarsa avtomatlaşdırma dayandırılır.';

  @override
  String get replicaEditorTitle => 'Dəqiq yayım təsirlərini yoxla';

  @override
  String replicaEditorRowCount(Object count) {
    return '$count fayl təsiri';
  }

  @override
  String get replicaColumnTarget => 'Hədəf';

  @override
  String get replicaColumnPath => 'Nisbi yol';

  @override
  String get replicaColumnAction => 'Əməliyyat';

  @override
  String get replicaColumnBeforeHash => 'Əvvəlki hash';

  @override
  String get replicaColumnAfterHash => 'Mənbə hash';

  @override
  String get replicaActionCopy => 'Yeni faylı kopyala';

  @override
  String get replicaActionReplace => 'Yedəklə və dəyişdir';

  @override
  String get noteReplicaMkdir => 'Tələb olunan qovluğu yarat';

  @override
  String get noteReplicaCopy => 'Çatışmayan hədəf faylı kopyala';

  @override
  String get noteReplicaReplace => 'Hədəfi yedəklə, yoxla və mənbədən dəyişdir';

  @override
  String get noteReplicaRestore => 'Hədəfi yayımdan əvvəlki yedəkdən bərpa et';

  @override
  String get warnReplicaTargetExtra =>
      'Hədəfdə mənbədə olmayan fayl var; silinmədi.';

  @override
  String get warnReplicaTargetExtraOverflow =>
      'Siyahıya sığmayan başqa yalnız-hədəf faylları da var; heç biri silinmədi.';

  @override
  String get warnReplicaNoChanges =>
      'Hər üç sistem artıq seçilmiş mənbə ilə eynidir.';

  @override
  String get errorReplicaRootInvalid => 'Üç oxuna bilən RFI Sistem kökü seçin.';

  @override
  String get errorReplicaBackupInvalid =>
      'Oxuna bilən ayrıca yedək qovluğu seçin.';

  @override
  String get errorReplicaSourceInvalid =>
      'Səlahiyyətli mənbə seçimi yanlışdır.';

  @override
  String get errorReplicaRootsOverlap =>
      'RFI Sistem kökləri eyni və ya iç-içə ola bilməz.';

  @override
  String get errorReplicaBackupOverlap =>
      'Yedək qovluğu heç bir RFI Sistem kökü ilə eyni və ya iç-içə ola bilməz.';

  @override
  String get errorReplicaRootFileInvalid => 'Əlavə kök fayl adı yanlışdır.';

  @override
  String get errorReplicaSourceRootFileMissing =>
      'Seçilmiş əlavə kök faylı mənbə sistemdə yoxdur.';

  @override
  String get errorReplicaTooManyRootFiles =>
      'Ən çox 64 əlavə kök faylı seçilə bilər.';

  @override
  String get errorReplicaIntervalInvalid =>
      'Yoxlama intervalı 1 ilə 1440 dəqiqə arasında olmalıdır.';

  @override
  String get errorReplicaSettleInvalid =>
      'Faylın sabitləşmə müddəti 5 ilə 3600 saniyə arasında olmalıdır.';

  @override
  String get errorReplicaChangeLimitInvalid =>
      'Avtomatik dəyişiklik hədləri sıfırdan böyük və dəstəklənən aralıqda olmalıdır.';

  @override
  String get errorReplicaSourceSettling =>
      'Mənbədə yeni dəyişmiş fayl var; avtomatik dərc fayl sabitləşənədək təxirə salındı.';

  @override
  String get errorReplicaChangeLimitExceeded =>
      'Avtomatik dərc təsdiqlənmiş fayl və ya ölçü həddini aşır və təhlükəsizlik üçün dayandırıldı.';

  @override
  String get errorReplicaDisciplineMissing =>
      'Mənbədə CIV, MEC, ELE və PRC qovluqlarının hamısı olmalıdır.';

  @override
  String get errorReplicaScanIncomplete =>
      'Qovluq taraması tamamlanmadı; heç bir fayl yayımlanmadı.';

  @override
  String get errorReplicaEntryLimit =>
      'RFI Sistemi 500000 qeyd təhlükəsizlik limitini keçir.';

  @override
  String get errorReplicaSymlinkRefused =>
      'Simvolik keçid tapıldı; təhlükəsiz yayım dayandırıldı.';

  @override
  String get errorReplicaPathCollision =>
      'Bir neçə fayl eyni nisbi yola uyğun gəlir.';

  @override
  String get errorReplicaFileUnstable =>
      'Fayl tarama zamanı dəyişdi; yenidən önizləyin.';

  @override
  String get errorReplicaApprovedStepInvalid =>
      'Təsdiqlənmiş yayım addımı yanlışdır; yenidən önizləyin.';

  @override
  String get errorReplicaSourceChanged =>
      'Mənbə önizləmədən sonra dəyişdi; yayım dayandırıldı.';

  @override
  String get errorReplicaTargetChanged =>
      'Hədəf önizləmədən sonra dəyişdi; yayım dayandırıldı.';

  @override
  String get errorReplicaBackupExists =>
      'Bu yayım yedəyi artıq mövcuddur və üzərinə yazılmadı.';

  @override
  String get errorReplicaBackupVerificationFailed =>
      'Yedək hash yoxlaması uğursuz oldu; avtomatlaşdırma dayandırıldı.';

  @override
  String get errorReplicaCopyVerificationFailed =>
      'Müvəqqəti nüsxənin hash yoxlaması uğursuz oldu.';

  @override
  String get errorReplicaPublishVerificationFailed =>
      'Yayımdan sonrakı hash yoxlaması uğursuz oldu; recovery tələb olunur.';

  @override
  String get errorReplicaProfileMissing =>
      'Təsdiqlənmiş avtomatik sync profili yoxdur.';

  @override
  String get errorReplicaProfileInvalid =>
      'Saxlanmış avtomatik sync profili yanlışdır.';

  @override
  String get errorReplicaActivationFailed =>
      'Fayllar yayımlandı, lakin avtomatik profil aktiv edilə bilmədi.';

  @override
  String get errorReplicaAutomaticApplyFailed =>
      'Avtomatik yayım tamamlanmadı və dayandırıldı.';

  @override
  String get progressPhaseReplicaDiscovery => 'Üç RFI Sistemi taranır';

  @override
  String get progressPhaseReplicaSourceHash =>
      'Mənbə faylları məhdud çoxnüvəli hash yoxlamasından keçir';

  @override
  String get progressPhaseReplicaTargetHash =>
      'Hədəf faylları məhdud çoxnüvəli hash yoxlamasından keçir';

  @override
  String get prefsAutomation => 'Windows və avtomatlaşdırma';

  @override
  String get prefsAutomationHelp =>
      'Başlanğıcı və əvvəl təsdiqlənmiş fon sync profilini idarə edin.';

  @override
  String get prefsStartWithWindows => 'DocFlow-u Windows ilə başlat';

  @override
  String get prefsReplicaSyncEnabled => 'Təsdiqsiz RFI Sistem sync aktivdir';

  @override
  String get prefsReplicaSyncEnabledHelp =>
      'İstənilən xəta və ya qeyri-müəyyənlik bunu avtomatik söndürür. Yedəklər avtomatik silinmir.';

  @override
  String get prefsReplicaStatusUnconfigured =>
      'Vəziyyət: Hələ təsdiqlənmiş profil yoxdur';

  @override
  String get prefsReplicaStatusDisabled => 'Vəziyyət: Sönülü';

  @override
  String get prefsReplicaStatusIdle => 'Vəziyyət: Növbəti icra gözlənilir';

  @override
  String get prefsReplicaStatusRunning => 'Vəziyyət: İşləyir';

  @override
  String get prefsReplicaStatusWaitingForSignIn => 'Vəziyyət: Giriş gözlənilir';

  @override
  String get prefsReplicaStatusDeferred =>
      'Vəziyyət: Digər əməliyyat bitənədək təxirə salınıb';

  @override
  String get prefsReplicaStatusPaused =>
      'Vəziyyət: Təhlükəsizlik üçün dayandırılıb';

  @override
  String get paramRfiProcessScansCorrectOrientation => 'Səhifə dönüşünü düzəlt';

  @override
  String get paramRfiProcessScansCorrectOrientationHelp =>
      'Aktiv olduqda yalnız istiqaməti etibarlı müəyyən edilən səhifələr döndərilir. Hər hansı səhifə qeyri-müəyyəndirsə, bütün PDF üçün düzəliş buraxılır və fayl olduğu kimi _review-a göndərilir.';

  @override
  String get warnRfiProcessScansOrientationReview =>
      'Səhifə istiqaməti etibarlı müəyyən edilmədi; PDF _review qovluğuna göndərildi';

  @override
  String get progressPhaseScanOrientation =>
      'PDF səhifələrinin istiqaməti yoxlanılır';

  @override
  String get rfiExtractionPages => 'Çıxarılan səhifələr';

  @override
  String get reviewSelectAllKeep => 'Hamısını Saxla';

  @override
  String get reviewSelectAllPut => 'Mümkün Olanları Köçür';

  @override
  String get reviewSelectAllTrash => 'Hamısını Zibilə Köçür';

  @override
  String get opsSearchPlaceholder => 'Əməliyyat axtar…';

  @override
  String get opsCategoryAll => 'Hamısı';

  @override
  String get opsCategoryScans => 'Skanlar və PDF';

  @override
  String get opsCategoryExcel => 'Excel və Jurnallar';

  @override
  String get opsCategoryMail => 'Outlook və Poçt';

  @override
  String get opsCategorySystem => 'Yayımlama və Çap';

  @override
  String opsEmptySearch(Object query) {
    return '\'$query\' ilə uyğun gələn əməliyyat tapılmadı';
  }

  @override
  String get actionOpenDestination => 'Hədəf qovluğu aç';

  @override
  String get opRfiGenerateConcreteDocumentsTitle => 'Beton sənədlərini yarat';

  @override
  String get opRfiGenerateConcreteDocumentsDesc =>
      'Seçilmiş RFI növləri və şablonlarla RFI Log və Follow Up iş kitablarından beton sənədlərini toplu yaradır.';

  @override
  String get paramRfiDocumentsRfiLog => 'RFI Log iş kitabı';

  @override
  String get paramRfiDocumentsFollowUp => 'Follow Up iş kitabı';

  @override
  String get paramRfiDocumentsTemplate010 =>
      '010 şablonu — Follow Up sahələri; temperatur = zavodda təzə qarışığın temperaturu çıxılsın təsadüfi 0,5–0,7';

  @override
  String get paramRfiDocumentsTemplate011 =>
      '011 şablonu — hesabat/məkan və tənzimlənən təsadüfi temperatur aralığı (standart 19–24)';

  @override
  String get paramRfiDocumentsTemplate013 =>
      '013 şablonu — yalnız hesabat/məkan';

  @override
  String get paramRfiDocumentsTemplate014 =>
      '014 şablonu — yalnız hesabat/məkan';

  @override
  String get paramRfiDocumentsOutput => 'Çıxış qovluğu';

  @override
  String get paramRfiDocumentsRfis010 => '010 şablonu üçün RFI nömrələri';

  @override
  String get paramRfiDocumentsRfis011 => '011 şablonu üçün RFI nömrələri';

  @override
  String get paramRfiDocumentsRfis013 => '013 şablonu üçün RFI nömrələri';

  @override
  String get paramRfiDocumentsRfis014 => '014 şablonu üçün RFI nömrələri';

  @override
  String get paramRfiDocumentsWaterRatios =>
      'İstəyə bağlı RFI üzrə su/sement nisbətləri (olmadıqda boş)';

  @override
  String get paramRfiDocumentsAmbientMin => 'Minimum mühit temperaturu';

  @override
  String get paramRfiDocumentsAmbientMax => 'Maksimum mühit temperaturu';

  @override
  String get paramRfiDocumentsAmbientGroups =>
      'RFI qrupları üçün mühit temperaturu diapazonları';

  @override
  String get errorParamRequiredOutputDirectory => 'Çıxış qovluğu tələb olunur';

  @override
  String get errorRfiDocumentsInvalidInput =>
      'Beton sənədi girişləri yanlışdır';

  @override
  String get warnRfiDocumentsOutputExists =>
      'Çıxış faylı artıq mövcuddur; üzərinə yazılmayacaq';

  @override
  String get warnRfiDocumentsFollowUpMissing =>
      'Bu RFI üçün Beton Nümunəsi Testi İzləməsində sətir tapılmadı. Bu RFI ötürüləcək. Tətbiq yalnız mövcud sənədləri yaradacaq; cədvəli düzəldib yenidən baxmaq üçün Ləğv et seçin.';

  @override
  String get warnRfiDocumentsFormNotRequired =>
      'RFI Log-un Forms sahəsində bu RFI üçün CON-CIV-010 göstərilmir; buna görə CON-CIV-010 yaradılmayacaq. İstənilən sənəd növünü və ya RFI Log-u yoxlayıb yenidən önizləyin.';

  @override
  String get paramRfiDocumentsRfiLogHelp =>
      'Civil vərəqi seçilmiş hər qeyd üçün hesabat RFI nömrəsini və yeri verir.';

  @override
  String get paramRfiDocumentsFollowUpHelp =>
      'Yalnız CON-CIV-010 yaradılarkən tələb olunur; beton tökmə sahələrini verir.';

  @override
  String get paramRfiDocumentsOutputHelp =>
      'Nəticələr burada yaradılır; mövcud faylların üzərinə yazılmır.';

  @override
  String get paramRfiDocumentsRfisHelp =>
      'Tam RFI nömrələrini və ya CIV seriya nömrələrini vergüllə və ya yeni sətirlə daxil edin.';

  @override
  String get paramRfiDocumentsWaterRatiosHelp =>
      'İstəyə bağlı: hər sətirdə RFI nömrəsi = nisbət. Göstərilməyən RFI üçün xana boş qalır.';

  @override
  String get paramRfiDocumentsAmbientGroupsHelp =>
      'İstəyə bağlı, hər qrup üçün bir sətir: RFI[, RFI...]=MIN-MAX. Diapazonun hər iki sərhədi daxildir; göstərilməyən RFI-lər üçün standart Minimum/Maksimum mühit temperaturu istifadə olunur.';

  @override
  String get stepRfiGeneratedConcreteDocument => 'Beton sənədini yarat';
}
