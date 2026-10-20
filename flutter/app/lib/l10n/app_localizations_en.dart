// ignore: unused_import
import 'package:intl/intl.dart' as intl;

import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for English (`en`).
class AppLocalizationsEn extends AppLocalizations {
  AppLocalizationsEn([String locale = 'en']) : super(locale);

  @override
  String get appTitle => 'DocFlow';

  @override
  String get actionPreview => 'Preview';

  @override
  String get actionApply => 'Apply';

  @override
  String get actionUndo => 'Undo';

  @override
  String get actionCancel => 'Cancel';

  @override
  String get actionBack => 'Back';

  @override
  String get actionBrowse => 'Browse';

  @override
  String get uiWarnings => 'Warnings';

  @override
  String resultApplied(int count) {
    return '$count applied';
  }

  @override
  String resultFailed(int count) {
    return '$count failed';
  }

  @override
  String get actionOpen => 'Open';

  @override
  String get actionReview => 'Review folder';

  @override
  String get reviewTitle => 'Review Files';

  @override
  String get reviewEmpty => 'The review folder is empty';

  @override
  String get reviewKeep => 'Keep as is';

  @override
  String get reviewPut => 'Move to destination';

  @override
  String get reviewDelete => 'Move to Trash';

  @override
  String get reviewNoCode => 'No RFI code';

  @override
  String get reviewWillReplace => 'replaces the existing file';

  @override
  String get noteDeleted => 'Deleted (to trash)';

  @override
  String get noteDisplaced => 'Replaced — moved to trash';

  @override
  String get actionHistory => 'History';

  @override
  String get historyTitle => 'History';

  @override
  String get historyUnknownOperation => 'Unknown operation';

  @override
  String get historyEmpty => 'No history yet';

  @override
  String historyMoves(int count) {
    return '$count files';
  }

  @override
  String get historyUndoOriginal => 'Undo to original location';

  @override
  String get historyUndoToFolder => 'Undo to a folder…';

  @override
  String get planEmpty => 'Nothing to do';

  @override
  String planSummary(int count) {
    return '$count files will be processed';
  }

  @override
  String get planImpactTitle => 'Before you apply';

  @override
  String get planImpactPreviewOnly =>
      'Nothing has changed yet. Apply executes only this approved preview.';

  @override
  String get planImpactOutputs => 'New outputs';

  @override
  String get planImpactUpdates => 'Files updated';

  @override
  String get planImpactMoves => 'Moved / renamed';

  @override
  String get planImpactPrintJobs => 'Print jobs';

  @override
  String get planImpactDrafts => 'Drafts opened';

  @override
  String get planImpactCleanup => 'Cleanup';

  @override
  String get planImpactWarnings => 'Warnings';

  @override
  String get scanIndexTitle => 'Incremental scan index';

  @override
  String get scanIndexHelp =>
      'Unchanged files were reused from the durable local index.';

  @override
  String get scanIndexForceApplied => 'Full rescan was requested for this run.';

  @override
  String get scanIndexDiscovered => 'Found';

  @override
  String get scanIndexReused => 'Reused';

  @override
  String get scanIndexAnalyzed => 'Scanned now';

  @override
  String get scanIndexFailed => 'Failed';

  @override
  String get scanIndexInterrupted => 'Earlier interrupted runs';

  @override
  String get planWarningsSearch => 'Search warnings';

  @override
  String get planWarningsType => 'Warning type';

  @override
  String get planWarningsAll => 'All warning types';

  @override
  String get planWarningsFocus => 'Show related page';

  @override
  String get planWarningsNone => 'No warnings match these filters.';

  @override
  String planWarningsShowing(Object count, Object shown) {
    return 'Showing $shown of $count warnings';
  }

  @override
  String get planWorkbookChangesTitle => 'Exact workbook cell changes';

  @override
  String planWorkbookChangesCount(Object count) {
    return '$count approved cell changes';
  }

  @override
  String get planWorkbookChangesHelp =>
      'Expand to compare the current value with the exact value Apply will write.';

  @override
  String get planWorkbookChangesWorkbook => 'Workbook';

  @override
  String get planWorkbookChangesSheetCell => 'Sheet and cell';

  @override
  String get planWorkbookChangesRfi => 'RFI';

  @override
  String get planWorkbookChangesBefore => 'Current';

  @override
  String get planWorkbookChangesAfter => 'After Apply';

  @override
  String get planWorkbookChangesBlank => '(blank)';

  @override
  String get planWorkbookChangesKindRfiLog => 'RFI Log';

  @override
  String get planWorkbookChangesKindFollowUp => 'Follow Up';

  @override
  String get planWorkbookChangesKindWorkbook => 'Workbook';

  @override
  String planWorkbookChangesLimit(Object count) {
    return 'The preview is limited to the first $count approved cell changes.';
  }

  @override
  String planExactTitle(Object count) {
    return '$count approved effects';
  }

  @override
  String get planExactHelp =>
      'Open to inspect exact source and destination paths';

  @override
  String get opRfiProcessScansTitle => 'Process Scans';

  @override
  String get opRfiProcessScansFastCoversTitle => 'Fast RFI Scan';

  @override
  String get opRfiProcessScansFastCoversDesc =>
      'Scans RFI covers in multi-page scans. Choose Multi-RFI to separate them or Single-RFI to read only page one and keep the complete PDF. OCR-only CLV readings are normalized to CIV. Uncertain scans go to _review.';

  @override
  String get paramRfiProcessScansScanMode => 'Scan mode';

  @override
  String get paramRfiProcessScansScanModeHelp =>
      'Multi-RFI separates a combined scan. Single-RFI reads only its first page and keeps the complete PDF intact.';

  @override
  String get choiceRfiProcessScansScanModeMultiRfi => 'Separate multiple RFIs';

  @override
  String get choiceRfiProcessScansScanModeSingleRfi =>
      'One RFI — read first page only';

  @override
  String get rfiFastScanEditorTitle => 'Review and correct RFI numbers';

  @override
  String rfiFastScanEditorRowCount(int count) {
    return '$count scan files';
  }

  @override
  String get rfiFastScanColumnSourceFile => 'Source file';

  @override
  String get rfiFastScanColumnEvidence => 'Recognition';

  @override
  String get rfiFastScanColumnDetectedCode => 'Detected RFI';

  @override
  String get rfiFastScanColumnCode => 'RFI number';

  @override
  String get rfiFastScanColumnTarget => 'Destination';

  @override
  String get rfiFastScanEvidenceClvToCiv => 'OCR alias: CLV → CIV';

  @override
  String get rfiFastScanEvidenceConfirmed => 'Confirmed';

  @override
  String get rfiFastScanEditorUpdatePreview => 'Update preview';

  @override
  String get rfiFastScanEditorUpdatingPreview => 'Updating preview…';

  @override
  String get rfiFastScanEditorPreviewUpdated =>
      'Preview updated. Check the destination paths before applying.';

  @override
  String get errorRfiFastScanCodeInvalid =>
      'Enter a supported full RFI code with a one-to-four ASCII-digit suffix; it is stored with four digits.';

  @override
  String get errorRfiFastScanTargetExists =>
      'Another scan already uses this RFI number, or the destination already exists.';

  @override
  String get errorRfiFastScanRevisionInvalid =>
      'The edited scan preview is invalid. Review the RFI numbers and try again.';

  @override
  String get opRfiGroupFilesTitle => 'Group Files by RFI';

  @override
  String get opRfiGroupFilesDesc =>
      'Finds exact RFI numbers in file names and contents, then copies approved files to <discipline>/<full RFI code> folders.';

  @override
  String get paramRfiFileGroupSource => 'Incoming files folder';

  @override
  String get paramRfiFileGroupSourceHelp =>
      'Only top-level files are scanned; subfolders are left unchanged and skipped.';

  @override
  String get paramRfiFileGroupDestination => 'Grouped output folder';

  @override
  String get paramRfiFileGroupDestinationHelp =>
      'Choose an empty or separate target outside the source. Existing files are never overwritten.';

  @override
  String get rfiFileGroupEditorTitle => 'Detected RFI matches';

  @override
  String rfiFileGroupEditorRowCount(Object count) {
    return '$count copy matches';
  }

  @override
  String get rfiFileGroupColumnEnabled => 'Copy';

  @override
  String get rfiFileGroupColumnRfiCode => 'RFI number';

  @override
  String get rfiFileGroupColumnSourceFile => 'Source file';

  @override
  String get rfiFileGroupColumnMethod => 'Detection method';

  @override
  String get rfiFileGroupMethodFilename => 'File name';

  @override
  String get rfiFileGroupMethodPdf => 'PDF / OCR';

  @override
  String get rfiFileGroupMethodMail => 'Mail content';

  @override
  String get rfiFileGroupMethodOffice => 'Office content';

  @override
  String get rfiFileGroupMethodContent => 'File content';

  @override
  String get noteRfiFileGroupFolderCreated => 'RFI folder will be created';

  @override
  String get noteRfiFileGroupCopy =>
      'File will be copied into the discipline and full RFI-code folder';

  @override
  String get noteRfiFileGroupReviewCopy =>
      'File will be copied into the review folder';

  @override
  String get warnRfiFileGroupNoCode =>
      'No RFI number was found; the file will be placed in review';

  @override
  String get warnRfiFileGroupUnreadable =>
      'The file could not be fully read; it will be placed in review';

  @override
  String get warnRfiFileGroupPartialInspection =>
      'File-name matches are usable, but the content could not be fully inspected';

  @override
  String get warnRfiFileGroupExists =>
      'The destination file already exists and will not be overwritten';

  @override
  String get warnRfiFileGroupNoFiles =>
      'No top-level files were found in the source folder';

  @override
  String get warnRfiFileGroupMissingEmail =>
      'No EML or MSG email file was found for this RFI; related files will not be placed';

  @override
  String get errorParamRequiredDestination => 'Destination folder is required';

  @override
  String get paramSource => 'Source folder';

  @override
  String get paramSourceHelp => 'Folder containing the scanned PDFs';

  @override
  String get paramDest => 'Destination folder';

  @override
  String get paramDestHelp => 'Where processed RFI PDFs will be saved';

  @override
  String get paramKeepOriginal => 'Keep originals (copy instead of move)';

  @override
  String get noteRenamed => 'Renamed';

  @override
  String get noteNeedsReview => 'Will be moved to review';

  @override
  String get noteUnreadable => 'Unreadable — will be moved to review';

  @override
  String get warnNoCode => 'RFI number not found';

  @override
  String get warnLowConfidence => 'Low confidence — needs review';

  @override
  String get warnUnknownRfiFolder =>
      'Detected RFI folder does not exist — moved to review';

  @override
  String get warnUnreadable => 'File could not be read';

  @override
  String get warnRevisionGuessed => 'Revision number guessed';

  @override
  String get warnDuplicate => 'Duplicate RFI number — moved to review';

  @override
  String get warnExists =>
      'Target already exists — moved to review (not overwritten)';

  @override
  String get warnNoPdfs => 'No PDFs in folder';

  @override
  String get errorStepFailed => 'Step failed';

  @override
  String get errorUndoFailed => 'Undo failed';

  @override
  String get errorParamRequiredSource => 'Source folder is required';

  @override
  String get errorParamRequiredDest => 'Destination folder is required';

  @override
  String get actionAdmin => 'Admin panel';

  @override
  String get actionRetry => 'Check again';

  @override
  String get bootstrapFatalTitle => 'DocFlow couldn\'t start';

  @override
  String get bootstrapRetry => 'Try again';

  @override
  String get authEmail => 'Email';

  @override
  String get authPassword => 'Password';

  @override
  String get authSignIn => 'Sign in';

  @override
  String get authSignUp => 'Create account';

  @override
  String get authSignOut => 'Sign out';

  @override
  String get authSignupOk =>
      'Account created. Ask the admin to approve it, then sign in.';

  @override
  String get authPending => 'Waiting for approval';

  @override
  String get authPendingHelp =>
      'Your account exists but has not been approved yet. The admin must grant you access.';

  @override
  String get adminTitle => 'Users';

  @override
  String get adminAllowed => 'Access';

  @override
  String get adminAdmin => 'Admin';

  @override
  String get adminEmpty => 'No users yet';

  @override
  String get actionDone => 'Done';

  @override
  String get adminHelp =>
      'Turn on Access to let someone use the app. Your own switches are locked so you cannot lock yourself out.';

  @override
  String get adminNoAccess => 'no access';

  @override
  String get adminYou => 'you';

  @override
  String get authSubtitle => 'Sign in to continue';

  @override
  String get authShow => 'Show password';

  @override
  String get authHide => 'Hide password';

  @override
  String get authPasswordHint => 'At least 6 characters';

  @override
  String get historyEmptyHelp =>
      'Operations you apply will be listed here, and you can undo them.';

  @override
  String get historyUndone => 'Undone';

  @override
  String get historyLocalJournalRequired =>
      'The local journal on this device is required to undo.';

  @override
  String get historyCloud => 'Cloud';

  @override
  String get historyLocal => 'Local';

  @override
  String get historySourceCloud =>
      'Shared history, visible on all your machines';

  @override
  String get historySourceLocal => 'Local history only — cloud unreachable';

  @override
  String get opsHeading => 'Operations';

  @override
  String get opsSubheading =>
      'Choose an operation. Nothing will change on disk until you review and approve.';

  @override
  String get homeTitle => 'Workspace';

  @override
  String get homeSubtitle =>
      'Document operations and every supporting tool in one place.';

  @override
  String get homePreviewFirst => 'Preview first';

  @override
  String get ocrConfidence => 'OCR evidence score';

  @override
  String get ocrConfidenceShort => 'Evidence';

  @override
  String get ocrConfidenceHelp =>
      'Heuristic OCR consistency score out of 100. It is not an accuracy probability, confirmation, or automatic approval.';

  @override
  String get homeFeatures => 'Other features';

  @override
  String get homeFeaturesHelp =>
      'Review, history, account, and application management.';

  @override
  String get homeReviewDesc =>
      'Inspect pending files individually and place them safely.';

  @override
  String get homeHistoryDesc =>
      'See completed operations and safely undo them.';

  @override
  String get homeSettingsDesc =>
      'Manage language, appearance, text, and account preferences.';

  @override
  String get homeAdminDesc =>
      'Manage user access and administrator permissions.';

  @override
  String get paramHintFolder => 'Choose a folder';

  @override
  String get paramHintFile => 'Choose a file';

  @override
  String get planApplying => 'Applying';

  @override
  String get planReadOnly => 'Nothing has changed yet';

  @override
  String get prefsTitle => 'Settings';

  @override
  String get prefsLanguage => 'Language';

  @override
  String get prefsLangSystem => 'System language';

  @override
  String get prefsLangTr => 'Türkçe';

  @override
  String get prefsLangEn => 'English';

  @override
  String get prefsLangRu => 'Русский';

  @override
  String get prefsLangAz => 'Azərbaycanca';

  @override
  String get prefsTheme => 'Theme';

  @override
  String get prefsThemeSystem => 'System';

  @override
  String get prefsThemeLight => 'Light';

  @override
  String get prefsThemeDark => 'Dark';

  @override
  String get prefsFont => 'Font';

  @override
  String get prefsFontHelp =>
      'Uses fonts already installed on this PC — nothing is downloaded.';

  @override
  String get prefsFontSystem => 'System default';

  @override
  String get prefsFontSans => 'Sans serif';

  @override
  String get prefsFontSerif => 'Serif';

  @override
  String get prefsFontMono => 'Monospace';

  @override
  String get prefsFontSize => 'Text size';

  @override
  String get prefsFontSizeSample =>
      'AZE-SUN-INT-QMS-RFI-CIV-0064 — sample text at this size.';

  @override
  String get prefsDensity => 'Spacing';

  @override
  String get prefsDensityCompact => 'Compact layout';

  @override
  String get prefsDensityHelp =>
      'Fits more on screen. Useful on small displays.';

  @override
  String get prefsSupportTitle => 'Advanced support';

  @override
  String get prefsSupportHelp =>
      'Create a read-only, redacted local support bundle.';

  @override
  String get prefsSupportPrivacy =>
      'Includes the version, platform, sidecar/OCR status, recent error codes, and the latest pathless printer-queue result; document contents, credentials, tokens, and private paths are excluded. Nothing is sent anywhere.';

  @override
  String get prefsSupportPreview => 'Preview support bundle';

  @override
  String get prefsSupportPreviewLabel => 'Support bundle preview';

  @override
  String prefsSupportSize(Object count) {
    return '$count bytes';
  }

  @override
  String get prefsSupportCopy => 'Copy bundle to clipboard';

  @override
  String get prefsSupportCopied => 'Support bundle copied to the clipboard.';

  @override
  String get prefsSupportDownload => 'Download support bundle';

  @override
  String get prefsSupportDownloaded => 'Support bundle downloaded.';

  @override
  String get paramDropFiles => 'Drop files here or click to add';

  @override
  String get paramDropFolder => 'Drop folder here or browse';

  @override
  String get paramDropFile => 'Drop file here or browse';

  @override
  String get actionRecent => 'Recent selections';

  @override
  String get actionToggleTheme => 'Toggle theme (Light / Dark / System)';

  @override
  String get filterSearchPlaceholder =>
      'Search (RFI no, status, description)...';

  @override
  String get filterAll => 'All';

  @override
  String get filterChanges => 'Changes';

  @override
  String get filterManualReview => 'Manual Review';

  @override
  String tableFilterCount(Object filtered, Object total) {
    return 'Showing $filtered of $total rows';
  }

  @override
  String get prefsReset => 'Reset to defaults';

  @override
  String get reviewHelp =>
      'Decide what happens to each file. Nothing is changed until you approve the plan.';

  @override
  String get reviewEmptyHelp => 'No files are waiting for review.';

  @override
  String get reviewNoChanges => 'Nothing selected';

  @override
  String reviewSelected(Object count) {
    return '$count selected';
  }

  @override
  String reviewToTrash(Object count) {
    return '$count to trash';
  }

  @override
  String get reviewTrashHint =>
      'Deleted files go to recoverable trash, not permanently removed.';

  @override
  String get stepForm => 'Configure';

  @override
  String get stepPreview => 'Preview';

  @override
  String get stepApply => 'Apply';

  @override
  String get actionSave => 'Save';

  @override
  String get authUsername => 'Username';

  @override
  String get authUsernameHelp =>
      '3-24 characters: letters, numbers, dot, dash, underscore';

  @override
  String get authIdentifier => 'Username or email';

  @override
  String get authIdentifierHelp => 'Either one works';

  @override
  String get authEmailSignInHelp =>
      'Username sign-in is unavailable. Use your account email address.';

  @override
  String get authSubtitleUp => 'Create an account to get started';

  @override
  String get authErrCredentials => 'Wrong username/email or password.';

  @override
  String get authErrUsernameTaken => 'That username is taken.';

  @override
  String get authErrUsernameInvalid =>
      'Username must be 3-24 characters: letters, numbers, dot, dash, underscore.';

  @override
  String get authErrEmailTaken => 'An account with this email already exists.';

  @override
  String get prefsAccount => 'Account';

  @override
  String get prefsAccountEmailFixed => 'cannot be changed';

  @override
  String get prefsAccountSaved => 'Saved';

  @override
  String get authErrEmailNotConfirmed =>
      'This account exists but the email is not confirmed yet. The admin can turn off email confirmation, or open the link sent to your inbox.';

  @override
  String get authErrRateLimit =>
      'Too many attempts. Wait a few minutes and try again.';

  @override
  String get authErrWeakPassword =>
      'Password is too weak. Use at least 6 characters.';

  @override
  String get warnNoOcr =>
      'OCR engine (Tesseract) not found — no files were touched. Install it, or reinstall the app bundle.';

  @override
  String get noteFolderCreated => 'New folder';

  @override
  String get warnOcrBroken =>
      'OCR failed on every file — this is an installation problem, not a file problem. Nothing was touched. Details:';

  @override
  String get opFilesBatchPrintTitle => 'Print multiple files';

  @override
  String get opFilesBatchPrintDesc =>
      'Builds one ordered native print queue from PDFs, Office documents and supported images.';

  @override
  String get paramPrintFiles => 'Files to print';

  @override
  String get paramPrintFilesHelp =>
      'Add mixed file types, then reorder or remove them. The displayed order is the print order.';

  @override
  String get paramPrintRfiRoot => 'RFI System folder';

  @override
  String get paramPrintRfiRootHelp =>
      'Search this folder and its subfolders without changing any source files.';

  @override
  String get paramPrintRfiNumbers => 'RFI numbers';

  @override
  String get paramPrintRfiNumbersHelp =>
      'Enter one full RFI code, discipline-number code such as CIV-1319, or a one-to-four-digit number per line; short numbers are padded to four digits.';

  @override
  String get paramPrintIncludeFirstPage => 'First Page documents';

  @override
  String get paramPrintIncludeSketch => 'Sketch documents';

  @override
  String get printRfiTitle => 'Find RFI documents';

  @override
  String get printRfiInputs => 'Include these RFI requests';

  @override
  String get printRfiRootRequired => 'Select an RFI System folder first.';

  @override
  String get printRfiInputRequired =>
      'Select at least one RFI request before searching.';

  @override
  String get printRfiTypesRequired =>
      'Select First Page, Sketch, or both before searching.';

  @override
  String get printRfiSearch => 'Search RFI System';

  @override
  String get printRfiSearching => 'Searching…';

  @override
  String printRfiFound(Object count) {
    return '$count matching documents';
  }

  @override
  String get printRfiAmbiguous =>
      'This number exists in multiple disciplines. Choose the intended files explicitly.';

  @override
  String get printRfiMissingFirstPage => 'First Page document not found.';

  @override
  String get printRfiMissingSketch => 'Sketch document not found.';

  @override
  String get printRfiToggleRequest => 'Toggle this RFI';

  @override
  String get printRfiSelectTypeFirstPage => 'Select First Pages';

  @override
  String get printRfiSelectTypeSketch => 'Select Sketches';

  @override
  String get printRfiClearTypeFirstPage => 'Clear First Pages';

  @override
  String printRfiSelectDiscipline(Object discipline) {
    return 'Select $discipline';
  }

  @override
  String get printRfiClearTypeSketch => 'Clear Sketches';

  @override
  String get printRfiTypeFirstPage => 'First Page';

  @override
  String get printRfiTypeSketch => 'Sketch';

  @override
  String get printRfiNoMatches => 'No matching documents were found.';

  @override
  String get printRfiTruncated =>
      'Search reached its safe scan limit. Some folders may not have been searched.';

  @override
  String get actionSelectAll => 'Select all';

  @override
  String get actionSelectNone => 'Select none';

  @override
  String get errorPrintRfiInvalidRequest =>
      'Check the RFI requests and selected document types.';

  @override
  String get errorPrintRfiRootEscape =>
      'A discovered file changed outside the selected RFI System. Search again.';

  @override
  String get errorPrintRfiChanged =>
      'A discovered file changed during search. Search again.';

  @override
  String get paramPrintPrinter => 'Printer';

  @override
  String get paramPrintPrinterHelp =>
      'Uses printers currently installed on this computer.';

  @override
  String get paramPrintCopies => 'Copies';

  @override
  String get paramPrintCopiesHelp => 'From 1 to 99 copies of the queue.';

  @override
  String get paramPrintCollate => 'Collate copies';

  @override
  String get paramPrintCollateHelp =>
      'Print the complete ordered set before starting the next copy.';

  @override
  String get errorParamRequiredFiles => 'Select at least one file';

  @override
  String get errorParamRequiredPrinter => 'Select a printer';

  @override
  String get errorParamRequiredCopies => 'Number of copies is required';

  @override
  String get actionAddFiles => 'Add files';

  @override
  String get actionClear => 'Clear';

  @override
  String get actionMoveUp => 'Move up';

  @override
  String get actionMoveDown => 'Move down';

  @override
  String get actionRemove => 'Remove';

  @override
  String get actionDecrease => 'Decrease';

  @override
  String get actionIncrease => 'Increase';

  @override
  String get paramHintFiles => 'No files selected';

  @override
  String printFilesSelected(Object count) {
    return '$count files selected';
  }

  @override
  String get notePrintQueued => 'Ready to print';

  @override
  String get warnPrintNoPrinters => 'No printers were found';

  @override
  String get warnPrintPrinterUnavailable =>
      'The selected printer is no longer available';

  @override
  String get warnPrintInvalidCopies => 'Copies must be between 1 and 99';

  @override
  String get warnPrintTooManyFiles => 'A queue can contain at most 250 files';

  @override
  String get warnPrintFileMissing => 'File is missing and will be skipped';

  @override
  String get warnPrintWorkbookDiscoveryFailed =>
      'The workbook could not be inspected safely and was not added to the print order';

  @override
  String get warnPrintWorkbookTooManyWorksheets =>
      'The workbook contains too many worksheets';

  @override
  String get warnPrintExpandedOrderTooLarge =>
      'The expanded print order can contain at most 250 jobs';

  @override
  String get warnPrintWorkbookUnsafeXlm =>
      'The workbook contains an Excel 4.0 macro sheet and was rejected';

  @override
  String get warnPrintWorkbookUnsafeProtection =>
      'The workbook structure is protected and was rejected';

  @override
  String get warnPrintWorkbookUnsafeExternalLinks =>
      'The workbook contains external links and was rejected';

  @override
  String get warnPrintWorkbookUnsafeConnections =>
      'The workbook contains external data connections and was rejected';

  @override
  String get warnPrintWorkbookUnsafeQueryTables =>
      'The workbook contains query tables and was rejected';

  @override
  String get warnPrintNoHandler =>
      'No native print handler is available for this file type; install or choose a default app that can print it';

  @override
  String get warnPrintUnsupportedOptions =>
      'The selected printer or native application does not support these print options';

  @override
  String get warnPrintNonUndoable =>
      'Printing cannot be undone. Files run strictly one at a time; if an app asks for confirmation, the queue waits before starting the next file.';

  @override
  String get errorPrintInvalidPlan =>
      'The print plan is invalid. Preview the queue again.';

  @override
  String get errorPrintFailed => 'Print submission failed';

  @override
  String get errorPrintQueueStopped =>
      'Strict print order was protected; remaining files were not submitted';

  @override
  String get errorPrintPageRangeInvalid =>
      'Enter all or a comma-separated list of pages and ranges, such as 1-3,5.';

  @override
  String get errorPrintPageRangeBounds =>
      'The selected page range exceeds this PDF\'s page count.';

  @override
  String get errorPrintPageRangeImageUnsupported =>
      'Images accept only all or page 1.';

  @override
  String get errorPrintPageRangeNativeUnsupported =>
      'Files printed by a native application accept only all pages.';

  @override
  String get errorPrintWorksheetPageRangeAllOnly =>
      'Selected worksheets accept only all pages.';

  @override
  String get errorPrintWorkbookChanged =>
      'The workbook changed after preview. Preview the print order again.';

  @override
  String get errorPrintWorksheetChanged =>
      'The worksheet structure changed after preview. Preview the print order again.';

  @override
  String get errorPrintExcelUnavailable =>
      'Excel worksheet export is not available on this computer.';

  @override
  String get errorPrintExcelExportTimedOut =>
      'Excel worksheet export timed out before printing.';

  @override
  String get errorPrintExcelExportCancelled =>
      'Excel worksheet export was cancelled before printing.';

  @override
  String get errorPrintExcelExportQuarantined =>
      'Excel export is disabled until DocFlow restarts because the previous Excel worker could not be closed safely.';

  @override
  String get errorPrintExportCleanupRequiresReview =>
      'Private Excel export cleanup could not be confirmed. Restart DocFlow and review the temporary export state before retrying.';

  @override
  String get errorPrintExcelSourceMotwBlocked =>
      'The workbook is blocked because its Windows download-zone metadata is unsafe, malformed, oversized, or unreadable.';

  @override
  String get errorPrintExcelWorkbookUnsafe =>
      'The workbook requires unsafe protection, link, connection, query, or Excel 4.0 macro handling and was not opened.';

  @override
  String get errorPrintExcelComContractInvalid =>
      'The approved Excel automation request is invalid and was not opened.';

  @override
  String get errorPrintExcelComFailed =>
      'Excel automation failed before printing.';

  @override
  String get errorPrintExcelExportDestinationExists =>
      'The private worksheet PDF destination already exists, so export was stopped.';

  @override
  String get errorPrintExcelExportFailed =>
      'Excel could not export the selected worksheet to PDF.';

  @override
  String get errorPrintExcelExportMalformedPdf =>
      'Excel produced an empty or invalid worksheet PDF, so nothing was printed.';

  @override
  String get errorPrintExcelPrinterContextUnavailable =>
      'Windows could not create the approved printer context for the Excel worksheet.';

  @override
  String get errorPrintExcelPrintJobRejected =>
      'Windows rejected the controlled Excel worksheet print job before submission began.';

  @override
  String get errorPrintExcelPrintPreparationFailed =>
      'Windows could not prepare the Excel worksheet for controlled printing.';

  @override
  String get errorPrintExcelPrintOptionsRejected =>
      'The Windows printer rejected the approved Excel worksheet print settings.';

  @override
  String get errorPrintExcelPrintCancelledUnconfirmed =>
      'Cancellation could not be confirmed after the Excel worksheet print job entered the spooler. Check the printer before retrying.';

  @override
  String get errorPrintExcelPrintCompletionUnconfirmed =>
      'Windows could not confirm completion after the Excel worksheet print job entered the spooler. Check the printer before retrying.';

  @override
  String get errorPrintExcelControlledPdfUnavailable =>
      'Controlled PDF printing for Excel worksheets is not available in this build.';

  @override
  String get errorPrintExcelWorkerStartFailed =>
      'The isolated Excel export worker could not start.';

  @override
  String get errorPrintExcelDeadlineInvalid =>
      'The Excel export timeout is invalid.';

  @override
  String get errorPrintExcelWorksheetMetadataMissing =>
      'The approved worksheet metadata is missing. Preview the print order again.';

  @override
  String get errorPrintExcelSafetyMetadataMissing =>
      'The approved workbook safety metadata is missing. Preview the print order again.';

  @override
  String get errorPrintExcelGenerationExhausted =>
      'Excel export cannot continue safely until DocFlow restarts.';

  @override
  String get errorPrintExcelWorkerStopped =>
      'The isolated Excel export worker stopped unexpectedly.';

  @override
  String get errorPrintExcelResponseStale =>
      'A stale Excel export response was rejected before printing.';

  @override
  String get errorPrintExcelStagingUnavailable =>
      'Private workbook staging is unavailable.';

  @override
  String get errorPrintExcelSourceNameUnavailable =>
      'The approved workbook file name is unavailable.';

  @override
  String get errorPrintExcelStagingFailed =>
      'The workbook could not be copied into private staging.';

  @override
  String get errorPrintExcelStagingVerificationFailed =>
      'The private workbook copy could not be verified against the approved source.';

  @override
  String get errorPrintExcelPrivatePdfUnavailable =>
      'The private worksheet PDF is unavailable.';

  @override
  String get errorPrintExcelPrivatePdfEmpty =>
      'Excel produced an empty worksheet PDF, so nothing was printed.';

  @override
  String get printWaitingForCurrent =>
      'Waiting for this file to finish before starting the next one';

  @override
  String get paramPrintPaper => 'Paper size';

  @override
  String get paramPrintPaperHelp =>
      'Default for all files; you can override it per file in Preview.';

  @override
  String get paramPrintOrientation => 'Orientation';

  @override
  String get paramPrintScaling => 'Page scaling';

  @override
  String get paramPrintColor => 'Color mode';

  @override
  String get paramPrintDuplex => 'Two-sided printing';

  @override
  String get paramPrintRetries => 'Automatic retries';

  @override
  String get paramPrintRetriesHelp =>
      'Retries a failed job 0 to 3 times before applying the queue error rule.';

  @override
  String get paramPrintOnError => 'If a job still fails';

  @override
  String get paramPrintOnErrorHelp =>
      'Pause protects order; Skip continues and leaves the failed job available for retry.';

  @override
  String get paramPrintTimeout => 'Job timeout (minutes)';

  @override
  String get paramPrintTimeoutHelp =>
      'Stops waiting when a printer remains blocked, offline or unresponsive.';

  @override
  String get choicePrintPaperAuto => 'Printer default';

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
  String get choicePrintOrientationAuto => 'Auto from page';

  @override
  String get choicePrintOrientationPortrait => 'Portrait';

  @override
  String get choicePrintOrientationLandscape => 'Landscape';

  @override
  String get choicePrintScalingFit => 'Fit to printable area';

  @override
  String get choicePrintScalingActual => 'Actual size';

  @override
  String get choicePrintScalingShrink => 'Shrink oversized pages only';

  @override
  String get choicePrintColorPrinter => 'Printer default';

  @override
  String get choicePrintColorColor => 'Color';

  @override
  String get choicePrintColorGrayscale => 'Grayscale';

  @override
  String get choicePrintDuplexPrinter => 'Printer default';

  @override
  String get choicePrintDuplexSimplex => 'One-sided';

  @override
  String get choicePrintDuplexLong => 'Two-sided, long edge';

  @override
  String get choicePrintDuplexShort => 'Two-sided, short edge';

  @override
  String get choicePrintOnErrorPause => 'Pause queue';

  @override
  String get choicePrintOnErrorSkip => 'Skip and continue';

  @override
  String get printEditorTitle => 'Review and customize the print queue';

  @override
  String printEditorRowCount(Object count) {
    return '$count files/jobs';
  }

  @override
  String get printColInclude => 'Print';

  @override
  String get printColStatus => 'Status';

  @override
  String get printColFile => 'File';

  @override
  String get printColFormat => 'Format';

  @override
  String get printColWorksheet => 'Worksheet';

  @override
  String get printColWorksheetVisibility => 'Visibility';

  @override
  String get printWorksheetVisibilityVisible => 'Visible';

  @override
  String get printWorksheetVisibilityHidden => 'Hidden';

  @override
  String get printWorksheetVisibilityVeryHidden => 'Very hidden';

  @override
  String get printColPrinter => 'Printer';

  @override
  String get printColPaper => 'Paper';

  @override
  String get printColOrientation => 'Orientation';

  @override
  String get printColScaling => 'Scaling';

  @override
  String get printColColor => 'Color';

  @override
  String get printColDuplex => 'Duplex';

  @override
  String get printColCopies => 'Copies';

  @override
  String get printColPageRange => 'Page range';

  @override
  String get printColPageRangeHelp =>
      'Use all, a page, or comma-separated ranges. Images accept all or 1; native-application files accept only all.';

  @override
  String get printStatusReady => 'Ready';

  @override
  String get printStatusPending => 'Pending';

  @override
  String get printStatusPrinting => 'Printing';

  @override
  String get printStatusCompleted => 'Completed';

  @override
  String get printStatusCompletedUnconfirmed => 'Sent; receipt unavailable';

  @override
  String get printStatusFailed => 'Failed';

  @override
  String get printStatusCancelled => 'Cancelled';

  @override
  String get printStatusUncertain => 'Check printer';

  @override
  String get warnPrintSettingsLimited =>
      'This format is printed by its native application; paper and layout support depends on that application';

  @override
  String get warnPrintNoJobsSelected => 'No print rows are selected';

  @override
  String get warnPrintInvalidRetries =>
      'Automatic retries must be between 0 and 3';

  @override
  String get warnPrintInvalidTimeout =>
      'Job timeout must be between 1 and 240 minutes';

  @override
  String get warnPrintResumingQueue =>
      'Reviewing an unfinished saved print queue';

  @override
  String get warnPrintCancelled => 'The print queue was paused by the user';

  @override
  String get warnPrintCancelledUncertain =>
      'The system had not identified the current spool job when cancellation arrived. Check the printer before selecting it again.';

  @override
  String get warnPrintStateUncertain =>
      'The system could not confirm whether this job printed. It was not retried; check the output before selecting it again.';

  @override
  String get warnPrintSubmittedUnconfirmed =>
      'Windows accepted this job but did not retain a final receipt. It was not retried, and the queue continued to the next file.';

  @override
  String get errorPrintQueuePaused =>
      'The queue was paused; remaining jobs were kept for Resume';

  @override
  String get errorPrintQueueSourceReselectionRequired =>
      'This older print queue cannot be resumed safely. Discard it, then reselect the files.';

  @override
  String get printResumeTitle => 'Unfinished print queue found';

  @override
  String printResumeSummary(Object count) {
    return '$count jobs can be reviewed or resumed';
  }

  @override
  String get printResumeReview => 'Retry unprinted only';

  @override
  String get printResumeDiscard => 'Discard queue';

  @override
  String get paramsPreviousTitle => 'Previous print settings available';

  @override
  String get paramsPreviousHelp =>
      'Files are not retained for safety. Restore the previous printer, paper, orientation, color, and queue settings only when you choose.';

  @override
  String get paramsPreviousApply => 'Use previous settings';

  @override
  String get printResumeIncomplete => 'The queue is safely paused';

  @override
  String get printCancelQueue => 'Pause queue';

  @override
  String get printCancelling => 'Pausing…';

  @override
  String get editorActionShowDetails => 'Show automatic fields';

  @override
  String get editorActionHideDetails => 'Hide automatic fields';

  @override
  String get layoutDataResetHeight => 'Reset height';

  @override
  String get layoutDataFit => 'Fit available space';

  @override
  String layoutDataResizeHeight(Object label) {
    return 'Resize $label height';
  }

  @override
  String layoutColumnResize(Object column) {
    return 'Resize $column column';
  }

  @override
  String layoutColumnWidth(Object width) {
    return '$width pixels wide';
  }

  @override
  String get editorBulkTitle => 'Bulk edit';

  @override
  String editorSelectedCount(Object count) {
    return '$count selected';
  }

  @override
  String get editorSelectAll => 'Select all';

  @override
  String get editorSelectNone => 'Clear selection';

  @override
  String editorSelectRow(Object row) {
    return 'Select row $row';
  }

  @override
  String get editorSelectGroup => 'Select this group';

  @override
  String get editorBulkField => 'Field to change';

  @override
  String get editorBulkValue => 'New value';

  @override
  String get editorBulkChoose => 'Choose a value';

  @override
  String get editorBulkApply => 'Apply to selected';

  @override
  String get editorBoolTrue => 'Yes';

  @override
  String get editorBoolFalse => 'No';

  @override
  String get operationCancel => 'Stop after current item';

  @override
  String get operationCancelling => 'Stopping…';

  @override
  String get warnOperationCancelled =>
      'The operation was stopped safely; completed items were kept in history and remaining items were not started';

  @override
  String get opRfiLogAddTitle => 'Add multi-sheet RFI Log entries';

  @override
  String get opRfiFollowUpSyncTitle => 'Synchronize RFI Log to Follow Up';

  @override
  String get opRfiFollowUpSyncDesc =>
      'Safely synchronizes CIV/MEC lifecycle rows and FINAL Worley Signed/Not Signed rows from the RFI Log, including prepared missing entries, removals, and grouped statuses.';

  @override
  String get paramRfiFollowUpRfiLogWorkbook => 'Source RFI Log workbook';

  @override
  String get paramRfiFollowUpRfiLogWorkbookHelp =>
      'Select the .xlsm or .xlsx RFI Log workbook whose statuses will be read.';

  @override
  String get paramRfiFollowUpFollowUpWorkbook => 'Target Follow Up workbook';

  @override
  String get paramRfiFollowUpFollowUpWorkbookHelp =>
      'Select the .xlsm or .xlsx workbook where only previewed RFI and status cells may change.';

  @override
  String get errorParamRequiredRfiLogWorkbook => 'RFI Log workbook is required';

  @override
  String get errorParamRequiredFollowUpWorkbook =>
      'Follow Up workbook is required';

  @override
  String get errorRfiFollowUpWorkbookInvalid =>
      'Select valid RFI Log and Follow Up .xlsm or .xlsx workbooks.';

  @override
  String get errorRfiFollowUpWorkbookLocked =>
      'Close the selected workbook in Excel or another application, then try again.';

  @override
  String get errorRfiFollowUpWorkbookChanged =>
      'A selected workbook changed after preview. Create a new preview before applying.';

  @override
  String get errorRfiFollowUpLayoutAmbiguous =>
      'More than one safe Follow Up table was found. Keep one exact RFI NO./STATUS table.';

  @override
  String get errorRfiFollowUpLayoutMissing =>
      'No safe Follow Up table with exact RFI NO. and STATUS headers was found.';

  @override
  String get errorRfiFollowUpPatchVerification =>
      'DocFlow could not verify that only approved status cells changed. No output was accepted.';

  @override
  String get errorRfiFollowUpProtected =>
      'The workbook, target worksheet, or required cells are protected or unsafe to edit.';

  @override
  String get errorRfiFollowUpSignedPackage =>
      'The workbook contains a digital signature and cannot be changed safely by automation.';

  @override
  String get errorRfiFollowUpLimitExceeded =>
      'The workbook exceeds the safe processing limits for this operation.';

  @override
  String get errorRfiFollowUpNoRowsSelected =>
      'Select at least one safe Follow Up row before applying.';

  @override
  String get errorRfiFollowUpRetainedStorageFull =>
      'The bounded recovery storage is full. Review or remove older retained DocFlow evidence before retrying.';

  @override
  String get errorRfiFollowUpBackupExists =>
      'A backup with the reserved name already exists. No workbook was overwritten.';

  @override
  String get errorRfiFollowUpBackupVerify =>
      'DocFlow could not verify the workbook backup. The operation was stopped for recovery.';

  @override
  String get errorRfiFollowUpRecoveryRequired =>
      'The operation requires recovery; do not apply it again.';

  @override
  String get rfiFollowUpEditorTitle => 'Review RFI Follow Up status candidates';

  @override
  String rfiFollowUpEditorRowCount(Object count) {
    return '$count Follow Up rows';
  }

  @override
  String get rfiFollowUpColumnEnabled => 'Apply';

  @override
  String get rfiFollowUpColumnRfiNo => 'RFI number';

  @override
  String get rfiFollowUpColumnSourceStatuses => 'RFI Log source value';

  @override
  String get rfiFollowUpColumnCurrentStatus => 'Current Follow Up status';

  @override
  String get rfiFollowUpColumnProposedRfiNo => 'Proposed RFI group';

  @override
  String get rfiFollowUpColumnProposedStatus => 'Proposed status';

  @override
  String get rfiFollowUpColumnDisposition => 'Decision';

  @override
  String get rfiFollowUpColumnReason => 'Why';

  @override
  String get rfiFollowUpColumnSheet => 'Worksheet';

  @override
  String get rfiFollowUpColumnRow => 'Row';

  @override
  String get rfiFollowUpColumnMappingDetails => 'Mapping details';

  @override
  String get rfiFollowUpDispositionReady => 'Ready to update';

  @override
  String get rfiFollowUpDispositionAlreadySynced => 'Already synchronized';

  @override
  String get rfiFollowUpDispositionManualReview => 'Manual review only';

  @override
  String get warnRfiFollowUpManualRows =>
      'rows require manual review and cannot be selected automatically.';

  @override
  String get warnRfiFollowUpNoRowsSelected =>
      'No safe rows can be applied. Disabled checkboxes are manual-only; review the Why column.';

  @override
  String get warnRfiFollowUpUnmappedRecords =>
      'RFI Log records with unsupported routes, missing prepared rows, or ambiguous matches were left unchanged. Review Why and Mapping details for the exact ITP/item and source/target context.';

  @override
  String get warnRfiFollowUpReasonTargetRfiFormula =>
      'The target RFI number is a formula.';

  @override
  String get warnRfiFollowUpReasonTargetRfiMerged =>
      'The target RFI number is inside a merged cell.';

  @override
  String get warnRfiFollowUpReasonBlankRfi => 'The target RFI number is blank.';

  @override
  String get warnRfiFollowUpReasonInvalidRfi =>
      'The target RFI number must use the exact short format, for example CIV-0170.';

  @override
  String get warnRfiFollowUpReasonTargetDuplicate =>
      'The same full RFI number appears more than once in the target table.';

  @override
  String get warnRfiFollowUpReasonSourceMissing =>
      'No exact full-value RFI match exists in the RFI Log.';

  @override
  String get warnRfiFollowUpReasonSourceFormula =>
      'The matching source RFI or status is a formula.';

  @override
  String get warnRfiFollowUpReasonSourceMerged =>
      'The matching source RFI or status is in a merged cell.';

  @override
  String get warnRfiFollowUpReasonSourceStatusBlank =>
      'The matching RFI Log status is blank.';

  @override
  String get warnRfiFollowUpReasonSourceStatusUnknown =>
      'The matching RFI Log status is not exactly Completed or Ongoing.';

  @override
  String get warnRfiFollowUpReasonSourceConflict =>
      'The RFI Log contains conflicting statuses for this exact RFI number.';

  @override
  String get warnRfiFollowUpReasonTargetCellMissing =>
      'The target status cell does not exist.';

  @override
  String get warnRfiFollowUpReasonTargetFormula =>
      'The target status cell is a formula.';

  @override
  String get warnRfiFollowUpReasonTargetMerged =>
      'The target status cell is merged.';

  @override
  String get warnRfiFollowUpReasonTargetStatusUnknown =>
      'The current target status is not blank, Signed, or Not Signed, so it will not be overwritten.';

  @override
  String get warnRfiFollowUpReasonAlreadySynced =>
      'The target already has the proposed status.';

  @override
  String get warnRfiFollowUpReasonReadySigned =>
      'The exact RFI Log remark Signed by Worley can be applied as Signed.';

  @override
  String get warnRfiFollowUpReasonReadyLogSync =>
      'The RFI group and shared status can be synchronized from the approved RFI Log mapping.';

  @override
  String get warnRfiFollowUpReasonReadyWorleySync =>
      'The FINAL Worley status can be synchronized from the exact Signed by Worley remark line.';

  @override
  String get warnRfiFollowUpReasonReadyRemove =>
      'Cancelled or Rejected RFI entries will be removed and the remaining shared status recalculated.';

  @override
  String get warnRfiFollowUpReasonMappingMissing =>
      'A supported Follow Up route exists, but no prepared target row matches this RFI\'s exact ITP, item, and context. Nothing was changed.';

  @override
  String get warnRfiFollowUpReasonPartialContext =>
      'Some source contexts match prepared Follow Up rows and others do not. Check the unmatched Tag No, Area, and Inspection Location in the RFI Log. DocFlow will not edit the source log.';

  @override
  String get warnRfiFollowUpReasonMappingUnsupported =>
      'This ITP/item combination has no Follow Up route in the current template. It cannot be synchronized automatically.';

  @override
  String get warnRfiFollowUpReasonMappingAmbiguous =>
      'More than one target row matches this record; it was left unchanged instead of guessing.';

  @override
  String get warnRfiFollowUpReasonReadyNotSigned =>
      'No exact Signed by Worley remark was found; Not Signed can be applied.';

  @override
  String get opRfiLogAddDesc =>
      'Creates CIV, MEC, ELE and PRC RFI numbers together, schedules them globally, fills lookup fields and prepares separate Excel-native mail tables.';

  @override
  String get paramRfiWorkbook => 'RFI Log workbook';

  @override
  String get paramRfiWorkbookHelp =>
      'Select a macro-enabled .xlsm or standard .xlsx RFI Log workbook to read and update safely.';

  @override
  String get dialogFilterExcelWorkbook => 'Excel (.xlsm, .xlsx)';

  @override
  String get errorRfiWorkbookInvalid =>
      'Select a valid macro-enabled .xlsm or standard .xlsx RFI Log workbook.';

  @override
  String get paramRfiEmailText => 'Paste the request email';

  @override
  String get paramRfiEmailTextHelp =>
      'Paste the email body. It is used only for this preview and is never saved to settings or the workbook.';

  @override
  String get paramRfiEmailProvider => 'Email extraction method';

  @override
  String get paramRfiEmailProviderHelp =>
      'Offline is private and free. Gemini Free is optional and sends the pasted email to Google.';

  @override
  String get choiceRfiEmailProviderOffline => 'Offline — private (recommended)';

  @override
  String get choiceRfiEmailProviderGemini => 'Gemini Free — external AI';

  @override
  String get paramRfiGeminiApiKey => 'Gemini API key (only for Gemini)';

  @override
  String get paramRfiGeminiApiKeyHelp =>
      'Not saved. You may instead define GEMINI_API_KEY in Windows. Do not use Gemini Free for confidential company emails.';

  @override
  String get errorParamRequiredEmailProvider =>
      'Choose an email extraction method';

  @override
  String get warnRfiEmailExtracted =>
      'Email fields were extracted into the editable preview';

  @override
  String get warnRfiEmailNeedsReview =>
      'Some email details are ambiguous or incomplete; review the rows';

  @override
  String get warnRfiAiFallback =>
      'Gemini was unavailable; private offline extraction was used';

  @override
  String get warnRfiGeminiPrivacy =>
      'Gemini Free sends this email to Google. Free-tier content may be used for product improvement; do not send confidential information.';

  @override
  String get paramRfiDiscipline => 'Discipline';

  @override
  String get paramRfiCount => 'Number of RFI numbers';

  @override
  String get paramRfiCountHelp =>
      'Counts unique RFI numbers; duplicate rows do not consume another number or time slot.';

  @override
  String get paramRfiSheetCounts => 'Sheets and RFI counts';

  @override
  String get paramRfiSheetCountsHelp =>
      'Select one or more sheets and set the unique RFI number count for each. All selected sheets are edited and applied together.';

  @override
  String rfiSheetCountsTotal(Object count) {
    return '$count RFI numbers in this operation';
  }

  @override
  String get paramRfiOverflowNextDay =>
      'Move overflow to the next calendar day';

  @override
  String get paramRfiOverflowNextDayHelp =>
      'After 17:30, continue at 09:00 the next day. Turn off to keep later times on the same day.';

  @override
  String get errorParamRequiredWorkbook => 'RFI Log workbook is required';

  @override
  String get errorParamRequiredDiscipline => 'Discipline is required';

  @override
  String get errorParamRequiredCount => 'RFI count is required';

  @override
  String get errorParamRequiredSheetCounts =>
      'Select at least one sheet and RFI number';

  @override
  String get noteRfiWorkbookUpdate => 'RFI Log rows will be added';

  @override
  String get noteRfiRestoreBackup => 'Restore the workbook backup';

  @override
  String get warnRfiOverflowMoved =>
      'RFI time slots exceeded 17:30 and were moved to the next calendar day';

  @override
  String get warnRfiOverflowKept =>
      'RFI time slots exceed 17:30 and will remain on the same day';

  @override
  String get warnRfiGroupScheduleMismatch =>
      'Rows sharing an RFI number have different inspection dates or times';

  @override
  String get rfiEditorTitle => 'Review and edit RFI entries';

  @override
  String rfiEditorRowCount(Object count) {
    return '$count Excel rows';
  }

  @override
  String get rfiEditorScrollHint => 'Scroll horizontally to edit every field';

  @override
  String get rfiEditorCompactHint =>
      'Hover for full numbers; automatic fields are under Details';

  @override
  String get rfiEditorReviewWarnings =>
      'The edited rows were validated. Review the new warnings, then apply again.';

  @override
  String rfiEditorCopied(Object count) {
    return 'Excel opened; $count rows were filtered and copied with native Excel formatting';
  }

  @override
  String get rfiActionDuplicate => 'Add another row with the same RFI number';

  @override
  String get rfiActionRemoveDuplicate => 'Remove this duplicate row';

  @override
  String get rfiActionCopyMail => 'Filter and copy exactly in Excel';

  @override
  String get rfiActionPrepareReply => 'Prepare Outlook reply';

  @override
  String get rfiActionCopySheet => 'Copy from Excel';

  @override
  String get rfiMailPreviewTitle => 'Excel table for Outlook';

  @override
  String get rfiMailPreviewSourceWorkbook => 'Current data from Excel';

  @override
  String get rfiMailPreviewSourcePlan => 'Approved plan data';

  @override
  String get rfiMailPreviewHint =>
      'This preview shows the exact columns and values that will be copied. Paste normally into Outlook; Excel formatting and cell borders are preserved.';

  @override
  String get rfiMailPreviewCopy => 'Copy formatted table';

  @override
  String get rfiActionDetails => 'Automatic fields and details';

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
  String get rfiColIssueDate => 'Issued';

  @override
  String get rfiColInspectionDate => 'Inspection';

  @override
  String get rfiColStartTime => 'Time';

  @override
  String get rfiColAzfenType => 'Insp. Type AZFEN';

  @override
  String get rfiColIntecType => 'Insp. Type INTEC';

  @override
  String get rfiColLocation => 'Location';

  @override
  String get rfiColSubcontractor => 'Subcontractor';

  @override
  String get rfiDateDay => 'Day';

  @override
  String get rfiDateMonth => 'Month';

  @override
  String get rfiDateChangeYear => 'Change year';

  @override
  String get rfiDateYear => 'Year';

  @override
  String get rfiColActions => 'Rows';

  @override
  String get actionClose => 'Close';

  @override
  String get paramOrganizeIntoRfiFolders =>
      'Create a separate folder for each RFI';

  @override
  String get paramOrganizeIntoRfiFoldersHelp =>
      'Turn off to save renamed PDFs directly in the destination; unreadable or duplicate files still go to _review for safety';

  @override
  String get errorRfiWorkbookLocked =>
      'The RFI Log is open or read-only. Save and close it in Excel, then check that the file is writable.';

  @override
  String get opRfiStageSignedFirstPagesTitle => 'Stage signed RFI first pages';

  @override
  String get opRfiNormalizePdfOrientationTitle => 'Rotate RFI PDF pages';

  @override
  String get opRfiNormalizePdfOrientationDesc =>
      'Standalone page rotation and orientation correction for PDFs under CIV, MEC, ELE and PRC, separate from Fast RFI Scan. Only confidently determined corrections are written to a separate review copy; if any page is uncertain, the PDF remains unchanged for manual review.';

  @override
  String get paramRfiOrientationRfiSystemRoot => 'RFI system root';

  @override
  String get paramRfiOrientationRfiSystemRootHelp =>
      'Read-only source containing the CIV, MEC, ELE and PRC folders. Source files are never changed.';

  @override
  String get paramRfiOrientationReviewRoot => 'Orientation review folder';

  @override
  String get paramRfiOrientationReviewRootHelp =>
      'Separate folder where corrected copies are created with the original directory layout.';

  @override
  String get paramScanIndexForceRescan => 'Force a full rescan this time';

  @override
  String get paramScanIndexForceRescanHelp =>
      'Ignore cached analysis for this run. This switch resets automatically after leaving the task.';

  @override
  String get errorRfiOrientationReviewRootMustBeSeparate =>
      'The review folder must not overlap the RFI system';

  @override
  String get warnRfiOrientationNoPdfs =>
      'No PDFs were found under CIV, MEC, ELE or PRC';

  @override
  String get warnRfiOrientationUnsupported =>
      'The PDF could not be analysed safely';

  @override
  String get warnRfiOrientationNeedsReview =>
      'At least one page orientation was not confident enough; the file was not applied';

  @override
  String get warnRfiOrientationReviewExists =>
      'The review destination already exists and was not overwritten';

  @override
  String get noteRfiPdfOrientationNormalized =>
      'Correct approved PDF page orientations in a separate review copy';

  @override
  String get rfiOrientationPages => 'Page rotations';

  @override
  String get opRfiStageSignedFirstPagesDesc =>
      'Identifies signed scans, searches every nested RFI System folder with bounded multi-core discovery, and replaces only page one in separate review copies without touching the archive.';

  @override
  String get paramRfiSignedScans => 'New signed scans folder';

  @override
  String get paramRfiSignedScansHelp =>
      'New signed RFI PDFs. Every page of a combined scan is identified independently.';

  @override
  String get paramRfiArchiveRoot => 'Existing RFI archive root';

  @override
  String get paramRfiArchiveRootHelp =>
      'Read-only RFI System root. Every nested folder is searched with bounded multi-core discovery; only exact RFI PDF filenames can match.';

  @override
  String get paramRfiReviewRoot => 'Review output folder';

  @override
  String get paramRfiReviewRootHelp =>
      'A separate, preferably empty folder where copies are written using the archive\'s relative folder layout.';

  @override
  String get paramRfiScanSubfolders => 'Also scan signed-scan subfolders';

  @override
  String get paramRfiScanSubfoldersHelp =>
      'Enabled by default. Identifies PDFs at every depth below the new scans folder; disable only for a deliberate top-level-only run.';

  @override
  String get errorParamRequiredSignedScans =>
      'New signed scans folder is required';

  @override
  String get errorParamRequiredArchiveRoot =>
      'Existing RFI archive root is required';

  @override
  String get errorParamRequiredReviewRoot => 'Review output folder is required';

  @override
  String get errorRfiReviewRootMustBeSeparate =>
      'The review output folder must be completely separate from both the archive and the new scans folder. No files were touched.';

  @override
  String get noteRfiSignedFirstPageStaged =>
      'Create a review copy with the signed first page';

  @override
  String get noteRfiCleanupStaleTemp =>
      'Remove the stale temporary link for the completed review PDF';

  @override
  String get warnRfiNoSignedPdfs =>
      'No PDFs were found in the new scans folder';

  @override
  String get warnRfiSignedUnreadable =>
      'The PDF could not be read; no output was prepared for this RFI';

  @override
  String get warnRfiSignedDuplicate =>
      'Multiple new scans have the same RFI number; none was selected automatically';

  @override
  String get warnRfiArchiveUnreadable =>
      'The entire archive could not be scanned safely; no output was prepared to avoid a missing or false match';

  @override
  String get warnRfiArchiveMissing =>
      'No archive file was found for this RFI number';

  @override
  String get warnRfiArchiveAmbiguous =>
      'Multiple archive files were found for this RFI number; choose one PDF from the archive-path field';

  @override
  String get warnRfiReviewExists =>
      'A file already exists at the review destination; it was not overwritten';

  @override
  String get warnRfiStaleTempFound =>
      'The review PDF is already complete; a temporary file left by the earlier Windows lock will be removed safely';

  @override
  String get warnRfiReviewPathBlocked =>
      'The review path is blocked by a file or redirected folder; no output was prepared';

  @override
  String get warnRfiSignedExtraPages =>
      'The new PDF has multiple pages; only its signed first page will be used';

  @override
  String get warnRfiSignedMultiPage =>
      'Multi-page signed scan detected; every page was analyzed independently';

  @override
  String get warnRfiFirstPageLayoutDiffers =>
      'The new first page has a different size or orientation; check the preview before applying';

  @override
  String get warnRfiPdfFidelityUnsupported =>
      'This PDF cannot be staged without risking document fidelity; review the reason and handle it manually';

  @override
  String get rfiSignedEditorTitle => 'Signed first-page review';

  @override
  String rfiSignedEditorRowCount(Object count) {
    return '$count matched RFI PDFs';
  }

  @override
  String get rfiSignedColumnEnabled => 'Stage';

  @override
  String get rfiSignedColumnCode => 'RFI number';

  @override
  String get rfiSignedColumnScan => 'Signed scan';

  @override
  String get rfiSignedColumnArchive => 'Archive path';

  @override
  String get rfiSignedArchiveChoose => 'Choose an archive PDF';

  @override
  String get rfiSignedPreviewTitle => 'First-page comparison';

  @override
  String get rfiSignedPreviewHelp =>
      'Compare the archived first page with the exact signed page selected by the approved plan.';

  @override
  String get rfiSignedPreviewRfi => 'RFI to preview';

  @override
  String get rfiSignedPreviewArchive => 'Current archive page';

  @override
  String get rfiSignedPreviewSigned => 'New signed page';

  @override
  String rfiSignedPreviewArchiveAlt(Object code) {
    return 'Current archived first page for $code';
  }

  @override
  String rfiSignedPreviewSignedAlt(Object code) {
    return 'New signed page for $code';
  }

  @override
  String get rfiSignedPreviewChooseArchive =>
      'Choose an archive PDF to display its first page.';

  @override
  String get rfiSignedPreviewUnavailable =>
      'A bounded page preview could not be prepared; inspect the PDF manually before applying.';

  @override
  String rfiSignedPreviewOmitted(Object count) {
    return 'Preview unavailable for $count additional RFI rows because the safe preview limit was reached.';
  }

  @override
  String get rfiSignedPreviewPrevious => 'Previous RFI';

  @override
  String get rfiSignedPreviewNext => 'Next RFI';

  @override
  String get rfiSignedPreviewOnlyPageOne =>
      'Only page 1 will be replaced; the remaining archived pages stay unchanged.';

  @override
  String get rfiSignedPreviewBefore => 'BEFORE';

  @override
  String get rfiSignedPreviewAfter => 'AFTER';

  @override
  String get rfiSignedPreviewEnlarge => 'Enlarge';

  @override
  String get rfiSignedNavLabel => 'Signed-page preview sections';

  @override
  String get rfiSignedNavApproved => 'Approved RFIs';

  @override
  String get rfiSignedNavCompare => 'Compare pages';

  @override
  String get rfiSignedNavReview => 'Review copies';

  @override
  String get rfiSignedWarningReviewTitle => 'Warning PDFs sent to Review';

  @override
  String get rfiSignedWarningReviewHelp =>
      'Apply copies each affected source PDF without changing it, preserving its subfolder path under _review.';

  @override
  String get rfiSignedWarningReviewPlanned => 'Will copy';

  @override
  String get rfiSignedWarningReviewExists => 'Already exists';

  @override
  String get rfiSignedWarningReviewBlocked => 'Blocked';

  @override
  String get rfiSignedWarningReviewDestination => 'Review destination';

  @override
  String rfiSignedWarningReviewPage(Object page) {
    return 'Page $page';
  }

  @override
  String get rfiSignedWarningReviewFile => 'Whole file';

  @override
  String get noteRfiSignedWarningReviewCopy => 'Copy warning PDF to Review';

  @override
  String get warnRfiWarningReviewExists =>
      'The warning PDF already exists in Review and will not be overwritten';

  @override
  String get warnRfiWarningReviewBlocked =>
      'The warning PDF cannot be copied to Review because its destination path is blocked';

  @override
  String get progressStages => 'Processing stages';

  @override
  String get progressPhasePreparing => 'Preparing preview';

  @override
  String get progressPhasePreview => 'Building preview';

  @override
  String get progressPhaseApply => 'Applying approved plan';

  @override
  String get progressPhasePrintWaiting => 'Waiting for printer';

  @override
  String get progressPhaseScan => 'Scanning files';

  @override
  String get progressPhaseScanDiscovery => 'Finding PDFs to scan';

  @override
  String get progressPhaseFileGroupDiscovery => 'Finding files to group';

  @override
  String get progressPhaseFileGroupAnalysis => 'Reading RFI numbers';

  @override
  String get progressPhaseFileGroupPlanning => 'Preparing the RFI folder plan';

  @override
  String get progressPhaseScanOcr => 'Reading PDFs with OCR';

  @override
  String get progressPhaseScanPlanning => 'Preparing the file plan';

  @override
  String get progressPhaseWorkbookReading => 'Reading the workbook';

  @override
  String get progressPhaseRfiRequestAnalysis => 'Analysing the RFI request';

  @override
  String get progressPhaseRfiPlanPreparation => 'Preparing RFI rows';

  @override
  String get progressPhaseFollowUpAnalysis => 'Calculating Follow Up mappings';

  @override
  String get progressPhaseSystemDiscovery => 'Finding RFI system records';

  @override
  String get progressPhaseSystemSignatureAnalysis =>
      'Analysing Worley signatures';

  @override
  String get progressPhaseSystemWorkbookRead => 'Reading RFI workbooks';

  @override
  String get progressPhaseSystemMapping =>
      'Matching signatures to workbook records';

  @override
  String get progressPhaseOrientationDiscovery =>
      'Finding PDFs for orientation checks';

  @override
  String get progressPhaseOrientationInspection =>
      'Checking PDF structure and scan cache';

  @override
  String get progressPhaseOrientationAnalysis =>
      'Analysing PDF page orientation';

  @override
  String get progressPhasePrinterCheck => 'Checking printer readiness';

  @override
  String get progressPhasePrintFileInspection =>
      'Inspecting print files and settings';

  @override
  String get progressPhaseOutlookSelection =>
      'Validating the Outlook selection';

  @override
  String get progressPhaseSignedDiscovery => 'Finding signed PDFs';

  @override
  String get progressPhaseSignedInspection => 'Inspecting PDF pages';

  @override
  String get progressPhaseSignedOcr => 'Reading RFI numbers';

  @override
  String get progressPhaseSignedClassification => 'Matching signed pages';

  @override
  String get progressPhaseArchiveScan => 'Searching the RFI archive';

  @override
  String get progressPhaseComparisonPreparation => 'Preparing page comparisons';

  @override
  String get progressPhasePreviewReady => 'Preview ready';

  @override
  String get recoveryTitle => 'Recovery center';

  @override
  String get homeRecoveryDesc =>
      'Safely review pending journals and uncertain print jobs.';

  @override
  String get recoveryHelp =>
      'Only local journals and saved print queues authorize recovery.';

  @override
  String get recoveryNoEffect =>
      'No change occurred — previewing again is safe';

  @override
  String get recoveryOutcomeUnknown => 'Outcome unknown — no automatic retry';

  @override
  String get recoveryManual => 'Manual review required';

  @override
  String get recoveryRestoreOriginal => 'Restore original Follow Up file';

  @override
  String get recoveryPrintQueue => 'Uncertain print queue';

  @override
  String get recoveryEmpty => 'Nothing needs recovery';

  @override
  String get recoveryEmptyHelp =>
      'No pending journal or unfinished print queue was found.';

  @override
  String get effectCommitted => 'Completed';

  @override
  String get effectNoEffectRetryable => 'No change occurred — retry is safe';

  @override
  String get effectPartialCommitted => 'Partially completed — review recovery';

  @override
  String get effectOutcomeUnknown => 'Outcome unknown — no automatic retry';

  @override
  String get effectCancelledClean => 'Cancelled cleanly — no change occurred';

  @override
  String get effectCancelledPartial => 'Partially cancelled — review recovery';

  @override
  String get presetTitle => 'Presets';

  @override
  String get presetSelect => 'Saved preset';

  @override
  String get presetSelectPlaceholder => 'Choose a preset';

  @override
  String get presetName => 'New preset name';

  @override
  String get presetApply => 'Apply preset';

  @override
  String get presetSave => 'Save preset';

  @override
  String get presetReplace => 'Replace preset';

  @override
  String get presetDelete => 'Delete preset';

  @override
  String presetConfirmReplace(Object name) {
    return 'A preset named $name already exists. Replace it?';
  }

  @override
  String presetConfirmDelete(Object name) {
    return 'Delete the preset named $name?';
  }

  @override
  String get presetEmpty => 'No presets have been saved yet.';

  @override
  String presetSaved(Object name) {
    return 'Preset $name was saved.';
  }

  @override
  String presetDeleted(Object name) {
    return 'Preset $name was deleted.';
  }

  @override
  String get errorPresetNameInvalid =>
      'The preset name must contain 1–64 characters.';

  @override
  String get errorPresetDuplicate => 'A preset with this name already exists.';

  @override
  String get errorPresetLimit =>
      'You can save at most 20 presets for this operation.';

  @override
  String get errorPresetStale =>
      'This preset no longer exists. Refresh the list and try again.';

  @override
  String get errorPresetWrite =>
      'The preset settings could not be saved. Try again.';

  @override
  String get opRfiOutlookReplyDraftTitle => 'Prepare Outlook Reply All draft';

  @override
  String get opRfiOutlookReplyDraftDesc =>
      'Uses only the one currently selected Outlook email, prepares a Reply All response, and displays it as an UNSENT draft for review.';

  @override
  String get paramRfiOutlookReplyDraftReplyText => 'Reply text';

  @override
  String get paramRfiOutlookReplyDraftReplyTextHelp =>
      'Enter the text to prepend. The preview escapes HTML, and apply never sends the message or adds recipients.';

  @override
  String get errorParamRequiredReplyText => 'Reply text is required';

  @override
  String get stepRfiOutlookReplyDraft => 'Display an unsent Reply All draft';

  @override
  String get outlookDraftPreviewTitle => 'Unsent Outlook reply preview';

  @override
  String get outlookDraftSubject => 'Selected message subject';

  @override
  String get outlookDraftPlainText => 'Plain text';

  @override
  String get outlookDraftHtml => 'Escaped HTML';

  @override
  String get warnRfiOutlookDraftUnsent =>
      'Apply will display an UNSENT Reply All draft; DocFlow never sends it.';

  @override
  String get warnRfiOutlookDraftUndoConditional =>
      'Undo is available only if Outlook auto-saves a durable draft and that exact draft remains unchanged.';

  @override
  String get warnRfiOutlookDraftUndoUnavailable =>
      'Outlook did not expose a durable auto-saved draft, so undo is unavailable.';

  @override
  String get errorRfiOutlookDraftUnsupportedPlatform =>
      'Outlook draft automation is available only on Windows.';

  @override
  String get errorRfiOutlookDraftReplyRequired =>
      'Enter reply text before previewing.';

  @override
  String get errorRfiOutlookDraftRfiLogNotCommitted =>
      'The Outlook reply can only be prepared from the same completed RFI Log operation.';

  @override
  String get errorRfiOutlookDraftReplyTooLong =>
      'Reply text exceeds the 10,000-character limit.';

  @override
  String get errorRfiOutlookDraftReplyInvalid =>
      'Reply text contains an unsupported control character.';

  @override
  String get errorRfiOutlookDraftSelectionRequired =>
      'Select exactly one email in the active Outlook Explorer window.';

  @override
  String get errorRfiOutlookDraftSingleSelectionRequired =>
      'Select exactly one Outlook email.';

  @override
  String get errorRfiOutlookDraftMailItemRequired =>
      'The current Outlook selection is not an email message.';

  @override
  String get errorRfiOutlookDraftSelectionInvalid =>
      'The selected Outlook email has no stable identity.';

  @override
  String get errorRfiOutlookDraftSubjectTooLong =>
      'The selected Outlook subject exceeds the preview limit.';

  @override
  String get errorRfiOutlookDraftSelectionChanged =>
      'The selected Outlook email changed after preview; preview again.';

  @override
  String get errorRfiOutlookDraftOutlookUnavailable =>
      'Microsoft Outlook desktop automation is unavailable.';

  @override
  String get errorRfiOutlookDraftOutlookNotRunning =>
      'Open Outlook desktop and select one email before previewing.';

  @override
  String get errorRfiOutlookDraftComUnavailable =>
      'The Windows Outlook automation apartment is unavailable.';

  @override
  String get errorRfiOutlookDraftComFailed =>
      'Outlook could not provide the required bounded message property.';

  @override
  String get errorRfiOutlookDraftReplyAllFailed =>
      'Outlook could not confirm creation of the Reply All draft; review Outlook manually.';

  @override
  String get errorRfiOutlookDraftBodyPrependFailed =>
      'Outlook could not confirm the reply body update; review Outlook manually.';

  @override
  String get errorRfiOutlookDraftBodyFormatUnsupported =>
      'The Outlook reply uses an unsupported rich-text body format; review Outlook manually.';

  @override
  String get errorRfiOutlookDraftContentTooLarge =>
      'The Outlook draft body exceeds the safe automation limit.';

  @override
  String get errorRfiOutlookDraftDisplayUnknown =>
      'Outlook could not confirm that the unsent draft was displayed; review Outlook manually.';

  @override
  String get errorRfiOutlookDraftDraftMissingOrChanged =>
      'Undo refused because the exact auto-saved draft is missing or has changed.';

  @override
  String get errorRfiOutlookDraftDeleteUnknown =>
      'Outlook could not confirm deletion of the unchanged draft; review Outlook manually.';

  @override
  String get errorRfiOutlookDraftUndoUnavailable =>
      'Undo is unavailable because Outlook did not auto-save a durable draft.';

  @override
  String get opRfiOutlookArchiveSelectedTitle =>
      'Archive selected Outlook email';

  @override
  String get opRfiOutlookArchiveSelectedDesc =>
      'Saves exactly one selected Outlook email as a local Unicode MSG copy without sending, moving, or deleting mail.';

  @override
  String get paramRfiOutlookArchiveDestination => 'Mail archive folder';

  @override
  String get paramRfiOutlookArchiveDestinationHelp =>
      'Choose a separate staging folder. Group Files by RFI can later copy this MSG into every exact related RFI folder.';

  @override
  String get stepRfiOutlookArchive => 'Create a guarded local MSG copy';

  @override
  String get warnRfiOutlookArchiveLocalCopy =>
      'Apply creates one local MSG copy; the selected Outlook email and mailbox remain unchanged.';

  @override
  String get errorRfiOutlookArchiveDestinationExists =>
      'The planned MSG filename already exists in the archive folder; rename or move it, then preview again.';

  @override
  String get errorRfiOutlookArchivePlanUnavailable =>
      'The approved Outlook archive plan is no longer available; preview again.';

  @override
  String get errorRfiOutlookArchiveSelectionChanged =>
      'The selected Outlook email changed after preview; select the original email and preview again.';

  @override
  String get errorRfiOutlookArchiveExportFailed =>
      'Outlook could not create the private MSG export; no archive file was published.';

  @override
  String get errorRfiOutlookArchiveInvalidMsg =>
      'Outlook produced an empty, oversized, or invalid MSG export; no archive file was published.';

  @override
  String get errorRfiOutlookArchiveWriteFailed =>
      'The MSG archive file could not be created; no existing file was overwritten.';

  @override
  String get errorRfiOutlookArchiveWriteUnknown =>
      'The MSG archive write could not be verified; review the destination before retrying.';

  @override
  String get opRfiSystemSyncTitle =>
      'Analyze RFI System & Sync Worley Signatures';

  @override
  String get opRfiSystemSyncDesc =>
      'Scans RFI system folders across disciplines, inspects page 1 for Worley signatures in the Others section, and synchronizes RFI Log and Follow Up workbooks.';

  @override
  String get paramRfiSystemSyncRfiSystemRoot => 'RFI System Root Folder';

  @override
  String get paramRfiSystemSyncRfiSystemRootHelp =>
      'Select the root folder containing CIV, MEC, ELE, and PRC discipline directories.';

  @override
  String get paramRfiSystemSyncRfiLogWorkbook => 'RFI Log Workbook';

  @override
  String get paramRfiSystemSyncRfiLogWorkbookHelp =>
      'Select the RFI Log (.xlsm or .xlsx) workbook to update Remarks to \'Signed by Worley\'.';

  @override
  String get paramRfiSystemSyncFollowUpWorkbook => 'Follow Up Workbook';

  @override
  String get paramRfiSystemSyncFollowUpWorkbookHelp =>
      'Select the current Follow Up workbook containing the Signed By Worley CIV and Signed By Worley MEC control tables.';

  @override
  String get errorParamRequiredRfiSystemRoot =>
      'RFI System root folder is required';

  @override
  String get errorRfiSystemSyncWorkbookInvalid =>
      'Select valid RFI Log and Follow Up .xlsm or .xlsx workbooks.';

  @override
  String get errorRfiSystemSyncWorkbookLocked =>
      'Close the selected workbook in Excel or another application, then try again.';

  @override
  String get errorRfiSystemSyncWorkbookLayoutInvalid =>
      'The RFI Log must contain one exact RFI No. and REMARKS header in each CIV, MEC, ELE, and PRC sheet.';

  @override
  String get errorRfiSystemSyncFollowUpLayoutInvalid =>
      'The Follow Up workbook must contain one Signed By Worley CIV table and one Signed By Worley MEC control table.';

  @override
  String get errorRfiSystemSyncApprovedUpdateInvalid =>
      'The approved workbook update is invalid; preview again.';

  @override
  String get errorRfiSystemSyncWorkbookChanged =>
      'A selected workbook changed after preview; no update was applied. Preview again.';

  @override
  String get warnRfiSystemSyncNoChangesNeeded =>
      'All detected RFIs are already synchronized or unsigned.';

  @override
  String get warnRfiSystemSyncNoRowsSelected =>
      'Select at least one safe signed RFI to synchronize.';

  @override
  String get noteRfiSystemSyncRfiLog =>
      'Synchronize Worley signatures into RFI Log remarks';

  @override
  String get noteRfiSystemSyncFollowUp =>
      'Synchronize Final ITP signed RFIs into Follow Up';

  @override
  String get noteRfiSystemSyncBackup => 'Create backup before synchronization';

  @override
  String get noteRfiSystemSyncRestore =>
      'Restore workbook from pre-sync backup';

  @override
  String get rfiSystemSyncEditorTitle =>
      'Review RFI System & Worley Signature Synchronization';

  @override
  String rfiSystemSyncEditorRowCount(Object count) {
    return '$count scanned RFIs';
  }

  @override
  String get rfiSystemSyncColumnEnabled => 'Apply';

  @override
  String get rfiSystemSyncColumnDiscipline => 'Discipline';

  @override
  String get rfiSystemSyncColumnRfiNo => 'RFI No.';

  @override
  String get rfiSystemSyncColumnWorleyStatus => 'Worley Signature';

  @override
  String get rfiSystemSyncColumnRfiLogAction => 'RFI Log Action';

  @override
  String get rfiSystemSyncColumnFollowUpAction => 'Follow Up Action';

  @override
  String get rfiSystemSyncColumnDisposition => 'Status';

  @override
  String get rfiSystemSyncColumnReason => 'Evidence / Details';

  @override
  String get rfiSystemSyncDispositionReady => 'Ready to sync';

  @override
  String get rfiSystemSyncDispositionAlreadySynced => 'Already synchronized';

  @override
  String get rfiSystemSyncDispositionUnsigned => 'Unsigned';

  @override
  String get rfiSystemSyncDispositionManualReview => 'Manual review';

  @override
  String get opRfiCompleteStatusSyncTitle =>
      'Mark completed RFIs from the RFI System';

  @override
  String get opRfiCompleteStatusSyncDesc =>
      'Scans every RFI System subfolder for exact RFI No. PDFs and prepares safe RFI Log Inspection Status updates to Completed.';

  @override
  String get paramRfiCompleteStatusRfiSystemRoot => 'RFI System Root Folder';

  @override
  String get paramRfiCompleteStatusRfiSystemRootHelp =>
      'Select the RFI System root. Every nested folder is inspected read-only; symlinks are not followed.';

  @override
  String get paramRfiCompleteStatusRfiLogWorkbook => 'RFI Log Workbook';

  @override
  String get paramRfiCompleteStatusRfiLogWorkbookHelp =>
      'Select the .xlsm or .xlsx RFI Log whose safe Inspection Status cells may be changed to Completed after review.';

  @override
  String get errorRfiCompleteStatusSystemRootInvalid =>
      'Select a readable RFI System root folder.';

  @override
  String get errorRfiCompleteStatusWorkbookInvalid =>
      'Select a valid RFI Log .xlsm or .xlsx workbook.';

  @override
  String get errorRfiCompleteStatusWorkbookLocked =>
      'Close the selected RFI Log in Excel or another application, then try again.';

  @override
  String get errorRfiCompleteStatusWorkbookLayoutInvalid =>
      'Each CIV, MEC, ELE, and PRC sheet must contain one exact RFI No. and Inspection Status header.';

  @override
  String get errorRfiCompleteStatusApprovedUpdateInvalid =>
      'The approved completion-status update is invalid; preview again.';

  @override
  String get warnRfiCompleteStatusInvalidPdfName =>
      'An RFI No. PDF has a non-canonical filename and was not used.';

  @override
  String get warnRfiCompleteStatusInvalidPdfNameOverflow =>
      'More non-canonical RFI No. PDF names were omitted from this warning list.';

  @override
  String get warnRfiCompleteStatusNoChangesNeeded =>
      'No safe RFI Log completion changes are ready.';

  @override
  String get warnRfiCompleteStatusNoRowsSelected =>
      'Select at least one safe completed RFI to synchronize.';

  @override
  String get noteRfiCompleteStatusRfiLog =>
      'Set approved RFI Log Inspection Status cells to Completed';

  @override
  String get rfiCompleteStatusEditorTitle =>
      'Review completed RFI evidence and status updates';

  @override
  String rfiCompleteStatusEditorRowCount(Object count) {
    return '$count RFIs with exact completed PDFs';
  }

  @override
  String get rfiCompleteStatusColumnEnabled => 'Apply';

  @override
  String get rfiCompleteStatusColumnDiscipline => 'Discipline';

  @override
  String get rfiCompleteStatusColumnRfiNo => 'RFI No.';

  @override
  String get rfiCompleteStatusColumnPdfPath => 'Completed PDF';

  @override
  String get rfiCompleteStatusColumnPdfCount => 'PDF count';

  @override
  String get rfiCompleteStatusColumnCurrentStatus => 'Current status';

  @override
  String get rfiCompleteStatusColumnProposedStatus => 'New status';

  @override
  String get rfiCompleteStatusColumnDisposition => 'Decision';

  @override
  String get rfiCompleteStatusColumnAction => 'Action';

  @override
  String get rfiCompleteStatusColumnPdfPaths => 'All matching PDFs';

  @override
  String get rfiCompleteStatusColumnReason => 'Evidence / Details';

  @override
  String get rfiCompleteStatusDispositionReady => 'Ready';

  @override
  String get rfiCompleteStatusDispositionAlreadyCompleted =>
      'Already Completed';

  @override
  String get rfiCompleteStatusDispositionManualReview => 'Manual review';

  @override
  String get progressPhaseCompletionPdfDiscovery =>
      'Scanning all RFI System folders for completed PDFs';

  @override
  String get progressPhaseCompletionWorkbookRead =>
      'Reading RFI Log status cells';

  @override
  String get progressPhaseCompletionMapping =>
      'Matching completed PDFs to RFI Log rows';

  @override
  String get opFilesReplicaPublishTitle => 'Publish RFI Systems';

  @override
  String get opFilesReplicaPublishDesc =>
      'Publishes the selected source RFI System to two targets with retained backups, hash verification, and Undo.';

  @override
  String get paramReplicaRootA => 'System A folder';

  @override
  String get paramReplicaRootAHelp =>
      'One SharePoint, network, or local RFI System root.';

  @override
  String get paramReplicaRootB => 'System B folder';

  @override
  String get paramReplicaRootBHelp => 'Second RFI System root.';

  @override
  String get paramReplicaRootC => 'System C folder';

  @override
  String get paramReplicaRootCHelp => 'Third RFI System root.';

  @override
  String get paramReplicaSourceRoot => 'Authoritative source';

  @override
  String get paramReplicaSourceRootHelp =>
      'This system publishes to the other two. Target changes never flow back to the source.';

  @override
  String get choiceReplicaSourceA => 'System A';

  @override
  String get choiceReplicaSourceB => 'System B';

  @override
  String get choiceReplicaSourceC => 'System C';

  @override
  String get paramReplicaBackupRoot => 'Version backup folder';

  @override
  String get paramReplicaBackupRootHelp =>
      'Every target file being replaced is first copied to a separate retained backup.';

  @override
  String get paramReplicaRootFiles => 'Additional root files';

  @override
  String get paramReplicaRootFilesHelp =>
      'Separate root file names outside CIV, MEC, ELE, and PRC with semicolons or new lines.';

  @override
  String get paramReplicaUnattended => 'Enable unattended automatic mode';

  @override
  String get paramReplicaUnattendedHelp =>
      'Applying this preview once grants standing authority only to this fixed profile. Ambiguity pauses automation.';

  @override
  String get paramReplicaIntervalMinutes => 'Check interval (minutes)';

  @override
  String get paramReplicaIntervalMinutesHelp =>
      'Between 1 and 1440 minutes. Runs while DocFlow is open.';

  @override
  String get paramReplicaSettleSeconds => 'File settle time (seconds)';

  @override
  String get paramReplicaSettleSecondsHelp =>
      'Automatic publication waits this long after the newest source change. Between 5 and 3600.';

  @override
  String get paramReplicaMaxChangedFiles =>
      'Maximum file effects per automatic run';

  @override
  String get paramReplicaMaxChangedFilesHelp =>
      'Automation pauses if one run would change more target files.';

  @override
  String get paramReplicaMaxChangedMib => 'Maximum MiB per automatic run';

  @override
  String get paramReplicaMaxChangedMibHelp =>
      'Automation pauses if one run would publish more total data.';

  @override
  String get replicaEditorTitle => 'Review exact publication effects';

  @override
  String replicaEditorRowCount(Object count) {
    return '$count file effects';
  }

  @override
  String get replicaColumnTarget => 'Target';

  @override
  String get replicaColumnPath => 'Relative path';

  @override
  String get replicaColumnAction => 'Action';

  @override
  String get replicaColumnBeforeHash => 'Previous hash';

  @override
  String get replicaColumnAfterHash => 'Source hash';

  @override
  String get replicaActionCopy => 'Copy new file';

  @override
  String get replicaActionReplace => 'Back up and replace';

  @override
  String get noteReplicaMkdir => 'Create required folder';

  @override
  String get noteReplicaCopy => 'Copy missing target file';

  @override
  String get noteReplicaReplace =>
      'Back up, verify, and replace target from source';

  @override
  String get noteReplicaRestore =>
      'Restore target from its pre-publication backup';

  @override
  String get warnReplicaTargetExtra =>
      'Target contains a file absent from source; it was not deleted.';

  @override
  String get warnReplicaTargetExtraOverflow =>
      'More target-only files could not be listed; none were deleted.';

  @override
  String get warnReplicaNoChanges =>
      'All three systems already match the selected source.';

  @override
  String get errorReplicaRootInvalid =>
      'Select three readable RFI System roots.';

  @override
  String get errorReplicaBackupInvalid =>
      'Select a readable, separate backup folder.';

  @override
  String get errorReplicaSourceInvalid =>
      'Authoritative source selection is invalid.';

  @override
  String get errorReplicaRootsOverlap =>
      'RFI System roots cannot be equal or nested.';

  @override
  String get errorReplicaBackupOverlap =>
      'Backup folder cannot equal or contain any RFI System root.';

  @override
  String get errorReplicaRootFileInvalid =>
      'An additional root file name is invalid.';

  @override
  String get errorReplicaSourceRootFileMissing =>
      'A selected additional root file is missing from the source system.';

  @override
  String get errorReplicaTooManyRootFiles =>
      'At most 64 additional root files are allowed.';

  @override
  String get errorReplicaIntervalInvalid =>
      'Check interval must be between 1 and 1440 minutes.';

  @override
  String get errorReplicaSettleInvalid =>
      'File settle time must be between 5 and 3600 seconds.';

  @override
  String get errorReplicaChangeLimitInvalid =>
      'Automatic change limits must be positive and within the supported range.';

  @override
  String get errorReplicaSourceSettling =>
      'A source file changed recently; automatic publication was deferred until it settles.';

  @override
  String get errorReplicaChangeLimitExceeded =>
      'Automatic publication exceeds the approved file or size limit and was paused for safety.';

  @override
  String get errorReplicaDisciplineMissing =>
      'Source must contain all CIV, MEC, ELE, and PRC folders.';

  @override
  String get errorReplicaScanIncomplete =>
      'Folder scan could not finish; no files were published.';

  @override
  String get errorReplicaEntryLimit =>
      'RFI System exceeds the 500000-entry safety limit.';

  @override
  String get errorReplicaSymlinkRefused =>
      'A symbolic link was found; safe publication stopped.';

  @override
  String get errorReplicaPathCollision =>
      'Multiple files resolve to the same relative path.';

  @override
  String get errorReplicaFileUnstable =>
      'A file changed during scanning; preview again.';

  @override
  String get errorReplicaApprovedStepInvalid =>
      'Approved publication step is invalid; preview again.';

  @override
  String get errorReplicaSourceChanged =>
      'Source changed after preview; publication stopped.';

  @override
  String get errorReplicaTargetChanged =>
      'Target changed after preview; publication stopped.';

  @override
  String get errorReplicaBackupExists =>
      'This publication backup already exists and was not overwritten.';

  @override
  String get errorReplicaBackupVerificationFailed =>
      'Backup hash verification failed; automation paused.';

  @override
  String get errorReplicaCopyVerificationFailed =>
      'Temporary copy hash verification failed.';

  @override
  String get errorReplicaPublishVerificationFailed =>
      'Post-publication hash verification failed; recovery is required.';

  @override
  String get errorReplicaProfileMissing =>
      'No approved automatic sync profile exists.';

  @override
  String get errorReplicaProfileInvalid =>
      'Saved automatic sync profile is invalid.';

  @override
  String get errorReplicaActivationFailed =>
      'Files were published, but automatic profile activation failed.';

  @override
  String get errorReplicaAutomaticApplyFailed =>
      'Automatic publication failed and was paused.';

  @override
  String get progressPhaseReplicaDiscovery => 'Scanning all three RFI Systems';

  @override
  String get progressPhaseReplicaSourceHash =>
      'Hashing source files across bounded CPU workers';

  @override
  String get progressPhaseReplicaTargetHash =>
      'Hashing target files across bounded CPU workers';

  @override
  String get prefsAutomation => 'Windows and automation';

  @override
  String get prefsAutomationHelp =>
      'Control startup and the previously approved background sync profile.';

  @override
  String get prefsStartWithWindows => 'Start DocFlow with Windows';

  @override
  String get prefsReplicaSyncEnabled => 'Unattended RFI System sync active';

  @override
  String get prefsReplicaSyncEnabledHelp =>
      'Any error or ambiguity turns this off automatically. Backups are never auto-deleted.';

  @override
  String get prefsReplicaStatusUnconfigured =>
      'Status: No approved profile yet';

  @override
  String get prefsReplicaStatusDisabled => 'Status: Off';

  @override
  String get prefsReplicaStatusIdle => 'Status: Waiting for the next run';

  @override
  String get prefsReplicaStatusRunning => 'Status: Running';

  @override
  String get prefsReplicaStatusWaitingForSignIn =>
      'Status: Waiting for sign-in';

  @override
  String get prefsReplicaStatusDeferred =>
      'Status: Deferred until another operation finishes';

  @override
  String get prefsReplicaStatusPaused => 'Status: Paused for safety';

  @override
  String get paramRfiProcessScansCorrectOrientation => 'Correct page rotation';

  @override
  String get paramRfiProcessScansCorrectOrientationHelp =>
      'When enabled, rotates pages only when orientation is confidently determined. If any page is uncertain, correction is skipped for the entire PDF and it is sent intact to _review.';

  @override
  String get warnRfiProcessScansOrientationReview =>
      'Page orientation was uncertain; PDF was sent to _review';

  @override
  String get progressPhaseScanOrientation => 'Checking PDF page orientation';

  @override
  String get rfiExtractionPages => 'Extracted pages';

  @override
  String get reviewSelectAllKeep => 'Keep All';

  @override
  String get reviewSelectAllPut => 'Move All Possible';

  @override
  String get reviewSelectAllTrash => 'Move All to Trash';

  @override
  String get opsSearchPlaceholder => 'Search operations…';

  @override
  String get opsCategoryAll => 'All';

  @override
  String get opsCategoryScans => 'Scans & PDFs';

  @override
  String get opsCategoryExcel => 'Excel & Logs';

  @override
  String get opsCategoryMail => 'Outlook & Mail';

  @override
  String get opsCategorySystem => 'Publish & Print';

  @override
  String opsEmptySearch(Object query) {
    return 'No operations matching \'$query\'';
  }

  @override
  String get actionOpenDestination => 'Open Destination Folder';

  @override
  String get opRfiGenerateConcreteDocumentsTitle =>
      'Generate Concrete Documents';

  @override
  String get opRfiGenerateConcreteDocumentsDesc =>
      'Batch-generate concrete documents from RFI Log and Follow Up workbooks using selected RFI types and templates.';

  @override
  String get paramRfiDocumentsRfiLog => 'RFI Log workbook';

  @override
  String get paramRfiDocumentsFollowUp => 'Follow Up workbook';

  @override
  String get paramRfiDocumentsTemplate010 =>
      'Template 010 — Follow Up fields; ambient = plant fresh temperature minus random 0.5–0.7';

  @override
  String get paramRfiDocumentsTemplate011 =>
      'Template 011 — report/location and adjustable random ambient range (default 19–24)';

  @override
  String get paramRfiDocumentsTemplate013 =>
      'Template 013 — report/location only';

  @override
  String get paramRfiDocumentsTemplate014 =>
      'Template 014 — report/location only';

  @override
  String get paramRfiDocumentsOutput => 'Output folder';

  @override
  String get paramRfiDocumentsRfis010 => 'RFI numbers for Template 010';

  @override
  String get paramRfiDocumentsRfis011 => 'RFI numbers for Template 011';

  @override
  String get paramRfiDocumentsRfis013 => 'RFI numbers for Template 013';

  @override
  String get paramRfiDocumentsRfis014 => 'RFI numbers for Template 014';

  @override
  String get paramRfiDocumentsWaterRatios =>
      'Optional per-RFI water/cement ratios (blank when absent)';

  @override
  String get paramRfiDocumentsAmbientMin => 'Minimum ambient temperature';

  @override
  String get paramRfiDocumentsAmbientMax => 'Maximum ambient temperature';

  @override
  String get paramRfiDocumentsAmbientGroups =>
      'RFI-specific ambient temperature ranges';

  @override
  String get errorParamRequiredOutputDirectory => 'Output folder is required';

  @override
  String get errorRfiDocumentsInvalidInput =>
      'The concrete document inputs are invalid';

  @override
  String get warnRfiDocumentsOutputExists =>
      'An output file already exists and will not be overwritten';

  @override
  String get warnRfiDocumentsFollowUpMissing =>
      'No Concrete Specimen Test Follow-Up row was found for this RFI. It will be skipped. Apply creates available documents only; Cancel to correct the workbook and preview again.';

  @override
  String get warnRfiDocumentsFormNotRequired =>
      'The RFI Log Forms field does not list CON-CIV-010 for this RFI, so no CON-CIV-010 will be created. Check the requested type or correct the RFI Log, then preview again.';

  @override
  String get paramRfiDocumentsRfiLogHelp =>
      'The Civil sheet supplies each requested Report RFI Number and location.';

  @override
  String get paramRfiDocumentsFollowUpHelp =>
      'Required only when generating CON-CIV-010; supplies its concrete-pour fields.';

  @override
  String get paramRfiDocumentsOutputHelp =>
      'Generated files are created here; existing files are never overwritten.';

  @override
  String get paramRfiDocumentsRfisHelp =>
      'Enter full RFI numbers or CIV serials, separated by commas or new lines.';

  @override
  String get paramRfiDocumentsWaterRatiosHelp =>
      'Optional one per line as RFI number = ratio. Leave out an RFI to keep its cell blank.';

  @override
  String get paramRfiDocumentsAmbientGroupsHelp =>
      'Optional, one line per group: RFI[, RFI...]=MIN-MAX. Both range endpoints are included; unlisted RFIs use the default Minimum/Maximum ambient temperature.';

  @override
  String get stepRfiGeneratedConcreteDocument => 'Generate concrete document';
}
