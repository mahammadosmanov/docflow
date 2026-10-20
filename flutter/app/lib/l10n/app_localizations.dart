import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:intl/intl.dart' as intl;

import 'app_localizations_az.dart';
import 'app_localizations_en.dart';
import 'app_localizations_ru.dart';
import 'app_localizations_tr.dart';

// ignore_for_file: type=lint

/// Callers can lookup localized strings with an instance of AppLocalizations
/// returned by `AppLocalizations.of(context)`.
///
/// Applications need to include `AppLocalizations.delegate()` in their app's
/// `localizationDelegates` list, and the locales they support in the app's
/// `supportedLocales` list. For example:
///
/// ```dart
/// import 'l10n/app_localizations.dart';
///
/// return MaterialApp(
///   localizationsDelegates: AppLocalizations.localizationsDelegates,
///   supportedLocales: AppLocalizations.supportedLocales,
///   home: MyApplicationHome(),
/// );
/// ```
///
/// ## Update pubspec.yaml
///
/// Please make sure to update your pubspec.yaml to include the following
/// packages:
///
/// ```yaml
/// dependencies:
///   # Internationalization support.
///   flutter_localizations:
///     sdk: flutter
///   intl: any # Use the pinned version from flutter_localizations
///
///   # Rest of dependencies
/// ```
///
/// ## iOS Applications
///
/// iOS applications define key application metadata, including supported
/// locales, in an Info.plist file that is built into the application bundle.
/// To configure the locales supported by your app, you’ll need to edit this
/// file.
///
/// First, open your project’s ios/Runner.xcworkspace Xcode workspace file.
/// Then, in the Project Navigator, open the Info.plist file under the Runner
/// project’s Runner folder.
///
/// Next, select the Information Property List item, select Add Item from the
/// Editor menu, then select Localizations from the pop-up menu.
///
/// Select and expand the newly-created Localizations item then, for each
/// locale your application supports, add a new item and select the locale
/// you wish to add from the pop-up menu in the Value field. This list should
/// be consistent with the languages listed in the AppLocalizations.supportedLocales
/// property.
abstract class AppLocalizations {
  AppLocalizations(String locale)
    : localeName = intl.Intl.canonicalizedLocale(locale.toString());

  final String localeName;

  static AppLocalizations of(BuildContext context) {
    return Localizations.of<AppLocalizations>(context, AppLocalizations)!;
  }

  static const LocalizationsDelegate<AppLocalizations> delegate =
      _AppLocalizationsDelegate();

  /// A list of this localizations delegate along with the default localizations
  /// delegates.
  ///
  /// Returns a list of localizations delegates containing this delegate along with
  /// GlobalMaterialLocalizations.delegate, GlobalCupertinoLocalizations.delegate,
  /// and GlobalWidgetsLocalizations.delegate.
  ///
  /// Additional delegates can be added by appending to this list in
  /// MaterialApp. This list does not have to be used at all if a custom list
  /// of delegates is preferred or required.
  static const List<LocalizationsDelegate<dynamic>> localizationsDelegates =
      <LocalizationsDelegate<dynamic>>[
        delegate,
        GlobalMaterialLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
      ];

  /// A list of this localizations delegate's supported locales.
  static const List<Locale> supportedLocales = <Locale>[
    Locale('az'),
    Locale('en'),
    Locale('ru'),
    Locale('tr'),
  ];

  /// No description provided for @appTitle.
  ///
  /// In en, this message translates to:
  /// **'DocFlow'**
  String get appTitle;

  /// No description provided for @actionPreview.
  ///
  /// In en, this message translates to:
  /// **'Preview'**
  String get actionPreview;

  /// No description provided for @actionApply.
  ///
  /// In en, this message translates to:
  /// **'Apply'**
  String get actionApply;

  /// No description provided for @actionUndo.
  ///
  /// In en, this message translates to:
  /// **'Undo'**
  String get actionUndo;

  /// No description provided for @actionCancel.
  ///
  /// In en, this message translates to:
  /// **'Cancel'**
  String get actionCancel;

  /// No description provided for @actionBack.
  ///
  /// In en, this message translates to:
  /// **'Back'**
  String get actionBack;

  /// No description provided for @actionBrowse.
  ///
  /// In en, this message translates to:
  /// **'Browse'**
  String get actionBrowse;

  /// No description provided for @uiWarnings.
  ///
  /// In en, this message translates to:
  /// **'Warnings'**
  String get uiWarnings;

  /// No description provided for @resultApplied.
  ///
  /// In en, this message translates to:
  /// **'{count} applied'**
  String resultApplied(int count);

  /// No description provided for @resultFailed.
  ///
  /// In en, this message translates to:
  /// **'{count} failed'**
  String resultFailed(int count);

  /// No description provided for @actionOpen.
  ///
  /// In en, this message translates to:
  /// **'Open'**
  String get actionOpen;

  /// No description provided for @actionReview.
  ///
  /// In en, this message translates to:
  /// **'Review folder'**
  String get actionReview;

  /// No description provided for @reviewTitle.
  ///
  /// In en, this message translates to:
  /// **'Review Files'**
  String get reviewTitle;

  /// No description provided for @reviewEmpty.
  ///
  /// In en, this message translates to:
  /// **'The review folder is empty'**
  String get reviewEmpty;

  /// No description provided for @reviewKeep.
  ///
  /// In en, this message translates to:
  /// **'Keep as is'**
  String get reviewKeep;

  /// No description provided for @reviewPut.
  ///
  /// In en, this message translates to:
  /// **'Move to destination'**
  String get reviewPut;

  /// No description provided for @reviewDelete.
  ///
  /// In en, this message translates to:
  /// **'Move to Trash'**
  String get reviewDelete;

  /// No description provided for @reviewNoCode.
  ///
  /// In en, this message translates to:
  /// **'No RFI code'**
  String get reviewNoCode;

  /// No description provided for @reviewWillReplace.
  ///
  /// In en, this message translates to:
  /// **'replaces the existing file'**
  String get reviewWillReplace;

  /// No description provided for @noteDeleted.
  ///
  /// In en, this message translates to:
  /// **'Deleted (to trash)'**
  String get noteDeleted;

  /// No description provided for @noteDisplaced.
  ///
  /// In en, this message translates to:
  /// **'Replaced — moved to trash'**
  String get noteDisplaced;

  /// No description provided for @actionHistory.
  ///
  /// In en, this message translates to:
  /// **'History'**
  String get actionHistory;

  /// No description provided for @historyTitle.
  ///
  /// In en, this message translates to:
  /// **'History'**
  String get historyTitle;

  /// No description provided for @historyUnknownOperation.
  ///
  /// In en, this message translates to:
  /// **'Unknown operation'**
  String get historyUnknownOperation;

  /// No description provided for @historyEmpty.
  ///
  /// In en, this message translates to:
  /// **'No history yet'**
  String get historyEmpty;

  /// No description provided for @historyMoves.
  ///
  /// In en, this message translates to:
  /// **'{count} files'**
  String historyMoves(int count);

  /// No description provided for @historyUndoOriginal.
  ///
  /// In en, this message translates to:
  /// **'Undo to original location'**
  String get historyUndoOriginal;

  /// No description provided for @historyUndoToFolder.
  ///
  /// In en, this message translates to:
  /// **'Undo to a folder…'**
  String get historyUndoToFolder;

  /// No description provided for @planEmpty.
  ///
  /// In en, this message translates to:
  /// **'Nothing to do'**
  String get planEmpty;

  /// No description provided for @planSummary.
  ///
  /// In en, this message translates to:
  /// **'{count} files will be processed'**
  String planSummary(int count);

  /// No description provided for @planImpactTitle.
  ///
  /// In en, this message translates to:
  /// **'Before you apply'**
  String get planImpactTitle;

  /// No description provided for @planImpactPreviewOnly.
  ///
  /// In en, this message translates to:
  /// **'Nothing has changed yet. Apply executes only this approved preview.'**
  String get planImpactPreviewOnly;

  /// No description provided for @planImpactOutputs.
  ///
  /// In en, this message translates to:
  /// **'New outputs'**
  String get planImpactOutputs;

  /// No description provided for @planImpactUpdates.
  ///
  /// In en, this message translates to:
  /// **'Files updated'**
  String get planImpactUpdates;

  /// No description provided for @planImpactMoves.
  ///
  /// In en, this message translates to:
  /// **'Moved / renamed'**
  String get planImpactMoves;

  /// No description provided for @planImpactPrintJobs.
  ///
  /// In en, this message translates to:
  /// **'Print jobs'**
  String get planImpactPrintJobs;

  /// No description provided for @planImpactDrafts.
  ///
  /// In en, this message translates to:
  /// **'Drafts opened'**
  String get planImpactDrafts;

  /// No description provided for @planImpactCleanup.
  ///
  /// In en, this message translates to:
  /// **'Cleanup'**
  String get planImpactCleanup;

  /// No description provided for @planImpactWarnings.
  ///
  /// In en, this message translates to:
  /// **'Warnings'**
  String get planImpactWarnings;

  /// No description provided for @scanIndexTitle.
  ///
  /// In en, this message translates to:
  /// **'Incremental scan index'**
  String get scanIndexTitle;

  /// No description provided for @scanIndexHelp.
  ///
  /// In en, this message translates to:
  /// **'Unchanged files were reused from the durable local index.'**
  String get scanIndexHelp;

  /// No description provided for @scanIndexForceApplied.
  ///
  /// In en, this message translates to:
  /// **'Full rescan was requested for this run.'**
  String get scanIndexForceApplied;

  /// No description provided for @scanIndexDiscovered.
  ///
  /// In en, this message translates to:
  /// **'Found'**
  String get scanIndexDiscovered;

  /// No description provided for @scanIndexReused.
  ///
  /// In en, this message translates to:
  /// **'Reused'**
  String get scanIndexReused;

  /// No description provided for @scanIndexAnalyzed.
  ///
  /// In en, this message translates to:
  /// **'Scanned now'**
  String get scanIndexAnalyzed;

  /// No description provided for @scanIndexFailed.
  ///
  /// In en, this message translates to:
  /// **'Failed'**
  String get scanIndexFailed;

  /// No description provided for @scanIndexInterrupted.
  ///
  /// In en, this message translates to:
  /// **'Earlier interrupted runs'**
  String get scanIndexInterrupted;

  /// No description provided for @planWarningsSearch.
  ///
  /// In en, this message translates to:
  /// **'Search warnings'**
  String get planWarningsSearch;

  /// No description provided for @planWarningsType.
  ///
  /// In en, this message translates to:
  /// **'Warning type'**
  String get planWarningsType;

  /// No description provided for @planWarningsAll.
  ///
  /// In en, this message translates to:
  /// **'All warning types'**
  String get planWarningsAll;

  /// No description provided for @planWarningsFocus.
  ///
  /// In en, this message translates to:
  /// **'Show related page'**
  String get planWarningsFocus;

  /// No description provided for @planWarningsNone.
  ///
  /// In en, this message translates to:
  /// **'No warnings match these filters.'**
  String get planWarningsNone;

  /// No description provided for @planWarningsShowing.
  ///
  /// In en, this message translates to:
  /// **'Showing {shown} of {count} warnings'**
  String planWarningsShowing(Object count, Object shown);

  /// No description provided for @planWorkbookChangesTitle.
  ///
  /// In en, this message translates to:
  /// **'Exact workbook cell changes'**
  String get planWorkbookChangesTitle;

  /// No description provided for @planWorkbookChangesCount.
  ///
  /// In en, this message translates to:
  /// **'{count} approved cell changes'**
  String planWorkbookChangesCount(Object count);

  /// No description provided for @planWorkbookChangesHelp.
  ///
  /// In en, this message translates to:
  /// **'Expand to compare the current value with the exact value Apply will write.'**
  String get planWorkbookChangesHelp;

  /// No description provided for @planWorkbookChangesWorkbook.
  ///
  /// In en, this message translates to:
  /// **'Workbook'**
  String get planWorkbookChangesWorkbook;

  /// No description provided for @planWorkbookChangesSheetCell.
  ///
  /// In en, this message translates to:
  /// **'Sheet and cell'**
  String get planWorkbookChangesSheetCell;

  /// No description provided for @planWorkbookChangesRfi.
  ///
  /// In en, this message translates to:
  /// **'RFI'**
  String get planWorkbookChangesRfi;

  /// No description provided for @planWorkbookChangesBefore.
  ///
  /// In en, this message translates to:
  /// **'Current'**
  String get planWorkbookChangesBefore;

  /// No description provided for @planWorkbookChangesAfter.
  ///
  /// In en, this message translates to:
  /// **'After Apply'**
  String get planWorkbookChangesAfter;

  /// No description provided for @planWorkbookChangesBlank.
  ///
  /// In en, this message translates to:
  /// **'(blank)'**
  String get planWorkbookChangesBlank;

  /// No description provided for @planWorkbookChangesKindRfiLog.
  ///
  /// In en, this message translates to:
  /// **'RFI Log'**
  String get planWorkbookChangesKindRfiLog;

  /// No description provided for @planWorkbookChangesKindFollowUp.
  ///
  /// In en, this message translates to:
  /// **'Follow Up'**
  String get planWorkbookChangesKindFollowUp;

  /// No description provided for @planWorkbookChangesKindWorkbook.
  ///
  /// In en, this message translates to:
  /// **'Workbook'**
  String get planWorkbookChangesKindWorkbook;

  /// No description provided for @planWorkbookChangesLimit.
  ///
  /// In en, this message translates to:
  /// **'The preview is limited to the first {count} approved cell changes.'**
  String planWorkbookChangesLimit(Object count);

  /// No description provided for @planExactTitle.
  ///
  /// In en, this message translates to:
  /// **'{count} approved effects'**
  String planExactTitle(Object count);

  /// No description provided for @planExactHelp.
  ///
  /// In en, this message translates to:
  /// **'Open to inspect exact source and destination paths'**
  String get planExactHelp;

  /// No description provided for @opRfiProcessScansTitle.
  ///
  /// In en, this message translates to:
  /// **'Process Scans'**
  String get opRfiProcessScansTitle;

  /// No description provided for @opRfiProcessScansFastCoversTitle.
  ///
  /// In en, this message translates to:
  /// **'Fast RFI Scan'**
  String get opRfiProcessScansFastCoversTitle;

  /// No description provided for @opRfiProcessScansFastCoversDesc.
  ///
  /// In en, this message translates to:
  /// **'Scans RFI covers in multi-page scans. Choose Multi-RFI to separate them or Single-RFI to read only page one and keep the complete PDF. OCR-only CLV readings are normalized to CIV. Uncertain scans go to _review.'**
  String get opRfiProcessScansFastCoversDesc;

  /// No description provided for @paramRfiProcessScansScanMode.
  ///
  /// In en, this message translates to:
  /// **'Scan mode'**
  String get paramRfiProcessScansScanMode;

  /// No description provided for @paramRfiProcessScansScanModeHelp.
  ///
  /// In en, this message translates to:
  /// **'Multi-RFI separates a combined scan. Single-RFI reads only its first page and keeps the complete PDF intact.'**
  String get paramRfiProcessScansScanModeHelp;

  /// No description provided for @choiceRfiProcessScansScanModeMultiRfi.
  ///
  /// In en, this message translates to:
  /// **'Separate multiple RFIs'**
  String get choiceRfiProcessScansScanModeMultiRfi;

  /// No description provided for @choiceRfiProcessScansScanModeSingleRfi.
  ///
  /// In en, this message translates to:
  /// **'One RFI — read first page only'**
  String get choiceRfiProcessScansScanModeSingleRfi;

  /// No description provided for @rfiFastScanEditorTitle.
  ///
  /// In en, this message translates to:
  /// **'Review and correct RFI numbers'**
  String get rfiFastScanEditorTitle;

  /// No description provided for @rfiFastScanEditorRowCount.
  ///
  /// In en, this message translates to:
  /// **'{count} scan files'**
  String rfiFastScanEditorRowCount(int count);

  /// No description provided for @rfiFastScanColumnSourceFile.
  ///
  /// In en, this message translates to:
  /// **'Source file'**
  String get rfiFastScanColumnSourceFile;

  /// No description provided for @rfiFastScanColumnEvidence.
  ///
  /// In en, this message translates to:
  /// **'Recognition'**
  String get rfiFastScanColumnEvidence;

  /// No description provided for @rfiFastScanColumnDetectedCode.
  ///
  /// In en, this message translates to:
  /// **'Detected RFI'**
  String get rfiFastScanColumnDetectedCode;

  /// No description provided for @rfiFastScanColumnCode.
  ///
  /// In en, this message translates to:
  /// **'RFI number'**
  String get rfiFastScanColumnCode;

  /// No description provided for @rfiFastScanColumnTarget.
  ///
  /// In en, this message translates to:
  /// **'Destination'**
  String get rfiFastScanColumnTarget;

  /// No description provided for @rfiFastScanEvidenceClvToCiv.
  ///
  /// In en, this message translates to:
  /// **'OCR alias: CLV → CIV'**
  String get rfiFastScanEvidenceClvToCiv;

  /// No description provided for @rfiFastScanEvidenceConfirmed.
  ///
  /// In en, this message translates to:
  /// **'Confirmed'**
  String get rfiFastScanEvidenceConfirmed;

  /// No description provided for @rfiFastScanEditorUpdatePreview.
  ///
  /// In en, this message translates to:
  /// **'Update preview'**
  String get rfiFastScanEditorUpdatePreview;

  /// No description provided for @rfiFastScanEditorUpdatingPreview.
  ///
  /// In en, this message translates to:
  /// **'Updating preview…'**
  String get rfiFastScanEditorUpdatingPreview;

  /// No description provided for @rfiFastScanEditorPreviewUpdated.
  ///
  /// In en, this message translates to:
  /// **'Preview updated. Check the destination paths before applying.'**
  String get rfiFastScanEditorPreviewUpdated;

  /// No description provided for @errorRfiFastScanCodeInvalid.
  ///
  /// In en, this message translates to:
  /// **'Enter a supported full RFI code with a one-to-four ASCII-digit suffix; it is stored with four digits.'**
  String get errorRfiFastScanCodeInvalid;

  /// No description provided for @errorRfiFastScanTargetExists.
  ///
  /// In en, this message translates to:
  /// **'Another scan already uses this RFI number, or the destination already exists.'**
  String get errorRfiFastScanTargetExists;

  /// No description provided for @errorRfiFastScanRevisionInvalid.
  ///
  /// In en, this message translates to:
  /// **'The edited scan preview is invalid. Review the RFI numbers and try again.'**
  String get errorRfiFastScanRevisionInvalid;

  /// No description provided for @opRfiGroupFilesTitle.
  ///
  /// In en, this message translates to:
  /// **'Group Files by RFI'**
  String get opRfiGroupFilesTitle;

  /// No description provided for @opRfiGroupFilesDesc.
  ///
  /// In en, this message translates to:
  /// **'Finds exact RFI numbers in file names and contents, then copies approved files to <discipline>/<full RFI code> folders.'**
  String get opRfiGroupFilesDesc;

  /// No description provided for @paramRfiFileGroupSource.
  ///
  /// In en, this message translates to:
  /// **'Incoming files folder'**
  String get paramRfiFileGroupSource;

  /// No description provided for @paramRfiFileGroupSourceHelp.
  ///
  /// In en, this message translates to:
  /// **'Only top-level files are scanned; subfolders are left unchanged and skipped.'**
  String get paramRfiFileGroupSourceHelp;

  /// No description provided for @paramRfiFileGroupDestination.
  ///
  /// In en, this message translates to:
  /// **'Grouped output folder'**
  String get paramRfiFileGroupDestination;

  /// No description provided for @paramRfiFileGroupDestinationHelp.
  ///
  /// In en, this message translates to:
  /// **'Choose an empty or separate target outside the source. Existing files are never overwritten.'**
  String get paramRfiFileGroupDestinationHelp;

  /// No description provided for @rfiFileGroupEditorTitle.
  ///
  /// In en, this message translates to:
  /// **'Detected RFI matches'**
  String get rfiFileGroupEditorTitle;

  /// No description provided for @rfiFileGroupEditorRowCount.
  ///
  /// In en, this message translates to:
  /// **'{count} copy matches'**
  String rfiFileGroupEditorRowCount(Object count);

  /// No description provided for @rfiFileGroupColumnEnabled.
  ///
  /// In en, this message translates to:
  /// **'Copy'**
  String get rfiFileGroupColumnEnabled;

  /// No description provided for @rfiFileGroupColumnRfiCode.
  ///
  /// In en, this message translates to:
  /// **'RFI number'**
  String get rfiFileGroupColumnRfiCode;

  /// No description provided for @rfiFileGroupColumnSourceFile.
  ///
  /// In en, this message translates to:
  /// **'Source file'**
  String get rfiFileGroupColumnSourceFile;

  /// No description provided for @rfiFileGroupColumnMethod.
  ///
  /// In en, this message translates to:
  /// **'Detection method'**
  String get rfiFileGroupColumnMethod;

  /// No description provided for @rfiFileGroupMethodFilename.
  ///
  /// In en, this message translates to:
  /// **'File name'**
  String get rfiFileGroupMethodFilename;

  /// No description provided for @rfiFileGroupMethodPdf.
  ///
  /// In en, this message translates to:
  /// **'PDF / OCR'**
  String get rfiFileGroupMethodPdf;

  /// No description provided for @rfiFileGroupMethodMail.
  ///
  /// In en, this message translates to:
  /// **'Mail content'**
  String get rfiFileGroupMethodMail;

  /// No description provided for @rfiFileGroupMethodOffice.
  ///
  /// In en, this message translates to:
  /// **'Office content'**
  String get rfiFileGroupMethodOffice;

  /// No description provided for @rfiFileGroupMethodContent.
  ///
  /// In en, this message translates to:
  /// **'File content'**
  String get rfiFileGroupMethodContent;

  /// No description provided for @noteRfiFileGroupFolderCreated.
  ///
  /// In en, this message translates to:
  /// **'RFI folder will be created'**
  String get noteRfiFileGroupFolderCreated;

  /// No description provided for @noteRfiFileGroupCopy.
  ///
  /// In en, this message translates to:
  /// **'File will be copied into the discipline and full RFI-code folder'**
  String get noteRfiFileGroupCopy;

  /// No description provided for @noteRfiFileGroupReviewCopy.
  ///
  /// In en, this message translates to:
  /// **'File will be copied into the review folder'**
  String get noteRfiFileGroupReviewCopy;

  /// No description provided for @warnRfiFileGroupNoCode.
  ///
  /// In en, this message translates to:
  /// **'No RFI number was found; the file will be placed in review'**
  String get warnRfiFileGroupNoCode;

  /// No description provided for @warnRfiFileGroupUnreadable.
  ///
  /// In en, this message translates to:
  /// **'The file could not be fully read; it will be placed in review'**
  String get warnRfiFileGroupUnreadable;

  /// No description provided for @warnRfiFileGroupPartialInspection.
  ///
  /// In en, this message translates to:
  /// **'File-name matches are usable, but the content could not be fully inspected'**
  String get warnRfiFileGroupPartialInspection;

  /// No description provided for @warnRfiFileGroupExists.
  ///
  /// In en, this message translates to:
  /// **'The destination file already exists and will not be overwritten'**
  String get warnRfiFileGroupExists;

  /// No description provided for @warnRfiFileGroupNoFiles.
  ///
  /// In en, this message translates to:
  /// **'No top-level files were found in the source folder'**
  String get warnRfiFileGroupNoFiles;

  /// No description provided for @warnRfiFileGroupMissingEmail.
  ///
  /// In en, this message translates to:
  /// **'No EML or MSG email file was found for this RFI; related files will not be placed'**
  String get warnRfiFileGroupMissingEmail;

  /// No description provided for @errorParamRequiredDestination.
  ///
  /// In en, this message translates to:
  /// **'Destination folder is required'**
  String get errorParamRequiredDestination;

  /// No description provided for @paramSource.
  ///
  /// In en, this message translates to:
  /// **'Source folder'**
  String get paramSource;

  /// No description provided for @paramSourceHelp.
  ///
  /// In en, this message translates to:
  /// **'Folder containing the scanned PDFs'**
  String get paramSourceHelp;

  /// No description provided for @paramDest.
  ///
  /// In en, this message translates to:
  /// **'Destination folder'**
  String get paramDest;

  /// No description provided for @paramDestHelp.
  ///
  /// In en, this message translates to:
  /// **'Where processed RFI PDFs will be saved'**
  String get paramDestHelp;

  /// No description provided for @paramKeepOriginal.
  ///
  /// In en, this message translates to:
  /// **'Keep originals (copy instead of move)'**
  String get paramKeepOriginal;

  /// No description provided for @noteRenamed.
  ///
  /// In en, this message translates to:
  /// **'Renamed'**
  String get noteRenamed;

  /// No description provided for @noteNeedsReview.
  ///
  /// In en, this message translates to:
  /// **'Will be moved to review'**
  String get noteNeedsReview;

  /// No description provided for @noteUnreadable.
  ///
  /// In en, this message translates to:
  /// **'Unreadable — will be moved to review'**
  String get noteUnreadable;

  /// No description provided for @warnNoCode.
  ///
  /// In en, this message translates to:
  /// **'RFI number not found'**
  String get warnNoCode;

  /// No description provided for @warnLowConfidence.
  ///
  /// In en, this message translates to:
  /// **'Low confidence — needs review'**
  String get warnLowConfidence;

  /// No description provided for @warnUnknownRfiFolder.
  ///
  /// In en, this message translates to:
  /// **'Detected RFI folder does not exist — moved to review'**
  String get warnUnknownRfiFolder;

  /// No description provided for @warnUnreadable.
  ///
  /// In en, this message translates to:
  /// **'File could not be read'**
  String get warnUnreadable;

  /// No description provided for @warnRevisionGuessed.
  ///
  /// In en, this message translates to:
  /// **'Revision number guessed'**
  String get warnRevisionGuessed;

  /// No description provided for @warnDuplicate.
  ///
  /// In en, this message translates to:
  /// **'Duplicate RFI number — moved to review'**
  String get warnDuplicate;

  /// No description provided for @warnExists.
  ///
  /// In en, this message translates to:
  /// **'Target already exists — moved to review (not overwritten)'**
  String get warnExists;

  /// No description provided for @warnNoPdfs.
  ///
  /// In en, this message translates to:
  /// **'No PDFs in folder'**
  String get warnNoPdfs;

  /// No description provided for @errorStepFailed.
  ///
  /// In en, this message translates to:
  /// **'Step failed'**
  String get errorStepFailed;

  /// No description provided for @errorUndoFailed.
  ///
  /// In en, this message translates to:
  /// **'Undo failed'**
  String get errorUndoFailed;

  /// No description provided for @errorParamRequiredSource.
  ///
  /// In en, this message translates to:
  /// **'Source folder is required'**
  String get errorParamRequiredSource;

  /// No description provided for @errorParamRequiredDest.
  ///
  /// In en, this message translates to:
  /// **'Destination folder is required'**
  String get errorParamRequiredDest;

  /// No description provided for @actionAdmin.
  ///
  /// In en, this message translates to:
  /// **'Admin panel'**
  String get actionAdmin;

  /// No description provided for @actionRetry.
  ///
  /// In en, this message translates to:
  /// **'Check again'**
  String get actionRetry;

  /// No description provided for @bootstrapFatalTitle.
  ///
  /// In en, this message translates to:
  /// **'DocFlow couldn\'t start'**
  String get bootstrapFatalTitle;

  /// No description provided for @bootstrapRetry.
  ///
  /// In en, this message translates to:
  /// **'Try again'**
  String get bootstrapRetry;

  /// No description provided for @authEmail.
  ///
  /// In en, this message translates to:
  /// **'Email'**
  String get authEmail;

  /// No description provided for @authPassword.
  ///
  /// In en, this message translates to:
  /// **'Password'**
  String get authPassword;

  /// No description provided for @authSignIn.
  ///
  /// In en, this message translates to:
  /// **'Sign in'**
  String get authSignIn;

  /// No description provided for @authSignUp.
  ///
  /// In en, this message translates to:
  /// **'Create account'**
  String get authSignUp;

  /// No description provided for @authSignOut.
  ///
  /// In en, this message translates to:
  /// **'Sign out'**
  String get authSignOut;

  /// No description provided for @authSignupOk.
  ///
  /// In en, this message translates to:
  /// **'Account created. Ask the admin to approve it, then sign in.'**
  String get authSignupOk;

  /// No description provided for @authPending.
  ///
  /// In en, this message translates to:
  /// **'Waiting for approval'**
  String get authPending;

  /// No description provided for @authPendingHelp.
  ///
  /// In en, this message translates to:
  /// **'Your account exists but has not been approved yet. The admin must grant you access.'**
  String get authPendingHelp;

  /// No description provided for @adminTitle.
  ///
  /// In en, this message translates to:
  /// **'Users'**
  String get adminTitle;

  /// No description provided for @adminAllowed.
  ///
  /// In en, this message translates to:
  /// **'Access'**
  String get adminAllowed;

  /// No description provided for @adminAdmin.
  ///
  /// In en, this message translates to:
  /// **'Admin'**
  String get adminAdmin;

  /// No description provided for @adminEmpty.
  ///
  /// In en, this message translates to:
  /// **'No users yet'**
  String get adminEmpty;

  /// No description provided for @actionDone.
  ///
  /// In en, this message translates to:
  /// **'Done'**
  String get actionDone;

  /// No description provided for @adminHelp.
  ///
  /// In en, this message translates to:
  /// **'Turn on Access to let someone use the app. Your own switches are locked so you cannot lock yourself out.'**
  String get adminHelp;

  /// No description provided for @adminNoAccess.
  ///
  /// In en, this message translates to:
  /// **'no access'**
  String get adminNoAccess;

  /// No description provided for @adminYou.
  ///
  /// In en, this message translates to:
  /// **'you'**
  String get adminYou;

  /// No description provided for @authSubtitle.
  ///
  /// In en, this message translates to:
  /// **'Sign in to continue'**
  String get authSubtitle;

  /// No description provided for @authShow.
  ///
  /// In en, this message translates to:
  /// **'Show password'**
  String get authShow;

  /// No description provided for @authHide.
  ///
  /// In en, this message translates to:
  /// **'Hide password'**
  String get authHide;

  /// No description provided for @authPasswordHint.
  ///
  /// In en, this message translates to:
  /// **'At least 6 characters'**
  String get authPasswordHint;

  /// No description provided for @historyEmptyHelp.
  ///
  /// In en, this message translates to:
  /// **'Operations you apply will be listed here, and you can undo them.'**
  String get historyEmptyHelp;

  /// No description provided for @historyUndone.
  ///
  /// In en, this message translates to:
  /// **'Undone'**
  String get historyUndone;

  /// No description provided for @historyLocalJournalRequired.
  ///
  /// In en, this message translates to:
  /// **'The local journal on this device is required to undo.'**
  String get historyLocalJournalRequired;

  /// No description provided for @historyCloud.
  ///
  /// In en, this message translates to:
  /// **'Cloud'**
  String get historyCloud;

  /// No description provided for @historyLocal.
  ///
  /// In en, this message translates to:
  /// **'Local'**
  String get historyLocal;

  /// No description provided for @historySourceCloud.
  ///
  /// In en, this message translates to:
  /// **'Shared history, visible on all your machines'**
  String get historySourceCloud;

  /// No description provided for @historySourceLocal.
  ///
  /// In en, this message translates to:
  /// **'Local history only — cloud unreachable'**
  String get historySourceLocal;

  /// No description provided for @opsHeading.
  ///
  /// In en, this message translates to:
  /// **'Operations'**
  String get opsHeading;

  /// No description provided for @opsSubheading.
  ///
  /// In en, this message translates to:
  /// **'Choose an operation. Nothing will change on disk until you review and approve.'**
  String get opsSubheading;

  /// No description provided for @homeTitle.
  ///
  /// In en, this message translates to:
  /// **'Workspace'**
  String get homeTitle;

  /// No description provided for @homeSubtitle.
  ///
  /// In en, this message translates to:
  /// **'Document operations and every supporting tool in one place.'**
  String get homeSubtitle;

  /// No description provided for @homePreviewFirst.
  ///
  /// In en, this message translates to:
  /// **'Preview first'**
  String get homePreviewFirst;

  /// No description provided for @ocrConfidence.
  ///
  /// In en, this message translates to:
  /// **'OCR evidence score'**
  String get ocrConfidence;

  /// No description provided for @ocrConfidenceShort.
  ///
  /// In en, this message translates to:
  /// **'Evidence'**
  String get ocrConfidenceShort;

  /// No description provided for @ocrConfidenceHelp.
  ///
  /// In en, this message translates to:
  /// **'Heuristic OCR consistency score out of 100. It is not an accuracy probability, confirmation, or automatic approval.'**
  String get ocrConfidenceHelp;

  /// No description provided for @homeFeatures.
  ///
  /// In en, this message translates to:
  /// **'Other features'**
  String get homeFeatures;

  /// No description provided for @homeFeaturesHelp.
  ///
  /// In en, this message translates to:
  /// **'Review, history, account, and application management.'**
  String get homeFeaturesHelp;

  /// No description provided for @homeReviewDesc.
  ///
  /// In en, this message translates to:
  /// **'Inspect pending files individually and place them safely.'**
  String get homeReviewDesc;

  /// No description provided for @homeHistoryDesc.
  ///
  /// In en, this message translates to:
  /// **'See completed operations and safely undo them.'**
  String get homeHistoryDesc;

  /// No description provided for @homeSettingsDesc.
  ///
  /// In en, this message translates to:
  /// **'Manage language, appearance, text, and account preferences.'**
  String get homeSettingsDesc;

  /// No description provided for @homeAdminDesc.
  ///
  /// In en, this message translates to:
  /// **'Manage user access and administrator permissions.'**
  String get homeAdminDesc;

  /// No description provided for @paramHintFolder.
  ///
  /// In en, this message translates to:
  /// **'Choose a folder'**
  String get paramHintFolder;

  /// No description provided for @paramHintFile.
  ///
  /// In en, this message translates to:
  /// **'Choose a file'**
  String get paramHintFile;

  /// No description provided for @planApplying.
  ///
  /// In en, this message translates to:
  /// **'Applying'**
  String get planApplying;

  /// No description provided for @planReadOnly.
  ///
  /// In en, this message translates to:
  /// **'Nothing has changed yet'**
  String get planReadOnly;

  /// No description provided for @prefsTitle.
  ///
  /// In en, this message translates to:
  /// **'Settings'**
  String get prefsTitle;

  /// No description provided for @prefsLanguage.
  ///
  /// In en, this message translates to:
  /// **'Language'**
  String get prefsLanguage;

  /// No description provided for @prefsLangSystem.
  ///
  /// In en, this message translates to:
  /// **'System language'**
  String get prefsLangSystem;

  /// No description provided for @prefsLangTr.
  ///
  /// In en, this message translates to:
  /// **'Türkçe'**
  String get prefsLangTr;

  /// No description provided for @prefsLangEn.
  ///
  /// In en, this message translates to:
  /// **'English'**
  String get prefsLangEn;

  /// No description provided for @prefsLangRu.
  ///
  /// In en, this message translates to:
  /// **'Русский'**
  String get prefsLangRu;

  /// No description provided for @prefsLangAz.
  ///
  /// In en, this message translates to:
  /// **'Azərbaycanca'**
  String get prefsLangAz;

  /// No description provided for @prefsTheme.
  ///
  /// In en, this message translates to:
  /// **'Theme'**
  String get prefsTheme;

  /// No description provided for @prefsThemeSystem.
  ///
  /// In en, this message translates to:
  /// **'System'**
  String get prefsThemeSystem;

  /// No description provided for @prefsThemeLight.
  ///
  /// In en, this message translates to:
  /// **'Light'**
  String get prefsThemeLight;

  /// No description provided for @prefsThemeDark.
  ///
  /// In en, this message translates to:
  /// **'Dark'**
  String get prefsThemeDark;

  /// No description provided for @prefsFont.
  ///
  /// In en, this message translates to:
  /// **'Font'**
  String get prefsFont;

  /// No description provided for @prefsFontHelp.
  ///
  /// In en, this message translates to:
  /// **'Uses fonts already installed on this PC — nothing is downloaded.'**
  String get prefsFontHelp;

  /// No description provided for @prefsFontSystem.
  ///
  /// In en, this message translates to:
  /// **'System default'**
  String get prefsFontSystem;

  /// No description provided for @prefsFontSans.
  ///
  /// In en, this message translates to:
  /// **'Sans serif'**
  String get prefsFontSans;

  /// No description provided for @prefsFontSerif.
  ///
  /// In en, this message translates to:
  /// **'Serif'**
  String get prefsFontSerif;

  /// No description provided for @prefsFontMono.
  ///
  /// In en, this message translates to:
  /// **'Monospace'**
  String get prefsFontMono;

  /// No description provided for @prefsFontSize.
  ///
  /// In en, this message translates to:
  /// **'Text size'**
  String get prefsFontSize;

  /// No description provided for @prefsFontSizeSample.
  ///
  /// In en, this message translates to:
  /// **'AZE-SUN-INT-QMS-RFI-CIV-0064 — sample text at this size.'**
  String get prefsFontSizeSample;

  /// No description provided for @prefsDensity.
  ///
  /// In en, this message translates to:
  /// **'Spacing'**
  String get prefsDensity;

  /// No description provided for @prefsDensityCompact.
  ///
  /// In en, this message translates to:
  /// **'Compact layout'**
  String get prefsDensityCompact;

  /// No description provided for @prefsDensityHelp.
  ///
  /// In en, this message translates to:
  /// **'Fits more on screen. Useful on small displays.'**
  String get prefsDensityHelp;

  /// No description provided for @prefsSupportTitle.
  ///
  /// In en, this message translates to:
  /// **'Advanced support'**
  String get prefsSupportTitle;

  /// No description provided for @prefsSupportHelp.
  ///
  /// In en, this message translates to:
  /// **'Create a read-only, redacted local support bundle.'**
  String get prefsSupportHelp;

  /// No description provided for @prefsSupportPrivacy.
  ///
  /// In en, this message translates to:
  /// **'Includes the version, platform, sidecar/OCR status, recent error codes, and the latest pathless printer-queue result; document contents, credentials, tokens, and private paths are excluded. Nothing is sent anywhere.'**
  String get prefsSupportPrivacy;

  /// No description provided for @prefsSupportPreview.
  ///
  /// In en, this message translates to:
  /// **'Preview support bundle'**
  String get prefsSupportPreview;

  /// No description provided for @prefsSupportPreviewLabel.
  ///
  /// In en, this message translates to:
  /// **'Support bundle preview'**
  String get prefsSupportPreviewLabel;

  /// No description provided for @prefsSupportSize.
  ///
  /// In en, this message translates to:
  /// **'{count} bytes'**
  String prefsSupportSize(Object count);

  /// No description provided for @prefsSupportCopy.
  ///
  /// In en, this message translates to:
  /// **'Copy bundle to clipboard'**
  String get prefsSupportCopy;

  /// No description provided for @prefsSupportCopied.
  ///
  /// In en, this message translates to:
  /// **'Support bundle copied to the clipboard.'**
  String get prefsSupportCopied;

  /// No description provided for @prefsSupportDownload.
  ///
  /// In en, this message translates to:
  /// **'Download support bundle'**
  String get prefsSupportDownload;

  /// No description provided for @prefsSupportDownloaded.
  ///
  /// In en, this message translates to:
  /// **'Support bundle downloaded.'**
  String get prefsSupportDownloaded;

  /// No description provided for @paramDropFiles.
  ///
  /// In en, this message translates to:
  /// **'Drop files here or click to add'**
  String get paramDropFiles;

  /// No description provided for @paramDropFolder.
  ///
  /// In en, this message translates to:
  /// **'Drop folder here or browse'**
  String get paramDropFolder;

  /// No description provided for @paramDropFile.
  ///
  /// In en, this message translates to:
  /// **'Drop file here or browse'**
  String get paramDropFile;

  /// No description provided for @actionRecent.
  ///
  /// In en, this message translates to:
  /// **'Recent selections'**
  String get actionRecent;

  /// No description provided for @actionToggleTheme.
  ///
  /// In en, this message translates to:
  /// **'Toggle theme (Light / Dark / System)'**
  String get actionToggleTheme;

  /// No description provided for @filterSearchPlaceholder.
  ///
  /// In en, this message translates to:
  /// **'Search (RFI no, status, description)...'**
  String get filterSearchPlaceholder;

  /// No description provided for @filterAll.
  ///
  /// In en, this message translates to:
  /// **'All'**
  String get filterAll;

  /// No description provided for @filterChanges.
  ///
  /// In en, this message translates to:
  /// **'Changes'**
  String get filterChanges;

  /// No description provided for @filterManualReview.
  ///
  /// In en, this message translates to:
  /// **'Manual Review'**
  String get filterManualReview;

  /// No description provided for @tableFilterCount.
  ///
  /// In en, this message translates to:
  /// **'Showing {filtered} of {total} rows'**
  String tableFilterCount(Object filtered, Object total);

  /// No description provided for @prefsReset.
  ///
  /// In en, this message translates to:
  /// **'Reset to defaults'**
  String get prefsReset;

  /// No description provided for @reviewHelp.
  ///
  /// In en, this message translates to:
  /// **'Decide what happens to each file. Nothing is changed until you approve the plan.'**
  String get reviewHelp;

  /// No description provided for @reviewEmptyHelp.
  ///
  /// In en, this message translates to:
  /// **'No files are waiting for review.'**
  String get reviewEmptyHelp;

  /// No description provided for @reviewNoChanges.
  ///
  /// In en, this message translates to:
  /// **'Nothing selected'**
  String get reviewNoChanges;

  /// No description provided for @reviewSelected.
  ///
  /// In en, this message translates to:
  /// **'{count} selected'**
  String reviewSelected(Object count);

  /// No description provided for @reviewToTrash.
  ///
  /// In en, this message translates to:
  /// **'{count} to trash'**
  String reviewToTrash(Object count);

  /// No description provided for @reviewTrashHint.
  ///
  /// In en, this message translates to:
  /// **'Deleted files go to recoverable trash, not permanently removed.'**
  String get reviewTrashHint;

  /// No description provided for @stepForm.
  ///
  /// In en, this message translates to:
  /// **'Configure'**
  String get stepForm;

  /// No description provided for @stepPreview.
  ///
  /// In en, this message translates to:
  /// **'Preview'**
  String get stepPreview;

  /// No description provided for @stepApply.
  ///
  /// In en, this message translates to:
  /// **'Apply'**
  String get stepApply;

  /// No description provided for @actionSave.
  ///
  /// In en, this message translates to:
  /// **'Save'**
  String get actionSave;

  /// No description provided for @authUsername.
  ///
  /// In en, this message translates to:
  /// **'Username'**
  String get authUsername;

  /// No description provided for @authUsernameHelp.
  ///
  /// In en, this message translates to:
  /// **'3-24 characters: letters, numbers, dot, dash, underscore'**
  String get authUsernameHelp;

  /// No description provided for @authIdentifier.
  ///
  /// In en, this message translates to:
  /// **'Username or email'**
  String get authIdentifier;

  /// No description provided for @authIdentifierHelp.
  ///
  /// In en, this message translates to:
  /// **'Either one works'**
  String get authIdentifierHelp;

  /// No description provided for @authEmailSignInHelp.
  ///
  /// In en, this message translates to:
  /// **'Username sign-in is unavailable. Use your account email address.'**
  String get authEmailSignInHelp;

  /// No description provided for @authSubtitleUp.
  ///
  /// In en, this message translates to:
  /// **'Create an account to get started'**
  String get authSubtitleUp;

  /// No description provided for @authErrCredentials.
  ///
  /// In en, this message translates to:
  /// **'Wrong username/email or password.'**
  String get authErrCredentials;

  /// No description provided for @authErrUsernameTaken.
  ///
  /// In en, this message translates to:
  /// **'That username is taken.'**
  String get authErrUsernameTaken;

  /// No description provided for @authErrUsernameInvalid.
  ///
  /// In en, this message translates to:
  /// **'Username must be 3-24 characters: letters, numbers, dot, dash, underscore.'**
  String get authErrUsernameInvalid;

  /// No description provided for @authErrEmailTaken.
  ///
  /// In en, this message translates to:
  /// **'An account with this email already exists.'**
  String get authErrEmailTaken;

  /// No description provided for @prefsAccount.
  ///
  /// In en, this message translates to:
  /// **'Account'**
  String get prefsAccount;

  /// No description provided for @prefsAccountEmailFixed.
  ///
  /// In en, this message translates to:
  /// **'cannot be changed'**
  String get prefsAccountEmailFixed;

  /// No description provided for @prefsAccountSaved.
  ///
  /// In en, this message translates to:
  /// **'Saved'**
  String get prefsAccountSaved;

  /// No description provided for @authErrEmailNotConfirmed.
  ///
  /// In en, this message translates to:
  /// **'This account exists but the email is not confirmed yet. The admin can turn off email confirmation, or open the link sent to your inbox.'**
  String get authErrEmailNotConfirmed;

  /// No description provided for @authErrRateLimit.
  ///
  /// In en, this message translates to:
  /// **'Too many attempts. Wait a few minutes and try again.'**
  String get authErrRateLimit;

  /// No description provided for @authErrWeakPassword.
  ///
  /// In en, this message translates to:
  /// **'Password is too weak. Use at least 6 characters.'**
  String get authErrWeakPassword;

  /// No description provided for @warnNoOcr.
  ///
  /// In en, this message translates to:
  /// **'OCR engine (Tesseract) not found — no files were touched. Install it, or reinstall the app bundle.'**
  String get warnNoOcr;

  /// No description provided for @noteFolderCreated.
  ///
  /// In en, this message translates to:
  /// **'New folder'**
  String get noteFolderCreated;

  /// No description provided for @warnOcrBroken.
  ///
  /// In en, this message translates to:
  /// **'OCR failed on every file — this is an installation problem, not a file problem. Nothing was touched. Details:'**
  String get warnOcrBroken;

  /// No description provided for @opFilesBatchPrintTitle.
  ///
  /// In en, this message translates to:
  /// **'Print multiple files'**
  String get opFilesBatchPrintTitle;

  /// No description provided for @opFilesBatchPrintDesc.
  ///
  /// In en, this message translates to:
  /// **'Builds one ordered native print queue from PDFs, Office documents and supported images.'**
  String get opFilesBatchPrintDesc;

  /// No description provided for @paramPrintFiles.
  ///
  /// In en, this message translates to:
  /// **'Files to print'**
  String get paramPrintFiles;

  /// No description provided for @paramPrintFilesHelp.
  ///
  /// In en, this message translates to:
  /// **'Add mixed file types, then reorder or remove them. The displayed order is the print order.'**
  String get paramPrintFilesHelp;

  /// No description provided for @paramPrintRfiRoot.
  ///
  /// In en, this message translates to:
  /// **'RFI System folder'**
  String get paramPrintRfiRoot;

  /// No description provided for @paramPrintRfiRootHelp.
  ///
  /// In en, this message translates to:
  /// **'Search this folder and its subfolders without changing any source files.'**
  String get paramPrintRfiRootHelp;

  /// No description provided for @paramPrintRfiNumbers.
  ///
  /// In en, this message translates to:
  /// **'RFI numbers'**
  String get paramPrintRfiNumbers;

  /// No description provided for @paramPrintRfiNumbersHelp.
  ///
  /// In en, this message translates to:
  /// **'Enter one full RFI code, discipline-number code such as CIV-1319, or a one-to-four-digit number per line; short numbers are padded to four digits.'**
  String get paramPrintRfiNumbersHelp;

  /// No description provided for @paramPrintIncludeFirstPage.
  ///
  /// In en, this message translates to:
  /// **'First Page documents'**
  String get paramPrintIncludeFirstPage;

  /// No description provided for @paramPrintIncludeSketch.
  ///
  /// In en, this message translates to:
  /// **'Sketch documents'**
  String get paramPrintIncludeSketch;

  /// No description provided for @printRfiTitle.
  ///
  /// In en, this message translates to:
  /// **'Find RFI documents'**
  String get printRfiTitle;

  /// No description provided for @printRfiInputs.
  ///
  /// In en, this message translates to:
  /// **'Include these RFI requests'**
  String get printRfiInputs;

  /// No description provided for @printRfiRootRequired.
  ///
  /// In en, this message translates to:
  /// **'Select an RFI System folder first.'**
  String get printRfiRootRequired;

  /// No description provided for @printRfiInputRequired.
  ///
  /// In en, this message translates to:
  /// **'Select at least one RFI request before searching.'**
  String get printRfiInputRequired;

  /// No description provided for @printRfiTypesRequired.
  ///
  /// In en, this message translates to:
  /// **'Select First Page, Sketch, or both before searching.'**
  String get printRfiTypesRequired;

  /// No description provided for @printRfiSearch.
  ///
  /// In en, this message translates to:
  /// **'Search RFI System'**
  String get printRfiSearch;

  /// No description provided for @printRfiSearching.
  ///
  /// In en, this message translates to:
  /// **'Searching…'**
  String get printRfiSearching;

  /// No description provided for @printRfiFound.
  ///
  /// In en, this message translates to:
  /// **'{count} matching documents'**
  String printRfiFound(Object count);

  /// No description provided for @printRfiAmbiguous.
  ///
  /// In en, this message translates to:
  /// **'This number exists in multiple disciplines. Choose the intended files explicitly.'**
  String get printRfiAmbiguous;

  /// No description provided for @printRfiMissingFirstPage.
  ///
  /// In en, this message translates to:
  /// **'First Page document not found.'**
  String get printRfiMissingFirstPage;

  /// No description provided for @printRfiMissingSketch.
  ///
  /// In en, this message translates to:
  /// **'Sketch document not found.'**
  String get printRfiMissingSketch;

  /// No description provided for @printRfiToggleRequest.
  ///
  /// In en, this message translates to:
  /// **'Toggle this RFI'**
  String get printRfiToggleRequest;

  /// No description provided for @printRfiSelectTypeFirstPage.
  ///
  /// In en, this message translates to:
  /// **'Select First Pages'**
  String get printRfiSelectTypeFirstPage;

  /// No description provided for @printRfiSelectTypeSketch.
  ///
  /// In en, this message translates to:
  /// **'Select Sketches'**
  String get printRfiSelectTypeSketch;

  /// No description provided for @printRfiClearTypeFirstPage.
  ///
  /// In en, this message translates to:
  /// **'Clear First Pages'**
  String get printRfiClearTypeFirstPage;

  /// No description provided for @printRfiSelectDiscipline.
  ///
  /// In en, this message translates to:
  /// **'Select {discipline}'**
  String printRfiSelectDiscipline(Object discipline);

  /// No description provided for @printRfiClearTypeSketch.
  ///
  /// In en, this message translates to:
  /// **'Clear Sketches'**
  String get printRfiClearTypeSketch;

  /// No description provided for @printRfiTypeFirstPage.
  ///
  /// In en, this message translates to:
  /// **'First Page'**
  String get printRfiTypeFirstPage;

  /// No description provided for @printRfiTypeSketch.
  ///
  /// In en, this message translates to:
  /// **'Sketch'**
  String get printRfiTypeSketch;

  /// No description provided for @printRfiNoMatches.
  ///
  /// In en, this message translates to:
  /// **'No matching documents were found.'**
  String get printRfiNoMatches;

  /// No description provided for @printRfiTruncated.
  ///
  /// In en, this message translates to:
  /// **'Search reached its safe scan limit. Some folders may not have been searched.'**
  String get printRfiTruncated;

  /// No description provided for @actionSelectAll.
  ///
  /// In en, this message translates to:
  /// **'Select all'**
  String get actionSelectAll;

  /// No description provided for @actionSelectNone.
  ///
  /// In en, this message translates to:
  /// **'Select none'**
  String get actionSelectNone;

  /// No description provided for @errorPrintRfiInvalidRequest.
  ///
  /// In en, this message translates to:
  /// **'Check the RFI requests and selected document types.'**
  String get errorPrintRfiInvalidRequest;

  /// No description provided for @errorPrintRfiRootEscape.
  ///
  /// In en, this message translates to:
  /// **'A discovered file changed outside the selected RFI System. Search again.'**
  String get errorPrintRfiRootEscape;

  /// No description provided for @errorPrintRfiChanged.
  ///
  /// In en, this message translates to:
  /// **'A discovered file changed during search. Search again.'**
  String get errorPrintRfiChanged;

  /// No description provided for @paramPrintPrinter.
  ///
  /// In en, this message translates to:
  /// **'Printer'**
  String get paramPrintPrinter;

  /// No description provided for @paramPrintPrinterHelp.
  ///
  /// In en, this message translates to:
  /// **'Uses printers currently installed on this computer.'**
  String get paramPrintPrinterHelp;

  /// No description provided for @paramPrintCopies.
  ///
  /// In en, this message translates to:
  /// **'Copies'**
  String get paramPrintCopies;

  /// No description provided for @paramPrintCopiesHelp.
  ///
  /// In en, this message translates to:
  /// **'From 1 to 99 copies of the queue.'**
  String get paramPrintCopiesHelp;

  /// No description provided for @paramPrintCollate.
  ///
  /// In en, this message translates to:
  /// **'Collate copies'**
  String get paramPrintCollate;

  /// No description provided for @paramPrintCollateHelp.
  ///
  /// In en, this message translates to:
  /// **'Print the complete ordered set before starting the next copy.'**
  String get paramPrintCollateHelp;

  /// No description provided for @errorParamRequiredFiles.
  ///
  /// In en, this message translates to:
  /// **'Select at least one file'**
  String get errorParamRequiredFiles;

  /// No description provided for @errorParamRequiredPrinter.
  ///
  /// In en, this message translates to:
  /// **'Select a printer'**
  String get errorParamRequiredPrinter;

  /// No description provided for @errorParamRequiredCopies.
  ///
  /// In en, this message translates to:
  /// **'Number of copies is required'**
  String get errorParamRequiredCopies;

  /// No description provided for @actionAddFiles.
  ///
  /// In en, this message translates to:
  /// **'Add files'**
  String get actionAddFiles;

  /// No description provided for @actionClear.
  ///
  /// In en, this message translates to:
  /// **'Clear'**
  String get actionClear;

  /// No description provided for @actionMoveUp.
  ///
  /// In en, this message translates to:
  /// **'Move up'**
  String get actionMoveUp;

  /// No description provided for @actionMoveDown.
  ///
  /// In en, this message translates to:
  /// **'Move down'**
  String get actionMoveDown;

  /// No description provided for @actionRemove.
  ///
  /// In en, this message translates to:
  /// **'Remove'**
  String get actionRemove;

  /// No description provided for @actionDecrease.
  ///
  /// In en, this message translates to:
  /// **'Decrease'**
  String get actionDecrease;

  /// No description provided for @actionIncrease.
  ///
  /// In en, this message translates to:
  /// **'Increase'**
  String get actionIncrease;

  /// No description provided for @paramHintFiles.
  ///
  /// In en, this message translates to:
  /// **'No files selected'**
  String get paramHintFiles;

  /// No description provided for @printFilesSelected.
  ///
  /// In en, this message translates to:
  /// **'{count} files selected'**
  String printFilesSelected(Object count);

  /// No description provided for @notePrintQueued.
  ///
  /// In en, this message translates to:
  /// **'Ready to print'**
  String get notePrintQueued;

  /// No description provided for @warnPrintNoPrinters.
  ///
  /// In en, this message translates to:
  /// **'No printers were found'**
  String get warnPrintNoPrinters;

  /// No description provided for @warnPrintPrinterUnavailable.
  ///
  /// In en, this message translates to:
  /// **'The selected printer is no longer available'**
  String get warnPrintPrinterUnavailable;

  /// No description provided for @warnPrintInvalidCopies.
  ///
  /// In en, this message translates to:
  /// **'Copies must be between 1 and 99'**
  String get warnPrintInvalidCopies;

  /// No description provided for @warnPrintTooManyFiles.
  ///
  /// In en, this message translates to:
  /// **'A queue can contain at most 250 files'**
  String get warnPrintTooManyFiles;

  /// No description provided for @warnPrintFileMissing.
  ///
  /// In en, this message translates to:
  /// **'File is missing and will be skipped'**
  String get warnPrintFileMissing;

  /// No description provided for @warnPrintWorkbookDiscoveryFailed.
  ///
  /// In en, this message translates to:
  /// **'The workbook could not be inspected safely and was not added to the print order'**
  String get warnPrintWorkbookDiscoveryFailed;

  /// No description provided for @warnPrintWorkbookTooManyWorksheets.
  ///
  /// In en, this message translates to:
  /// **'The workbook contains too many worksheets'**
  String get warnPrintWorkbookTooManyWorksheets;

  /// No description provided for @warnPrintExpandedOrderTooLarge.
  ///
  /// In en, this message translates to:
  /// **'The expanded print order can contain at most 250 jobs'**
  String get warnPrintExpandedOrderTooLarge;

  /// No description provided for @warnPrintWorkbookUnsafeXlm.
  ///
  /// In en, this message translates to:
  /// **'The workbook contains an Excel 4.0 macro sheet and was rejected'**
  String get warnPrintWorkbookUnsafeXlm;

  /// No description provided for @warnPrintWorkbookUnsafeProtection.
  ///
  /// In en, this message translates to:
  /// **'The workbook structure is protected and was rejected'**
  String get warnPrintWorkbookUnsafeProtection;

  /// No description provided for @warnPrintWorkbookUnsafeExternalLinks.
  ///
  /// In en, this message translates to:
  /// **'The workbook contains external links and was rejected'**
  String get warnPrintWorkbookUnsafeExternalLinks;

  /// No description provided for @warnPrintWorkbookUnsafeConnections.
  ///
  /// In en, this message translates to:
  /// **'The workbook contains external data connections and was rejected'**
  String get warnPrintWorkbookUnsafeConnections;

  /// No description provided for @warnPrintWorkbookUnsafeQueryTables.
  ///
  /// In en, this message translates to:
  /// **'The workbook contains query tables and was rejected'**
  String get warnPrintWorkbookUnsafeQueryTables;

  /// No description provided for @warnPrintNoHandler.
  ///
  /// In en, this message translates to:
  /// **'No native print handler is available for this file type; install or choose a default app that can print it'**
  String get warnPrintNoHandler;

  /// No description provided for @warnPrintUnsupportedOptions.
  ///
  /// In en, this message translates to:
  /// **'The selected printer or native application does not support these print options'**
  String get warnPrintUnsupportedOptions;

  /// No description provided for @warnPrintNonUndoable.
  ///
  /// In en, this message translates to:
  /// **'Printing cannot be undone. Files run strictly one at a time; if an app asks for confirmation, the queue waits before starting the next file.'**
  String get warnPrintNonUndoable;

  /// No description provided for @errorPrintInvalidPlan.
  ///
  /// In en, this message translates to:
  /// **'The print plan is invalid. Preview the queue again.'**
  String get errorPrintInvalidPlan;

  /// No description provided for @errorPrintFailed.
  ///
  /// In en, this message translates to:
  /// **'Print submission failed'**
  String get errorPrintFailed;

  /// No description provided for @errorPrintQueueStopped.
  ///
  /// In en, this message translates to:
  /// **'Strict print order was protected; remaining files were not submitted'**
  String get errorPrintQueueStopped;

  /// No description provided for @errorPrintPageRangeInvalid.
  ///
  /// In en, this message translates to:
  /// **'Enter all or a comma-separated list of pages and ranges, such as 1-3,5.'**
  String get errorPrintPageRangeInvalid;

  /// No description provided for @errorPrintPageRangeBounds.
  ///
  /// In en, this message translates to:
  /// **'The selected page range exceeds this PDF\'s page count.'**
  String get errorPrintPageRangeBounds;

  /// No description provided for @errorPrintPageRangeImageUnsupported.
  ///
  /// In en, this message translates to:
  /// **'Images accept only all or page 1.'**
  String get errorPrintPageRangeImageUnsupported;

  /// No description provided for @errorPrintPageRangeNativeUnsupported.
  ///
  /// In en, this message translates to:
  /// **'Files printed by a native application accept only all pages.'**
  String get errorPrintPageRangeNativeUnsupported;

  /// No description provided for @errorPrintWorksheetPageRangeAllOnly.
  ///
  /// In en, this message translates to:
  /// **'Selected worksheets accept only all pages.'**
  String get errorPrintWorksheetPageRangeAllOnly;

  /// No description provided for @errorPrintWorkbookChanged.
  ///
  /// In en, this message translates to:
  /// **'The workbook changed after preview. Preview the print order again.'**
  String get errorPrintWorkbookChanged;

  /// No description provided for @errorPrintWorksheetChanged.
  ///
  /// In en, this message translates to:
  /// **'The worksheet structure changed after preview. Preview the print order again.'**
  String get errorPrintWorksheetChanged;

  /// No description provided for @errorPrintExcelUnavailable.
  ///
  /// In en, this message translates to:
  /// **'Excel worksheet export is not available on this computer.'**
  String get errorPrintExcelUnavailable;

  /// No description provided for @errorPrintExcelExportTimedOut.
  ///
  /// In en, this message translates to:
  /// **'Excel worksheet export timed out before printing.'**
  String get errorPrintExcelExportTimedOut;

  /// No description provided for @errorPrintExcelExportCancelled.
  ///
  /// In en, this message translates to:
  /// **'Excel worksheet export was cancelled before printing.'**
  String get errorPrintExcelExportCancelled;

  /// No description provided for @errorPrintExcelExportQuarantined.
  ///
  /// In en, this message translates to:
  /// **'Excel export is disabled until DocFlow restarts because the previous Excel worker could not be closed safely.'**
  String get errorPrintExcelExportQuarantined;

  /// No description provided for @errorPrintExportCleanupRequiresReview.
  ///
  /// In en, this message translates to:
  /// **'Private Excel export cleanup could not be confirmed. Restart DocFlow and review the temporary export state before retrying.'**
  String get errorPrintExportCleanupRequiresReview;

  /// No description provided for @errorPrintExcelSourceMotwBlocked.
  ///
  /// In en, this message translates to:
  /// **'The workbook is blocked because its Windows download-zone metadata is unsafe, malformed, oversized, or unreadable.'**
  String get errorPrintExcelSourceMotwBlocked;

  /// No description provided for @errorPrintExcelWorkbookUnsafe.
  ///
  /// In en, this message translates to:
  /// **'The workbook requires unsafe protection, link, connection, query, or Excel 4.0 macro handling and was not opened.'**
  String get errorPrintExcelWorkbookUnsafe;

  /// No description provided for @errorPrintExcelComContractInvalid.
  ///
  /// In en, this message translates to:
  /// **'The approved Excel automation request is invalid and was not opened.'**
  String get errorPrintExcelComContractInvalid;

  /// No description provided for @errorPrintExcelComFailed.
  ///
  /// In en, this message translates to:
  /// **'Excel automation failed before printing.'**
  String get errorPrintExcelComFailed;

  /// No description provided for @errorPrintExcelExportDestinationExists.
  ///
  /// In en, this message translates to:
  /// **'The private worksheet PDF destination already exists, so export was stopped.'**
  String get errorPrintExcelExportDestinationExists;

  /// No description provided for @errorPrintExcelExportFailed.
  ///
  /// In en, this message translates to:
  /// **'Excel could not export the selected worksheet to PDF.'**
  String get errorPrintExcelExportFailed;

  /// No description provided for @errorPrintExcelExportMalformedPdf.
  ///
  /// In en, this message translates to:
  /// **'Excel produced an empty or invalid worksheet PDF, so nothing was printed.'**
  String get errorPrintExcelExportMalformedPdf;

  /// No description provided for @errorPrintExcelPrinterContextUnavailable.
  ///
  /// In en, this message translates to:
  /// **'Windows could not create the approved printer context for the Excel worksheet.'**
  String get errorPrintExcelPrinterContextUnavailable;

  /// No description provided for @errorPrintExcelPrintJobRejected.
  ///
  /// In en, this message translates to:
  /// **'Windows rejected the controlled Excel worksheet print job before submission began.'**
  String get errorPrintExcelPrintJobRejected;

  /// No description provided for @errorPrintExcelPrintPreparationFailed.
  ///
  /// In en, this message translates to:
  /// **'Windows could not prepare the Excel worksheet for controlled printing.'**
  String get errorPrintExcelPrintPreparationFailed;

  /// No description provided for @errorPrintExcelPrintOptionsRejected.
  ///
  /// In en, this message translates to:
  /// **'The Windows printer rejected the approved Excel worksheet print settings.'**
  String get errorPrintExcelPrintOptionsRejected;

  /// No description provided for @errorPrintExcelPrintCancelledUnconfirmed.
  ///
  /// In en, this message translates to:
  /// **'Cancellation could not be confirmed after the Excel worksheet print job entered the spooler. Check the printer before retrying.'**
  String get errorPrintExcelPrintCancelledUnconfirmed;

  /// No description provided for @errorPrintExcelPrintCompletionUnconfirmed.
  ///
  /// In en, this message translates to:
  /// **'Windows could not confirm completion after the Excel worksheet print job entered the spooler. Check the printer before retrying.'**
  String get errorPrintExcelPrintCompletionUnconfirmed;

  /// No description provided for @errorPrintExcelControlledPdfUnavailable.
  ///
  /// In en, this message translates to:
  /// **'Controlled PDF printing for Excel worksheets is not available in this build.'**
  String get errorPrintExcelControlledPdfUnavailable;

  /// No description provided for @errorPrintExcelWorkerStartFailed.
  ///
  /// In en, this message translates to:
  /// **'The isolated Excel export worker could not start.'**
  String get errorPrintExcelWorkerStartFailed;

  /// No description provided for @errorPrintExcelDeadlineInvalid.
  ///
  /// In en, this message translates to:
  /// **'The Excel export timeout is invalid.'**
  String get errorPrintExcelDeadlineInvalid;

  /// No description provided for @errorPrintExcelWorksheetMetadataMissing.
  ///
  /// In en, this message translates to:
  /// **'The approved worksheet metadata is missing. Preview the print order again.'**
  String get errorPrintExcelWorksheetMetadataMissing;

  /// No description provided for @errorPrintExcelSafetyMetadataMissing.
  ///
  /// In en, this message translates to:
  /// **'The approved workbook safety metadata is missing. Preview the print order again.'**
  String get errorPrintExcelSafetyMetadataMissing;

  /// No description provided for @errorPrintExcelGenerationExhausted.
  ///
  /// In en, this message translates to:
  /// **'Excel export cannot continue safely until DocFlow restarts.'**
  String get errorPrintExcelGenerationExhausted;

  /// No description provided for @errorPrintExcelWorkerStopped.
  ///
  /// In en, this message translates to:
  /// **'The isolated Excel export worker stopped unexpectedly.'**
  String get errorPrintExcelWorkerStopped;

  /// No description provided for @errorPrintExcelResponseStale.
  ///
  /// In en, this message translates to:
  /// **'A stale Excel export response was rejected before printing.'**
  String get errorPrintExcelResponseStale;

  /// No description provided for @errorPrintExcelStagingUnavailable.
  ///
  /// In en, this message translates to:
  /// **'Private workbook staging is unavailable.'**
  String get errorPrintExcelStagingUnavailable;

  /// No description provided for @errorPrintExcelSourceNameUnavailable.
  ///
  /// In en, this message translates to:
  /// **'The approved workbook file name is unavailable.'**
  String get errorPrintExcelSourceNameUnavailable;

  /// No description provided for @errorPrintExcelStagingFailed.
  ///
  /// In en, this message translates to:
  /// **'The workbook could not be copied into private staging.'**
  String get errorPrintExcelStagingFailed;

  /// No description provided for @errorPrintExcelStagingVerificationFailed.
  ///
  /// In en, this message translates to:
  /// **'The private workbook copy could not be verified against the approved source.'**
  String get errorPrintExcelStagingVerificationFailed;

  /// No description provided for @errorPrintExcelPrivatePdfUnavailable.
  ///
  /// In en, this message translates to:
  /// **'The private worksheet PDF is unavailable.'**
  String get errorPrintExcelPrivatePdfUnavailable;

  /// No description provided for @errorPrintExcelPrivatePdfEmpty.
  ///
  /// In en, this message translates to:
  /// **'Excel produced an empty worksheet PDF, so nothing was printed.'**
  String get errorPrintExcelPrivatePdfEmpty;

  /// No description provided for @printWaitingForCurrent.
  ///
  /// In en, this message translates to:
  /// **'Waiting for this file to finish before starting the next one'**
  String get printWaitingForCurrent;

  /// No description provided for @paramPrintPaper.
  ///
  /// In en, this message translates to:
  /// **'Paper size'**
  String get paramPrintPaper;

  /// No description provided for @paramPrintPaperHelp.
  ///
  /// In en, this message translates to:
  /// **'Default for all files; you can override it per file in Preview.'**
  String get paramPrintPaperHelp;

  /// No description provided for @paramPrintOrientation.
  ///
  /// In en, this message translates to:
  /// **'Orientation'**
  String get paramPrintOrientation;

  /// No description provided for @paramPrintScaling.
  ///
  /// In en, this message translates to:
  /// **'Page scaling'**
  String get paramPrintScaling;

  /// No description provided for @paramPrintColor.
  ///
  /// In en, this message translates to:
  /// **'Color mode'**
  String get paramPrintColor;

  /// No description provided for @paramPrintDuplex.
  ///
  /// In en, this message translates to:
  /// **'Two-sided printing'**
  String get paramPrintDuplex;

  /// No description provided for @paramPrintRetries.
  ///
  /// In en, this message translates to:
  /// **'Automatic retries'**
  String get paramPrintRetries;

  /// No description provided for @paramPrintRetriesHelp.
  ///
  /// In en, this message translates to:
  /// **'Retries a failed job 0 to 3 times before applying the queue error rule.'**
  String get paramPrintRetriesHelp;

  /// No description provided for @paramPrintOnError.
  ///
  /// In en, this message translates to:
  /// **'If a job still fails'**
  String get paramPrintOnError;

  /// No description provided for @paramPrintOnErrorHelp.
  ///
  /// In en, this message translates to:
  /// **'Pause protects order; Skip continues and leaves the failed job available for retry.'**
  String get paramPrintOnErrorHelp;

  /// No description provided for @paramPrintTimeout.
  ///
  /// In en, this message translates to:
  /// **'Job timeout (minutes)'**
  String get paramPrintTimeout;

  /// No description provided for @paramPrintTimeoutHelp.
  ///
  /// In en, this message translates to:
  /// **'Stops waiting when a printer remains blocked, offline or unresponsive.'**
  String get paramPrintTimeoutHelp;

  /// No description provided for @choicePrintPaperAuto.
  ///
  /// In en, this message translates to:
  /// **'Printer default'**
  String get choicePrintPaperAuto;

  /// No description provided for @choicePrintPaperA4.
  ///
  /// In en, this message translates to:
  /// **'A4'**
  String get choicePrintPaperA4;

  /// No description provided for @choicePrintPaperA3.
  ///
  /// In en, this message translates to:
  /// **'A3'**
  String get choicePrintPaperA3;

  /// No description provided for @choicePrintPaperA5.
  ///
  /// In en, this message translates to:
  /// **'A5'**
  String get choicePrintPaperA5;

  /// No description provided for @choicePrintPaperLetter.
  ///
  /// In en, this message translates to:
  /// **'Letter'**
  String get choicePrintPaperLetter;

  /// No description provided for @choicePrintPaperLegal.
  ///
  /// In en, this message translates to:
  /// **'Legal'**
  String get choicePrintPaperLegal;

  /// No description provided for @choicePrintOrientationAuto.
  ///
  /// In en, this message translates to:
  /// **'Auto from page'**
  String get choicePrintOrientationAuto;

  /// No description provided for @choicePrintOrientationPortrait.
  ///
  /// In en, this message translates to:
  /// **'Portrait'**
  String get choicePrintOrientationPortrait;

  /// No description provided for @choicePrintOrientationLandscape.
  ///
  /// In en, this message translates to:
  /// **'Landscape'**
  String get choicePrintOrientationLandscape;

  /// No description provided for @choicePrintScalingFit.
  ///
  /// In en, this message translates to:
  /// **'Fit to printable area'**
  String get choicePrintScalingFit;

  /// No description provided for @choicePrintScalingActual.
  ///
  /// In en, this message translates to:
  /// **'Actual size'**
  String get choicePrintScalingActual;

  /// No description provided for @choicePrintScalingShrink.
  ///
  /// In en, this message translates to:
  /// **'Shrink oversized pages only'**
  String get choicePrintScalingShrink;

  /// No description provided for @choicePrintColorPrinter.
  ///
  /// In en, this message translates to:
  /// **'Printer default'**
  String get choicePrintColorPrinter;

  /// No description provided for @choicePrintColorColor.
  ///
  /// In en, this message translates to:
  /// **'Color'**
  String get choicePrintColorColor;

  /// No description provided for @choicePrintColorGrayscale.
  ///
  /// In en, this message translates to:
  /// **'Grayscale'**
  String get choicePrintColorGrayscale;

  /// No description provided for @choicePrintDuplexPrinter.
  ///
  /// In en, this message translates to:
  /// **'Printer default'**
  String get choicePrintDuplexPrinter;

  /// No description provided for @choicePrintDuplexSimplex.
  ///
  /// In en, this message translates to:
  /// **'One-sided'**
  String get choicePrintDuplexSimplex;

  /// No description provided for @choicePrintDuplexLong.
  ///
  /// In en, this message translates to:
  /// **'Two-sided, long edge'**
  String get choicePrintDuplexLong;

  /// No description provided for @choicePrintDuplexShort.
  ///
  /// In en, this message translates to:
  /// **'Two-sided, short edge'**
  String get choicePrintDuplexShort;

  /// No description provided for @choicePrintOnErrorPause.
  ///
  /// In en, this message translates to:
  /// **'Pause queue'**
  String get choicePrintOnErrorPause;

  /// No description provided for @choicePrintOnErrorSkip.
  ///
  /// In en, this message translates to:
  /// **'Skip and continue'**
  String get choicePrintOnErrorSkip;

  /// No description provided for @printEditorTitle.
  ///
  /// In en, this message translates to:
  /// **'Review and customize the print queue'**
  String get printEditorTitle;

  /// No description provided for @printEditorRowCount.
  ///
  /// In en, this message translates to:
  /// **'{count} files/jobs'**
  String printEditorRowCount(Object count);

  /// No description provided for @printColInclude.
  ///
  /// In en, this message translates to:
  /// **'Print'**
  String get printColInclude;

  /// No description provided for @printColStatus.
  ///
  /// In en, this message translates to:
  /// **'Status'**
  String get printColStatus;

  /// No description provided for @printColFile.
  ///
  /// In en, this message translates to:
  /// **'File'**
  String get printColFile;

  /// No description provided for @printColFormat.
  ///
  /// In en, this message translates to:
  /// **'Format'**
  String get printColFormat;

  /// No description provided for @printColWorksheet.
  ///
  /// In en, this message translates to:
  /// **'Worksheet'**
  String get printColWorksheet;

  /// No description provided for @printColWorksheetVisibility.
  ///
  /// In en, this message translates to:
  /// **'Visibility'**
  String get printColWorksheetVisibility;

  /// No description provided for @printWorksheetVisibilityVisible.
  ///
  /// In en, this message translates to:
  /// **'Visible'**
  String get printWorksheetVisibilityVisible;

  /// No description provided for @printWorksheetVisibilityHidden.
  ///
  /// In en, this message translates to:
  /// **'Hidden'**
  String get printWorksheetVisibilityHidden;

  /// No description provided for @printWorksheetVisibilityVeryHidden.
  ///
  /// In en, this message translates to:
  /// **'Very hidden'**
  String get printWorksheetVisibilityVeryHidden;

  /// No description provided for @printColPrinter.
  ///
  /// In en, this message translates to:
  /// **'Printer'**
  String get printColPrinter;

  /// No description provided for @printColPaper.
  ///
  /// In en, this message translates to:
  /// **'Paper'**
  String get printColPaper;

  /// No description provided for @printColOrientation.
  ///
  /// In en, this message translates to:
  /// **'Orientation'**
  String get printColOrientation;

  /// No description provided for @printColScaling.
  ///
  /// In en, this message translates to:
  /// **'Scaling'**
  String get printColScaling;

  /// No description provided for @printColColor.
  ///
  /// In en, this message translates to:
  /// **'Color'**
  String get printColColor;

  /// No description provided for @printColDuplex.
  ///
  /// In en, this message translates to:
  /// **'Duplex'**
  String get printColDuplex;

  /// No description provided for @printColCopies.
  ///
  /// In en, this message translates to:
  /// **'Copies'**
  String get printColCopies;

  /// No description provided for @printColPageRange.
  ///
  /// In en, this message translates to:
  /// **'Page range'**
  String get printColPageRange;

  /// No description provided for @printColPageRangeHelp.
  ///
  /// In en, this message translates to:
  /// **'Use all, a page, or comma-separated ranges. Images accept all or 1; native-application files accept only all.'**
  String get printColPageRangeHelp;

  /// No description provided for @printStatusReady.
  ///
  /// In en, this message translates to:
  /// **'Ready'**
  String get printStatusReady;

  /// No description provided for @printStatusPending.
  ///
  /// In en, this message translates to:
  /// **'Pending'**
  String get printStatusPending;

  /// No description provided for @printStatusPrinting.
  ///
  /// In en, this message translates to:
  /// **'Printing'**
  String get printStatusPrinting;

  /// No description provided for @printStatusCompleted.
  ///
  /// In en, this message translates to:
  /// **'Completed'**
  String get printStatusCompleted;

  /// No description provided for @printStatusCompletedUnconfirmed.
  ///
  /// In en, this message translates to:
  /// **'Sent; receipt unavailable'**
  String get printStatusCompletedUnconfirmed;

  /// No description provided for @printStatusFailed.
  ///
  /// In en, this message translates to:
  /// **'Failed'**
  String get printStatusFailed;

  /// No description provided for @printStatusCancelled.
  ///
  /// In en, this message translates to:
  /// **'Cancelled'**
  String get printStatusCancelled;

  /// No description provided for @printStatusUncertain.
  ///
  /// In en, this message translates to:
  /// **'Check printer'**
  String get printStatusUncertain;

  /// No description provided for @warnPrintSettingsLimited.
  ///
  /// In en, this message translates to:
  /// **'This format is printed by its native application; paper and layout support depends on that application'**
  String get warnPrintSettingsLimited;

  /// No description provided for @warnPrintNoJobsSelected.
  ///
  /// In en, this message translates to:
  /// **'No print rows are selected'**
  String get warnPrintNoJobsSelected;

  /// No description provided for @warnPrintInvalidRetries.
  ///
  /// In en, this message translates to:
  /// **'Automatic retries must be between 0 and 3'**
  String get warnPrintInvalidRetries;

  /// No description provided for @warnPrintInvalidTimeout.
  ///
  /// In en, this message translates to:
  /// **'Job timeout must be between 1 and 240 minutes'**
  String get warnPrintInvalidTimeout;

  /// No description provided for @warnPrintResumingQueue.
  ///
  /// In en, this message translates to:
  /// **'Reviewing an unfinished saved print queue'**
  String get warnPrintResumingQueue;

  /// No description provided for @warnPrintCancelled.
  ///
  /// In en, this message translates to:
  /// **'The print queue was paused by the user'**
  String get warnPrintCancelled;

  /// No description provided for @warnPrintCancelledUncertain.
  ///
  /// In en, this message translates to:
  /// **'The system had not identified the current spool job when cancellation arrived. Check the printer before selecting it again.'**
  String get warnPrintCancelledUncertain;

  /// No description provided for @warnPrintStateUncertain.
  ///
  /// In en, this message translates to:
  /// **'The system could not confirm whether this job printed. It was not retried; check the output before selecting it again.'**
  String get warnPrintStateUncertain;

  /// No description provided for @warnPrintSubmittedUnconfirmed.
  ///
  /// In en, this message translates to:
  /// **'Windows accepted this job but did not retain a final receipt. It was not retried, and the queue continued to the next file.'**
  String get warnPrintSubmittedUnconfirmed;

  /// No description provided for @errorPrintQueuePaused.
  ///
  /// In en, this message translates to:
  /// **'The queue was paused; remaining jobs were kept for Resume'**
  String get errorPrintQueuePaused;

  /// No description provided for @errorPrintQueueSourceReselectionRequired.
  ///
  /// In en, this message translates to:
  /// **'This older print queue cannot be resumed safely. Discard it, then reselect the files.'**
  String get errorPrintQueueSourceReselectionRequired;

  /// No description provided for @printResumeTitle.
  ///
  /// In en, this message translates to:
  /// **'Unfinished print queue found'**
  String get printResumeTitle;

  /// No description provided for @printResumeSummary.
  ///
  /// In en, this message translates to:
  /// **'{count} jobs can be reviewed or resumed'**
  String printResumeSummary(Object count);

  /// No description provided for @printResumeReview.
  ///
  /// In en, this message translates to:
  /// **'Retry unprinted only'**
  String get printResumeReview;

  /// No description provided for @printResumeDiscard.
  ///
  /// In en, this message translates to:
  /// **'Discard queue'**
  String get printResumeDiscard;

  /// No description provided for @paramsPreviousTitle.
  ///
  /// In en, this message translates to:
  /// **'Previous print settings available'**
  String get paramsPreviousTitle;

  /// No description provided for @paramsPreviousHelp.
  ///
  /// In en, this message translates to:
  /// **'Files are not retained for safety. Restore the previous printer, paper, orientation, color, and queue settings only when you choose.'**
  String get paramsPreviousHelp;

  /// No description provided for @paramsPreviousApply.
  ///
  /// In en, this message translates to:
  /// **'Use previous settings'**
  String get paramsPreviousApply;

  /// No description provided for @printResumeIncomplete.
  ///
  /// In en, this message translates to:
  /// **'The queue is safely paused'**
  String get printResumeIncomplete;

  /// No description provided for @printCancelQueue.
  ///
  /// In en, this message translates to:
  /// **'Pause queue'**
  String get printCancelQueue;

  /// No description provided for @printCancelling.
  ///
  /// In en, this message translates to:
  /// **'Pausing…'**
  String get printCancelling;

  /// No description provided for @editorActionShowDetails.
  ///
  /// In en, this message translates to:
  /// **'Show automatic fields'**
  String get editorActionShowDetails;

  /// No description provided for @editorActionHideDetails.
  ///
  /// In en, this message translates to:
  /// **'Hide automatic fields'**
  String get editorActionHideDetails;

  /// No description provided for @layoutDataResetHeight.
  ///
  /// In en, this message translates to:
  /// **'Reset height'**
  String get layoutDataResetHeight;

  /// No description provided for @layoutDataFit.
  ///
  /// In en, this message translates to:
  /// **'Fit available space'**
  String get layoutDataFit;

  /// No description provided for @layoutDataResizeHeight.
  ///
  /// In en, this message translates to:
  /// **'Resize {label} height'**
  String layoutDataResizeHeight(Object label);

  /// No description provided for @layoutColumnResize.
  ///
  /// In en, this message translates to:
  /// **'Resize {column} column'**
  String layoutColumnResize(Object column);

  /// No description provided for @layoutColumnWidth.
  ///
  /// In en, this message translates to:
  /// **'{width} pixels wide'**
  String layoutColumnWidth(Object width);

  /// No description provided for @editorBulkTitle.
  ///
  /// In en, this message translates to:
  /// **'Bulk edit'**
  String get editorBulkTitle;

  /// No description provided for @editorSelectedCount.
  ///
  /// In en, this message translates to:
  /// **'{count} selected'**
  String editorSelectedCount(Object count);

  /// No description provided for @editorSelectAll.
  ///
  /// In en, this message translates to:
  /// **'Select all'**
  String get editorSelectAll;

  /// No description provided for @editorSelectNone.
  ///
  /// In en, this message translates to:
  /// **'Clear selection'**
  String get editorSelectNone;

  /// No description provided for @editorSelectRow.
  ///
  /// In en, this message translates to:
  /// **'Select row {row}'**
  String editorSelectRow(Object row);

  /// No description provided for @editorSelectGroup.
  ///
  /// In en, this message translates to:
  /// **'Select this group'**
  String get editorSelectGroup;

  /// No description provided for @editorBulkField.
  ///
  /// In en, this message translates to:
  /// **'Field to change'**
  String get editorBulkField;

  /// No description provided for @editorBulkValue.
  ///
  /// In en, this message translates to:
  /// **'New value'**
  String get editorBulkValue;

  /// No description provided for @editorBulkChoose.
  ///
  /// In en, this message translates to:
  /// **'Choose a value'**
  String get editorBulkChoose;

  /// No description provided for @editorBulkApply.
  ///
  /// In en, this message translates to:
  /// **'Apply to selected'**
  String get editorBulkApply;

  /// No description provided for @editorBoolTrue.
  ///
  /// In en, this message translates to:
  /// **'Yes'**
  String get editorBoolTrue;

  /// No description provided for @editorBoolFalse.
  ///
  /// In en, this message translates to:
  /// **'No'**
  String get editorBoolFalse;

  /// No description provided for @operationCancel.
  ///
  /// In en, this message translates to:
  /// **'Stop after current item'**
  String get operationCancel;

  /// No description provided for @operationCancelling.
  ///
  /// In en, this message translates to:
  /// **'Stopping…'**
  String get operationCancelling;

  /// No description provided for @warnOperationCancelled.
  ///
  /// In en, this message translates to:
  /// **'The operation was stopped safely; completed items were kept in history and remaining items were not started'**
  String get warnOperationCancelled;

  /// No description provided for @opRfiLogAddTitle.
  ///
  /// In en, this message translates to:
  /// **'Add multi-sheet RFI Log entries'**
  String get opRfiLogAddTitle;

  /// No description provided for @opRfiFollowUpSyncTitle.
  ///
  /// In en, this message translates to:
  /// **'Synchronize RFI Log to Follow Up'**
  String get opRfiFollowUpSyncTitle;

  /// No description provided for @opRfiFollowUpSyncDesc.
  ///
  /// In en, this message translates to:
  /// **'Safely synchronizes CIV/MEC lifecycle rows and FINAL Worley Signed/Not Signed rows from the RFI Log, including prepared missing entries, removals, and grouped statuses.'**
  String get opRfiFollowUpSyncDesc;

  /// No description provided for @paramRfiFollowUpRfiLogWorkbook.
  ///
  /// In en, this message translates to:
  /// **'Source RFI Log workbook'**
  String get paramRfiFollowUpRfiLogWorkbook;

  /// No description provided for @paramRfiFollowUpRfiLogWorkbookHelp.
  ///
  /// In en, this message translates to:
  /// **'Select the .xlsm or .xlsx RFI Log workbook whose statuses will be read.'**
  String get paramRfiFollowUpRfiLogWorkbookHelp;

  /// No description provided for @paramRfiFollowUpFollowUpWorkbook.
  ///
  /// In en, this message translates to:
  /// **'Target Follow Up workbook'**
  String get paramRfiFollowUpFollowUpWorkbook;

  /// No description provided for @paramRfiFollowUpFollowUpWorkbookHelp.
  ///
  /// In en, this message translates to:
  /// **'Select the .xlsm or .xlsx workbook where only previewed RFI and status cells may change.'**
  String get paramRfiFollowUpFollowUpWorkbookHelp;

  /// No description provided for @errorParamRequiredRfiLogWorkbook.
  ///
  /// In en, this message translates to:
  /// **'RFI Log workbook is required'**
  String get errorParamRequiredRfiLogWorkbook;

  /// No description provided for @errorParamRequiredFollowUpWorkbook.
  ///
  /// In en, this message translates to:
  /// **'Follow Up workbook is required'**
  String get errorParamRequiredFollowUpWorkbook;

  /// No description provided for @errorRfiFollowUpWorkbookInvalid.
  ///
  /// In en, this message translates to:
  /// **'Select valid RFI Log and Follow Up .xlsm or .xlsx workbooks.'**
  String get errorRfiFollowUpWorkbookInvalid;

  /// No description provided for @errorRfiFollowUpWorkbookLocked.
  ///
  /// In en, this message translates to:
  /// **'Close the selected workbook in Excel or another application, then try again.'**
  String get errorRfiFollowUpWorkbookLocked;

  /// No description provided for @errorRfiFollowUpWorkbookChanged.
  ///
  /// In en, this message translates to:
  /// **'A selected workbook changed after preview. Create a new preview before applying.'**
  String get errorRfiFollowUpWorkbookChanged;

  /// No description provided for @errorRfiFollowUpLayoutAmbiguous.
  ///
  /// In en, this message translates to:
  /// **'More than one safe Follow Up table was found. Keep one exact RFI NO./STATUS table.'**
  String get errorRfiFollowUpLayoutAmbiguous;

  /// No description provided for @errorRfiFollowUpLayoutMissing.
  ///
  /// In en, this message translates to:
  /// **'No safe Follow Up table with exact RFI NO. and STATUS headers was found.'**
  String get errorRfiFollowUpLayoutMissing;

  /// No description provided for @errorRfiFollowUpPatchVerification.
  ///
  /// In en, this message translates to:
  /// **'DocFlow could not verify that only approved status cells changed. No output was accepted.'**
  String get errorRfiFollowUpPatchVerification;

  /// No description provided for @errorRfiFollowUpProtected.
  ///
  /// In en, this message translates to:
  /// **'The workbook, target worksheet, or required cells are protected or unsafe to edit.'**
  String get errorRfiFollowUpProtected;

  /// No description provided for @errorRfiFollowUpSignedPackage.
  ///
  /// In en, this message translates to:
  /// **'The workbook contains a digital signature and cannot be changed safely by automation.'**
  String get errorRfiFollowUpSignedPackage;

  /// No description provided for @errorRfiFollowUpLimitExceeded.
  ///
  /// In en, this message translates to:
  /// **'The workbook exceeds the safe processing limits for this operation.'**
  String get errorRfiFollowUpLimitExceeded;

  /// No description provided for @errorRfiFollowUpNoRowsSelected.
  ///
  /// In en, this message translates to:
  /// **'Select at least one safe Follow Up row before applying.'**
  String get errorRfiFollowUpNoRowsSelected;

  /// No description provided for @errorRfiFollowUpRetainedStorageFull.
  ///
  /// In en, this message translates to:
  /// **'The bounded recovery storage is full. Review or remove older retained DocFlow evidence before retrying.'**
  String get errorRfiFollowUpRetainedStorageFull;

  /// No description provided for @errorRfiFollowUpBackupExists.
  ///
  /// In en, this message translates to:
  /// **'A backup with the reserved name already exists. No workbook was overwritten.'**
  String get errorRfiFollowUpBackupExists;

  /// No description provided for @errorRfiFollowUpBackupVerify.
  ///
  /// In en, this message translates to:
  /// **'DocFlow could not verify the workbook backup. The operation was stopped for recovery.'**
  String get errorRfiFollowUpBackupVerify;

  /// No description provided for @errorRfiFollowUpRecoveryRequired.
  ///
  /// In en, this message translates to:
  /// **'The operation requires recovery; do not apply it again.'**
  String get errorRfiFollowUpRecoveryRequired;

  /// No description provided for @rfiFollowUpEditorTitle.
  ///
  /// In en, this message translates to:
  /// **'Review RFI Follow Up status candidates'**
  String get rfiFollowUpEditorTitle;

  /// No description provided for @rfiFollowUpEditorRowCount.
  ///
  /// In en, this message translates to:
  /// **'{count} Follow Up rows'**
  String rfiFollowUpEditorRowCount(Object count);

  /// No description provided for @rfiFollowUpColumnEnabled.
  ///
  /// In en, this message translates to:
  /// **'Apply'**
  String get rfiFollowUpColumnEnabled;

  /// No description provided for @rfiFollowUpColumnRfiNo.
  ///
  /// In en, this message translates to:
  /// **'RFI number'**
  String get rfiFollowUpColumnRfiNo;

  /// No description provided for @rfiFollowUpColumnSourceStatuses.
  ///
  /// In en, this message translates to:
  /// **'RFI Log source value'**
  String get rfiFollowUpColumnSourceStatuses;

  /// No description provided for @rfiFollowUpColumnCurrentStatus.
  ///
  /// In en, this message translates to:
  /// **'Current Follow Up status'**
  String get rfiFollowUpColumnCurrentStatus;

  /// No description provided for @rfiFollowUpColumnProposedRfiNo.
  ///
  /// In en, this message translates to:
  /// **'Proposed RFI group'**
  String get rfiFollowUpColumnProposedRfiNo;

  /// No description provided for @rfiFollowUpColumnProposedStatus.
  ///
  /// In en, this message translates to:
  /// **'Proposed status'**
  String get rfiFollowUpColumnProposedStatus;

  /// No description provided for @rfiFollowUpColumnDisposition.
  ///
  /// In en, this message translates to:
  /// **'Decision'**
  String get rfiFollowUpColumnDisposition;

  /// No description provided for @rfiFollowUpColumnReason.
  ///
  /// In en, this message translates to:
  /// **'Why'**
  String get rfiFollowUpColumnReason;

  /// No description provided for @rfiFollowUpColumnSheet.
  ///
  /// In en, this message translates to:
  /// **'Worksheet'**
  String get rfiFollowUpColumnSheet;

  /// No description provided for @rfiFollowUpColumnRow.
  ///
  /// In en, this message translates to:
  /// **'Row'**
  String get rfiFollowUpColumnRow;

  /// No description provided for @rfiFollowUpColumnMappingDetails.
  ///
  /// In en, this message translates to:
  /// **'Mapping details'**
  String get rfiFollowUpColumnMappingDetails;

  /// No description provided for @rfiFollowUpDispositionReady.
  ///
  /// In en, this message translates to:
  /// **'Ready to update'**
  String get rfiFollowUpDispositionReady;

  /// No description provided for @rfiFollowUpDispositionAlreadySynced.
  ///
  /// In en, this message translates to:
  /// **'Already synchronized'**
  String get rfiFollowUpDispositionAlreadySynced;

  /// No description provided for @rfiFollowUpDispositionManualReview.
  ///
  /// In en, this message translates to:
  /// **'Manual review only'**
  String get rfiFollowUpDispositionManualReview;

  /// No description provided for @warnRfiFollowUpManualRows.
  ///
  /// In en, this message translates to:
  /// **'rows require manual review and cannot be selected automatically.'**
  String get warnRfiFollowUpManualRows;

  /// No description provided for @warnRfiFollowUpNoRowsSelected.
  ///
  /// In en, this message translates to:
  /// **'No safe rows can be applied. Disabled checkboxes are manual-only; review the Why column.'**
  String get warnRfiFollowUpNoRowsSelected;

  /// No description provided for @warnRfiFollowUpUnmappedRecords.
  ///
  /// In en, this message translates to:
  /// **'RFI Log records with unsupported routes, missing prepared rows, or ambiguous matches were left unchanged. Review Why and Mapping details for the exact ITP/item and source/target context.'**
  String get warnRfiFollowUpUnmappedRecords;

  /// No description provided for @warnRfiFollowUpReasonTargetRfiFormula.
  ///
  /// In en, this message translates to:
  /// **'The target RFI number is a formula.'**
  String get warnRfiFollowUpReasonTargetRfiFormula;

  /// No description provided for @warnRfiFollowUpReasonTargetRfiMerged.
  ///
  /// In en, this message translates to:
  /// **'The target RFI number is inside a merged cell.'**
  String get warnRfiFollowUpReasonTargetRfiMerged;

  /// No description provided for @warnRfiFollowUpReasonBlankRfi.
  ///
  /// In en, this message translates to:
  /// **'The target RFI number is blank.'**
  String get warnRfiFollowUpReasonBlankRfi;

  /// No description provided for @warnRfiFollowUpReasonInvalidRfi.
  ///
  /// In en, this message translates to:
  /// **'The target RFI number must use the exact short format, for example CIV-0170.'**
  String get warnRfiFollowUpReasonInvalidRfi;

  /// No description provided for @warnRfiFollowUpReasonTargetDuplicate.
  ///
  /// In en, this message translates to:
  /// **'The same full RFI number appears more than once in the target table.'**
  String get warnRfiFollowUpReasonTargetDuplicate;

  /// No description provided for @warnRfiFollowUpReasonSourceMissing.
  ///
  /// In en, this message translates to:
  /// **'No exact full-value RFI match exists in the RFI Log.'**
  String get warnRfiFollowUpReasonSourceMissing;

  /// No description provided for @warnRfiFollowUpReasonSourceFormula.
  ///
  /// In en, this message translates to:
  /// **'The matching source RFI or status is a formula.'**
  String get warnRfiFollowUpReasonSourceFormula;

  /// No description provided for @warnRfiFollowUpReasonSourceMerged.
  ///
  /// In en, this message translates to:
  /// **'The matching source RFI or status is in a merged cell.'**
  String get warnRfiFollowUpReasonSourceMerged;

  /// No description provided for @warnRfiFollowUpReasonSourceStatusBlank.
  ///
  /// In en, this message translates to:
  /// **'The matching RFI Log status is blank.'**
  String get warnRfiFollowUpReasonSourceStatusBlank;

  /// No description provided for @warnRfiFollowUpReasonSourceStatusUnknown.
  ///
  /// In en, this message translates to:
  /// **'The matching RFI Log status is not exactly Completed or Ongoing.'**
  String get warnRfiFollowUpReasonSourceStatusUnknown;

  /// No description provided for @warnRfiFollowUpReasonSourceConflict.
  ///
  /// In en, this message translates to:
  /// **'The RFI Log contains conflicting statuses for this exact RFI number.'**
  String get warnRfiFollowUpReasonSourceConflict;

  /// No description provided for @warnRfiFollowUpReasonTargetCellMissing.
  ///
  /// In en, this message translates to:
  /// **'The target status cell does not exist.'**
  String get warnRfiFollowUpReasonTargetCellMissing;

  /// No description provided for @warnRfiFollowUpReasonTargetFormula.
  ///
  /// In en, this message translates to:
  /// **'The target status cell is a formula.'**
  String get warnRfiFollowUpReasonTargetFormula;

  /// No description provided for @warnRfiFollowUpReasonTargetMerged.
  ///
  /// In en, this message translates to:
  /// **'The target status cell is merged.'**
  String get warnRfiFollowUpReasonTargetMerged;

  /// No description provided for @warnRfiFollowUpReasonTargetStatusUnknown.
  ///
  /// In en, this message translates to:
  /// **'The current target status is not blank, Signed, or Not Signed, so it will not be overwritten.'**
  String get warnRfiFollowUpReasonTargetStatusUnknown;

  /// No description provided for @warnRfiFollowUpReasonAlreadySynced.
  ///
  /// In en, this message translates to:
  /// **'The target already has the proposed status.'**
  String get warnRfiFollowUpReasonAlreadySynced;

  /// No description provided for @warnRfiFollowUpReasonReadySigned.
  ///
  /// In en, this message translates to:
  /// **'The exact RFI Log remark Signed by Worley can be applied as Signed.'**
  String get warnRfiFollowUpReasonReadySigned;

  /// No description provided for @warnRfiFollowUpReasonReadyLogSync.
  ///
  /// In en, this message translates to:
  /// **'The RFI group and shared status can be synchronized from the approved RFI Log mapping.'**
  String get warnRfiFollowUpReasonReadyLogSync;

  /// No description provided for @warnRfiFollowUpReasonReadyWorleySync.
  ///
  /// In en, this message translates to:
  /// **'The FINAL Worley status can be synchronized from the exact Signed by Worley remark line.'**
  String get warnRfiFollowUpReasonReadyWorleySync;

  /// No description provided for @warnRfiFollowUpReasonReadyRemove.
  ///
  /// In en, this message translates to:
  /// **'Cancelled or Rejected RFI entries will be removed and the remaining shared status recalculated.'**
  String get warnRfiFollowUpReasonReadyRemove;

  /// No description provided for @warnRfiFollowUpReasonMappingMissing.
  ///
  /// In en, this message translates to:
  /// **'A supported Follow Up route exists, but no prepared target row matches this RFI\'s exact ITP, item, and context. Nothing was changed.'**
  String get warnRfiFollowUpReasonMappingMissing;

  /// No description provided for @warnRfiFollowUpReasonPartialContext.
  ///
  /// In en, this message translates to:
  /// **'Some source contexts match prepared Follow Up rows and others do not. Check the unmatched Tag No, Area, and Inspection Location in the RFI Log. DocFlow will not edit the source log.'**
  String get warnRfiFollowUpReasonPartialContext;

  /// No description provided for @warnRfiFollowUpReasonMappingUnsupported.
  ///
  /// In en, this message translates to:
  /// **'This ITP/item combination has no Follow Up route in the current template. It cannot be synchronized automatically.'**
  String get warnRfiFollowUpReasonMappingUnsupported;

  /// No description provided for @warnRfiFollowUpReasonMappingAmbiguous.
  ///
  /// In en, this message translates to:
  /// **'More than one target row matches this record; it was left unchanged instead of guessing.'**
  String get warnRfiFollowUpReasonMappingAmbiguous;

  /// No description provided for @warnRfiFollowUpReasonReadyNotSigned.
  ///
  /// In en, this message translates to:
  /// **'No exact Signed by Worley remark was found; Not Signed can be applied.'**
  String get warnRfiFollowUpReasonReadyNotSigned;

  /// No description provided for @opRfiLogAddDesc.
  ///
  /// In en, this message translates to:
  /// **'Creates CIV, MEC, ELE and PRC RFI numbers together, schedules them globally, fills lookup fields and prepares separate Excel-native mail tables.'**
  String get opRfiLogAddDesc;

  /// No description provided for @paramRfiWorkbook.
  ///
  /// In en, this message translates to:
  /// **'RFI Log workbook'**
  String get paramRfiWorkbook;

  /// No description provided for @paramRfiWorkbookHelp.
  ///
  /// In en, this message translates to:
  /// **'Select a macro-enabled .xlsm or standard .xlsx RFI Log workbook to read and update safely.'**
  String get paramRfiWorkbookHelp;

  /// No description provided for @dialogFilterExcelWorkbook.
  ///
  /// In en, this message translates to:
  /// **'Excel (.xlsm, .xlsx)'**
  String get dialogFilterExcelWorkbook;

  /// No description provided for @errorRfiWorkbookInvalid.
  ///
  /// In en, this message translates to:
  /// **'Select a valid macro-enabled .xlsm or standard .xlsx RFI Log workbook.'**
  String get errorRfiWorkbookInvalid;

  /// No description provided for @paramRfiEmailText.
  ///
  /// In en, this message translates to:
  /// **'Paste the request email'**
  String get paramRfiEmailText;

  /// No description provided for @paramRfiEmailTextHelp.
  ///
  /// In en, this message translates to:
  /// **'Paste the email body. It is used only for this preview and is never saved to settings or the workbook.'**
  String get paramRfiEmailTextHelp;

  /// No description provided for @paramRfiEmailProvider.
  ///
  /// In en, this message translates to:
  /// **'Email extraction method'**
  String get paramRfiEmailProvider;

  /// No description provided for @paramRfiEmailProviderHelp.
  ///
  /// In en, this message translates to:
  /// **'Offline is private and free. Gemini Free is optional and sends the pasted email to Google.'**
  String get paramRfiEmailProviderHelp;

  /// No description provided for @choiceRfiEmailProviderOffline.
  ///
  /// In en, this message translates to:
  /// **'Offline — private (recommended)'**
  String get choiceRfiEmailProviderOffline;

  /// No description provided for @choiceRfiEmailProviderGemini.
  ///
  /// In en, this message translates to:
  /// **'Gemini Free — external AI'**
  String get choiceRfiEmailProviderGemini;

  /// No description provided for @paramRfiGeminiApiKey.
  ///
  /// In en, this message translates to:
  /// **'Gemini API key (only for Gemini)'**
  String get paramRfiGeminiApiKey;

  /// No description provided for @paramRfiGeminiApiKeyHelp.
  ///
  /// In en, this message translates to:
  /// **'Not saved. You may instead define GEMINI_API_KEY in Windows. Do not use Gemini Free for confidential company emails.'**
  String get paramRfiGeminiApiKeyHelp;

  /// No description provided for @errorParamRequiredEmailProvider.
  ///
  /// In en, this message translates to:
  /// **'Choose an email extraction method'**
  String get errorParamRequiredEmailProvider;

  /// No description provided for @warnRfiEmailExtracted.
  ///
  /// In en, this message translates to:
  /// **'Email fields were extracted into the editable preview'**
  String get warnRfiEmailExtracted;

  /// No description provided for @warnRfiEmailNeedsReview.
  ///
  /// In en, this message translates to:
  /// **'Some email details are ambiguous or incomplete; review the rows'**
  String get warnRfiEmailNeedsReview;

  /// No description provided for @warnRfiAiFallback.
  ///
  /// In en, this message translates to:
  /// **'Gemini was unavailable; private offline extraction was used'**
  String get warnRfiAiFallback;

  /// No description provided for @warnRfiGeminiPrivacy.
  ///
  /// In en, this message translates to:
  /// **'Gemini Free sends this email to Google. Free-tier content may be used for product improvement; do not send confidential information.'**
  String get warnRfiGeminiPrivacy;

  /// No description provided for @paramRfiDiscipline.
  ///
  /// In en, this message translates to:
  /// **'Discipline'**
  String get paramRfiDiscipline;

  /// No description provided for @paramRfiCount.
  ///
  /// In en, this message translates to:
  /// **'Number of RFI numbers'**
  String get paramRfiCount;

  /// No description provided for @paramRfiCountHelp.
  ///
  /// In en, this message translates to:
  /// **'Counts unique RFI numbers; duplicate rows do not consume another number or time slot.'**
  String get paramRfiCountHelp;

  /// No description provided for @paramRfiSheetCounts.
  ///
  /// In en, this message translates to:
  /// **'Sheets and RFI counts'**
  String get paramRfiSheetCounts;

  /// No description provided for @paramRfiSheetCountsHelp.
  ///
  /// In en, this message translates to:
  /// **'Select one or more sheets and set the unique RFI number count for each. All selected sheets are edited and applied together.'**
  String get paramRfiSheetCountsHelp;

  /// No description provided for @rfiSheetCountsTotal.
  ///
  /// In en, this message translates to:
  /// **'{count} RFI numbers in this operation'**
  String rfiSheetCountsTotal(Object count);

  /// No description provided for @paramRfiOverflowNextDay.
  ///
  /// In en, this message translates to:
  /// **'Move overflow to the next calendar day'**
  String get paramRfiOverflowNextDay;

  /// No description provided for @paramRfiOverflowNextDayHelp.
  ///
  /// In en, this message translates to:
  /// **'After 17:30, continue at 09:00 the next day. Turn off to keep later times on the same day.'**
  String get paramRfiOverflowNextDayHelp;

  /// No description provided for @errorParamRequiredWorkbook.
  ///
  /// In en, this message translates to:
  /// **'RFI Log workbook is required'**
  String get errorParamRequiredWorkbook;

  /// No description provided for @errorParamRequiredDiscipline.
  ///
  /// In en, this message translates to:
  /// **'Discipline is required'**
  String get errorParamRequiredDiscipline;

  /// No description provided for @errorParamRequiredCount.
  ///
  /// In en, this message translates to:
  /// **'RFI count is required'**
  String get errorParamRequiredCount;

  /// No description provided for @errorParamRequiredSheetCounts.
  ///
  /// In en, this message translates to:
  /// **'Select at least one sheet and RFI number'**
  String get errorParamRequiredSheetCounts;

  /// No description provided for @noteRfiWorkbookUpdate.
  ///
  /// In en, this message translates to:
  /// **'RFI Log rows will be added'**
  String get noteRfiWorkbookUpdate;

  /// No description provided for @noteRfiRestoreBackup.
  ///
  /// In en, this message translates to:
  /// **'Restore the workbook backup'**
  String get noteRfiRestoreBackup;

  /// No description provided for @warnRfiOverflowMoved.
  ///
  /// In en, this message translates to:
  /// **'RFI time slots exceeded 17:30 and were moved to the next calendar day'**
  String get warnRfiOverflowMoved;

  /// No description provided for @warnRfiOverflowKept.
  ///
  /// In en, this message translates to:
  /// **'RFI time slots exceed 17:30 and will remain on the same day'**
  String get warnRfiOverflowKept;

  /// No description provided for @warnRfiGroupScheduleMismatch.
  ///
  /// In en, this message translates to:
  /// **'Rows sharing an RFI number have different inspection dates or times'**
  String get warnRfiGroupScheduleMismatch;

  /// No description provided for @rfiEditorTitle.
  ///
  /// In en, this message translates to:
  /// **'Review and edit RFI entries'**
  String get rfiEditorTitle;

  /// No description provided for @rfiEditorRowCount.
  ///
  /// In en, this message translates to:
  /// **'{count} Excel rows'**
  String rfiEditorRowCount(Object count);

  /// No description provided for @rfiEditorScrollHint.
  ///
  /// In en, this message translates to:
  /// **'Scroll horizontally to edit every field'**
  String get rfiEditorScrollHint;

  /// No description provided for @rfiEditorCompactHint.
  ///
  /// In en, this message translates to:
  /// **'Hover for full numbers; automatic fields are under Details'**
  String get rfiEditorCompactHint;

  /// No description provided for @rfiEditorReviewWarnings.
  ///
  /// In en, this message translates to:
  /// **'The edited rows were validated. Review the new warnings, then apply again.'**
  String get rfiEditorReviewWarnings;

  /// No description provided for @rfiEditorCopied.
  ///
  /// In en, this message translates to:
  /// **'Excel opened; {count} rows were filtered and copied with native Excel formatting'**
  String rfiEditorCopied(Object count);

  /// No description provided for @rfiActionDuplicate.
  ///
  /// In en, this message translates to:
  /// **'Add another row with the same RFI number'**
  String get rfiActionDuplicate;

  /// No description provided for @rfiActionRemoveDuplicate.
  ///
  /// In en, this message translates to:
  /// **'Remove this duplicate row'**
  String get rfiActionRemoveDuplicate;

  /// No description provided for @rfiActionCopyMail.
  ///
  /// In en, this message translates to:
  /// **'Filter and copy exactly in Excel'**
  String get rfiActionCopyMail;

  /// No description provided for @rfiActionPrepareReply.
  ///
  /// In en, this message translates to:
  /// **'Prepare Outlook reply'**
  String get rfiActionPrepareReply;

  /// No description provided for @rfiActionCopySheet.
  ///
  /// In en, this message translates to:
  /// **'Copy from Excel'**
  String get rfiActionCopySheet;

  /// No description provided for @rfiMailPreviewTitle.
  ///
  /// In en, this message translates to:
  /// **'Excel table for Outlook'**
  String get rfiMailPreviewTitle;

  /// No description provided for @rfiMailPreviewSourceWorkbook.
  ///
  /// In en, this message translates to:
  /// **'Current data from Excel'**
  String get rfiMailPreviewSourceWorkbook;

  /// No description provided for @rfiMailPreviewSourcePlan.
  ///
  /// In en, this message translates to:
  /// **'Approved plan data'**
  String get rfiMailPreviewSourcePlan;

  /// No description provided for @rfiMailPreviewHint.
  ///
  /// In en, this message translates to:
  /// **'This preview shows the exact columns and values that will be copied. Paste normally into Outlook; Excel formatting and cell borders are preserved.'**
  String get rfiMailPreviewHint;

  /// No description provided for @rfiMailPreviewCopy.
  ///
  /// In en, this message translates to:
  /// **'Copy formatted table'**
  String get rfiMailPreviewCopy;

  /// No description provided for @rfiActionDetails.
  ///
  /// In en, this message translates to:
  /// **'Automatic fields and details'**
  String get rfiActionDetails;

  /// No description provided for @rfiColSheet.
  ///
  /// In en, this message translates to:
  /// **'Sheet'**
  String get rfiColSheet;

  /// No description provided for @rfiColRfiNo.
  ///
  /// In en, this message translates to:
  /// **'RFI'**
  String get rfiColRfiNo;

  /// No description provided for @rfiColItpNo.
  ///
  /// In en, this message translates to:
  /// **'ITP'**
  String get rfiColItpNo;

  /// No description provided for @rfiColItemNo.
  ///
  /// In en, this message translates to:
  /// **'Item'**
  String get rfiColItemNo;

  /// No description provided for @rfiColDescription.
  ///
  /// In en, this message translates to:
  /// **'Inspection Description'**
  String get rfiColDescription;

  /// No description provided for @rfiColForms.
  ///
  /// In en, this message translates to:
  /// **'Forms'**
  String get rfiColForms;

  /// No description provided for @rfiColTagNo.
  ///
  /// In en, this message translates to:
  /// **'Tag No.'**
  String get rfiColTagNo;

  /// No description provided for @rfiColArea.
  ///
  /// In en, this message translates to:
  /// **'Area'**
  String get rfiColArea;

  /// No description provided for @rfiColRowNo.
  ///
  /// In en, this message translates to:
  /// **'Row No.'**
  String get rfiColRowNo;

  /// No description provided for @rfiColIssueDate.
  ///
  /// In en, this message translates to:
  /// **'Issued'**
  String get rfiColIssueDate;

  /// No description provided for @rfiColInspectionDate.
  ///
  /// In en, this message translates to:
  /// **'Inspection'**
  String get rfiColInspectionDate;

  /// No description provided for @rfiColStartTime.
  ///
  /// In en, this message translates to:
  /// **'Time'**
  String get rfiColStartTime;

  /// No description provided for @rfiColAzfenType.
  ///
  /// In en, this message translates to:
  /// **'Insp. Type AZFEN'**
  String get rfiColAzfenType;

  /// No description provided for @rfiColIntecType.
  ///
  /// In en, this message translates to:
  /// **'Insp. Type INTEC'**
  String get rfiColIntecType;

  /// No description provided for @rfiColLocation.
  ///
  /// In en, this message translates to:
  /// **'Location'**
  String get rfiColLocation;

  /// No description provided for @rfiColSubcontractor.
  ///
  /// In en, this message translates to:
  /// **'Subcontractor'**
  String get rfiColSubcontractor;

  /// No description provided for @rfiDateDay.
  ///
  /// In en, this message translates to:
  /// **'Day'**
  String get rfiDateDay;

  /// No description provided for @rfiDateMonth.
  ///
  /// In en, this message translates to:
  /// **'Month'**
  String get rfiDateMonth;

  /// No description provided for @rfiDateChangeYear.
  ///
  /// In en, this message translates to:
  /// **'Change year'**
  String get rfiDateChangeYear;

  /// No description provided for @rfiDateYear.
  ///
  /// In en, this message translates to:
  /// **'Year'**
  String get rfiDateYear;

  /// No description provided for @rfiColActions.
  ///
  /// In en, this message translates to:
  /// **'Rows'**
  String get rfiColActions;

  /// No description provided for @actionClose.
  ///
  /// In en, this message translates to:
  /// **'Close'**
  String get actionClose;

  /// No description provided for @paramOrganizeIntoRfiFolders.
  ///
  /// In en, this message translates to:
  /// **'Create a separate folder for each RFI'**
  String get paramOrganizeIntoRfiFolders;

  /// No description provided for @paramOrganizeIntoRfiFoldersHelp.
  ///
  /// In en, this message translates to:
  /// **'Turn off to save renamed PDFs directly in the destination; unreadable or duplicate files still go to _review for safety'**
  String get paramOrganizeIntoRfiFoldersHelp;

  /// No description provided for @errorRfiWorkbookLocked.
  ///
  /// In en, this message translates to:
  /// **'The RFI Log is open or read-only. Save and close it in Excel, then check that the file is writable.'**
  String get errorRfiWorkbookLocked;

  /// No description provided for @opRfiStageSignedFirstPagesTitle.
  ///
  /// In en, this message translates to:
  /// **'Stage signed RFI first pages'**
  String get opRfiStageSignedFirstPagesTitle;

  /// No description provided for @opRfiNormalizePdfOrientationTitle.
  ///
  /// In en, this message translates to:
  /// **'Rotate RFI PDF pages'**
  String get opRfiNormalizePdfOrientationTitle;

  /// No description provided for @opRfiNormalizePdfOrientationDesc.
  ///
  /// In en, this message translates to:
  /// **'Standalone page rotation and orientation correction for PDFs under CIV, MEC, ELE and PRC, separate from Fast RFI Scan. Only confidently determined corrections are written to a separate review copy; if any page is uncertain, the PDF remains unchanged for manual review.'**
  String get opRfiNormalizePdfOrientationDesc;

  /// No description provided for @paramRfiOrientationRfiSystemRoot.
  ///
  /// In en, this message translates to:
  /// **'RFI system root'**
  String get paramRfiOrientationRfiSystemRoot;

  /// No description provided for @paramRfiOrientationRfiSystemRootHelp.
  ///
  /// In en, this message translates to:
  /// **'Read-only source containing the CIV, MEC, ELE and PRC folders. Source files are never changed.'**
  String get paramRfiOrientationRfiSystemRootHelp;

  /// No description provided for @paramRfiOrientationReviewRoot.
  ///
  /// In en, this message translates to:
  /// **'Orientation review folder'**
  String get paramRfiOrientationReviewRoot;

  /// No description provided for @paramRfiOrientationReviewRootHelp.
  ///
  /// In en, this message translates to:
  /// **'Separate folder where corrected copies are created with the original directory layout.'**
  String get paramRfiOrientationReviewRootHelp;

  /// No description provided for @paramScanIndexForceRescan.
  ///
  /// In en, this message translates to:
  /// **'Force a full rescan this time'**
  String get paramScanIndexForceRescan;

  /// No description provided for @paramScanIndexForceRescanHelp.
  ///
  /// In en, this message translates to:
  /// **'Ignore cached analysis for this run. This switch resets automatically after leaving the task.'**
  String get paramScanIndexForceRescanHelp;

  /// No description provided for @errorRfiOrientationReviewRootMustBeSeparate.
  ///
  /// In en, this message translates to:
  /// **'The review folder must not overlap the RFI system'**
  String get errorRfiOrientationReviewRootMustBeSeparate;

  /// No description provided for @warnRfiOrientationNoPdfs.
  ///
  /// In en, this message translates to:
  /// **'No PDFs were found under CIV, MEC, ELE or PRC'**
  String get warnRfiOrientationNoPdfs;

  /// No description provided for @warnRfiOrientationUnsupported.
  ///
  /// In en, this message translates to:
  /// **'The PDF could not be analysed safely'**
  String get warnRfiOrientationUnsupported;

  /// No description provided for @warnRfiOrientationNeedsReview.
  ///
  /// In en, this message translates to:
  /// **'At least one page orientation was not confident enough; the file was not applied'**
  String get warnRfiOrientationNeedsReview;

  /// No description provided for @warnRfiOrientationReviewExists.
  ///
  /// In en, this message translates to:
  /// **'The review destination already exists and was not overwritten'**
  String get warnRfiOrientationReviewExists;

  /// No description provided for @noteRfiPdfOrientationNormalized.
  ///
  /// In en, this message translates to:
  /// **'Correct approved PDF page orientations in a separate review copy'**
  String get noteRfiPdfOrientationNormalized;

  /// No description provided for @rfiOrientationPages.
  ///
  /// In en, this message translates to:
  /// **'Page rotations'**
  String get rfiOrientationPages;

  /// No description provided for @opRfiStageSignedFirstPagesDesc.
  ///
  /// In en, this message translates to:
  /// **'Identifies signed scans, searches every nested RFI System folder with bounded multi-core discovery, and replaces only page one in separate review copies without touching the archive.'**
  String get opRfiStageSignedFirstPagesDesc;

  /// No description provided for @paramRfiSignedScans.
  ///
  /// In en, this message translates to:
  /// **'New signed scans folder'**
  String get paramRfiSignedScans;

  /// No description provided for @paramRfiSignedScansHelp.
  ///
  /// In en, this message translates to:
  /// **'New signed RFI PDFs. Every page of a combined scan is identified independently.'**
  String get paramRfiSignedScansHelp;

  /// No description provided for @paramRfiArchiveRoot.
  ///
  /// In en, this message translates to:
  /// **'Existing RFI archive root'**
  String get paramRfiArchiveRoot;

  /// No description provided for @paramRfiArchiveRootHelp.
  ///
  /// In en, this message translates to:
  /// **'Read-only RFI System root. Every nested folder is searched with bounded multi-core discovery; only exact RFI PDF filenames can match.'**
  String get paramRfiArchiveRootHelp;

  /// No description provided for @paramRfiReviewRoot.
  ///
  /// In en, this message translates to:
  /// **'Review output folder'**
  String get paramRfiReviewRoot;

  /// No description provided for @paramRfiReviewRootHelp.
  ///
  /// In en, this message translates to:
  /// **'A separate, preferably empty folder where copies are written using the archive\'s relative folder layout.'**
  String get paramRfiReviewRootHelp;

  /// No description provided for @paramRfiScanSubfolders.
  ///
  /// In en, this message translates to:
  /// **'Also scan signed-scan subfolders'**
  String get paramRfiScanSubfolders;

  /// No description provided for @paramRfiScanSubfoldersHelp.
  ///
  /// In en, this message translates to:
  /// **'Enabled by default. Identifies PDFs at every depth below the new scans folder; disable only for a deliberate top-level-only run.'**
  String get paramRfiScanSubfoldersHelp;

  /// No description provided for @errorParamRequiredSignedScans.
  ///
  /// In en, this message translates to:
  /// **'New signed scans folder is required'**
  String get errorParamRequiredSignedScans;

  /// No description provided for @errorParamRequiredArchiveRoot.
  ///
  /// In en, this message translates to:
  /// **'Existing RFI archive root is required'**
  String get errorParamRequiredArchiveRoot;

  /// No description provided for @errorParamRequiredReviewRoot.
  ///
  /// In en, this message translates to:
  /// **'Review output folder is required'**
  String get errorParamRequiredReviewRoot;

  /// No description provided for @errorRfiReviewRootMustBeSeparate.
  ///
  /// In en, this message translates to:
  /// **'The review output folder must be completely separate from both the archive and the new scans folder. No files were touched.'**
  String get errorRfiReviewRootMustBeSeparate;

  /// No description provided for @noteRfiSignedFirstPageStaged.
  ///
  /// In en, this message translates to:
  /// **'Create a review copy with the signed first page'**
  String get noteRfiSignedFirstPageStaged;

  /// No description provided for @noteRfiCleanupStaleTemp.
  ///
  /// In en, this message translates to:
  /// **'Remove the stale temporary link for the completed review PDF'**
  String get noteRfiCleanupStaleTemp;

  /// No description provided for @warnRfiNoSignedPdfs.
  ///
  /// In en, this message translates to:
  /// **'No PDFs were found in the new scans folder'**
  String get warnRfiNoSignedPdfs;

  /// No description provided for @warnRfiSignedUnreadable.
  ///
  /// In en, this message translates to:
  /// **'The PDF could not be read; no output was prepared for this RFI'**
  String get warnRfiSignedUnreadable;

  /// No description provided for @warnRfiSignedDuplicate.
  ///
  /// In en, this message translates to:
  /// **'Multiple new scans have the same RFI number; none was selected automatically'**
  String get warnRfiSignedDuplicate;

  /// No description provided for @warnRfiArchiveUnreadable.
  ///
  /// In en, this message translates to:
  /// **'The entire archive could not be scanned safely; no output was prepared to avoid a missing or false match'**
  String get warnRfiArchiveUnreadable;

  /// No description provided for @warnRfiArchiveMissing.
  ///
  /// In en, this message translates to:
  /// **'No archive file was found for this RFI number'**
  String get warnRfiArchiveMissing;

  /// No description provided for @warnRfiArchiveAmbiguous.
  ///
  /// In en, this message translates to:
  /// **'Multiple archive files were found for this RFI number; choose one PDF from the archive-path field'**
  String get warnRfiArchiveAmbiguous;

  /// No description provided for @warnRfiReviewExists.
  ///
  /// In en, this message translates to:
  /// **'A file already exists at the review destination; it was not overwritten'**
  String get warnRfiReviewExists;

  /// No description provided for @warnRfiStaleTempFound.
  ///
  /// In en, this message translates to:
  /// **'The review PDF is already complete; a temporary file left by the earlier Windows lock will be removed safely'**
  String get warnRfiStaleTempFound;

  /// No description provided for @warnRfiReviewPathBlocked.
  ///
  /// In en, this message translates to:
  /// **'The review path is blocked by a file or redirected folder; no output was prepared'**
  String get warnRfiReviewPathBlocked;

  /// No description provided for @warnRfiSignedExtraPages.
  ///
  /// In en, this message translates to:
  /// **'The new PDF has multiple pages; only its signed first page will be used'**
  String get warnRfiSignedExtraPages;

  /// No description provided for @warnRfiSignedMultiPage.
  ///
  /// In en, this message translates to:
  /// **'Multi-page signed scan detected; every page was analyzed independently'**
  String get warnRfiSignedMultiPage;

  /// No description provided for @warnRfiFirstPageLayoutDiffers.
  ///
  /// In en, this message translates to:
  /// **'The new first page has a different size or orientation; check the preview before applying'**
  String get warnRfiFirstPageLayoutDiffers;

  /// No description provided for @warnRfiPdfFidelityUnsupported.
  ///
  /// In en, this message translates to:
  /// **'This PDF cannot be staged without risking document fidelity; review the reason and handle it manually'**
  String get warnRfiPdfFidelityUnsupported;

  /// No description provided for @rfiSignedEditorTitle.
  ///
  /// In en, this message translates to:
  /// **'Signed first-page review'**
  String get rfiSignedEditorTitle;

  /// No description provided for @rfiSignedEditorRowCount.
  ///
  /// In en, this message translates to:
  /// **'{count} matched RFI PDFs'**
  String rfiSignedEditorRowCount(Object count);

  /// No description provided for @rfiSignedColumnEnabled.
  ///
  /// In en, this message translates to:
  /// **'Stage'**
  String get rfiSignedColumnEnabled;

  /// No description provided for @rfiSignedColumnCode.
  ///
  /// In en, this message translates to:
  /// **'RFI number'**
  String get rfiSignedColumnCode;

  /// No description provided for @rfiSignedColumnScan.
  ///
  /// In en, this message translates to:
  /// **'Signed scan'**
  String get rfiSignedColumnScan;

  /// No description provided for @rfiSignedColumnArchive.
  ///
  /// In en, this message translates to:
  /// **'Archive path'**
  String get rfiSignedColumnArchive;

  /// No description provided for @rfiSignedArchiveChoose.
  ///
  /// In en, this message translates to:
  /// **'Choose an archive PDF'**
  String get rfiSignedArchiveChoose;

  /// No description provided for @rfiSignedPreviewTitle.
  ///
  /// In en, this message translates to:
  /// **'First-page comparison'**
  String get rfiSignedPreviewTitle;

  /// No description provided for @rfiSignedPreviewHelp.
  ///
  /// In en, this message translates to:
  /// **'Compare the archived first page with the exact signed page selected by the approved plan.'**
  String get rfiSignedPreviewHelp;

  /// No description provided for @rfiSignedPreviewRfi.
  ///
  /// In en, this message translates to:
  /// **'RFI to preview'**
  String get rfiSignedPreviewRfi;

  /// No description provided for @rfiSignedPreviewArchive.
  ///
  /// In en, this message translates to:
  /// **'Current archive page'**
  String get rfiSignedPreviewArchive;

  /// No description provided for @rfiSignedPreviewSigned.
  ///
  /// In en, this message translates to:
  /// **'New signed page'**
  String get rfiSignedPreviewSigned;

  /// No description provided for @rfiSignedPreviewArchiveAlt.
  ///
  /// In en, this message translates to:
  /// **'Current archived first page for {code}'**
  String rfiSignedPreviewArchiveAlt(Object code);

  /// No description provided for @rfiSignedPreviewSignedAlt.
  ///
  /// In en, this message translates to:
  /// **'New signed page for {code}'**
  String rfiSignedPreviewSignedAlt(Object code);

  /// No description provided for @rfiSignedPreviewChooseArchive.
  ///
  /// In en, this message translates to:
  /// **'Choose an archive PDF to display its first page.'**
  String get rfiSignedPreviewChooseArchive;

  /// No description provided for @rfiSignedPreviewUnavailable.
  ///
  /// In en, this message translates to:
  /// **'A bounded page preview could not be prepared; inspect the PDF manually before applying.'**
  String get rfiSignedPreviewUnavailable;

  /// No description provided for @rfiSignedPreviewOmitted.
  ///
  /// In en, this message translates to:
  /// **'Preview unavailable for {count} additional RFI rows because the safe preview limit was reached.'**
  String rfiSignedPreviewOmitted(Object count);

  /// No description provided for @rfiSignedPreviewPrevious.
  ///
  /// In en, this message translates to:
  /// **'Previous RFI'**
  String get rfiSignedPreviewPrevious;

  /// No description provided for @rfiSignedPreviewNext.
  ///
  /// In en, this message translates to:
  /// **'Next RFI'**
  String get rfiSignedPreviewNext;

  /// No description provided for @rfiSignedPreviewOnlyPageOne.
  ///
  /// In en, this message translates to:
  /// **'Only page 1 will be replaced; the remaining archived pages stay unchanged.'**
  String get rfiSignedPreviewOnlyPageOne;

  /// No description provided for @rfiSignedPreviewBefore.
  ///
  /// In en, this message translates to:
  /// **'BEFORE'**
  String get rfiSignedPreviewBefore;

  /// No description provided for @rfiSignedPreviewAfter.
  ///
  /// In en, this message translates to:
  /// **'AFTER'**
  String get rfiSignedPreviewAfter;

  /// No description provided for @rfiSignedPreviewEnlarge.
  ///
  /// In en, this message translates to:
  /// **'Enlarge'**
  String get rfiSignedPreviewEnlarge;

  /// No description provided for @rfiSignedNavLabel.
  ///
  /// In en, this message translates to:
  /// **'Signed-page preview sections'**
  String get rfiSignedNavLabel;

  /// No description provided for @rfiSignedNavApproved.
  ///
  /// In en, this message translates to:
  /// **'Approved RFIs'**
  String get rfiSignedNavApproved;

  /// No description provided for @rfiSignedNavCompare.
  ///
  /// In en, this message translates to:
  /// **'Compare pages'**
  String get rfiSignedNavCompare;

  /// No description provided for @rfiSignedNavReview.
  ///
  /// In en, this message translates to:
  /// **'Review copies'**
  String get rfiSignedNavReview;

  /// No description provided for @rfiSignedWarningReviewTitle.
  ///
  /// In en, this message translates to:
  /// **'Warning PDFs sent to Review'**
  String get rfiSignedWarningReviewTitle;

  /// No description provided for @rfiSignedWarningReviewHelp.
  ///
  /// In en, this message translates to:
  /// **'Apply copies each affected source PDF without changing it, preserving its subfolder path under _review.'**
  String get rfiSignedWarningReviewHelp;

  /// No description provided for @rfiSignedWarningReviewPlanned.
  ///
  /// In en, this message translates to:
  /// **'Will copy'**
  String get rfiSignedWarningReviewPlanned;

  /// No description provided for @rfiSignedWarningReviewExists.
  ///
  /// In en, this message translates to:
  /// **'Already exists'**
  String get rfiSignedWarningReviewExists;

  /// No description provided for @rfiSignedWarningReviewBlocked.
  ///
  /// In en, this message translates to:
  /// **'Blocked'**
  String get rfiSignedWarningReviewBlocked;

  /// No description provided for @rfiSignedWarningReviewDestination.
  ///
  /// In en, this message translates to:
  /// **'Review destination'**
  String get rfiSignedWarningReviewDestination;

  /// No description provided for @rfiSignedWarningReviewPage.
  ///
  /// In en, this message translates to:
  /// **'Page {page}'**
  String rfiSignedWarningReviewPage(Object page);

  /// No description provided for @rfiSignedWarningReviewFile.
  ///
  /// In en, this message translates to:
  /// **'Whole file'**
  String get rfiSignedWarningReviewFile;

  /// No description provided for @noteRfiSignedWarningReviewCopy.
  ///
  /// In en, this message translates to:
  /// **'Copy warning PDF to Review'**
  String get noteRfiSignedWarningReviewCopy;

  /// No description provided for @warnRfiWarningReviewExists.
  ///
  /// In en, this message translates to:
  /// **'The warning PDF already exists in Review and will not be overwritten'**
  String get warnRfiWarningReviewExists;

  /// No description provided for @warnRfiWarningReviewBlocked.
  ///
  /// In en, this message translates to:
  /// **'The warning PDF cannot be copied to Review because its destination path is blocked'**
  String get warnRfiWarningReviewBlocked;

  /// No description provided for @progressStages.
  ///
  /// In en, this message translates to:
  /// **'Processing stages'**
  String get progressStages;

  /// No description provided for @progressPhasePreparing.
  ///
  /// In en, this message translates to:
  /// **'Preparing preview'**
  String get progressPhasePreparing;

  /// No description provided for @progressPhasePreview.
  ///
  /// In en, this message translates to:
  /// **'Building preview'**
  String get progressPhasePreview;

  /// No description provided for @progressPhaseApply.
  ///
  /// In en, this message translates to:
  /// **'Applying approved plan'**
  String get progressPhaseApply;

  /// No description provided for @progressPhasePrintWaiting.
  ///
  /// In en, this message translates to:
  /// **'Waiting for printer'**
  String get progressPhasePrintWaiting;

  /// No description provided for @progressPhaseScan.
  ///
  /// In en, this message translates to:
  /// **'Scanning files'**
  String get progressPhaseScan;

  /// No description provided for @progressPhaseScanDiscovery.
  ///
  /// In en, this message translates to:
  /// **'Finding PDFs to scan'**
  String get progressPhaseScanDiscovery;

  /// No description provided for @progressPhaseFileGroupDiscovery.
  ///
  /// In en, this message translates to:
  /// **'Finding files to group'**
  String get progressPhaseFileGroupDiscovery;

  /// No description provided for @progressPhaseFileGroupAnalysis.
  ///
  /// In en, this message translates to:
  /// **'Reading RFI numbers'**
  String get progressPhaseFileGroupAnalysis;

  /// No description provided for @progressPhaseFileGroupPlanning.
  ///
  /// In en, this message translates to:
  /// **'Preparing the RFI folder plan'**
  String get progressPhaseFileGroupPlanning;

  /// No description provided for @progressPhaseScanOcr.
  ///
  /// In en, this message translates to:
  /// **'Reading PDFs with OCR'**
  String get progressPhaseScanOcr;

  /// No description provided for @progressPhaseScanPlanning.
  ///
  /// In en, this message translates to:
  /// **'Preparing the file plan'**
  String get progressPhaseScanPlanning;

  /// No description provided for @progressPhaseWorkbookReading.
  ///
  /// In en, this message translates to:
  /// **'Reading the workbook'**
  String get progressPhaseWorkbookReading;

  /// No description provided for @progressPhaseRfiRequestAnalysis.
  ///
  /// In en, this message translates to:
  /// **'Analysing the RFI request'**
  String get progressPhaseRfiRequestAnalysis;

  /// No description provided for @progressPhaseRfiPlanPreparation.
  ///
  /// In en, this message translates to:
  /// **'Preparing RFI rows'**
  String get progressPhaseRfiPlanPreparation;

  /// No description provided for @progressPhaseFollowUpAnalysis.
  ///
  /// In en, this message translates to:
  /// **'Calculating Follow Up mappings'**
  String get progressPhaseFollowUpAnalysis;

  /// No description provided for @progressPhaseSystemDiscovery.
  ///
  /// In en, this message translates to:
  /// **'Finding RFI system records'**
  String get progressPhaseSystemDiscovery;

  /// No description provided for @progressPhaseSystemSignatureAnalysis.
  ///
  /// In en, this message translates to:
  /// **'Analysing Worley signatures'**
  String get progressPhaseSystemSignatureAnalysis;

  /// No description provided for @progressPhaseSystemWorkbookRead.
  ///
  /// In en, this message translates to:
  /// **'Reading RFI workbooks'**
  String get progressPhaseSystemWorkbookRead;

  /// No description provided for @progressPhaseSystemMapping.
  ///
  /// In en, this message translates to:
  /// **'Matching signatures to workbook records'**
  String get progressPhaseSystemMapping;

  /// No description provided for @progressPhaseOrientationDiscovery.
  ///
  /// In en, this message translates to:
  /// **'Finding PDFs for orientation checks'**
  String get progressPhaseOrientationDiscovery;

  /// No description provided for @progressPhaseOrientationInspection.
  ///
  /// In en, this message translates to:
  /// **'Checking PDF structure and scan cache'**
  String get progressPhaseOrientationInspection;

  /// No description provided for @progressPhaseOrientationAnalysis.
  ///
  /// In en, this message translates to:
  /// **'Analysing PDF page orientation'**
  String get progressPhaseOrientationAnalysis;

  /// No description provided for @progressPhasePrinterCheck.
  ///
  /// In en, this message translates to:
  /// **'Checking printer readiness'**
  String get progressPhasePrinterCheck;

  /// No description provided for @progressPhasePrintFileInspection.
  ///
  /// In en, this message translates to:
  /// **'Inspecting print files and settings'**
  String get progressPhasePrintFileInspection;

  /// No description provided for @progressPhaseOutlookSelection.
  ///
  /// In en, this message translates to:
  /// **'Validating the Outlook selection'**
  String get progressPhaseOutlookSelection;

  /// No description provided for @progressPhaseSignedDiscovery.
  ///
  /// In en, this message translates to:
  /// **'Finding signed PDFs'**
  String get progressPhaseSignedDiscovery;

  /// No description provided for @progressPhaseSignedInspection.
  ///
  /// In en, this message translates to:
  /// **'Inspecting PDF pages'**
  String get progressPhaseSignedInspection;

  /// No description provided for @progressPhaseSignedOcr.
  ///
  /// In en, this message translates to:
  /// **'Reading RFI numbers'**
  String get progressPhaseSignedOcr;

  /// No description provided for @progressPhaseSignedClassification.
  ///
  /// In en, this message translates to:
  /// **'Matching signed pages'**
  String get progressPhaseSignedClassification;

  /// No description provided for @progressPhaseArchiveScan.
  ///
  /// In en, this message translates to:
  /// **'Searching the RFI archive'**
  String get progressPhaseArchiveScan;

  /// No description provided for @progressPhaseComparisonPreparation.
  ///
  /// In en, this message translates to:
  /// **'Preparing page comparisons'**
  String get progressPhaseComparisonPreparation;

  /// No description provided for @progressPhasePreviewReady.
  ///
  /// In en, this message translates to:
  /// **'Preview ready'**
  String get progressPhasePreviewReady;

  /// No description provided for @recoveryTitle.
  ///
  /// In en, this message translates to:
  /// **'Recovery center'**
  String get recoveryTitle;

  /// No description provided for @homeRecoveryDesc.
  ///
  /// In en, this message translates to:
  /// **'Safely review pending journals and uncertain print jobs.'**
  String get homeRecoveryDesc;

  /// No description provided for @recoveryHelp.
  ///
  /// In en, this message translates to:
  /// **'Only local journals and saved print queues authorize recovery.'**
  String get recoveryHelp;

  /// No description provided for @recoveryNoEffect.
  ///
  /// In en, this message translates to:
  /// **'No change occurred — previewing again is safe'**
  String get recoveryNoEffect;

  /// No description provided for @recoveryOutcomeUnknown.
  ///
  /// In en, this message translates to:
  /// **'Outcome unknown — no automatic retry'**
  String get recoveryOutcomeUnknown;

  /// No description provided for @recoveryManual.
  ///
  /// In en, this message translates to:
  /// **'Manual review required'**
  String get recoveryManual;

  /// No description provided for @recoveryRestoreOriginal.
  ///
  /// In en, this message translates to:
  /// **'Restore original Follow Up file'**
  String get recoveryRestoreOriginal;

  /// No description provided for @recoveryPrintQueue.
  ///
  /// In en, this message translates to:
  /// **'Uncertain print queue'**
  String get recoveryPrintQueue;

  /// No description provided for @recoveryEmpty.
  ///
  /// In en, this message translates to:
  /// **'Nothing needs recovery'**
  String get recoveryEmpty;

  /// No description provided for @recoveryEmptyHelp.
  ///
  /// In en, this message translates to:
  /// **'No pending journal or unfinished print queue was found.'**
  String get recoveryEmptyHelp;

  /// No description provided for @effectCommitted.
  ///
  /// In en, this message translates to:
  /// **'Completed'**
  String get effectCommitted;

  /// No description provided for @effectNoEffectRetryable.
  ///
  /// In en, this message translates to:
  /// **'No change occurred — retry is safe'**
  String get effectNoEffectRetryable;

  /// No description provided for @effectPartialCommitted.
  ///
  /// In en, this message translates to:
  /// **'Partially completed — review recovery'**
  String get effectPartialCommitted;

  /// No description provided for @effectOutcomeUnknown.
  ///
  /// In en, this message translates to:
  /// **'Outcome unknown — no automatic retry'**
  String get effectOutcomeUnknown;

  /// No description provided for @effectCancelledClean.
  ///
  /// In en, this message translates to:
  /// **'Cancelled cleanly — no change occurred'**
  String get effectCancelledClean;

  /// No description provided for @effectCancelledPartial.
  ///
  /// In en, this message translates to:
  /// **'Partially cancelled — review recovery'**
  String get effectCancelledPartial;

  /// No description provided for @presetTitle.
  ///
  /// In en, this message translates to:
  /// **'Presets'**
  String get presetTitle;

  /// No description provided for @presetSelect.
  ///
  /// In en, this message translates to:
  /// **'Saved preset'**
  String get presetSelect;

  /// No description provided for @presetSelectPlaceholder.
  ///
  /// In en, this message translates to:
  /// **'Choose a preset'**
  String get presetSelectPlaceholder;

  /// No description provided for @presetName.
  ///
  /// In en, this message translates to:
  /// **'New preset name'**
  String get presetName;

  /// No description provided for @presetApply.
  ///
  /// In en, this message translates to:
  /// **'Apply preset'**
  String get presetApply;

  /// No description provided for @presetSave.
  ///
  /// In en, this message translates to:
  /// **'Save preset'**
  String get presetSave;

  /// No description provided for @presetReplace.
  ///
  /// In en, this message translates to:
  /// **'Replace preset'**
  String get presetReplace;

  /// No description provided for @presetDelete.
  ///
  /// In en, this message translates to:
  /// **'Delete preset'**
  String get presetDelete;

  /// No description provided for @presetConfirmReplace.
  ///
  /// In en, this message translates to:
  /// **'A preset named {name} already exists. Replace it?'**
  String presetConfirmReplace(Object name);

  /// No description provided for @presetConfirmDelete.
  ///
  /// In en, this message translates to:
  /// **'Delete the preset named {name}?'**
  String presetConfirmDelete(Object name);

  /// No description provided for @presetEmpty.
  ///
  /// In en, this message translates to:
  /// **'No presets have been saved yet.'**
  String get presetEmpty;

  /// No description provided for @presetSaved.
  ///
  /// In en, this message translates to:
  /// **'Preset {name} was saved.'**
  String presetSaved(Object name);

  /// No description provided for @presetDeleted.
  ///
  /// In en, this message translates to:
  /// **'Preset {name} was deleted.'**
  String presetDeleted(Object name);

  /// No description provided for @errorPresetNameInvalid.
  ///
  /// In en, this message translates to:
  /// **'The preset name must contain 1–64 characters.'**
  String get errorPresetNameInvalid;

  /// No description provided for @errorPresetDuplicate.
  ///
  /// In en, this message translates to:
  /// **'A preset with this name already exists.'**
  String get errorPresetDuplicate;

  /// No description provided for @errorPresetLimit.
  ///
  /// In en, this message translates to:
  /// **'You can save at most 20 presets for this operation.'**
  String get errorPresetLimit;

  /// No description provided for @errorPresetStale.
  ///
  /// In en, this message translates to:
  /// **'This preset no longer exists. Refresh the list and try again.'**
  String get errorPresetStale;

  /// No description provided for @errorPresetWrite.
  ///
  /// In en, this message translates to:
  /// **'The preset settings could not be saved. Try again.'**
  String get errorPresetWrite;

  /// No description provided for @opRfiOutlookReplyDraftTitle.
  ///
  /// In en, this message translates to:
  /// **'Prepare Outlook Reply All draft'**
  String get opRfiOutlookReplyDraftTitle;

  /// No description provided for @opRfiOutlookReplyDraftDesc.
  ///
  /// In en, this message translates to:
  /// **'Uses only the one currently selected Outlook email, prepares a Reply All response, and displays it as an UNSENT draft for review.'**
  String get opRfiOutlookReplyDraftDesc;

  /// No description provided for @paramRfiOutlookReplyDraftReplyText.
  ///
  /// In en, this message translates to:
  /// **'Reply text'**
  String get paramRfiOutlookReplyDraftReplyText;

  /// No description provided for @paramRfiOutlookReplyDraftReplyTextHelp.
  ///
  /// In en, this message translates to:
  /// **'Enter the text to prepend. The preview escapes HTML, and apply never sends the message or adds recipients.'**
  String get paramRfiOutlookReplyDraftReplyTextHelp;

  /// No description provided for @errorParamRequiredReplyText.
  ///
  /// In en, this message translates to:
  /// **'Reply text is required'**
  String get errorParamRequiredReplyText;

  /// No description provided for @stepRfiOutlookReplyDraft.
  ///
  /// In en, this message translates to:
  /// **'Display an unsent Reply All draft'**
  String get stepRfiOutlookReplyDraft;

  /// No description provided for @outlookDraftPreviewTitle.
  ///
  /// In en, this message translates to:
  /// **'Unsent Outlook reply preview'**
  String get outlookDraftPreviewTitle;

  /// No description provided for @outlookDraftSubject.
  ///
  /// In en, this message translates to:
  /// **'Selected message subject'**
  String get outlookDraftSubject;

  /// No description provided for @outlookDraftPlainText.
  ///
  /// In en, this message translates to:
  /// **'Plain text'**
  String get outlookDraftPlainText;

  /// No description provided for @outlookDraftHtml.
  ///
  /// In en, this message translates to:
  /// **'Escaped HTML'**
  String get outlookDraftHtml;

  /// No description provided for @warnRfiOutlookDraftUnsent.
  ///
  /// In en, this message translates to:
  /// **'Apply will display an UNSENT Reply All draft; DocFlow never sends it.'**
  String get warnRfiOutlookDraftUnsent;

  /// No description provided for @warnRfiOutlookDraftUndoConditional.
  ///
  /// In en, this message translates to:
  /// **'Undo is available only if Outlook auto-saves a durable draft and that exact draft remains unchanged.'**
  String get warnRfiOutlookDraftUndoConditional;

  /// No description provided for @warnRfiOutlookDraftUndoUnavailable.
  ///
  /// In en, this message translates to:
  /// **'Outlook did not expose a durable auto-saved draft, so undo is unavailable.'**
  String get warnRfiOutlookDraftUndoUnavailable;

  /// No description provided for @errorRfiOutlookDraftUnsupportedPlatform.
  ///
  /// In en, this message translates to:
  /// **'Outlook draft automation is available only on Windows.'**
  String get errorRfiOutlookDraftUnsupportedPlatform;

  /// No description provided for @errorRfiOutlookDraftReplyRequired.
  ///
  /// In en, this message translates to:
  /// **'Enter reply text before previewing.'**
  String get errorRfiOutlookDraftReplyRequired;

  /// No description provided for @errorRfiOutlookDraftRfiLogNotCommitted.
  ///
  /// In en, this message translates to:
  /// **'The Outlook reply can only be prepared from the same completed RFI Log operation.'**
  String get errorRfiOutlookDraftRfiLogNotCommitted;

  /// No description provided for @errorRfiOutlookDraftReplyTooLong.
  ///
  /// In en, this message translates to:
  /// **'Reply text exceeds the 10,000-character limit.'**
  String get errorRfiOutlookDraftReplyTooLong;

  /// No description provided for @errorRfiOutlookDraftReplyInvalid.
  ///
  /// In en, this message translates to:
  /// **'Reply text contains an unsupported control character.'**
  String get errorRfiOutlookDraftReplyInvalid;

  /// No description provided for @errorRfiOutlookDraftSelectionRequired.
  ///
  /// In en, this message translates to:
  /// **'Select exactly one email in the active Outlook Explorer window.'**
  String get errorRfiOutlookDraftSelectionRequired;

  /// No description provided for @errorRfiOutlookDraftSingleSelectionRequired.
  ///
  /// In en, this message translates to:
  /// **'Select exactly one Outlook email.'**
  String get errorRfiOutlookDraftSingleSelectionRequired;

  /// No description provided for @errorRfiOutlookDraftMailItemRequired.
  ///
  /// In en, this message translates to:
  /// **'The current Outlook selection is not an email message.'**
  String get errorRfiOutlookDraftMailItemRequired;

  /// No description provided for @errorRfiOutlookDraftSelectionInvalid.
  ///
  /// In en, this message translates to:
  /// **'The selected Outlook email has no stable identity.'**
  String get errorRfiOutlookDraftSelectionInvalid;

  /// No description provided for @errorRfiOutlookDraftSubjectTooLong.
  ///
  /// In en, this message translates to:
  /// **'The selected Outlook subject exceeds the preview limit.'**
  String get errorRfiOutlookDraftSubjectTooLong;

  /// No description provided for @errorRfiOutlookDraftSelectionChanged.
  ///
  /// In en, this message translates to:
  /// **'The selected Outlook email changed after preview; preview again.'**
  String get errorRfiOutlookDraftSelectionChanged;

  /// No description provided for @errorRfiOutlookDraftOutlookUnavailable.
  ///
  /// In en, this message translates to:
  /// **'Microsoft Outlook desktop automation is unavailable.'**
  String get errorRfiOutlookDraftOutlookUnavailable;

  /// No description provided for @errorRfiOutlookDraftOutlookNotRunning.
  ///
  /// In en, this message translates to:
  /// **'Open Outlook desktop and select one email before previewing.'**
  String get errorRfiOutlookDraftOutlookNotRunning;

  /// No description provided for @errorRfiOutlookDraftComUnavailable.
  ///
  /// In en, this message translates to:
  /// **'The Windows Outlook automation apartment is unavailable.'**
  String get errorRfiOutlookDraftComUnavailable;

  /// No description provided for @errorRfiOutlookDraftComFailed.
  ///
  /// In en, this message translates to:
  /// **'Outlook could not provide the required bounded message property.'**
  String get errorRfiOutlookDraftComFailed;

  /// No description provided for @errorRfiOutlookDraftReplyAllFailed.
  ///
  /// In en, this message translates to:
  /// **'Outlook could not confirm creation of the Reply All draft; review Outlook manually.'**
  String get errorRfiOutlookDraftReplyAllFailed;

  /// No description provided for @errorRfiOutlookDraftBodyPrependFailed.
  ///
  /// In en, this message translates to:
  /// **'Outlook could not confirm the reply body update; review Outlook manually.'**
  String get errorRfiOutlookDraftBodyPrependFailed;

  /// No description provided for @errorRfiOutlookDraftBodyFormatUnsupported.
  ///
  /// In en, this message translates to:
  /// **'The Outlook reply uses an unsupported rich-text body format; review Outlook manually.'**
  String get errorRfiOutlookDraftBodyFormatUnsupported;

  /// No description provided for @errorRfiOutlookDraftContentTooLarge.
  ///
  /// In en, this message translates to:
  /// **'The Outlook draft body exceeds the safe automation limit.'**
  String get errorRfiOutlookDraftContentTooLarge;

  /// No description provided for @errorRfiOutlookDraftDisplayUnknown.
  ///
  /// In en, this message translates to:
  /// **'Outlook could not confirm that the unsent draft was displayed; review Outlook manually.'**
  String get errorRfiOutlookDraftDisplayUnknown;

  /// No description provided for @errorRfiOutlookDraftDraftMissingOrChanged.
  ///
  /// In en, this message translates to:
  /// **'Undo refused because the exact auto-saved draft is missing or has changed.'**
  String get errorRfiOutlookDraftDraftMissingOrChanged;

  /// No description provided for @errorRfiOutlookDraftDeleteUnknown.
  ///
  /// In en, this message translates to:
  /// **'Outlook could not confirm deletion of the unchanged draft; review Outlook manually.'**
  String get errorRfiOutlookDraftDeleteUnknown;

  /// No description provided for @errorRfiOutlookDraftUndoUnavailable.
  ///
  /// In en, this message translates to:
  /// **'Undo is unavailable because Outlook did not auto-save a durable draft.'**
  String get errorRfiOutlookDraftUndoUnavailable;

  /// No description provided for @opRfiOutlookArchiveSelectedTitle.
  ///
  /// In en, this message translates to:
  /// **'Archive selected Outlook email'**
  String get opRfiOutlookArchiveSelectedTitle;

  /// No description provided for @opRfiOutlookArchiveSelectedDesc.
  ///
  /// In en, this message translates to:
  /// **'Saves exactly one selected Outlook email as a local Unicode MSG copy without sending, moving, or deleting mail.'**
  String get opRfiOutlookArchiveSelectedDesc;

  /// No description provided for @paramRfiOutlookArchiveDestination.
  ///
  /// In en, this message translates to:
  /// **'Mail archive folder'**
  String get paramRfiOutlookArchiveDestination;

  /// No description provided for @paramRfiOutlookArchiveDestinationHelp.
  ///
  /// In en, this message translates to:
  /// **'Choose a separate staging folder. Group Files by RFI can later copy this MSG into every exact related RFI folder.'**
  String get paramRfiOutlookArchiveDestinationHelp;

  /// No description provided for @stepRfiOutlookArchive.
  ///
  /// In en, this message translates to:
  /// **'Create a guarded local MSG copy'**
  String get stepRfiOutlookArchive;

  /// No description provided for @warnRfiOutlookArchiveLocalCopy.
  ///
  /// In en, this message translates to:
  /// **'Apply creates one local MSG copy; the selected Outlook email and mailbox remain unchanged.'**
  String get warnRfiOutlookArchiveLocalCopy;

  /// No description provided for @errorRfiOutlookArchiveDestinationExists.
  ///
  /// In en, this message translates to:
  /// **'The planned MSG filename already exists in the archive folder; rename or move it, then preview again.'**
  String get errorRfiOutlookArchiveDestinationExists;

  /// No description provided for @errorRfiOutlookArchivePlanUnavailable.
  ///
  /// In en, this message translates to:
  /// **'The approved Outlook archive plan is no longer available; preview again.'**
  String get errorRfiOutlookArchivePlanUnavailable;

  /// No description provided for @errorRfiOutlookArchiveSelectionChanged.
  ///
  /// In en, this message translates to:
  /// **'The selected Outlook email changed after preview; select the original email and preview again.'**
  String get errorRfiOutlookArchiveSelectionChanged;

  /// No description provided for @errorRfiOutlookArchiveExportFailed.
  ///
  /// In en, this message translates to:
  /// **'Outlook could not create the private MSG export; no archive file was published.'**
  String get errorRfiOutlookArchiveExportFailed;

  /// No description provided for @errorRfiOutlookArchiveInvalidMsg.
  ///
  /// In en, this message translates to:
  /// **'Outlook produced an empty, oversized, or invalid MSG export; no archive file was published.'**
  String get errorRfiOutlookArchiveInvalidMsg;

  /// No description provided for @errorRfiOutlookArchiveWriteFailed.
  ///
  /// In en, this message translates to:
  /// **'The MSG archive file could not be created; no existing file was overwritten.'**
  String get errorRfiOutlookArchiveWriteFailed;

  /// No description provided for @errorRfiOutlookArchiveWriteUnknown.
  ///
  /// In en, this message translates to:
  /// **'The MSG archive write could not be verified; review the destination before retrying.'**
  String get errorRfiOutlookArchiveWriteUnknown;

  /// No description provided for @opRfiSystemSyncTitle.
  ///
  /// In en, this message translates to:
  /// **'Analyze RFI System & Sync Worley Signatures'**
  String get opRfiSystemSyncTitle;

  /// No description provided for @opRfiSystemSyncDesc.
  ///
  /// In en, this message translates to:
  /// **'Scans RFI system folders across disciplines, inspects page 1 for Worley signatures in the Others section, and synchronizes RFI Log and Follow Up workbooks.'**
  String get opRfiSystemSyncDesc;

  /// No description provided for @paramRfiSystemSyncRfiSystemRoot.
  ///
  /// In en, this message translates to:
  /// **'RFI System Root Folder'**
  String get paramRfiSystemSyncRfiSystemRoot;

  /// No description provided for @paramRfiSystemSyncRfiSystemRootHelp.
  ///
  /// In en, this message translates to:
  /// **'Select the root folder containing CIV, MEC, ELE, and PRC discipline directories.'**
  String get paramRfiSystemSyncRfiSystemRootHelp;

  /// No description provided for @paramRfiSystemSyncRfiLogWorkbook.
  ///
  /// In en, this message translates to:
  /// **'RFI Log Workbook'**
  String get paramRfiSystemSyncRfiLogWorkbook;

  /// No description provided for @paramRfiSystemSyncRfiLogWorkbookHelp.
  ///
  /// In en, this message translates to:
  /// **'Select the RFI Log (.xlsm or .xlsx) workbook to update Remarks to \'Signed by Worley\'.'**
  String get paramRfiSystemSyncRfiLogWorkbookHelp;

  /// No description provided for @paramRfiSystemSyncFollowUpWorkbook.
  ///
  /// In en, this message translates to:
  /// **'Follow Up Workbook'**
  String get paramRfiSystemSyncFollowUpWorkbook;

  /// No description provided for @paramRfiSystemSyncFollowUpWorkbookHelp.
  ///
  /// In en, this message translates to:
  /// **'Select the current Follow Up workbook containing the Signed By Worley CIV and Signed By Worley MEC control tables.'**
  String get paramRfiSystemSyncFollowUpWorkbookHelp;

  /// No description provided for @errorParamRequiredRfiSystemRoot.
  ///
  /// In en, this message translates to:
  /// **'RFI System root folder is required'**
  String get errorParamRequiredRfiSystemRoot;

  /// No description provided for @errorRfiSystemSyncWorkbookInvalid.
  ///
  /// In en, this message translates to:
  /// **'Select valid RFI Log and Follow Up .xlsm or .xlsx workbooks.'**
  String get errorRfiSystemSyncWorkbookInvalid;

  /// No description provided for @errorRfiSystemSyncWorkbookLocked.
  ///
  /// In en, this message translates to:
  /// **'Close the selected workbook in Excel or another application, then try again.'**
  String get errorRfiSystemSyncWorkbookLocked;

  /// No description provided for @errorRfiSystemSyncWorkbookLayoutInvalid.
  ///
  /// In en, this message translates to:
  /// **'The RFI Log must contain one exact RFI No. and REMARKS header in each CIV, MEC, ELE, and PRC sheet.'**
  String get errorRfiSystemSyncWorkbookLayoutInvalid;

  /// No description provided for @errorRfiSystemSyncFollowUpLayoutInvalid.
  ///
  /// In en, this message translates to:
  /// **'The Follow Up workbook must contain one Signed By Worley CIV table and one Signed By Worley MEC control table.'**
  String get errorRfiSystemSyncFollowUpLayoutInvalid;

  /// No description provided for @errorRfiSystemSyncApprovedUpdateInvalid.
  ///
  /// In en, this message translates to:
  /// **'The approved workbook update is invalid; preview again.'**
  String get errorRfiSystemSyncApprovedUpdateInvalid;

  /// No description provided for @errorRfiSystemSyncWorkbookChanged.
  ///
  /// In en, this message translates to:
  /// **'A selected workbook changed after preview; no update was applied. Preview again.'**
  String get errorRfiSystemSyncWorkbookChanged;

  /// No description provided for @warnRfiSystemSyncNoChangesNeeded.
  ///
  /// In en, this message translates to:
  /// **'All detected RFIs are already synchronized or unsigned.'**
  String get warnRfiSystemSyncNoChangesNeeded;

  /// No description provided for @warnRfiSystemSyncNoRowsSelected.
  ///
  /// In en, this message translates to:
  /// **'Select at least one safe signed RFI to synchronize.'**
  String get warnRfiSystemSyncNoRowsSelected;

  /// No description provided for @noteRfiSystemSyncRfiLog.
  ///
  /// In en, this message translates to:
  /// **'Synchronize Worley signatures into RFI Log remarks'**
  String get noteRfiSystemSyncRfiLog;

  /// No description provided for @noteRfiSystemSyncFollowUp.
  ///
  /// In en, this message translates to:
  /// **'Synchronize Final ITP signed RFIs into Follow Up'**
  String get noteRfiSystemSyncFollowUp;

  /// No description provided for @noteRfiSystemSyncBackup.
  ///
  /// In en, this message translates to:
  /// **'Create backup before synchronization'**
  String get noteRfiSystemSyncBackup;

  /// No description provided for @noteRfiSystemSyncRestore.
  ///
  /// In en, this message translates to:
  /// **'Restore workbook from pre-sync backup'**
  String get noteRfiSystemSyncRestore;

  /// No description provided for @rfiSystemSyncEditorTitle.
  ///
  /// In en, this message translates to:
  /// **'Review RFI System & Worley Signature Synchronization'**
  String get rfiSystemSyncEditorTitle;

  /// No description provided for @rfiSystemSyncEditorRowCount.
  ///
  /// In en, this message translates to:
  /// **'{count} scanned RFIs'**
  String rfiSystemSyncEditorRowCount(Object count);

  /// No description provided for @rfiSystemSyncColumnEnabled.
  ///
  /// In en, this message translates to:
  /// **'Apply'**
  String get rfiSystemSyncColumnEnabled;

  /// No description provided for @rfiSystemSyncColumnDiscipline.
  ///
  /// In en, this message translates to:
  /// **'Discipline'**
  String get rfiSystemSyncColumnDiscipline;

  /// No description provided for @rfiSystemSyncColumnRfiNo.
  ///
  /// In en, this message translates to:
  /// **'RFI No.'**
  String get rfiSystemSyncColumnRfiNo;

  /// No description provided for @rfiSystemSyncColumnWorleyStatus.
  ///
  /// In en, this message translates to:
  /// **'Worley Signature'**
  String get rfiSystemSyncColumnWorleyStatus;

  /// No description provided for @rfiSystemSyncColumnRfiLogAction.
  ///
  /// In en, this message translates to:
  /// **'RFI Log Action'**
  String get rfiSystemSyncColumnRfiLogAction;

  /// No description provided for @rfiSystemSyncColumnFollowUpAction.
  ///
  /// In en, this message translates to:
  /// **'Follow Up Action'**
  String get rfiSystemSyncColumnFollowUpAction;

  /// No description provided for @rfiSystemSyncColumnDisposition.
  ///
  /// In en, this message translates to:
  /// **'Status'**
  String get rfiSystemSyncColumnDisposition;

  /// No description provided for @rfiSystemSyncColumnReason.
  ///
  /// In en, this message translates to:
  /// **'Evidence / Details'**
  String get rfiSystemSyncColumnReason;

  /// No description provided for @rfiSystemSyncDispositionReady.
  ///
  /// In en, this message translates to:
  /// **'Ready to sync'**
  String get rfiSystemSyncDispositionReady;

  /// No description provided for @rfiSystemSyncDispositionAlreadySynced.
  ///
  /// In en, this message translates to:
  /// **'Already synchronized'**
  String get rfiSystemSyncDispositionAlreadySynced;

  /// No description provided for @rfiSystemSyncDispositionUnsigned.
  ///
  /// In en, this message translates to:
  /// **'Unsigned'**
  String get rfiSystemSyncDispositionUnsigned;

  /// No description provided for @rfiSystemSyncDispositionManualReview.
  ///
  /// In en, this message translates to:
  /// **'Manual review'**
  String get rfiSystemSyncDispositionManualReview;

  /// No description provided for @opRfiCompleteStatusSyncTitle.
  ///
  /// In en, this message translates to:
  /// **'Mark completed RFIs from the RFI System'**
  String get opRfiCompleteStatusSyncTitle;

  /// No description provided for @opRfiCompleteStatusSyncDesc.
  ///
  /// In en, this message translates to:
  /// **'Scans every RFI System subfolder for exact RFI No. PDFs and prepares safe RFI Log Inspection Status updates to Completed.'**
  String get opRfiCompleteStatusSyncDesc;

  /// No description provided for @paramRfiCompleteStatusRfiSystemRoot.
  ///
  /// In en, this message translates to:
  /// **'RFI System Root Folder'**
  String get paramRfiCompleteStatusRfiSystemRoot;

  /// No description provided for @paramRfiCompleteStatusRfiSystemRootHelp.
  ///
  /// In en, this message translates to:
  /// **'Select the RFI System root. Every nested folder is inspected read-only; symlinks are not followed.'**
  String get paramRfiCompleteStatusRfiSystemRootHelp;

  /// No description provided for @paramRfiCompleteStatusRfiLogWorkbook.
  ///
  /// In en, this message translates to:
  /// **'RFI Log Workbook'**
  String get paramRfiCompleteStatusRfiLogWorkbook;

  /// No description provided for @paramRfiCompleteStatusRfiLogWorkbookHelp.
  ///
  /// In en, this message translates to:
  /// **'Select the .xlsm or .xlsx RFI Log whose safe Inspection Status cells may be changed to Completed after review.'**
  String get paramRfiCompleteStatusRfiLogWorkbookHelp;

  /// No description provided for @errorRfiCompleteStatusSystemRootInvalid.
  ///
  /// In en, this message translates to:
  /// **'Select a readable RFI System root folder.'**
  String get errorRfiCompleteStatusSystemRootInvalid;

  /// No description provided for @errorRfiCompleteStatusWorkbookInvalid.
  ///
  /// In en, this message translates to:
  /// **'Select a valid RFI Log .xlsm or .xlsx workbook.'**
  String get errorRfiCompleteStatusWorkbookInvalid;

  /// No description provided for @errorRfiCompleteStatusWorkbookLocked.
  ///
  /// In en, this message translates to:
  /// **'Close the selected RFI Log in Excel or another application, then try again.'**
  String get errorRfiCompleteStatusWorkbookLocked;

  /// No description provided for @errorRfiCompleteStatusWorkbookLayoutInvalid.
  ///
  /// In en, this message translates to:
  /// **'Each CIV, MEC, ELE, and PRC sheet must contain one exact RFI No. and Inspection Status header.'**
  String get errorRfiCompleteStatusWorkbookLayoutInvalid;

  /// No description provided for @errorRfiCompleteStatusApprovedUpdateInvalid.
  ///
  /// In en, this message translates to:
  /// **'The approved completion-status update is invalid; preview again.'**
  String get errorRfiCompleteStatusApprovedUpdateInvalid;

  /// No description provided for @warnRfiCompleteStatusInvalidPdfName.
  ///
  /// In en, this message translates to:
  /// **'An RFI No. PDF has a non-canonical filename and was not used.'**
  String get warnRfiCompleteStatusInvalidPdfName;

  /// No description provided for @warnRfiCompleteStatusInvalidPdfNameOverflow.
  ///
  /// In en, this message translates to:
  /// **'More non-canonical RFI No. PDF names were omitted from this warning list.'**
  String get warnRfiCompleteStatusInvalidPdfNameOverflow;

  /// No description provided for @warnRfiCompleteStatusNoChangesNeeded.
  ///
  /// In en, this message translates to:
  /// **'No safe RFI Log completion changes are ready.'**
  String get warnRfiCompleteStatusNoChangesNeeded;

  /// No description provided for @warnRfiCompleteStatusNoRowsSelected.
  ///
  /// In en, this message translates to:
  /// **'Select at least one safe completed RFI to synchronize.'**
  String get warnRfiCompleteStatusNoRowsSelected;

  /// No description provided for @noteRfiCompleteStatusRfiLog.
  ///
  /// In en, this message translates to:
  /// **'Set approved RFI Log Inspection Status cells to Completed'**
  String get noteRfiCompleteStatusRfiLog;

  /// No description provided for @rfiCompleteStatusEditorTitle.
  ///
  /// In en, this message translates to:
  /// **'Review completed RFI evidence and status updates'**
  String get rfiCompleteStatusEditorTitle;

  /// No description provided for @rfiCompleteStatusEditorRowCount.
  ///
  /// In en, this message translates to:
  /// **'{count} RFIs with exact completed PDFs'**
  String rfiCompleteStatusEditorRowCount(Object count);

  /// No description provided for @rfiCompleteStatusColumnEnabled.
  ///
  /// In en, this message translates to:
  /// **'Apply'**
  String get rfiCompleteStatusColumnEnabled;

  /// No description provided for @rfiCompleteStatusColumnDiscipline.
  ///
  /// In en, this message translates to:
  /// **'Discipline'**
  String get rfiCompleteStatusColumnDiscipline;

  /// No description provided for @rfiCompleteStatusColumnRfiNo.
  ///
  /// In en, this message translates to:
  /// **'RFI No.'**
  String get rfiCompleteStatusColumnRfiNo;

  /// No description provided for @rfiCompleteStatusColumnPdfPath.
  ///
  /// In en, this message translates to:
  /// **'Completed PDF'**
  String get rfiCompleteStatusColumnPdfPath;

  /// No description provided for @rfiCompleteStatusColumnPdfCount.
  ///
  /// In en, this message translates to:
  /// **'PDF count'**
  String get rfiCompleteStatusColumnPdfCount;

  /// No description provided for @rfiCompleteStatusColumnCurrentStatus.
  ///
  /// In en, this message translates to:
  /// **'Current status'**
  String get rfiCompleteStatusColumnCurrentStatus;

  /// No description provided for @rfiCompleteStatusColumnProposedStatus.
  ///
  /// In en, this message translates to:
  /// **'New status'**
  String get rfiCompleteStatusColumnProposedStatus;

  /// No description provided for @rfiCompleteStatusColumnDisposition.
  ///
  /// In en, this message translates to:
  /// **'Decision'**
  String get rfiCompleteStatusColumnDisposition;

  /// No description provided for @rfiCompleteStatusColumnAction.
  ///
  /// In en, this message translates to:
  /// **'Action'**
  String get rfiCompleteStatusColumnAction;

  /// No description provided for @rfiCompleteStatusColumnPdfPaths.
  ///
  /// In en, this message translates to:
  /// **'All matching PDFs'**
  String get rfiCompleteStatusColumnPdfPaths;

  /// No description provided for @rfiCompleteStatusColumnReason.
  ///
  /// In en, this message translates to:
  /// **'Evidence / Details'**
  String get rfiCompleteStatusColumnReason;

  /// No description provided for @rfiCompleteStatusDispositionReady.
  ///
  /// In en, this message translates to:
  /// **'Ready'**
  String get rfiCompleteStatusDispositionReady;

  /// No description provided for @rfiCompleteStatusDispositionAlreadyCompleted.
  ///
  /// In en, this message translates to:
  /// **'Already Completed'**
  String get rfiCompleteStatusDispositionAlreadyCompleted;

  /// No description provided for @rfiCompleteStatusDispositionManualReview.
  ///
  /// In en, this message translates to:
  /// **'Manual review'**
  String get rfiCompleteStatusDispositionManualReview;

  /// No description provided for @progressPhaseCompletionPdfDiscovery.
  ///
  /// In en, this message translates to:
  /// **'Scanning all RFI System folders for completed PDFs'**
  String get progressPhaseCompletionPdfDiscovery;

  /// No description provided for @progressPhaseCompletionWorkbookRead.
  ///
  /// In en, this message translates to:
  /// **'Reading RFI Log status cells'**
  String get progressPhaseCompletionWorkbookRead;

  /// No description provided for @progressPhaseCompletionMapping.
  ///
  /// In en, this message translates to:
  /// **'Matching completed PDFs to RFI Log rows'**
  String get progressPhaseCompletionMapping;

  /// No description provided for @opFilesReplicaPublishTitle.
  ///
  /// In en, this message translates to:
  /// **'Publish RFI Systems'**
  String get opFilesReplicaPublishTitle;

  /// No description provided for @opFilesReplicaPublishDesc.
  ///
  /// In en, this message translates to:
  /// **'Publishes the selected source RFI System to two targets with retained backups, hash verification, and Undo.'**
  String get opFilesReplicaPublishDesc;

  /// No description provided for @paramReplicaRootA.
  ///
  /// In en, this message translates to:
  /// **'System A folder'**
  String get paramReplicaRootA;

  /// No description provided for @paramReplicaRootAHelp.
  ///
  /// In en, this message translates to:
  /// **'One SharePoint, network, or local RFI System root.'**
  String get paramReplicaRootAHelp;

  /// No description provided for @paramReplicaRootB.
  ///
  /// In en, this message translates to:
  /// **'System B folder'**
  String get paramReplicaRootB;

  /// No description provided for @paramReplicaRootBHelp.
  ///
  /// In en, this message translates to:
  /// **'Second RFI System root.'**
  String get paramReplicaRootBHelp;

  /// No description provided for @paramReplicaRootC.
  ///
  /// In en, this message translates to:
  /// **'System C folder'**
  String get paramReplicaRootC;

  /// No description provided for @paramReplicaRootCHelp.
  ///
  /// In en, this message translates to:
  /// **'Third RFI System root.'**
  String get paramReplicaRootCHelp;

  /// No description provided for @paramReplicaSourceRoot.
  ///
  /// In en, this message translates to:
  /// **'Authoritative source'**
  String get paramReplicaSourceRoot;

  /// No description provided for @paramReplicaSourceRootHelp.
  ///
  /// In en, this message translates to:
  /// **'This system publishes to the other two. Target changes never flow back to the source.'**
  String get paramReplicaSourceRootHelp;

  /// No description provided for @choiceReplicaSourceA.
  ///
  /// In en, this message translates to:
  /// **'System A'**
  String get choiceReplicaSourceA;

  /// No description provided for @choiceReplicaSourceB.
  ///
  /// In en, this message translates to:
  /// **'System B'**
  String get choiceReplicaSourceB;

  /// No description provided for @choiceReplicaSourceC.
  ///
  /// In en, this message translates to:
  /// **'System C'**
  String get choiceReplicaSourceC;

  /// No description provided for @paramReplicaBackupRoot.
  ///
  /// In en, this message translates to:
  /// **'Version backup folder'**
  String get paramReplicaBackupRoot;

  /// No description provided for @paramReplicaBackupRootHelp.
  ///
  /// In en, this message translates to:
  /// **'Every target file being replaced is first copied to a separate retained backup.'**
  String get paramReplicaBackupRootHelp;

  /// No description provided for @paramReplicaRootFiles.
  ///
  /// In en, this message translates to:
  /// **'Additional root files'**
  String get paramReplicaRootFiles;

  /// No description provided for @paramReplicaRootFilesHelp.
  ///
  /// In en, this message translates to:
  /// **'Separate root file names outside CIV, MEC, ELE, and PRC with semicolons or new lines.'**
  String get paramReplicaRootFilesHelp;

  /// No description provided for @paramReplicaUnattended.
  ///
  /// In en, this message translates to:
  /// **'Enable unattended automatic mode'**
  String get paramReplicaUnattended;

  /// No description provided for @paramReplicaUnattendedHelp.
  ///
  /// In en, this message translates to:
  /// **'Applying this preview once grants standing authority only to this fixed profile. Ambiguity pauses automation.'**
  String get paramReplicaUnattendedHelp;

  /// No description provided for @paramReplicaIntervalMinutes.
  ///
  /// In en, this message translates to:
  /// **'Check interval (minutes)'**
  String get paramReplicaIntervalMinutes;

  /// No description provided for @paramReplicaIntervalMinutesHelp.
  ///
  /// In en, this message translates to:
  /// **'Between 1 and 1440 minutes. Runs while DocFlow is open.'**
  String get paramReplicaIntervalMinutesHelp;

  /// No description provided for @paramReplicaSettleSeconds.
  ///
  /// In en, this message translates to:
  /// **'File settle time (seconds)'**
  String get paramReplicaSettleSeconds;

  /// No description provided for @paramReplicaSettleSecondsHelp.
  ///
  /// In en, this message translates to:
  /// **'Automatic publication waits this long after the newest source change. Between 5 and 3600.'**
  String get paramReplicaSettleSecondsHelp;

  /// No description provided for @paramReplicaMaxChangedFiles.
  ///
  /// In en, this message translates to:
  /// **'Maximum file effects per automatic run'**
  String get paramReplicaMaxChangedFiles;

  /// No description provided for @paramReplicaMaxChangedFilesHelp.
  ///
  /// In en, this message translates to:
  /// **'Automation pauses if one run would change more target files.'**
  String get paramReplicaMaxChangedFilesHelp;

  /// No description provided for @paramReplicaMaxChangedMib.
  ///
  /// In en, this message translates to:
  /// **'Maximum MiB per automatic run'**
  String get paramReplicaMaxChangedMib;

  /// No description provided for @paramReplicaMaxChangedMibHelp.
  ///
  /// In en, this message translates to:
  /// **'Automation pauses if one run would publish more total data.'**
  String get paramReplicaMaxChangedMibHelp;

  /// No description provided for @replicaEditorTitle.
  ///
  /// In en, this message translates to:
  /// **'Review exact publication effects'**
  String get replicaEditorTitle;

  /// No description provided for @replicaEditorRowCount.
  ///
  /// In en, this message translates to:
  /// **'{count} file effects'**
  String replicaEditorRowCount(Object count);

  /// No description provided for @replicaColumnTarget.
  ///
  /// In en, this message translates to:
  /// **'Target'**
  String get replicaColumnTarget;

  /// No description provided for @replicaColumnPath.
  ///
  /// In en, this message translates to:
  /// **'Relative path'**
  String get replicaColumnPath;

  /// No description provided for @replicaColumnAction.
  ///
  /// In en, this message translates to:
  /// **'Action'**
  String get replicaColumnAction;

  /// No description provided for @replicaColumnBeforeHash.
  ///
  /// In en, this message translates to:
  /// **'Previous hash'**
  String get replicaColumnBeforeHash;

  /// No description provided for @replicaColumnAfterHash.
  ///
  /// In en, this message translates to:
  /// **'Source hash'**
  String get replicaColumnAfterHash;

  /// No description provided for @replicaActionCopy.
  ///
  /// In en, this message translates to:
  /// **'Copy new file'**
  String get replicaActionCopy;

  /// No description provided for @replicaActionReplace.
  ///
  /// In en, this message translates to:
  /// **'Back up and replace'**
  String get replicaActionReplace;

  /// No description provided for @noteReplicaMkdir.
  ///
  /// In en, this message translates to:
  /// **'Create required folder'**
  String get noteReplicaMkdir;

  /// No description provided for @noteReplicaCopy.
  ///
  /// In en, this message translates to:
  /// **'Copy missing target file'**
  String get noteReplicaCopy;

  /// No description provided for @noteReplicaReplace.
  ///
  /// In en, this message translates to:
  /// **'Back up, verify, and replace target from source'**
  String get noteReplicaReplace;

  /// No description provided for @noteReplicaRestore.
  ///
  /// In en, this message translates to:
  /// **'Restore target from its pre-publication backup'**
  String get noteReplicaRestore;

  /// No description provided for @warnReplicaTargetExtra.
  ///
  /// In en, this message translates to:
  /// **'Target contains a file absent from source; it was not deleted.'**
  String get warnReplicaTargetExtra;

  /// No description provided for @warnReplicaTargetExtraOverflow.
  ///
  /// In en, this message translates to:
  /// **'More target-only files could not be listed; none were deleted.'**
  String get warnReplicaTargetExtraOverflow;

  /// No description provided for @warnReplicaNoChanges.
  ///
  /// In en, this message translates to:
  /// **'All three systems already match the selected source.'**
  String get warnReplicaNoChanges;

  /// No description provided for @errorReplicaRootInvalid.
  ///
  /// In en, this message translates to:
  /// **'Select three readable RFI System roots.'**
  String get errorReplicaRootInvalid;

  /// No description provided for @errorReplicaBackupInvalid.
  ///
  /// In en, this message translates to:
  /// **'Select a readable, separate backup folder.'**
  String get errorReplicaBackupInvalid;

  /// No description provided for @errorReplicaSourceInvalid.
  ///
  /// In en, this message translates to:
  /// **'Authoritative source selection is invalid.'**
  String get errorReplicaSourceInvalid;

  /// No description provided for @errorReplicaRootsOverlap.
  ///
  /// In en, this message translates to:
  /// **'RFI System roots cannot be equal or nested.'**
  String get errorReplicaRootsOverlap;

  /// No description provided for @errorReplicaBackupOverlap.
  ///
  /// In en, this message translates to:
  /// **'Backup folder cannot equal or contain any RFI System root.'**
  String get errorReplicaBackupOverlap;

  /// No description provided for @errorReplicaRootFileInvalid.
  ///
  /// In en, this message translates to:
  /// **'An additional root file name is invalid.'**
  String get errorReplicaRootFileInvalid;

  /// No description provided for @errorReplicaSourceRootFileMissing.
  ///
  /// In en, this message translates to:
  /// **'A selected additional root file is missing from the source system.'**
  String get errorReplicaSourceRootFileMissing;

  /// No description provided for @errorReplicaTooManyRootFiles.
  ///
  /// In en, this message translates to:
  /// **'At most 64 additional root files are allowed.'**
  String get errorReplicaTooManyRootFiles;

  /// No description provided for @errorReplicaIntervalInvalid.
  ///
  /// In en, this message translates to:
  /// **'Check interval must be between 1 and 1440 minutes.'**
  String get errorReplicaIntervalInvalid;

  /// No description provided for @errorReplicaSettleInvalid.
  ///
  /// In en, this message translates to:
  /// **'File settle time must be between 5 and 3600 seconds.'**
  String get errorReplicaSettleInvalid;

  /// No description provided for @errorReplicaChangeLimitInvalid.
  ///
  /// In en, this message translates to:
  /// **'Automatic change limits must be positive and within the supported range.'**
  String get errorReplicaChangeLimitInvalid;

  /// No description provided for @errorReplicaSourceSettling.
  ///
  /// In en, this message translates to:
  /// **'A source file changed recently; automatic publication was deferred until it settles.'**
  String get errorReplicaSourceSettling;

  /// No description provided for @errorReplicaChangeLimitExceeded.
  ///
  /// In en, this message translates to:
  /// **'Automatic publication exceeds the approved file or size limit and was paused for safety.'**
  String get errorReplicaChangeLimitExceeded;

  /// No description provided for @errorReplicaDisciplineMissing.
  ///
  /// In en, this message translates to:
  /// **'Source must contain all CIV, MEC, ELE, and PRC folders.'**
  String get errorReplicaDisciplineMissing;

  /// No description provided for @errorReplicaScanIncomplete.
  ///
  /// In en, this message translates to:
  /// **'Folder scan could not finish; no files were published.'**
  String get errorReplicaScanIncomplete;

  /// No description provided for @errorReplicaEntryLimit.
  ///
  /// In en, this message translates to:
  /// **'RFI System exceeds the 500000-entry safety limit.'**
  String get errorReplicaEntryLimit;

  /// No description provided for @errorReplicaSymlinkRefused.
  ///
  /// In en, this message translates to:
  /// **'A symbolic link was found; safe publication stopped.'**
  String get errorReplicaSymlinkRefused;

  /// No description provided for @errorReplicaPathCollision.
  ///
  /// In en, this message translates to:
  /// **'Multiple files resolve to the same relative path.'**
  String get errorReplicaPathCollision;

  /// No description provided for @errorReplicaFileUnstable.
  ///
  /// In en, this message translates to:
  /// **'A file changed during scanning; preview again.'**
  String get errorReplicaFileUnstable;

  /// No description provided for @errorReplicaApprovedStepInvalid.
  ///
  /// In en, this message translates to:
  /// **'Approved publication step is invalid; preview again.'**
  String get errorReplicaApprovedStepInvalid;

  /// No description provided for @errorReplicaSourceChanged.
  ///
  /// In en, this message translates to:
  /// **'Source changed after preview; publication stopped.'**
  String get errorReplicaSourceChanged;

  /// No description provided for @errorReplicaTargetChanged.
  ///
  /// In en, this message translates to:
  /// **'Target changed after preview; publication stopped.'**
  String get errorReplicaTargetChanged;

  /// No description provided for @errorReplicaBackupExists.
  ///
  /// In en, this message translates to:
  /// **'This publication backup already exists and was not overwritten.'**
  String get errorReplicaBackupExists;

  /// No description provided for @errorReplicaBackupVerificationFailed.
  ///
  /// In en, this message translates to:
  /// **'Backup hash verification failed; automation paused.'**
  String get errorReplicaBackupVerificationFailed;

  /// No description provided for @errorReplicaCopyVerificationFailed.
  ///
  /// In en, this message translates to:
  /// **'Temporary copy hash verification failed.'**
  String get errorReplicaCopyVerificationFailed;

  /// No description provided for @errorReplicaPublishVerificationFailed.
  ///
  /// In en, this message translates to:
  /// **'Post-publication hash verification failed; recovery is required.'**
  String get errorReplicaPublishVerificationFailed;

  /// No description provided for @errorReplicaProfileMissing.
  ///
  /// In en, this message translates to:
  /// **'No approved automatic sync profile exists.'**
  String get errorReplicaProfileMissing;

  /// No description provided for @errorReplicaProfileInvalid.
  ///
  /// In en, this message translates to:
  /// **'Saved automatic sync profile is invalid.'**
  String get errorReplicaProfileInvalid;

  /// No description provided for @errorReplicaActivationFailed.
  ///
  /// In en, this message translates to:
  /// **'Files were published, but automatic profile activation failed.'**
  String get errorReplicaActivationFailed;

  /// No description provided for @errorReplicaAutomaticApplyFailed.
  ///
  /// In en, this message translates to:
  /// **'Automatic publication failed and was paused.'**
  String get errorReplicaAutomaticApplyFailed;

  /// No description provided for @progressPhaseReplicaDiscovery.
  ///
  /// In en, this message translates to:
  /// **'Scanning all three RFI Systems'**
  String get progressPhaseReplicaDiscovery;

  /// No description provided for @progressPhaseReplicaSourceHash.
  ///
  /// In en, this message translates to:
  /// **'Hashing source files across bounded CPU workers'**
  String get progressPhaseReplicaSourceHash;

  /// No description provided for @progressPhaseReplicaTargetHash.
  ///
  /// In en, this message translates to:
  /// **'Hashing target files across bounded CPU workers'**
  String get progressPhaseReplicaTargetHash;

  /// No description provided for @prefsAutomation.
  ///
  /// In en, this message translates to:
  /// **'Windows and automation'**
  String get prefsAutomation;

  /// No description provided for @prefsAutomationHelp.
  ///
  /// In en, this message translates to:
  /// **'Control startup and the previously approved background sync profile.'**
  String get prefsAutomationHelp;

  /// No description provided for @prefsStartWithWindows.
  ///
  /// In en, this message translates to:
  /// **'Start DocFlow with Windows'**
  String get prefsStartWithWindows;

  /// No description provided for @prefsReplicaSyncEnabled.
  ///
  /// In en, this message translates to:
  /// **'Unattended RFI System sync active'**
  String get prefsReplicaSyncEnabled;

  /// No description provided for @prefsReplicaSyncEnabledHelp.
  ///
  /// In en, this message translates to:
  /// **'Any error or ambiguity turns this off automatically. Backups are never auto-deleted.'**
  String get prefsReplicaSyncEnabledHelp;

  /// No description provided for @prefsReplicaStatusUnconfigured.
  ///
  /// In en, this message translates to:
  /// **'Status: No approved profile yet'**
  String get prefsReplicaStatusUnconfigured;

  /// No description provided for @prefsReplicaStatusDisabled.
  ///
  /// In en, this message translates to:
  /// **'Status: Off'**
  String get prefsReplicaStatusDisabled;

  /// No description provided for @prefsReplicaStatusIdle.
  ///
  /// In en, this message translates to:
  /// **'Status: Waiting for the next run'**
  String get prefsReplicaStatusIdle;

  /// No description provided for @prefsReplicaStatusRunning.
  ///
  /// In en, this message translates to:
  /// **'Status: Running'**
  String get prefsReplicaStatusRunning;

  /// No description provided for @prefsReplicaStatusWaitingForSignIn.
  ///
  /// In en, this message translates to:
  /// **'Status: Waiting for sign-in'**
  String get prefsReplicaStatusWaitingForSignIn;

  /// No description provided for @prefsReplicaStatusDeferred.
  ///
  /// In en, this message translates to:
  /// **'Status: Deferred until another operation finishes'**
  String get prefsReplicaStatusDeferred;

  /// No description provided for @prefsReplicaStatusPaused.
  ///
  /// In en, this message translates to:
  /// **'Status: Paused for safety'**
  String get prefsReplicaStatusPaused;

  /// No description provided for @paramRfiProcessScansCorrectOrientation.
  ///
  /// In en, this message translates to:
  /// **'Correct page rotation'**
  String get paramRfiProcessScansCorrectOrientation;

  /// No description provided for @paramRfiProcessScansCorrectOrientationHelp.
  ///
  /// In en, this message translates to:
  /// **'When enabled, rotates pages only when orientation is confidently determined. If any page is uncertain, correction is skipped for the entire PDF and it is sent intact to _review.'**
  String get paramRfiProcessScansCorrectOrientationHelp;

  /// No description provided for @warnRfiProcessScansOrientationReview.
  ///
  /// In en, this message translates to:
  /// **'Page orientation was uncertain; PDF was sent to _review'**
  String get warnRfiProcessScansOrientationReview;

  /// No description provided for @progressPhaseScanOrientation.
  ///
  /// In en, this message translates to:
  /// **'Checking PDF page orientation'**
  String get progressPhaseScanOrientation;

  /// No description provided for @rfiExtractionPages.
  ///
  /// In en, this message translates to:
  /// **'Extracted pages'**
  String get rfiExtractionPages;

  /// No description provided for @reviewSelectAllKeep.
  ///
  /// In en, this message translates to:
  /// **'Keep All'**
  String get reviewSelectAllKeep;

  /// No description provided for @reviewSelectAllPut.
  ///
  /// In en, this message translates to:
  /// **'Move All Possible'**
  String get reviewSelectAllPut;

  /// No description provided for @reviewSelectAllTrash.
  ///
  /// In en, this message translates to:
  /// **'Move All to Trash'**
  String get reviewSelectAllTrash;

  /// No description provided for @opsSearchPlaceholder.
  ///
  /// In en, this message translates to:
  /// **'Search operations…'**
  String get opsSearchPlaceholder;

  /// No description provided for @opsCategoryAll.
  ///
  /// In en, this message translates to:
  /// **'All'**
  String get opsCategoryAll;

  /// No description provided for @opsCategoryScans.
  ///
  /// In en, this message translates to:
  /// **'Scans & PDFs'**
  String get opsCategoryScans;

  /// No description provided for @opsCategoryExcel.
  ///
  /// In en, this message translates to:
  /// **'Excel & Logs'**
  String get opsCategoryExcel;

  /// No description provided for @opsCategoryMail.
  ///
  /// In en, this message translates to:
  /// **'Outlook & Mail'**
  String get opsCategoryMail;

  /// No description provided for @opsCategorySystem.
  ///
  /// In en, this message translates to:
  /// **'Publish & Print'**
  String get opsCategorySystem;

  /// No description provided for @opsEmptySearch.
  ///
  /// In en, this message translates to:
  /// **'No operations matching \'{query}\''**
  String opsEmptySearch(Object query);

  /// No description provided for @actionOpenDestination.
  ///
  /// In en, this message translates to:
  /// **'Open Destination Folder'**
  String get actionOpenDestination;

  /// No description provided for @opRfiGenerateConcreteDocumentsTitle.
  ///
  /// In en, this message translates to:
  /// **'Generate Concrete Documents'**
  String get opRfiGenerateConcreteDocumentsTitle;

  /// No description provided for @opRfiGenerateConcreteDocumentsDesc.
  ///
  /// In en, this message translates to:
  /// **'Batch-generate concrete documents from RFI Log and Follow Up workbooks using selected RFI types and templates.'**
  String get opRfiGenerateConcreteDocumentsDesc;

  /// No description provided for @paramRfiDocumentsRfiLog.
  ///
  /// In en, this message translates to:
  /// **'RFI Log workbook'**
  String get paramRfiDocumentsRfiLog;

  /// No description provided for @paramRfiDocumentsFollowUp.
  ///
  /// In en, this message translates to:
  /// **'Follow Up workbook'**
  String get paramRfiDocumentsFollowUp;

  /// No description provided for @paramRfiDocumentsTemplate010.
  ///
  /// In en, this message translates to:
  /// **'Template 010 — Follow Up fields; ambient = plant fresh temperature minus random 0.5–0.7'**
  String get paramRfiDocumentsTemplate010;

  /// No description provided for @paramRfiDocumentsTemplate011.
  ///
  /// In en, this message translates to:
  /// **'Template 011 — report/location and adjustable random ambient range (default 19–24)'**
  String get paramRfiDocumentsTemplate011;

  /// No description provided for @paramRfiDocumentsTemplate013.
  ///
  /// In en, this message translates to:
  /// **'Template 013 — report/location only'**
  String get paramRfiDocumentsTemplate013;

  /// No description provided for @paramRfiDocumentsTemplate014.
  ///
  /// In en, this message translates to:
  /// **'Template 014 — report/location only'**
  String get paramRfiDocumentsTemplate014;

  /// No description provided for @paramRfiDocumentsOutput.
  ///
  /// In en, this message translates to:
  /// **'Output folder'**
  String get paramRfiDocumentsOutput;

  /// No description provided for @paramRfiDocumentsRfis010.
  ///
  /// In en, this message translates to:
  /// **'RFI numbers for Template 010'**
  String get paramRfiDocumentsRfis010;

  /// No description provided for @paramRfiDocumentsRfis011.
  ///
  /// In en, this message translates to:
  /// **'RFI numbers for Template 011'**
  String get paramRfiDocumentsRfis011;

  /// No description provided for @paramRfiDocumentsRfis013.
  ///
  /// In en, this message translates to:
  /// **'RFI numbers for Template 013'**
  String get paramRfiDocumentsRfis013;

  /// No description provided for @paramRfiDocumentsRfis014.
  ///
  /// In en, this message translates to:
  /// **'RFI numbers for Template 014'**
  String get paramRfiDocumentsRfis014;

  /// No description provided for @paramRfiDocumentsWaterRatios.
  ///
  /// In en, this message translates to:
  /// **'Optional per-RFI water/cement ratios (blank when absent)'**
  String get paramRfiDocumentsWaterRatios;

  /// No description provided for @paramRfiDocumentsAmbientMin.
  ///
  /// In en, this message translates to:
  /// **'Minimum ambient temperature'**
  String get paramRfiDocumentsAmbientMin;

  /// No description provided for @paramRfiDocumentsAmbientMax.
  ///
  /// In en, this message translates to:
  /// **'Maximum ambient temperature'**
  String get paramRfiDocumentsAmbientMax;

  /// No description provided for @paramRfiDocumentsAmbientGroups.
  ///
  /// In en, this message translates to:
  /// **'RFI-specific ambient temperature ranges'**
  String get paramRfiDocumentsAmbientGroups;

  /// No description provided for @errorParamRequiredOutputDirectory.
  ///
  /// In en, this message translates to:
  /// **'Output folder is required'**
  String get errorParamRequiredOutputDirectory;

  /// No description provided for @errorRfiDocumentsInvalidInput.
  ///
  /// In en, this message translates to:
  /// **'The concrete document inputs are invalid'**
  String get errorRfiDocumentsInvalidInput;

  /// No description provided for @warnRfiDocumentsOutputExists.
  ///
  /// In en, this message translates to:
  /// **'An output file already exists and will not be overwritten'**
  String get warnRfiDocumentsOutputExists;

  /// No description provided for @warnRfiDocumentsFollowUpMissing.
  ///
  /// In en, this message translates to:
  /// **'No Concrete Specimen Test Follow-Up row was found for this RFI. It will be skipped. Apply creates available documents only; Cancel to correct the workbook and preview again.'**
  String get warnRfiDocumentsFollowUpMissing;

  /// No description provided for @warnRfiDocumentsFormNotRequired.
  ///
  /// In en, this message translates to:
  /// **'The RFI Log Forms field does not list CON-CIV-010 for this RFI, so no CON-CIV-010 will be created. Check the requested type or correct the RFI Log, then preview again.'**
  String get warnRfiDocumentsFormNotRequired;

  /// No description provided for @paramRfiDocumentsRfiLogHelp.
  ///
  /// In en, this message translates to:
  /// **'The Civil sheet supplies each requested Report RFI Number and location.'**
  String get paramRfiDocumentsRfiLogHelp;

  /// No description provided for @paramRfiDocumentsFollowUpHelp.
  ///
  /// In en, this message translates to:
  /// **'Required only when generating CON-CIV-010; supplies its concrete-pour fields.'**
  String get paramRfiDocumentsFollowUpHelp;

  /// No description provided for @paramRfiDocumentsOutputHelp.
  ///
  /// In en, this message translates to:
  /// **'Generated files are created here; existing files are never overwritten.'**
  String get paramRfiDocumentsOutputHelp;

  /// No description provided for @paramRfiDocumentsRfisHelp.
  ///
  /// In en, this message translates to:
  /// **'Enter full RFI numbers or CIV serials, separated by commas or new lines.'**
  String get paramRfiDocumentsRfisHelp;

  /// No description provided for @paramRfiDocumentsWaterRatiosHelp.
  ///
  /// In en, this message translates to:
  /// **'Optional one per line as RFI number = ratio. Leave out an RFI to keep its cell blank.'**
  String get paramRfiDocumentsWaterRatiosHelp;

  /// No description provided for @paramRfiDocumentsAmbientGroupsHelp.
  ///
  /// In en, this message translates to:
  /// **'Optional, one line per group: RFI[, RFI...]=MIN-MAX. Both range endpoints are included; unlisted RFIs use the default Minimum/Maximum ambient temperature.'**
  String get paramRfiDocumentsAmbientGroupsHelp;

  /// No description provided for @stepRfiGeneratedConcreteDocument.
  ///
  /// In en, this message translates to:
  /// **'Generate concrete document'**
  String get stepRfiGeneratedConcreteDocument;
}

class _AppLocalizationsDelegate
    extends LocalizationsDelegate<AppLocalizations> {
  const _AppLocalizationsDelegate();

  @override
  Future<AppLocalizations> load(Locale locale) {
    return SynchronousFuture<AppLocalizations>(lookupAppLocalizations(locale));
  }

  @override
  bool isSupported(Locale locale) =>
      <String>['az', 'en', 'ru', 'tr'].contains(locale.languageCode);

  @override
  bool shouldReload(_AppLocalizationsDelegate old) => false;
}

AppLocalizations lookupAppLocalizations(Locale locale) {
  // Lookup logic when only language code is specified.
  switch (locale.languageCode) {
    case 'az':
      return AppLocalizationsAz();
    case 'en':
      return AppLocalizationsEn();
    case 'ru':
      return AppLocalizationsRu();
    case 'tr':
      return AppLocalizationsTr();
  }

  throw FlutterError(
    'AppLocalizations.delegate failed to load unsupported locale "$locale". This is likely '
    'an issue with the localizations generation tool. Please file an issue '
    'on GitHub with a reproducible sample app and the gen-l10n configuration '
    'that was used.',
  );
}
