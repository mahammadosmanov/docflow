// ignore: unused_import
import 'package:intl/intl.dart' as intl;

import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for Russian (`ru`).
class AppLocalizationsRu extends AppLocalizations {
  AppLocalizationsRu([String locale = 'ru']) : super(locale);

  @override
  String get appTitle => 'DocFlow';

  @override
  String get actionPreview => 'Предпросмотр';

  @override
  String get actionApply => 'Применить';

  @override
  String get actionUndo => 'Отменить';

  @override
  String get actionCancel => 'Отмена';

  @override
  String get actionBack => 'Назад';

  @override
  String get actionBrowse => 'Обзор';

  @override
  String get uiWarnings => 'Предупреждения';

  @override
  String resultApplied(int count) {
    return 'Применено: $count';
  }

  @override
  String resultFailed(int count) {
    return 'Не выполнено: $count';
  }

  @override
  String get actionOpen => 'Открыть';

  @override
  String get actionReview => 'Папка проверки';

  @override
  String get reviewTitle => 'Проверка файлов';

  @override
  String get reviewEmpty => 'Папка проверки пуста';

  @override
  String get reviewKeep => 'Оставить как есть';

  @override
  String get reviewPut => 'Переместить в целевую папку';

  @override
  String get reviewDelete => 'Переместить в корзину';

  @override
  String get reviewNoCode => 'Нет номера RFI';

  @override
  String get reviewWillReplace => 'заменит существующий файл';

  @override
  String get noteDeleted => 'Удалено (в корзину)';

  @override
  String get noteDisplaced => 'Заменено — перемещено в корзину';

  @override
  String get actionHistory => 'История';

  @override
  String get historyTitle => 'История';

  @override
  String get historyUnknownOperation => 'Неизвестная операция';

  @override
  String get historyEmpty => 'Истории пока нет';

  @override
  String historyMoves(int count) {
    return 'Файлов: $count';
  }

  @override
  String get historyUndoOriginal => 'Отменить в исходное место';

  @override
  String get historyUndoToFolder => 'Отменить в папку…';

  @override
  String get planEmpty => 'Нечего делать';

  @override
  String planSummary(int count) {
    return 'Будет обработано файлов: $count';
  }

  @override
  String get planImpactTitle => 'Перед применением';

  @override
  String get planImpactPreviewOnly =>
      'Пока ничего не изменено. Будет выполнен только этот утвержденный план.';

  @override
  String get planImpactOutputs => 'Новые файлы';

  @override
  String get planImpactUpdates => 'Обновление файлов';

  @override
  String get planImpactMoves => 'Перемещение / имя';

  @override
  String get planImpactPrintJobs => 'Задания печати';

  @override
  String get planImpactDrafts => 'Открытие черновиков';

  @override
  String get planImpactCleanup => 'Очистка';

  @override
  String get planImpactWarnings => 'Предупреждения';

  @override
  String get scanIndexTitle => 'Инкрементальный индекс сканирования';

  @override
  String get scanIndexHelp =>
      'Неизмененные файлы повторно использованы из устойчивого локального индекса.';

  @override
  String get scanIndexForceApplied =>
      'Для этого запуска явно выбрано полное сканирование.';

  @override
  String get scanIndexDiscovered => 'Найдено';

  @override
  String get scanIndexReused => 'Из индекса';

  @override
  String get scanIndexAnalyzed => 'Проверено сейчас';

  @override
  String get scanIndexFailed => 'Ошибки';

  @override
  String get scanIndexInterrupted => 'Ранее прерванные запуски';

  @override
  String get planWarningsSearch => 'Поиск предупреждений';

  @override
  String get planWarningsType => 'Тип предупреждения';

  @override
  String get planWarningsAll => 'Все типы предупреждений';

  @override
  String get planWarningsFocus => 'Показать связанную страницу';

  @override
  String get planWarningsNone =>
      'Нет предупреждений, соответствующих фильтрам.';

  @override
  String planWarningsShowing(Object count, Object shown) {
    return 'Показано $shown из $count предупреждений';
  }

  @override
  String get planWorkbookChangesTitle => 'Точные изменения ячеек Excel';

  @override
  String planWorkbookChangesCount(Object count) {
    return 'Одобрено изменений ячеек: $count';
  }

  @override
  String get planWorkbookChangesHelp =>
      'Разверните, чтобы сравнить текущее значение с точным значением после применения.';

  @override
  String get planWorkbookChangesWorkbook => 'Книга';

  @override
  String get planWorkbookChangesSheetCell => 'Лист и ячейка';

  @override
  String get planWorkbookChangesRfi => 'RFI';

  @override
  String get planWorkbookChangesBefore => 'Сейчас';

  @override
  String get planWorkbookChangesAfter => 'После применения';

  @override
  String get planWorkbookChangesBlank => '(пусто)';

  @override
  String get planWorkbookChangesKindRfiLog => 'RFI Log';

  @override
  String get planWorkbookChangesKindFollowUp => 'Follow Up';

  @override
  String get planWorkbookChangesKindWorkbook => 'Книга';

  @override
  String planWorkbookChangesLimit(Object count) {
    return 'Предпросмотр ограничен первыми $count одобренными изменениями ячеек.';
  }

  @override
  String planExactTitle(Object count) {
    return 'Подтвержденных действий: $count';
  }

  @override
  String get planExactHelp =>
      'Откройте для проверки точных исходных и целевых путей';

  @override
  String get opRfiProcessScansTitle => 'Обработать сканы';

  @override
  String get opRfiProcessScansFastCoversTitle => 'Быстрое сканирование RFI';

  @override
  String get opRfiProcessScansFastCoversDesc =>
      'Находит обложки RFI в многостраничных сканах. Выберите режим «Несколько RFI», чтобы разделить скан, или «Один RFI», чтобы прочитать только первую страницу и сохранить PDF целиком. Распознанное OCR значение CLV заменяется на CIV. Неуверенные сканы помещаются в _review.';

  @override
  String get paramRfiProcessScansScanMode => 'Режим сканирования';

  @override
  String get paramRfiProcessScansScanModeHelp =>
      'Режим «Несколько RFI» разделяет объединённый скан. Режим «Один RFI» читает только первую страницу и сохраняет PDF целиком.';

  @override
  String get choiceRfiProcessScansScanModeMultiRfi => 'Разделить несколько RFI';

  @override
  String get choiceRfiProcessScansScanModeSingleRfi =>
      'Один RFI — читать только первую страницу';

  @override
  String get rfiFastScanEditorTitle => 'Проверка и исправление номеров RFI';

  @override
  String rfiFastScanEditorRowCount(int count) {
    return 'Файлов сканирования: $count';
  }

  @override
  String get rfiFastScanColumnSourceFile => 'Исходный файл';

  @override
  String get rfiFastScanColumnEvidence => 'Распознавание';

  @override
  String get rfiFastScanColumnDetectedCode => 'Обнаруженный RFI';

  @override
  String get rfiFastScanColumnCode => 'Номер RFI';

  @override
  String get rfiFastScanColumnTarget => 'Назначение';

  @override
  String get rfiFastScanEvidenceClvToCiv => 'Подстановка OCR: CLV → CIV';

  @override
  String get rfiFastScanEvidenceConfirmed => 'Подтверждено';

  @override
  String get rfiFastScanEditorUpdatePreview => 'Обновить предпросмотр';

  @override
  String get rfiFastScanEditorUpdatingPreview => 'Обновление предпросмотра…';

  @override
  String get rfiFastScanEditorPreviewUpdated =>
      'Предпросмотр обновлён. Проверьте целевые пути перед применением.';

  @override
  String get errorRfiFastScanCodeInvalid =>
      'Введите полный код поддерживаемого RFI с суффиксом из 1–4 ASCII-цифр; код сохраняется с четырьмя цифрами.';

  @override
  String get errorRfiFastScanTargetExists =>
      'Другой скан уже использует этот номер RFI или целевой файл существует.';

  @override
  String get errorRfiFastScanRevisionInvalid =>
      'Изменённый предпросмотр сканирования недопустим. Проверьте номера RFI и повторите попытку.';

  @override
  String get opRfiGroupFilesTitle => 'Сгруппировать файлы по RFI';

  @override
  String get opRfiGroupFilesDesc =>
      'Находит точные номера RFI в именах и содержимом файлов и копирует одобренные файлы в папки <дисциплина>/<полный код RFI>.';

  @override
  String get paramRfiFileGroupSource => 'Папка входящих файлов';

  @override
  String get paramRfiFileGroupSourceHelp =>
      'Проверяются только файлы верхнего уровня; вложенные папки пропускаются без изменений.';

  @override
  String get paramRfiFileGroupDestination =>
      'Папка сгруппированного результата';

  @override
  String get paramRfiFileGroupDestinationHelp =>
      'Выберите пустую или отдельную папку вне источника. Существующие файлы не перезаписываются.';

  @override
  String get rfiFileGroupEditorTitle => 'Найденные соответствия RFI';

  @override
  String rfiFileGroupEditorRowCount(Object count) {
    return 'Соответствий для копирования: $count';
  }

  @override
  String get rfiFileGroupColumnEnabled => 'Копировать';

  @override
  String get rfiFileGroupColumnRfiCode => 'Номер RFI';

  @override
  String get rfiFileGroupColumnSourceFile => 'Исходный файл';

  @override
  String get rfiFileGroupColumnMethod => 'Способ определения';

  @override
  String get rfiFileGroupMethodFilename => 'Имя файла';

  @override
  String get rfiFileGroupMethodPdf => 'PDF / OCR';

  @override
  String get rfiFileGroupMethodMail => 'Содержимое письма';

  @override
  String get rfiFileGroupMethodOffice => 'Содержимое Office';

  @override
  String get rfiFileGroupMethodContent => 'Содержимое файла';

  @override
  String get noteRfiFileGroupFolderCreated => 'Будет создана папка RFI';

  @override
  String get noteRfiFileGroupCopy =>
      'Файл будет скопирован в папку дисциплины и полного кода RFI';

  @override
  String get noteRfiFileGroupReviewCopy =>
      'Файл будет скопирован в папку проверки';

  @override
  String get warnRfiFileGroupNoCode =>
      'Номер RFI не найден; файл будет направлен на проверку';

  @override
  String get warnRfiFileGroupUnreadable =>
      'Файл не удалось полностью прочитать; он будет направлен на проверку';

  @override
  String get warnRfiFileGroupPartialInspection =>
      'Совпадения в имени файла доступны, но содержимое не удалось полностью проверить';

  @override
  String get warnRfiFileGroupExists =>
      'Целевой файл уже существует и не будет перезаписан';

  @override
  String get warnRfiFileGroupNoFiles =>
      'В исходной папке нет файлов верхнего уровня';

  @override
  String get warnRfiFileGroupMissingEmail =>
      'Для этого RFI не найден файл электронной почты EML или MSG; связанные файлы не будут размещены';

  @override
  String get errorParamRequiredDestination => 'Требуется папка назначения';

  @override
  String get paramSource => 'Исходная папка';

  @override
  String get paramSourceHelp => 'Папка со сканированными PDF';

  @override
  String get paramDest => 'Целевая папка';

  @override
  String get paramDestHelp => 'Где будут сохранены обработанные PDF-файлы RFI';

  @override
  String get paramKeepOriginal =>
      'Сохранить оригиналы (копировать, не перемещать)';

  @override
  String get noteRenamed => 'Переименовано';

  @override
  String get noteNeedsReview => 'Будет перемещено на проверку';

  @override
  String get noteUnreadable => 'Не читается — будет перемещено на проверку';

  @override
  String get warnNoCode => 'Номер RFI не найден';

  @override
  String get warnLowConfidence => 'Низкая уверенность — нужна проверка';

  @override
  String get warnUnknownRfiFolder =>
      'Обнаруженная папка RFI не существует — перемещено на проверку';

  @override
  String get warnUnreadable => 'Не удалось прочитать файл';

  @override
  String get warnRevisionGuessed => 'Номер ревизии определён предположительно';

  @override
  String get warnDuplicate =>
      'Повторяющийся номер RFI — перемещено на проверку';

  @override
  String get warnExists =>
      'Целевой файл уже существует — перемещено на проверку (без перезаписи)';

  @override
  String get warnNoPdfs => 'В папке нет PDF';

  @override
  String get errorStepFailed => 'Шаг не выполнен';

  @override
  String get errorUndoFailed => 'Не удалось отменить';

  @override
  String get errorParamRequiredSource => 'Исходная папка обязательна';

  @override
  String get errorParamRequiredDest => 'Целевая папка обязательна';

  @override
  String get actionAdmin => 'Панель администратора';

  @override
  String get actionRetry => 'Проверить снова';

  @override
  String get bootstrapFatalTitle => 'Не удалось запустить DocFlow';

  @override
  String get bootstrapRetry => 'Повторить';

  @override
  String get authEmail => 'Эл. почта';

  @override
  String get authPassword => 'Пароль';

  @override
  String get authSignIn => 'Войти';

  @override
  String get authSignUp => 'Создать аккаунт';

  @override
  String get authSignOut => 'Выйти';

  @override
  String get authSignupOk =>
      'Аккаунт создан. Попросите администратора одобрить его, затем войдите.';

  @override
  String get authPending => 'Ожидание одобрения';

  @override
  String get authPendingHelp =>
      'Аккаунт создан, но ещё не одобрен. Администратор должен предоставить доступ.';

  @override
  String get adminTitle => 'Пользователи';

  @override
  String get adminAllowed => 'Доступ';

  @override
  String get adminAdmin => 'Администратор';

  @override
  String get adminEmpty => 'Пользователей пока нет';

  @override
  String get actionDone => 'Готово';

  @override
  String get adminHelp =>
      'Включите Доступ, чтобы разрешить пользоваться приложением. Ваши переключатели заблокированы — вы не сможете закрыть доступ себе.';

  @override
  String get adminNoAccess => 'нет доступа';

  @override
  String get adminYou => 'вы';

  @override
  String get authSubtitle => 'Войдите, чтобы продолжить';

  @override
  String get authShow => 'Показать пароль';

  @override
  String get authHide => 'Скрыть пароль';

  @override
  String get authPasswordHint => 'Не менее 6 символов';

  @override
  String get historyEmptyHelp =>
      'Применённые операции появятся здесь, и их можно будет отменить.';

  @override
  String get historyUndone => 'Отменено';

  @override
  String get historyLocalJournalRequired =>
      'Для отмены требуется локальный журнал на этом устройстве.';

  @override
  String get historyCloud => 'Облако';

  @override
  String get historyLocal => 'Локально';

  @override
  String get historySourceCloud =>
      'Общая история, видна на всех ваших компьютерах';

  @override
  String get historySourceLocal =>
      'Только локальная история — облако недоступно';

  @override
  String get opsHeading => 'Операции';

  @override
  String get opsSubheading =>
      'Выберите операцию. Никакие файлы не изменятся, пока вы не проверите и не одобрите план.';

  @override
  String get homeTitle => 'Рабочее пространство';

  @override
  String get homeSubtitle =>
      'Операции с документами и все вспомогательные инструменты в одном месте.';

  @override
  String get homePreviewFirst => 'Сначала предпросмотр';

  @override
  String get ocrConfidence => 'Оценка доказательности OCR';

  @override
  String get ocrConfidenceShort => 'Доказательство';

  @override
  String get ocrConfidenceHelp =>
      'Эвристическая оценка согласованности OCR из 100. Это не вероятность точности, подтверждение или автоматическое одобрение.';

  @override
  String get homeFeatures => 'Другие функции';

  @override
  String get homeFeaturesHelp =>
      'Проверка, история, аккаунт и управление приложением.';

  @override
  String get homeReviewDesc =>
      'Проверяйте ожидающие файлы по отдельности и безопасно размещайте их.';

  @override
  String get homeHistoryDesc =>
      'Просматривайте выполненные операции и безопасно отменяйте их.';

  @override
  String get homeSettingsDesc =>
      'Управляйте языком, видом, текстом и настройками аккаунта.';

  @override
  String get homeAdminDesc =>
      'Управляйте доступом пользователей и правами администратора.';

  @override
  String get paramHintFolder => 'Выберите папку';

  @override
  String get paramHintFile => 'Выберите файл';

  @override
  String get planApplying => 'Применение';

  @override
  String get planReadOnly => 'Пока ничего не изменено';

  @override
  String get prefsTitle => 'Настройки';

  @override
  String get prefsLanguage => 'Язык';

  @override
  String get prefsLangSystem => 'Язык системы';

  @override
  String get prefsLangTr => 'Türkçe';

  @override
  String get prefsLangEn => 'English';

  @override
  String get prefsLangRu => 'Русский';

  @override
  String get prefsLangAz => 'Azərbaycanca';

  @override
  String get prefsTheme => 'Тема';

  @override
  String get prefsThemeSystem => 'Системная';

  @override
  String get prefsThemeLight => 'Светлая';

  @override
  String get prefsThemeDark => 'Тёмная';

  @override
  String get prefsFont => 'Шрифт';

  @override
  String get prefsFontHelp =>
      'Используются шрифты, уже установленные на этом ПК — ничего не загружается.';

  @override
  String get prefsFontSystem => 'По умолчанию';

  @override
  String get prefsFontSans => 'Без засечек';

  @override
  String get prefsFontSerif => 'С засечками';

  @override
  String get prefsFontMono => 'Моноширинный';

  @override
  String get prefsFontSize => 'Размер текста';

  @override
  String get prefsFontSizeSample =>
      'AZE-SUN-INT-QMS-RFI-CIV-0064 — пример текста этого размера.';

  @override
  String get prefsDensity => 'Интервалы';

  @override
  String get prefsDensityCompact => 'Компактный вид';

  @override
  String get prefsDensityHelp =>
      'Больше помещается на экране. Полезно на маленьких экранах.';

  @override
  String get prefsSupportTitle => 'Расширенная поддержка';

  @override
  String get prefsSupportHelp =>
      'Создайте локальный пакет поддержки только для чтения с удалёнными конфиденциальными данными.';

  @override
  String get prefsSupportPrivacy =>
      'Включает версию, платформу, состояние sidecar/OCR, последние коды ошибок и последний результат очереди печати без путей; содержимое документов, учётные данные, токены и приватные пути исключены. Ничего никуда не отправляется.';

  @override
  String get prefsSupportPreview => 'Предпросмотр пакета поддержки';

  @override
  String get prefsSupportPreviewLabel => 'Предпросмотр пакета поддержки';

  @override
  String prefsSupportSize(Object count) {
    return '$count байт';
  }

  @override
  String get prefsSupportCopy => 'Копировать пакет в буфер обмена';

  @override
  String get prefsSupportCopied => 'Пакет поддержки скопирован в буфер обмена.';

  @override
  String get prefsSupportDownload => 'Скачать пакет поддержки';

  @override
  String get prefsSupportDownloaded => 'Пакет поддержки скачан.';

  @override
  String get paramDropFiles =>
      'Перетащите файлы сюда или нажмите для добавления';

  @override
  String get paramDropFolder => 'Перетащите папку сюда или выберите';

  @override
  String get paramDropFile => 'Перетащите файл сюда или выберите';

  @override
  String get actionRecent => 'Недавние пути';

  @override
  String get actionToggleTheme =>
      'Переключить тему (Светлая / Тёмная / Системная)';

  @override
  String get filterSearchPlaceholder =>
      'Поиск (номер RFI, статус, описание)...';

  @override
  String get filterAll => 'Все';

  @override
  String get filterChanges => 'Изменения';

  @override
  String get filterManualReview => 'Ручная проверка';

  @override
  String tableFilterCount(Object filtered, Object total) {
    return 'Показано $filtered из $total строк';
  }

  @override
  String get prefsReset => 'Сбросить по умолчанию';

  @override
  String get reviewHelp =>
      'Решите, что сделать с каждым файлом. Ничего не изменится, пока вы не одобрите план.';

  @override
  String get reviewEmptyHelp => 'Нет файлов, ожидающих проверки.';

  @override
  String get reviewNoChanges => 'Ничего не выбрано';

  @override
  String reviewSelected(Object count) {
    return 'Выбрано: $count';
  }

  @override
  String reviewToTrash(Object count) {
    return '$count в корзину';
  }

  @override
  String get reviewTrashHint =>
      'Удалённые файлы попадают в восстановимую корзину, а не удаляются навсегда.';

  @override
  String get stepForm => 'Настройка';

  @override
  String get stepPreview => 'Просмотр';

  @override
  String get stepApply => 'Применить';

  @override
  String get actionSave => 'Сохранить';

  @override
  String get authUsername => 'Имя пользователя';

  @override
  String get authUsernameHelp =>
      '3-24 символа: буквы, цифры, точка, дефис, подчёркивание';

  @override
  String get authIdentifier => 'Имя пользователя или эл. почта';

  @override
  String get authIdentifierHelp => 'Подходит любое';

  @override
  String get authEmailSignInHelp =>
      'Вход по имени пользователя недоступен. Используйте адрес электронной почты своей учётной записи.';

  @override
  String get authSubtitleUp => 'Создайте аккаунт, чтобы начать';

  @override
  String get authErrCredentials =>
      'Неверное имя пользователя/почта или пароль.';

  @override
  String get authErrUsernameTaken => 'Это имя пользователя занято.';

  @override
  String get authErrUsernameInvalid =>
      'Имя пользователя: 3-24 символа — буквы, цифры, точка, дефис, подчёркивание.';

  @override
  String get authErrEmailTaken => 'Аккаунт с этой почтой уже существует.';

  @override
  String get prefsAccount => 'Аккаунт';

  @override
  String get prefsAccountEmailFixed => 'нельзя изменить';

  @override
  String get prefsAccountSaved => 'Сохранено';

  @override
  String get authErrEmailNotConfirmed =>
      'Аккаунт существует, но почта ещё не подтверждена. Администратор может отключить подтверждение почты, либо откройте ссылку из письма.';

  @override
  String get authErrRateLimit =>
      'Слишком много попыток. Подождите несколько минут и повторите.';

  @override
  String get authErrWeakPassword =>
      'Пароль слишком простой. Используйте не менее 6 символов.';

  @override
  String get warnNoOcr =>
      'Движок OCR (Tesseract) не найден — файлы не тронуты. Установите его или переустановите приложение.';

  @override
  String get noteFolderCreated => 'Новая папка';

  @override
  String get warnOcrBroken =>
      'OCR не сработал ни на одном файле — это проблема установки, а не файлов. Ничего не тронуто. Подробности:';

  @override
  String get opFilesBatchPrintTitle => 'Печать нескольких файлов';

  @override
  String get opFilesBatchPrintDesc =>
      'Создаёт одну упорядоченную системную очередь печати из PDF, документов Office и поддерживаемых изображений.';

  @override
  String get paramPrintFiles => 'Файлы для печати';

  @override
  String get paramPrintFilesHelp =>
      'Добавляйте файлы разных типов, меняйте порядок или удаляйте их. Порядок на экране — это порядок печати.';

  @override
  String get paramPrintRfiRoot => 'Папка системы RFI';

  @override
  String get paramPrintRfiRootHelp =>
      'Поиск в этой папке и вложенных папках без изменения исходных файлов.';

  @override
  String get paramPrintRfiNumbers => 'Номера RFI';

  @override
  String get paramPrintRfiNumbersHelp =>
      'Введите полный код RFI, код дисциплины и номера, например CIV-1319, или число от 1 до 4 цифр в каждой строке; короткий номер дополняется до четырёх цифр.';

  @override
  String get paramPrintIncludeFirstPage => 'Документы First Page';

  @override
  String get paramPrintIncludeSketch => 'Документы Sketch';

  @override
  String get printRfiTitle => 'Поиск документов RFI';

  @override
  String get printRfiInputs => 'Включить запросы RFI';

  @override
  String get printRfiRootRequired => 'Сначала выберите папку системы RFI.';

  @override
  String get printRfiInputRequired =>
      'Перед поиском выберите хотя бы один запрос RFI.';

  @override
  String get printRfiTypesRequired =>
      'Перед поиском выберите First Page, Sketch или оба типа.';

  @override
  String get printRfiSearch => 'Искать в системе RFI';

  @override
  String get printRfiSearching => 'Поиск…';

  @override
  String printRfiFound(Object count) {
    return 'Найдено документов: $count';
  }

  @override
  String get printRfiAmbiguous =>
      'Этот номер встречается в нескольких дисциплинах. Выберите нужные файлы явно.';

  @override
  String get printRfiMissingFirstPage => 'Документ First Page не найден.';

  @override
  String get printRfiMissingSketch => 'Документ Sketch не найден.';

  @override
  String get printRfiToggleRequest => 'Переключить этот RFI';

  @override
  String get printRfiSelectTypeFirstPage => 'Выбрать First Page';

  @override
  String get printRfiSelectTypeSketch => 'Выбрать Sketch';

  @override
  String get printRfiClearTypeFirstPage => 'Снять First Page';

  @override
  String printRfiSelectDiscipline(Object discipline) {
    return 'Выбрать дисциплину $discipline';
  }

  @override
  String get printRfiClearTypeSketch => 'Снять Sketch';

  @override
  String get printRfiTypeFirstPage => 'First Page';

  @override
  String get printRfiTypeSketch => 'Sketch';

  @override
  String get printRfiNoMatches => 'Подходящие документы не найдены.';

  @override
  String get printRfiTruncated =>
      'Достигнут безопасный предел поиска. Некоторые папки могли остаться непроверенными.';

  @override
  String get actionSelectAll => 'Выбрать все';

  @override
  String get actionSelectNone => 'Снять выбор';

  @override
  String get errorPrintRfiInvalidRequest =>
      'Проверьте запросы RFI и выбранные типы документов.';

  @override
  String get errorPrintRfiRootEscape =>
      'Найденный файл изменился за пределами выбранной системы RFI. Выполните поиск снова.';

  @override
  String get errorPrintRfiChanged =>
      'Найденный файл изменился во время поиска. Выполните поиск снова.';

  @override
  String get paramPrintPrinter => 'Принтер';

  @override
  String get paramPrintPrinterHelp =>
      'Использует принтеры, установленные на этом компьютере.';

  @override
  String get paramPrintCopies => 'Количество копий';

  @override
  String get paramPrintCopiesHelp => 'От 1 до 99 копий очереди.';

  @override
  String get paramPrintCollate => 'Разобрать по копиям';

  @override
  String get paramPrintCollateHelp =>
      'Печатает весь упорядоченный комплект перед началом следующей копии.';

  @override
  String get errorParamRequiredFiles => 'Выберите хотя бы один файл';

  @override
  String get errorParamRequiredPrinter => 'Выберите принтер';

  @override
  String get errorParamRequiredCopies => 'Укажите количество копий';

  @override
  String get actionAddFiles => 'Добавить файлы';

  @override
  String get actionClear => 'Очистить';

  @override
  String get actionMoveUp => 'Переместить вверх';

  @override
  String get actionMoveDown => 'Переместить вниз';

  @override
  String get actionRemove => 'Удалить';

  @override
  String get actionDecrease => 'Уменьшить';

  @override
  String get actionIncrease => 'Увеличить';

  @override
  String get paramHintFiles => 'Файлы не выбраны';

  @override
  String printFilesSelected(Object count) {
    return 'Выбрано файлов: $count';
  }

  @override
  String get notePrintQueued => 'Готово к печати';

  @override
  String get warnPrintNoPrinters => 'Принтеры не найдены';

  @override
  String get warnPrintPrinterUnavailable =>
      'Выбранный принтер больше недоступен';

  @override
  String get warnPrintInvalidCopies =>
      'Количество копий должно быть от 1 до 99';

  @override
  String get warnPrintTooManyFiles =>
      'Очередь может содержать не более 250 файлов';

  @override
  String get warnPrintFileMissing => 'Файл отсутствует и будет пропущен';

  @override
  String get warnPrintWorkbookDiscoveryFailed =>
      'Не удалось безопасно проверить книгу, поэтому она не была добавлена в порядок печати';

  @override
  String get warnPrintWorkbookTooManyWorksheets =>
      'Книга содержит слишком много листов';

  @override
  String get warnPrintExpandedOrderTooLarge =>
      'Расширенный порядок печати может содержать не более 250 заданий';

  @override
  String get warnPrintWorkbookUnsafeXlm =>
      'Книга отклонена, потому что содержит макролист Excel 4.0';

  @override
  String get warnPrintWorkbookUnsafeProtection =>
      'Книга отклонена, потому что её структура защищена';

  @override
  String get warnPrintWorkbookUnsafeExternalLinks =>
      'Книга отклонена, потому что содержит внешние ссылки';

  @override
  String get warnPrintWorkbookUnsafeConnections =>
      'Книга отклонена, потому что содержит внешние подключения данных';

  @override
  String get warnPrintWorkbookUnsafeQueryTables =>
      'Книга отклонена, потому что содержит таблицы запросов';

  @override
  String get warnPrintNoHandler =>
      'Для этого типа файла нет системного обработчика печати; установите или выберите приложение по умолчанию, которое умеет печатать';

  @override
  String get warnPrintUnsupportedOptions =>
      'Выбранный принтер или системное приложение не поддерживает эти параметры печати';

  @override
  String get warnPrintNonUndoable =>
      'Печать нельзя отменить. Файлы запускаются строго по одному; если приложение запрашивает подтверждение, очередь ждёт перед запуском следующего файла.';

  @override
  String get errorPrintInvalidPlan =>
      'План печати недействителен. Повторно откройте предварительный просмотр.';

  @override
  String get errorPrintFailed => 'Не удалось отправить на печать';

  @override
  String get errorPrintQueueStopped =>
      'Строгий порядок печати сохранён; оставшиеся файлы не были отправлены';

  @override
  String get errorPrintPageRangeInvalid =>
      'Введите all или список страниц и диапазонов через запятую, например 1-3,5.';

  @override
  String get errorPrintPageRangeBounds =>
      'Выбранный диапазон превышает количество страниц в этом PDF.';

  @override
  String get errorPrintPageRangeImageUnsupported =>
      'Для изображений допустимы только all или страница 1.';

  @override
  String get errorPrintPageRangeNativeUnsupported =>
      'Для файлов, печатаемых нативным приложением, допустимо только значение all.';

  @override
  String get errorPrintWorksheetPageRangeAllOnly =>
      'Для выбранных листов допустимо только значение all.';

  @override
  String get errorPrintWorkbookChanged =>
      'Книга изменилась после предпросмотра. Повторно откройте предпросмотр порядка печати.';

  @override
  String get errorPrintWorksheetChanged =>
      'Структура листов изменилась после предпросмотра. Повторно откройте предпросмотр порядка печати.';

  @override
  String get errorPrintExcelUnavailable =>
      'Экспорт листов Excel недоступен на этом компьютере.';

  @override
  String get errorPrintExcelExportTimedOut =>
      'Время экспорта листа Excel истекло до начала печати.';

  @override
  String get errorPrintExcelExportCancelled =>
      'Экспорт листа Excel отменён до начала печати.';

  @override
  String get errorPrintExcelExportQuarantined =>
      'Экспорт Excel отключён до перезапуска DocFlow, потому что предыдущий процесс Excel не удалось безопасно закрыть.';

  @override
  String get errorPrintExportCleanupRequiresReview =>
      'Не удалось подтвердить очистку приватного экспорта Excel. Перезапустите DocFlow и проверьте временное состояние перед повтором.';

  @override
  String get errorPrintExcelSourceMotwBlocked =>
      'Книга заблокирована: данные Windows о зоне загрузки небезопасны, повреждены, слишком велики или недоступны для чтения.';

  @override
  String get errorPrintExcelWorkbookUnsafe =>
      'Книга требует небезопасной обработки защиты, ссылок, подключений, запросов или макросов Excel 4.0 и не была открыта.';

  @override
  String get errorPrintExcelComContractInvalid =>
      'Утверждённый запрос автоматизации Excel недействителен, поэтому книга не была открыта.';

  @override
  String get errorPrintExcelComFailed =>
      'Автоматизация Excel завершилась ошибкой до начала печати.';

  @override
  String get errorPrintExcelExportDestinationExists =>
      'Приватный PDF выбранного листа уже существует, поэтому экспорт остановлен.';

  @override
  String get errorPrintExcelExportFailed =>
      'Excel не удалось экспортировать выбранный лист в PDF.';

  @override
  String get errorPrintExcelExportMalformedPdf =>
      'Excel создал пустой или недействительный PDF листа, поэтому печать не выполнялась.';

  @override
  String get errorPrintExcelPrinterContextUnavailable =>
      'Windows не удалось создать утверждённый контекст принтера для листа Excel.';

  @override
  String get errorPrintExcelPrintJobRejected =>
      'Windows отклонила контролируемое задание печати листа Excel до начала отправки.';

  @override
  String get errorPrintExcelPrintPreparationFailed =>
      'Windows не удалось подготовить лист Excel к контролируемой печати.';

  @override
  String get errorPrintExcelPrintOptionsRejected =>
      'Принтер Windows отклонил утверждённые параметры печати листа Excel.';

  @override
  String get errorPrintExcelPrintCancelledUnconfirmed =>
      'Не удалось подтвердить отмену после передачи задания печати листа Excel в очередь. Проверьте принтер перед повтором.';

  @override
  String get errorPrintExcelPrintCompletionUnconfirmed =>
      'Windows не удалось подтвердить завершение после передачи задания листа Excel в очередь печати. Проверьте принтер перед повтором.';

  @override
  String get errorPrintExcelControlledPdfUnavailable =>
      'Контролируемая печать PDF для листов Excel недоступна в этой сборке.';

  @override
  String get errorPrintExcelWorkerStartFailed =>
      'Не удалось запустить изолированный процесс экспорта Excel.';

  @override
  String get errorPrintExcelDeadlineInvalid =>
      'Недопустимый тайм-аут экспорта Excel.';

  @override
  String get errorPrintExcelWorksheetMetadataMissing =>
      'Отсутствуют утверждённые данные листа. Повторно откройте предварительный просмотр порядка печати.';

  @override
  String get errorPrintExcelSafetyMetadataMissing =>
      'Отсутствуют утверждённые данные безопасности книги. Повторно откройте предварительный просмотр порядка печати.';

  @override
  String get errorPrintExcelGenerationExhausted =>
      'Экспорт Excel не может безопасно продолжаться до перезапуска DocFlow.';

  @override
  String get errorPrintExcelWorkerStopped =>
      'Изолированный процесс экспорта Excel неожиданно остановился.';

  @override
  String get errorPrintExcelResponseStale =>
      'Устаревший ответ экспорта Excel отклонён до печати.';

  @override
  String get errorPrintExcelStagingUnavailable =>
      'Закрытая область подготовки книги недоступна.';

  @override
  String get errorPrintExcelSourceNameUnavailable =>
      'Имя утверждённого файла книги недоступно.';

  @override
  String get errorPrintExcelStagingFailed =>
      'Не удалось скопировать книгу в закрытую область подготовки.';

  @override
  String get errorPrintExcelStagingVerificationFailed =>
      'Не удалось сверить закрытую копию книги с утверждённым источником.';

  @override
  String get errorPrintExcelPrivatePdfUnavailable =>
      'Закрытый PDF выбранного листа недоступен.';

  @override
  String get errorPrintExcelPrivatePdfEmpty =>
      'Excel создал пустой PDF листа; печать не выполнялась.';

  @override
  String get printWaitingForCurrent =>
      'Ожидание завершения этого файла перед запуском следующего';

  @override
  String get paramPrintPaper => 'Размер бумаги';

  @override
  String get paramPrintPaperHelp =>
      'Начальная настройка для всех файлов; её можно изменить для каждого файла в предпросмотре.';

  @override
  String get paramPrintOrientation => 'Ориентация';

  @override
  String get paramPrintScaling => 'Масштаб страницы';

  @override
  String get paramPrintColor => 'Цветовой режим';

  @override
  String get paramPrintDuplex => 'Двусторонняя печать';

  @override
  String get paramPrintRetries => 'Автоматические повторы';

  @override
  String get paramPrintRetriesHelp =>
      'Повторяет неудачное задание 0-3 раза перед применением правила очереди.';

  @override
  String get paramPrintOnError => 'Если задание снова не удалось';

  @override
  String get paramPrintOnErrorHelp =>
      'Пауза сохраняет порядок; Пропустить продолжает очередь и сохраняет задание для повтора.';

  @override
  String get paramPrintTimeout => 'Тайм-аут задания (минуты)';

  @override
  String get paramPrintTimeoutHelp =>
      'Прекращает ожидание, если принтер заблокирован, отключён или не отвечает.';

  @override
  String get choicePrintPaperAuto => 'Настройка принтера';

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
  String get choicePrintOrientationAuto => 'Автоматически по странице';

  @override
  String get choicePrintOrientationPortrait => 'Книжная';

  @override
  String get choicePrintOrientationLandscape => 'Альбомная';

  @override
  String get choicePrintScalingFit => 'Вписать в область печати';

  @override
  String get choicePrintScalingActual => 'Фактический размер';

  @override
  String get choicePrintScalingShrink => 'Уменьшать только большие страницы';

  @override
  String get choicePrintColorPrinter => 'Настройка принтера';

  @override
  String get choicePrintColorColor => 'Цвет';

  @override
  String get choicePrintColorGrayscale => 'Оттенки серого';

  @override
  String get choicePrintDuplexPrinter => 'Настройка принтера';

  @override
  String get choicePrintDuplexSimplex => 'Односторонняя';

  @override
  String get choicePrintDuplexLong => 'Двусторонняя, длинный край';

  @override
  String get choicePrintDuplexShort => 'Двусторонняя, короткий край';

  @override
  String get choicePrintOnErrorPause => 'Поставить очередь на паузу';

  @override
  String get choicePrintOnErrorSkip => 'Пропустить и продолжить';

  @override
  String get printEditorTitle => 'Проверка и настройка очереди печати';

  @override
  String printEditorRowCount(Object count) {
    return '$count файлов/заданий';
  }

  @override
  String get printColInclude => 'Печать';

  @override
  String get printColStatus => 'Статус';

  @override
  String get printColFile => 'Файл';

  @override
  String get printColFormat => 'Формат';

  @override
  String get printColWorksheet => 'Лист';

  @override
  String get printColWorksheetVisibility => 'Видимость';

  @override
  String get printWorksheetVisibilityVisible => 'Видимый';

  @override
  String get printWorksheetVisibilityHidden => 'Скрытый';

  @override
  String get printWorksheetVisibilityVeryHidden => 'Очень скрытый';

  @override
  String get printColPrinter => 'Принтер';

  @override
  String get printColPaper => 'Бумага';

  @override
  String get printColOrientation => 'Ориентация';

  @override
  String get printColScaling => 'Масштаб';

  @override
  String get printColColor => 'Цвет';

  @override
  String get printColDuplex => 'Дуплекс';

  @override
  String get printColCopies => 'Копии';

  @override
  String get printColPageRange => 'Диапазон страниц';

  @override
  String get printColPageRangeHelp =>
      'Используйте all, одну страницу или диапазоны через запятую. Для изображений допустимы all или 1, для файлов нативных приложений — только all.';

  @override
  String get printStatusReady => 'Готово';

  @override
  String get printStatusPending => 'Ожидает';

  @override
  String get printStatusPrinting => 'Печатается';

  @override
  String get printStatusCompleted => 'Завершено';

  @override
  String get printStatusCompletedUnconfirmed =>
      'Отправлено; подтверждение недоступно';

  @override
  String get printStatusFailed => 'Ошибка';

  @override
  String get printStatusCancelled => 'Отменено';

  @override
  String get printStatusUncertain => 'Проверьте принтер';

  @override
  String get warnPrintSettingsLimited =>
      'Этот формат печатается системным приложением; поддержка бумаги и макета зависит от приложения';

  @override
  String get warnPrintNoJobsSelected => 'Не выбраны строки для печати';

  @override
  String get warnPrintInvalidRetries =>
      'Количество повторов должно быть от 0 до 3';

  @override
  String get warnPrintInvalidTimeout =>
      'Тайм-аут должен быть от 1 до 240 минут';

  @override
  String get warnPrintResumingQueue =>
      'Проверяется незавершённая сохранённая очередь печати';

  @override
  String get warnPrintCancelled =>
      'Очередь печати поставлена пользователем на паузу';

  @override
  String get warnPrintCancelledUncertain =>
      'При отмене система ещё не определила текущее задание. Проверьте принтер перед повторным выбором.';

  @override
  String get warnPrintStateUncertain =>
      'Система не смогла подтвердить печать этого задания. Автоповтор не выполнен; проверьте результат перед повторным выбором.';

  @override
  String get warnPrintSubmittedUnconfirmed =>
      'Windows приняла задание, но не сохранила окончательную квитанцию. Повторной отправки не было; очередь продолжила работу со следующим файлом.';

  @override
  String get errorPrintQueuePaused =>
      'Очередь поставлена на паузу; оставшиеся задания сохранены для продолжения';

  @override
  String get errorPrintQueueSourceReselectionRequired =>
      'Эту старую очередь печати нельзя безопасно продолжить. Удалите её и заново выберите файлы.';

  @override
  String get printResumeTitle => 'Найдена незавершённая очередь печати';

  @override
  String printResumeSummary(Object count) {
    return 'Можно проверить или продолжить $count заданий';
  }

  @override
  String get printResumeReview => 'Повторить только ненапечатанные';

  @override
  String get printResumeDiscard => 'Отбросить очередь';

  @override
  String get paramsPreviousTitle => 'Доступны предыдущие настройки печати';

  @override
  String get paramsPreviousHelp =>
      'Файлы не сохраняются из соображений безопасности. Восстановите предыдущий принтер, бумагу, ориентацию, цвет и настройки очереди только по своему выбору.';

  @override
  String get paramsPreviousApply => 'Использовать предыдущие настройки';

  @override
  String get printResumeIncomplete => 'Очередь безопасно приостановлена';

  @override
  String get printCancelQueue => 'Поставить на паузу';

  @override
  String get printCancelling => 'Приостановка…';

  @override
  String get editorActionShowDetails => 'Показать автоматические поля';

  @override
  String get editorActionHideDetails => 'Скрыть автоматические поля';

  @override
  String get layoutDataResetHeight => 'Сбросить высоту';

  @override
  String get layoutDataFit => 'По размеру доступной области';

  @override
  String layoutDataResizeHeight(Object label) {
    return 'Изменить высоту: $label';
  }

  @override
  String layoutColumnResize(Object column) {
    return 'Изменить ширину столбца «$column»';
  }

  @override
  String layoutColumnWidth(Object width) {
    return 'Ширина: $width пикс.';
  }

  @override
  String get editorBulkTitle => 'Массовое редактирование';

  @override
  String editorSelectedCount(Object count) {
    return 'Выбрано: $count';
  }

  @override
  String get editorSelectAll => 'Выбрать все';

  @override
  String get editorSelectNone => 'Очистить выбор';

  @override
  String editorSelectRow(Object row) {
    return 'Выбрать строку $row';
  }

  @override
  String get editorSelectGroup => 'Выбрать эту группу';

  @override
  String get editorBulkField => 'Изменяемое поле';

  @override
  String get editorBulkValue => 'Новое значение';

  @override
  String get editorBulkChoose => 'Выберите значение';

  @override
  String get editorBulkApply => 'Применить к выбранным';

  @override
  String get editorBoolTrue => 'Да';

  @override
  String get editorBoolFalse => 'Нет';

  @override
  String get operationCancel => 'Остановить после текущего элемента';

  @override
  String get operationCancelling => 'Остановка…';

  @override
  String get warnOperationCancelled =>
      'Операция безопасно остановлена; завершённые элементы сохранены в истории, оставшиеся не запускались';

  @override
  String get opRfiLogAddTitle => 'Добавить записи RFI на несколько листов';

  @override
  String get opRfiFollowUpSyncTitle => 'Синхронизировать RFI Log с Follow Up';

  @override
  String get opRfiFollowUpSyncDesc =>
      'Безопасно синхронизирует строки жизненного цикла CIV/MEC и FINAL-строки Worley Signed/Not Signed из RFI Log, включая подготовленные отсутствующие записи, удаления и групповые статусы.';

  @override
  String get paramRfiFollowUpRfiLogWorkbook => 'Исходная книга RFI Log';

  @override
  String get paramRfiFollowUpRfiLogWorkbookHelp =>
      'Выберите книгу RFI Log .xlsm или .xlsx, из которой будут прочитаны статусы.';

  @override
  String get paramRfiFollowUpFollowUpWorkbook => 'Целевая книга Follow Up';

  @override
  String get paramRfiFollowUpFollowUpWorkbookHelp =>
      'Выберите .xlsm или .xlsx, где могут меняться только показанные в предпросмотре ячейки RFI и статуса.';

  @override
  String get errorParamRequiredRfiLogWorkbook => 'Требуется книга RFI Log';

  @override
  String get errorParamRequiredFollowUpWorkbook => 'Требуется книга Follow Up';

  @override
  String get errorRfiFollowUpWorkbookInvalid =>
      'Выберите допустимые книги RFI Log и Follow Up в формате .xlsm или .xlsx.';

  @override
  String get errorRfiFollowUpWorkbookLocked =>
      'Закройте выбранную книгу в Excel или другом приложении и повторите попытку.';

  @override
  String get errorRfiFollowUpWorkbookChanged =>
      'Одна из выбранных книг изменилась после предпросмотра. Создайте новый предпросмотр перед применением.';

  @override
  String get errorRfiFollowUpLayoutAmbiguous =>
      'Найдено несколько безопасных таблиц Follow Up. Оставьте одну точную таблицу RFI NO./STATUS.';

  @override
  String get errorRfiFollowUpLayoutMissing =>
      'Не найдена безопасная таблица Follow Up с точными заголовками RFI NO. и STATUS.';

  @override
  String get errorRfiFollowUpPatchVerification =>
      'DocFlow не смог подтвердить, что изменились только одобренные ячейки статуса. Результат не принят.';

  @override
  String get errorRfiFollowUpProtected =>
      'Книга, целевой лист или необходимые ячейки защищены либо небезопасны для изменения.';

  @override
  String get errorRfiFollowUpSignedPackage =>
      'Книга содержит цифровую подпись и не может быть безопасно изменена автоматизацией.';

  @override
  String get errorRfiFollowUpLimitExceeded =>
      'Книга превышает безопасные пределы обработки для этой операции.';

  @override
  String get errorRfiFollowUpNoRowsSelected =>
      'Перед применением выберите хотя бы одну безопасную строку Follow Up.';

  @override
  String get errorRfiFollowUpRetainedStorageFull =>
      'Ограниченное хранилище восстановления заполнено. Проверьте или удалите старые данные DocFlow перед повтором.';

  @override
  String get errorRfiFollowUpBackupExists =>
      'Резервная копия с зарезервированным именем уже существует. Ни одна книга не была перезаписана.';

  @override
  String get errorRfiFollowUpBackupVerify =>
      'DocFlow не смог проверить резервную копию книги. Операция остановлена для восстановления.';

  @override
  String get errorRfiFollowUpRecoveryRequired =>
      'Операция требует восстановления; не запускайте её повторно.';

  @override
  String get rfiFollowUpEditorTitle =>
      'Проверка кандидатов статуса RFI Follow Up';

  @override
  String rfiFollowUpEditorRowCount(Object count) {
    return 'Строк Follow Up: $count';
  }

  @override
  String get rfiFollowUpColumnEnabled => 'Применить';

  @override
  String get rfiFollowUpColumnRfiNo => 'Номер RFI';

  @override
  String get rfiFollowUpColumnSourceStatuses => 'Исходное значение RFI Log';

  @override
  String get rfiFollowUpColumnCurrentStatus => 'Текущий статус Follow Up';

  @override
  String get rfiFollowUpColumnProposedRfiNo => 'Предлагаемая группа RFI';

  @override
  String get rfiFollowUpColumnProposedStatus => 'Предлагаемый статус';

  @override
  String get rfiFollowUpColumnDisposition => 'Решение';

  @override
  String get rfiFollowUpColumnReason => 'Причина';

  @override
  String get rfiFollowUpColumnSheet => 'Лист';

  @override
  String get rfiFollowUpColumnRow => 'Строка';

  @override
  String get rfiFollowUpColumnMappingDetails => 'Детали сопоставления';

  @override
  String get rfiFollowUpDispositionReady => 'Готово к обновлению';

  @override
  String get rfiFollowUpDispositionAlreadySynced => 'Уже синхронизировано';

  @override
  String get rfiFollowUpDispositionManualReview => 'Только ручная проверка';

  @override
  String get warnRfiFollowUpManualRows =>
      'строк требуют ручной проверки и не могут быть выбраны автоматически.';

  @override
  String get warnRfiFollowUpNoRowsSelected =>
      'Нет строк, которые можно безопасно применить. Отключённые флажки означают ручную проверку; смотрите столбец Причина.';

  @override
  String get warnRfiFollowUpUnmappedRecords =>
      'Записи RFI Log с неподдерживаемым маршрутом, отсутствующей подготовленной строкой или неоднозначным совпадением оставлены без изменений. Точные ITP/item и исходный/целевой контекст указаны в столбцах «Причина» и «Детали сопоставления».';

  @override
  String get warnRfiFollowUpReasonTargetRfiFormula =>
      'Номер RFI в целевой книге является формулой.';

  @override
  String get warnRfiFollowUpReasonTargetRfiMerged =>
      'Номер RFI в целевой книге находится в объединённой ячейке.';

  @override
  String get warnRfiFollowUpReasonBlankRfi => 'Номер RFI в целевой книге пуст.';

  @override
  String get warnRfiFollowUpReasonInvalidRfi =>
      'Целевой номер RFI должен иметь точный короткий формат, например CIV-0170.';

  @override
  String get warnRfiFollowUpReasonTargetDuplicate =>
      'Один и тот же полный номер RFI несколько раз встречается в целевой таблице.';

  @override
  String get warnRfiFollowUpReasonSourceMissing =>
      'В RFI Log нет точного совпадения по полному значению RFI.';

  @override
  String get warnRfiFollowUpReasonSourceFormula =>
      'Совпавший исходный RFI или статус является формулой.';

  @override
  String get warnRfiFollowUpReasonSourceMerged =>
      'Совпавший исходный RFI или статус находится в объединённой ячейке.';

  @override
  String get warnRfiFollowUpReasonSourceStatusBlank =>
      'Совпавший статус RFI Log пуст.';

  @override
  String get warnRfiFollowUpReasonSourceStatusUnknown =>
      'Совпавший статус RFI Log не равен точно Completed или Ongoing.';

  @override
  String get warnRfiFollowUpReasonSourceConflict =>
      'RFI Log содержит противоречивые статусы для этого точного номера RFI.';

  @override
  String get warnRfiFollowUpReasonTargetCellMissing =>
      'Целевая ячейка статуса отсутствует.';

  @override
  String get warnRfiFollowUpReasonTargetFormula =>
      'Целевая ячейка статуса является формулой.';

  @override
  String get warnRfiFollowUpReasonTargetMerged =>
      'Целевая ячейка статуса объединена.';

  @override
  String get warnRfiFollowUpReasonTargetStatusUnknown =>
      'Текущий целевой статус не пуст и не равен Signed или Not Signed; он не будет перезаписан.';

  @override
  String get warnRfiFollowUpReasonAlreadySynced =>
      'В целевой книге уже установлен предлагаемый статус.';

  @override
  String get warnRfiFollowUpReasonReadySigned =>
      'Точную отметку Signed by Worley из REMARKS можно применить как Signed.';

  @override
  String get warnRfiFollowUpReasonReadyLogSync =>
      'Группу RFI и общий статус можно синхронизировать по утвержденному сопоставлению RFI Log.';

  @override
  String get warnRfiFollowUpReasonReadyWorleySync =>
      'Статус FINAL Worley можно синхронизировать по точной строке Signed by Worley в REMARKS.';

  @override
  String get warnRfiFollowUpReasonReadyRemove =>
      'Записи Cancelled или Rejected будут удалены, а общий статус оставшейся группы пересчитан.';

  @override
  String get warnRfiFollowUpReasonMappingMissing =>
      'Поддерживаемый маршрут Follow Up найден, но подготовленная целевая строка с точными ITP, item и контекстом этого RFI отсутствует. Изменения не внесены.';

  @override
  String get warnRfiFollowUpReasonPartialContext =>
      'Некоторые исходные контексты соответствуют подготовленным строкам Follow Up, другие — нет. Проверьте в RFI Log значения Tag No, Area и Inspection Location для несовпавших контекстов. DocFlow не изменяет исходный журнал.';

  @override
  String get warnRfiFollowUpReasonMappingUnsupported =>
      'Для этой пары ITP/item в текущем шаблоне нет маршрута Follow Up. Автоматическая синхронизация невозможна.';

  @override
  String get warnRfiFollowUpReasonMappingAmbiguous =>
      'Записи соответствуют несколько целевых строк; вместо предположения изменения не выполняются.';

  @override
  String get warnRfiFollowUpReasonReadyNotSigned =>
      'Точная отметка Signed by Worley не найдена; можно применить Not Signed.';

  @override
  String get opRfiLogAddDesc =>
      'Создаёт номера CIV, MEC, ELE и PRC вместе, планирует общее время и готовит отдельную таблицу для каждого листа Excel.';

  @override
  String get paramRfiWorkbook => 'Файл Excel RFI Log';

  @override
  String get paramRfiWorkbookHelp =>
      'Выберите RFI Log с поддержкой макросов .xlsm или стандартный .xlsx для безопасного чтения и обновления.';

  @override
  String get dialogFilterExcelWorkbook => 'Excel (.xlsm, .xlsx)';

  @override
  String get errorRfiWorkbookInvalid =>
      'Выберите допустимый RFI Log с поддержкой макросов .xlsm или стандартный .xlsx.';

  @override
  String get paramRfiEmailText => 'Вставьте письмо с запросом';

  @override
  String get paramRfiEmailTextHelp =>
      'Вставьте текст письма. Он используется только для этого предпросмотра и не сохраняется в настройках или Excel.';

  @override
  String get paramRfiEmailProvider => 'Способ извлечения данных';

  @override
  String get paramRfiEmailProviderHelp =>
      'Offline-режим бесплатный и приватный. Gemini Free необязателен и отправляет письмо в Google.';

  @override
  String get choiceRfiEmailProviderOffline =>
      'Offline — приватно (рекомендуется)';

  @override
  String get choiceRfiEmailProviderGemini => 'Gemini Free — внешний ИИ';

  @override
  String get paramRfiGeminiApiKey => 'Ключ Gemini API (только для Gemini)';

  @override
  String get paramRfiGeminiApiKeyHelp =>
      'Не сохраняется. Можно задать GEMINI_API_KEY в Windows. Не используйте Gemini Free для конфиденциальной корпоративной почты.';

  @override
  String get errorParamRequiredEmailProvider =>
      'Выберите способ извлечения данных';

  @override
  String get warnRfiEmailExtracted =>
      'Поля письма извлечены в редактируемый предпросмотр';

  @override
  String get warnRfiEmailNeedsReview =>
      'Некоторые данные неоднозначны или отсутствуют; проверьте строки';

  @override
  String get warnRfiAiFallback =>
      'Gemini недоступен; использовано приватное offline-извлечение';

  @override
  String get warnRfiGeminiPrivacy =>
      'Gemini Free отправляет письмо в Google. Данные бесплатного уровня могут использоваться для улучшения продуктов; не отправляйте конфиденциальные сведения.';

  @override
  String get paramRfiDiscipline => 'Дисциплина';

  @override
  String get paramRfiCount => 'Количество номеров RFI';

  @override
  String get paramRfiCountHelp =>
      'Считает уникальные номера RFI; дополнительные строки того же RFI не занимают новый номер или интервал времени.';

  @override
  String get paramRfiSheetCounts => 'Sheet и количество RFI';

  @override
  String get paramRfiSheetCountsHelp =>
      'Выберите один или несколько sheet и укажите количество уникальных RFI для каждого. Все изменения применяются одной операцией.';

  @override
  String rfiSheetCountsTotal(Object count) {
    return 'Номеров RFI в этой операции: $count';
  }

  @override
  String get paramRfiOverflowNextDay =>
      'Переносить превышение на следующий календарный день';

  @override
  String get paramRfiOverflowNextDayHelp =>
      'После 17:30 продолжает на следующий день с 09:00. Отключите, чтобы оставить позднее время в том же дне.';

  @override
  String get errorParamRequiredWorkbook =>
      'Необходимо выбрать файл Excel RFI Log';

  @override
  String get errorParamRequiredDiscipline => 'Необходимо выбрать дисциплину';

  @override
  String get errorParamRequiredCount => 'Необходимо указать количество RFI';

  @override
  String get errorParamRequiredSheetCounts =>
      'Выберите хотя бы один sheet и RFI';

  @override
  String get noteRfiWorkbookUpdate => 'Будут добавлены строки RFI Log';

  @override
  String get noteRfiRestoreBackup => 'Восстановить резервную копию Excel';

  @override
  String get warnRfiOverflowMoved =>
      'Время RFI превысило 17:30 и перенесено на следующий календарный день';

  @override
  String get warnRfiOverflowKept =>
      'Время RFI превышает 17:30 и останется в том же дне';

  @override
  String get warnRfiGroupScheduleMismatch =>
      'У строк с одним номером RFI различаются дата или время инспекции';

  @override
  String get rfiEditorTitle => 'Проверка и редактирование записей RFI';

  @override
  String rfiEditorRowCount(Object count) {
    return '$count строк Excel';
  }

  @override
  String get rfiEditorScrollHint =>
      'Прокрутите по горизонтали, чтобы изменить все поля';

  @override
  String get rfiEditorCompactHint =>
      'Наведите для полного номера; автоматические поля находятся в разделе «Детали»';

  @override
  String get rfiEditorReviewWarnings =>
      'Изменённые строки проверены. Просмотрите новые предупреждения и снова нажмите «Применить».';

  @override
  String rfiEditorCopied(Object count) {
    return 'Excel открыт: отфильтровано $count строк и скопировано с исходным форматированием';
  }

  @override
  String get rfiActionDuplicate => 'Добавить строку с тем же номером RFI';

  @override
  String get rfiActionRemoveDuplicate => 'Удалить эту дополнительную строку';

  @override
  String get rfiActionCopyMail => 'Отфильтровать и точно скопировать в Excel';

  @override
  String get rfiActionPrepareReply => 'Подготовить ответ в Outlook';

  @override
  String get rfiActionCopySheet => 'Скопировать из Excel';

  @override
  String get rfiMailPreviewTitle => 'Таблица Excel для Outlook';

  @override
  String get rfiMailPreviewSourceWorkbook => 'Актуальные данные из Excel';

  @override
  String get rfiMailPreviewSourcePlan => 'Данные утверждённого плана';

  @override
  String get rfiMailPreviewHint =>
      'Предпросмотр показывает точные столбцы и значения для копирования. Вставьте таблицу в Outlook обычным способом; формат Excel и границы ячеек сохранятся.';

  @override
  String get rfiMailPreviewCopy => 'Копировать с форматированием';

  @override
  String get rfiActionDetails => 'Автоматические поля и детали';

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
  String get rfiColIssueDate => 'Выдача';

  @override
  String get rfiColInspectionDate => 'Проверка';

  @override
  String get rfiColStartTime => 'Время';

  @override
  String get rfiColAzfenType => 'Insp. Type AZFEN';

  @override
  String get rfiColIntecType => 'Insp. Type INTEC';

  @override
  String get rfiColLocation => 'Место';

  @override
  String get rfiColSubcontractor => 'Субподрядчик';

  @override
  String get rfiDateDay => 'День';

  @override
  String get rfiDateMonth => 'Месяц';

  @override
  String get rfiDateChangeYear => 'Изменить год';

  @override
  String get rfiDateYear => 'Год';

  @override
  String get rfiColActions => 'Строки';

  @override
  String get actionClose => 'Закрыть';

  @override
  String get paramOrganizeIntoRfiFolders =>
      'Создавать отдельную папку для каждого RFI';

  @override
  String get paramOrganizeIntoRfiFoldersHelp =>
      'Отключите для сохранения PDF прямо в целевой папке; нечитаемые и повторные файлы всё равно попадут в _review для безопасности';

  @override
  String get errorRfiWorkbookLocked =>
      'RFI Log открыт или доступен только для чтения. Сохраните и закройте его в Excel, затем проверьте доступ на запись.';

  @override
  String get opRfiStageSignedFirstPagesTitle =>
      'Подготовить подписанные первые страницы RFI';

  @override
  String get opRfiNormalizePdfOrientationTitle => 'Повернуть страницы PDF RFI';

  @override
  String get opRfiNormalizePdfOrientationDesc =>
      'Отдельная от Fast RFI Scan операция коррекции поворота и ориентации страниц PDF в CIV, MEC, ELE и PRC. Только уверенно определённые исправления записываются в отдельную копию для проверки; если хотя бы одна страница неоднозначна, PDF остаётся без изменений для ручной проверки.';

  @override
  String get paramRfiOrientationRfiSystemRoot => 'Корень системы RFI';

  @override
  String get paramRfiOrientationRfiSystemRootHelp =>
      'Источник только для чтения с папками CIV, MEC, ELE и PRC. Исходные файлы не изменяются.';

  @override
  String get paramRfiOrientationReviewRoot => 'Папка проверки ориентации';

  @override
  String get paramRfiOrientationReviewRootHelp =>
      'Отдельная папка, где исправленные копии создаются с исходной структурой каталогов.';

  @override
  String get paramScanIndexForceRescan =>
      'Принудительно выполнить полное сканирование';

  @override
  String get paramScanIndexForceRescanHelp =>
      'Не использовать прежний индекс для этого запуска. После выхода из задачи переключатель автоматически выключится.';

  @override
  String get errorRfiOrientationReviewRootMustBeSeparate =>
      'Папка проверки не должна пересекаться с системой RFI';

  @override
  String get warnRfiOrientationNoPdfs =>
      'PDF не найдены в CIV, MEC, ELE или PRC';

  @override
  String get warnRfiOrientationUnsupported =>
      'PDF нельзя безопасно проанализировать';

  @override
  String get warnRfiOrientationNeedsReview =>
      'Ориентация хотя бы одной страницы определена недостаточно уверенно; файл не применен';

  @override
  String get warnRfiOrientationReviewExists =>
      'Файл назначения уже существует и не был перезаписан';

  @override
  String get noteRfiPdfOrientationNormalized =>
      'Исправить утвержденную ориентацию страниц PDF в отдельной копии для проверки';

  @override
  String get rfiOrientationPages => 'Повороты страниц';

  @override
  String get opRfiStageSignedFirstPagesDesc =>
      'Распознаёт подписанные сканы, выполняет быстрый и безопасный поиск по системе RFI и заменяет только первую страницу в отдельных копиях для проверки, не изменяя архив.';

  @override
  String get paramRfiSignedScans => 'Папка новых подписанных сканов';

  @override
  String get paramRfiSignedScansHelp =>
      'Новые подписанные PDF RFI. Каждая страница объединенного скана распознается отдельно.';

  @override
  String get paramRfiArchiveRoot => 'Корень существующего архива RFI';

  @override
  String get paramRfiArchiveRootHelp =>
      'Корень RFI System только для чтения. Все вложенные папки сканируются ограниченно и многопоточно; совпадают только точные имена PDF RFI.';

  @override
  String get paramRfiReviewRoot => 'Папка результатов для проверки';

  @override
  String get paramRfiReviewRootHelp =>
      'Отдельная, желательно пустая папка, куда копии записываются с сохранением относительной структуры архива.';

  @override
  String get paramRfiScanSubfolders => 'Также сканировать вложенные папки';

  @override
  String get paramRfiScanSubfoldersHelp =>
      'Включено по умолчанию. Распознаёт PDF на любой глубине папки новых сканов; отключайте только для намеренной проверки верхнего уровня.';

  @override
  String get errorParamRequiredSignedScans =>
      'Укажите папку новых подписанных сканов';

  @override
  String get errorParamRequiredArchiveRoot =>
      'Укажите корень существующего архива RFI';

  @override
  String get errorParamRequiredReviewRoot =>
      'Укажите папку результатов для проверки';

  @override
  String get errorRfiReviewRootMustBeSeparate =>
      'Папка результатов должна быть полностью отделена и от архива, и от папки новых сканов. Файлы не были затронуты.';

  @override
  String get noteRfiSignedFirstPageStaged =>
      'Создать копию для проверки с подписанной первой страницей';

  @override
  String get noteRfiCleanupStaleTemp =>
      'Удалить оставшуюся временную ссылку завершённого PDF для проверки';

  @override
  String get warnRfiNoSignedPdfs => 'В папке новых сканов не найдены PDF';

  @override
  String get warnRfiSignedUnreadable =>
      'PDF не удалось прочитать; результат для этого RFI не подготовлен';

  @override
  String get warnRfiSignedDuplicate =>
      'Найдено несколько новых сканов с одним номером RFI; автоматический выбор не выполнен';

  @override
  String get warnRfiArchiveUnreadable =>
      'Не удалось безопасно просканировать весь архив; результаты не подготовлены во избежание пропущенного или ложного совпадения';

  @override
  String get warnRfiArchiveMissing =>
      'В архиве не найден файл с этим номером RFI';

  @override
  String get warnRfiArchiveAmbiguous =>
      'В архиве найдено несколько файлов с этим номером RFI; выберите один PDF в поле пути архива';

  @override
  String get warnRfiReviewExists =>
      'В папке проверки уже есть целевой файл; он не был перезаписан';

  @override
  String get warnRfiStaleTempFound =>
      'PDF для проверки уже готов; временный файл, оставшийся после блокировки Windows, будет безопасно удалён';

  @override
  String get warnRfiReviewPathBlocked =>
      'Целевой путь заблокирован файлом или перенаправленной папкой; результат не подготовлен';

  @override
  String get warnRfiSignedExtraPages =>
      'Новый PDF содержит несколько страниц; будет использована только подписанная первая страница';

  @override
  String get warnRfiSignedMultiPage =>
      'Обнаружен многостраничный подписанный скан; каждая страница проанализирована отдельно';

  @override
  String get warnRfiFirstPageLayoutDiffers =>
      'Размер или ориентация новой первой страницы отличается; проверьте предварительный просмотр перед применением';

  @override
  String get warnRfiPdfFidelityUnsupported =>
      'Этот PDF нельзя подготовить без риска нарушения целостности документа; изучите причину и обработайте файл вручную';

  @override
  String get rfiSignedEditorTitle => 'Проверка подписанных первых страниц';

  @override
  String rfiSignedEditorRowCount(Object count) {
    return 'Сопоставлено PDF RFI: $count';
  }

  @override
  String get rfiSignedColumnEnabled => 'Подготовить';

  @override
  String get rfiSignedColumnCode => 'Номер RFI';

  @override
  String get rfiSignedColumnScan => 'Подписанный скан';

  @override
  String get rfiSignedColumnArchive => 'Путь в архиве';

  @override
  String get rfiSignedArchiveChoose => 'Выберите архивный PDF';

  @override
  String get rfiSignedPreviewTitle => 'Сравнение первых страниц';

  @override
  String get rfiSignedPreviewHelp =>
      'Сравните первую страницу архива с новой подписанной страницей, выбранной утверждённым планом.';

  @override
  String get rfiSignedPreviewRfi => 'RFI для просмотра';

  @override
  String get rfiSignedPreviewArchive => 'Текущая страница архива';

  @override
  String get rfiSignedPreviewSigned => 'Новая подписанная страница';

  @override
  String rfiSignedPreviewArchiveAlt(Object code) {
    return 'Текущая первая страница архива для $code';
  }

  @override
  String rfiSignedPreviewSignedAlt(Object code) {
    return 'Новая подписанная страница для $code';
  }

  @override
  String get rfiSignedPreviewChooseArchive =>
      'Выберите архивный PDF, чтобы показать его первую страницу.';

  @override
  String get rfiSignedPreviewUnavailable =>
      'Не удалось подготовить ограниченный просмотр страницы; перед применением проверьте PDF вручную.';

  @override
  String rfiSignedPreviewOmitted(Object count) {
    return 'Для ещё $count строк RFI просмотр недоступен из-за безопасного ограничения.';
  }

  @override
  String get rfiSignedPreviewPrevious => 'Предыдущий RFI';

  @override
  String get rfiSignedPreviewNext => 'Следующий RFI';

  @override
  String get rfiSignedPreviewOnlyPageOne =>
      'Будет заменена только страница 1; остальные страницы архива останутся без изменений.';

  @override
  String get rfiSignedPreviewBefore => 'ДО';

  @override
  String get rfiSignedPreviewAfter => 'ПОСЛЕ';

  @override
  String get rfiSignedPreviewEnlarge => 'Увеличить';

  @override
  String get rfiSignedNavLabel => 'Разделы просмотра подписанных страниц';

  @override
  String get rfiSignedNavApproved => 'Одобренные RFI';

  @override
  String get rfiSignedNavCompare => 'Сравнить страницы';

  @override
  String get rfiSignedNavReview => 'Копии для проверки';

  @override
  String get rfiSignedWarningReviewTitle =>
      'PDF с предупреждениями будут отправлены в Review';

  @override
  String get rfiSignedWarningReviewHelp =>
      'Apply копирует каждый затронутый исходный PDF без изменений и сохраняет путь подпапки внутри _review.';

  @override
  String get rfiSignedWarningReviewPlanned => 'Будет скопирован';

  @override
  String get rfiSignedWarningReviewExists => 'Уже существует';

  @override
  String get rfiSignedWarningReviewBlocked => 'Заблокировано';

  @override
  String get rfiSignedWarningReviewDestination => 'Папка Review';

  @override
  String rfiSignedWarningReviewPage(Object page) {
    return 'Страница $page';
  }

  @override
  String get rfiSignedWarningReviewFile => 'Весь файл';

  @override
  String get noteRfiSignedWarningReviewCopy =>
      'Копировать PDF с предупреждением в Review';

  @override
  String get warnRfiWarningReviewExists =>
      'PDF с предупреждением уже существует в Review и не будет перезаписан';

  @override
  String get warnRfiWarningReviewBlocked =>
      'PDF с предупреждением нельзя скопировать в Review: путь назначения заблокирован';

  @override
  String get progressStages => 'Этапы обработки';

  @override
  String get progressPhasePreparing => 'Подготовка просмотра';

  @override
  String get progressPhasePreview => 'Создание просмотра';

  @override
  String get progressPhaseApply => 'Применение одобренного плана';

  @override
  String get progressPhasePrintWaiting => 'Ожидание принтера';

  @override
  String get progressPhaseScan => 'Сканирование файлов';

  @override
  String get progressPhaseScanDiscovery => 'Поиск PDF для сканирования';

  @override
  String get progressPhaseFileGroupDiscovery => 'Поиск файлов для группировки';

  @override
  String get progressPhaseFileGroupAnalysis => 'Чтение номеров RFI';

  @override
  String get progressPhaseFileGroupPlanning => 'Подготовка плана папок RFI';

  @override
  String get progressPhaseScanOcr => 'Распознавание PDF с помощью OCR';

  @override
  String get progressPhaseScanPlanning => 'Подготовка плана файлов';

  @override
  String get progressPhaseWorkbookReading => 'Чтение рабочей книги';

  @override
  String get progressPhaseRfiRequestAnalysis => 'Анализ запроса RFI';

  @override
  String get progressPhaseRfiPlanPreparation => 'Подготовка строк RFI';

  @override
  String get progressPhaseFollowUpAnalysis => 'Расчёт сопоставлений Follow Up';

  @override
  String get progressPhaseSystemDiscovery => 'Поиск записей в системе RFI';

  @override
  String get progressPhaseSystemSignatureAnalysis => 'Анализ подписей Worley';

  @override
  String get progressPhaseSystemWorkbookRead => 'Чтение книг RFI';

  @override
  String get progressPhaseSystemMapping =>
      'Сопоставление подписей с записями книг';

  @override
  String get progressPhaseOrientationDiscovery =>
      'Поиск PDF для проверки ориентации';

  @override
  String get progressPhaseOrientationInspection =>
      'Проверка структуры PDF и кеша сканирования';

  @override
  String get progressPhaseOrientationAnalysis =>
      'Анализ ориентации страниц PDF';

  @override
  String get progressPhasePrinterCheck => 'Проверка готовности принтера';

  @override
  String get progressPhasePrintFileInspection =>
      'Проверка файлов и параметров печати';

  @override
  String get progressPhaseOutlookSelection => 'Проверка выбора Outlook';

  @override
  String get progressPhaseSignedDiscovery => 'Поиск подписанных PDF';

  @override
  String get progressPhaseSignedInspection => 'Проверка страниц PDF';

  @override
  String get progressPhaseSignedOcr => 'Чтение номеров RFI';

  @override
  String get progressPhaseSignedClassification =>
      'Сопоставление подписанных страниц';

  @override
  String get progressPhaseArchiveScan => 'Поиск в архиве RFI';

  @override
  String get progressPhaseComparisonPreparation =>
      'Подготовка сравнения страниц';

  @override
  String get progressPhasePreviewReady => 'Просмотр готов';

  @override
  String get recoveryTitle => 'Центр восстановления';

  @override
  String get homeRecoveryDesc =>
      'Безопасно проверьте ожидающие журналы и неопределённые задания печати.';

  @override
  String get recoveryHelp =>
      'Восстановление разрешают только локальные журналы и сохранённые очереди печати.';

  @override
  String get recoveryNoEffect => 'Изменений нет — повторный просмотр безопасен';

  @override
  String get recoveryOutcomeUnknown =>
      'Результат неизвестен — без автоматического повтора';

  @override
  String get recoveryManual => 'Требуется ручная проверка';

  @override
  String get recoveryRestoreOriginal => 'Восстановить исходный файл Follow Up';

  @override
  String get recoveryPrintQueue => 'Неопределённая очередь печати';

  @override
  String get recoveryEmpty => 'Восстановление не требуется';

  @override
  String get recoveryEmptyHelp =>
      'Ожидающих журналов и незавершённых очередей печати нет.';

  @override
  String get effectCommitted => 'Завершено';

  @override
  String get effectNoEffectRetryable => 'Изменений нет — повтор безопасен';

  @override
  String get effectPartialCommitted =>
      'Завершено частично — проверьте восстановление';

  @override
  String get effectOutcomeUnknown =>
      'Результат неизвестен — без автоматического повтора';

  @override
  String get effectCancelledClean => 'Отменено без изменений';

  @override
  String get effectCancelledPartial =>
      'Отменено частично — проверьте восстановление';

  @override
  String get presetTitle => 'Предустановки';

  @override
  String get presetSelect => 'Сохранённая предустановка';

  @override
  String get presetSelectPlaceholder => 'Выберите предустановку';

  @override
  String get presetName => 'Название новой предустановки';

  @override
  String get presetApply => 'Применить предустановку';

  @override
  String get presetSave => 'Сохранить предустановку';

  @override
  String get presetReplace => 'Заменить предустановку';

  @override
  String get presetDelete => 'Удалить предустановку';

  @override
  String presetConfirmReplace(Object name) {
    return 'Предустановка с названием $name уже существует. Заменить её?';
  }

  @override
  String presetConfirmDelete(Object name) {
    return 'Удалить предустановку $name?';
  }

  @override
  String get presetEmpty => 'Сохранённых предустановок пока нет.';

  @override
  String presetSaved(Object name) {
    return 'Предустановка $name сохранена.';
  }

  @override
  String presetDeleted(Object name) {
    return 'Предустановка $name удалена.';
  }

  @override
  String get errorPresetNameInvalid =>
      'Название предустановки должно содержать от 1 до 64 символов.';

  @override
  String get errorPresetDuplicate =>
      'Предустановка с таким названием уже существует.';

  @override
  String get errorPresetLimit =>
      'Для этой операции можно сохранить не более 20 предустановок.';

  @override
  String get errorPresetStale =>
      'Эта предустановка больше не существует. Обновите список и повторите попытку.';

  @override
  String get errorPresetWrite =>
      'Не удалось сохранить настройки предустановки. Повторите попытку.';

  @override
  String get opRfiOutlookReplyDraftTitle =>
      'Подготовить черновик «Ответить всем» в Outlook';

  @override
  String get opRfiOutlookReplyDraftDesc =>
      'Использует только одно выбранное письмо Outlook, готовит ответ всем и показывает его как НЕОТПРАВЛЕННЫЙ черновик для проверки.';

  @override
  String get paramRfiOutlookReplyDraftReplyText => 'Текст ответа';

  @override
  String get paramRfiOutlookReplyDraftReplyTextHelp =>
      'Введите текст для добавления в начало. Предпросмотр экранирует HTML; операция никогда не отправляет письмо и не добавляет получателей.';

  @override
  String get errorParamRequiredReplyText => 'Требуется текст ответа';

  @override
  String get stepRfiOutlookReplyDraft =>
      'Показать неотправленный черновик ответа всем';

  @override
  String get outlookDraftPreviewTitle =>
      'Предпросмотр неотправленного ответа Outlook';

  @override
  String get outlookDraftSubject => 'Тема выбранного письма';

  @override
  String get outlookDraftPlainText => 'Обычный текст';

  @override
  String get outlookDraftHtml => 'Экранированный HTML';

  @override
  String get warnRfiOutlookDraftUnsent =>
      'Применение покажет НЕОТПРАВЛЕННЫЙ черновик ответа всем; DocFlow никогда его не отправляет.';

  @override
  String get warnRfiOutlookDraftUndoConditional =>
      'Отмена доступна только если Outlook автоматически сохранит постоянный черновик и именно этот черновик останется неизменным.';

  @override
  String get warnRfiOutlookDraftUndoUnavailable =>
      'Outlook не предоставил постоянный автоматически сохранённый черновик, поэтому отмена недоступна.';

  @override
  String get errorRfiOutlookDraftUnsupportedPlatform =>
      'Автоматизация черновика Outlook доступна только в Windows.';

  @override
  String get errorRfiOutlookDraftReplyRequired =>
      'Введите текст ответа перед предпросмотром.';

  @override
  String get errorRfiOutlookDraftRfiLogNotCommitted =>
      'Ответ Outlook можно подготовить только из той же завершённой операции RFI Log.';

  @override
  String get errorRfiOutlookDraftReplyTooLong =>
      'Текст ответа превышает лимит 10 000 символов.';

  @override
  String get errorRfiOutlookDraftReplyInvalid =>
      'Текст ответа содержит неподдерживаемый управляющий символ.';

  @override
  String get errorRfiOutlookDraftSelectionRequired =>
      'Выберите ровно одно письмо в активном окне Проводника Outlook.';

  @override
  String get errorRfiOutlookDraftSingleSelectionRequired =>
      'Выберите ровно одно письмо Outlook.';

  @override
  String get errorRfiOutlookDraftMailItemRequired =>
      'Текущий объект Outlook не является почтовым сообщением.';

  @override
  String get errorRfiOutlookDraftSelectionInvalid =>
      'У выбранного письма Outlook нет стабильного идентификатора.';

  @override
  String get errorRfiOutlookDraftSubjectTooLong =>
      'Тема выбранного письма Outlook превышает лимит предпросмотра.';

  @override
  String get errorRfiOutlookDraftSelectionChanged =>
      'Выбранное письмо Outlook изменилось после предпросмотра; выполните предпросмотр снова.';

  @override
  String get errorRfiOutlookDraftOutlookUnavailable =>
      'Автоматизация Microsoft Outlook для рабочего стола недоступна.';

  @override
  String get errorRfiOutlookDraftOutlookNotRunning =>
      'Откройте Outlook для рабочего стола и выберите одно письмо перед предпросмотром.';

  @override
  String get errorRfiOutlookDraftComUnavailable =>
      'COM-апартамент автоматизации Outlook в Windows недоступен.';

  @override
  String get errorRfiOutlookDraftComFailed =>
      'Outlook не смог предоставить требуемое ограниченное свойство сообщения.';

  @override
  String get errorRfiOutlookDraftReplyAllFailed =>
      'Outlook не подтвердил создание черновика ответа всем; проверьте Outlook вручную.';

  @override
  String get errorRfiOutlookDraftBodyPrependFailed =>
      'Outlook не подтвердил обновление текста ответа; проверьте Outlook вручную.';

  @override
  String get errorRfiOutlookDraftBodyFormatUnsupported =>
      'Ответ Outlook использует неподдерживаемый формат RTF; проверьте Outlook вручную.';

  @override
  String get errorRfiOutlookDraftContentTooLarge =>
      'Тело черновика Outlook превышает безопасный лимит автоматизации.';

  @override
  String get errorRfiOutlookDraftDisplayUnknown =>
      'Outlook не подтвердил показ неотправленного черновика; проверьте Outlook вручную.';

  @override
  String get errorRfiOutlookDraftDraftMissingOrChanged =>
      'Отмена отклонена: точный автоматически сохранённый черновик отсутствует или изменён.';

  @override
  String get errorRfiOutlookDraftDeleteUnknown =>
      'Outlook не подтвердил удаление неизменённого черновика; проверьте Outlook вручную.';

  @override
  String get errorRfiOutlookDraftUndoUnavailable =>
      'Отмена недоступна, потому что Outlook не сохранил постоянный черновик автоматически.';

  @override
  String get opRfiOutlookArchiveSelectedTitle =>
      'Архивировать выбранное письмо Outlook';

  @override
  String get opRfiOutlookArchiveSelectedDesc =>
      'Сохраняет одно выбранное письмо Outlook как локальную копию Unicode MSG без отправки, перемещения или удаления письма.';

  @override
  String get paramRfiOutlookArchiveDestination => 'Папка архива почты';

  @override
  String get paramRfiOutlookArchiveDestinationHelp =>
      'Выберите отдельную промежуточную папку. Группировка файлов по RFI затем сможет скопировать MSG во все точно связанные папки RFI.';

  @override
  String get stepRfiOutlookArchive => 'Создать защищённую локальную копию MSG';

  @override
  String get warnRfiOutlookArchiveLocalCopy =>
      'Применение создаст одну локальную копию MSG; выбранное письмо Outlook и почтовый ящик не изменятся.';

  @override
  String get errorRfiOutlookArchiveDestinationExists =>
      'Запланированное имя MSG уже существует в папке архива; переименуйте или переместите файл и повторите предпросмотр.';

  @override
  String get errorRfiOutlookArchivePlanUnavailable =>
      'Утверждённый план архива Outlook больше недоступен; повторите предпросмотр.';

  @override
  String get errorRfiOutlookArchiveSelectionChanged =>
      'Выбранное письмо Outlook изменилось после предпросмотра; выберите исходное письмо и повторите предпросмотр.';

  @override
  String get errorRfiOutlookArchiveExportFailed =>
      'Outlook не смог создать приватный экспорт MSG; файл архива не опубликован.';

  @override
  String get errorRfiOutlookArchiveInvalidMsg =>
      'Outlook создал пустой, слишком большой или недопустимый экспорт MSG; файл архива не опубликован.';

  @override
  String get errorRfiOutlookArchiveWriteFailed =>
      'Не удалось создать файл архива MSG; существующий файл не перезаписан.';

  @override
  String get errorRfiOutlookArchiveWriteUnknown =>
      'Не удалось подтвердить запись архива MSG; проверьте папку назначения перед повтором.';

  @override
  String get opRfiSystemSyncTitle =>
      'Анализировать систему RFI и синхронизировать подписи Worley';

  @override
  String get opRfiSystemSyncDesc =>
      'Сканирует папки системы RFI по дисциплинам, проверяет наличие подписи Worley в разделе Others на стр. 1 и синхронизирует журналы RFI Log и Follow Up.';

  @override
  String get paramRfiSystemSyncRfiSystemRoot => 'Корневая папка системы RFI';

  @override
  String get paramRfiSystemSyncRfiSystemRootHelp =>
      'Выберите корневую папку, содержащую дисциплины CIV, MEC, ELE и PRC.';

  @override
  String get paramRfiSystemSyncRfiLogWorkbook => 'Книга RFI Log';

  @override
  String get paramRfiSystemSyncRfiLogWorkbookHelp =>
      'Выберите файл RFI Log (.xlsm или .xlsx) для обновления примечания на \'Signed by Worley\'.';

  @override
  String get paramRfiSystemSyncFollowUpWorkbook => 'Книга Follow Up';

  @override
  String get paramRfiSystemSyncFollowUpWorkbookHelp =>
      'Выберите текущую книгу Follow Up с контрольными таблицами Signed By Worley CIV и Signed By Worley MEC.';

  @override
  String get errorParamRequiredRfiSystemRoot =>
      'Требуется корневая папка системы RFI';

  @override
  String get errorRfiSystemSyncWorkbookInvalid =>
      'Выберите корректные книги RFI Log и Follow Up (.xlsm или .xlsx).';

  @override
  String get errorRfiSystemSyncWorkbookLocked =>
      'Закройте выбранную книгу в Excel или другой программе и повторите попытку.';

  @override
  String get errorRfiSystemSyncWorkbookLayoutInvalid =>
      'На каждом листе CIV, MEC, ELE и PRC журнала RFI должен быть один точный заголовок RFI No. и REMARKS.';

  @override
  String get errorRfiSystemSyncFollowUpLayoutInvalid =>
      'В книге Follow Up должны быть одна таблица Signed By Worley CIV и одна контрольная таблица Signed By Worley MEC.';

  @override
  String get errorRfiSystemSyncApprovedUpdateInvalid =>
      'Утвержденное изменение книги недействительно; выполните предпросмотр снова.';

  @override
  String get errorRfiSystemSyncWorkbookChanged =>
      'Выбранная книга изменилась после предпросмотра; изменения не применены. Выполните предпросмотр снова.';

  @override
  String get warnRfiSystemSyncNoChangesNeeded =>
      'Все обнаруженные RFI уже синхронизированы или не подписаны.';

  @override
  String get warnRfiSystemSyncNoRowsSelected =>
      'Выберите хотя бы один безопасный подписанный RFI для синхронизации.';

  @override
  String get noteRfiSystemSyncRfiLog =>
      'Синхронизировать подписи Worley в примечания RFI Log';

  @override
  String get noteRfiSystemSyncFollowUp =>
      'Синхронизировать подписанные Final ITP RFI в Follow Up';

  @override
  String get noteRfiSystemSyncBackup =>
      'Создать резервную копию перед синхронизацией';

  @override
  String get noteRfiSystemSyncRestore =>
      'Восстановить книгу из резервной копии';

  @override
  String get rfiSystemSyncEditorTitle =>
      'Просмотр синхронизации системы RFI и подписей Worley';

  @override
  String rfiSystemSyncEditorRowCount(Object count) {
    return '$count обнаруженных RFI';
  }

  @override
  String get rfiSystemSyncColumnEnabled => 'Применить';

  @override
  String get rfiSystemSyncColumnDiscipline => 'Дисциплина';

  @override
  String get rfiSystemSyncColumnRfiNo => 'Номер RFI';

  @override
  String get rfiSystemSyncColumnWorleyStatus => 'Подпись Worley';

  @override
  String get rfiSystemSyncColumnRfiLogAction => 'Действие в RFI Log';

  @override
  String get rfiSystemSyncColumnFollowUpAction => 'Действие в Follow Up';

  @override
  String get rfiSystemSyncColumnDisposition => 'Статус';

  @override
  String get rfiSystemSyncColumnReason => 'Доказательство / Детали';

  @override
  String get rfiSystemSyncDispositionReady => 'Готово к синхронизации';

  @override
  String get rfiSystemSyncDispositionAlreadySynced => 'Уже синхронизировано';

  @override
  String get rfiSystemSyncDispositionUnsigned => 'Не подписано';

  @override
  String get rfiSystemSyncDispositionManualReview => 'Ручная проверка';

  @override
  String get opRfiCompleteStatusSyncTitle =>
      'Отметить завершённые RFI по системе RFI';

  @override
  String get opRfiCompleteStatusSyncDesc =>
      'Сканирует вложенные папки системы RFI на PDF-файлы RFI и готовит безопасное обновление статуса проверки в RFI Log на \'Completed\'.';

  @override
  String get paramRfiCompleteStatusRfiSystemRoot =>
      'Корневая папка системы RFI';

  @override
  String get paramRfiCompleteStatusRfiSystemRootHelp =>
      'Выберите корень системы RFI. Все вложенные папки проверяются только для чтения; символические ссылки не используются.';

  @override
  String get paramRfiCompleteStatusRfiLogWorkbook => 'Книга RFI Log';

  @override
  String get paramRfiCompleteStatusRfiLogWorkbookHelp =>
      'Выберите RFI Log .xlsm или .xlsx, где после проверки безопасные ячейки Inspection Status можно изменить на Completed.';

  @override
  String get errorRfiCompleteStatusSystemRootInvalid =>
      'Выберите доступную для чтения корневую папку системы RFI.';

  @override
  String get errorRfiCompleteStatusWorkbookInvalid =>
      'Выберите корректную книгу RFI Log .xlsm или .xlsx.';

  @override
  String get errorRfiCompleteStatusWorkbookLocked =>
      'Закройте выбранный RFI Log в Excel или другом приложении и повторите попытку.';

  @override
  String get errorRfiCompleteStatusWorkbookLayoutInvalid =>
      'На каждом листе CIV, MEC, ELE и PRC должны быть точные заголовки RFI No. и Inspection Status.';

  @override
  String get errorRfiCompleteStatusApprovedUpdateInvalid =>
      'Утверждённое изменение статуса завершения недействительно; выполните предпросмотр снова.';

  @override
  String get warnRfiCompleteStatusInvalidPdfName =>
      'PDF RFI No. имеет неканоническое имя и не использован.';

  @override
  String get warnRfiCompleteStatusInvalidPdfNameOverflow =>
      'Остальные неканонические имена PDF RFI No. не показаны в списке предупреждений.';

  @override
  String get warnRfiCompleteStatusNoChangesNeeded =>
      'Нет безопасных изменений завершённости для RFI Log.';

  @override
  String get warnRfiCompleteStatusNoRowsSelected =>
      'Выберите хотя бы один безопасный завершённый RFI для синхронизации.';

  @override
  String get noteRfiCompleteStatusRfiLog =>
      'Изменить утверждённые ячейки Inspection Status в RFI Log на Completed';

  @override
  String get rfiCompleteStatusEditorTitle =>
      'Проверка завершённых RFI и изменений статуса';

  @override
  String rfiCompleteStatusEditorRowCount(Object count) {
    return 'RFI с точным завершённым PDF: $count';
  }

  @override
  String get rfiCompleteStatusColumnEnabled => 'Применить';

  @override
  String get rfiCompleteStatusColumnDiscipline => 'Раздел';

  @override
  String get rfiCompleteStatusColumnRfiNo => 'RFI No.';

  @override
  String get rfiCompleteStatusColumnPdfPath => 'Завершённый PDF';

  @override
  String get rfiCompleteStatusColumnPdfCount => 'Количество PDF';

  @override
  String get rfiCompleteStatusColumnCurrentStatus => 'Текущий статус';

  @override
  String get rfiCompleteStatusColumnProposedStatus => 'Новый статус';

  @override
  String get rfiCompleteStatusColumnDisposition => 'Решение';

  @override
  String get rfiCompleteStatusColumnAction => 'Действие';

  @override
  String get rfiCompleteStatusColumnPdfPaths => 'Все совпавшие PDF';

  @override
  String get rfiCompleteStatusColumnReason => 'Доказательство / Подробности';

  @override
  String get rfiCompleteStatusDispositionReady => 'Готово';

  @override
  String get rfiCompleteStatusDispositionAlreadyCompleted => 'Уже Completed';

  @override
  String get rfiCompleteStatusDispositionManualReview => 'Ручная проверка';

  @override
  String get progressPhaseCompletionPdfDiscovery =>
      'Поиск завершённых PDF во всех папках системы RFI';

  @override
  String get progressPhaseCompletionWorkbookRead =>
      'Чтение ячеек статуса RFI Log';

  @override
  String get progressPhaseCompletionMapping =>
      'Сопоставление завершённых PDF со строками RFI Log';

  @override
  String get opFilesReplicaPublishTitle => 'Опубликовать системы RFI';

  @override
  String get opFilesReplicaPublishDesc =>
      'Публикует выбранную исходную систему RFI в две цели с сохраняемыми резервными копиями, проверкой hash и отменой.';

  @override
  String get paramReplicaRootA => 'Папка системы A';

  @override
  String get paramReplicaRootAHelp =>
      'Один из корней системы RFI: SharePoint, сеть или локальная копия.';

  @override
  String get paramReplicaRootB => 'Папка системы B';

  @override
  String get paramReplicaRootBHelp => 'Второй корень системы RFI.';

  @override
  String get paramReplicaRootC => 'Папка системы C';

  @override
  String get paramReplicaRootCHelp => 'Третий корень системы RFI.';

  @override
  String get paramReplicaSourceRoot => 'Авторитетный источник';

  @override
  String get paramReplicaSourceRootHelp =>
      'Эта система публикуется в две другие. Изменения целей не возвращаются в источник.';

  @override
  String get choiceReplicaSourceA => 'Система A';

  @override
  String get choiceReplicaSourceB => 'Система B';

  @override
  String get choiceReplicaSourceC => 'Система C';

  @override
  String get paramReplicaBackupRoot => 'Папка резервных версий';

  @override
  String get paramReplicaBackupRootHelp =>
      'Каждый заменяемый целевой файл сначала копируется в отдельную сохраняемую резервную копию.';

  @override
  String get paramReplicaRootFiles => 'Дополнительные корневые файлы';

  @override
  String get paramReplicaRootFilesHelp =>
      'Разделите точкой с запятой или новой строкой имена корневых файлов вне CIV, MEC, ELE и PRC.';

  @override
  String get paramReplicaUnattended =>
      'Включить автоматический режим без подтверждения';

  @override
  String get paramReplicaUnattendedHelp =>
      'Одно применение этого предпросмотра даёт постоянное разрешение только фиксированному профилю. Неопределённость приостанавливает автоматизацию.';

  @override
  String get paramReplicaIntervalMinutes => 'Интервал проверки (минуты)';

  @override
  String get paramReplicaIntervalMinutesHelp =>
      'От 1 до 1440 минут. Работает, пока DocFlow открыт.';

  @override
  String get paramReplicaSettleSeconds => 'Время стабилизации файла (секунды)';

  @override
  String get paramReplicaSettleSecondsHelp =>
      'Автопубликация ждёт это время после последнего изменения источника. От 5 до 3600.';

  @override
  String get paramReplicaMaxChangedFiles =>
      'Максимум файловых изменений за автозапуск';

  @override
  String get paramReplicaMaxChangedFilesHelp =>
      'Автоматизация приостанавливается, если один запуск изменит больше целевых файлов.';

  @override
  String get paramReplicaMaxChangedMib => 'Максимум MiB за автозапуск';

  @override
  String get paramReplicaMaxChangedMibHelp =>
      'Автоматизация приостанавливается, если общий объём публикации превысит предел.';

  @override
  String get replicaEditorTitle => 'Проверить точные эффекты публикации';

  @override
  String replicaEditorRowCount(Object count) {
    return 'Воздействий на файлы: $count';
  }

  @override
  String get replicaColumnTarget => 'Цель';

  @override
  String get replicaColumnPath => 'Относительный путь';

  @override
  String get replicaColumnAction => 'Действие';

  @override
  String get replicaColumnBeforeHash => 'Предыдущий hash';

  @override
  String get replicaColumnAfterHash => 'Hash источника';

  @override
  String get replicaActionCopy => 'Копировать новый файл';

  @override
  String get replicaActionReplace => 'Создать резервную копию и заменить';

  @override
  String get noteReplicaMkdir => 'Создать требуемую папку';

  @override
  String get noteReplicaCopy => 'Копировать отсутствующий целевой файл';

  @override
  String get noteReplicaReplace =>
      'Сохранить, проверить и заменить цель из источника';

  @override
  String get noteReplicaRestore =>
      'Восстановить цель из резервной копии до публикации';

  @override
  String get warnReplicaTargetExtra =>
      'В цели есть файл, отсутствующий в источнике; он не удалён.';

  @override
  String get warnReplicaTargetExtraOverflow =>
      'Другие файлы только в цели не поместились в список; ни один не удалён.';

  @override
  String get warnReplicaNoChanges =>
      'Все три системы уже совпадают с выбранным источником.';

  @override
  String get errorReplicaRootInvalid =>
      'Выберите три читаемых корня системы RFI.';

  @override
  String get errorReplicaBackupInvalid =>
      'Выберите отдельную читаемую папку резервных копий.';

  @override
  String get errorReplicaSourceInvalid =>
      'Выбран неверный авторитетный источник.';

  @override
  String get errorReplicaRootsOverlap =>
      'Корни систем RFI не могут совпадать или быть вложенными.';

  @override
  String get errorReplicaBackupOverlap =>
      'Папка резервных копий не может совпадать или пересекаться с корнем системы RFI.';

  @override
  String get errorReplicaRootFileInvalid =>
      'Неверное имя дополнительного корневого файла.';

  @override
  String get errorReplicaSourceRootFileMissing =>
      'Выбранный дополнительный корневой файл отсутствует в исходной системе.';

  @override
  String get errorReplicaTooManyRootFiles =>
      'Допускается не более 64 дополнительных корневых файлов.';

  @override
  String get errorReplicaIntervalInvalid =>
      'Интервал проверки должен быть от 1 до 1440 минут.';

  @override
  String get errorReplicaSettleInvalid =>
      'Время стабилизации файла должно быть от 5 до 3600 секунд.';

  @override
  String get errorReplicaChangeLimitInvalid =>
      'Пределы автоматических изменений должны быть положительными и находиться в поддерживаемом диапазоне.';

  @override
  String get errorReplicaSourceSettling =>
      'Файл источника недавно изменён; автопубликация отложена до его стабилизации.';

  @override
  String get errorReplicaChangeLimitExceeded =>
      'Автопубликация превышает утверждённый предел файлов или размера и приостановлена для безопасности.';

  @override
  String get errorReplicaDisciplineMissing =>
      'Источник должен содержать папки CIV, MEC, ELE и PRC.';

  @override
  String get errorReplicaScanIncomplete =>
      'Сканирование папок не завершено; файлы не опубликованы.';

  @override
  String get errorReplicaEntryLimit =>
      'Система RFI превышает предел безопасности в 500000 записей.';

  @override
  String get errorReplicaSymlinkRefused =>
      'Найдена символическая ссылка; безопасная публикация остановлена.';

  @override
  String get errorReplicaPathCollision =>
      'Несколько файлов соответствуют одному относительному пути.';

  @override
  String get errorReplicaFileUnstable =>
      'Файл изменился во время сканирования; повторите предпросмотр.';

  @override
  String get errorReplicaApprovedStepInvalid =>
      'Утверждённый шаг публикации неверен; повторите предпросмотр.';

  @override
  String get errorReplicaSourceChanged =>
      'Источник изменился после предпросмотра; публикация остановлена.';

  @override
  String get errorReplicaTargetChanged =>
      'Цель изменилась после предпросмотра; публикация остановлена.';

  @override
  String get errorReplicaBackupExists =>
      'Резервная копия этой публикации уже существует и не перезаписана.';

  @override
  String get errorReplicaBackupVerificationFailed =>
      'Проверка hash резервной копии не пройдена; автоматизация остановлена.';

  @override
  String get errorReplicaCopyVerificationFailed =>
      'Проверка hash временной копии не пройдена.';

  @override
  String get errorReplicaPublishVerificationFailed =>
      'Проверка hash после публикации не пройдена; требуется recovery.';

  @override
  String get errorReplicaProfileMissing =>
      'Нет утверждённого профиля автоматической синхронизации.';

  @override
  String get errorReplicaProfileInvalid =>
      'Сохранённый профиль автоматической синхронизации неверен.';

  @override
  String get errorReplicaActivationFailed =>
      'Файлы опубликованы, но автоматический профиль не активирован.';

  @override
  String get errorReplicaAutomaticApplyFailed =>
      'Автоматическая публикация не завершена и приостановлена.';

  @override
  String get progressPhaseReplicaDiscovery => 'Сканирование трёх систем RFI';

  @override
  String get progressPhaseReplicaSourceHash =>
      'Hash исходных файлов проверяется ограниченным числом ядер';

  @override
  String get progressPhaseReplicaTargetHash =>
      'Hash целевых файлов проверяется ограниченным числом ядер';

  @override
  String get prefsAutomation => 'Windows и автоматизация';

  @override
  String get prefsAutomationHelp =>
      'Управляйте запуском и ранее утверждённым фоновым профилем синхронизации.';

  @override
  String get prefsStartWithWindows => 'Запускать DocFlow вместе с Windows';

  @override
  String get prefsReplicaSyncEnabled =>
      'Синхронизация RFI без подтверждения активна';

  @override
  String get prefsReplicaSyncEnabledHelp =>
      'Любая ошибка или неопределённость автоматически отключает режим. Резервные копии не удаляются автоматически.';

  @override
  String get prefsReplicaStatusUnconfigured =>
      'Состояние: подтверждённого профиля ещё нет';

  @override
  String get prefsReplicaStatusDisabled => 'Состояние: выключено';

  @override
  String get prefsReplicaStatusIdle => 'Состояние: ожидание следующего запуска';

  @override
  String get prefsReplicaStatusRunning => 'Состояние: выполняется';

  @override
  String get prefsReplicaStatusWaitingForSignIn => 'Состояние: ожидание входа';

  @override
  String get prefsReplicaStatusDeferred =>
      'Состояние: отложено до завершения другой операции';

  @override
  String get prefsReplicaStatusPaused =>
      'Состояние: приостановлено для безопасности';

  @override
  String get paramRfiProcessScansCorrectOrientation =>
      'Исправлять поворот страниц';

  @override
  String get paramRfiProcessScansCorrectOrientationHelp =>
      'Если включено, поворачиваются только страницы с уверенно определённой ориентацией. Если хотя бы одна страница неоднозначна, коррекция для всего PDF пропускается, а файл целиком отправляется в _review без изменений.';

  @override
  String get warnRfiProcessScansOrientationReview =>
      'Не удалось надёжно определить ориентацию; PDF помещён в _review';

  @override
  String get progressPhaseScanOrientation => 'Проверка ориентации страниц PDF';

  @override
  String get rfiExtractionPages => 'Извлечённые страницы';

  @override
  String get reviewSelectAllKeep => 'Оставить все';

  @override
  String get reviewSelectAllPut => 'Переместить все возможные';

  @override
  String get reviewSelectAllTrash => 'Все в корзину';

  @override
  String get opsSearchPlaceholder => 'Поиск операций…';

  @override
  String get opsCategoryAll => 'Все';

  @override
  String get opsCategoryScans => 'Сканы и PDF';

  @override
  String get opsCategoryExcel => 'Excel и журналы';

  @override
  String get opsCategoryMail => 'Outlook и почта';

  @override
  String get opsCategorySystem => 'Публикация и печать';

  @override
  String opsEmptySearch(Object query) {
    return 'Нет операций, соответствующих \'$query\'';
  }

  @override
  String get actionOpenDestination => 'Открыть целевую папку';

  @override
  String get opRfiGenerateConcreteDocumentsTitle =>
      'Создание документов по бетону';

  @override
  String get opRfiGenerateConcreteDocumentsDesc =>
      'Пакетное создание документов по бетону из книг RFI Log и Follow Up с выбранными типами RFI и шаблонами.';

  @override
  String get paramRfiDocumentsRfiLog => 'Книга RFI Log';

  @override
  String get paramRfiDocumentsFollowUp => 'Книга Follow Up';

  @override
  String get paramRfiDocumentsTemplate010 =>
      'Шаблон 010 — поля Follow Up; температура = температура свежей смеси на заводе минус случайное значение 0,5–0,7';

  @override
  String get paramRfiDocumentsTemplate011 =>
      'Шаблон 011 — номер отчёта/место и настраиваемый случайный диапазон температуры (по умолчанию 19–24)';

  @override
  String get paramRfiDocumentsTemplate013 =>
      'Шаблон 013 — только номер отчёта/место';

  @override
  String get paramRfiDocumentsTemplate014 =>
      'Шаблон 014 — только номер отчёта/место';

  @override
  String get paramRfiDocumentsOutput => 'Папка результатов';

  @override
  String get paramRfiDocumentsRfis010 => 'Номера RFI для шаблона 010';

  @override
  String get paramRfiDocumentsRfis011 => 'Номера RFI для шаблона 011';

  @override
  String get paramRfiDocumentsRfis013 => 'Номера RFI для шаблона 013';

  @override
  String get paramRfiDocumentsRfis014 => 'Номера RFI для шаблона 014';

  @override
  String get paramRfiDocumentsWaterRatios =>
      'Необязательные соотношения воды и цемента для каждого RFI (если нет — оставить пустым)';

  @override
  String get paramRfiDocumentsAmbientMin => 'Минимальная температура воздуха';

  @override
  String get paramRfiDocumentsAmbientMax => 'Максимальная температура воздуха';

  @override
  String get paramRfiDocumentsAmbientGroups =>
      'Диапазоны температуры воздуха для групп RFI';

  @override
  String get errorParamRequiredOutputDirectory => 'Укажите папку результатов';

  @override
  String get errorRfiDocumentsInvalidInput =>
      'Неверные входные данные для документов по бетону';

  @override
  String get warnRfiDocumentsOutputExists =>
      'Файл результата уже существует; перезапись не выполняется';

  @override
  String get warnRfiDocumentsFollowUpMissing =>
      'В Concrete Specimen Test Follow-Up не найдена строка для этого RFI. Он будет пропущен. «Применить» создаст только доступные документы; нажмите «Отмена», чтобы исправить книгу и повторить просмотр.';

  @override
  String get warnRfiDocumentsFormNotRequired =>
      'В поле Forms журнала RFI для этого RFI не указан CON-CIV-010, поэтому документ CON-CIV-010 не будет создан. Проверьте выбранный тип документа или исправьте журнал RFI и повторите просмотр.';

  @override
  String get paramRfiDocumentsRfiLogHelp =>
      'Лист Civil содержит номер отчёта RFI и место для каждой выбранной записи.';

  @override
  String get paramRfiDocumentsFollowUpHelp =>
      'Нужна только для CON-CIV-010; содержит поля заливки бетона.';

  @override
  String get paramRfiDocumentsOutputHelp =>
      'Результаты создаются здесь; существующие файлы не перезаписываются.';

  @override
  String get paramRfiDocumentsRfisHelp =>
      'Введите полные номера RFI или серийные номера CIV через запятую или с новой строки.';

  @override
  String get paramRfiDocumentsWaterRatiosHelp =>
      'Необязательно: по одной строке «номер RFI = коэффициент». Для отсутствующего значения ячейка остаётся пустой.';

  @override
  String get paramRfiDocumentsAmbientGroupsHelp =>
      'Необязательно, одна строка на группу: RFI[, RFI...]=MIN-MAX. Границы диапазона включены; для неуказанных RFI используются значения по умолчанию Минимальная/Максимальная температура воздуха.';

  @override
  String get stepRfiGeneratedConcreteDocument => 'Создание документа по бетону';
}
